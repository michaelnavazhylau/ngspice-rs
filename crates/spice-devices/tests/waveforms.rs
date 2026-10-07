//! PULSE/PWL source binding, C defaults, left/right limits and lazy breakpoints
//! through the production deck -> circuit -> linear-system path (#9).
use spice_devices::{Circuit, Limit, LinearSystem, TransientTiming, Waveform};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

fn system(body: &str) -> LinearSystem {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("w.cir"),
            &format!("w\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&netlist)
        .unwrap()
        .linear_system()
        .unwrap()
}
fn timing(step: f64, stop: f64) -> TransientTiming {
    TransientTiming::new(step, stop).unwrap()
}
fn rhs(s: &LinearSystem, t: f64, limit: Limit) -> Vec<f64> {
    s.transient_rhs(t, limit).unwrap().as_slice().to_vec()
}

#[test]
fn voltage_and_current_pulses_bind_with_branch_and_node_signs() {
    // Rows: v(in)=0, v(out)=1, i(v1)=2. The V source drives its branch row; the
    // I source (current from first to second terminal) pulls out of node 1.
    let mut s = system(
        "v1 in 0 pulse(0 5 1m 1u 1u 2m 10m)\ni1 out 0 pulse(1m 3m 0 1u 1u 1m 4m)\nr1 in out 1k\nr2 out 0 1k",
    );
    s.bind_transient_timing(&timing(1e-6, 20e-3)).unwrap();
    assert_eq!(rhs(&s, 0., Limit::Right), [0., -1e-3, 0.]);
    assert_eq!(rhs(&s, 2e-3, Limit::Right), [0., -1e-3, 5.]);
    // I1 is high on [1u, 1.001m] of each 4 ms cycle.
    assert_eq!(rhs(&s, 0.5e-3, Limit::Right)[1], -3e-3);
    assert_eq!(rhs(&s, 4e-3 + 0.5e-3, Limit::Right)[1], -3e-3);
    assert_eq!(rhs(&s, 2.5e-3, Limit::Right)[1], -1e-3);
}

#[test]
fn dc_ac_and_time_forcing_stay_distinct() {
    let mut s = system("v1 in 0 dc 2 ac 3 pulse(7 9 0 1u 1u 1m 2m)\nr1 in 0 1k");
    s.bind_transient_timing(&timing(1e-6, 1e-2)).unwrap();
    assert_eq!(s.dc_rhs(None).unwrap().as_slice(), [0., 2.]);
    assert_eq!(s.ac_rhs()[1].re, 3.);
    assert_eq!(rhs(&s, 0.5e-3, Limit::Right)[1], 9.);
    // Without an explicit DC value, DC analyses see the time-zero level (V1).
    let s = system("v1 in 0 pulse(7 9)\nr1 in 0 1k");
    assert_eq!(s.dc_rhs(None).unwrap().as_slice(), [0., 7.]);
    let s = system("v1 in 0 pwl(1m 4 2m 8)\nr1 in 0 1k");
    assert_eq!(s.dc_rhs(None).unwrap().as_slice(), [0., 4.]);
}

#[test]
fn c_defaults_come_from_the_analysis_not_the_parser() {
    // TR=TF=step, PW=PER=stop: a 1 ms ramp, then a plateau for the whole run.
    let mut s = system("v1 in 0 pulse(0 1)\nr1 in 0 1k");
    assert!(s.transient_rhs(0.5e-3, Limit::Right).is_err());
    assert!(s.breakpoints_in(0., 1.).is_err());
    s.bind_transient_timing(&timing(1e-3, 1e-2)).unwrap();
    assert_eq!(rhs(&s, 0.5e-3, Limit::Right)[1], 0.5);
    assert_eq!(rhs(&s, 5e-3, Limit::Right)[1], 1.);
    let corners: Vec<_> = s.breakpoints_in(0., 1e-2).unwrap().collect();
    assert_eq!(corners, [0., 1e-3, 1e-2]);
    // Exactly five fields: PW=0 (a triangle), not the stop time.
    let mut s = system("v1 in 0 pulse(0 1 0 1m 1m)\nr1 in 0 1k");
    s.bind_transient_timing(&timing(1e-4, 1e-2)).unwrap();
    assert_eq!(rhs(&s, 1e-3, Limit::Right)[1], 1.);
    assert_eq!(rhs(&s, 2e-3, Limit::Right)[1], 0.);
}

#[test]
fn the_last_waveform_setter_wins_like_c() {
    let s = system("v1 in 0 pwl(0 1 1 2) pulse(5 6 0 1 1 1 4)\nr1 in 0 1k");
    assert!(matches!(s.sources[0].waveform, Waveform::PulseDefaults(_)));
    let s = system("v1 in 0 pulse(5 6 0 1 1 1 4) pwl(0 1 1 2)\nr1 in 0 1k");
    assert!(matches!(s.sources[0].waveform, Waveform::Pwl(_)));
}

#[test]
fn breakpoints_merge_across_sources_lazily_and_deduplicate() {
    let mut s = system(
        "v1 in 0 pulse(0 1 1 1 1 1 10)\nv2 a 0 pwl(0 0 1 1 2 1 3 0)\ni1 a in pulse(0 1 3 1 1 1 10)\nr1 in a 1\nr2 a 0 1",
    );
    s.bind_transient_timing(&timing(1e-3, 100.)).unwrap();
    let all: Vec<_> = s.breakpoints_in(0., 14.).unwrap().collect();
    assert_eq!(all, [0., 1., 2., 3., 4., 5., 6., 11., 12., 13., 14.]);
    assert!(all.windows(2).all(|w| w[0] < w[1]));
    // A window starting mid-run, and an unbounded-looking run consumed lazily.
    let tail: Vec<_> = s.breakpoints_in(5., 11.).unwrap().collect();
    assert_eq!(tail, [5., 6., 11.]);
    let mut fast = system("v1 in 0 pulse(0 1 0 1p 1p 1p 4p)\nr1 in 0 1k");
    fast.bind_transient_timing(&timing(1e-12, 1.)).unwrap();
    assert_eq!(
        fast.breakpoints_in(0., 1.).unwrap().take(1000).count(),
        1000
    );
    assert!(fast.breakpoints_in(2., 1.).is_err());
    assert!(fast.breakpoints_in(0., f64::NAN).is_err());
}

#[test]
fn invalid_decks_and_timing_are_explicit_errors() {
    for card in [
        "v1 in 0 pwl(1 1 1 2)",
        "v1 in 0 pwl(-1 1 1 2)",
        "v1 in 0 pulse(0 1 -1)",
        "v1 in 0 pulse(0 1 {a})",
    ] {
        let parsed = Parser::new().parse_deck(&parse_deck_text(
            Path::new("w.cir"),
            &format!("w\n{card}\nr1 in 0 1k\n.end\n"),
        ));
        // Unevaluated expressions may be refused by the parser or the factory,
        // but never accepted.
        if let Ok(netlist) = parsed {
            assert!(Circuit::from_netlist(&netlist).is_err(), "{card}");
        }
    }
    let s = system("v1 in 0 pulse(0 1 0 1m 1m 1m 5m)\nr1 in 0 1k");
    for bad in [(0., 1.), (1., 0.), (f64::NAN, 1.), (1., f64::INFINITY)] {
        assert!(TransientTiming::new(bad.0, bad.1).is_err());
    }
    assert!(s.transient_rhs(f64::NAN, Limit::Right).is_err());
    assert!(matches!(s.sources[0].waveform, Waveform::PulseDefaults(_)));
}
