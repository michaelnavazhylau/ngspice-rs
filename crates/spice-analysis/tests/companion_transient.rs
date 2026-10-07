//! The adaptive trap/Gear-2 companion transient driver (#26) through production
//! APIs: parsed decks, `RunConfig`, `runner` and `companion_transient`.
//!
//! Accuracy is judged against closed-form solutions, never against the step
//! sequence of another simulator.
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use spice_analysis::{
    AnalysisContext, AnalysisRequest, Plot, RunConfig, TransientStats, companion_transient, runner,
};
use spice_core::{AnalysisKind, NodeId, SpiceError, SpiceResult};
use spice_devices::{
    AcceptContext, AnalysisMode, Circuit, Device, IndependentSource, LinearContext, StampContext,
    Waveform,
};
use spice_netlist::{Parser, source::parse_deck_text};

fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&netlist(body)).unwrap()
}
fn netlist(body: &str) -> spice_netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("tran.cir"),
            &format!("t\n{body}\n.end\n"),
        ))
        .unwrap()
}
/// Replaces `v1` (device 0) by an ideal step from 0 to 1 V at `time`.
fn step_at(c: &mut Circuit, time: f64) {
    let t = c.devices()[0].terminals().to_vec();
    c.devices_mut()[0] = Box::new(
        IndependentSource::new(
            "v1",
            [t[0], t[1]],
            true,
            0.,
            spice_core::Complex::real(1.),
            Waveform::Step {
                before: 0.,
                after: 1.,
                time,
            },
        )
        .unwrap(),
    );
}
fn request(args: &[&str]) -> AnalysisRequest {
    AnalysisRequest::with_arguments(AnalysisKind::Transient, args.iter().copied())
}
fn run(c: &mut Circuit, args: &[&str]) -> SpiceResult<(Plot, TransientStats)> {
    companion_transient(c, &request(args), &AnalysisContext::default())
}
fn times(p: &Plot) -> Vec<f64> {
    (0..p.point_count())
        .map(|i| p.value("time", i).unwrap().re)
        .collect()
}
/// Largest `|signal - exact(t)|` over every accepted point.
fn max_error(p: &Plot, signal: &str, exact: impl Fn(f64) -> f64) -> f64 {
    (0..p.point_count())
        .map(|i| {
            let t = p.value("time", i).unwrap().re;
            (p.value(signal, i).unwrap().re - exact(t)).abs()
        })
        .fold(0., f64::max)
}
fn error_text(result: SpiceResult<(Plot, TransientStats)>) -> String {
    result.expect_err("expected failure").to_string()
}

const TAU: f64 = 1e-3;
/// RC low-pass (tau = 1 ms) and RL (tau = L/R = 1 ms), unit step at 1 ms.
const RC: &str = "v1 in 0 0\nr1 in out 1k\nc1 out 0 1u";
const RL: &str = "v1 in 0 0\nr1 in out 1k\nl1 out 0 1";
/// Series RLC: alpha = R/2L = 5000 /s, w0 = 31623 rad/s (zeta = 0.158).
const RLC: &str = "v1 in 0 0\nr1 in a 10\nl1 a out 1m\nc1 out 0 1u";

fn rc_step(t: f64) -> f64 {
    if t <= 1e-3 {
        0.
    } else {
        1. - (-(t - 1e-3) / TAU).exp()
    }
}
fn rl_step_current(t: f64) -> f64 {
    rc_step(t) / 1e3
}
fn rlc_step(t: f64) -> f64 {
    let alpha = 5000_f64;
    // w0^2 = 1/(L C) = 1e9; damped frequency wd = sqrt(w0^2 - alpha^2).
    let wd = (1e9_f64 - alpha * alpha).sqrt();
    let s = t - 1e-3;
    if s <= 0. {
        0.
    } else {
        1. - (-alpha * s).exp() * ((wd * s).cos() + alpha / wd * (wd * s).sin())
    }
}

#[test]
fn rc_rl_and_rlc_steps_match_the_analytic_response_for_both_methods() {
    // (deck, signal, exact, max step for the coarsest run, error limits by method)
    type Case = (&'static str, &'static str, fn(f64) -> f64, f64, f64, f64);
    let cases: [Case; 3] = [
        (RC, "v(out)", rc_step, 1e-4, 4e-4, 1.5e-3),
        (RL, "i(l1)", rl_step_current, 1e-4, 4e-7, 1.5e-6),
        (RLC, "v(out)", rlc_step, 4e-6, 6e-3, 2e-2),
    ];
    for (body, signal, exact, h, trap_limit, gear_limit) in cases {
        for (method, limit) in [("trap", trap_limit), ("gear", gear_limit)] {
            let mut errors = Vec::new();
            for refinement in 0..3 {
                let h = h / f64::from(1_u32 << refinement);
                let mut c = circuit(body);
                step_at(&mut c, 1e-3);
                let (hs, tmax) = (format!("{h:e}"), format!("{h:e}"));
                // rtol = 1 disables truncation limiting so tmax alone sets the
                // step: a clean convergence-order measurement.
                let (p, stats) = run(
                    &mut c,
                    &[&hs, "3m", "0", &tmax, &format!("method={method}"), "rtol=1"],
                )
                .unwrap();
                assert!(stats.max_step <= h * (1. + 1e-12), "{stats:?}");
                errors.push(max_error(&p, signal, exact));
            }
            assert!(errors[0] < limit, "{body} {method}: {errors:?} vs {limit}");
            for pair in errors.windows(2) {
                let ratio = pair[0] / pair[1];
                assert!(
                    (3.4..4.6).contains(&ratio),
                    "{body} {method}: order-2 error ratio {ratio} in {errors:?}"
                );
            }
        }
    }
}

#[test]
fn tightening_reltol_shrinks_the_error_under_truncation_control() {
    for method in ["trap", "gear"] {
        let mut previous = f64::INFINITY;
        let mut steps = 0;
        for rtol in ["1e-3", "1e-5", "1e-7"] {
            let mut c = circuit(RC);
            step_at(&mut c, 1e-3);
            let (p, stats) = run(
                &mut c,
                &[
                    "2m",
                    "6m",
                    "0",
                    "2m",
                    &format!("method={method}"),
                    &format!("rtol={rtol}"),
                ],
            )
            .unwrap();
            let error = max_error(&p, "v(out)", rc_step);
            assert!(error < previous / 5., "{method} rtol={rtol}: {error:e}");
            assert!(stats.accepted > steps, "{stats:?}");
            previous = error;
            steps = stats.accepted;
        }
        assert!(previous < 2e-4, "{method}: {previous:e}");
    }
}

/// RC response to `PULSE(0 1 td tr tf pw per)` as a sum of unit-slope ramp
/// responses `r(t) = t - tau (1 - exp(-t/tau))` at every corner.
fn pulse_rc(t: f64, td: f64, tr: f64, pw: f64, tf: f64, per: f64) -> f64 {
    let ramp = |t: f64| {
        if t <= 0. {
            0.
        } else {
            t - TAU * (1. - (-t / TAU).exp())
        }
    };
    let mut v = 0.;
    let mut start = td;
    while start < t {
        v += (ramp(t - start) - ramp(t - start - tr)) / tr
            - (ramp(t - start - tr - pw) - ramp(t - start - tr - pw - tf)) / tf;
        start += per;
    }
    v
}

#[test]
fn pulse_driven_rc_matches_analytic_lands_on_every_corner_and_refines() {
    let deck = "v1 in 0 pulse(0 1 1m 0.2m 0.1m 2m 5m)\nr1 in out 1k\nc1 out 0 1u";
    let exact = |t: f64| pulse_rc(t, 1e-3, 2e-4, 2e-3, 1e-4, 5e-3);
    // Corners of cycles starting at 1 ms and 6 ms (and 11 ms), inside (0, 12 ms].
    let corners: Vec<f64> = [1e-3, 6e-3, 11e-3]
        .iter()
        .flat_map(|s| [*s, s + 2e-4, s + 2.2e-3, s + 2.3e-3])
        .filter(|t| *t < 12e-3)
        .collect();
    for (method, limit) in [("trap", 5e-4), ("gear", 2e-3)] {
        let mut errors = Vec::new();
        for h in ["0.1m", "0.05m", "0.025m"] {
            let mut c = circuit(deck);
            let (p, stats) = run(
                &mut c,
                &[h, "12m", "0", h, &format!("method={method}"), "rtol=1"],
            )
            .unwrap();
            let ts = times(&p);
            for corner in &corners {
                assert!(
                    ts.iter().any(|t| (t - corner).abs() <= 1e-15),
                    "{method}: no sample on corner {corner:e}"
                );
            }
            assert_eq!(*ts.last().unwrap(), 12e-3);
            assert_eq!(stats.breakpoints, corners.len() + 1, "corners and stop");
            assert!(ts.windows(2).all(|w| w[1] > w[0]), "strictly increasing");
            errors.push(max_error(&p, "v(out)", exact));
        }
        assert!(errors[0] < limit, "{method}: {errors:?}");
        // Order 2 away from the corners; each corner restarts with a short
        // backward-Euler step, whose small error does not shrink with tmax and
        // so bends the last halving of the less accurate Gear-2 formula.
        assert!(
            (3.0..4.8).contains(&(errors[0] / errors[1])),
            "{method}: {errors:?}"
        );
        assert!(errors[1] / errors[2] > 2.2, "{method}: {errors:?}");
        assert!(errors[0] / errors[2] > 7., "{method}: {errors:?}");
    }
}

#[test]
fn parsed_rc_transient_fixture_runs_through_an_ordinary_tran() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/netlists");
    let netlist = Parser::new()
        .parse_file(root.join("rc_transient.cir"))
        .unwrap();
    // No backend=, method= or tolerance: the card alone selects the driver.
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    assert!(request.named("backend").is_none() && request.named("method").is_none());
    let mut c = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap();
    let ts = times(&plot);
    assert_eq!(ts[0], 0.);
    // `5u` parses to 4.9999999999999996e-6; the run ends exactly on it.
    assert_eq!(
        *ts.last().unwrap(),
        spice_core::parse_spice_number("5u").unwrap()
    );
    assert!(ts.contains(&1e-9), "rise corner is a sample");
    assert!(ts.windows(2).all(|w| w[1] > w[0]));
    assert!(max_error(&plot, "v(out)", exact_us) < 2e-3);
    // maxstep = min(tstep, tstop/50) = 0.1 us.
    assert!(ts.windows(2).all(|w| w[1] - w[0] <= 1e-7 * (1. + 1e-9)));
    assert_eq!(plot.value("v(in)", 0).unwrap().re, 0.);
    assert!(plot.variable_index("i(v1)").is_some());
}

/// `PULSE(0 5 0 1n 1n 10u 20u)` into 1k/1n (tau = 1 us), first period.
fn exact_us(t: f64) -> f64 {
    const TAU_US: f64 = 1e-6;
    let (tr, v) = (1e-9, 5.);
    let ramp = |t: f64| {
        if t <= 0. {
            0.
        } else {
            t - TAU_US * (1. - (-t / TAU_US).exp())
        }
    };
    v * (ramp(t) - ramp(t - tr)) / tr
}

#[test]
fn backend_and_method_dispatch_is_explicit() {
    let body = "v1 in 0 1\nr1 in out 1k\nc1 out 0 1u";
    let base = ["0.5m", "3m"];
    let ok = |extra: &[&str]| {
        let mut c = circuit(body);
        let mut args = base.to_vec();
        args.extend(extra);
        runner(AnalysisKind::Transient).unwrap().run(
            &mut c,
            &request(&args),
            &AnalysisContext::default(),
        )
    };
    // Ordinary .tran defaults to the companion backend and trapezoidal rule.
    let default = ok(&[]).unwrap();
    let trap = ok(&["method=trap"]).unwrap();
    assert_eq!(times(&default), times(&trap));
    assert_eq!(ok(&["method=TRAPEZOIDAL"]).unwrap(), trap);
    assert_eq!(ok(&["backend=companion"]).unwrap(), trap);
    // Gear order 2 is a different, selectable integrator.
    let gear = ok(&["method=gear"]).unwrap();
    assert_ne!(gear, trap);
    assert_eq!(ok(&["method=gear", "maxord=2"]).unwrap(), gear);
    // diffsol BDF remains explicit.
    let bdf = ok(&["backend=diffsol", "method=bdf"]).unwrap();
    assert_ne!(times(&bdf), times(&trap));
    for extra in [
        &["backend=spice"][..],
        &["backend=diffsol"][..],
        &["backend=diffsol", "method=gear"][..],
        &["method=bdf"][..],
        &["method=euler"][..],
        &["method=gear", "maxord=3"][..],
        &["method=trap", "maxord=6"][..],
        &["maxord=0"][..],
        &["maxord=two"][..],
        &["method=gear", "method=trap"][..],
        &["bogus=1"][..],
        &["rtol=0"][..],
        &["vntol=-1"][..],
        &["trtol=0"][..],
        &["maxsteps=0"][..],
        &["maxsteps=lots"][..],
        &["uic"][..],
    ] {
        let result = ok(extra);
        assert!(result.is_err(), "{extra:?} must be rejected");
    }
    for args in [
        &["1m"][..],
        &["0", "1m"][..],
        &["1m", "0"][..],
        &["1m", "1m", "1m"][..],
        &["1m", "1m", "-1m"][..],
        &["1m", "1m", "0", "-1"][..],
        &["1m", "1m", "0", "1", "1"][..],
    ] {
        let mut c = circuit(body);
        assert!(
            companion_transient(&mut c, &request(args), &AnalysisContext::default()).is_err(),
            "{args:?}"
        );
    }
}

#[test]
fn maxord_one_is_backward_euler_and_first_order() {
    let run_h = |method: &str, h: f64| {
        let mut c = circuit(RC);
        step_at(&mut c, 1e-3);
        let hs = format!("{h:e}");
        let (p, _) = run(
            &mut c,
            &[
                &hs,
                "3m",
                "0",
                &hs,
                &format!("method={method}"),
                "maxord=1",
                "rtol=1",
            ],
        )
        .unwrap();
        max_error(&p, "v(out)", rc_step)
    };
    // Trapezoidal and Gear both degrade to the same order-1 formula.
    let (coarse, fine) = (run_h("trap", 1e-4), run_h("trap", 5e-5));
    assert_eq!(coarse, run_h("gear", 1e-4));
    assert!(coarse > 1e-2, "backward Euler error {coarse:e}");
    assert!((1.8..2.2).contains(&(coarse / fine)), "{coarse:e} {fine:e}");
}

#[test]
fn truncation_error_rejects_and_shrinks_steps_without_advancing_history() {
    // A probe device counts accepted state vectors: if a rejected trial ever
    // advanced the history, the counter written by a later trial would skip.
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut c = circuit(RC);
    step_at(&mut c, 1e-3);
    add_probe(&mut c, &log, f64::INFINITY);
    let (plot, stats) = run(&mut c, &["2m", "6m", "0", "2m", "rtol=1e-4"]).unwrap();
    assert!(
        stats.rejected >= 1,
        "scenario must force a rejection: {stats:?}"
    );
    let log = log.borrow();
    // DC point plus every accepted step, nothing else.
    assert_eq!(log.len(), stats.accepted + 1);
    assert_eq!(plot.point_count(), stats.accepted + 1);
    let mut last = f64::NEG_INFINITY;
    for (index, (time, counter)) in log.iter().enumerate() {
        let time = time.unwrap();
        assert!(time > last || index == 0, "accept times must increase");
        last = time;
        assert_eq!(*counter, Some(index as f64), "history advanced out of step");
    }
    assert_eq!(log[0].0, Some(0.));
    assert_eq!(last, 6e-3);
    // The accepted times are exactly the plot's times.
    let accepted: Vec<f64> = log.iter().map(|(t, _)| t.unwrap()).collect();
    assert_eq!(accepted, times(&plot));
}

#[test]
fn an_accept_callback_failure_aborts_the_run() {
    for (fail_after, expect_points_before) in [(2e-3, true), (-1., false)] {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut c = circuit(RC);
        step_at(&mut c, 1e-3);
        add_probe(&mut c, &log, fail_after);
        let error = error_text(run(&mut c, &["0.1m", "6m"]));
        assert!(error.contains("probe refused"), "{error}");
        let log = log.borrow();
        if expect_points_before {
            // Hooks ran for earlier accepted points only; the failing point
            // was never logged, and the run did not report success.
            assert!(!log.is_empty());
            assert!(log.iter().all(|(t, _)| t.unwrap() <= 2e-3));
        } else {
            assert!(log.is_empty(), "failure at the DC point");
        }
    }
}

#[test]
fn minimum_step_and_work_limit_failures_are_explicit() {
    let mut c = circuit(RC);
    step_at(&mut c, 1e-3);
    // An absurdly strict truncation factor forces the step to delmin and below.
    let error = error_text(run(&mut c, &["0.1m", "3m", "trtol=1e-12"]));
    assert!(error.contains("timestep too small"), "{error}");
    let error = error_text(run(&mut c, &["0.1m", "6m", "maxsteps=20"]));
    assert!(error.contains("work limit of 20"), "{error}");
    // Accepted + rejected steps both count against the budget.
    let (_, stats) = run(&mut c, &["0.1m", "6m", "maxsteps=1000"]).unwrap();
    assert!(stats.accepted + stats.rejected <= 1000);
    // A very fast periodic source cannot expand without bound.
    let mut c = circuit("v1 in 0 pulse(0 1 0 0.1n 0.1n 0.3n 1n)\nr1 in out 1k\nc1 out 0 1n");
    let error = error_text(run(&mut c, &["1m", "10m", "maxsteps=5000"]));
    assert!(error.contains("limit"), "{error}");
}

#[test]
fn tstart_suppresses_earlier_output_only() {
    let mut full = circuit(RC);
    step_at(&mut full, 1e-3);
    // An explicit tmax keeps the default step limit (which depends on tstart) equal.
    let (all, _) = run(&mut full, &["0.1m", "6m", "0", "0.1m"]).unwrap();
    let mut late = circuit(RC);
    step_at(&mut late, 1e-3);
    let (tail, _) = run(&mut late, &["0.1m", "6m", "2m", "0.1m"]).unwrap();
    let want: Vec<f64> = times(&all).into_iter().filter(|t| *t >= 2e-3).collect();
    assert_eq!(times(&tail), want);
    assert!(times(&tail)[0] >= 2e-3);
    // The simulation itself still ran from t = 0: identical samples afterwards.
    let offset = all.point_count() - tail.point_count();
    for i in 0..tail.point_count() {
        assert_eq!(
            tail.value("v(out)", i).unwrap(),
            all.value("v(out)", offset + i).unwrap()
        );
    }
}

#[test]
fn initial_conditions_floating_nodes_and_bad_decks_are_explicit_errors() {
    let mut c = circuit("v1 in 0 0\nr1 in out 1k\nc1 out 0 1u ic=1");
    assert!(error_text(run(&mut c, &["1u", "1m"])).contains("ic"));
    let mut c = circuit(RC);
    let mut uic = request(&["1u", "1m"]);
    uic.uic = true;
    let error = companion_transient(&mut c, &uic, &AnalysisContext::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("uic"), "{error}");
    // A floating capacitor has no DC operating point.
    let mut c = circuit("c1 a 0 1u");
    assert!(run(&mut c, &["1u", "1m"]).is_err());
    // `.ic` cards stay rejected by the configuration layer.
    let n = netlist("v1 a 0 0\nr1 a b 1k\nc1 b 0 1u\n.ic v(b)=1\n.tran 1u 1m");
    let config = RunConfig::from_netlist(&n).unwrap();
    assert!(config.request_for(&n.analyses[0]).is_err());
}

#[test]
fn deck_options_reach_the_companion_driver() {
    let deck = |options: &str| {
        netlist(&format!(
            "v1 in 0 pulse(0 1 1m 0.2m 0.1m 2m 5m)\nr1 in out 1k\nc1 out 0 1u\n{options}\n.tran 0.1m 6m"
        ))
    };
    let run_deck = |options: &str| {
        let n = deck(options);
        let config = RunConfig::from_netlist(&n).unwrap();
        let request = config.request_for(&n.analyses[0]).unwrap();
        let mut c = config.circuit(&n).unwrap();
        companion_transient(&mut c, &request, &config.context()).unwrap()
    };
    let (default, default_stats) = run_deck("");
    let (gear, _) = run_deck(".options method=gear");
    assert_ne!(gear, default);
    let (tight, tight_stats) = run_deck(".options reltol=1e-6 abstol=1e-15 trtol=1");
    assert!(tight_stats.accepted > default_stats.accepted);
    let exact = |t: f64| pulse_rc(t, 1e-3, 2e-4, 2e-3, 1e-4, 5e-3);
    assert!(max_error(&tight, "v(out)", exact) < max_error(&default, "v(out)", exact));
    let (be, _) = run_deck(".options maxord=1");
    assert!(max_error(&be, "v(out)", exact) > max_error(&default, "v(out)", exact));
}

#[test]
fn sources_and_defaults_work_on_resistive_and_dc_circuits() {
    // PULSE defaults (TR = TF = tstep, PW = PER = tstop) resolve from .tran.
    let mut c = circuit("v1 in 0 pulse(0 1)\nr1 in 0 1k");
    let (p, _) = run(&mut c, &["1m", "10m"]).unwrap();
    let ts = times(&p);
    assert!(
        ts.contains(&1e-3),
        "end of the default rise is a breakpoint"
    );
    for (i, t) in ts.iter().enumerate() {
        let want = (t / 1e-3).min(1.);
        assert!(
            (p.value("v(in)", i).unwrap().re - want).abs() < 1e-12,
            "t={t}"
        );
    }
    // A purely DC deck takes the largest allowed steps and stays at its bias.
    let mut c = circuit("v1 in 0 5\nr1 in out 1k\nc1 out 0 1u\nr2 out 0 1k");
    let (p, stats) = run(&mut c, &["1m", "10m"]).unwrap();
    for i in 0..p.point_count() {
        assert!((p.value("v(out)", i).unwrap().re - 2.5).abs() < 1e-12);
    }
    assert!(stats.rejected == 0 && stats.max_step <= 1e-3 * (1. + 1e-12));
    // A current source into R||L: the inductor carries all of it at DC.
    let mut c = circuit("i1 0 a 1m\nr1 a 0 1k\nl1 a 0 1");
    let (p, _) = run(&mut c, &["0.1m", "5m"]).unwrap();
    for i in 0..p.point_count() {
        assert!((p.value("i(l1)", i).unwrap().re - 1e-3).abs() < 1e-15);
    }
}

/// One extra state slot and a trivial conductance; logs every accept hook.
type Log = Rc<RefCell<Vec<(Option<f64>, Option<f64>)>>>;
#[derive(Debug)]
struct Probe {
    terminals: [NodeId; 2],
    log: Log,
    fail_after: f64,
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
    fn state_count(&self) -> usize {
        1
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let counter = match context.mode {
            AnalysisMode::Transient { .. } => context.states.accepted(1, 0).unwrap() + 1.,
            _ => 0.,
        };
        context.states.set(0, counter)?;
        let [a, b] = self.terminals;
        context.stamp(a, a, 1e-3)?;
        context.stamp(b, b, 1e-3)?;
        context.stamp(a, b, -1e-3)?;
        context.stamp(b, a, -1e-3)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.terminals, 1e-3, false)
    }
    fn accept(&self, context: &AcceptContext<'_>) -> SpiceResult<()> {
        if context.time.is_some_and(|t| t > self.fail_after) {
            return Err(SpiceError::circuit("probe refused"));
        }
        self.log
            .borrow_mut()
            .push((context.time, context.states.map(|s| s[0])));
        Ok(())
    }
}
fn add_probe(c: &mut Circuit, log: &Log, fail_after: f64) {
    let out = c.add_node("out");
    c.add_device(Box::new(Probe {
        terminals: [out, NodeId::GROUND],
        log: Rc::clone(log),
        fail_after,
    }))
    .unwrap();
    c.finalize().unwrap();
}

#[test]
fn a_step_in_the_forcing_uses_the_left_limit_until_the_breakpoint_then_the_right() {
    let mut c = circuit(RC);
    step_at(&mut c, 1e-3);
    let (p, _) = run(&mut c, &["0.1m", "3m"]).unwrap();
    let ts = times(&p);
    let jump = ts
        .iter()
        .position(|t| *t == 1e-3)
        .expect("sample on the jump");
    // The step ending on the jump sees v(in) = 0 (left limit); there is no
    // second sample at the same instant (C emits none either) and the next
    // step, which starts at the jump, sees the new level.
    assert_eq!(p.value("v(in)", jump).unwrap().re, 0.);
    assert_eq!(p.value("v(out)", jump).unwrap().re, 0.);
    assert_eq!(ts.iter().filter(|t| **t == 1e-3).count(), 1);
    assert_eq!(p.value("v(in)", jump + 1).unwrap().re, 1.);
    // After the restart the first step is backward Euler and short.
    assert!(ts[jump + 1] - ts[jump] <= 0.1 * 1e-4 * (1. + 1e-9));
    assert!(ts[jump] - ts[jump - 1] > 0.);
}

/// `x1 out 0` draws `k v^3` through a Newton linearization; it contributes
/// nothing to the (linear) DC bias at 0 V.
#[derive(Debug)]
struct Cubic {
    terminals: [NodeId; 2],
    k: f64,
}
impl Device for Cubic {
    fn name(&self) -> &str {
        "x1"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let [a, b] = self.terminals;
        let v = context.node_voltage(a) - context.node_voltage(b);
        let (i, g) = (self.k * v * v * v, 3. * self.k * v * v);
        context.stamp(a, a, g)?;
        context.stamp(b, b, g)?;
        context.stamp(a, b, -g)?;
        context.stamp(b, a, -g)?;
        context.stamp_rhs(a, g * v - i)?;
        context.stamp_rhs(b, i - g * v)
    }
    fn assemble_linear(&self, _context: &mut LinearContext<'_>) -> SpiceResult<()> {
        Ok(())
    }
}

/// Never converges: every load flips the sign of a 1 A injection.
#[derive(Debug)]
struct Oscillator {
    terminals: [NodeId; 2],
    flips: std::cell::Cell<bool>,
}
impl Device for Oscillator {
    fn name(&self) -> &str {
        "x2"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let sign = if self.flips.replace(!self.flips.get()) {
            1.
        } else {
            -1.
        };
        context.stamp_rhs(self.terminals[0], sign)
    }
    fn assemble_linear(&self, _context: &mut LinearContext<'_>) -> SpiceResult<()> {
        Ok(())
    }
}

#[test]
fn nonlinear_trials_iterate_to_convergence_and_non_convergence_shrinks_the_step() {
    // C dv/dt = (V - v)/R - k v^3, integrated with fine RK4 as the reference.
    let (r, cap, k) = (1e3, 1e-6, 1e-3);
    let rhs = |v: f64, source: f64| ((source - v) / r - k * v * v * v) / cap;
    let reference = |t: f64| {
        let mut v = 0.;
        let (n, now) = (20_000_usize, 1e-3);
        if t <= now {
            return 0.;
        }
        let h = (t - now) / n as f64;
        for _ in 0..n {
            let k1 = rhs(v, 1.);
            let k2 = rhs(v + 0.5 * h * k1, 1.);
            let k3 = rhs(v + 0.5 * h * k2, 1.);
            let k4 = rhs(v + h * k3, 1.);
            v += h / 6. * (k1 + 2. * k2 + 2. * k3 + k4);
        }
        v
    };
    let mut c = circuit(RC);
    step_at(&mut c, 1e-3);
    let out = c.add_node("out");
    c.add_device(Box::new(Cubic {
        terminals: [out, NodeId::GROUND],
        k,
    }))
    .unwrap();
    let (p, stats) = run(&mut c, &["0.1m", "4m", "0", "0.025m", "rtol=1e-4"]).unwrap();
    let error = max_error(&p, "v(out)", reference);
    assert!(error < 5e-5, "error {error:e}, {stats:?}");
    // The cubic load pulls the final value well below the linear RC's 1 V.
    assert!(p.value("v(out)", p.point_count() - 1).unwrap().re < 0.8);

    let mut c = circuit(RC);
    step_at(&mut c, 1e-3);
    let out = c.add_node("out");
    c.add_device(Box::new(Oscillator {
        terminals: [out, NodeId::GROUND],
        flips: std::cell::Cell::new(false),
    }))
    .unwrap();
    let error = error_text(run(&mut c, &["0.1m", "4m"]));
    assert!(error.contains("timestep too small"), "{error}");
}
