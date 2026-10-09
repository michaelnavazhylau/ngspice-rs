//! #99 nonlinear initial conditions without the C binary: `.ic` forced
//! through the nonlinear transient operating point, `uic` starts from device
//! initial conditions, `.nodeset` forced in the `MODEINITJCT`/`MODEINITFIX`
//! loads, MOS1 `ic=` start voltages and the `off` flag.
//!
//! The C-parity evidence is the `m7_ic_*` goldens (`cargo xtask golden
//! verify`); these tests pin the mechanisms and the explicit errors.
use spice_analysis::bias::{DcOutcome, DcSettings, DcStrategy, NodeForcing, solve_dc_forced};
use spice_analysis::{AnalysisRequest, Plot, RunConfig, runner};
use spice_core::{AnalysisKind, SpiceResult};
use spice_devices::{Circuit, ModelContext};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::path::Path;

/// The symmetric CMOS latch of `m7_ic_latch_*`: its default operating point
/// is the unstable equilibrium, `v(q) = v(qb)`.
const LATCH: &str = "vdd vdd 0 dc 3
mp1 q qb vdd vdd pm w=4u l=1u
mn1 q qb 0 0 nm w=2u l=1u MN1
mp2 qb q vdd vdd pm w=4u l=1u
mn2 qb q 0 0 nm w=2u l=1u MN2
.model nm nmos(vto=0.7 kp=60u lambda=0.02 gamma=0.4 phi=0.65)
.model pm pmos(vto=-0.8 kp=25u lambda=0.03 gamma=0.5 phi=0.65)";

fn parse(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("initial.cir"),
            &format!("initial\n{body}\n.end\n"),
        ))
        .unwrap()
}

/// Runs the deck's first analysis card as `spice-rs simulate` does.
fn run(body: &str) -> SpiceResult<Plot> {
    let netlist = parse(body);
    let config = RunConfig::from_netlist(&netlist)?;
    let mut circuit = config.circuit(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

fn value(plot: &Plot, name: &str, row: usize) -> f64 {
    plot.value(name, row)
        .unwrap_or_else(|| panic!("{name}[{row}]"))
        .re
}

fn latch(mn1: &str, mn2: &str, extra: &str) -> String {
    format!(
        "{}\n{extra}\n.op",
        LATCH.replace("MN1", mn1).replace("MN2", mn2)
    )
}

#[test]
fn nodesets_and_mos1_ic_vectors_select_the_latch_state() {
    let plain = run(&latch("", "", "")).unwrap();
    let (q, qb) = (value(&plain, "v(q)", 0), value(&plain, "v(qb)", 0));
    assert!((q - qb).abs() < 1e-6 && q > 1. && q < 2., "{q} {qb}");
    // cktload.c forces the nodeset rows in MODEINITJCT/MODEINITFIX only; the
    // released solve is an exact operating point in the selected state.
    for extra in [".nodeset v(q)=3 v(qb)=0", ".nodeset v(q)=2.5"] {
        let hinted = run(&latch("", "", extra)).unwrap();
        assert!(value(&hinted, "v(q)", 0) > 2.99, "{extra}");
        assert!(value(&hinted, "v(qb)", 0) < 1e-6, "{extra}");
    }
    // The other side of the equilibrium falls the other way.
    let low = run(&latch("", "", ".nodeset v(q)=1")).unwrap();
    assert!(value(&low, "v(q)", 0) < 1e-6);
    // mos1load.c starts MODEINITJCT at a nonzero IC vector even without uic.
    let started = run(&latch("ic=3,0,0", "ic=0,3,0", "")).unwrap();
    assert!(value(&started, "v(q)", 0) > 2.99);
    assert!(value(&started, "v(qb)", 0) < 1e-6);
    // An all-zero IC vector keeps the default start (and the equilibrium).
    let zero = run(&latch("ic=0,0,0", "", "")).unwrap();
    assert!((value(&zero, "v(q)", 0) - q).abs() < 1e-9);
}

#[test]
fn nodesets_on_source_fixed_nodes_are_dropped_and_ignored_by_linear_circuits() {
    // The hint on vdd conflicts with its source; it is dropped, not forced.
    let hinted = run(&latch("", "", ".nodeset v(vdd)=1 v(q)=3")).unwrap();
    assert_eq!(value(&hinted, "v(vdd)", 0), 3.);
    assert!(value(&hinted, "v(q)", 0) > 2.99);
    // A linear operating point is unique and bit-identical.
    let divider = "v1 a 0 2\nr1 a b 1k\nr2 b 0 3k\n";
    let plain = run(&format!("{divider}.op")).unwrap();
    let hinted = run(&format!("{divider}.nodeset v(b)=7\n.op")).unwrap();
    assert_eq!(value(&plain, "v(b)", 0), value(&hinted, "v(b)", 0));
}

#[test]
fn forced_ic_rows_are_exact_and_validated() {
    let netlist = parse("v1 a 0 dc 10\nr1 a b 1k\nd1 b 0 dm\nr2 b 0 1k\n.model dm d");
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let row = circuit
        .unknowns()
        .node_row(circuit.nodes().get("b").unwrap())
        .unwrap();
    let settings = DcSettings::default();
    let forced = |initial: Vec<(usize, f64)>| {
        solve_dc_forced(
            &circuit,
            &ModelContext::default(),
            &settings,
            &[],
            None,
            None,
            &NodeForcing {
                initial,
                nodesets: Vec::new(),
            },
        )
    };
    // An .ic row is exact at the returned point; the source current balances
    // the rest of the circuit, not the hidden constraint.
    let solved = forced(vec![(row, 0.25)]).unwrap();
    assert_eq!(solved.solution.values.as_slice()[row], 0.25);
    assert!(forced(vec![(99, 0.25)]).is_err());
    assert!(forced(vec![(row, f64::NAN)]).is_err());
}

#[test]
fn ic_is_forced_through_the_nonlinear_transient_operating_point_and_released() {
    // Diode and capacitor: without .ic the junction holds the node near
    // 0.65 V; .ic v(a)=0.2 fixes the t = 0 row, then the capacitor charges.
    let deck = "v1 in 0 dc 5\nr1 in a 10k\nc1 a 0 10n\nd1 a 0 dm\n.model dm d(cjo=5p)\n";
    let plain = run(&format!("{deck}.tran 1u 200u")).unwrap();
    let constrained = run(&format!("{deck}.ic v(a)=0.2\n.tran 1u 200u")).unwrap();
    assert!(value(&plain, "v(a)", 0) > 0.5);
    assert_eq!(value(&constrained, "time", 0), 0.);
    assert_eq!(value(&constrained, "v(a)", 0), 0.2);
    // At t = 0 the source current is the resistor's: KCL at `a` is replaced.
    assert!((value(&constrained, "i(v1)", 0) + 4.8 / 10e3).abs() < 1e-9);
    let last = constrained.point_count() - 1;
    assert!((value(&constrained, "v(a)", last) - value(&plain, "v(a)", last)).abs() < 1e-3);
    // An .ic on a source node that disagrees is an explicit error.
    let error = run(&format!("{deck}.ic v(in)=4\n.tran 1u 200u")).unwrap_err();
    assert!(error.to_string().contains(".ic V(in)=4"), "{error}");
}

#[test]
fn uic_starts_junctions_from_the_node_vector_and_ignores_diode_ic_like_c() {
    let deck = |diode: &str, ic: &str| {
        format!(
            "c1 a 0 10n ic=2\nd1 a b dm {diode}\nr1 b 0 1k\nc2 b 0 1n\n{ic}\n\
             .model dm d(is=1e-14 rs=10 cjo=20p tt=50n)\n.tran 0.1u 2u uic"
        )
    };
    let base = run(&deck("", ".ic v(b)=0.5")).unwrap();
    // C writes no t = 0 row under uic.
    assert!(value(&base, "time", 0) > 0.);
    // dioparam.c never sets DIOinitCondGiven: diogetic.c always replaces the
    // diode's own ic= with the node difference, so it has no effect.
    let with_ic = run(&deck("ic=0.4", ".ic v(b)=0.5")).unwrap();
    assert_eq!(base.point_count(), with_ic.point_count());
    for row in 0..base.point_count() {
        assert_eq!(value(&base, "v(b)", row), value(&with_ic, "v(b)", row));
    }
    // The node vector does matter: the junction charge starts elsewhere.
    let other = run(&deck("", ".ic v(b)=0")).unwrap();
    assert!((value(&other, "v(b)", 0) - value(&base, "v(b)", 0)).abs() > 1e-3);
}

#[test]
fn uic_starts_bjt_and_mos1_from_their_ic_vectors() {
    let bjt = |ic: &str| {
        format!(
            "vcc vcc 0 5\nrc vcc c 1k\nrb vcc b 100k\nq1 c b 0 qm {ic}\n\
             .model qm npn(cje=5p cjc=2p tf=1n)\n.tran 1n 20n uic"
        )
    };
    let given = run(&bjt("ic=0.7,0.2")).unwrap();
    let partial = run(&bjt("icvbe=0.7")).unwrap();
    let defaulted = run(&bjt("")).unwrap();
    // Different start charges, different first points; the defaults come
    // from the (all-zero) node vector, which an explicit zero IC reproduces.
    assert!((value(&given, "v(c)", 0) - value(&defaulted, "v(c)", 0)).abs() > 1e-6);
    assert!((value(&given, "v(c)", 0) - value(&partial, "v(c)", 0)).abs() > 1e-9);
    let zero = run(&bjt("ic=0,0")).unwrap();
    assert_eq!(value(&zero, "v(c)", 0), value(&defaulted, "v(c)", 0));

    let mos = |ic: &str| {
        format!(
            "vdd vdd 0 3\nrd vdd d 10k\nvg g 0 1.5\nm1 d g 0 0 nm w=2u l=1u {ic}\ncd d 0 10f\n\
             .model nm nmos(vto=0.7 kp=60u tox=20n cj=0.2m cgso=0.2n cgdo=0.2n)\n\
             .tran 1n 20n uic"
        )
    };
    let started = run(&mos("ic=3,1.5,0")).unwrap();
    let defaulted = run(&mos("")).unwrap();
    assert!((value(&started, "v(d)", 0) - value(&defaulted, "v(d)", 0)).abs() > 1e-6);
    // OFF wins over the IC vector in mos1load.c's MODEINITJCT branch.
    let off = run(&mos("ic=3,1.5,0 off")).unwrap();
    let off_zero = run(&mos("ic=0,0,0 off")).unwrap();
    assert_eq!(value(&off, "v(d)", 0), value(&off_zero, "v(d)", 0));
}

#[test]
fn off_follows_cs_held_fix_phase_and_convergence_test() {
    // A forward-biased OFF diode: held at 0 V through MODEINITFIX, its
    // DIOconvTest keeps failing, the direct iteration exhausts itl1 and
    // dynamic gmin stepping returns the physical operating point.
    let netlist = parse("v1 a 0 5\nr1 a b 1k\nd1 b 0 dm OFF\n.model dm d");
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let solved = solve_dc_forced(
        &circuit,
        &ModelContext::default(),
        &DcSettings::default(),
        &[],
        None,
        None,
        &NodeForcing::default(),
    )
    .unwrap();
    assert_eq!(
        solved.report.outcome,
        DcOutcome::Converged(DcStrategy::GminStepping)
    );
    let b = solved.solution.values.as_slice()[1];
    assert!((b - 0.6929).abs() < 1e-3, "{b}");
    // An OFF device whose voltages stay near zero converges directly.
    let netlist = parse("v1 a 0 -5\nr1 a b 1k\nd1 b 0 dm OFF\nr2 b 0 1\n.model dm d");
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let solved = solve_dc_forced(
        &circuit,
        &ModelContext::default(),
        &DcSettings::default(),
        &[],
        None,
        None,
        &NodeForcing::default(),
    )
    .unwrap();
    assert_eq!(
        solved.report.outcome,
        DcOutcome::Converged(DcStrategy::Direct)
    );
}

#[test]
fn start_settings_need_device_limiting() {
    for (mn1, mn2) in [("off", ""), ("", "ic=1,2,0")] {
        let netlist = parse(&latch(mn1, mn2, ""));
        let mut circuit = Circuit::from_netlist(&netlist).unwrap();
        let request =
            AnalysisRequest::with_arguments(AnalysisKind::OperatingPoint, ["limiting=global"]);
        let error = runner(AnalysisKind::OperatingPoint)
            .unwrap()
            .run(&mut circuit, &request, &Default::default())
            .unwrap_err();
        assert!(error.to_string().contains("limiting=global"), "{error}");
    }
}

#[test]
fn the_bdf_backend_still_rejects_nonlinear_initial_conditions() {
    let deck = "v1 a 0 1\nr1 a b 1k\nd1 b 0 dm\nc1 b 0 1n\n.model dm d\n";
    for tail in [
        ".tran 1u 10u uic backend=diffsol method=bdf",
        ".ic v(b)=0.1\n.tran 1u 10u backend=diffsol method=bdf",
    ] {
        assert!(run(&format!("{deck}{tail}")).is_err(), "{tail}");
    }
}
