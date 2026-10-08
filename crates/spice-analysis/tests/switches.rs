//! S/W switches through the production analyses (GitHub #81): operating-point
//! flags and hysteresis bands, DC-sweep hysteresis carried by accepted state,
//! transient threshold crossings with `swtrunc.c` step control, Newton
//! nonconvergence on state flips, AC at C's `MODEINITSMSIG` state and
//! explicit failures.
//!
//! The committed C goldens `switch_op`, `switch_dc`, `switch_tran` and
//! `switch_w_tran` are compared by `cargo xtask golden verify`.
use std::path::Path;

use spice_analysis::newton::{NewtonOptions, PhasePolicy, solve_phased};
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use spice_core::{AnalysisKind, Real, SpiceResult};
use spice_devices::{Circuit, IterationPhase, StateHistory};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, source::parse_deck_text};

fn circuit(body: &str) -> SpiceResult<Circuit> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("switches.cir"),
        &format!("switches\n{body}\n.end\n"),
    ))?;
    Circuit::from_netlist(&netlist)
}

fn run(body: &str, kind: AnalysisKind, args: &[&str]) -> SpiceResult<Plot> {
    runner(kind)?.run(
        &mut circuit(body)?,
        &AnalysisRequest::with_arguments(kind, args.iter().copied()),
        &AnalysisContext::default(),
    )
}

fn column(plot: &Plot, name: &str) -> Vec<Real> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}

fn close(got: Real, want: Real, relative: Real) {
    assert!(
        (got - want).abs() <= relative * want.abs() + 1e-15,
        "{got} != {want}"
    );
}

/// 1 V through 1k into a switch to ground: 1/101 V closed (10 ohm),
/// 1M/1.001M V open.
const CLOSED: Real = 10. / 1010.;
const OPEN: Real = 1e6 / (1e6 + 1e3);

#[test]
fn operating_points_use_the_flag_only_inside_the_band() {
    let deck = |control: &str, flag: &str| {
        format!(
            "vc c 0 {control}\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm {flag}\n\
             .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)"
        )
    };
    for (control, flag, closed) in [
        ("1", "", false),
        ("1", "on", true),
        ("1.5", "on", true),
        ("1.5", "off on off", false),
        ("2", "", true),
        ("2", "off", true),
        ("0.4", "on", false),
    ] {
        let plot = run(&deck(control, flag), AnalysisKind::OperatingPoint, &[]).unwrap();
        close(
            column(&plot, "v(a)")[0],
            if closed { CLOSED } else { OPEN },
            1e-12,
        );
    }
}

#[test]
fn dc_sweeps_carry_hysteresis_from_point_to_point() {
    let deck = "vc c 0 0\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
                .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)";
    let crossing = |args: &[&str]| -> Real {
        let plot = run(deck, AnalysisKind::DcSweep, args).unwrap();
        let sweep = column(&plot, "v(c)");
        let a = column(&plot, "v(a)");
        let flips: Vec<Real> = a
            .windows(2)
            .zip(&sweep[1..])
            .filter(|(pair, _)| (pair[0] < 0.5) != (pair[1] < 0.5))
            .map(|(_, v)| *v)
            .collect();
        assert_eq!(flips.len(), 1, "{args:?}: {a:?}");
        flips[0]
    };
    // Up: closes at the first point above VT + VH; down: opens below VT - VH.
    close(crossing(&["vc", "0", "3", "0.25"]), 1.75, 1e-12);
    close(crossing(&["vc", "3", "0", "-0.25"]), 0.25, 1e-12);
}

#[test]
fn a_relaxation_oscillator_flips_within_the_newton_solve_of_a_timepoint() {
    // 1 mA charges 100 nF by 0.1 V per 10 us step until the open trial would
    // exceed 1.5 V; MODEINITFLOAT then closes the 100 ohm switch within the
    // same timepoint, so no accepted sample lies outside the 0.5 .. 1.5 V
    // band (the state flips before acceptance, never after it).
    let plot = run(
        "i1 0 c pulse(0 1m 10u 1u)\nc1 c 0 100n\ns1 c 0 c 0 sm\n\
         .model sm sw(vt=1 vh=0.5 ron=100 roff=1meg)",
        AnalysisKind::Transient,
        &["10u", "1m"],
    )
    .unwrap();
    let time = column(&plot, "time");
    let v = column(&plot, "v(c)");
    let mut peaks = Vec::new();
    let mut valleys = Vec::new();
    for k in 1..v.len() - 1 {
        if v[k] > v[k - 1] && v[k] > v[k + 1] {
            peaks.push(v[k]);
        }
        if v[k] < v[k - 1] && v[k] < v[k + 1] && time[k] > 100e-6 {
            valleys.push(v[k]);
        }
    }
    assert!(peaks.len() >= 8, "{peaks:?}");
    for peak in &peaks {
        assert!(*peak > 1.3 && *peak <= 1.5, "{peaks:?}");
    }
    for valley in &valleys {
        assert!(*valley > 0.5 && *valley < 0.75, "{valleys:?}");
    }
    // Charging 0 -> 1.5 V takes 150 us after the 10 us delay; the first
    // sample past it is the discharged point of the closing step.
    let first = v.iter().position(|v| *v > 1.3).unwrap();
    assert!(
        time[first] > 140e-6 && time[first] < 170e-6,
        "{}",
        time[first]
    );
}

#[test]
fn switch_step_control_lands_close_to_each_threshold() {
    // A purely resistive circuit has no truncation error: without swtrunc.c
    // the step would double up to the 20 ms tmax and leap across both
    // thresholds. The limit (0.75 (ref - v) +/- 0.05) / dv/dt keeps the first
    // sample past VT + VH within 0.05 V of it, and likewise below VT - VH.
    let plot = run(
        "vc c 0 pwl(0 0 10m 2 20m 0)\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
         .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)",
        AnalysisKind::Transient,
        &["1u", "20m", "0", "20m"],
    )
    .unwrap();
    let control = column(&plot, "v(c)");
    let a = column(&plot, "v(a)");
    let time = column(&plot, "time");
    let closing = (1..a.len())
        .find(|&k| a[k] < 0.5 && a[k - 1] > 0.5)
        .unwrap();
    assert!(
        control[closing] > 1.5 && control[closing] <= 1.55,
        "{control:?}"
    );
    assert!(control[closing - 1] <= 1.5);
    let opening = (closing..a.len())
        .find(|&k| a[k] > 0.5 && a[k - 1] < 0.5)
        .unwrap();
    assert!(time[opening] > 10e-3);
    assert!(
        control[opening] < 0.5 && control[opening] >= 0.45,
        "{control:?}"
    );
    assert!(control[opening - 1] >= 0.5);
    // The same 39 accepted points C takes (switch_tran-style golden parity).
    assert!(time.len() < 60, "{}", time.len());
}

#[test]
fn current_switches_follow_the_sensed_branch_current() {
    // A SIN of 1 mA amplitude through vsense: W closes above 0.4 mA and opens
    // below 0.2 mA, discharging c1 in the positive half-waves only.
    let plot = run(
        "va a 0 sin(0 1 1k)\nvsense a b 0\nrl b 0 1k\nvdd vdd 0 2\nr1 vdd c1 4k\n\
         c1 c1 0 50n\nw1 c1 0 vsense wm\n.model wm csw(it=0.3m ih=0.1m ron=200 roff=1meg)",
        AnalysisKind::Transient,
        &["2u", "2m"],
    )
    .unwrap();
    let time = column(&plot, "time");
    let current = column(&plot, "i(vsense)");
    let v = column(&plot, "v(c1)");
    // Low while the current is well above the band, recharged well below it.
    for ((t, i), v) in time.iter().zip(&current).zip(&v) {
        if *i > 0.6e-3 && (t * 1e3).fract() > 0.15 {
            assert!(*v < 0.3, "t={t}: i={i}, v={v}");
        }
    }
    assert!(v.iter().any(|v| *v > 1.5));
}

#[test]
fn ac_uses_cs_minitsmsig_state_not_the_operating_point() {
    // The switch is closed at the operating point (2 V > 1.5 V), but C's ACan
    // reloads with MODEINITSMSIG, which copies the zero CKTstate1 ("really
    // off") into CKTstate0: the AC divider is the open 1M / (1k + 1M), as in C
    // (docs/port/SWITCHES.md). The operating point itself is closed.
    let deck = "vc c 0 2\nvin in 0 0 ac 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
                .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)";
    let plot = run(deck, AnalysisKind::Ac, &["lin", "1", "1k", "1k"]).unwrap();
    close(plot.value("v(a)", 0).unwrap().re, OPEN, 1e-12);
    // The same for an ON-flagged switch inside its band (HYST_ON at the
    // operating point) and for W.
    let on = "vc c 0 1.2\nvin in 0 0 ac 1\nr1 in a 1k\ns1 a 0 c 0 sm on\n\
              .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)";
    let plot = run(on, AnalysisKind::Ac, &["lin", "1", "1k", "1k"]).unwrap();
    close(plot.value("v(a)", 0).unwrap().re, OPEN, 1e-12);
    let w = "vc c 0 2\nrc c d 1k\nvs d 0 0\nvin in 0 0 ac 1\nr1 in a 1k\nw1 a 0 vs wm\n\
             .model wm csw(it=1m ih=0.5m ron=10 roff=1meg)";
    let plot = run(w, AnalysisKind::Ac, &["lin", "1", "1k", "1k"]).unwrap();
    close(plot.value("v(a)", 0).unwrap().re, OPEN, 1e-12);
}

#[test]
fn the_off_conductance_defaults_to_the_gmin_option() {
    let text = "switches\nvin in 0 1\nr1 in a 1k\ns1 a 0 0 c sm\nvc c 0 1\n\
                .model sm sw(ron=1)\n.options gmin=1e-6\n.op\n.end\n";
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("gmin.cir"), text))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap();
    close(column(&plot, "v(a)")[0], 1e6 / (1e6 + 1e3), 1e-12);
}

#[test]
fn switches_inside_subcircuits_sense_renamed_sources() {
    let plot = run(
        ".subckt cell in out\nvs in mid 0\nrs mid 0 1k\nw1 out 0 vs wm\n.ends cell\n\
         vin x 0 2\nvdd vdd 0 1\nrl vdd y 1k\nxc x y cell\n.model wm csw(it=1m ron=10)",
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap();
    // 2 V / 1k = 2 mA > 1 mA through the renamed v.xc.vs closes w.xc.w1.
    close(column(&plot, "i(v.xc.vs)")[0], 2e-3, 1e-12);
    close(column(&plot, "v(y)")[0], 10. / 1010., 1e-12);
}

#[test]
fn unsupported_switch_analyses_fail_explicitly() {
    let deck = "vc c 0 pulse(0 2 1u 1u)\nvin in 0 1\nr1 in a 1k\nc1 a 0 1n\ns1 a 0 c 0 sm\n\
                .model sm sw(vt=1)";
    // The diffsol BDF backend needs immutable linear equations.
    let error = run(
        deck,
        AnalysisKind::Transient,
        &["1u", "10u", "backend=diffsol", "method=bdf"],
    )
    .unwrap_err();
    assert!(error.to_string().contains("s1"), "{error}");
    // Nonlinear uic initialization is not implemented.
    let mut c = circuit(deck).unwrap();
    let mut request = AnalysisRequest::with_arguments(AnalysisKind::Transient, ["1u", "10u"]);
    request.uic = true;
    let error = runner(AnalysisKind::Transient)
        .unwrap()
        .run(&mut c, &request, &AnalysisContext::default())
        .unwrap_err();
    assert!(error.to_string().contains("uic"), "{error}");
}

#[test]
fn phased_newton_never_converges_on_a_load_marked_nonconvergent() {
    // A one-unknown "circuit" whose load reports nonconvergence on its first
    // two float loads: the solve must continue although x is exact at once.
    let history = StateHistory::new(1);
    let mut phases = Vec::new();
    let mut floats = 0;
    // Start 0.1 from the root so voltage-step damping does not take part.
    let solved = solve_phased(
        &Vector::from_slice(&[1.9]),
        &[false],
        &NewtonOptions::default(),
        PhasePolicy::OperatingPoint,
        None,
        |_, phase, previous| {
            phases.push((phase, previous.is_some()));
            let mut trial = history.trial_in(phase, previous)?;
            let mut device = history.device(&mut trial, 0..1)?;
            device.set(0, 1.)?;
            if phase == IterationPhase::Float {
                floats += 1;
                if floats <= 2 {
                    device.report_nonconvergence()?;
                }
            }
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, 1.)?;
            let mut b = Vector::zeros(1);
            b.add_to(0, 2.)?;
            Ok((a, b, trial))
        },
    )
    .unwrap();
    assert_eq!(solved.values.as_slice(), [2.]);
    assert!(!solved.trial.is_nonconvergent());
    assert_eq!(solved.trial.phase(), IterationPhase::Float);
    // JCT, then FIX until the iterate converges, then FLOAT with the previous
    // load as the iterate; flagged float loads are never accepted.
    // Load 1 (JCT) solves x = 2; load 2 (FIX) reproduces it, so the FLOAT
    // check reload follows; the two flagged float loads keep iterating.
    assert_eq!(phases[0], (IterationPhase::Junction, false));
    assert_eq!(phases[1], (IterationPhase::Fix, true));
    assert!(
        phases[2..]
            .iter()
            .all(|step| *step == (IterationPhase::Float, true)),
        "{phases:?}"
    );
    assert_eq!(floats, 4, "{phases:?}");
    // A predicted solve starts in MODEINITPRED.
    let mut first = None;
    solve_phased(
        &Vector::zeros(1),
        &[false],
        &NewtonOptions::default(),
        PhasePolicy::Predicted,
        None,
        |_, phase, previous| {
            first.get_or_insert(phase);
            let mut trial = history.trial_in(phase, previous)?;
            history.device(&mut trial, 0..1)?.set(0, 0.)?;
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, 1.)?;
            Ok((a, Vector::zeros(1), trial))
        },
    )
    .unwrap();
    assert_eq!(first, Some(IterationPhase::Predict));
}

#[test]
fn accumulated_grids_visit_cs_dctrcurv_values() {
    use spice_analysis::sweep::{SweepSpec, SweepTarget};
    let spec = |target, start, stop, step| SweepSpec {
        target,
        start,
        stop,
        step,
    };
    let v = || SweepTarget::VoltageSource("vc".into());
    // C: value += step while sign(step) (value - stop) <= 1e3 DBL_EPSILON.
    let up = spec(v(), 0., 2., 0.1).accumulated_grid().unwrap();
    let mut value = 0.;
    let mut want = vec![];
    for _ in 0..21 {
        want.push(value);
        value += 0.1;
    }
    assert_eq!(up, want);
    assert_eq!(up[10], 0.9999999999999999);
    assert_eq!(up[20], 2.0000000000000004);
    // The exact grid differs there; continuous devices keep using it.
    assert_eq!(spec(v(), 0., 2., 0.1).grid().unwrap()[10], 1.);
    let down = spec(v(), 3., 0., -0.1).accumulated_grid().unwrap();
    assert_eq!(down.len(), 31);
    assert!(down[30].abs() < 1e-13);
    // Binary-exact steps are the same either way.
    let exact = spec(v(), 3., -3., -0.125);
    assert_eq!(exact.accumulated_grid().unwrap(), exact.grid().unwrap());
    // Temperatures accumulate in kelvin (CKTtemp) and are reported in Celsius.
    let t = spec(SweepTarget::Temperature, 27., 28., 0.1)
        .accumulated_grid()
        .unwrap();
    let mut kelvin: f64 = 27. + 273.15;
    let mut want = vec![];
    while (kelvin - 273.15) - 28. <= 1e3 * f64::EPSILON {
        want.push(kelvin - 273.15);
        kelvin += 0.1;
    }
    assert_eq!(t, want);
    // C's absolute stop test: the kelvin rounding leaves 28 C just out of
    // reach, so C (and the port) stop at the tenth step.
    assert_eq!(t.len(), 10);
    // The grid's input validation still applies.
    assert!(spec(v(), 0., 1., 0.).accumulated_grid().is_err());
    assert!(spec(v(), 0., 1., -0.1).accumulated_grid().is_err());
    assert!(
        spec(SweepTarget::Resistor("r1".into()), 1., -1., -1.)
            .accumulated_grid()
            .is_err()
    );
}

#[test]
fn decimal_sweeps_decide_thresholds_at_cs_accumulated_values() {
    // C's tenth value of `0 2 0.1` is 0.9999999999999999: a VT = 1 switch
    // (no band) is still open there and closes at 1.1 V.
    let plot = run(
        "vc c 0 0\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
         .model sm sw(vt=1 ron=10 roff=1meg)",
        AnalysisKind::DcSweep,
        &["vc", "0", "2", "0.1"],
    )
    .unwrap();
    let sweep = column(&plot, "sweep");
    assert_eq!(sweep[10], 0.9999999999999999);
    let a = column(&plot, "v(a)");
    close(a[10], OPEN, 1e-12);
    close(a[11], CLOSED, 1e-12);
}

#[test]
fn nested_inner_sweeps_restart_from_the_instance_flags() {
    // Inner vc 1 .. 3 (band 0.5 .. 1.5 V, OFF), outer vdd 1, 2. The first
    // inner sweep closes at 1.75 V and stays closed up to 3 V; the second
    // starts again from the OFF flag at 1 V (open inside the band), as C's
    // `firstTime` restart does, instead of continuing the closed state.
    let plot = run(
        "vc c 0 0\nvdd vdd 0 1\nr1 vdd a 1k\ns1 a 0 c 0 swh\n\
         .model swh sw(vt=1 vh=0.5 ron=10 roff=1meg)",
        AnalysisKind::DcSweep,
        &["vc", "1", "3", "0.25", "vdd", "1", "2", "1"],
    )
    .unwrap();
    let a = column(&plot, "v(a)");
    assert_eq!(a.len(), 18);
    for (k, scale) in [(0, 1.), (9, 2.)] {
        for value in &a[k..k + 3] {
            close(*value, scale * OPEN, 1e-12);
        }
        close(a[k + 3], scale * CLOSED, 1e-12);
        close(a[k + 8], scale * CLOSED, 1e-12);
    }
}
