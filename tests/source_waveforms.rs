//! Parsed PULSE/PWL decks through the default companion transient (#9).
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::AnalysisKind;
use std::path::Path;

fn circuit(body: &str) -> Circuit {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("w.cir"),
            &format!("w\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&netlist).unwrap()
}
fn tran(c: &mut Circuit, args: &[&str]) -> ngspice_rs::primitives::SpiceResult<Plot> {
    let request = AnalysisRequest::with_arguments(AnalysisKind::Transient, args.iter().copied());
    runner(request.kind)?.run(c, &request, &AnalysisContext::default())
}
/// The value at `time`: an accepted point exactly there if one exists (the
/// companion driver lands on source breakpoints), otherwise a linear
/// interpolation between the neighbouring accepted points. The companion driver
/// reports its own timesteps, not the requested print grid.
fn at(p: &Plot, name: &str, time: f64) -> f64 {
    let n = p.point_count();
    let t = |i: usize| p.value("time", i).unwrap().re;
    let v = |i: usize| p.value(name, i).unwrap().re;
    if let Some(i) = (0..n).find(|i| (t(*i) - time).abs() < 1e-15) {
        return v(i);
    }
    let upper = (0..n)
        .find(|i| t(*i) > time)
        .unwrap_or_else(|| panic!("no sample at {time}"));
    let (t0, t1) = (t(upper - 1), t(upper));
    v(upper - 1) + (v(upper) - v(upper - 1)) * (time - t0) / (t1 - t0)
}

#[test]
fn pwl_rc_deck_matches_the_analytic_response() {
    let mut c = circuit("v1 in 0 pwl(0 0 1m 0 1.01m 1 6m 1)\nr1 in out 1k\nc1 out 0 1u");
    let p = tran(&mut c, &["0.1m", "6m", "0", "0.05m"]).unwrap();
    for i in 0..p.point_count() {
        let t = p.value("time", i).unwrap().re;
        // Ramp of 10 us into a 1 ms time constant: step response within ~5e-3.
        let want = if t <= 1e-3 {
            0.
        } else {
            1. - (-(t - 1.005e-3) / 1e-3).exp()
        };
        let got = p.value("v(out)", i).unwrap().re;
        assert!(
            (got - want).abs() < 6e-3 || t < 1.011e-3,
            "t={t}: {got} vs {want}"
        );
    }
    assert_eq!(at(&p, "v(in)", 1e-3), 0.);
    assert_eq!(at(&p, "v(in)", 2e-3), 1.);
}

#[test]
fn pulse_rc_deck_follows_each_period_and_samples_corners() {
    // Period 4 ms, 1 ms delay, 2 ms wide with 1 us edges (tau = 0.1 ms).
    let mut c = circuit("v1 in 0 pulse(0 1 1m 1u 1u 2m 4m)\nr1 in out 1k\nc1 out 0 0.1u");
    let p = tran(&mut c, &["0.5m", "10m", "0", "0.1m"]).unwrap();
    let tau = 1e-4;
    let settle =
        |t: f64, since: f64, from: f64, to: f64| to + (from - to) * (-(t - since) / tau).exp();
    // 2 ms: charging from 1 ms; 4 ms: first edge sampled at its breakpoint;
    // 6 ms: fully discharged; 6 ms is the second period's rise start.
    assert!((at(&p, "v(out)", 2e-3) - settle(2e-3, 1e-3, 0., 1.)).abs() < 2e-3);
    assert!((at(&p, "v(out)", 4e-3) - settle(4e-3, 3e-3, 1., 0.)).abs() < 2e-3);
    assert!(at(&p, "v(out)", 4e-3 + 0.5e-3) < 1e-2);
    assert!((at(&p, "v(out)", 7e-3) - 1.).abs() < 1e-3);
    assert_eq!(at(&p, "v(in)", 9e-3), 0.);
}

#[test]
fn current_source_pulse_drives_the_expected_polarity() {
    // i1 0 out pushes current into `out`: v(out) = +I*R at steady state.
    let mut c = circuit("i1 0 out pulse(0 2m 0 1u 1u 5m 10m)\nr1 out 0 1k\nc1 out 0 1n");
    let p = tran(&mut c, &["1m", "4m", "0", "1m"]).unwrap();
    // Trapezoidal rings around the steady state (alternating ~2.6e-5 V with
    // steps of ~1 ms against tau = 1 us); C ngspice rings identically.
    assert!((at(&p, "v(out)", 3e-3) - 2.).abs() < 1e-4);
}

#[test]
fn a_period_cut_ramp_restarts_without_a_breakpoint_like_c() {
    // A 10 ms rise cut by a 5 ms period: v(in) reaches 0.5 then jumps to 0.
    let mut c = circuit("v1 in 0 pulse(0 1 0 10m 1u 0 5m)\nr1 in out 1k\nc1 out 0 1n");
    let p = tran(&mut c, &["1m", "9m", "0", "0.5m"]).unwrap();
    assert!((at(&p, "v(in)", 4e-3) - 0.4).abs() < 1e-9);
    // `VSRCaccept` requests the end of the rise (TR = 10 ms), never the
    // period cut, so ngspice (and the port, bit for bit) steps across 5 ms;
    // the first point after it is already on the restarted ramp.
    let times: Vec<f64> = (0..p.point_count())
        .map(|i| p.value("time", i).unwrap().re)
        .collect();
    assert!(times.iter().all(|t| (t - 5e-3).abs() > 1e-6));
    let after = times.iter().position(|t| *t > 5e-3).unwrap();
    let restarted = (times[after] - 5e-3) / 10e-3;
    assert!((p.value("v(in)", after).unwrap().re - restarted).abs() < 1e-9);
    assert!((at(&p, "v(in)", 6e-3) - 0.1).abs() < 1e-9);
}

#[test]
fn runaway_periodic_sources_hit_the_work_limit() {
    // 1 ns period over 10 ms would be 10 million segments; ngspice would run
    // them all, the port stops at its default work limit.
    let mut c = circuit("v1 in 0 pulse(0 1 0 0.1n 0.1n 0.3n 1n)\nr1 in out 1k\nc1 out 0 1n");
    let error = tran(&mut c, &["1m", "10m"]).unwrap_err();
    assert!(error.to_string().contains("work limit"), "{error}");
}
