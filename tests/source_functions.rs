//! SIN/EXP/SFFM/AM sources, the PULSE count and PWL `td=`/`r=` (#94, #95)
//! through the production deck -> runner path: OP time-zero values, analytic
//! transient responses (trap and Gear-2), explicit rejection by the diffsol
//! BDF backend, and `.four` of a SIN drive.
use std::f64::consts::PI;
use std::path::Path;

use ngspice_rs::analysis::{AnalysisContext, Plot, RunConfig, fourier, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

fn run(body: &str) -> Plot {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("s.cir"),
            &format!("s\n{body}\n.end\n"),
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &AnalysisContext::default())
        .unwrap()
}

fn column(plot: &Plot, name: &str) -> Vec<f64> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}

#[test]
fn operating_points_use_the_c_time_zero_values() {
    // vsrcload.c (MODEDC) evaluates the function at time 0 without a DC value.
    let plot = run(
        "v1 a 0 sin(0.5 2 1k 0 0 90)\nv2 b 0 exp(-1 1 1m)\nv3 c 0 sffm(3 1)\n\
         i1 0 d am(3 1m)\nv4 e 0 pulse(0.25 1 0 1u 1u 1m 2m 4)\nv5 f 0 dc 4 sin(0 1)\n\
         i2 0 g pwl(0 1m 1m 2m) td=-0.5m r=0\nr1 a 0 1\nr2 b 0 1\nr3 c 0 1\nr4 d 0 1k\nr5 e 0 1\n\
         r6 f 0 1\nr7 g 0 1k\n.op",
    );
    let value = |name: &str| column(&plot, name)[0];
    assert!((value("v(a)") - 2.5).abs() < 1e-12);
    assert_eq!(value("v(b)"), -1.);
    assert_eq!(value("v(c)"), 0.);
    assert_eq!(value("v(d)"), 0.);
    assert_eq!(value("v(e)"), 0.25);
    assert_eq!(value("v(f)"), 4.);
    assert!((value("v(g)") - 1.5).abs() < 1e-12);
}

/// RC response to EXP(0 1 TD1 TAU1 TD2 TAU2) before TD2, tau = RC.
fn exp_rc(t: f64, td1: f64, tau1: f64, tau: f64) -> f64 {
    if t <= td1 {
        return 0.;
    }
    let s = t - td1;
    1. - (tau1 * (-s / tau1).exp() - tau * (-s / tau).exp()) / (tau1 - tau)
}

#[test]
fn exp_rc_follows_the_analytic_response_with_trap_and_gear() {
    let deck = "v1 in 0 exp(0 1 0.2m 0.3m 1.5m 0.5m)\nr1 in out 1k\nc1 out 0 0.1u\n";
    for (label, analysis) in [
        ("trap", ".tran 5u 1.4m"),
        ("gear", ".options method=gear\n.tran 5u 1.4m"),
    ] {
        let plot = run(&format!("{deck}{analysis}"));
        let (time, out, input) = (
            column(&plot, "time"),
            column(&plot, "v(out)"),
            column(&plot, "v(in)"),
        );
        let mut corner = false;
        for ((t, v), u) in time.iter().zip(&out).zip(&input) {
            let want = exp_rc(*t, 0.2e-3, 0.3e-3, 1e-4);
            // Gear-2 restarts at TD1 with a first-order step: 5e-5 V covers its
            // local error near the corner on a 1 V scale.
            assert!(
                (v - want).abs() <= 2e-3 * want.abs() + 5e-5,
                "{label} t={t}: {v} vs {want}"
            );
            let source = if *t <= 0.2e-3 {
                0.
            } else {
                1. - (-(t - 0.2e-3) / 0.3e-3).exp()
            };
            assert!((u - source).abs() < 1e-12, "{label} t={t}");
            corner |= (t - 0.2e-3).abs() < 1e-15;
        }
        // The port lands on TD1 (C itself sets no EXP breakpoint).
        assert!(corner, "{label}: no sample at TD1");
    }
}

#[test]
fn sffm_am_pulse_count_and_repeated_pwl_drive_their_nodes_exactly() {
    let plot = run(
        "v1 a 0 sffm(0.1 1 5k 2 500 0.1m 90 30)\nr1 a 0 1k\ni1 0 b am(0 1m 0.5m 500 5k)\n\
         r2 b 0 1k\nv2 c 0 pulse(0 1 0.1m 20u 20u 0.2m 0.5m 2)\nr3 c 0 1k\n\
         v3 d 0 pwl(0 0 0.5m 1 1m 0.5) r=0.5m td=0.2m\nr4 d 0 1k\n.tran 10u 2m",
    );
    let time = column(&plot, "time");
    let (a, b, c, d) = (
        column(&plot, "v(a)"),
        column(&plot, "v(b)"),
        column(&plot, "v(c)"),
        column(&plot, "v(d)"),
    );
    let mut delay_samples = 0;
    for i in 0..time.len() {
        let t = time[i];
        // A repeated time marks a jump: the second sample is the right limit.
        let right = i > 0 && time[i - 1] == t;
        let s = t - 0.1e-3;
        let sffm = if s < 0. || (s == 0. && !right) {
            0.
        } else {
            0.1 + ((2. * PI * 5e3 * s + PI / 6.) + 2. * (2. * PI * 500. * s + PI / 2.).sin()).sin()
        };
        assert!((a[i] - sffm).abs() < 1e-9, "sffm t={t}: {} vs {sffm}", a[i]);
        delay_samples += usize::from(s == 0.);
        let am = 1e3 * (1e-3 + 0.5e-3 * (2. * PI * 500. * t).sin()) * (2. * PI * 5e3 * t).sin();
        assert!((b[i] - am).abs() < 1e-9, "am t={t}");
        // Two pulses (0.1-0.54 ms and 0.6-1.04 ms), then 0 V.
        if t > 1.1e-3 {
            assert_eq!(c[i], 0., "pulse t={t}");
        }
        // Repeating [0.5 ms, 1 ms] of the PWL from 1.2 ms: 1 -> 0.5 V ramps.
        if t > 1.2e-3 - 1e-12 {
            let local = (t - 1.2e-3).rem_euclid(0.5e-3);
            if local.min(0.5e-3 - local) < 1e-12 {
                // A repetition boundary: left limit 0.5 V, right limit 1 V.
                let want = if right { 1. } else { 0.5 };
                assert!((d[i] - want).abs() < 1e-9, "pwl boundary t={t}: {}", d[i]);
            } else {
                let want = 1. - local / 1e-3;
                assert!((d[i] - want).abs() < 1e-9, "pwl t={t}: {} vs {want}", d[i]);
            }
        }
    }
    // The port lands on the SFFM delay jump (C sets no breakpoint there).
    assert!(delay_samples >= 1);
}

#[test]
fn four_of_a_sin_driven_rc_has_the_analytic_spectrum() {
    // Steady-state RC low-pass at 1 kHz, tau = 0.1 ms: |H| = 1/sqrt(1+(w tau)^2),
    // phase -atan(w tau). The final period (4-5 ms) starts 40 tau after t = 0.
    let text = "four\nv1 in 0 sin(0 1 1k)\nr1 in out 1k\nc1 out 0 0.1u\n.tran 1u 5m\n\
                .four 1k v(in) v(out)\n.end\n";
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("four.cir"), text))
        .unwrap();
    let netlist = &parsed.netlist;
    let config = RunConfig::from_netlist(netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let results = fourier::resolve(&plot, request.kind, &parsed.fourier).unwrap();
    assert_eq!(results.len(), 2);
    let wt = 2. * PI * 1e3 * 1e-4;
    for (result, gain, phase) in [
        (&results[0], 1., 0.),
        (&results[1], 1. / (1. + wt * wt).sqrt(), -wt.atan()),
    ] {
        let fundamental = &result.harmonics[0];
        assert_eq!(fundamental.order, 1);
        assert!(
            (fundamental.amplitude - gain).abs() < 1e-3 * gain,
            "{}: {} vs {gain}",
            result.vector,
            fundamental.amplitude
        );
        assert!(
            (fundamental.phase - phase).abs() < 2e-3,
            "{}",
            result.vector
        );
        assert!(
            result.dc.abs() < 1e-3,
            "{}: dc {}",
            result.vector,
            result.dc
        );
        // A clean sine: total harmonic distortion well below 0.1 %.
        assert!(result.thd < 1e-3, "{}: THD {}", result.vector, result.thd);
    }
}

#[test]
fn the_bdf_backend_refuses_non_piecewise_linear_forcing() {
    // Its segments interpolate the forcing linearly between breakpoints, which
    // would silently approximate these functions.
    for source in ["sin(0 1 1k)", "exp(0 1)", "sffm(0 1)", "am(0 1)"] {
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("s.cir"),
                &format!(
                    "s\nv1 in 0 {source}\nr1 in out 1k\nc1 out 0 1u\n\
                     .tran 1u 1m 0 1u backend=diffsol method=bdf\n.end\n"
                ),
            ))
            .unwrap();
        let config = RunConfig::from_netlist(&netlist).unwrap();
        let request = config.request_for(&netlist.analyses[0]).unwrap();
        let mut circuit = config.circuit(&netlist).unwrap();
        let error = runner(request.kind)
            .unwrap()
            .run(&mut circuit, &request, &AnalysisContext::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("companion driver"), "{source}: {error}");
    }
    // Piecewise-linear PWL options and the PULSE count remain supported there.
    let plot = run(
        "v1 in 0 pwl(0 0 1m 1 2m 0) r=0 td=0.5m\nv2 b 0 pulse(0 1 0 1u 1u 1m 2m 1)\n\
         r1 in x 1k\nr2 b y 1k\nc1 x 0 1n\nc2 y 0 1n\n.tran 0.1m 5m 0 0.1m backend=diffsol method=bdf",
    );
    let (time, v) = (column(&plot, "time"), column(&plot, "v(in)"));
    let at = |t: f64| v[time.iter().position(|x| (x - t).abs() < 1e-12).unwrap()];
    assert!((at(1e-3) - 0.5).abs() < 1e-9);
    assert!(at(2.5e-3).abs() < 1e-9);
    assert!((at(3.5e-3) - 1.).abs() < 1e-9);
    assert!((at(4e-3) - 0.5).abs() < 1e-9);
    let b = column(&plot, "v(b)");
    assert_eq!(
        b[time.iter().position(|x| (x - 3e-3).abs() < 1e-12).unwrap()],
        0.
    );
}

/// Analytic RC (tau = 0.1 ms) response to the sawtooth `pwl(0 0 1m 1) r=0`:
/// within each ramp `v' = (t_local / P - v) / tau`, with `v` continuous across
/// the jumps of the source.
fn sawtooth_rc(t: f64) -> f64 {
    let (period, tau) = (1e-3, 1e-4);
    let ramp = |v0: f64, s: f64| {
        // Solution of v' = (s/P - v)/tau from v(0) = v0.
        let k = tau / period;
        s / period - k + (v0 + k) * (-s / tau).exp()
    };
    let mut v = 0.;
    let mut start = 0.;
    while t - start > period {
        v = ramp(v, period);
        start += period;
    }
    ramp(v, t - start)
}

#[test]
fn a_discontinuous_repeated_pwl_ramps_every_repetition_in_both_backends() {
    // Every repetition boundary is a 1 V -> 0 V jump: the step ending there
    // takes the left limit, so the diffsol segments interpolate the full ramp
    // instead of holding the restart value.
    for backend in ["", " backend=diffsol method=bdf"] {
        let plot = run(&format!(
            "v1 in 0 pwl(0 0 1m 1) r=0\nr1 in out 1k\nc1 out 0 0.1u\n.tran 10u 4m 0 10u{backend}"
        ));
        let (time, vin, vout) = (
            column(&plot, "time"),
            column(&plot, "v(in)"),
            column(&plot, "v(out)"),
        );
        for (i, t) in time.iter().enumerate() {
            let local = t - 1e-3 * (t / 1e-3).floor();
            // Away from a jump the input is the ramp itself (to the BDF
            // solver tolerance on algebraic rows).
            if local > 1e-9 && local < 1e-3 - 1e-9 {
                assert!(
                    (vin[i] - local / 1e-3).abs() < 1e-6,
                    "{backend} v(in) at {t:e}: {}",
                    vin[i]
                );
            }
            let want = sawtooth_rc(*t);
            assert!(
                (vout[i] - want).abs() < 5e-3,
                "{backend} v(out) at {t:e}: {} vs {want}",
                vout[i]
            );
        }
        // Every later repetition reaches the top of the ramp again.
        for k in 1..4 {
            let start = f64::from(k) * 1e-3;
            let peak = time
                .iter()
                .zip(&vin)
                .filter(|(t, _)| **t > start + 1e-9 && **t < start + 1e-3 - 1e-9)
                .map(|(_, v)| *v)
                .fold(0., f64::max);
            assert!(peak > 0.95, "{backend} repetition {k}: peak {peak}");
        }
    }
}
