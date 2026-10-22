//! MOS level 9 on the shared MOS shell (#89): `mos9/` is a copy of `mos3/`,
//! so these tests pin what is identical (bias, small-signal and transient
//! results equal level 3's bit for bit on a shared card), what differs
//! (`mos9temp.c`'s 1 um effective-size floor, no NLEV/GDSNOI setters, the
//! pre-NLEV `mos9noi.c` generators), whole-circuit Jacobians against finite
//! differences, C's internal node names and the explicit refusals. C-golden
//! comparisons (`m10_mos9_*`) live in `xtask golden verify`;
//! `tests/c_noise_reference.rs` and `tests/c_pole_zero.rs` compare MOS9
//! `.noise` and `.pz` with live C.
use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::{AnalysisMode, Circuit, LoadRequest, ModelContext};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::{NodeKind, SpiceError, SpiceResult};
use std::path::Path;

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
fn level(body: &str, level: u8) -> String {
    body.replace("level=9", &format!("level={level}"))
}

/// Cards exercising every `moseq3` branch: NSUB process extraction, XJ,
/// NFS weak inversion, VMAX, KAPPA, ETA, THETA, DELTA, temperature,
/// series resistance and junction geometry, PMOS and inverse mode.
const SHARED: [&str; 4] = [
    "vd c 0 2\nvg g 0 0.45\nvs e 0 0\nvb bulk 0 -0.5\n\
     m1 c g e bulk mm w=10u l=1.5u ad=20p as=20p pd=24u ps=24u temp=80\n\
     .model mm nmos(level=9 vto=0.7 tox=20n nsub=1e16 xj=0.25u nfs=2e11 eta=0.3 \
     theta=0.1 kappa=0 delta=0.5 rsh=20 js=1e-4 cj=0.5m)",
    "vd c 0 3\nvg g 0 2\nvs e 0 0\nvb bulk 0 -1\nm1 c g e bulk mm w=10u l=1.2u\n\
     .model mm nmos(level=9 vto=0.7 tox=20n nsub=1e16 xj=0.25u ld=0.08u uo=550 vmax=1.5e5 \
     nfs=2e11 eta=0.4 theta=0.08 kappa=0.5 delta=0.6 rd=50 rs=40)",
    "vd c 0 0.4\nvg g 0 -2.5\nvs e 0 0\nvb bulk 0 0.6\nm1 c g e bulk mm w=8u l=1.5u dtemp=20\n\
     .model mm pmos(level=9 tox=25n vto=-0.8 u0=250 gamma=0.4 eta=0.1 theta=0.1 rs=60 rd=30)",
    "vd c 0 -1\nvg g 0 1.8\nvs e 0 0\nvb bulk 0 -2\nm1 c g e bulk mm w=6u l=2u\n\
     .model mm nmos(level=9 tox=15n nsub=3e16 tpg=1 nss=1e10 kp=80u vmax=8e4 xj=0.2u \
     kappa=0.3 tnom=20 delvto=0.05)",
];

#[test]
fn level_nine_equals_level_three_bit_for_bit_on_a_shared_card() {
    // mos9temp.c and mos9load.c are mos3temp.c and mos3load.c: on a card
    // both levels accept (no NLEV/GDSNOI, effective sizes >= 1 um) the bias
    // point, its Jacobian and the small-signal capacitances are identical.
    for body in SHARED {
        let (nine, three) = (circuit(body), circuit(&level(body, 3)));
        let (b9, b3) = (bias(&nine), bias(&three));
        assert_eq!(b9.values.as_slice(), b3.values.as_slice(), "{body}");
        let context = ModelContext::default();
        let s9 = nine.small_signal_system(&context, &b9.values).unwrap();
        let s3 = three.small_signal_system(&context, &b3.values).unwrap();
        assert_eq!(s9.a.triplets(), s3.a.triplets(), "{body}");
        assert_eq!(s9.e.triplets(), s3.e.triplets(), "{body}");
    }
    // A transient inverter too (Meyer charge, junction charge, limiting).
    let inverter = "vdd vdd 0 3.3\nvin in 0 pulse(0 3.3 0.2n 0.2n 0.2n 1n 3n)\n\
         mn out in 0 0 nch w=4u l=1.1u ad=4p as=4p pd=10u ps=10u\n\
         mp out in vdd vdd pch w=10u l=1.1u ad=10p as=10p pd=22u ps=22u\ncl out 0 20f\n\
         .model nch nmos(level=9 vto=0.65 tox=9n nsub=3e16 xj=0.15u uo=450 vmax=1.5e5 nfs=5e11 \
         cgso=0.3n cgdo=0.3n cj=0.9m cjsw=0.25n)\n\
         .model pch pmos(level=9 vto=-0.75 tox=9n nsub=2.5e16 uo=180 kappa=0.4 cgso=0.3n \
         cgdo=0.3n cj=1m rd=30 rs=30)\n.tran 0.05n 3n";
    let (t9, t3) = (run(inverter).unwrap(), run(&level(inverter, 3)).unwrap());
    assert_eq!(t9[0].points, t3[0].points);
}

#[test]
fn series_resistance_creates_c_named_internal_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm w=10u l=2u nrs=3\n\
         .model mm nmos(level=9 vto=0.8 rd=50 rsh=20)",
    );
    // mos9set.c: CKTmkVolt(..., "internal#drain"/"internal#source").
    for name in ["m1#internal#drain", "m1#internal#source"] {
        let node = c.nodes().get(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(c.nodes().node(node).unwrap().kind, NodeKind::Internal);
    }
    let solved = bias(&c);
    let drain = solved.values.as_slice()[row(&c, "m1#internal#drain")];
    let source = solved.values.as_slice()[row(&c, "m1#internal#source")];
    assert!(drain < 3. && source > 0.);
    close((3. - drain) / 50., source / 60., 1e-6, 1e-12);
    // mos9trun.c: only the gate charges control the timestep.
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    assert_eq!(mos.truncation_slots(), vec![4, 6, 8]);
}

#[test]
fn dc_jacobian_matches_finite_differences_where_c_is_exact() {
    // The first and third shared cards bias where mos3load.c's (and so
    // mos9load.c's) derivatives are exact (see the `devices::mos3` unit
    // tests). The last is KAPPA = 0 with VMAX and NSUB beyond vdsat, where
    // mos9load.c divides 0/0 for the channel-length derivatives; the port's
    // guarded derivative is the exact zero, so the Jacobian stays finite.
    // Beyond vdsat C's own VMAX derivatives are approximate (a few percent;
    // the `devices::mos3` tests bound them), so that card only checks the
    // Jacobian is finite and close.
    // The absolute bound is the central difference's roundoff: residual rows
    // carry terms near 0.1 A (series conductance times node voltage), so
    // eps * 0.1 / (2 h) is about 1e-11 against sub-uA channel currents.
    let kappa_zero = "vd c 0 3\nvg g 0 2\nvs e 0 0\nvb bulk 0 -1\nm1 c g e bulk mm w=10u l=1.5u\n\
         .model mm nmos(level=9 vto=0.7 tox=20n nsub=1e16 vmax=1e5 theta=0.1 kappa=0 rd=20)";
    for (body, relative) in [(SHARED[0], 1e-6), (SHARED[2], 1e-6), (kappa_zero, 0.1)] {
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
                let analytic = entry(&system.a, r, col);
                assert!(analytic.is_finite(), "{body}");
                close(numerical, analytic, relative, 2e-11);
            }
        }
    }
}

#[test]
fn effective_sizes_below_one_micron_are_refused_as_in_mos9temp() {
    // mos9temp.c: L - 2 LD + XL < 1e-6 or W - 2 WD + XW < 1e-6 is
    // E_PARMVAL; level 3 accepts any positive effective size.
    for card in [
        "m1 d g 0 0 mm l=0.9u\n.model mm nmos(level=9)",
        "m1 d g 0 0 mm l=1.1u\n.model mm nmos(level=9 ld=0.1u xl=-0.05u)",
        "m1 d g 0 0 mm w=1.2u\n.model mm nmos(level=9 wd=0.1u xw=-0.05u)",
    ] {
        let error = Circuit::from_netlist(&deck(card)).unwrap_err();
        assert!(error.to_string().contains("mos9temp.c"), "{error}");
        assert!(
            Circuit::from_netlist(&deck(&level(card, 3))).is_ok(),
            "{card}"
        );
    }
    assert!(
        Circuit::from_netlist(&deck(
            "m1 d g 0 0 mm l=1.1u w=1.1u\n.model mm nmos(level=9)"
        ))
        .is_ok()
    );
    // Instance sweeps re-run the check.
    let c = circuit("vd d 0 1\nm1 d d 0 0 mm w=2u l=2u\n.model mm nmos(level=9)");
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    let context = ModelContext::default();
    assert!(mos.with_instance_parameter("l", 0.8e-6, &context).is_err());
    assert!(mos.with_instance_parameter("w", 0.8e-6, &context).is_err());
    assert!(mos.with_instance_parameter("l", 1.5e-6, &context).is_ok());
}

#[test]
fn noise_uses_the_pre_nlev_spice3_generators() {
    // mos9noi.c is mos3noi.c's ngbehavior=s3 branch with NLEV < 3: without
    // instance temperatures the two levels give the same spectra.
    let stage = "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1\nm1 out in 0 0 mm w=10u l=2u\nr1 vdd out 10k\n\
         .model mm nmos(level=9 vto=0.8 kp=1e-4 kf=1e-24 af=1.2 rd=20 rs=10 nsub=1e16)\n\
         .noise v(out) v1 dec 2 1k 100k 1";
    let nine = run(stage).unwrap();
    let three = run(&format!(
        "{}\n.control\nset ngbehavior=s3\n.endc",
        level(stage, 3)
    ))
    .unwrap();
    assert_eq!(nine.len(), three.len());
    for (a, b) in nine.iter().zip(&three) {
        assert_eq!(a.variables, b.variables);
        assert_eq!(a.points, b.points);
    }
    for name in [
        "onoise_m1_rd",
        "onoise_m1_rs",
        "onoise_m1_id",
        "onoise_m1_1overf",
    ] {
        assert!(nine[0].value(name, 0).unwrap().re > 0., "{name}");
    }
    // Without the compatibility mode level 3 uses its NLEV = 2 flicker law.
    let modern = run(&level(stage, 3)).unwrap();
    assert_ne!(modern[0].points, nine[0].points);
}

#[test]
fn unported_and_invalid_mos9_inputs_fail_explicitly() {
    for body in [
        // mos9.c has no NLEV/GDSNOI setters (mos3.c gained them later).
        "m1 d g 0 0 mm\n.model mm nmos(level=9 nlev=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 gdsnoi=2)",
        // MOS9mPTable lists XD, ALPHA and INPUT_DELTA, but MOS9mParam has no
        // setter for them (E_BADPARM).
        "m1 d g 0 0 mm\n.model mm nmos(level=9 xd=1u)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 alpha=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 input_delta=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 lambda=0.02)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 tox=0)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 nsub=1e9)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 phi=0)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 mj=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=9 tpg=0.5)",
    ] {
        assert!(Circuit::from_netlist(&deck(body)).is_err(), "{body}");
    }
    let message = Circuit::from_netlist(&deck("m1 d g 0 0 mm\n.model mm nmos(level=9 nsub=1e9)"))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("MOS9") && message.contains("mos9temp.c"),
        "{message}"
    );
    // .disto (mos9dset.c) and .sens are not ported: explicit errors, never
    // a silently linear or missing device.
    let stage = "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1 distof1 0.1\nm1 out in 0 0 mm w=10u l=2u\n\
                 r1 vdd out 10k\n.model mm nmos(level=9 vto=0.8 kp=1e-4)";
    let error = run(&format!("{stage}\n.disto dec 2 1k 10k")).unwrap_err();
    assert!(
        matches!(&error, SpiceError::NotYetPorted { c_reference, .. } if c_reference.contains("mos9dset.c")),
        "{error}"
    );
    assert!(run(&format!("{stage}\n.sens v(out)")).is_err());
}

#[test]
fn m10_mos9_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in ["m10_mos9_dc", "m10_mos9_ac", "m10_mos9_tran"] {
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
