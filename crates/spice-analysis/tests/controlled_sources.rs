//! Linear controlled sources E/F/G/H through the production analyses
//! (GitHub #78): analytic OP/DC/AC/transient results, an ideal op-amp feedback
//! circuit, branch-current exposure and explicit failures.
//!
//! The committed C goldens `controlled_op`, `controlled_ac` and
//! `controlled_tran` are compared by `cargo xtask golden verify`.
use std::path::Path;

use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, Selection, runner, write_requests};
use spice_core::{AnalysisKind, SpiceResult};
use spice_devices::Circuit;
use spice_netlist::{Parser, source::parse_deck_text};

fn circuit(body: &str) -> SpiceResult<Circuit> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("controlled.cir"),
        &format!("controlled\n{body}\n.end\n"),
    ))?;
    Circuit::from_netlist(&netlist)
}

fn run(c: &mut Circuit, kind: AnalysisKind, args: &[&str]) -> SpiceResult<Plot> {
    runner(kind)?.run(
        c,
        &AnalysisRequest::with_arguments(kind, args.iter().copied()),
        &AnalysisContext::default(),
    )
}

fn op(body: &str) -> Plot {
    run(
        &mut circuit(body).unwrap(),
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap()
}

fn re(plot: &Plot, name: &str, point: usize) -> f64 {
    plot.value(name, point)
        .unwrap_or_else(|| panic!("no {name}"))
        .re
}

fn close(got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs() + 1e-15,
        "{got} != {want}"
    );
}

/// The four sources and their C sign conventions in one operating point.
const ALL_FOUR: &str = "vin in 0 dc 2\nrin in 0 1k\n\
    e1 o1 0 in 0 3\nr1 o1 0 1k\n\
    g1 0 o2 in 0 1m\nr2 o2 0 1k\n\
    f1 0 o3 vin 2\nr3 o3 0 1k\n\
    h1 o4 0 vin 100\nr4 o4 0 1k";

#[test]
fn operating_point_signs_follow_c() {
    let p = op(ALL_FOUR);
    // E: v(o1) = 3 v(in); its branch current flows out of n+ into the load,
    // so i(e1), positive from n+ through the source to n-, is -6 mA.
    close(re(&p, "v(o1)", 0), 6.0, 1e-12);
    close(re(&p, "i(e1)", 0), -6e-3, 1e-12);
    // G: 1 mS x 2 V flows from n+ (ground) through g1 into o2.
    close(re(&p, "v(o2)", 0), 2.0, 1e-12);
    // i(vin) = -2 mA (a source delivering power has a negative current).
    close(re(&p, "i(vin)", 0), -2e-3, 1e-12);
    // F: 2 i(vin) = -4 mA flows from ground through f1 into o3.
    close(re(&p, "v(o3)", 0), -4.0, 1e-12);
    // H: v(o4) = 100 i(vin); i(h1) = +0.2 mA from the load through h1.
    close(re(&p, "v(o4)", 0), -0.2, 1e-12);
    close(re(&p, "i(h1)", 0), 2e-4, 1e-12);
    // Only E and H carry a branch-current vector.
    assert!(p.variable_index("i(g1)").is_none());
    assert!(p.variable_index("i(f1)").is_none());
}

#[test]
fn an_ideal_op_amp_closes_inverting_and_non_inverting_loops() {
    let a = 1e6;
    // Inverting amplifier, -R2/R1 = -10: KCL at the virtual ground gives
    // v(inv) = 10/(11 + A) and v(out) = -A v(inv).
    let p = op("vin in 0 dc 1\nr1 in inv 1k\nr2 inv out 10k\ne1 out 0 0 inv 1e6");
    close(re(&p, "v(out)", 0), -10.0 * a / (11.0 + a), 1e-11);
    close(re(&p, "v(inv)", 0), 10.0 / (11.0 + a), 1e-9);
    assert!((re(&p, "v(out)", 0) + 10.0).abs() < 1.2e-4);
    // Non-inverting amplifier, 1 + R2/R1 = 4.
    let p = op("vin in 0 dc 0.5\nrin in 0 1k\ne1 out 0 in fb 1e6\nr1 fb 0 1k\nr2 out fb 3k");
    close(re(&p, "v(out)", 0), 2.0 / (1.0 + 4.0 / a), 1e-11);
    // The op-amp supplies the feedback and load current through its branch.
    close(re(&p, "i(e1)", 0), -re(&p, "v(out)", 0) / 4e3, 1e-11);
    // A unity-gain follower driving a load from a high-impedance divider.
    let p = op("vin in 0 dc 1\nra in mid 1meg\nrb mid 0 1meg\ne1 out 0 mid out 1e6\nrl out 0 10");
    close(re(&p, "v(out)", 0), 0.5 * a / (1.0 + a), 1e-11);
}

#[test]
fn dc_sweeps_carry_controlled_sources_as_context_not_targets() {
    let mut c = circuit(ALL_FOUR).unwrap();
    let p = run(&mut c, AnalysisKind::DcSweep, &["vin", "-1", "1", "0.5"]).unwrap();
    assert_eq!(p.point_count(), 5);
    for point in 0..5 {
        let v = re(&p, "v(in)", point);
        close(re(&p, "v(o1)", point), 3.0 * v, 1e-12);
        close(re(&p, "v(o2)", point), 1e-3 * v * 1e3, 1e-12);
        close(re(&p, "v(o3)", point), 2.0 * (-v / 1e3) * 1e3, 1e-12);
        close(re(&p, "v(o4)", point), 100.0 * (-v / 1e3), 1e-12);
    }
    // Controlled sources are not sweep targets (C's .dc takes V/I/R/temp).
    for target in ["e1", "g1", "f1", "h1"] {
        let error = run(&mut c, AnalysisKind::DcSweep, &[target, "0", "1", "1"]).unwrap_err();
        assert!(!error.to_string().is_empty(), "{target}");
    }
}

#[test]
fn ac_gains_are_real_and_frequency_independent() {
    let body = "vin in 0 dc 0 ac 1\nrin in 0 1k\n\
        e1 o1 0 in 0 3\nr1 o1 0 1k\n\
        g1 0 o2 in 0 1m\nr2 o2 0 1k\n\
        f1 0 o3 vin 2\nr3 o3 0 1k\n\
        h1 o4 0 vin 100\nr4 o4 0 1k";
    let mut c = circuit(body).unwrap();
    let p = run(&mut c, AnalysisKind::Ac, &["dec", "2", "1", "1meg"]).unwrap();
    assert!(p.point_count() >= 2);
    for point in 0..p.point_count() {
        for (name, want) in [
            ("v(o1)", 3.0),
            ("v(o2)", 1.0),
            ("v(o3)", -2.0),
            ("v(o4)", -0.1),
            ("i(e1)", -3e-3),
            ("i(h1)", 1e-4),
        ] {
            let value = p.value(name, point).unwrap();
            close(value.re, want, 1e-12);
            assert!(value.im.abs() < 1e-15, "{name}: {value:?}");
        }
    }
    // A capacitor makes the controlled response frequency dependent only
    // through the controlling quantity: an E-buffered RC pole.
    let mut c = circuit(
        "vin in 0 dc 0 ac 1\nr1 in a 1k\nc1 a 0 1u\ne1 b 0 a 0 2\nrb b 0 1k\nvs b c 0\n\
         rc c 0 1k\nh1 d 0 vs 1k\nrd d 0 1k",
    )
    .unwrap();
    let p = run(&mut c, AnalysisKind::Ac, &["lin", "3", "100", "300"]).unwrap();
    for point in 0..3 {
        let w = 2.0 * std::f64::consts::PI * re(&p, "frequency", point);
        let pole = spice_core::Complex::new(1.0, w * 1e-3);
        let va = spice_core::Complex::real(1.0) / pole;
        let b = p.value("v(b)", point).unwrap();
        close(b.re, 2.0 * va.re, 1e-12);
        close(b.im, 2.0 * va.im, 1e-12);
        let d = p.value("v(d)", point).unwrap();
        // i(vs) = v(b)/1k, so v(d) = 1k i(vs) = v(b).
        close(d.re, b.re, 1e-12);
        close(d.im, b.im, 1e-12);
    }
}

/// E buffers an RC (tau = 1 ms), G turns the buffered voltage into a current
/// charging a second RC (tau = 1 ms) whose resistor current vs senses for an
/// F and an H. With s = t - 1 ms after the step,
/// v(a) = 1 - e^(-s/tau), v(b) = 2 v(a) and v(c) = 2 (1 - (1 + s/tau) e^(-s/tau)).
const TRANSIENT: &str = "vin in 0 pulse(0 1 1m 1n 1n 1 2)\nr1 in a 1k\nc1 a 0 1u\n\
    e1 b 0 a 0 2\nrb b 0 1k\ng1 0 c b 0 1m\nrc c d 1k\nvs d 0 0\nc2 c 0 1u\n\
    f1 0 o4 vs 2\nr4 o4 0 1k\nh1 o5 0 vs 100\nr5 o5 0 1k";

fn second_order(t: f64) -> f64 {
    let s = (t - 1e-3 - 0.5e-9).max(0.0) / 1e-3;
    2.0 * (1.0 - (1.0 + s) * (-s).exp())
}

#[test]
fn transients_match_analytic_responses_on_both_backends() {
    for extra in [&[][..], &["backend=diffsol", "method=bdf"][..]] {
        let mut c = circuit(TRANSIENT).unwrap();
        let mut args = vec!["10u", "6m"];
        args.extend(extra);
        let p = run(&mut c, AnalysisKind::Transient, &args).unwrap();
        let mut worst: f64 = 0.0;
        for point in 0..p.point_count() {
            let t = re(&p, "time", point);
            let (a, b, cc) = (
                re(&p, "v(a)", point),
                re(&p, "v(b)", point),
                re(&p, "v(c)", point),
            );
            let ivs = re(&p, "i(vs)", point);
            // Algebraic relations hold at every point to the solver's
            // tolerances. The companion driver solves them at each accepted
            // point; BDF output samples are interpolated, so they satisfy the
            // constraints only to its relative tolerance.
            let relative = if extra.is_empty() { 1e-9 } else { 1e-6 };
            let near = |got: f64, want: f64| {
                assert!(
                    (got - want).abs() <= relative * want.abs() + 1e-12,
                    "{extra:?} t={t}: {got} vs {want}"
                );
            };
            near(b, 2.0 * a);
            near(ivs, cc / 1e3);
            near(re(&p, "v(o4)", point), 2e3 * ivs);
            near(re(&p, "v(o5)", point), 100.0 * ivs);
            near(re(&p, "i(e1)", point), -b / 1e3);
            near(re(&p, "i(h1)", point), -100.0 * ivs / 1e3);
            worst = worst.max((cc - second_order(t)).abs());
        }
        // Integration error only: the default tolerances on a 10 us grid.
        assert!(worst < 2e-3, "{extra:?}: worst {worst}");
        close(re(&p, "time", p.point_count() - 1), 6e-3, 1e-12);
    }
}

#[test]
fn branch_currents_are_selectable_and_measurable() {
    let netlist = Parser::new()
        .parse_deck_with_output(&parse_deck_text(
            Path::new("save.cir"),
            "save\n.subckt buf in out\ne1 out 0 in 0 2\n.ends buf\nvin in 0 dc 1\nrin in 0 1k\n\
             x1 in o1 buf\nr1 o1 0 1k\nh1 o2 0 vin 10\nr2 o2 0 1k\n.op\n\
             .save i(e.x1.e1) i(h1) v(o2)\n.end\n",
        ))
        .unwrap();
    let mut c = Circuit::from_netlist(&netlist.netlist).unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
    let requests = write_requests(&netlist.output, AnalysisKind::OperatingPoint).unwrap();
    let selection = Selection::resolve(&plot, AnalysisKind::OperatingPoint, &requests).unwrap();
    let written = selection.apply(&plot).unwrap();
    let names: Vec<_> = written.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["i(e.x1.e1)", "i(h1)", "v(o2)"]);
    close(re(&written, "i(e.x1.e1)", 0), -2e-3, 1e-12);
    close(re(&written, "i(h1)", 0), 1e-5, 1e-12);
}

#[test]
fn branch_currents_are_measurable_over_a_dc_sweep() {
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(
            Path::new("meas.cir"),
            "meas\nvin in 0 dc 1\nrin in 0 1k\ne1 o1 0 in 0 2\nr1 o1 0 1k\nh1 o2 0 vin 10\n\
             r2 o2 0 1k\n.dc vin 0 2 0.5\n.meas dc emin min i(e1)\n.meas dc hmax max i(h1)\n.end\n",
        ))
        .unwrap();
    let mut c = Circuit::from_netlist(&parsed.netlist).unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vin", "0", "2", "0.5"]).unwrap();
    let results =
        spice_analysis::measure::resolve(&plot, AnalysisKind::DcSweep, &parsed.measurements)
            .unwrap();
    // i(e1) = -2 vin / 1k is most negative at vin = 2. v(o2) = 10 i(vin) =
    // -vin/100, so i(h1) = -v(o2)/1k = vin/100k is largest at vin = 2.
    close(results[0].value, -4e-3, 1e-12);
    assert_eq!(results[0].at, Some(2.0));
    close(results[1].value, 2e-5, 1e-12);
    assert_eq!(results[1].at, Some(2.0));
}

#[test]
fn invalid_controlling_sources_fail_before_any_analysis() {
    for (body, message) in [
        (
            "vin in 0 dc 1\nrin in 0 1k\nf1 a 0 vx 2\nra a 0 1k",
            "unknown controlling source vx",
        ),
        (
            "vin in 0 dc 1\nl1 in b 1m\nrb b 0 1k\nh1 a 0 l1 2\nra a 0 1k",
            "has no findable branch current",
        ),
        (
            "vin in 0 dc 1\nrin in 0 1k\ne1 a a in 0 2",
            "is a shorted VCVS",
        ),
    ] {
        let error = circuit(body).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    let error =
        circuit("vin in 0 dc 1\nrin in 0 1k\ne1 a 0 poly(1) in 0 0 2\nra a 0 1k").unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}

#[test]
fn ideal_controlled_source_loops_are_singular_not_silently_solved() {
    // Two E sources forcing the same node pair, and a VCVS whose output drives
    // its own control with unit gain (v = v), have no unique operating point.
    for body in [
        "vin in 0 dc 1\nrin in 0 1k\ne1 a 0 in 0 1\ne2 a 0 in 0 2",
        "e1 a 0 a 0 1\nra a 0 1k",
    ] {
        let result = run(
            &mut circuit(body).unwrap(),
            AnalysisKind::OperatingPoint,
            &[],
        );
        assert!(result.is_err(), "{body}");
    }
}
