//! MOS1 completion (#88): Meyer gate charge, series resistance, junction
//! geometry, process extraction and temperature, checked against independent
//! restatements of `mos1temp.c`/`mos1load.c`/`DEVqmeyer` and finite
//! differences. C-golden comparisons live in `xtask golden verify`.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::{
    AnalysisMode, Circuit, Forcing, Limit, LoadRequest, ModelContext, StateHistory, TransientTiming,
};
use ngspice_rs::maths::integrator::{Coefficients, DEFAULT_XMU, IntegrationMethod, StepHistory};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError};
use std::path::Path;

const BOLTZMANN: f64 = 1.38064852e-23;
const CHARGE: f64 = 1.6021766208e-19;
const K_OVER_Q: f64 = BOLTZMANN / CHARGE;
const REFTEMP: f64 = 300.15;
const EPS0: f64 = 8.854214871e-12;
const GMIN: f64 = 1e-12;

fn deck(body: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("m7.cir"),
            &format!("M7\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&deck(body)).unwrap()
}
fn op(c: &mut Circuit) -> Plot {
    runner(AnalysisKind::OperatingPoint)
        .unwrap()
        .run(
            c,
            &AnalysisRequest::with_arguments(AnalysisKind::OperatingPoint, [""; 0]),
            &AnalysisContext::default(),
        )
        .unwrap()
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}
fn bias(
    c: &Circuit,
) -> ngspice_rs::analysis::newton::NewtonSolution<ngspice_rs::devices::TrialState> {
    ngspice_rs::analysis::bias::solve_dc(
        c,
        &ModelContext::default(),
        &Default::default(),
        &[],
        None,
        None,
    )
    .unwrap()
}
fn row(c: &Circuit, node: &str) -> usize {
    c.unknowns().node_row(c.nodes().get(node).unwrap()).unwrap()
}
fn entry(matrix: &SparseMatrix, r: usize, col: usize) -> f64 {
    matrix
        .triplets()
        .iter()
        .filter(|t| t.row == r && t.col == col)
        .map(|t| t.value)
        .sum()
}
fn band_gap(t: f64) -> f64 {
    1.16 - 7.02e-4 * t * t / (t + 1108.)
}
fn pbfact(t: f64) -> f64 {
    let arg = -band_gap(t) / (2. * BOLTZMANN * t) + 1.1150877 / (BOLTZMANN * 2. * REFTEMP);
    -2. * K_OVER_Q * t * (1.5 * (t / REFTEMP).ln() + CHARGE * arg)
}

/// The MOS1 state window of the (single) MOS device in `c`.
fn mos_states<'a>(c: &Circuit, values: &'a [f64]) -> &'a [f64] {
    let index = c
        .devices()
        .iter()
        .position(|d| d.designator() == 'm')
        .unwrap();
    &values[c.state_rows(index).unwrap()]
}

/// `DEVqmeyer` restated for the linear region (vgst > 0, vds < vdsat).
fn meyer_linear(vgst: f64, vds: f64, cox: f64) -> (f64, f64) {
    let vdsat = vgst.max(0.025);
    let vddif = 2. * vdsat - vds;
    (
        cox / 3. * (1. - (vdsat - vds).powi(2) / vddif.powi(2)),
        cox / 3. * (1. - vdsat * vdsat / vddif.powi(2)),
    )
}

const SATURATED: &str = "vd d 0 3\nvg g 0 2\nvb b 0 -0.5\nm1 d g 0 b mm w=10u l=2u\n\
    .model mm nmos(vto=0.8 kp=50u gamma=0.5 phi=0.7 tox=20n ld=0.1u cgso=0.4n cgdo=0.3n cgbo=0.2n)";

#[test]
fn operating_point_meyer_charge_and_ac_capacitance_are_two_halves_plus_overlap() {
    let c = circuit(SATURATED);
    let solved = bias(&c);
    let cox = 3.9 * EPS0 / 20e-9 * 1.8e-6 * 10e-6;
    let (ovs, ovd, ovb) = (0.4e-9 * 10e-6, 0.3e-9 * 10e-6, 0.2e-9 * 1.8e-6);
    let s = mos_states(&c, solved.trial.values());
    // mos1load.c (TRANOP): q = v * (2 half + overlap). Saturation: the
    // gate-source half is Cox/3, the others are zero.
    close(s[4], 2. * (2. * cox / 3. + ovs), 1e-12, 0.);
    // vgd = 2 - 3 = -1 V.
    close(s[6], -ovd, 1e-12, 0.);
    close(s[8], 2.5 * ovb, 1e-12, 0.);
    for derivative in [5, 7, 9] {
        assert_eq!(s[derivative], 0.);
    }
    // mos1acld.c: the same capacitances, j omega times.
    let system = c
        .small_signal_system(&ModelContext::default(), &solved.values)
        .unwrap();
    let (g, d, b) = (row(&c, "g"), row(&c, "d"), row(&c, "b"));
    close(
        entry(&system.e, g, g),
        2. * cox / 3. + ovs + ovd + ovb,
        1e-12,
        0.,
    );
    close(entry(&system.e, g, d), -ovd, 1e-12, 0.);
    close(entry(&system.e, g, b), -ovb, 1e-12, 0.);
    close(entry(&system.e, d, d), ovd, 1e-12, 0.);
}

fn transient_load(
    c: &Circuit,
    history: &StateHistory,
    coefficients: &Coefficients,
    x: &Vector,
) -> (SparseMatrix, Vector, ngspice_rs::devices::TrialState) {
    let n = c.unknown_count();
    let dt = coefficients.dt();
    let mut a = SparseMatrix::new(n, n);
    let mut b = Vector::zeros(n);
    let mut trial = history.trial();
    c.load(
        &LoadRequest {
            mode: AnalysisMode::Transient { time: dt, dt },
            solution: x,
            model_context: &ModelContext::default(),
            integration: Some(coefficients),
            history,
            forcing: Some(Forcing {
                limit: Limit::Right,
                timing: TransientTiming::new(dt, 10. * dt).unwrap(),
            }),
        },
        &mut a,
        &mut b,
        &mut trial,
    )
    .unwrap();
    (a, b, trial)
}

#[test]
fn transient_meyer_charge_advances_with_the_averaged_half_capacitances() {
    // Linear region (small vds): every Meyer half depends on the bias.
    let c = circuit(
        "vd d 0 0.2\nvg g 0 2\nvb b 0 0\nm1 d g 0 b mm w=10u l=2u\n\
         .model mm nmos(vto=0.8 kp=50u tox=20n cgso=0.4n cgdo=0.3n)",
    );
    let solved = bias(&c);
    let mut history = c.state_history();
    for _ in 0..ngspice_rs::devices::ACCEPTED_DEPTH {
        history.commit(solved.trial.clone()).unwrap();
    }
    let cox = 3.9 * EPS0 / 20e-9 * 2e-6 * 10e-6;
    let (ovs, ovd) = (0.4e-9 * 10e-6, 0.3e-9 * 10e-6);
    let (gs1, gd1) = meyer_linear(2. - 0.8, 0.2, cox);
    let old = mos_states(&c, history.accepted(1).unwrap()).to_vec();
    close(old[13], gs1, 1e-12, 0.);
    close(old[14], gd1, 1e-12, 0.);
    close(old[4], 2. * (2. * gs1 + ovs), 1e-12, 0.);
    close(old[6], 1.8 * (2. * gd1 + ovd), 1e-12, 0.);
    // Move the gate and drain: the new charge is the old one plus the voltage
    // step times (half(new) + half(old) + overlap).
    let mut x = solved.values.clone();
    x.as_mut_slice()[row(&c, "g")] = 2.3;
    x.as_mut_slice()[row(&c, "d")] = 0.35;
    let dt = 1e-9;
    let coefficients = StepHistory::new()
        .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
        .unwrap();
    let (a, _, trial) = transient_load(&c, &history, &coefficients, &x);
    let s = mos_states(&c, trial.values());
    let (gs, gd) = meyer_linear(2.3 - 0.8, 0.35, cox);
    close(s[13], gs, 1e-12, 0.);
    close(s[14], gd, 1e-12, 0.);
    let qgs = old[4] + (2.3 - 2.) * (gs + gs1 + ovs);
    let qgd = old[6] + ((2.3 - 0.35) - 1.8) * (gd + gd1 + ovd);
    close(s[4], qgs, 1e-12, 0.);
    close(s[6], qgd, 1e-12, 0.);
    // Backward Euler: the derivative is (q - q1) / dt and the companion
    // conductance is the averaged capacitance / dt (mos1load.c NIintegrate).
    close(s[5], (qgs - old[4]) / dt, 1e-10, 0.);
    close(s[7], (qgd - old[6]) / dt, 1e-10, 0.);
    let (g, d) = (row(&c, "g"), row(&c, "d"));
    close(entry(&a, g, d), -(gd + gd1 + ovd) / dt, 1e-12, 0.);
}

/// `r(x) = A(x) x - b(x)` of one load, for finite differences.
fn residual(a: &SparseMatrix, b: &Vector, x: &Vector) -> Vec<f64> {
    let mut r: Vec<f64> = b.as_slice().iter().map(|v| -v).collect();
    for t in a.triplets() {
        r[t.row] += t.value * x.as_slice()[t.col];
    }
    r
}

#[test]
fn transient_companion_jacobian_is_exact_where_the_meyer_halves_are_constant() {
    // Saturation: the halves are Cox/3, 0, 0 regardless of a small
    // perturbation; junction depletion charge, series resistance and the
    // channel all have exact derivatives.
    let c = circuit(
        "vd d 0 3\nvg g 0 2\nvb b 0 -0.5\nm1 d g 0 b mm w=10u l=2u ad=20p as=20p pd=24u ps=24u\n\
         .model mm nmos(vto=0.8 kp=50u gamma=0.5 phi=0.7 lambda=0.03 tox=20n rd=40 rs=30\n\
         + cj=0.5m cjsw=0.4n mjsw=0.33 cgso=0.4n cgdo=0.3n cgbo=0.2n)",
    );
    let solved = bias(&c);
    let mut history = c.state_history();
    for _ in 0..ngspice_rs::devices::ACCEPTED_DEPTH {
        history.commit(solved.trial.clone()).unwrap();
    }
    let dt = 1e-9;
    let mut steps = StepHistory::new();
    let first = steps
        .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
        .unwrap();
    steps.accept(&first);
    let coefficients = steps
        .trial(IntegrationMethod::Trapezoidal, 2, dt, DEFAULT_XMU)
        .unwrap();
    let mut x = solved.values.clone();
    x.as_mut_slice()[row(&c, "g")] += 0.05;
    x.as_mut_slice()[row(&c, "d")] -= 0.1;
    let (a, _, _) = transient_load(&c, &history, &coefficients, &x);
    let n = c.unknown_count();
    for col in 0..n {
        let h = 1e-6;
        let mut low = x.clone();
        let mut high = x.clone();
        low.as_mut_slice()[col] -= h;
        high.as_mut_slice()[col] += h;
        let (al, bl, _) = transient_load(&c, &history, &coefficients, &low);
        let (ah, bh, _) = transient_load(&c, &history, &coefficients, &high);
        let rl = residual(&al, &bl, &low);
        let rh = residual(&ah, &bh, &high);
        for r in 0..n {
            let numerical = (rh[r] - rl[r]) / (2. * h);
            close(numerical, entry(&a, r, col), 1e-6, 1e-9);
        }
    }
}

#[test]
fn dc_jacobian_with_series_resistance_body_bias_geometry_and_temperature() {
    for body in [
        // Saturation, reverse body bias, RSH squares, area junctions, 80 C.
        "vd c 0 3\nvg g 0 2\nvs e 0 0\nvb bulk 0 -0.5\n\
         m1 c g e bulk mm w=10u l=2u nrd=2 nrs=3 ad=20p as=20p pd=24u ps=24u temp=80\n\
         .model mm nmos(vto=0.8 kp=50u gamma=0.5 phi=0.7 lambda=0.03 rsh=20 js=1e-4)",
        // Triode with forward body bias (GAMMA > 0, sarg > 0).
        "vd c 0 0.3\nvg g 0 2\nvs e 0 0\nvb bulk 0 0.3\nm1 c g e bulk mm w=10u l=2u\n\
         .model mm nmos(vto=0.8 kp=50u gamma=0.5 phi=0.7 lambda=0.03 rd=50 rs=40)",
        // Forward body bias beyond 2 PHI: sarg clamps at zero.
        "vd c 0 1\nvg g 0 2\nvs e 0 0\nvb bulk 0 0.5\nm1 c g e bulk mm w=10u l=2u\n\
         .model mm nmos(vto=0.8 kp=50u gamma=0.5 phi=0.2 lambda=0.03 rd=50)",
        // Reversed drain/source (inverse mode), PMOS, process extraction.
        "vd c 0 0.5\nvg g 0 -2\nvs e 0 0\nvb bulk 0 0.6\nm1 c g e bulk mm w=8u l=1u dtemp=20\n\
         .model mm pmos(tox=25n nsub=1e16 nss=2e10 u0=250 lambda=0.02 rs=60)",
    ] {
        let c = circuit(body);
        let context = ModelContext::default();
        let history = c.state_history();
        let solved = bias(&c);
        let system = c.small_signal_system(&context, &solved.values).unwrap();
        let n = c.unknown_count();
        let load = |x: &Vector| {
            let mut a = SparseMatrix::new(n, n);
            let mut b = Vector::zeros(n);
            let mut trial = history.trial();
            c.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: x,
                    model_context: &context,
                    integration: None,
                    history: &history,
                    forcing: None,
                },
                &mut a,
                &mut b,
                &mut trial,
            )
            .unwrap();
            residual(&a, &b, x)
        };
        for node in c.nodes().nodes() {
            let Some(col) = c.unknowns().node_row(node.id) else {
                continue;
            };
            let h = 1e-6;
            let mut low = solved.values.clone();
            let mut high = solved.values.clone();
            low.as_mut_slice()[col] -= h;
            high.as_mut_slice()[col] += h;
            let (rl, rh) = (load(&low), load(&high));
            for r in 0..n {
                let numerical = (rh[r] - rl[r]) / (2. * h);
                close(numerical, entry(&system.a, r, col), 1e-6, 1e-10);
            }
        }
    }
}

#[test]
fn series_resistance_creates_internal_nodes_carrying_the_drain_current() {
    let mut c = circuit(
        "vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm w=10u l=2u nrd=4\n\
         .model mm nmos(vto=0.8 kp=50u rsh=25 rs=60)",
    );
    for (name, present) in [("m1#drain", true), ("m1#source", true)] {
        let node = c.nodes().get(name);
        assert_eq!(node.is_some(), present, "{name}");
        assert_eq!(
            c.nodes().node(node.unwrap()).unwrap().kind,
            NodeKind::Internal
        );
    }
    let solved = bias(&c);
    let (d, drain, source) = (
        solved.values.as_slice()[row(&c, "d")],
        solved.values.as_slice()[row(&c, "m1#drain")],
        solved.values.as_slice()[row(&c, "m1#source")],
    );
    let p = op(&mut c);
    let id = -p.value("i(vd)", 0).unwrap().re;
    // The channel sees the internal nodes: square law at vgs' and vds'.
    let (vgs, vds) = (2. - source, drain - source);
    assert!(vgs - 0.8 <= vds);
    let channel = 0.5 * 50e-6 * 5. * (vgs - 0.8).powi(2);
    // Bulk (ground) junctions: the drain one is reverse biased beyond -3 Vt
    // (constant -IS), the source one sits at -v(m1#source).
    let vt = K_OVER_Q * REFTEMP;
    let bulk_to_source = 1e-14 * ((-source / vt).exp() - 1.) - GMIN * source;
    // RSH * NRD = 100 ohm drain carries the whole drain-terminal current;
    // the explicit RS = 60 ohm carries the channel plus the source junction.
    close((d - drain) / 100., id, 1e-9, 1e-18);
    close(id, channel + 1e-14 + GMIN * drain, 1e-9, 1e-18);
    close(source / 60., channel + bulk_to_source, 1e-9, 1e-18);
    // No RD/RS/RSH: no internal nodes.
    let plain = circuit("vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm\n.model mm nmos(rd=0)");
    assert!(plain.nodes().get("m1#drain").is_none());
    assert!(plain.nodes().get("m1#source").is_none());
}

#[test]
fn process_extraction_follows_mos1temp() {
    // TOX and NSUB with KP/VTO/GAMMA/PHI all derived (TPG defaults to 1).
    let mut c =
        circuit("vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n.model mm nmos(tox=25n nsub=1e16)");
    let tnom = REFTEMP;
    let vtnom = K_OVER_Q * tnom;
    let cox = 3.9 * EPS0 / 25e-9;
    let kp = 600. * cox * 1e-4;
    let density: f64 = 1e16 * 1e6;
    let phi = (2. * vtnom * (density / 1.45e16).ln()).max(0.1);
    let eg = band_gap(tnom);
    let gamma = (2. * 11.7 * EPS0 * CHARGE * density).sqrt() / cox;
    let wkfng = 3.25 + 0.5 * eg - 0.5 * eg;
    let vfb = wkfng - (3.25 + 0.5 * eg + 0.5 * phi);
    let vto = vfb + gamma * phi.sqrt() + phi;
    let p = op(&mut c);
    let id = 0.5 * kp * 5. * (3. - vto).powi(2);
    // Reverse drain junction: IS plus gmin * 4 V.
    close(
        -p.value("i(vd)", 0).unwrap().re,
        id + 1e-14 + 4. * GMIN,
        1e-9,
        1e-15,
    );
    // TPG = -1 flips the gate work function by the band gap; NSS shifts
    // the flat band by -q NSS / Cox.
    let mut c = circuit(
        "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n\
         .model mm nmos(tox=25n nsub=1e16 tpg=-1 nss=1e11 kp=40u)",
    );
    let vfb =
        (3.25 + 0.5 * eg + 0.5 * eg) - (3.25 + 0.5 * eg + 0.5 * phi) - 1e11 * 1e4 * CHARGE / cox;
    let vto = vfb + gamma * phi.sqrt() + phi;
    let p = op(&mut c);
    close(
        -p.value("i(vd)", 0).unwrap().re,
        0.5 * 40e-6 * 5. * (3. - vto).powi(2) + 1e-14 + 4. * GMIN,
        1e-9,
        1e-15,
    );
}

#[test]
fn temperature_scales_kp_threshold_and_saturation_current_per_mos1temp() {
    let mut c = circuit(
        "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u temp=100\n\
         .model mm nmos(vto=0.9 kp=60u phi=0.65 is=1e-13)",
    );
    let (t, tn) = (373.15, REFTEMP);
    let ratio = t / tn;
    let kp = 60e-6 / (ratio * ratio.sqrt());
    let tphi = t / REFTEMP * (0.65 - pbfact(tn)) / (tn / REFTEMP) + pbfact(t);
    let vbi = 0.9 + 0.5 * (band_gap(tn) - band_gap(t)) + 0.5 * (tphi - 0.65);
    let is = 1e-13 * (-band_gap(t) / (K_OVER_Q * t) + band_gap(tn) / (K_OVER_Q * tn)).exp();
    let p = op(&mut c);
    close(
        -p.value("i(vd)", 0).unwrap().re,
        0.5 * kp * 5. * (3. - vbi).powi(2) + is + 4. * GMIN,
        1e-9,
        1e-15,
    );
    // The same device at its nominal temperature is the plain square law.
    let mut nominal = circuit(
        "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n.model mm nmos(vto=0.9 kp=60u phi=0.65 is=1e-13)",
    );
    let p = op(&mut nominal);
    close(
        -p.value("i(vd)", 0).unwrap().re,
        0.5 * 60e-6 * 5. * (3f64 - 0.9).powi(2) + 1e-13 + 4. * GMIN,
        1e-12,
        1e-15,
    );
}

#[test]
fn only_the_meyer_gate_charges_control_the_timestep() {
    let c = circuit(SATURATED);
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    // mos1trun.c: CKTterr on qgs, qgd and qgb only.
    assert_eq!(mos.truncation_slots(), vec![4, 6, 8]);
}

#[test]
fn unported_and_invalid_mos1_inputs_fail_explicitly() {
    let body = "m1 d g 0 0 mm\n.model mm nmos(kf=1e-25)";
    match Circuit::from_netlist(&deck(body)) {
        Err(SpiceError::NotYetPorted { c_reference, .. }) => {
            assert!(c_reference.contains("mos1noi.c"), "{body}: {c_reference}");
        }
        other => panic!("{body}: {other:?}"),
    }
    // OFF and the IC vector are ported (#99, mos1load.c/mos1ic.c).
    for body in [
        "m1 d g 0 0 mm off\n.model mm nmos",
        "m1 d g 0 0 mm ic=1,2,3 icvds=1\n.model mm nmos",
    ] {
        Circuit::from_netlist(&deck(body)).unwrap();
    }
    for body in [
        // NSUB below the intrinsic density (mos1temp.c "Nsub < Ni").
        "m1 d g 0 0 mm\n.model mm nmos(tox=20n nsub=1e9)",
        // RSH with zero drain squares: C divides by zero.
        "m1 d g 0 0 mm nrd=0\n.model mm nmos(rsh=10)",
        // Explicit RD=0 next to RSH: C would create a floating node.
        "m1 d g 0 0 mm\n.model mm nmos(rd=0 rsh=10)",
        // Nonpositive effective length.
        "m1 d g 0 0 mm l=1u\n.model mm nmos(ld=0.6u)",
        // Non-integer gate type.
        "m1 d g 0 0 mm\n.model mm nmos(tox=20n nsub=1e16 tpg=0.5)",
        "m1 d g 0 0 mm\n.model mm nmos(mjsw=1)",
    ] {
        assert!(Circuit::from_netlist(&deck(body)).is_err(), "{body}");
    }
}

#[test]
fn new_m7_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in [
        "m7_mos1_inverter_tran",
        "m7_mos1_ring_tran",
        "m7_mos1_meyer_ac",
        "m7_mos1_process_dc",
    ] {
        let n = Parser::new()
            .parse_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("conformance/netlists/{name}.cir")),
            )
            .unwrap();
        let text = ngspice_rs::netlist::write_netlist(&n).unwrap();
        let round = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("round.cir"), &text))
            .unwrap();
        assert!(ngspice_rs::netlist::semantic_eq(&n, &round));
        assert_eq!(text, ngspice_rs::netlist::write_netlist(&round).unwrap());
    }
}
