//! S/W switch elaboration, Newton phases and accepted-state ownership through
//! the production circuit API (GitHub #81). C references: `swload.c`,
//! `cswload.c`, `swtrunc.c`, `swacload.c`.
use std::path::Path;

use spice_core::{SpiceError, SpiceResult};
use spice_devices::{
    AnalysisMode, Circuit, IterationPhase, LoadRequest, ModelContext, StateHistory, TrialState,
    TruncationContext,
};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, source::parse_deck_text};

fn circuit(body: &str) -> SpiceResult<Circuit> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("switch.cir"),
        &format!("switch\n{body}\n.end\n"),
    ))?;
    Circuit::from_netlist(&netlist)
}

/// One switch `s1` between `a` and ground (fed from `in` through 1k) controlled
/// by node `c`: unknowns are v(c), v(in), v(a), i(vc), i(vin).
const BAND: &str = "vc c 0 0\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
                    .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)";

fn solution(c: &Circuit, control: f64) -> Vector {
    let mut x = Vector::zeros(c.unknown_count());
    let row = c.unknowns().node_row(c.nodes().get("c").unwrap()).unwrap();
    x.as_mut_slice()[row] = control;
    x
}

/// Loads `c` at control voltage `control` in `phase` after `previous`,
/// returning the trial and the stamped switch conductance (A[a][a] - 1 mS).
fn load(
    c: &Circuit,
    history: &StateHistory,
    control: f64,
    phase: IterationPhase,
    previous: Option<&TrialState>,
) -> (TrialState, f64) {
    let n = c.unknown_count();
    let mut matrix = SparseMatrix::new(n, n);
    let mut rhs = Vector::zeros(n);
    let mut trial = history.trial_in(phase, previous).unwrap();
    c.load(
        &LoadRequest {
            mode: AnalysisMode::OperatingPoint,
            solution: &solution(c, control),
            model_context: &ModelContext::default(),
            integration: None,
            history,
            forcing: None,
        },
        &mut matrix,
        &mut rhs,
        &mut trial,
    )
    .unwrap();
    matrix.fold_duplicates();
    let row = c.unknowns().node_row(c.nodes().get("a").unwrap()).unwrap();
    (trial, matrix.get(row, row) - 1e-3)
}

fn switch_state(c: &Circuit, trial: &TrialState) -> f64 {
    let rows = c.state_rows(c.device_count() - 1).unwrap();
    trial.values()[rows.start]
}

fn close(got: f64, want: f64) {
    // Conductances are read back as A[a][a] - 1 mS: allow that rounding.
    assert!(
        (got - want).abs() <= 1e-9 * want.abs() + 1e-18,
        "{got} != {want}"
    );
}

#[test]
fn switches_elaborate_with_c_defaults_and_two_state_slots() {
    let c = circuit(BAND).unwrap();
    let s1 = c.device("s1").unwrap();
    assert_eq!(s1.designator(), 's');
    assert_eq!(s1.terminals().len(), 4);
    assert_eq!(s1.state_count(), 2);
    assert!(s1.is_nonlinear());
    assert_eq!(s1.branch_currents(), 0);
    // W senses the branch of a voltage source, resolved after elaboration.
    let w = circuit("vs a 0 0\nw1 b 0 vs wm\nr1 b 0 1\n.model wm csw").unwrap();
    assert_eq!(
        w.control_rows(1).unwrap(),
        w.branch_rows(0).unwrap().collect::<Vec<_>>()
    );
    // Defaults: RON 1 ohm, ROFF = gmin, VT/VH = 0 (on above 0 V).
    let d = circuit("vc c 0 0\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\n.model sm sw").unwrap();
    let history = d.state_history();
    let (_, open) = load(&d, &history, -1., IterationPhase::Predict, None);
    close(open, ModelContext::default().gmin);
    let (_, closed) = load(&d, &history, 1., IterationPhase::Predict, None);
    close(closed, 1.);
}

#[test]
fn invalid_switch_instances_and_models_fail_before_elaboration() {
    for (body, message) in [
        ("s1 a 0 c 0 nomodel", "not defined"),
        ("s1 a 0 c 0 dm\n.model dm d", "wrong model family"),
        ("w1 a 0 v1 sm\nv1 b 0 0\n.model sm sw", "wrong model family"),
        ("s1 a 0 c 0 sm\n.model sm sw(ron=0)", "NonZero"),
        (
            "s1 a 0 c 0 sm\n.model sm sw(it=1)",
            "unsupported setter 'it'",
        ),
        (
            "w1 a 0 v1 wm\nv1 b 0 0\n.model wm csw(vt=1)",
            "unsupported setter 'vt'",
        ),
        ("s1 a 0 c 0 sm\n.model sm sw(level=1)", "'level'"),
        (
            "s1 a 0 c 0 sm\n.model sm sw(ron=1e-320)",
            "no finite conductance",
        ),
        (
            "w1 a 0 vx wm\n.model wm csw",
            "unknown controlling source vx",
        ),
        (
            "w1 a 0 r1 wm\nr1 a 0 1\n.model wm csw",
            "no findable branch current",
        ),
    ] {
        let error = circuit(body).unwrap_err();
        let text = error.to_string();
        assert!(text.contains(message), "{body}: {text}");
    }
}

#[test]
fn trial_loads_never_flip_the_accepted_state() {
    let c = circuit(BAND).unwrap();
    let mut history = c.state_history();
    // Operating point at 0 V: really off (code 0), committed as accepted.
    let (op, g) = load(&c, &history, 0., IterationPhase::Junction, None);
    close(g, 1e-6);
    let x = solution(&c, 0.);
    c.accept_point(&x, Some(0.), &mut history, op).unwrap();
    // A predicted trial at 2 V closes the switch in the trial only.
    let (trial, g) = load(&c, &history, 2., IterationPhase::Predict, None);
    close(g, 0.1);
    assert_eq!(switch_state(&c, &trial), 1.);
    // Rejecting it (dropping the trial) leaves the accepted state open: a
    // retried, shorter step inside the band keeps the switch open.
    drop(trial);
    assert_eq!(history.accepted(1).unwrap()[0], 0.);
    let (retry, g) = load(&c, &history, 1.2, IterationPhase::Predict, None);
    close(g, 1e-6);
    // Only accepting a closed point changes what later trials continue from.
    let (closed, _) = load(&c, &history, 1.6, IterationPhase::Predict, None);
    c.accept_point(&solution(&c, 1.6), Some(1e-6), &mut history, closed)
        .unwrap();
    let (inside, g) = load(&c, &history, 1.2, IterationPhase::Predict, None);
    close(g, 0.1);
    assert_eq!(switch_state(&c, &inside), 1.);
    assert_eq!(switch_state(&c, &retry), 0.);
}

#[test]
fn hysteresis_and_threshold_crossings_follow_swload() {
    let c = circuit(BAND).unwrap();
    let mut history = c.state_history();
    let mut sweep = |controls: &[f64]| -> Vec<bool> {
        controls
            .iter()
            .map(|&v| {
                let (trial, g) = load(&c, &history, v, IterationPhase::Predict, None);
                c.accept_point(&solution(&c, v), Some(0.), &mut history, trial)
                    .unwrap();
                g > 1e-3
            })
            .collect()
    };
    // Rising: closes only above VT + VH = 1.5 V (1.5 itself is in the band).
    assert_eq!(
        sweep(&[0., 0.6, 1.0, 1.4, 1.5, 1.51, 1.2]),
        [false, false, false, false, false, true, true]
    );
    // Falling: opens only below VT - VH = 0.5 V.
    assert_eq!(sweep(&[0.9, 0.5, 0.49, 1.0]), [true, true, false, false]);
}

#[test]
fn initial_flags_decide_inside_the_band_and_float_flips_report_nonconvergence() {
    let on = circuit(&BAND.replace(" sm\n", " sm on\n")).unwrap();
    let history = on.state_history();
    // MODEINITJCT inside the band: the ON flag closes the switch (code 3).
    let (first, g) = load(&on, &history, 1.0, IterationPhase::Junction, None);
    close(g, 0.1);
    assert_eq!(switch_state(&on, &first), 3.);
    assert!(!first.is_nonconvergent());
    // MODEINITFLOAT keeps the previous iterate inside the band (no change).
    let (kept, _) = load(&on, &history, 1.2, IterationPhase::Float, Some(&first));
    assert_eq!(switch_state(&on, &kept), 3.);
    assert!(!kept.is_nonconvergent());
    // Leaving the band changes the state: C's CKTnoncon++.
    let (flipped, g) = load(&on, &history, 0.2, IterationPhase::Float, Some(&kept));
    close(g, 1e-6);
    assert!(flipped.is_nonconvergent());
    // The nonconvergence flag and the iterate are never part of a commit.
    let mut committed = history.clone();
    on.accept_point(&solution(&on, 0.2), None, &mut committed, flipped)
        .unwrap();
    assert_eq!(committed.accepted(1).unwrap()[0], 0.);
    // A float load must have a previous iterate.
    let n = on.unknown_count();
    let mut orphan = history.trial_in(IterationPhase::Float, None).unwrap();
    let error = on
        .load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: &solution(&on, 1.),
                model_context: &ModelContext::default(),
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut SparseMatrix::new(n, n),
            &mut Vector::zeros(n),
            &mut orphan,
        )
        .unwrap_err();
    assert!(matches!(error, SpiceError::Circuit { .. }), "{error}");
}

#[test]
fn current_switches_sense_their_controlling_branch() {
    // i(vs) is the unknown in the branch row; the switch closes above IT+IH.
    let c = circuit(
        "vin in 0 1\nr1 in a 1k\nw1 a 0 vs wm\nvs s 0 0\nrs s 0 1\n\
         .model wm csw(it=1m ih=0.2m ron=5)",
    )
    .unwrap();
    let history = c.state_history();
    let branch = c.branch_rows(3).unwrap().start;
    let n = c.unknown_count();
    let a = c.unknowns().node_row(c.nodes().get("a").unwrap()).unwrap();
    for (current, closed) in [(1.3e-3, true), (1.1e-3, false), (-5e-3, false)] {
        let mut x = Vector::zeros(n);
        x.as_mut_slice()[branch] = current;
        let mut matrix = SparseMatrix::new(n, n);
        let mut trial = history.trial_in(IterationPhase::Predict, None).unwrap();
        c.load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: &x,
                model_context: &ModelContext::default(),
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut matrix,
            &mut Vector::zeros(n),
            &mut trial,
        )
        .unwrap();
        matrix.fold_duplicates();
        let g = matrix.get(a, a) - 1e-3;
        close(g, if closed { 0.2 } else { 1e-12 });
        // Slot 1 records the control value.
        assert_eq!(trial.values()[1], current);
    }
}

#[test]
fn timestep_limits_follow_swtrunc() {
    let c = circuit(BAND).unwrap();
    let s1 = c.device("s1").unwrap();
    let limit = |code: f64, now: f64, before: f64| {
        s1.timestep_limit(&TruncationContext {
            trial: &[code, now],
            accepted: Some(&[0., before]),
            dt: 1e-6,
        })
        .unwrap()
    };
    // Really off, rising towards VT + VH = 1.5: (0.75 (1.5 - 1) + 0.05) / 0.2 dt.
    close(
        limit(0., 1.0, 0.8).unwrap(),
        (0.75 * 0.5 + 0.05) / 0.2 * 1e-6,
    );
    // Falling while off, or already past the reference: no limit.
    assert_eq!(limit(0., 1.0, 1.2), None);
    assert_eq!(limit(0., 1.6, 1.4), None);
    // Any other state (C tests `== 0` only, so off-in-band too) bounds a
    // falling control towards VT - VH = 0.5.
    for code in [1., 2., 3.] {
        close(
            limit(code, 1.0, 1.1).unwrap(),
            (0.75 * -0.5 - 0.05) / -0.1 * 1e-6,
        );
        assert_eq!(limit(code, 1.0, 0.9), None);
    }
    // No accepted point yet: nothing to compare with.
    assert_eq!(
        s1.timestep_limit(&TruncationContext {
            trial: &[0., 1.],
            accepted: None,
            dt: 1e-6,
        })
        .unwrap(),
        None
    );
    // W never limits: CSWload leaves cswtrunc.c's control slot unwritten.
    let w = circuit("vs a 0 0\nw1 b 0 vs wm\nr1 b 0 1\n.model wm csw(it=1)").unwrap();
    assert_eq!(
        w.device("w1")
            .unwrap()
            .timestep_limit(&TruncationContext {
                trial: &[0., 0.9],
                accepted: Some(&[0., 0.]),
                dt: 1e-6,
            })
            .unwrap(),
        None
    );
}

#[test]
fn small_signal_conductance_follows_the_bias_state() {
    let c = circuit(BAND).unwrap();
    let history = c.state_history();
    let a = c.unknowns().node_row(c.nodes().get("a").unwrap()).unwrap();
    // The solved bias state (closed) is used, not a reload from the flags.
    let (closed, _) = load(&c, &history, 2., IterationPhase::Predict, None);
    let bias = solution(&c, 0.);
    let system = c
        .small_signal_system_at(&ModelContext::default(), &bias, Some(&closed))
        .unwrap();
    close(system.a.get(a, a) - 1e-3, 0.1);
    // Without a solved state (zero-bias assemblies) the MODEINITJCT state.
    let system = c
        .small_signal_system(&ModelContext::default(), &bias)
        .unwrap();
    close(system.a.get(a, a) - 1e-3, 1e-6);
    // The switch has no immutable linear form: the BDF assembly refuses it.
    let mut c = c;
    assert!(c.linear_system().is_err());
    assert!(
        c.small_signal_system_at(
            &ModelContext::default(),
            &bias,
            Some(&StateHistory::new(1).trial())
        )
        .is_err()
    );
}
