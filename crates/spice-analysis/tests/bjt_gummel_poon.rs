//! Gummel-Poon BJT (#87): independent equations, Newton/companion Jacobians,
//! small-signal operators, charge conservation and explicit unported physics.
//! C-golden comparisons of the `m7_bjt_*` decks live in `xtask golden verify`.
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use spice_core::{AnalysisKind, NodeKind};
use spice_devices::{
    ACCEPTED_DEPTH, AnalysisMode, Circuit, Forcing, Limit, LoadRequest, ModelContext, StateHistory,
    TransientTiming,
};
use spice_maths::integrator::{Coefficients, DEFAULT_XMU, IntegrationMethod, StepHistory};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

const VT: f64 = (1.38064852e-23 / 1.6021766208e-19) * 300.15;

fn netlist(body: &str) -> spice_netlist::ast::Netlist {
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
        let p = run(&mut c, AnalysisKind::OperatingPoint, &[]);
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
        spice_analysis::bias::solve_dc(c, context, &Default::default(), &[], None, None).unwrap();
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

/// The Newton matrix of every load is the exact Jacobian of its residual,
/// in DC and with companion charges (including the `qbe(vbc)` cross term).
#[test]
fn newton_and_companion_jacobians_are_exact() {
    let c = circuit(FULL);
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
        "q1 c b 0 qm\n.model qm npn(kf=1e-16)",
        "q1 c b 0 qm\n.model qm npn(vbe_max=5)",
    ] {
        let error = Circuit::from_netlist(&netlist(body)).unwrap_err();
        assert!(error.is_not_yet_ported(), "{body}: {error}");
    }
    // OFF and the IC vector are ported (#99, bjtload.c/bjtgetic.c).
    for body in [
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
                    .join(format!("../../conformance/netlists/{name}.cir")),
            )
            .unwrap();
        let text = spice_netlist::write_netlist(&n).unwrap();
        let round = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("round.cir"), &text))
            .unwrap();
        assert!(spice_netlist::semantic_eq(&n, &round));
        assert_eq!(text, spice_netlist::write_netlist(&round).unwrap());
    }
}
