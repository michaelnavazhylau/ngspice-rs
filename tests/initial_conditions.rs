//! `.ic`, `.nodeset`, instance `ic=` and `uic` (GitHub #27, analysis half)
//! through production APIs: parsed decks, `RunConfig`, `runner` and the
//! companion transient driver. Accuracy is judged against closed-form
//! solutions; the opt-in live-C comparison is `c_initial_conditions.rs`.
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ngspice_rs::analysis::{
    AnalysisContext, AnalysisRequest, Plot, RunConfig, companion_transient, runner,
};
use ngspice_rs::devices::{
    AcceptContext, AnalysisMode, Circuit, Device, IndependentSource, LinearContext, StampContext,
    Waveform,
};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, Complex, NodeId, SpiceError, SpiceResult};

fn parse(deck: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(Path::new("ic.cir"), deck))
        .unwrap()
}

/// Runs analysis `index` of a deck exactly as the CLI path does.
fn run_at(deck: &str, index: usize) -> SpiceResult<Plot> {
    let netlist = parse(deck);
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[index])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}
fn run(deck: &str) -> SpiceResult<Plot> {
    run_at(deck, 0)
}
fn error_of(deck: &str) -> String {
    run(deck).expect_err("expected failure").to_string()
}

fn column(plot: &Plot, name: &str) -> Vec<f64> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}
fn max_error(plot: &Plot, name: &str, exact: impl Fn(f64) -> f64) -> f64 {
    column(plot, "time")
        .into_iter()
        .zip(column(plot, name))
        .map(|(t, v)| (v - exact(t)).abs())
        .fold(0., f64::max)
}

// ---------------------------------------------------------------- uic: analytic

#[test]
fn uic_capacitor_discharge_from_instance_ic_and_from_node_ic_match_the_analytic_decay() {
    // tau = RC = 1 ms; v(t) = 2 exp(-t/tau). Instance ic= and .ic v() must agree.
    let exact = |t: f64| 2. * (-t / 1e-3).exp();
    for (name, body) in [
        ("instance ic=", "r1 a 0 1k\nc1 a 0 1u ic=2"),
        ("node .ic", "r1 a 0 1k\nc1 a 0 1u\n.ic v(a)=2"),
    ] {
        let plot = run(&format!("t\n{body}\n.tran 10u 5m uic\n.end\n")).unwrap();
        let times = column(&plot, "time");
        // C writes no t = 0 row under uic (dctran.c: CKTtime > 0).
        assert!(times[0] > 0., "{name}: first row at {}", times[0]);
        assert!((times[times.len() - 1] - 5e-3).abs() < 1e-15);
        let error = max_error(&plot, "v(a)", exact);
        assert!(error < 2e-5, "{name}: trap error {error:e}");
        println!("uic discharge ({name}): max error {error:.3e} V");
    }
}

#[test]
fn instance_ic_beats_node_ic_and_nodeset_acts_as_a_node_ic_under_uic_like_c() {
    // CAPgetic: an instance ic= is never overridden by node values.
    let first = |deck: &str| {
        let plot = run(deck).unwrap();
        column(&plot, "v(a)")[0]
    };
    let base = "t\nr1 a 0 1k\nc1 a 0 1u ic=2\n.ic v(a)=1\n.tran 1u 1m uic\n.end\n";
    assert!((first(base) - 2.).abs() < 1e-2);
    // Without instance ic=, .ic is used; a .nodeset is used only when no .ic
    // is given for that node (CKTic copies nodesets first, .ic overrides).
    let node = |cards: &str| format!("t\nr1 a 0 1k\nc1 a 0 1u\n{cards}\n.tran 1u 1m uic\n.end\n");
    assert!((first(&node(".ic v(a)=1")) - 1.).abs() < 1e-2);
    assert!((first(&node(".nodeset v(a)=0.7")) - 0.7).abs() < 1e-2);
    assert!((first(&node(".nodeset v(a)=0.7\n.ic v(a)=0.4")) - 0.4).abs() < 1e-2);
    // Neither given: zero initial voltage; with nothing driving it stays 0.
    assert_eq!(first(&node("")), 0.);
}

#[test]
fn uic_rl_current_matches_the_analytic_decay_and_has_the_branch_current_sign() {
    // v1 = 1 V, R = 10, L = 10 mH: i(t) = 0.1 + (i0 - 0.1) exp(-t/tau), tau = 1 ms.
    for (terminals, label) in [("a 0", "a to ground"), ("0 a", "ground to a")] {
        let i0 = 0.5;
        let plot = run(&format!(
            "t\nv1 in 0 1\nr1 in a 10\nl1 {terminals} 10m ic={i0}\n.tran 10u 5m uic\n.end\n"
        ));
        let plot = match plot {
            Ok(plot) => plot,
            // `0 a` forces the current into the V1/R1 path the other way round;
            // it is still a legal circuit: the inductor current opposes the
            // source, so only the sign convention is under test here.
            Err(error) => panic!("{label}: {error}"),
        };
        // Current is positive from the first terminal to the second.
        let current = column(&plot, "i(l1)");
        assert!((current[0] - i0).abs() < 1e-3, "{label}: {}", current[0]);
        if terminals == "a 0" {
            let error = max_error(&plot, "i(l1)", |t| 0.1 + (i0 - 0.1) * (-t / 1e-3).exp());
            assert!(error < 5e-6, "{label}: error {error:e}");
            println!("uic RL: max current error {error:.3e} A");
            // Positive current from a down to ground: v(a) = L di/dt drop. At
            // t = 0+ the loop equation gives v(a) = 1 - 10 * 0.5 = -4 V.
            let v = column(&plot, "v(a)");
            assert!((v[0] + 4.).abs() < 5e-2, "v(a) = {}", v[0]);
        } else {
            // i flows ground -> a: R carries -0.5 A from a toward in.
            let v = column(&plot, "v(a)");
            assert!(
                v[0] > 1.9,
                "reversed orientation flips the node voltage: {}",
                v[0]
            );
        }
    }
}

#[test]
fn uic_series_rlc_with_inductor_and_capacitor_ic_matches_the_analytic_response() {
    // L = 1 mH, R = 10, C = 1 uF, a tied to ground by a 0 V source.
    let (r, l, c) = (10_f64, 1e-3_f64, 1e-6_f64);
    let (v0, i0) = (1.5_f64, 0.04_f64);
    let alpha = r / (2. * l);
    let wd = (1. / (l * c) - alpha * alpha).sqrt();
    let b = (i0 / c + alpha * v0) / wd;
    let vc = |t: f64| (-alpha * t).exp() * (v0 * (wd * t).cos() + b * (wd * t).sin());
    let current = |t: f64| {
        let dv = (-alpha * t).exp()
            * ((-alpha * v0 + b * wd) * (wd * t).cos() + (-alpha * b - wd * v0) * (wd * t).sin());
        c * dv
    };
    let deck = "t\nv1 a 0 0\nl1 a b 1m ic=0.04\nr1 b out 10\nc1 out 0 1u ic=1.5\n\
                .tran 0.5u 400u uic\n.end\n";
    let plot = run(deck).unwrap();
    let verror = max_error(&plot, "v(out)", vc);
    let ierror = max_error(&plot, "i(l1)", current);
    println!("uic RLC: v(out) error {verror:.3e} V, i(l1) error {ierror:.3e} A");
    assert!(verror < 3e-4, "{verror:e}");
    assert!(ierror < 1e-5, "{ierror:e}");
    // Gear-2 solves the same problem to a comparable accuracy.
    let gear = run(&deck.replace(".tran", ".options method=gear\n.tran")).unwrap();
    assert!(max_error(&gear, "v(out)", vc) < 1e-2);
}

// ---------------------------------------------------- .ic without uic (DC bias)

#[test]
fn ic_without_uic_is_enforced_during_the_bias_then_released() {
    // v(out) is held at 0.25 V for the t = 0 point, then relaxes to 1 V:
    // v(out) = 1 - 0.75 exp(-t/tau). The source keeps v(in) = 1 V and the
    // resistor current at t = 0 is the exact (1 - 0.25)/1k = 0.75 mA.
    let deck = "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u\n.ic v(out)=0.25\n.tran 10u 5m\n.end\n";
    let plot = run(deck).unwrap();
    let times = column(&plot, "time");
    assert_eq!(times[0], 0.);
    assert_eq!(column(&plot, "v(out)")[0], 0.25);
    assert_eq!(column(&plot, "v(in)")[0], 1.);
    assert!((column(&plot, "i(v1)")[0] + 0.75e-3).abs() < 1e-15);
    let error = max_error(&plot, "v(out)", |t| 1. - 0.75 * (-t / 1e-3).exp());
    assert!(error < 1e-5, "{error:e}");
    println!("bias .ic: max error {error:.3e} V");
    // Instance ic= is ignored without uic (capload.c uses it only for
    // UIC && INITTRAN), and no .ic means the plain DC point (v(out) = 1).
    let ignored = "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u ic=0.9\n.tran 10u 5m\n.end\n";
    assert_eq!(column(&run(ignored).unwrap(), "v(out)")[0], 1.);
}

#[test]
fn ic_duplicates_use_the_last_entry_and_node_names_are_case_insensitive() {
    let deck = "t\nv1 in 0 1\nr1 in OUT 1k\nc1 out 0 1u\n.ic v(out)=0.2\n.ic V(OUT)=0.4 v(out)=0.6\n.tran 10u 1m\n.end\n";
    assert_eq!(column(&run(deck).unwrap(), "v(out)")[0], 0.6);
}

#[test]
fn ic_on_a_node_pinned_by_a_source_must_agree_with_it() {
    let body = "v1 in 0 1\nr1 in out 1k\nc1 out 0 1u\n";
    // A consistent .ic on the source node is redundant: the source wins and
    // the branch current stays exact (C returns 0 A there via its 1e10 hack).
    let ok = run(&format!(
        "t\n{body}.ic v(in)=1 v(out)=0.25\n.tran 10u 1m\n.end\n"
    ))
    .unwrap();
    assert_eq!(column(&ok, "v(out)")[0], 0.25);
    assert!((column(&ok, "i(v1)")[0] + 0.75e-3).abs() < 1e-15);
    // A contradicting one is an explicit error naming the entry, never a mess.
    let error = error_of(&format!("t\n{body}.ic v(in)=0\n.tran 10u 1m\n.end\n"));
    assert!(
        error.contains("contradicts") && error.contains("V(in)"),
        "{error}"
    );
    assert!(error.contains("ic.cir:5"), "{error}");
    // An inductor is a DC short: it pins its node to the other terminal.
    let error = error_of("t\nv1 in 0 1\nr1 in a 1k\nl1 a 0 1m\n.ic v(a)=1\n.tran 1u 1m\n.end\n");
    assert!(error.contains("contradicts"), "{error}");
    // Nodes tied by a floating source are rigid together.
    let floating = "t\nv1 a b 2\nr1 a 0 1k\nr2 b 0 1k\nc1 a 0 1u\n";
    let ok = run(&format!(
        "{floating}.ic v(a)=1 v(b)=-1\n.tran 10u 1m\n.end\n"
    ))
    .unwrap();
    assert_eq!(column(&ok, "v(a)")[0], 1.);
    assert_eq!(column(&ok, "v(b)")[0], -1.);
    let error = error_of(&format!(
        "{floating}.ic v(a)=1 v(b)=0\n.tran 10u 1m\n.end\n"
    ));
    assert!(
        error.contains("contradicts") && error.contains("V(b)"),
        "{error}"
    );
}

#[test]
fn ic_can_define_the_bias_of_a_node_that_has_no_dc_path() {
    // c1/c2 in series form a floating node m: no DC operating point without
    // an IC (an explicit error), and the IC fixes the charge on m for the run.
    let body = "t\nv1 in 0 1\nc1 in m 1u\nc2 m 0 1u\n";
    assert!(run(&format!("{body}.tran 10u 1m\n.end\n")).is_err());
    let plot = run(&format!("{body}.ic v(m)=0.3\n.tran 10u 1m\n.end\n")).unwrap();
    for v in column(&plot, "v(m)") {
        assert!((v - 0.3).abs() < 1e-12, "{v}");
    }
}

// ------------------------------------------------------------------- .nodeset

#[test]
fn nodeset_never_changes_a_linear_result_and_differs_from_ic() {
    let body = "v1 in 0 1\nr1 in out 1k\nr2 out 0 3k\nc1 out 0 1u\n";
    let plain = run(&format!("t\n{body}.tran 20u 4m\n.end\n")).unwrap();
    let hinted = run(&format!(
        "t\n{body}.nodeset v(out)=0.1 v(in)=7\n.tran 20u 4m\n.end\n"
    ))
    .unwrap();
    assert_eq!(plain, hinted, "a nodeset is a convergence hint only");
    let pinned = run(&format!("t\n{body}.ic v(out)=0.1\n.tran 20u 4m\n.end\n")).unwrap();
    assert_eq!(column(&pinned, "v(out)")[0], 0.1);
    assert_eq!(column(&plain, "v(out)")[0], 0.75);
    // .op, .dc and .ac accept and validate both cards; C ignores .ic there and
    // a nodeset cannot move a linear operating point.
    let deck =
        |cards: &str| format!("t\n{body}{cards}\n.op\n.dc v1 0 1 0.5\n.ac dec 2 1 100\n.end\n");
    let reference = deck("");
    let with = deck(".ic v(out)=0.1\n.nodeset v(out)=0.2");
    for index in 0..3 {
        // The AC deck needs an AC magnitude; operating point and sweep only.
        if index == 2 {
            continue;
        }
        assert_eq!(
            run_at(&reference, index).unwrap(),
            run_at(&with, index).unwrap()
        );
    }
    let ac = |cards: &str| {
        format!(
            "t\nv1 in 0 1 ac 1\nr1 in out 1k\nr2 out 0 3k\nc1 out 0 1u\n{cards}\n.ac dec 2 1 100\n.end\n"
        )
    };
    assert_eq!(
        run(&ac("")).unwrap(),
        run(&ac(".ic v(out)=0.1\n.nodeset v(out)=0.2")).unwrap()
    );
}

#[test]
fn hints_naming_unknown_or_ground_nodes_are_explicit_errors_in_every_analysis() {
    let body = "v1 in 0 1 ac 1\nr1 in out 1k\nc1 out 0 1u\n";
    for card in [".ic v(nosuch)=1", ".nodeset v(nosuch)=1"] {
        for analysis in [
            ".op",
            ".dc v1 0 1 0.5",
            ".ac dec 2 1 100",
            ".tran 10u 1m",
            ".tran 10u 1m uic",
        ] {
            let error = error_of(&format!("t\n{body}{card}\n{analysis}\n.end\n"));
            assert!(
                error.contains("nosuch") && error.contains("does not exist"),
                "{card} {analysis}: {error}"
            );
            assert!(error.contains("ic.cir:"), "{error}");
        }
    }
    // Ground is rejected by the parser already.
    assert!(
        Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("g.cir"),
                "t\nr1 a 0 1k\n.ic v(0)=1\n.tran 1u 1m\n.end\n"
            ))
            .is_err()
    );
}

// --------------------------------------------------------------- inconsistency

/// Replaces `v1` (device 0) by an ideal step from 0 to 5 V at `time`.
fn step_source(circuit: &mut Circuit, time: f64) {
    let t = circuit.devices()[0].terminals().to_vec();
    circuit.devices_mut()[0] = Box::new(
        IndependentSource::new(
            "v1",
            [t[0], t[1]],
            true,
            0.,
            Complex::real(1.),
            Waveform::Step {
                before: 0.,
                after: 5.,
                time,
            },
        )
        .unwrap(),
    );
}

fn uic_request(args: &[&str]) -> AnalysisRequest {
    let mut request =
        AnalysisRequest::with_arguments(AnalysisKind::Transient, args.iter().copied());
    request.uic = true;
    request
}
fn circuit_of(deck: &str) -> Circuit {
    Circuit::from_netlist(&parse(deck)).unwrap()
}

#[test]
fn uic_requiring_an_impulse_is_rejected_and_consistent_ideal_source_cases_run() {
    // A capacitor directly across an ideal source: ic must equal the source.
    let consistent =
        run("t\nv1 in 0 5\nc1 in 0 1u ic=5\nr1 in 0 1k\n.tran 10u 1m uic\n.end\n").unwrap();
    for v in column(&consistent, "v(in)") {
        assert_eq!(v, 5.);
    }
    let error = error_of("t\nv1 in 0 5\nc1 in 0 1u ic=1\nr1 in 0 1k\n.tran 10u 1m uic\n.end\n");
    assert!(error.contains("impulse") && error.contains("c1"), "{error}");
    // .ic reaches the same check through the node voltage.
    let error =
        error_of("t\nv1 in 0 5\nc1 in 0 1u\nr1 in 0 1k\n.ic v(in)=1\n.tran 10u 1m uic\n.end\n");
    assert!(error.contains("impulse"), "{error}");
    // An inductor in series with a current source: the current is forced.
    let deck = |ic: &str| format!("t\ni1 0 a 1m\nl1 a 0 1m{ic}\n.tran 1u 100u uic\n.end\n");
    assert!(run(&deck(" ic=1m")).is_ok());
    let error = error_of(&deck(" ic=2m"));
    assert!(error.contains("impulse") && error.contains("l1"), "{error}");
}

#[test]
fn uic_checks_the_source_value_just_after_t_zero() {
    // The source steps 0 -> 5 V exactly at t = 0: its right limit is 5 V, so a
    // capacitor across it starting at 0 V would need an impulse, whereas the
    // plain bias (left limit 0 V) is consistent.
    let deck = "t\nv1 in 0 0\nc1 in 0 1u ic=0\nr1 in 0 1k\n";
    let mut c = circuit_of(deck);
    step_source(&mut c, 0.);
    let error = companion_transient(
        &mut c,
        &uic_request(&["10u", "1m"]),
        &AnalysisContext::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("impulse"), "{error}");
    let mut c = circuit_of("t\nv1 in 0 0\nc1 in 0 1u ic=5\nr1 in 0 1k\n");
    step_source(&mut c, 0.);
    let (plot, _) = companion_transient(
        &mut c,
        &uic_request(&["10u", "1m"]),
        &AnalysisContext::default(),
    )
    .unwrap();
    assert_eq!(column(&plot, "v(in)")[0], 5.);
}

type Log = Rc<RefCell<Vec<Option<f64>>>>;
/// A resistive device that logs every accept hook (accepted points).
#[derive(Debug)]
struct Probe {
    terminals: [NodeId; 2],
    log: Log,
}
impl Device for Probe {
    fn name(&self) -> &str {
        "x1"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let [a, b] = self.terminals;
        let _ = matches!(context.mode, AnalysisMode::Transient { .. });
        context.stamp(a, a, 1e-3)?;
        context.stamp(b, b, 1e-3)?;
        context.stamp(a, b, -1e-3)?;
        context.stamp(b, a, -1e-3)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.terminals, 1e-3, false)
    }
    fn accept(&self, context: &AcceptContext<'_>) -> SpiceResult<()> {
        self.log.borrow_mut().push(context.time);
        Ok(())
    }
}

#[test]
fn failed_initialization_is_atomic_no_accept_hook_no_plot() {
    let log: Log = Rc::default();
    let mut c = circuit_of("t\nv1 in 0 5\nc1 in 0 1u ic=1\nr1 in 0 1k\n");
    let node = c.add_node("in");
    c.add_device(Box::new(Probe {
        terminals: [node, NodeId::GROUND],
        log: Rc::clone(&log),
    }))
    .unwrap();
    c.finalize().unwrap();
    let result = companion_transient(
        &mut c,
        &uic_request(&["10u", "1m"]),
        &AnalysisContext::default(),
    );
    assert!(
        matches!(result, Err(SpiceError::Numerical { .. })),
        "{result:?}"
    );
    assert!(log.borrow().is_empty(), "no device saw an accepted point");
    // The same circuit with a consistent ic runs afterwards (nothing leaked),
    // and the first accept hook is the t = 0 initial point.
    let mut c = circuit_of("t\nv1 in 0 5\nc1 in 0 1u ic=5\nr1 in 0 1k\n");
    let node = c.add_node("in");
    c.add_device(Box::new(Probe {
        terminals: [node, NodeId::GROUND],
        log: Rc::clone(&log),
    }))
    .unwrap();
    c.finalize().unwrap();
    companion_transient(
        &mut c,
        &uic_request(&["10u", "1m"]),
        &AnalysisContext::default(),
    )
    .unwrap();
    assert_eq!(log.borrow()[0], Some(0.));
    // An unknown node fails before any assembly as well.
    let error = error_of("t\nr1 a 0 1k\nc1 a 0 1u\n.ic v(zz)=1\n.tran 1u 1m uic\n.end\n");
    assert!(error.contains("zz"), "{error}");
}

// -------------------------------------------------------------------- backends

#[test]
fn diffsol_keeps_rejecting_ic_uic_and_instance_ic_explicitly() {
    let tran = ".tran 10u 1m backend=diffsol method=bdf";
    let base = "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u";
    let error = error_of(&format!("{base}\n.ic v(out)=0.2\n{tran}\n.end\n"));
    assert!(
        error.contains(".ic") && error.contains("companion"),
        "{error}"
    );
    assert!(error.contains("ic.cir:5"), "{error}");
    let error = error_of(&format!("{base}\n{tran} uic\n.end\n"));
    assert!(error.contains("uic"), "{error}");
    let error = error_of(&format!(
        "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u ic=0.5\n{tran}\n.end\n"
    ));
    assert!(error.contains("ic="), "{error}");
    // A nodeset cannot change the linear bias: accepted (and validated).
    let with = run(&format!("{base}\n.nodeset v(out)=0.2\n{tran}\n.end\n")).unwrap();
    let without = run(&format!("{base}\n{tran}\n.end\n")).unwrap();
    assert_eq!(with, without);
    let error = error_of(&format!("{base}\n.nodeset v(zz)=0.2\n{tran}\n.end\n"));
    assert!(error.contains("zz"), "{error}");
}

#[test]
fn uic_adds_the_print_step_breakpoint_like_dctran() {
    // dctran.c: `if (UIC) CKTsetBreak(ckt, CKTstep)` at the first timepoint, so
    // a sample lands exactly on the .tran step.
    let plot = run("t\nr1 a 0 1k\nc1 a 0 1u ic=1\n.tran 100u 1m uic\n.end\n").unwrap();
    assert!(
        column(&plot, "time")
            .iter()
            .any(|t| (t - 100e-6).abs() < 1e-15)
    );
    let plot = run("t\nr1 a 0 1k\nc1 a 0 1u\n.tran 100u 1m\n.end\n").unwrap();
    assert!(
        !column(&plot, "time")
            .iter()
            .any(|t| (t - 100e-6).abs() < 1e-15)
    );
}
