//! Gummel-Poon BJT (#87): independent equations, Newton/companion Jacobians,
//! small-signal operators, charge conservation and explicit unported physics.
//! C-golden comparisons of the `m7_bjt_*` decks live in `xtask golden verify`.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::{
    ACCEPTED_DEPTH, AnalysisMode, Circuit, Forcing, Limit, LoadRequest, ModelContext, StateHistory,
    TransientTiming,
};
use ngspice_rs::maths::integrator::{Coefficients, DEFAULT_XMU, IntegrationMethod, StepHistory};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::{AnalysisKind, NodeKind};
use std::path::Path;

#[path = "../conformance/reference.rs"]
mod reference;

const VT: f64 = (1.38064852e-23 / 1.6021766208e-19) * 300.15;

fn netlist(body: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("gp.cir"),
            &format!("Gummel-Poon\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&netlist(body)).unwrap()
}
fn run(c: &mut Circuit, kind: AnalysisKind, args: &[&str]) -> Plot {
    runner(kind)
        .unwrap()
        .run(
            c,
            &AnalysisRequest::with_arguments(kind, args.iter().copied()),
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
fn entry(matrix: &SparseMatrix, row: usize, col: usize) -> f64 {
    matrix
        .triplets()
        .iter()
        .filter(|t| t.row == row && t.col == col)
        .map(|t| t.value)
        .sum()
}

#[test]
fn operating_point_obeys_independent_gummel_poon_equations_and_kcl() {
    let model = "is=1e-15 bf=100 br=2 vaf=50 var=10 ikf=10m ikr=1m ise=1e-14 ne=1.5 isc=1e-14 nc=2 rb=100 rbm=10 re=2 rc=5";
    for (family, pol) in [("npn", 1.), ("pnp", -1.)] {
        // SUBS=1 makes both devices vertical so that the PNP mirrors the NPN.
        let mut c = circuit(&format!(
            "vc c 0 {}\nvb b 0 {}\nve e 0 0\nq1 c b e qm\n.model qm {family}({model} subs=1)",
            pol * 3.,
            pol * 0.78
        ));
        // Tight Newton tolerances: ngspice's defaults (reltol 1e-3) stop
        // well short of the 1e-7 agreement checked here.
        let p = run(
            &mut c,
            AnalysisKind::OperatingPoint,
            &["rtol=1e-12", "vntol=1e-12"],
        );
        let v = |name: &str| pol * p.value(&format!("v({name})"), 0).unwrap().re;
        let (vb, vc, ve) = (v("q1#base"), v("q1#collcx"), v("q1#emitter"));
        let (vbe, vbc) = (vb - ve, vb - vc);
        let gmin = 1e-12;
        let junction = |v: f64, n: f64, is: f64| is * ((v / (n * VT)).exp() - 1.);
        let cbe = junction(vbe, 1., 1e-15);
        let cbc = junction(vbc, 1., 1e-15);
        let cben = junction(vbe, 1.5, 1e-14) + gmin * vbe;
        let cbcn = junction(vbc, 2., 1e-14) + gmin * vbc;
        let q1 = 1. / (1. - vbc / 50. - vbe / 10.);
        let qb = q1 * (1. + (1. + 4. * (cbe / 10e-3 + cbc / 1e-3)).sqrt()) / 2.;
        // The vertical substrate junction is gmin from ground to the internal collector.
        let ic = (cbe - cbc) / qb - cbc / 2. - cbcn + gmin * vc;
        let ib = cbe / 100. + cben + cbc / 2. + cbcn;
        close(-pol * p.value("i(vc)", 0).unwrap().re, ic, 1e-7, 1e-15);
        close(-pol * p.value("i(vb)", 0).unwrap().re, ib, 1e-7, 1e-15);
        // Series resistances: RC, RE and the qb-modulated base resistance.
        close(ic, (3. - vc) / 5., 1e-7, 1e-15);
        close(ib + ic - gmin * vc, ve / 2., 1e-7, 1e-15);
        close(ib, (0.78 - vb) / (10. + 90. / qb), 1e-7, 1e-15);
        assert!(qb > 1.05, "high injection and Early effect act: qb = {qb}");
    }
}

#[test]
fn internal_nodes_follow_bjtsetup_and_stay_internal() {
    let c = circuit("vc c 0 1\nq1 c b e qm\nrb b 0 1k\nre e 0 1k\n.model qm npn(rb=10 re=1 rc=2)");
    let names: Vec<_> = c
        .nodes()
        .nodes()
        .iter()
        .filter(|n| n.kind == NodeKind::Internal)
        .map(|n| n.name.to_ascii_lowercase())
        .collect();
    assert_eq!(names, ["q1#collcx", "q1#base", "q1#emitter"]);
    let q = c.devices().iter().find(|d| d.name() == "q1").unwrap();
    assert_eq!(q.terminals().len(), 6);
    // Without series resistances no internal node is created.
    let plain = circuit("vc c 0 1\nq1 c b 0 qm\nrb b 0 1k\n.model qm npn(vaf=50)");
    assert!(
        plain
            .nodes()
            .nodes()
            .iter()
            .all(|n| n.kind != NodeKind::Internal)
    );
}

/// The full Gummel-Poon device: every slice, a substrate terminal and charges.
const FULL: &str = "vc c 0 2.5\nvb b 0 0.8\nve e 0 0.05\nvs s 0 -1\nq1 c b e s qm area=1.5 m=2\n.model qm npn(is=1e-15 bf=120 br=3 vaf=40 var=9 ikf=8m ikr=1m nkf=0.6 ise=1e-14 ne=1.6 isc=1e-14 nc=1.8 rb=200 rbm=20 irb=0.1m re=1.5 rc=12 cje=3p mje=0.4 cjc=2p mjc=0.45 xcjc=0.5 cjs=1.5p mjs=0.3 iss=1e-17 tf=0.3n xtf=5 vtf=2 itf=30m tr=20n fc=0.7)";

fn solved(c: &Circuit, context: &ModelContext) -> (Vector, StateHistory) {
    let solved =
        ngspice_rs::analysis::bias::solve_dc(c, context, &Default::default(), &[], None, None)
            .unwrap();
    let mut history = c.state_history();
    for _ in 0..ACCEPTED_DEPTH {
        history.commit(solved.trial.clone()).unwrap();
    }
    (solved.values, history)
}

fn load(
    c: &Circuit,
    x: &Vector,
    history: &StateHistory,
    context: &ModelContext,
    coefficients: Option<&Coefficients>,
) -> (SparseMatrix, Vector, Vec<f64>) {
    let n = c.unknown_count();
    let mut a = SparseMatrix::new(n, n);
    let mut b = Vector::zeros(n);
    let mut trial = history.trial();
    let (mode, forcing) = match coefficients {
        Some(k) => (
            AnalysisMode::Transient {
                time: k.dt(),
                dt: k.dt(),
            },
            Some(Forcing {
                limit: Limit::Right,
                timing: TransientTiming::new(k.dt(), 10. * k.dt()).unwrap(),
            }),
        ),
        None => (AnalysisMode::OperatingPoint, None),
    };
    c.load(
        &LoadRequest {
            mode,
            solution: x,
            model_context: context,
            integration: coefficients,
            history,
            forcing,
        },
        &mut a,
        &mut b,
        &mut trial,
    )
    .unwrap();
    (a, b, trial.values().to_vec())
}

fn residual(a: &SparseMatrix, b: &Vector, x: &Vector) -> Vec<f64> {
    let mut r: Vec<f64> = b.as_slice().iter().map(|v| -v).collect();
    for t in a.triplets() {
        r[t.row] += t.value * x.as_slice()[t.col];
    }
    r
}

/// The Newton matrix of every load (`bjtload.c`'s) is the exact Jacobian of
/// its residual, in DC and with companion charges (including the
/// `qbe(vbc)` cross term), when the base resistance is bias independent:
/// like C, the load stamps `gx` alone and omits `d(gx)/dV`.
#[test]
fn newton_and_companion_jacobians_are_exact() {
    let c = circuit(&FULL.replace("rb=200 rbm=20 irb=0.1m", "rb=200"));
    for temperature in [27., 90.] {
        let context = ModelContext::new(temperature, 27.);
        let (bias, history) = solved(&c, &context);
        // Move off the solution so every branch carries a nonzero residual.
        let mut x = bias.clone();
        for (i, value) in x.as_mut_slice().iter_mut().enumerate() {
            *value += 0.003 * ((i % 5) as f64 - 2.);
        }
        let dt = 1e-9;
        let coefficients = StepHistory::new()
            .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
            .unwrap();
        for k in [None, Some(&coefficients)] {
            let (a, _, _) = load(&c, &x, &history, &context, k);
            let n = c.unknown_count();
            for col in 0..n {
                let h = 1e-6;
                let mut low = x.clone();
                let mut high = x.clone();
                low.as_mut_slice()[col] -= h;
                high.as_mut_slice()[col] += h;
                let (al, bl, _) = load(&c, &low, &history, &context, k);
                let (ah, bh, _) = load(&c, &high, &history, &context, k);
                let rl = residual(&al, &bl, &low);
                let rh = residual(&ah, &bh, &high);
                for row in 0..n {
                    let numerical = (rh[row] - rl[row]) / (2. * h);
                    let analytic = entry(&a, row, col);
                    close(numerical, analytic, 2e-5, 1e-9);
                }
            }
        }
    }
}

/// AC operators: the conductance matrix is the Newton Jacobian (here with a
/// bias-independent RB, where bjtacld.c's gx-only stamp is exact) and the
/// charge matrix is the companion Jacobian divided by `ag0`.
#[test]
fn small_signal_operators_are_the_dc_and_charge_jacobians() {
    let body = FULL.replace("rbm=20 irb=0.1m ", "rbm=200 ");
    let c = circuit(&body);
    let context = ModelContext::new(50., 27.);
    let (bias, history) = solved(&c, &context);
    let system = c.small_signal_system(&context, &bias).unwrap();
    let dt = 1e-9;
    let coefficients = StepHistory::new()
        .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
        .unwrap();
    let (dc, _, _) = load(&c, &bias, &history, &context, None);
    let (tran, _, _) = load(&c, &bias, &history, &context, Some(&coefficients));
    let ag0 = coefficients.ag()[0];
    let n = c.unknown_count();
    // Linear sources/resistors are part of both A operators; the capacitive
    // operator holds only the BJT charges here.
    let mut nonzero = 0;
    for row in 0..n {
        for col in 0..n {
            let g = entry(&dc, row, col);
            close(entry(&system.a, row, col), g, 1e-9, 1e-18);
            let charge = (entry(&tran, row, col) - g) / ag0;
            let e = entry(&system.e, row, col);
            close(e, charge, 1e-6, 1e-24);
            if e != 0. {
                nonzero += 1;
            }
        }
    }
    assert!(nonzero >= 12, "{nonzero}");
}

#[test]
fn charges_are_disposable_and_integrate_actual_nonlinear_q() {
    let c = circuit(FULL);
    let context = ModelContext::default();
    let (bias, history) = solved(&c, &context);
    let before = history.clone();
    let mut x = bias;
    for (node, step) in [("b", 0.01), ("q1#base", 0.01), ("q1#collcx", -0.02)] {
        let row = c.unknowns().node_row(c.nodes().get(node).unwrap()).unwrap();
        x.as_mut_slice()[row] += step;
    }
    let dt = 1e-9;
    let coefficients = StepHistory::new()
        .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
        .unwrap();
    let (_, _, first) = load(&c, &x, &history, &context, Some(&coefficients));
    let (_, _, repeat) = load(&c, &x, &history, &context, Some(&coefficients));
    assert_eq!(first, repeat);
    assert_eq!(history, before);
    let (index, device) = c
        .devices()
        .iter()
        .enumerate()
        .find(|(_, d)| d.name() == "q1")
        .unwrap();
    // bjttrunc.c: qbe, qbc, qsub, and qbx because XCJC < 1.
    assert_eq!(device.truncation_slots(), [0, 2, 4, 6]);
    let base = c.state_rows(index).unwrap().start;
    let mut moved = 0;
    for slot in device.truncation_slots() {
        let q = first[base + slot];
        let old = history.accepted(1).unwrap()[base + slot];
        close(first[base + slot + 1], (q - old) / dt, 1e-12, 1e-15);
        if q != old {
            moved += 1;
        }
    }
    assert!(moved >= 3, "{moved}");
}

#[test]
fn transient_terminal_currents_conserve_charge() {
    let mut c = circuit(
        "vc c 0 pulse(2 0.3 5n 2n 2n 10n 40n)\nvb b 0 pulse(0.6 0.85 2n 1n 1n 15n 40n)\nve e 0 0\nvs s 0 -1\nq1 c b e s qm\n.model qm npn(is=1e-15 bf=120 br=3 vaf=40 ikf=8m ise=1e-14 isc=1e-14 rb=200 rbm=20 irb=0.1m re=1.5 rc=12 cje=3p cjc=2p xcjc=0.5 cjs=1.5p iss=1e-17 tf=0.3n xtf=5 vtf=2 itf=30m tr=20n)",
    );
    let p = run(&mut c, AnalysisKind::Transient, &["0.5n", "40n"]);
    let currents = |row: usize| -> Vec<f64> {
        ["i(vc)", "i(vb)", "i(ve)", "i(vs)"]
            .iter()
            .map(|name| p.value(name, row).unwrap().re)
            .collect()
    };
    let peak = (0..p.point_count())
        .flat_map(currents)
        .fold(0., |m: f64, v| m.max(v.abs()));
    assert!(peak > 1e-3, "{peak}");
    // Every BJT current and charge flows between two of its nodes, so the
    // terminal currents (displacement currents included) sum to zero.
    for row in 0..p.point_count() {
        close(currents(row).iter().sum::<f64>(), 0., 0., 1e-12 * peak);
    }
}

#[test]
fn unported_gummel_poon_physics_is_explicit() {
    for body in [
        "q1 c b 0 qm\n.model qm npn(rco=10)",
        "q1 c b 0 qm\n.model qm npn(gamma=1e-11 vo=5)",
        "q1 c b 0 qm\n.model qm npn(tf=1n ptf=30)",
        "q1 c b 0 qm\n.model qm npn(vbe_max=5)",
    ] {
        let error = Circuit::from_netlist(&netlist(body)).unwrap_err();
        assert!(error.is_not_yet_ported(), "{body}: {error}");
    }
    // OFF and the IC vector are ported (#99, bjtload.c/bjtgetic.c), and so
    // is the flicker noise law (#100, bjtnoise.c).
    for body in [
        "q1 c b 0 qm\n.model qm npn(kf=1e-16 af=1.2)",
        "q1 c b 0 qm off\n.model qm npn",
        "q1 c b 0 qm icvbe=0.6 ic=0.7,2\n.model qm npn",
    ] {
        Circuit::from_netlist(&netlist(body)).unwrap();
    }
    // Excess phase has no effect without TF (bjttemp.c: PTF * TF).
    Circuit::from_netlist(&netlist("q1 c b 0 qm\n.model qm npn(ptf=30)")).unwrap();
    // Aliases resolve to their canonical setter (bjt.c IOPR/IOPAR).
    let mut alias = circuit("vc c 0 2\nvb b 0 0.7\nq1 c b 0 qm\n.model qm npn(va=50 ik=10m)");
    let mut canonical = circuit("vc c 0 2\nvb b 0 0.7\nq1 c b 0 qm\n.model qm npn(vaf=50 ikf=10m)");
    let a = run(&mut alias, AnalysisKind::OperatingPoint, &[]);
    let b = run(&mut canonical, AnalysisKind::OperatingPoint, &[]);
    assert_eq!(a.value("i(vc)", 0), b.value("i(vc)", 0));
    // Unknown and malformed setters remain errors.
    for body in [
        "q1 c b 0 qm\n.model qm npn(bogus=1)",
        "q1 c b 0 qm\n.model qm npn(mje=1)",
        "q1 c b 0 qm\n.model qm npn(tlev=2)",
    ] {
        assert!(Circuit::from_netlist(&netlist(body)).is_err(), "{body}");
    }
}

#[test]
fn m7_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in [
        "m7_bjt_gummel",
        "m7_bjt_output",
        "m7_bjt_temp",
        "m7_bjt_amp_ac",
        "m7_bjt_amp_tran",
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

/// A lone IBE/IBC is ignored by the initial thermal model but not discarded:
/// restoring the missing partner during `.sens` later activates the pair.
#[test]
fn single_saturation_current_setters_survive_in_sensitivity_records() {
    for (card, be, bc) in [
        ("ibe=1e-13", Some(1e-13), None),
        ("ibc=2e-14", None, Some(2e-14)),
        ("ibe=1e-13 ibc=2e-14", Some(1e-13), Some(2e-14)),
    ] {
        let c = circuit(&format!("q1 c b 0 qm\n.model qm npn {card}"));
        let replay = c
            .device("q1")
            .unwrap()
            .sensitivity(&ModelContext::default())
            .unwrap();
        let (m, _) = replay.records().unwrap();
        assert_eq!(m.given("ibe"), be.is_some(), "{card}");
        assert_eq!(m.given("ibc"), bc.is_some(), "{card}");
        assert_eq!(m.value("ibe").unwrap(), be.unwrap_or(0.));
        assert_eq!(m.value("ibc").unwrap(), bc.unwrap_or(0.));
    }
}

/// Large reference columns previously misdiagnosed as cancellation. The card
/// uses default tolerances, like the measured ngspice-47+ probe. In particular
/// VAF is stepped from zero and exposes the paired saturation-current switch.
#[test]
fn single_sided_saturation_currents_follow_cs_restore_walk() {
    for (card, pins) in [
        (
            "ibc=2e-14",
            [
                ("v(q1:ibe)", 5.433_384_999_027_637e11),
                ("v(q1:ibc)", 4.100_676_337_386_377_5),
                ("v(q1:vaf)", 0.086_011_074_718_727_68),
                ("v(q1:var)", -0.013_988_896_037_050_5),
            ],
        ),
        (
            "ibe=1e-13",
            [
                ("v(q1:ibc)", 54_283.616_822_039_774),
                ("v(q1:vaf)", 2.386_795_986_069_663e11),
                ("v(q1:ikf)", -55_264.635_827_176_47),
                ("v(q1_area)", 0.054_333_850_228_572_1),
            ],
        ),
    ] {
        let plot = sensitivity(&format!(
            "vcc 1 0 dc 5\nvids 1 2 dc 0\n\
            vb 4 0 dc 0.7\nrbb 4 3 1k\nq1 2 3 0 qn\n\
            .model qn npn level=1 {card}\n.sens i(vids)"
        ));
        for (name, expected) in pins {
            close(column(&plot, name), expected, 1e-6, 1e-9);
        }
    }
}

/// Runs the deck's own `.sens` analysis.
fn sensitivity(body: &str) -> Plot {
    let netlist = netlist(body);
    let config = ngspice_rs::analysis::RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap()
}
fn column(plot: &Plot, name: &str) -> f64 {
    let index = plot
        .variable_index(name)
        .unwrap_or_else(|| panic!("missing {name}: {:?}", plot.variables));
    plot.points[0][index].re
}

/// `.sens` over `bjt.c`'s tables (#164): 103 model setters and six instance
/// ones. The `IOPA` capacitance family is `IF_AC` (so `cje`/`tf`/`xcjc` and
/// friends appear only in an AC walk), `ic` is a vector, `off`/`sens_area` are
/// flags, the probe entries are ask-only and the `IOPR` aliases are redundant.
/// `rco` and the unported quasi-saturation/SOA parameters are still enumerated.
#[test]
fn sensitivity_covers_the_bjt_tables() {
    let plot = sensitivity(
        "vcc 1 0 dc 5\nq1 1 3 2 qn area=2\nvids 2 0 dc 0\nvb 4 0 dc 0.72\n\
         rb 4 3 1k\n.model qn npn level=1\n.sens i(vids)",
    );
    let names: Vec<&str> = plot
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .filter(|name| name.starts_with("v(q1"))
        .collect();
    assert_eq!(names.len(), 109, "{names:?}");
    for absent in ["v(q1:xcjc)", "v(q1:tf)", "v(q1:vto)", "v(q1:icvbe)"] {
        assert!(!names.contains(&absent), "{absent} in {names:?}");
    }
    for present in ["v(q1:tnom)", "v(q1:nkf)", "v(q1:rth0)", "v(q1_temp)"] {
        assert!(names.contains(&present), "{present} missing from {names:?}");
    }
}

/// `sensitivity()` refuses a card with `RC != 0`: `bjtsetup.c` then creates
/// `collCX`, and C's walk marks `VO`/`GAMMA`/`QCO` given, switching the Kull
/// quasi-saturation terms on in the perturbed loads (measured: `v(q1:gamma)`
/// is 0.652 where the port has no such physics).
#[test]
fn sensitivity_refuses_a_card_with_a_collector_resistance() {
    let netlist = netlist(
        "vcc 1 0 dc 5\nq1 1 3 2 qn\nvids 2 0 dc 0\nvb 4 0 dc 0.72\nrb 4 3 1k\n\
         .model qn npn level=1 rc=40\n.sens i(vids)",
    );
    let config = ngspice_rs::analysis::RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("RC"), "{error}");
}

/// The `sens_bjt` card: an NPN at `dtemp=-10` with `ISE > 1e-4` (rescales to
/// `IS*ISE` in `bjtsetup.c`), `rb`/`re`/`irb`/`rbm`, area factors and a `BF`
/// polynomial, plus a PNP at `dtemp=20` with `tnom=50`. The reference binary
/// prints these columns; `rco` is zero because `sens_setp()` skips it, and
/// `bf`/`vaf` are zero because C's own linearization does not see them here.
/// Use the matching C platform: the 1e-25 ISE step amplifies libm differences
/// enough to change the sign of C's own column between macOS and Linux.
#[test]
fn sensitivity_matches_the_reference_on_the_fixture_card() {
    let plot = sensitivity(
        ".options reltol=1e-10 vntol=1e-13 abstol=1e-16\n\
         vcc 1 0 dc 5\nq1 1 3 2 qn area=2 areab=3 areac=4 dtemp=-10\n\
         vids 2 0 dc 0\nvb 4 0 dc 0.72\nrb 4 3 1k\nvcc2 5 0 dc -5\n\
         q2 5 7 2 qp dtemp=20\nvb2 8 0 dc -0.68\nrb2 8 7 1k\n\
         .model qn npn level=1 ise=1e-3 rb=300 re=20 irb=1e-3 rbm=100 \
         tbf1=0.01 tbf2=1e-5\n\
         .model qp pnp level=1 tnom=50 tbr1=0.02 tbr2=2e-5\n.sens i(vids)",
    );
    let raw = ngspice_rs::analysis::RawFile::load(reference::golden_path(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "sens_bjt",
    ))
    .unwrap();
    let expected = &raw.plots[0].plot;
    assert_eq!(plot.variable_count(), expected.variable_count());
    assert_eq!(plot.point_count(), expected.point_count());
    for variable in &expected.variables {
        close(
            column(&plot, &variable.name),
            column(expected, &variable.name),
            1e-6,
            1e-9,
        );
    }
    assert_eq!(column(&plot, "v(q1:rco)"), 0., "sens_setp skips RCO");
}
