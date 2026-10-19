//! MOS level 6 on the shared MOS shell (#89): closed forms restated from
//! `mos6temp.c`/`mos6load.c`, C's per-unit `M` handling of `gmin`, whole-
//! circuit Jacobians against finite differences, Meyer capacitances, C's
//! internal node names and the explicit refusals (C's level 6 has no AC,
//! pole-zero, noise, distortion or sensitivity routines). C-golden
//! comparisons (`m10_mos6_*`) live in `xtask golden verify`.
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
fn drain_current(body: &str) -> f64 {
    -op(&mut circuit(body)).value("i(vd)", 0).unwrap().re
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

/// `mos6load.c`'s drain current in the normalized frame for the
/// temperature-scaled `betac` (with M), `vbi` and `phi`.
#[allow(clippy::too_many_arguments)]
fn power_law(
    betac: f64,
    [kv, nv, nc]: [f64; 3],
    [vbi, gamma, gamma1, sigma, phi]: [f64; 5],
    [lambda0, lambda1]: [f64; 2],
    [vgs, vds, vbs]: [f64; 3],
) -> f64 {
    let sarg = if vbs <= 0. {
        (phi - vbs).sqrt()
    } else {
        (phi.sqrt() - vbs / (2. * phi.sqrt())).max(0.)
    };
    let vgon = vgs - (vbi + gamma * sarg - gamma1 * vbs - sigma * vds);
    if vgon <= 0. {
        return 0.;
    }
    let vdsat = kv * vgon.powf(nv);
    let mut id = betac * vgon.powf(nc) * (1. + (lambda0 - lambda1 * vbs) * vds);
    if vdsat > vds {
        let x = vds / vdsat;
        id *= (2. - x) * x;
    }
    id
}

#[test]
fn the_simplest_level_six_card_is_linear_in_the_gate_overdrive_with_c_defaults() {
    // mos6set.c: KV = 2, NV = 0.5, KC = 5e-5, NC = 1, VTO = 0, no CLM. At
    // vgs = 1 V, vdsat = 2 V: vds = 4 V saturates (Id = KC W/L vgs), vds =
    // 0.5 V is linear ((2 - x) x with x = 1/4). The drain-bulk junction adds
    // IS + gmin vds.
    let betac = 5e-5 * 10. / 2.;
    for (vds, factor) in [(4., 1.), (0.5, (2. - 0.25) * 0.25)] {
        let id = drain_current(&format!(
            "vd d 0 {vds}\nvg g 0 1\nm1 d g 0 0 mm w=10u l=2u\n.model mm nmos(level=6)"
        ));
        close(id, betac * factor + 1e-14 + vds * GMIN, 1e-9, 1e-18);
    }
}

#[test]
fn threshold_clm_and_body_terms_follow_mos6load() {
    // GAMMA, GAMMA1, SIGMA, LAMBDA0/LAMBDA1, LD in reverse body bias, with a
    // PMOS mirror; NC/NV away from their defaults; the LAMBDA alias.
    let law = [1.7, 0.75, 1.35];
    let threshold = |vto: f64| [vto - 0.45 * 0.7f64.sqrt(), 0.45, 0.04, 0.025, 0.7];
    for (vds, vgs) in [(3., 2.5), (0.4, 2.5), (1.2, 1.4)] {
        let card = "kc=7e-5 kv=1.7 nv=0.75 nc=1.35 gamma=0.45 gamma1=0.04 sigma=0.025 \
                    phi=0.7 lambda0=0.03 lambda1=0.012 ld=0.1u";
        let betac = 7e-5 * 8. / (1.5e-6 - 0.2e-6) * 1e-6;
        let expected = power_law(betac, law, threshold(0.75), [0.03, 0.012], [vgs, vds, -0.8]);
        let n = drain_current(&format!(
            "vd d 0 {vds}\nvg g 0 {vgs}\nvb b 0 -0.8\nm1 d g 0 b mm w=8u l=1.5u\n\
             .model mm nmos(level=6 vto=0.75 {card})"
        ));
        // The drain junction is reverse biased by vds + 0.8 V.
        close(n, expected + 1e-14 + (vds + 0.8) * GMIN, 1e-9, 1e-18);
        let p = drain_current(&format!(
            "vd d 0 {}\nvg g 0 {}\nvb b 0 0.8\nm1 d g 0 b mm w=8u l=1.5u\n\
             .model mm pmos(level=6 vto=-0.75 {card})",
            -vds, -vgs
        ));
        close(p, -(expected + 1e-14 + (vds + 0.8) * GMIN), 1e-9, 1e-18);
    }
    // LAMBDA sets LAMBDA0 when LAMBDA0 is not given; LAMBDA0 wins otherwise.
    let base = "vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm w=10u l=2u\n.model mm nmos(level=6 vto=0.5";
    let with = |extra: &str| drain_current(&format!("{base} {extra})"));
    let id = |lambda: f64| {
        power_law(
            2.5e-4,
            [2., 0.5, 1.],
            [0.5, 0., 0., 0., 0.6],
            [lambda, 0.],
            [2., 3., 0.],
        )
    };
    close(
        with("lambda=0.05"),
        id(0.05) + 1e-14 + 3. * GMIN,
        1e-9,
        1e-18,
    );
    close(
        with("lambda=0.05 lambda0=0.02"),
        id(0.02) + 1e-14 + 3. * GMIN,
        1e-9,
        1e-18,
    );
    // NVTH and PS are settable in C and unused by mos6load.c.
    close(with("nvth=3 ps=2"), id(0.) + 1e-14 + 3. * GMIN, 1e-9, 1e-18);
}

#[test]
fn process_extraction_and_temperature_follow_mos6temp() {
    // TOX: KC = 0.5 U0 Cox 1e-4 (U0 = 600 when absent). NSUB derives PHI,
    // GAMMA and VTO with the 1.45e16 m^-3 intrinsic density at TNOM; the
    // device runs at TEMP: KC / ratio^1.5, PHI and VBI scale as in level 1.
    for (tnom, temp, uo) in [(27., 27., None), (20., 60., Some(450.))] {
        let uo_card = uo.map_or(String::new(), |u| format!(" uo={u}"));
        let body = format!(
            "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u temp={temp}\n\
             .model mm nmos(level=6 tox=25n nsub=1e16 nss=1e10 kv=3 nv=0.6 nc=1.2 \
             tnom={tnom}{uo_card})"
        );
        let (tn, t) = (tnom + 273.15, temp + 273.15);
        let vtnom = K_OVER_Q * tn;
        let eg1 = band_gap(tn);
        let cox = 3.9 * EPS0 / 25e-9;
        let kc = 0.5 * uo.unwrap_or(600.) * cox * 1e-4;
        let density: f64 = 1e22;
        let phi = (2. * vtnom * (density / 1.45e16).ln()).max(0.1);
        let gamma = (2. * 11.7 * EPS0 * CHARGE * density).sqrt() / cox;
        let vfb = (3.25 + 0.5 * eg1 - 0.5 * eg1)
            - (3.25 + 0.5 * eg1 + 0.5 * phi)
            - 1e10 * 1e4 * CHARGE / cox;
        let vto = vfb + gamma * phi.sqrt() + phi;
        let ratio: f64 = t / tn;
        let pbfact = |t: f64| {
            let arg = -band_gap(t) / (2. * BOLTZMANN * t) + 1.1150877 / (BOLTZMANN * 2. * REFTEMP);
            -2. * K_OVER_Q * t * (1.5 * (t / REFTEMP).ln() + CHARGE * arg)
        };
        let tphi = t / REFTEMP * (phi - pbfact(tn)) / (tn / REFTEMP) + pbfact(t);
        let vbi = vto - gamma * phi.sqrt() + 0.5 * (eg1 - band_gap(t)) + 0.5 * (tphi - phi);
        let expected = power_law(
            kc / (ratio * ratio.sqrt()) * 5.,
            [3., 0.6, 1.2],
            [vbi, gamma, 0., 0., tphi],
            [0., 0.],
            [3., 4., 0.],
        );
        let is = 1e-14 * (-band_gap(t) / (K_OVER_Q * t) + eg1 / vtnom).exp();
        close(drain_current(&body), expected + is + 4. * GMIN, 1e-9, 1e-18);
    }
}

#[test]
fn the_multiplier_scales_gmin_with_the_stamp_as_mos6load_does() {
    // An off device (vgs = 0 < VTO): only the drain junction at -4 V, which
    // mos6load.c stamps M times, gmin included: M (IS + 4 gmin). Levels 1
    // and 3 add gmin once.
    for m in [1., 3.] {
        let body =
            format!("vd d 0 4\nvg g 0 0\nm1 d g 0 0 mm m={m}\n.model mm nmos(level=6 vto=1)");
        close(drain_current(&body), m * (1e-14 + 4. * GMIN), 1e-12, 0.);
        let level1 = body.replace("level=6", "level=1");
        close(drain_current(&level1), m * 1e-14 + 4. * GMIN, 1e-12, 0.);
    }
    // The channel and junction currents scale with M.
    let one = drain_current("vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm\n.model mm nmos(level=6 vto=0.5)");
    let two =
        drain_current("vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm m=2\n.model mm nmos(level=6 vto=0.5)");
    close(two, 2. * one, 1e-12, 0.);
}

#[test]
fn series_resistance_follows_mos6set() {
    let c = circuit(
        "vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm w=10u l=2u nrs=3\n\
         .model mm nmos(level=6 vto=0.8 kc=1e-4 rd=50 rsh=20)",
    );
    // mos6set.c: CKTmkVolt(..., "drain"/"source").
    for name in ["m1#drain", "m1#source"] {
        let node = c.nodes().get(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(c.nodes().node(node).unwrap().kind, NodeKind::Internal);
    }
    let solved = bias(&c);
    let drain = solved.values.as_slice()[row(&c, "m1#drain")];
    let source = solved.values.as_slice()[row(&c, "m1#source")];
    // RD = 50 ohm carries the drain-terminal current; RSH * NRS = 60 ohm the
    // source current.
    assert!(drain < 3. && source > 0.);
    close((3. - drain) / 50., source / 60., 1e-6, 1e-12);
    let plain = circuit("vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm\n.model mm nmos(level=6)");
    assert!(plain.nodes().get("m1#drain").is_none());
    // mos6set.c gives NRD/NRS no default: C applies no sheet resistance to
    // an instance without them, which the port refuses (here NRD is
    // missing; RS covers the source).
    match Circuit::from_netlist(&deck(
        "vd d 0 3\nm1 d d 0 0 mm nrs=1\n.model mm nmos(level=6 rsh=20)",
    )) {
        Err(SpiceError::Unsupported { feature, .. }) => {
            assert!(feature.contains("NRD"), "{feature}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert!(
        Circuit::from_netlist(&deck(
            "vd d 0 3\nm1 d d 0 0 mm nrd=1\n.model mm nmos(level=6 rsh=20)"
        ))
        .is_err()
    );
    assert!(
        Circuit::from_netlist(&deck(
            "vd d 0 3\nm1 d d 0 0 mm nrd=1\n.model mm nmos(level=6 rsh=20 rs=5)"
        ))
        .is_ok()
    );
}

#[test]
fn meyer_and_overlap_capacitances_use_the_effective_length() {
    // Saturation: the gate-source half is Cox/3 with Cox over
    // (L - 2 LD) W M; CGBO uses the effective length, CGSO/CGDO the width.
    let c = circuit(
        "vd d 0 4\nvg g 0 2\nvb b 0 -0.5\nm1 d g 0 b mm w=10u l=2u m=2\n\
         .model mm nmos(level=6 vto=0.7 kc=5e-5 kv=1 nv=0.5 tox=20n ld=0.1u\n\
         + cgso=0.4n cgdo=0.3n cgbo=0.2n)",
    );
    let solved = bias(&c);
    let leff = 2e-6 - 0.2e-6;
    let cox = 3.9 * EPS0 / 20e-9 * leff * 10e-6 * 2.;
    let (ovs, ovd, ovb) = (0.4e-9 * 2. * 10e-6, 0.3e-9 * 2. * 10e-6, 0.2e-9 * 2. * leff);
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
    // mos6trun.c: only the gate charges control the timestep.
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    assert_eq!(mos.truncation_slots(), vec![4, 6, 8]);
}

#[test]
fn dc_jacobian_matches_finite_differences() {
    // Saturation with GAMMA1/SIGMA/LAMBDA1 in reverse body bias with
    // junction geometry, temperature and RSH; the linear region in forward
    // body bias with M and RD/RS; an inverse-mode PMOS.
    for body in [
        "vd c 0 2\nvg g 0 1.6\nvs e 0 0\nvb bulk 0 -0.5\n\
         m1 c g e bulk mm w=10u l=1u ad=20p as=20p pd=24u ps=24u temp=80 nrd=1 nrs=2\n\
         .model mm nmos(level=6 vto=0.7 tox=20n nsub=1e16 kv=1.5 nv=0.8 nc=1.3 gamma1=0.03 \
         sigma=0.02 lambda0=0.04 lambda1=0.01 rsh=20 js=1e-4 cj=0.5m)",
        "vd c 0 0.2\nvg g 0 2\nvs e 0 0\nvb bulk 0 0.3\nm1 c g e bulk mm w=10u l=1u m=2\n\
         .model mm nmos(level=6 vto=0.7 kc=6e-5 gamma=0.5 phi=0.7 gamma1=0.05 sigma=0.03 \
         lambda0=0.02 lambda1=0.01 kv=1.2 nv=0.7 nc=1.4 rd=50 rs=40)",
        "vd c 0 0.4\nvg g 0 -2.5\nvs e 0 0\nvb bulk 0 0.6\nm1 c g e bulk mm w=8u l=1u dtemp=20\n\
         .model mm pmos(level=6 tox=25n vto=-0.8 u0=250 gamma=0.4 sigma=0.01 lambda0=0.05 \
         rs=60 rd=30)",
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
fn analyses_c_has_no_mos6_routine_for_fail_explicitly() {
    let stage = "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1 distof1 0.1\nm1 out in 0 0 mm w=10u l=2u\n\
                 r1 vdd out 10k\n.model mm nmos(level=6 vto=0.8 kc=1e-4)";
    // The DC operating point and transient run.
    assert!(run(&format!("{stage}\n.op")).is_ok());
    assert!(run(&format!("{stage}\n.tran 1n 10n")).is_ok());
    // mos6init.c: no DEVacLoad/DEVpzLoad. C would drop the device from the
    // small-signal matrix; the port refuses.
    for card in [
        ".ac dec 2 1k 10k",
        ".pz in 0 out 0 vol pz",
        ".noise v(out) v1 dec 2 1k 10k",
    ] {
        let error = run(&format!("{stage}\n{card}")).unwrap_err();
        assert!(
            matches!(&error, SpiceError::Unsupported { feature, .. } if feature.contains("mos6init.c")),
            "{card}: {error}"
        );
    }
    // Modern .sens uses cktsens.c's finite-difference walk, not the null
    // old sensitivity routine slots. Filter KP's level-6 counterpart here
    // because the full walk's absolute LD step collapses this 2 um channel.
    assert!(run(&format!("{stage}\n.sens v(out) m1:kc")).is_ok());
    // .disto is still an explicit error, never a missing device.
    {
        let card = ".disto dec 2 1k 10k";
        let error = run(&format!("{stage}\n{card}")).unwrap_err();
        assert!(
            matches!(
                error,
                SpiceError::Unsupported { .. } | SpiceError::NotYetPorted { .. }
            ),
            "{card}: {error}"
        );
    }
}

#[test]
fn invalid_mos6_inputs_fail_explicitly() {
    for body in [
        // Setters of other levels that MOS6mParam does not have.
        "m1 d g 0 0 mm\n.model mm nmos(level=6 kp=1e-4)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 kf=1e-24)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 nlev=3)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 eta=0.1)",
        // mos6temp.c "Phi is not positive" and "Nsub < Ni".
        "m1 d g 0 0 mm\n.model mm nmos(level=6 phi=0)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 tox=20n nsub=1e9)",
        // C warns and continues for L - 2 LD <= 0.
        "m1 d g 0 0 mm l=1u\n.model mm nmos(level=6 ld=0.5u)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 mj=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 tpg=0.5)",
        "m1 d g 0 0 mm\n.model mm nmos(level=6 kc=-1)",
    ] {
        assert!(Circuit::from_netlist(&deck(body)).is_err(), "{body}");
    }
}

#[test]
fn m10_mos6_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in ["m10_mos6_dc", "m10_mos6_tran"] {
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
