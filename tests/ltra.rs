//! The lossy transmission line `O` with `ltra` models (#85) through
//! production APIs: parsed decks, `RunConfig` and the analysis runners,
//! judged against closed-form line theory and against the lossless `T` line.
use std::f64::consts::PI;
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::netlist::writer::write_netlist;
use ngspice_rs::primitives::{Complex, SpiceError};

fn netlist(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("ltra.cir"),
        &format!("ltra\n{body}\n.end\n"),
    ))
}

fn simulate(body: &str) -> Result<Plot, SpiceError> {
    let netlist = netlist(body)?;
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

fn real(plot: &Plot, name: &str) -> Vec<f64> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}

fn complex(plot: &Plot, name: &str) -> Vec<Complex> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .to_vec()
}

/// The PWL trapezoid the transient tests drive: 0 until 1 ns, 1 V from
/// 1.5 ns to 6 ns, 0 again from 6.5 ns.
fn source(t: f64) -> f64 {
    let ns = t * 1e9;
    if ns <= 1. {
        0.
    } else if ns <= 1.5 {
        (ns - 1.) / 0.5
    } else if ns <= 6. {
        1.
    } else if ns <= 6.5 {
        1. - (ns - 6.) / 0.5
    } else {
        0.
    }
}
const PWL: &str = "v1 in 0 pwl(0 0 1n 0 1.5n 1 6n 1 6.5n 0)";

/// Max |simulated - expected(t)| over the plot.
fn worst(plot: &Plot, name: &str, expected: impl Fn(f64) -> f64) -> f64 {
    real(plot, "time")
        .iter()
        .zip(real(plot, name))
        .map(|(t, v)| (v - expected(*t)).abs())
        .fold(0., f64::max)
}

/// Linear interpolation of a plot column at `t`.
fn at(plot: &Plot, name: &str, t: f64) -> f64 {
    let times = real(plot, "time");
    let values = real(plot, name);
    let k = times.partition_point(|x| *x < t).clamp(1, times.len() - 1);
    let (t0, t1) = (times[k - 1], times[k]);
    values[k - 1] + (values[k] - values[k - 1]) * (t - t0) / (t1 - t0)
}

#[test]
fn a_matched_lc_line_delays_without_reflection() {
    // L = 250 nH/m, C = 100 pF/m, len = 2 m: Z0 = 50, TD = 10 ns. Matched at
    // both ends, the far end is the source at half amplitude TD later and
    // the near end never sees a reflection. The LC case is the lossless
    // line: its breakpoints land on the delayed corners and the quadratic
    // interpolation of piecewise-linear samples is exact (measured 1e-15).
    let plot = simulate(&format!(
        "{PWL}\nrs in a 50\no1 a 0 b 0 lc\nrl b 0 50\n\
         .model lc ltra l=250n c=100p len=2\n.tran 0.05n 30n"
    ))
    .unwrap();
    let far = worst(&plot, "v(b)", |t| 0.5 * source(t - 10e-9));
    let near = worst(&plot, "v(a)", |t| 0.5 * source(t));
    assert!(far < 1e-9, "{far}");
    assert!(near < 1e-12, "{near}");
    // i1 into port 1, i2 = -v(b)/50 into port 2 (C's CKTmkVolt unknowns).
    let i1 = worst(&plot, "v(o1#i1)", |t| source(t) / 100.);
    let i2 = worst(&plot, "v(o1#i2)", |t| -source(t - 10e-9) / 100.);
    assert!(i1 < 1e-14 && i2 < 1e-12, "{i1} {i2}");
    // ltratrun.c's default steplimit bounds every step by TD.
    let times = real(&plot, "time");
    assert!(
        times
            .windows(2)
            .all(|w| w[1] - w[0] <= 10e-9 * (1. + 1e-12))
    );
}

#[test]
fn the_lc_case_equals_the_lossless_t_line() {
    // A 25 ohm source and a 200 ohm load reflect at both ends; the LTRA LC
    // line (L = 250 nH/m, C = 100 pF/m, len = 0.4 m: Z0 = 50, TD = 2 ns) and
    // the TRA line with the same Z0 and TD give the same waves. Their step
    // sequences differ (LTRA bounds every step by TD and, like C, skips the
    // breakpoint where a wave leaves a plateau because ltraacct.c's CHECK
    // calls it steady), so compare inside the linear pieces of the exact
    // answer, where both are exact.
    let deck = |line: &str| format!("{PWL}\nrs in a 25\n{line}\nrl b 0 200\n.tran 0.02n 12n");
    let o = simulate(&deck(
        "o1 a 0 b 0 lc\n.model lc ltra l=250n c=100p len=0.4 rel=1e-3 abs=1e3",
    ))
    .unwrap();
    let t = simulate(&deck("t1 a 0 b 0 z0=50 td=2n rel=1e-3 abs=1e3")).unwrap();
    // Exact ringing: the launched wave (2/3) times 1 + 0.6 at the load,
    // then (-1/3)(0.6) per round trip.
    let exact = |time: f64| {
        let mut v = 0.;
        let mut k = 0;
        let mut factor = (2. / 3.) * 1.6;
        while time - 2e-9 * f64::from(2 * k + 1) > 0. {
            v += factor * source(time - 2e-9 * f64::from(2 * k + 1));
            factor *= -0.2;
            k += 1;
        }
        v
    };
    for time in [
        2.5e-9, 3.2e-9, 5e-9, 7.25e-9, 7.8e-9, 9.5e-9, 10.5e-9, 11.3e-9,
    ] {
        let (vo, vt, ve) = (at(&o, "v(b)", time), at(&t, "v(b)", time), exact(time));
        assert!((vo - ve).abs() < 1e-9, "{time:e}: {vo} vs exact {ve}");
        assert!((vo - vt).abs() < 1e-9, "{time:e}: {vo} vs T line {vt}");
    }
}

/// `v(len, t) / V` of an RC line held at a step `V` at `x = 0` and open at
/// `x = len`, from rest: `1 - (4/pi) sum (-1)^k/(2k+1) exp(-(2k+1)^2 pi^2 t
/// / (4 R C len^2))`.
fn diffusion_open_end(t: f64, rcl2: f64) -> f64 {
    if t <= 0. {
        return 0.;
    }
    let mut sum = 0.;
    for k in 0..200 {
        let n = f64::from(2 * k + 1);
        let sign = if k % 2 == 0 { 1. } else { -1. };
        sum += sign / n * (-n * n * PI * PI * t / (4. * rcl2)).exp();
    }
    1. - 4. / PI * sum
}

#[test]
fn an_rc_line_follows_its_diffusion_solution() {
    // R = 1 kohm/m, C = 1 nF/m, len = 1 m: R C len^2 = 1 us. A 1 V step at
    // 1 us drives the near end directly; the far end is open, so its
    // voltage is the diffusion series above. The convolution with C's
    // piecewise-linear erfc responses is a low-order quadrature: the
    // measured error is 3.17e-3 V just after the step (1.12 us), the same
    // as the reference binary's own (3.17e-3 V on this deck), and it settles
    // 7.8e-4 V above 1 V.
    let plot = simulate(
        "v1 a 0 pulse(0 1 1u 1n 1n 1 2)\no1 a 0 b 0 rc\n\
         .model rc ltra r=1k c=1n len=1\n.tran 20n 6u",
    )
    .unwrap();
    let far = worst(&plot, "v(b)", |t| diffusion_open_end(t - 1e-6, 1e-6));
    assert!(far < 4e-3, "{far}");
    // The far end is open: i2 = 0 at every point.
    let i2 = real(&plot, "v(o1#i2)")
        .iter()
        .fold(0., |m: f64, v| m.max(v.abs()));
    assert!(i2 < 1e-15, "{i2}");
    // DC limit: the whole line charges to the source.
    let last = *real(&plot, "v(b)").last().unwrap();
    assert!((last - 1.).abs() < 1e-3, "{last}");
}

#[test]
fn an_rlc_line_attenuates_its_first_arrival_and_settles_to_dc() {
    // R = 5 ohm/m, L = 250 nH/m, C = 100 pF/m, len = 2 m: Z0 = 50, TD = 10 ns,
    // beta = R/(2L) = 1e7/s, attenuation exp(-beta TD) = exp(-0.1). Matched
    // at both ends, the far end first sees half the source step times the
    // attenuation; the convolution tails then raise it towards the DC
    // divider RL / (Rs + R len + RL) = 50/110.
    let plot = simulate(
        "v1 in 0 pulse(0 1 1n 0.1n 0.1n 1 2)\nrs in a 50\no1 a 0 b 0 rlc\nrl b 0 50\n\
         .model rlc ltra r=5 l=250n c=100p len=2\n.tran 0.1n 400n",
    )
    .unwrap();
    let arrival = at(&plot, "v(b)", 11.2e-9);
    let expected = 0.5 * (-0.1f64).exp();
    assert!((arrival - expected).abs() < 5e-3, "{arrival} vs {expected}");
    // Nothing arrives before TD.
    assert!(at(&plot, "v(b)", 10.9e-9).abs() < 1e-12);
    let last = *real(&plot, "v(b)").last().unwrap();
    assert!((last - 50. / 110.).abs() < 2e-3, "{last}");
}

#[test]
fn dc_matches_c_two_ports() {
    // LC/RLC/RC: i1 + i2 = 0 and v(p1) - v(p2) = R len i1 (ltraload.c MODEDC
    // ignores the negative terminals). RG: the exact cosh/sinh two-port.
    let plot = simulate(
        "v1 in 0 dc 1\nrs in a 50\no1 a 0 b 0 rlc\nrl b 0 50\n\
         rs2 in c 50\no2 c 0 d 0 rg\nrl2 d 0 100\n\
         .model rlc ltra r=3 l=250n c=100p len=2\n.model rg ltra r=10 g=1m len=2\n.op",
    )
    .unwrap();
    let v = |name: &str| real(&plot, name)[0];
    let i = 1. / (50. + 6. + 50.);
    assert!((v("v(b)") - 50. * i).abs() < 1e-12);
    assert!((v("v(o1#i1)") - i).abs() < 1e-15);
    assert!((v("v(o1#i2)") + i).abs() < 1e-15);
    // RG: Z_in of a line terminated in RL is Z0 (RL + Z0 tanh) / (Z0 + RL tanh)
    // with Z0 = sqrt(R/G) = 100, gamma len = sqrt(RG) len = 0.2.
    let (z0, gl) = (100., 0.2f64);
    let zin = z0 * (100. + z0 * gl.tanh()) / (z0 + 100. * gl.tanh());
    let vc = zin / (50. + zin);
    assert!((v("v(c)") - vc).abs() < 1e-9, "{} {vc}", v("v(c)"));
    // v(d) = v(c) / (cosh + Z0/RL sinh).
    let vd = vc / (gl.cosh() + z0 / 100. * gl.sinh());
    assert!((v("v(d)") - vd).abs() < 1e-9, "{} {vd}", v("v(d)"));
}

fn csqrt(z: Complex) -> Complex {
    let (r, phase) = (z.magnitude().sqrt(), z.phase() / 2.);
    Complex::new(r * phase.cos(), r * phase.sin())
}

fn ccosh(z: Complex) -> Complex {
    let exp = |w: Complex| Complex::new(w.re.exp() * w.im.cos(), w.re.exp() * w.im.sin());
    let half = Complex::new(0.5, 0.);
    (exp(z) + exp(-z)) * half
}

#[test]
fn ac_is_the_exact_open_ended_line() {
    // An ideal source on port 1 and an open port 2: V2/V1 = 1/cosh(gamma len)
    // with gamma = sqrt((R + j omega L) j omega C) for every case.
    let plot = simulate(
        "v1 a 0 dc 0 ac 1\no1 a 0 b 0 lc\no2 a 0 c 0 rlc\no3 a 0 d 0 rc\n\
         .model lc ltra l=250n c=100p len=1\n.model rlc ltra r=20 l=250n c=100p len=1\n\
         .model rc ltra r=1k c=10p len=1\n.ac dec 7 1meg 30meg",
    )
    .unwrap();
    let frequencies = complex(&plot, "frequency");
    for (k, f) in frequencies.iter().enumerate() {
        let w = 2. * PI * f.re;
        for (node, r, l, c) in [
            ("v(b)", 0., 250e-9, 100e-12),
            ("v(c)", 20., 250e-9, 100e-12),
            ("v(d)", 1e3, 0., 10e-12),
        ] {
            let gamma = csqrt(Complex::new(r, w * l) * Complex::new(0., w * c));
            let expected = Complex::new(1., 0.) / ccosh(gamma);
            let got = complex(&plot, node)[k];
            assert!(
                (got - expected).magnitude() < 1e-9 * (1. + expected.magnitude()),
                "{node} at {} Hz: {got:?} vs {expected:?}",
                f.re
            );
        }
    }
}

#[test]
fn model_flags_and_setters_follow_c_order() {
    // LTRAmParam applies flags in written order: the last interpolation and
    // step-limit flags win; nocontrol, truncnr and ltra change nothing.
    let base = "v1 in 0 pwl(0 0 1n 0 1.5n 1)\nrs in a 30\no1 a 0 b 0 m\nrl b 0 120\n";
    let a = simulate(&format!(
        "{base}.model m ltra ltra r=5 l=250n c=100p len=1 lininterp mixedinterp nocontrol \
         truncnr\n.tran 0.05n 20n"
    ))
    .unwrap();
    let b = simulate(&format!(
        "{base}.model m ltra r=5 l=250n c=100p len=1 mixedinterp\n.tran 0.05n 20n"
    ))
    .unwrap();
    assert_eq!(real(&a, "v(b)"), real(&b, "v(b)"));
    // nosteplimit lets the steps grow beyond TD = 5 ns on the flat tail.
    let free = simulate(&format!(
        "{base}.model m ltra l=250n c=100p len=1 nosteplimit\n.tran 20n 400n 0 20n"
    ))
    .unwrap();
    let limited = simulate(&format!(
        "{base}.model m ltra l=250n c=100p len=1 steplimit\n.tran 20n 400n 0 20n"
    ))
    .unwrap();
    let longest = |plot: &Plot| {
        real(plot, "time")
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(0., f64::max)
    };
    assert!(longest(&limited) <= 5e-9 * (1. + 1e-12));
    assert!(longest(&free) > 5e-9, "{}", longest(&free));
}

#[test]
fn unsupported_lines_and_analyses_are_explicit() {
    let line = |model: &str, analysis: &str| {
        simulate(&format!(
            "v1 a 0 dc 1 ac 1\nrs a b 50\no1 b 0 c 0 m\nrl c 0 50\n.model m ltra {model}\n\
             {analysis}"
        ))
    };
    let message = |result: Result<Plot, SpiceError>| result.unwrap_err().to_string();
    for (model, text) in [
        ("r=1 l=1n len=1", "RL line not supported yet"),
        ("g=1m c=1p l=1n len=1", "Nonzero G (except RG)"),
        ("c=1p len=1", "At least two of R,L,G,C"),
        ("l=1n c=1p", "length must be given"),
        ("l=1n c=1p len=0", "len must satisfy"),
        ("r=-1 l=1n c=1p len=1", "r must satisfy"),
        ("l=1n c=1p len=1 level=1", "level on an ltra model"),
        ("l=1n c=1p len=1 foo=1", "unsupported setter 'foo'"),
        ("l=1n c=1p len=1 v1=1", "instance-parameter default"),
    ] {
        let error = message(line(model, ".tran 1n 10n"));
        assert!(error.contains(text), "{model}: {error}");
    }
    let lc = "l=1n c=1p len=1";
    for (analysis, text) in [
        (".tran 1n 10n uic", "ltraload.c"),
        (
            ".pz b 0 c 0 vol pz",
            "pole-zero analysis of lossy transmission line",
        ),
        (
            ".noise v(c) v1 dec 1 1k 1meg",
            ".noise with lossy transmission line",
        ),
        (".disto dec 1 1k 10k", ".disto with lossy transmission line"),
        (".sens v(c)", "sensitivity"),
        (
            ".tran 1n 10n backend=diffsol method=bdf",
            "the adaptive BDF backend was removed",
        ),
    ] {
        let error = line(lc, analysis).unwrap_err();
        assert!(error.to_string().contains(text), "{analysis}: {error}");
    }
    assert!(
        line(lc, ".tran 1n 10n uic")
            .unwrap_err()
            .is_not_yet_ported()
    );
    // C's LTRAacLoad corrupts every other lossy line's AC load once it meets
    // an RG model; the port refuses RG lines in AC.
    let error = message(line("r=10 g=1m len=2", ".ac dec 1 1meg 10meg"));
    assert!(error.contains("RG lossy line"), "{error}");
    // ... but simulates them in DC and transient.
    line("r=10 g=1m len=2", ".tran 1n 10n").unwrap();
}

#[test]
fn the_card_grammar_follows_inp2o() {
    let parsed =
        netlist("o1 a 0 b 0 m v1=1 ic=2,3 i2=4\n.model m ltra(l=1n c=1p len=1 nosteplimit)\n.op")
            .unwrap();
    let device = &parsed.devices[0];
    assert_eq!(device.designator, 'o');
    assert_eq!(device.model.as_deref(), Some("m"));
    let names: Vec<_> = device.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["v1", "ic", "i2"]);
    for (body, text) in [
        ("o1 a 0 b 0", "lossy-line model name"),
        ("o1 a 0 b 0 m 5", "leading value"),
        ("o1 a 0 b 0 m z0=50", "unknown lossy-line parameter"),
        ("o1 a 0 b 0 m ic=1,2,3,4,5", "too many"),
    ] {
        let error = netlist(&format!("{body}\n.model m ltra l=1n c=1p len=1\n.op"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(text), "{body}: {error}");
    }
    // A wrong model family is C's "incorrect model type".
    let netlist = netlist("o1 a 0 b 0 m\n.model m d\nv1 a 0 1\n.op").unwrap();
    assert!(Circuit::from_netlist(&netlist).is_err());
}

#[test]
fn the_deck_writer_round_trips_o_cards_and_ltra_models() {
    let parsed = netlist(
        "v1 a 0 1\no1 a 0 b 0 m v1=1 ic=2,3\nrl b 0 50\n\
         .model m ltra l=1n c=1p len=1 mixedinterp truncdontcut\n.op",
    )
    .unwrap();
    let text = write_netlist(&parsed).unwrap();
    assert!(text.contains("o1 a 0 b 0 m v1=1 ic=(2,3)"), "{text}");
    assert!(text.contains("mixedinterp truncdontcut"), "{text}");
    let again = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("again.cir"), &text))
        .unwrap();
    let shape = |netlist: &ngspice_rs::netlist::ast::Netlist| {
        netlist.devices[1]
            .parameters
            .iter()
            .map(|p| match &p.kind {
                ngspice_rs::netlist::ast::ParameterKind::InitialConditions(values) => format!(
                    "{}={}",
                    p.name,
                    values
                        .iter()
                        .map(|v| format!("{}:{}", v.name, v.value.text))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                _ => format!("{}={}", p.name, p.value),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(&again), shape(&parsed));
    assert_eq!(shape(&parsed), ["v1=1", "ic=v1:2,i1:3"]);
}
