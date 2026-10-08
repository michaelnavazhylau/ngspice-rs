//! PULSE/PWL source binding, C defaults, left/right limits and lazy breakpoints
//! through the production deck -> circuit -> linear-system path (#9).
use spice_devices::{Circuit, Limit, LinearSystem, TransientTiming, Waveform};
use spice_netlist::{Parser, source::parse_deck_text};
use std::f64::consts::PI;
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

fn build_error(body: &str) -> String {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("w.cir"),
            &format!("w\n{body}\n.end\n"),
        ))
        .unwrap();
    match Circuit::from_netlist(&netlist) {
        Ok(_) => panic!("{body}: accepted"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn source_functions_bind_with_c_defaults_and_signs() {
    // #94. Rows: v(a)=0, v(b)=1, i(v1)=2, i(v2)=3.
    let mut s = system(
        "v1 a 0 sin(1 2)\nv2 b 0 exp(0 1)\ni1 a b sffm(0 1m)\ni2 b 0 am(0 1m 0.5 1k 10k)\nr1 a b 1k\nr2 b 0 1k",
    );
    // Unbound defaults are refused, never guessed.
    assert!(s.transient_rhs(0.1, Limit::Right).is_err());
    assert!(s.breakpoints_in(0., 1.).is_err());
    s.bind_transient_timing(&timing(1e-3, 1.)).unwrap();
    // SIN: FREQ = 1/stop = 1 Hz.
    let t = 0.125;
    let rhs = rhs(&s, t, Limit::Right);
    assert!((rhs[2] - (1. + 2. * (2. * PI * t).sin())).abs() < 1e-12);
    // EXP: TD1 = TAU1 = step, TD2 = 2*step, TAU2 = step.
    let exp = |t: f64| (1. - (-(t - 1e-3) / 1e-3).exp()) - (1. - (-(t - 2e-3) / 1e-3).exp());
    assert!((rhs[3] - exp(t)).abs() < 1e-12);
    // SFFM: FC = 5 Hz, FM = 500 Hz, MDI limited to 0.01; I1 flows a -> b.
    let sffm = 1e-3 * ((2. * PI * 5. * t) + 0.01 * (2. * PI * 500. * t).sin()).sin();
    let am = (1e-3 + 0.5 * (2. * PI * 1e3 * t).sin()) * (2. * PI * 1e4 * t).sin();
    assert!((rhs[0] + sffm).abs() < 1e-15);
    assert!((rhs[1] - sffm + am).abs() < 1e-15);
    // Corners the port lands on: EXP TD1/TD2; SIN/SFFM/AM delays are zero.
    let corners: Vec<_> = s.breakpoints_in(0., 1.).unwrap().collect();
    assert_eq!(corners, [0., 1e-3, 2e-3]);
}

#[test]
fn dc_levels_are_the_c_time_zero_values() {
    // vsrcload.c evaluates the function at time 0 when no DC value is given:
    // SIN gives VO + VA sin(PHASE), EXP gives V1 and SFFM/AM give zero.
    let s = system(
        "v1 a 0 sin(1 2 1k 0 0 30)\nv2 b 0 exp(3 4 1m)\nv3 c 0 sffm(5 1)\nv4 d 0 am(6 1)\n\
         v5 e 0 dc 7 sin(1 2)\nv6 f 0 pwl(0 1 1m 2) td=-0.5m\nr1 a 0 1\nr2 b 0 1\nr3 c 0 1\nr4 d 0 1\nr5 e 0 1\nr6 f 0 1",
    );
    let dc = s.dc_rhs(None).unwrap();
    let dc = &dc.as_slice()[6..];
    assert!((dc[0] - 2.).abs() < 1e-12, "{dc:?}");
    assert_eq!(&dc[1..], [3., 0., 0., 7., 1.5]);
}

#[test]
fn pulse_count_and_pwl_repeat_from_decks() {
    // #95: three pulses, then V1; repeated PWL with a delay.
    let mut s = system(
        "v1 a 0 pulse(0 1 1m 1m 1m 2m 10m 3)\ni1 b 0 pwl(0 0 1m 1m 2m 0) r=0 td=0.5m\nr1 a 0 1\nr2 b 0 1",
    );
    s.bind_transient_timing(&timing(1e-4, 50e-3)).unwrap();
    assert_eq!(rhs(&s, 23e-3, Limit::Right)[2], 1.);
    assert_eq!(rhs(&s, 32e-3, Limit::Right)[2], 0.);
    // I1 is a 2 ms triangle starting at 0.5 ms, pulled out of node b.
    assert!((rhs(&s, 1e-3, Limit::Right)[1] + 0.5e-3).abs() < 1e-15);
    assert!((rhs(&s, 10.5e-3 + 1e-3, Limit::Right)[1] + 1e-3).abs() < 1e-15);
    let close = |got: Vec<f64>, want: &[f64]| {
        assert_eq!(got.len(), want.len(), "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-15, "{got:?}");
        }
    };
    close(
        s.breakpoints_in(0., 8e-3).unwrap().collect(),
        &[
            0.5e-3, 1e-3, 1.5e-3, 2e-3, 2.5e-3, 3.5e-3, 4e-3, 4.5e-3, 5e-3, 5.5e-3, 6.5e-3, 7.5e-3,
        ],
    );
    // Past the third pulse only its final boundary (31 ms) and C's last
    // request (the end of the next rise, 32 ms) remain.
    close(
        s.breakpoints_in(29.6e-3, 33e-3).unwrap().collect(),
        &[30.5e-3, 31e-3, 31.5e-3, 32e-3, 32.5e-3],
    );
    close(
        s.breakpoints_in(33e-3, 40e-3).unwrap().collect(),
        &[
            33.5e-3, 34.5e-3, 35.5e-3, 36.5e-3, 37.5e-3, 38.5e-3, 39.5e-3,
        ],
    );
}

#[test]
fn unsupported_source_options_are_explicit() {
    for (body, needle) in [
        (
            "v1 a 0 pwl(0 0 1m 1) r=0.5m\nr1 a 0 1",
            "matches no time point",
        ),
        (
            "v1 a 0 pwl(0 0 1m 1) r=1m\nr1 a 0 1",
            "smaller than the last",
        ),
        (
            "v1 a 0 r=0 pwl(0 0 1m 1)\nr1 a 0 1",
            "without a preceding PWL",
        ),
        (
            "v1 a 0 pwl(0 0 1m 1) r=0 pwl(0 0 2m 1)\nr1 a 0 1",
            "after r=",
        ),
        ("v1 a 0 sin(0 1) td=1m\nr1 a 0 1", "without a PWL"),
        ("v1 a 0 sin(0 1 1k -1m)\nr1 a 0 1", "negative TD"),
        ("v1 a 0 exp(0 1 1m 1m -1m)\nr1 a 0 1", "negative TD2"),
    ] {
        let error = build_error(body);
        assert!(error.contains(needle), "{body}: {error}");
    }
}
