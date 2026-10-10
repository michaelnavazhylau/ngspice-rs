//! The lossless transmission line `T` (#84) through production APIs: parsed
//! decks, `RunConfig` and the analysis runners, judged against closed-form
//! line theory (never against another simulator's step sequence).
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::{Circuit, ModelContext};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{Complex, SpiceError};

fn netlist(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("tline.cir"),
        &format!("tline\n{body}\n.end\n"),
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

/// The PWL trapezoid every transient test drives: 0 until 1 ns, 1 V from
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
/// Line options making the line land on every slope change of its waves.
const LAND: &str = "rel=1e-3 abs=1e3";

/// Max |simulated - expected(t)| over the plot.
fn worst(plot: &Plot, name: &str, expected: impl Fn(f64) -> f64) -> f64 {
    real(plot, "time")
        .iter()
        .zip(real(plot, name))
        .map(|(t, v)| (v - expected(*t)).abs())
        .fold(0., f64::max)
}

#[test]
fn a_matched_line_delays_without_reflection() {
    // Rs = Z0 = RL: the far end is the source at half amplitude, TD later;
    // the near end never sees a reflection. Without capacitors the only
    // error is the delayed-wave interpolation (traload.c's quadratic through
    // samples on both sides of a corner, for one step after each corner).
    let plot = simulate(&format!(
        "{PWL}\nrs in a 50\nt1 a 0 b 0 z0=50 td=2n {LAND}\nrl b 0 50\n.tran 0.05n 12n"
    ))
    .unwrap();
    let far = worst(&plot, "v(b)", |t| 0.5 * source(t - 2e-9));
    let near = worst(&plot, "v(a)", |t| 0.5 * source(t));
    // Measured 1.1e-15: after each echo the far end repeats the near end's
    // restart steps, so t - TD lands on stored samples.
    assert!(far < 1e-9, "{far}");
    assert!(near < 1e-12, "{near}");
    // i1 into port 1 is the incident current; i2 = -v(b)/50 into port 2.
    let i1 = worst(&plot, "v(t1#i1)", |t| source(t) / 100.);
    assert!(i1 < 1e-14, "{i1}");
    let times = real(&plot, "time");
    for corner in [3e-9, 3.5e-9, 8e-9, 8.5e-9] {
        assert!(
            times.iter().any(|t| (t - corner).abs() < 1e-20),
            "no sample at the echo {corner:e}"
        );
    }
}

#[test]
fn default_tolerances_follow_the_line_between_corners() {
    // C's default rel = abs = 1 set breakpoints only where a wave's slope
    // reverses, so the far end does not land on the delayed corners (td is
    // deliberately incommensurate with the step). traload.c's quadratic
    // through samples i-2, i-1, i spans a corner only for targets inside the
    // first (restart) step after a source corner, of length d << h, where it
    // errs by at most |slope change| d^2 / (4 h); elsewhere all three samples
    // lie on one linear piece. Measured: 1.2e-15 V.
    let plot = simulate(&format!(
        "{PWL}\nrs in a 50\nt1 a 0 b 0 z0=50 td=2.0173n\nrl b 0 50\n.tran 0.05n 12n"
    ))
    .unwrap();
    let far = worst(&plot, "v(b)", |t| 0.5 * source(t - 2.0173e-9));
    assert!(far < 1e-9, "{far}");
}

#[test]
fn an_open_line_doubles_at_the_far_end_and_returns_at_two_td() {
    // Matched source, open far end (1 Meg): the far end steps to the full
    // source TD after it; the near end sits at half the source until the
    // reflection returns 2 TD later and doubles it.
    let plot = simulate(&format!(
        "{PWL}\nrs in a 50\nt1 a 0 b 0 z0=50 td=2n {LAND}\nro b 0 1meg\n.tran 0.05n 12n"
    ))
    .unwrap();
    let gamma = (1e6 - 50.) / (1e6 + 50.);
    let far = worst(&plot, "v(b)", |t| 0.5 * (1. + gamma) * source(t - 2e-9));
    let near = worst(&plot, "v(a)", |t| {
        0.5 * source(t) + 0.5 * gamma * source(t - 4e-9)
    });
    assert!(far < 1e-9, "{far}");
    assert!(near < 1e-9, "{near}");
    let times = real(&plot, "time");
    let v = real(&plot, "v(a)");
    let at = |t: f64| v[times.iter().position(|x| (x - t).abs() < 1e-20).unwrap()];
    assert!((at(5e-9) - 0.5).abs() < 1e-12, "{}", at(5e-9));
    assert!((at(5.5e-9) - 0.5 * (1. + gamma)).abs() < 1e-9);
}

#[test]
fn a_shorted_line_cancels_at_two_td() {
    // Port 2 shorted (both far terminals on ground): reflection -1, so the
    // near end returns to zero 2 TD after each edge while the current
    // doubles.
    let plot = simulate(&format!(
        "{PWL}\nrs in a 50\nt1 a 0 0 0 z0=50 td=1n {LAND}\n.tran 0.05n 12n"
    ))
    .unwrap();
    let near = worst(&plot, "v(a)", |t| 0.5 * (source(t) - source(t - 2e-9)));
    assert!(near < 1e-9, "{near}");
    let current = worst(&plot, "v(t1#i1)", |t| (source(t) + source(t - 2e-9)) / 100.);
    assert!(current < 1e-12, "{current}");
}

#[test]
fn a_mismatched_line_rings_to_the_resistive_divider() {
    // Rs = 25 (Gs = -1/3), RL = 200 (Gl = 0.6): the far end steps by
    // 2/3 * 1.6 = 1.0667 V after TD and changes by the factor Gs*Gl = -0.2
    // every round trip, converging to 200/225.
    let plot = simulate(&format!(
        "v1 in 0 pwl(0 0 0.1n 1)\nrs in a 25\nt1 a 0 b 0 z0=50 td=1n {LAND}\nrl b 0 200\n\
         .tran 0.05n 30n"
    ))
    .unwrap();
    let times = real(&plot, "time");
    let v = real(&plot, "v(b)");
    let at = |t: f64| {
        v[times
            .iter()
            .position(|x| (x - t).abs() < 1e-20)
            .unwrap_or_else(|| panic!("no sample at {t:e}"))]
    };
    let first = 2. / 3. * 1.6;
    let mut expected = 0.;
    let mut term = first;
    for k in 0..6 {
        expected += term;
        // Plateau just before the next arrival at (2k + 3) TD.
        let t = (2. * f64::from(k) + 3.) * 1e-9 - 1e-12;
        let index = times.partition_point(|x| *x < t) - 1;
        assert!(
            (v[index] - expected).abs() < 1e-9,
            "after {k} round trips: {} vs {expected}",
            v[index]
        );
        term *= -0.2;
    }
    assert!((at(1.1e-9) - first).abs() < 1e-9, "{}", at(1.1e-9));
    assert!((v[v.len() - 1] - 200. / 225.).abs() < 1e-6);
}

#[test]
fn dc_is_a_wire_and_ac_is_the_exact_line() {
    // DC: port voltages equal, currents opposite (traload.c MODEDC).
    let op = simulate("v1 in 0 1\nrs in a 50\nt1 a 0 b 0 z0=50 td=1n\nrl b 0 150\n.op").unwrap();
    let value = |name: &str| real(&op, name)[0];
    assert!((value("v(a)") - 0.75).abs() < 1e-12);
    assert!((value("v(b)") - value("v(a)")).abs() < 1e-11);
    assert!((value("v(t1#i1)") + value("v(t1#i2)")).abs() < 1e-14);
    // AC: Zin = Z0 (ZL + j Z0 tan(w td)) / (Z0 + j ZL tan(w td)); v(a) is the
    // divider Zin / (Rs + Zin). f/nl give td = nl / f = 1 ns.
    let ac = simulate(
        "v1 in 0 dc 0 ac 1\nrs in a 50\nt1 a 0 b 0 zo=50 f=250meg nl=0.25\nrl b 0 1k\n\
         .ac lin 31 1meg 601meg",
    )
    .unwrap();
    let frequencies = real(&ac, "frequency");
    let va = ac.column("v(a)").unwrap();
    for (f, v) in frequencies.iter().zip(va) {
        let tan = (2. * std::f64::consts::PI * f * 1e-9).tan();
        let j = Complex::new(0., 1.);
        let z0 = Complex::real(50.);
        let zl = Complex::real(1000.);
        let zin = z0 * (zl + j * z0 * Complex::real(tan)) / (z0 + j * zl * Complex::real(tan));
        let expected = zin / (Complex::real(50.) + zin);
        let error = (v - expected).magnitude();
        assert!(
            error < 1e-9 * expected.magnitude() + 1e-12,
            "{f}: {v:?} vs {expected:?}"
        );
    }
}

#[test]
fn a_line_inside_a_nonlinear_circuit_and_a_subcircuit_runs() {
    let plot = simulate(
        "v1 in 0 sin(0 2 50meg)\nrs in a 50\nx1 a b line\nd1 b 0 dm\nrl b 0 1k\n.model dm d\n\
         .subckt line p q\nt1 p 0 q 0 z0=50 td=3n\n.ends\n.tran 0.1n 60n",
    )
    .unwrap();
    let clamp = real(&plot, "v(b)").into_iter().fold(0., f64::max);
    assert!(clamp > 0.5 && clamp < 1.0, "{clamp}");
    assert!(plot.column("v(t.x1.t1#i1)").is_some());
}

#[test]
fn setters_apply_in_order_and_defaults_follow_trasetup() {
    let netlist = netlist(
        "v1 a 0 1\nt1 a 0 b 0 z0=10 zo=50 ic=1,2 v1=3 rel=0.5\nrl b 0 50\n\
         t2 a 0 c 0 z0 75 td 2n f=1g nl=4\nrc c 0 50\n.op",
    )
    .unwrap();
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let context = ModelContext::default();
    let ask = |device: &str, keyword: &str| {
        circuit
            .device(device)
            .unwrap()
            .observation_parameter(keyword, &context)
            .unwrap()
            .unwrap()
    };
    assert_eq!(ask("t1", "z0"), 50.);
    assert_eq!(ask("t1", "v1"), 3.);
    assert_eq!(ask("t1", "i1"), 2.);
    assert_eq!(ask("t1", "v2"), 0.);
    assert_eq!(ask("t1", "rel"), 0.5);
    assert_eq!(ask("t1", "abs"), 1.);
    // Default nl = 0.25 at f = 1 GHz.
    assert_eq!(ask("t1", "td"), 0.25e-9);
    // td given wins over nl/f.
    assert_eq!(ask("t2", "td"), 2e-9);
    assert_eq!(ask("t2", "nl"), 4.);
}

#[test]
fn invalid_and_unsupported_uses_are_explicit_errors() {
    let card = |line: &str| netlist(&format!("v1 a 0 1\n{line}\nrl b 0 50\n.op"));
    let build = |line: &str| card(line).and_then(|n| Circuit::from_netlist(&n));
    for (line, needle) in [
        ("t1 a 0 b 0 td=1n", "z0 must be given"),
        ("t1 a 0 b 0 z0=0 td=1n", "z0 must be positive"),
        ("t1 a 0 b 0 z0=-50 td=1n", "z0 must be positive"),
        ("t1 a 0 b 0 z0=50 td=0", "td must be positive"),
        ("t1 a 0 b 0 z0=50 f=0", "f and nl must be positive"),
        ("t1 a 0 b 0 z0=50 td=1n rel=-1", "must not be negative"),
    ] {
        let error = build(line).unwrap_err();
        assert!(error.to_string().contains(needle), "{line}: {error}");
    }
    for (line, needle) in [
        ("t1 a 0 b 0 7 z0=50 td=1n", "leading value"),
        (
            "t1 a 0 b 0 z0=50 td=1n len=2",
            "unknown transmission-line parameter",
        ),
        ("t1 a 0 b 0 z0=50 td=1n ic=1,2,3,4,5", "too many"),
        ("t1 a 0 b z0=50", "parameter '='"),
    ] {
        let error = card(line).unwrap_err();
        assert!(error.to_string().contains(needle), "{line}: {error}");
    }
    let deck = "v1 in 0 pulse(0 1 1n 1n 1n 5n 20n) ac 1\nrs in a 50\nt1 a 0 b 0 z0=50 td=1n\n\
                rl b 0 100\n";
    for (analysis, needle) in [
        (".tran 0.1n 10n backend=diffsol method=bdf", "diffsol BDF"),
        (".tran 0.1n 10n uic", "uic"),
        (".pz in 0 b 0 vol pz", "pole-zero"),
        (".noise v(b) v1 dec 2 1meg 10meg", ".noise"),
        (".disto dec 2 1meg 10meg", ".disto"),
        (".sens v(b)", "sensitivity"),
    ] {
        let error = simulate(&format!("{deck}{analysis}")).unwrap_err();
        assert!(
            error
                .to_string()
                .to_lowercase()
                .contains(&needle.to_lowercase()),
            "{analysis}: {error}"
        );
    }
}

#[test]
fn the_card_round_trips_through_the_writer() {
    let first = netlist(
        "v1 a 0 1\nt1 a 0 b 0 zo=50 ic=(1,2,3) v2=0.5 td=1n rel=0.1 abs=10\nrl b 0 50\n.op",
    )
    .unwrap();
    let written = ngspice_rs::netlist::write_netlist(&first).unwrap();
    let second = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("written.cir"), &written))
        .unwrap();
    assert!(
        ngspice_rs::netlist::semantic_eq(&first, &second),
        "{written}"
    );
    let line = &second.devices[1];
    assert_eq!(line.designator, 't');
    let names: Vec<_> = line.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["zo", "ic", "v2", "td", "rel", "abs"]);
}
