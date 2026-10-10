//! MOS level 3 on the shared MOS shell (#89): closed forms restated from
//! `mos3temp.c`/`mos3load.c`, whole-circuit Jacobians against finite
//! differences, effective-geometry Meyer capacitances, C's internal node
//! names and the explicit refusals. C-golden comparisons (`m10_mos3_*`) live
//! in `xtask golden verify`; `tests/c_noise_reference.rs` compares MOS3
//! `.noise` with live C.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use ngspice_rs::devices::{AnalysisMode, Circuit, LoadRequest, ModelContext};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError, SpiceResult};
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
            Path::new("m10.cir"),
            &format!("M10\n{body}\n.end\n"),
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
fn residual(a: &SparseMatrix, b: &Vector, x: &Vector) -> Vec<f64> {
    let mut r: Vec<f64> = b.as_slice().iter().map(|v| -v).collect();
    for t in a.triplets() {
        r[t.row] += t.value * x.as_slice()[t.col];
    }
    r
}
fn band_gap(t: f64) -> f64 {
    1.16 - 7.02e-4 * t * t / (t + 1108.)
}
/// Every analysis of `body`, through `RunConfig` as the CLI runs it.
fn run(body: &str) -> SpiceResult<Vec<Plot>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("m10.cir"),
        &format!("M10\n{body}\n.end\n"),
    ))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run_plots(&mut circuit, &request, &config.context())
}

/// The cubic reverse-bias law of `mos3load.c` for a junction at `v <= -3 Vt`.
fn reverse(is: f64, v: f64, vt: f64) -> f64 {
    let arg = (3. * vt / (v * std::f64::consts::E)).powi(3);
    -is * (1. + arg)
}

#[test]
fn the_simplest_level_three_card_is_the_square_law_with_c_defaults() {
    // GAMMA, ETA, THETA, VMAX, NFS, XJ, DELTA all zero and no NSUB (so no
    // channel-length modulation): mos3load.c reduces to 0.5 beta vgst^2 in
    // saturation. TOX defaults to 1e-7 m, so KP = U0 Cox 1e-4 = 600 Cox 1e-4.
    let vt = K_OVER_Q * REFTEMP;
    for (card, kp) in [
        ("vto=0.8 kp=50u", 50e-6),
        ("vto=0.8", 600. * 3.9 * EPS0 / 1e-7 * 1e-4),
        ("vto=0.8 uo=400 tox=40n", 400. * 3.9 * EPS0 / 40e-9 * 1e-4),
    ] {
        let mut c = circuit(&format!(
            "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n.model mm nmos(level=3 {card})"
        ));
        let p = op(&mut c);
        // The drain-bulk junction sits at -4 V: the cubic reverse law plus gmin.
        let junction = -reverse(1e-14, -4., vt) + 4. * GMIN;
        close(
            -p.value("i(vd)", 0).unwrap().re,
            0.5 * kp * 5. * (3f64 - 0.8).powi(2) + junction,
            1e-9,
            1e-18,
        );
    }
}

#[test]
fn process_extraction_uses_the_tnom_intrinsic_density_of_mos3temp() {
    // NSUB with VTO/GAMMA/PHI derived and KAPPA = 0 (no channel-length
    // modulation): Id = 0.5 beta vgst^2 / (1 + fbody), fbody = GAMMA /
    // (4 sqrt(PHI)), with the intrinsic density scaled to TNOM.
    for tnom in [27., 50.] {
        let mut c = circuit(&format!(
            "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n\
             .model mm nmos(level=3 tox=25n nsub=1e16 kappa=0 tnom={tnom})"
        ));
        let tn = tnom + 273.15;
        let vtnom = K_OVER_Q * tn;
        let eg = band_gap(tn);
        let ni = 1.45e16
            * (tn / 300.)
            * (tn / 300.).sqrt()
            * (0.5 * eg * (1. / 300. - 1. / tn) / K_OVER_Q).exp();
        let cox = 3.9 * EPS0 / 25e-9;
        let density: f64 = 1e16 * 1e6;
        let phi = (2. * vtnom * (density / ni).ln()).max(0.1);
        let gamma = (2. * 11.7 * EPS0 * CHARGE * density).sqrt() / cox;
        let vfb = (3.25 + 0.5 * eg - 0.5 * eg) - (3.25 + 0.5 * eg + 0.5 * phi);
        let vto = vfb + gamma * phi.sqrt() + phi;
        // The device runs at 27 C: KP and PHI scale from TNOM.
        let t = REFTEMP;
        let ratio = t / tn;
        let kp = 600. * cox * 1e-4 / (ratio * ratio.sqrt());
        let pbfact = |t: f64| {
            let arg = -band_gap(t) / (2. * BOLTZMANN * t) + 1.1150877 / (BOLTZMANN * 2. * REFTEMP);
            -2. * K_OVER_Q * t * (1.5 * (t / REFTEMP).ln() + CHARGE * arg)
        };
        let tphi = t / REFTEMP * (phi - pbfact(tn)) / (tn / REFTEMP) + pbfact(t);
        let vbi = vto - gamma * phi.sqrt() + 0.5 * (eg - band_gap(t)) + 0.5 * (tphi - phi);
        let vth = vbi + gamma * tphi.sqrt();
        let fbody = 0.5 * gamma / (2. * tphi.sqrt());
        let is = 1e-14 * (-band_gap(t) / (K_OVER_Q * t) + eg / vtnom).exp();
        let p = op(&mut c);
        close(
            -p.value("i(vd)", 0).unwrap().re,
            0.5 * kp * 5. * (3. - vth).powi(2) / (1. + fbody) - reverse(is, -4., K_OVER_Q * t)
                + 4. * GMIN,
            1e-9,
            1e-18,
        );
    }
}

#[test]
fn series_resistance_creates_c_named_internal_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm w=10u l=2u nrs=3\n\
         .model mm nmos(level=3 vto=0.8 rd=50 rsh=20)",
    );
    // mos3set.c: CKTmkVolt(..., "internal#drain"/"internal#source").
    for name in ["m1#internal#drain", "m1#internal#source"] {
        let node = c.nodes().get(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(c.nodes().node(node).unwrap().kind, NodeKind::Internal);
    }
    let solved = bias(&c);
    let drain = solved.values.as_slice()[row(&c, "m1#internal#drain")];
    let source = solved.values.as_slice()[row(&c, "m1#internal#source")];
    // RD = 50 ohm carries the drain-terminal current; RSH * NRS = 60 ohm the
    // source current.
    assert!(drain < 3. && source > 0.);
    close((3. - drain) / 50., source / 60., 1e-6, 1e-12);
    let plain = circuit("vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm\n.model mm nmos(level=3)");
    assert!(plain.nodes().get("m1#internal#drain").is_none());
}

#[test]
fn meyer_and_overlap_capacitances_use_the_effective_geometry() {
    // Saturation: the gate-source half is Cox/3 with Cox over
    // (L - 2 LD + XL)(W - 2 WD + XW) M; the overlaps use the same widths.
    let c = circuit(
        "vd d 0 4\nvg g 0 2\nvb b 0 -0.5\nm1 d g 0 b mm w=10u l=2u m=2\n\
         .model mm nmos(level=3 vto=0.7 kp=50u tox=20n ld=0.1u xl=0.05u wd=0.2u xw=0.1u\n\
         + cgso=0.4n cgdo=0.3n cgbo=0.2n)",
    );
    let solved = bias(&c);
    let (leff, weff) = (2e-6 - 0.2e-6 + 0.05e-6, 10e-6 - 0.4e-6 + 0.1e-6);
    let cox = 3.9 * EPS0 / 20e-9 * leff * weff * 2.;
    let (ovs, ovd, ovb) = (0.4e-9 * 2. * weff, 0.3e-9 * 2. * weff, 0.2e-9 * 2. * leff);
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
    // mos3trun.c: only the gate charges control the timestep.
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    assert_eq!(mos.truncation_slots(), vec![4, 6, 8]);
}

#[test]
fn dc_jacobian_matches_finite_differences_where_c_is_exact() {
    // Bias points where mos3load.c's derivatives are exact (see the
    // `devices::mos3` unit tests for where C's are approximate): weak
    // inversion (NFS) with the XJ short-channel factor and NSUB but no
    // channel-length modulation, velocity saturation in the linear region,
    // inverse-mode PMOS with series resistance, DELTA in reverse body bias,
    // temperature and junction geometry.
    for body in [
        "vd c 0 2\nvg g 0 0.45\nvs e 0 0\nvb bulk 0 -0.5\n\
         m1 c g e bulk mm w=10u l=1u ad=20p as=20p pd=24u ps=24u temp=80\n\
         .model mm nmos(level=3 vto=0.7 tox=20n nsub=1e16 xj=0.25u nfs=2e11 eta=0.3 \
         theta=0.1 kappa=0 delta=0.5 rsh=20 js=1e-4 cj=0.5m)",
        "vd c 0 0.2\nvg g 0 2\nvs e 0 0\nvb bulk 0 -1\nm1 c g e bulk mm w=10u l=1u\n\
         .model mm nmos(level=3 vto=0.7 kp=60u gamma=0.5 phi=0.7 vmax=1e5 theta=0.1 eta=0.2 \
         delta=0.6 rd=50 rs=40)",
        "vd c 0 0.4\nvg g 0 -2.5\nvs e 0 0\nvb bulk 0 0.6\nm1 c g e bulk mm w=8u l=1u dtemp=20\n\
         .model mm pmos(level=3 tox=25n vto=-0.8 u0=250 gamma=0.4 eta=0.1 theta=0.1 rs=60 rd=30)",
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
                close(numerical, entry(&system.a, r, col), 1e-6, 1e-12);
            }
        }
    }
}

#[test]
fn noise_runs_through_the_shared_mos_generators() {
    let plots = run(
        "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1\nm1 out in 0 0 mm w=10u l=2u\nr1 vdd out 10k\n\
         .model mm nmos(level=3 vto=0.8 kp=1e-4 kf=1e-24 af=1.2 rd=20 rs=10)\n\
         .noise v(out) v1 dec 2 1k 100k 1",
    )
    .unwrap();
    let spectrum = &plots[0];
    for name in [
        "onoise_m1_rd",
        "onoise_m1_rs",
        "onoise_m1_id",
        "onoise_m1_1overf",
        "onoise_m1",
    ] {
        let value = spectrum.value(name, 0).unwrap_or_else(|| panic!("{name}"));
        assert!(value.re > 0., "{name}");
    }
}

fn expect_not_ported(body: &str, reference: &str) {
    match Circuit::from_netlist(&deck(body)) {
        Err(SpiceError::NotYetPorted { c_reference, .. }) => {
            assert!(c_reference.contains(reference), "{body}: {c_reference}");
        }
        other => panic!("{body}: expected NotYetPorted, got {other:?}"),
    }
}

#[test]
fn unported_mos_levels_name_their_c_directory() {
    for (level, directory) in [
        (2, "mos2/"),
        (4, "bsim1/"),
        (6, "mos6/"),
        (9, "mos9/"),
        (49, "bsim3/"),
        (54, "bsim4/"),
    ] {
        expect_not_ported(
            &format!("m1 d g 0 0 mm\n.model mm nmos(level={level})"),
            directory,
        );
    }
}

#[test]
fn unported_and_invalid_mos3_inputs_fail_explicitly() {
    for body in [
        // MOS3mPTable lists XD, ALPHA and INPUT_DELTA, but MOS3mParam has no
        // setter for them (E_BADPARM).
        "m1 d g 0 0 mm\n.model mm nmos(level=3 xd=1u)",
        "m1 d g 0 0 mm\n.model mm nmos(level=3 alpha=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=3 input_delta=1)",
        // A level-1 setter that MOS3 does not have.
        "m1 d g 0 0 mm\n.model mm nmos(level=3 lambda=0.02)",
        // TOX = 0 is an infinite oxide capacitance in C.
        "m1 d g 0 0 mm\n.model mm nmos(level=3 tox=0)",
        // mos3temp.c E_PARMVAL: nonpositive effective length and width.
        "m1 d g 0 0 mm l=1u\n.model mm nmos(level=3 ld=0.4u xl=-0.3u)",
        "m1 d g 0 0 mm w=1u\n.model mm nmos(level=3 wd=0.5u)",
        // NSUB below the TNOM intrinsic density (mos3temp.c "Nsub < Ni").
        "m1 d g 0 0 mm\n.model mm nmos(level=3 nsub=1e9)",
        // mos3temp.c "Phi is not positive".
        "m1 d g 0 0 mm\n.model mm nmos(level=3 phi=0)",
        "m1 d g 0 0 mm\n.model mm nmos(level=3 mjsw=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=3 nlev=4)",
    ] {
        assert!(Circuit::from_netlist(&deck(body)).is_err(), "{body}");
    }
    // A swept effective width that C rejects is refused too.
    let c = circuit("vd d 0 1\nm1 d d 0 0 mm w=2u\n.model mm nmos(level=3 wd=0.5u)");
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    assert!(
        mos.with_instance_parameter("w", 1e-6, &ModelContext::default())
            .is_err()
    );
    assert!(
        mos.with_instance_parameter("w", 3e-6, &ModelContext::default())
            .is_ok()
    );
    // .disto (mos3dset.c) and .sens are not ported: explicit errors, never
    // a silently linear or missing device.
    let stage = "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1 distof1 0.1\nm1 out in 0 0 mm w=10u l=2u\n\
                 r1 vdd out 10k\n.model mm nmos(level=3 vto=0.8 kp=1e-4)";
    let error = run(&format!("{stage}\n.disto dec 2 1k 10k")).unwrap_err();
    assert!(
        matches!(&error, SpiceError::NotYetPorted { c_reference, .. } if c_reference.contains("mos3dset.c")),
        "{error}"
    );
    assert!(run(&format!("{stage}\n.sens v(out)")).is_err());
}

#[test]
fn m10_mos3_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in ["m10_mos3_dc", "m10_mos3_ac", "m10_mos3_tran"] {
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
