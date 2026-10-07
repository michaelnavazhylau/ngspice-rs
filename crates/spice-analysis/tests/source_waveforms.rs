//! Parsed PULSE/PWL decks through the explicit diffsol BDF transient (#9).
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use spice_core::AnalysisKind;
use spice_devices::Circuit;
use spice_netlist::{Parser, source::parse_deck_text};
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
fn tran(c: &mut Circuit, args: &[&str]) -> spice_core::SpiceResult<Plot> {
    let request = AnalysisRequest::with_arguments(AnalysisKind::Transient, args.iter().copied());
    runner(request.kind)?.run(c, &request, &AnalysisContext::default())
}
fn bdf(mut args: Vec<&str>) -> Vec<&str> {
    args.extend(["backend=diffsol", "method=bdf"]);
    args
}
fn at(p: &Plot, name: &str, time: f64) -> f64 {
    let i = (0..p.point_count())
        .find(|i| (p.value("time", *i).unwrap().re - time).abs() < 1e-15)
        .unwrap_or_else(|| panic!("no sample at {time}"));
    p.value(name, i).unwrap().re
}

#[test]
fn pwl_rc_deck_matches_the_analytic_response() {
    let mut c = circuit("v1 in 0 pwl(0 0 1m 0 1.01m 1 6m 1)\nr1 in out 1k\nc1 out 0 1u");
    let p = tran(&mut c, &bdf(vec!["0.1m", "6m", "0", "0.05m"])).unwrap();
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
    let p = tran(&mut c, &bdf(vec!["0.5m", "10m", "0", "0.1m"])).unwrap();
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
    let p = tran(&mut c, &bdf(vec!["1m", "4m", "0", "1m"])).unwrap();
    assert!((at(&p, "v(out)", 3e-3) - 2.).abs() < 1e-6);
}

#[test]
fn a_period_cut_ramp_is_a_jump_sampled_from_the_right() {
    // A 10 ms rise cut by a 5 ms period: v(in) reaches 0.5 then jumps to 0.
    let mut c = circuit("v1 in 0 pulse(0 1 0 10m 1u 0 5m)\nr1 in out 1k\nc1 out 0 1n");
    let p = tran(&mut c, &bdf(vec!["1m", "9m", "0", "0.5m"])).unwrap();
    assert!((at(&p, "v(in)", 4e-3) - 0.4).abs() < 1e-12);
    // The accepted sample at the breakpoint is the right limit, never a
    // value interpolated across the jump.
    assert_eq!(at(&p, "v(in)", 5e-3), 0.);
    assert!((at(&p, "v(in)", 6e-3) - 0.1).abs() < 1e-12);
}

#[test]
fn ordinary_tran_and_unsupported_requests_still_fail() {
    let mut c = circuit("v1 in 0 pulse(0 1)\nr1 in 0 1k");
    assert!(tran(&mut c, &["1m", "10m"]).is_err());
    assert!(tran(&mut c, &["1m", "10m", "backend=diffsol"]).is_err());
    assert!(tran(&mut c, &bdf(vec!["1m", "10m", "maxord=2"])).is_err());
}

#[test]
fn breakpoint_budget_stops_runaway_periodic_sources() {
    // 1 ns period over 10 ms would be 10 million segments.
    let mut c = circuit("v1 in 0 pulse(0 1 0 0.1n 0.1n 0.3n 1n)\nr1 in out 1k\nc1 out 0 1n");
    let error = tran(&mut c, &bdf(vec!["1m", "10m"])).unwrap_err();
    assert!(error.to_string().contains("breakpoint limit"), "{error}");
}
