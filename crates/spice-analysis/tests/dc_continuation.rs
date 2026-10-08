//! #34 DC continuation policy: bounded, typed, reported and physically honest.
//!
//! Every success must solve the *original* equations (full sources, zero
//! artificial nodal gmin); continuation never accepts device state, runs accept
//! hooks or edits a source value. Transient/diffsol do not read these options.
use spice_analysis::bias::{
    AttemptOutcome, ContinuationPolicy, DEFAULT_GMIN_SCHEDULE, DcOutcome, DcReport, DcSettings,
    DcStrategy, MAX_GMIN_STAGES, MAX_SOURCE_STEPS, MAX_TOTAL_ITERATIONS, SourceStepping, solve_dc,
    solve_dc_with,
};
use spice_analysis::newton::NewtonOptions;
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use spice_core::{AnalysisKind, SpiceError, SpiceResult};
use spice_devices::{AnalysisMode, Circuit, LoadRequest, ModelContext};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

const VT: f64 = (1.38064852e-23 / 1.6021766208e-19) * 300.15;
/// 1 mA through a 1 mA-saturation diode: direct Newton needs more than four
/// iterations, while source stepping converges within four at every stage.
const DIFFICULT: &str = "i1 0 a 1m\nd1 a 0 dm\n.model dm d(is=1m)";
/// The same hard bias, with an AC unit drive so an `.ac` run must solve it first.
const DIFFICULT_AC: &str = "i1 0 a dc 1m ac 1\nd1 a 0 dm\n.model dm d(is=1m)";

fn parse(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("dc.cir"),
            &format!("DC continuation\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&parse(body)).unwrap()
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}
fn bounded() -> NewtonOptions {
    NewtonOptions {
        max_iterations: 4,
        ..NewtonOptions::default()
    }
}
fn settings(newton: NewtonOptions, continuation: ContinuationPolicy) -> DcSettings {
    DcSettings {
        newton,
        continuation,
    }
}
fn attempts(report: &DcReport) -> Vec<(DcStrategy, AttemptOutcome)> {
    report
        .attempts
        .iter()
        .map(|attempt| (attempt.strategy, attempt.outcome))
        .collect()
}
/// The unmodified diode equation `Id(v) = 1 mA`, including the fixed 1e-12 S
/// junction gmin, evaluated independently of the solver.
fn diode_residual(v: f64) -> f64 {
    1e-3 * ((v / VT).exp() - 1.) + 1e-12 * v - 1e-3
}
fn assert_original_equations_solved(report: &DcReport) {
    let last = report.stages.last().unwrap();
    assert!(last.converged() && last.is_unregularized(), "{last:?}");
    assert_eq!(
        report.total_iterations,
        report.stages.iter().map(|s| s.iterations).sum::<usize>()
    );
    assert!(report.total_iterations <= report.budget);
}
fn dc_rhs(c: &Circuit) -> Vec<f64> {
    c.small_signal_system(&ModelContext::default(), &Vector::zeros(c.unknown_count()))
        .unwrap()
        .dc_rhs(None)
        .unwrap()
        .as_slice()
        .to_vec()
}

/// `v(a)^3 = i` with a singular Jacobian at the zero seed. Counts accept hooks.
#[derive(Debug)]
struct Cubic {
    nodes: [spice_core::NodeId; 2],
    accepted: Rc<Cell<usize>>,
}
impl spice_devices::Device for Cubic {
    fn name(&self) -> &str {
        "x1"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[spice_core::NodeId] {
        &self.nodes
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn stamp(&self, context: &mut spice_devices::StampContext<'_>) -> SpiceResult<()> {
        let [a, b] = self.nodes;
        let v = context.node_voltage(a) - context.node_voltage(b);
        let g = 3. * v * v;
        context.stamp(a, a, g)?;
        context.stamp(b, b, g)?;
        context.stamp(a, b, -g)?;
        context.stamp(b, a, -g)?;
        context.stamp_rhs(a, g * v - v * v * v)?;
        context.stamp_rhs(b, v * v * v - g * v)
    }
    fn assemble_small_signal(
        &self,
        context: &mut spice_devices::LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let v = context
            .unknowns
            .node_row(self.nodes[0])
            .and_then(|r| bias.get(r))
            .unwrap_or(0.);
        context.nodal(self.nodes, 3. * v * v, false)
    }
    fn accept(&self, _: &spice_devices::AcceptContext<'_>) -> SpiceResult<()> {
        self.accepted.set(self.accepted.get() + 1);
        Ok(())
    }
}
fn cubic(accepted: &Rc<Cell<usize>>) -> Circuit {
    let mut c = circuit("i1 0 a 1");
    let a = c.nodes().get("a").unwrap();
    c.add_device(Box::new(Cubic {
        nodes: [a, spice_core::NodeId::GROUND],
        accepted: Rc::clone(accepted),
    }))
    .unwrap();
    c.finalize().unwrap();
    c
}

/// A nonlinear device that reports unimplemented physics rather than failing to
/// converge. Continuation must surface that error unchanged instead of retrying
/// it against a gmin or source-stepped equation set.
#[derive(Debug)]
struct Rejecting {
    nodes: [spice_core::NodeId; 2],
    accepted: Rc<Cell<usize>>,
}
impl spice_devices::Device for Rejecting {
    fn name(&self) -> &str {
        "x1"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[spice_core::NodeId] {
        &self.nodes
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn stamp(&self, _: &mut spice_devices::StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "x1 junction physics",
            "src/spicelib/devices/x/xload.c",
        ))
    }
    fn assemble_linear(&self, context: &mut spice_devices::LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.nodes, 1e-3, false)
    }
    fn accept(&self, _: &spice_devices::AcceptContext<'_>) -> SpiceResult<()> {
        self.accepted.set(self.accepted.get() + 1);
        Ok(())
    }
}

#[test]
fn structural_device_errors_are_not_retried_by_continuation() {
    let accepted = Rc::new(Cell::new(0));
    let mut c = circuit("v1 a 0 1\nr1 a 0 1k");
    let a = c.nodes().get("a").unwrap();
    c.add_device(Box::new(Rejecting {
        nodes: [a, spice_core::NodeId::GROUND],
        accepted: Rc::clone(&accepted),
    }))
    .unwrap();
    c.finalize().unwrap();
    let context = ModelContext::default();
    let failure = solve_dc_with(&c, &context, &DcSettings::default(), &[], None, None).unwrap_err();
    assert!(failure.error.is_not_yet_ported(), "{}", failure.error);
    assert_eq!(failure.report.outcome, DcOutcome::NonRetryableFailure);
    // Exactly the direct attempt and its single unregularized stage: the gmin and
    // source-stepping strategies in the default policy were never started.
    assert_eq!(failure.report.attempts.len(), 1);
    assert_eq!(failure.report.attempts[0].outcome, AttemptOutcome::Failed);
    assert_eq!(failure.report.stages.len(), 1);
    let stage = &failure.report.stages[0];
    assert_eq!(stage.strategy, DcStrategy::Direct);
    assert!(stage.is_unregularized(), "{stage:?}");
    assert_eq!(accepted.get(), 0);
    let default_policy = ContinuationPolicy::default();
    assert!(
        default_policy.source_stepping.is_some() && !default_policy.gmin_schedule.is_empty(),
        "this test only proves skipping when the default policy would otherwise continue"
    );
}

fn run_ac(options: &str, explicit: &[&str]) -> SpiceResult<Plot> {
    let netlist = parse(&format!("{DIFFICULT_AC}\n{options}"));
    let config = RunConfig::from_netlist(&netlist)?;
    let mut c = config.circuit(&netlist)?;
    let mut arguments = vec!["lin", "1", "1k", "1k"];
    arguments.extend(explicit);
    let request = config.request(AnalysisRequest::with_arguments(AnalysisKind::Ac, arguments))?;
    runner(AnalysisKind::Ac)?.run(&mut c, &request, &config.context())
}

#[test]
fn ac_bias_deck_continuation_options_use_last_set_wins_and_request_precedence() {
    // Deck-only, both continuations disabled: the same hard bias that fails for
    // `.op` fails here, with the same bounded diagnosis.
    let error = run_ac(".options itl1=4 srcsteps=0 gminsteps=0", &[]).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("source stepping: disabled"), "{message}");
    assert!(message.contains("gmin stepping: disabled"), "{message}");
    // Deck source stepping alone rescues the budget, exactly as for `.op`, and
    // the AC point is linearized about the same physical full-source bias.
    let solved = run_ac(".options itl1=4 gminsteps=0", &[]).unwrap();
    close(
        solved.value("v(a)", 0).unwrap().re,
        1. / (2. * (1e-3 / VT)),
        1e-6,
        1e-9,
    );
    // Last deck occurrence wins: the later `srcsteps=20` re-enables the strategy.
    assert!(run_ac(".options itl1=4 gminsteps=0 srcsteps=0 srcsteps=20", &[]).is_ok());
    // An explicit request argument outranks the deck value.
    assert!(run_ac(".options itl1=4 gminsteps=0", &["srcsteps=0"]).is_err());
}

#[test]
fn difficult_diode_fails_bounded_direct_newton_but_the_configured_policy_solves_it() {
    let c = circuit(DIFFICULT);
    let context = ModelContext::default();
    let run = |continuation| {
        solve_dc_with(
            &c,
            &context,
            &settings(bounded(), continuation),
            &[],
            None,
            None,
        )
    };

    // Direct Newton alone exhausts four iterations; nothing else is tried.
    let failure = run(ContinuationPolicy::disabled()).unwrap_err();
    assert!(matches!(failure.error, SpiceError::Numerical { .. }));
    assert_eq!(failure.report.outcome, DcOutcome::Exhausted);
    assert_eq!(
        attempts(&failure.report),
        [
            (DcStrategy::Direct, AttemptOutcome::Failed),
            (DcStrategy::GminStepping, AttemptOutcome::Disabled),
            (DcStrategy::SourceStepping, AttemptOutcome::Disabled),
        ]
    );
    assert_eq!(failure.report.stages.len(), 1);
    assert_eq!(failure.report.stages[0].iterations, 4);
    let message = failure.error.to_string();
    assert!(message.contains("gmin stepping: disabled"), "{message}");
    assert!(message.contains("source stepping: disabled"), "{message}");
    assert!(message.contains("iteration limit"), "{message}");

    // The gmin schedule alone cannot rescue it within the same iteration budget.
    let gmin_only = ContinuationPolicy {
        source_stepping: None,
        ..ContinuationPolicy::default()
    };
    let failure = run(gmin_only).unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::Exhausted);
    assert_eq!(failure.report.stages.len(), 2);
    assert_eq!(failure.report.stages[1].gmin, 1e-3);
    assert!(!failure.report.stages[1].converged());
    assert_eq!(
        attempts(&failure.report)[1],
        (DcStrategy::GminStepping, AttemptOutcome::Failed)
    );
    assert!(
        failure
            .error
            .to_string()
            .contains("source stepping: disabled")
    );

    // Configured source stepping (no gmin stepping) succeeds in the same budget.
    let source_only = ContinuationPolicy {
        gmin_schedule: Vec::new(),
        source_stepping: Some(SourceStepping::uniform(20, 1e-8).unwrap()),
        max_total_iterations: None,
    };
    let solved = run(source_only).unwrap();
    let report = &solved.report;
    assert_eq!(
        report.outcome,
        DcOutcome::Converged(DcStrategy::SourceStepping)
    );
    assert_eq!(
        attempts(report),
        [
            (DcStrategy::Direct, AttemptOutcome::Failed),
            (DcStrategy::GminStepping, AttemptOutcome::Disabled),
            (DcStrategy::SourceStepping, AttemptOutcome::Converged),
        ]
    );
    assert_eq!(report.stages.len(), 1 + 21 + 1);
    assert!(!report.stages[0].converged());
    assert!(
        report.stages[1..]
            .iter()
            .all(|s| s.converged() && s.iterations <= 4)
    );
    assert_eq!(
        (report.stages[1].source_scale, report.stages[1].gmin),
        (0., 1e-8)
    );
    assert_eq!(
        (report.stages[21].source_scale, report.stages[21].gmin),
        (1., 1e-8)
    );
    assert!(!report.stages[21].is_unregularized());
    assert_original_equations_solved(report);
    // Success is checked against the unmodified equations, not the solver's own.
    let v = solved.solution.values.as_slice()[0];
    close(v, VT * 2_f64.ln(), 1e-8, 1e-12);
    close(diode_residual(v) + 1e-3, 1e-3, 1e-7, 1e-12);

    // The default policy tries gmin stepping first, then source stepping.
    let defaults = run(ContinuationPolicy::default()).unwrap();
    assert_eq!(
        attempts(&defaults.report),
        [
            (DcStrategy::Direct, AttemptOutcome::Failed),
            (DcStrategy::GminStepping, AttemptOutcome::Failed),
            (DcStrategy::SourceStepping, AttemptOutcome::Converged),
        ]
    );
    assert_original_equations_solved(&defaults.report);
    // The compatible wrapper is the same solve with the default policy.
    let wrapped = solve_dc(&c, &context, &bounded(), &[], None, None).unwrap();
    assert_eq!(
        wrapped.values.as_slice(),
        defaults.solution.values.as_slice()
    );
}

#[test]
fn configured_gmin_schedule_rescues_a_singular_jacobian_without_source_stepping() {
    let accepted = Rc::new(Cell::new(0));
    let c = cubic(&accepted);
    let context = ModelContext::default();
    let gmin_only = ContinuationPolicy {
        source_stepping: None,
        ..ContinuationPolicy::default()
    };
    let solved = solve_dc_with(
        &c,
        &context,
        &settings(NewtonOptions::default(), gmin_only),
        &[],
        None,
        None,
    )
    .unwrap();
    close(solved.solution.values.as_slice()[0], 1., 1e-8, 1e-12);
    assert_eq!(
        solved.report.outcome,
        DcOutcome::Converged(DcStrategy::GminStepping)
    );
    let scheduled: Vec<f64> = solved
        .report
        .stages
        .iter()
        .filter(|s| s.strategy == DcStrategy::GminStepping)
        .map(|s| s.gmin)
        .collect();
    let mut expected = DEFAULT_GMIN_SCHEDULE.to_vec();
    expected.push(0.);
    assert_eq!(scheduled, expected);
    assert!(!solved.report.stages[0].converged());
    assert_original_equations_solved(&solved.report);

    // A custom, shorter schedule is honoured exactly.
    let custom = ContinuationPolicy {
        gmin_schedule: vec![1e-2, 1e-6],
        source_stepping: None,
        max_total_iterations: None,
    };
    let solved = solve_dc_with(
        &c,
        &context,
        &settings(NewtonOptions::default(), custom),
        &[],
        None,
        None,
    )
    .unwrap();
    let gmins: Vec<f64> = solved.report.stages[1..].iter().map(|s| s.gmin).collect();
    assert_eq!(gmins, [1e-2, 1e-6, 0.]);
    close(solved.solution.values.as_slice()[0], 1., 1e-8, 1e-12);

    // With every schedule disabled the same singular start is a reported failure.
    let failure = solve_dc_with(
        &c,
        &context,
        &settings(NewtonOptions::default(), ContinuationPolicy::disabled()),
        &[],
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::Exhausted);
    assert_eq!(failure.report.stages.len(), 1);
    assert_eq!(accepted.get(), 0);
}

#[test]
fn exhausted_schedules_and_total_budget_are_bounded_and_reported() {
    let c = circuit(DIFFICULT);
    let context = ModelContext::default();
    // The budget covers direct Newton (4 iterations) plus two more: the gmin
    // stage is cut short, the attempt stops and nothing further runs.
    let policy = ContinuationPolicy {
        max_total_iterations: Some(6),
        ..ContinuationPolicy::default()
    };
    let failure =
        solve_dc_with(&c, &context, &settings(bounded(), policy), &[], None, None).unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::BudgetExhausted);
    assert_eq!(failure.report.budget, 6);
    assert_eq!(failure.report.total_iterations, 6);
    assert_eq!(failure.report.stages.len(), 2);
    assert_eq!(failure.report.stages[1].iterations, 2);
    assert_eq!(failure.report.attempts.len(), 2);
    assert!(
        failure.error.to_string().contains("budget"),
        "{}",
        failure.error
    );

    // A one-iteration budget leaves nothing for continuation.
    let policy = ContinuationPolicy {
        max_total_iterations: Some(1),
        ..ContinuationPolicy::default()
    };
    let failure =
        solve_dc_with(&c, &context, &settings(bounded(), policy), &[], None, None).unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::BudgetExhausted);
    assert!(failure.report.total_iterations <= 1);

    // Without an explicit budget the work is still bounded by stages * maxiter.
    let failure = solve_dc_with(
        &c,
        &context,
        &settings(bounded(), ContinuationPolicy::disabled()),
        &[],
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(failure.report.budget, 4);
    let defaults = solve_dc_with(
        &c,
        &context,
        &settings(bounded(), ContinuationPolicy::default()),
        &[],
        None,
        None,
    )
    .unwrap();
    assert_eq!(defaults.report.budget, (1 + 11 + 22) * 4);
}

#[test]
fn invalid_schedules_scales_and_budgets_are_rejected_before_any_load() {
    let c = circuit(DIFFICULT);
    let context = ModelContext::default();
    let bad_gmin: [Vec<f64>; 8] = [
        vec![f64::NAN],
        vec![f64::INFINITY],
        vec![0.],
        vec![-1e-3],
        vec![2.],
        vec![1e-3, 1e-3],
        vec![1e-6, 1e-3],
        vec![1e-3; MAX_GMIN_STAGES + 1],
    ];
    let mut policies: Vec<ContinuationPolicy> = bad_gmin
        .into_iter()
        .map(|gmin_schedule| ContinuationPolicy {
            gmin_schedule,
            ..ContinuationPolicy::default()
        })
        .collect();
    let scales = |scales: &[f64], gmin: f64| ContinuationPolicy {
        source_stepping: Some(SourceStepping {
            scales: scales.to_vec(),
            gmin,
        }),
        ..ContinuationPolicy::default()
    };
    let bad_scales: [&[f64]; 7] = [
        &[],
        &[f64::NAN],
        &[f64::INFINITY],
        &[-0.1, 1.],
        &[0., 1.5],
        &[0., 0.5, 0.5],
        &[0.5, 0.25],
    ];
    for bad in bad_scales {
        policies.push(scales(bad, 1e-8));
    }
    for gmin in [f64::NAN, -1e-8, 2.] {
        policies.push(scales(&[0., 1.], gmin));
    }
    for total in [0, MAX_TOTAL_ITERATIONS + 1] {
        policies.push(ContinuationPolicy {
            max_total_iterations: Some(total),
            ..ContinuationPolicy::default()
        });
    }
    for (index, policy) in policies.into_iter().enumerate() {
        assert!(policy.validate().is_err(), "policy {index}");
        let failure =
            solve_dc_with(&c, &context, &settings(bounded(), policy), &[], None, None).unwrap_err();
        assert_eq!(
            failure.report.outcome,
            DcOutcome::Rejected,
            "policy {index}"
        );
        assert!(failure.report.stages.is_empty() && failure.report.attempts.is_empty());
    }
    assert!(SourceStepping::uniform(0, 1e-8).is_err());
    assert!(SourceStepping::uniform(MAX_SOURCE_STEPS + 1, 1e-8).is_err());
    assert!(SourceStepping::uniform(MAX_SOURCE_STEPS, 1e-8).is_ok());
    assert!(ContinuationPolicy::geometric_gmin(1e-3, 1., 3).is_err());
    assert!(ContinuationPolicy::geometric_gmin(1e-3, 0.5, 3).is_err());
    assert!(ContinuationPolicy::geometric_gmin(1e-3, f64::NAN, 3).is_err());
    assert!(ContinuationPolicy::geometric_gmin(1e-3, 10., MAX_GMIN_STAGES + 1).is_err());
    assert!(ContinuationPolicy::geometric_gmin(1e-3, 1e6, MAX_GMIN_STAGES).is_err());
    assert!(ContinuationPolicy::geometric_gmin(f64::NAN, 10., 3).is_err());
    // Newton options are validated by the same entry point.
    let broken = NewtonOptions {
        max_iterations: 0,
        ..NewtonOptions::default()
    };
    assert!(
        solve_dc_with(
            &c,
            &context,
            &settings(broken, ContinuationPolicy::default()),
            &[],
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn default_policy_is_the_historical_schedule_and_geometric_builders_are_exact() {
    let default = ContinuationPolicy::default();
    assert_eq!(default.gmin_schedule, DEFAULT_GMIN_SCHEDULE);
    let source = default.source_stepping.as_ref().unwrap();
    assert_eq!(source.gmin, 1e-8);
    assert_eq!(
        source.scales,
        (0..=20).map(|k| f64::from(k) / 20.).collect::<Vec<_>>()
    );
    assert_eq!(default.max_total_iterations, None);
    assert_eq!(
        ContinuationPolicy::from_steps(None, None, None).unwrap(),
        default
    );
    assert_eq!(
        ContinuationPolicy::from_steps(Some(20), Some(10), Some(10.)).unwrap(),
        default
    );
    assert_eq!(
        ContinuationPolicy::geometric_gmin(1e-3, 2., 3).unwrap(),
        [1e-3, 1e-3 / 2., 1e-3 / 4.]
    );
    let none = ContinuationPolicy::from_steps(Some(0), Some(0), None).unwrap();
    assert_eq!(none, ContinuationPolicy::disabled());
    let four = ContinuationPolicy::from_steps(Some(4), Some(3), None).unwrap();
    assert_eq!(four.gmin_schedule, DEFAULT_GMIN_SCHEDULE[..3].to_vec());
    assert_eq!(
        four.source_stepping.unwrap().scales,
        [0., 0.25, 0.5, 0.75, 1.]
    );
    // A factor alone rebuilds the default number of stages.
    let wide = ContinuationPolicy::from_steps(None, None, Some(100.)).unwrap();
    assert_eq!(wide.gmin_schedule.len(), 10);
    close(wide.gmin_schedule[1], 1e-5, 1e-14, 0.);
}

#[test]
fn impossible_ideal_source_loops_fail_boundedly_without_a_regularized_answer() {
    let context = ModelContext::default();
    // Nonlinear circuit: the KVL rows of two parallel ideal sources stay
    // singular under every artificial gmin and every source scale.
    let c = circuit("v1 a 0 1\nv2 a 0 2\nd1 a 0 dm\n.model dm d");
    for continuation in [
        ContinuationPolicy::default(),
        ContinuationPolicy::disabled(),
    ] {
        let failure = solve_dc_with(
            &c,
            &context,
            &settings(NewtonOptions::default(), continuation),
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(failure.report.outcome, DcOutcome::Exhausted));
        assert!(!failure.report.stages.is_empty());
        assert!(
            failure
                .report
                .stages
                .iter()
                .all(|s| !(s.converged() && s.is_unregularized()))
        );
        assert!(failure.report.total_iterations <= failure.report.budget);
    }
    // Linear circuits keep the exact solve and report its failure unchanged.
    let linear = circuit("v1 a 0 1\nv2 a 0 2");
    let failure =
        solve_dc_with(&linear, &context, &DcSettings::default(), &[], None, None).unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::NonRetryableFailure);
    assert_eq!(failure.report.attempts.len(), 1);
    assert_eq!(failure.report.attempts[0].outcome, AttemptOutcome::Failed);
    assert!(
        solve_dc(
            &linear,
            &context,
            &NewtonOptions::default(),
            &[],
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn final_unregularized_physical_failure_is_never_masked_by_a_regularized_solve() {
    // A floating diode has no DC reference: every gmin-regularized stage
    // converges (to zero), but the original equations are singular.
    let c = circuit("d1 a b dm\n.model dm d");
    let failure = solve_dc_with(
        &c,
        &ModelContext::default(),
        &DcSettings::default(),
        &[],
        None,
        None,
    )
    .unwrap_err();
    let report = &failure.report;
    assert_eq!(report.outcome, DcOutcome::Exhausted);
    let gmin: Vec<_> = report
        .stages
        .iter()
        .filter(|s| s.strategy == DcStrategy::GminStepping)
        .collect();
    assert_eq!(gmin.len(), 11);
    assert!(gmin[..10].iter().all(|s| s.gmin > 0. && s.converged()));
    let last = gmin[10];
    assert!(last.is_unregularized() && !last.converged());
    assert!(report.stages.iter().any(|s| s.gmin > 0. && s.converged()));
    assert!(
        report
            .stages
            .iter()
            .all(|s| !(s.is_unregularized() && s.converged()))
    );
    let message = failure.error.to_string();
    assert!(message.contains("gmin stepping: failed"), "{message}");
    assert!(message.contains("source stepping: failed"), "{message}");
}

#[test]
fn easy_nonlinear_and_linear_solves_stay_direct_and_exact() {
    let context = ModelContext::default();
    let c = circuit("v1 a 0 0.6\nd1 a 0 dm\n.model dm d");
    let solved = solve_dc_with(&c, &context, &DcSettings::default(), &[], None, None).unwrap();
    assert_eq!(
        solved.report.outcome,
        DcOutcome::Converged(DcStrategy::Direct)
    );
    assert_eq!(
        attempts(&solved.report),
        [(DcStrategy::Direct, AttemptOutcome::Converged)]
    );
    assert_eq!(solved.report.stages.len(), 1);
    assert_original_equations_solved(&solved.report);
    close(solved.solution.values.as_slice()[0], 0.6, 1e-9, 1e-12);
    // A disabled policy changes nothing when direct Newton already converges.
    let direct = solve_dc_with(
        &c,
        &context,
        &settings(NewtonOptions::default(), ContinuationPolicy::disabled()),
        &[],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        direct.solution.values.as_slice(),
        solved.solution.values.as_slice()
    );
    assert_eq!(direct.solution.iterations, solved.solution.iterations);

    let linear = circuit("v1 a 0 1\nr1 a b 1k\nr2 b 0 1k");
    let solved = solve_dc_with(&linear, &context, &DcSettings::default(), &[], None, None).unwrap();
    assert_eq!(solved.report.stages.len(), 1);
    assert_eq!(solved.solution.iterations, 1);
    close(solved.solution.values.as_slice()[1], 0.5, 1e-12, 1e-15);
    // Even a hostile policy cannot affect (or reject) a linear solve.
    let policy = ContinuationPolicy {
        gmin_schedule: Vec::new(),
        source_stepping: None,
        max_total_iterations: Some(1),
    };
    assert!(
        solve_dc_with(
            &linear,
            &context,
            &settings(bounded(), policy),
            &[],
            None,
            None
        )
        .is_ok()
    );
}

#[test]
fn trial_state_belongs_to_the_solution_and_history_is_untouched() {
    let c = circuit("v1 a 0 0.3\nd1 a 0 dm\n.model dm d(cjo=1u vj=1)");
    let context = ModelContext::default();
    let history_before = c.state_history();
    let rhs_before = dc_rhs(&c);
    let solved = solve_dc_with(&c, &context, &DcSettings::default(), &[], None, None).unwrap();
    assert_eq!(c.state_history(), history_before);
    assert_eq!(dc_rhs(&c), rhs_before);
    let history = c.state_history();
    let mut trial = history.trial();
    c.load(
        &LoadRequest {
            mode: AnalysisMode::OperatingPoint,
            solution: &solved.solution.values,
            model_context: &context,
            integration: None,
            history: &history,
            forcing: None,
        },
        &mut SparseMatrix::new(c.unknown_count(), c.unknown_count()),
        &mut Vector::zeros(c.unknown_count()),
        &mut trial,
    )
    .unwrap();
    assert_eq!(trial.values(), solved.solution.trial.values());
}

#[test]
fn continuation_never_accepts_state_or_edits_sources_on_success_or_failure() {
    let accepted = Rc::new(Cell::new(0));
    let c = cubic(&accepted);
    let context = ModelContext::default();
    let rhs_before = dc_rhs(&c);
    let history_before = c.state_history();

    // Success needs gmin continuation; an override is applied to a temporary RHS.
    let solved = solve_dc_with(
        &c,
        &context,
        &DcSettings::default(),
        &[("I1", 8.)],
        None,
        None,
    )
    .unwrap();
    close(solved.solution.values.as_slice()[0], 2., 1e-8, 1e-12);
    assert_eq!(
        solved.report.outcome,
        DcOutcome::Converged(DcStrategy::GminStepping)
    );
    assert!(solved.report.stages.len() > 1);
    assert_eq!(accepted.get(), 0);
    assert_eq!(dc_rhs(&c), rhs_before);
    assert_eq!(c.state_history(), history_before);

    // Failure (continuation disabled) leaves the same invariants.
    let failure = solve_dc_with(
        &c,
        &context,
        &settings(NewtonOptions::default(), ContinuationPolicy::disabled()),
        &[("i1", 8.)],
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::Exhausted);
    assert_eq!(accepted.get(), 0);
    assert_eq!(dc_rhs(&c), rhs_before);

    // Forcing replaces source values for the solve only (transient 0- bias).
    let mut forcing = Vector::zeros(c.unknown_count());
    forcing.as_mut_slice().copy_from_slice(&dc_rhs(&c));
    forcing.as_mut_slice().iter_mut().for_each(|v| *v *= 27.);
    let forced = solve_dc_with(
        &c,
        &context,
        &DcSettings::default(),
        &[],
        None,
        Some(&forcing),
    )
    .unwrap();
    close(forced.solution.values.as_slice()[0], 3., 1e-8, 1e-12);
    assert_eq!(dc_rhs(&c), rhs_before);

    // Only an explicit accept runs hooks, exactly once, after the solve.
    let plain = solve_dc(&c, &context, &NewtonOptions::default(), &[], None, None).unwrap();
    close(plain.values.as_slice()[0], 1., 1e-8, 1e-12);
    assert_eq!(accepted.get(), 0);
    c.accept_solution(&plain.values, None).unwrap();
    assert_eq!(accepted.get(), 1);

    // Invalid overrides/seeds are rejected before any stage.
    let bad_overrides: [&[(&str, f64)]; 3] = [
        &[("nosuch", 1.)],
        &[("i1", f64::NAN)],
        &[("i1", 1.), ("I1", 2.)],
    ];
    for overrides in bad_overrides {
        let failure =
            solve_dc_with(&c, &context, &DcSettings::default(), overrides, None, None).unwrap_err();
        assert_eq!(failure.report.outcome, DcOutcome::Rejected);
    }
    let seed = Vector::zeros(c.unknown_count() + 1);
    assert!(solve_dc_with(&c, &context, &DcSettings::default(), &[], Some(&seed), None).is_err());
}

fn request(arguments: &[&str]) -> AnalysisRequest {
    AnalysisRequest::with_arguments(AnalysisKind::OperatingPoint, arguments.iter().copied())
}

#[test]
fn request_arguments_resolve_and_invalid_ones_fail() {
    let resolved = DcSettings::from_request(&request(&[
        "maxiter=4",
        "RTOL=1e-6",
        "srcsteps=5",
        "gminsteps=3",
        "gminfactor=100",
    ]))
    .unwrap();
    assert_eq!(resolved.newton.max_iterations, 4);
    assert_eq!(resolved.newton.reltol, 1e-6);
    assert_eq!(resolved.continuation.gmin_schedule.len(), 3);
    for (got, want) in resolved
        .continuation
        .gmin_schedule
        .iter()
        .zip([1e-3, 1e-5, 1e-7])
    {
        close(*got, want, 1e-14, 0.);
    }
    assert_eq!(
        resolved.continuation.source_stepping.unwrap().scales,
        [0., 0.2, 0.4, 0.6, 0.8, 1.]
    );
    let off = DcSettings::from_request(&request(&["srcsteps=0", "gminsteps=0"])).unwrap();
    assert_eq!(off.continuation, ContinuationPolicy::disabled());
    assert_eq!(
        DcSettings::from_request(&request(&[])).unwrap(),
        DcSettings::default()
    );
    // Positional arguments are not options.
    assert!(DcSettings::from_request(&request(&["lin", "1"])).is_ok());
    for bad in [
        "srcsteps=1.5",
        "srcsteps=-1",
        "srcsteps=1001",
        "srcsteps=nan",
        "srcsteps=abc",
        "gminsteps=101",
        "gminsteps=0.5",
        "gminfactor=1",
        "gminfactor=0.5",
        "gminfactor=1e7",
        "gminfactor=inf",
        "maxiter=0",
        "maxiter=10001",
        "bogus=1",
    ] {
        assert!(DcSettings::from_request(&request(&[bad])).is_err(), "{bad}");
    }
    for duplicate in [
        ["srcsteps=1", "SRCSTEPS=2"],
        ["gminsteps=1", "gminsteps=2"],
        ["gminfactor=2", "gminfactor=3"],
        ["maxiter=1", "maxiter=2"],
    ] {
        assert!(
            DcSettings::from_request(&request(&duplicate)).is_err(),
            "{duplicate:?}"
        );
    }
    // A factor/steps pair whose schedule underflows is invalid as a whole.
    assert!(DcSettings::from_request(&request(&["gminfactor=1e6", "gminsteps=100"])).is_err());
    // Newton-only resolution refuses (instead of silently dropping) continuation names.
    let error = NewtonOptions::from_request(&request(&["srcsteps=3"])).unwrap_err();
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
    assert!(NewtonOptions::from_request(&request(&["maxiter=7"])).is_ok());
}

fn run_op(options: &str, explicit: &[&str]) -> SpiceResult<Plot> {
    let netlist = parse(&format!("{DIFFICULT}\n{options}"));
    let config = RunConfig::from_netlist(&netlist)?;
    let mut c = config.circuit(&netlist)?;
    let request = config.request(request(explicit))?;
    runner(AnalysisKind::OperatingPoint)?.run(&mut c, &request, &config.context())
}

#[test]
fn deck_options_resolve_with_last_set_wins_and_request_precedence() {
    // Defaults: nothing is added to a request.
    let plain = RunConfig::default();
    assert_eq!(plain.request(request(&[])).unwrap(), request(&[]));
    let ok = |plot: SpiceResult<Plot>| plot.unwrap().value("v(a)", 0).unwrap().re;

    // Deck-only: itl1=4 with every continuation disabled fails and says why.
    let error = run_op(".options itl1=4 srcsteps=0 gminsteps=0", &[]).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("source stepping: disabled"), "{message}");
    assert!(message.contains("gmin stepping: disabled"), "{message}");
    // Source stepping alone (gmin disabled) rescues the same bounded budget.
    let v = ok(run_op(".options itl1=4 gminsteps=0", &[]));
    close(v, VT * 2_f64.ln(), 1e-8, 1e-12);
    // Deck default options (no DC options) use the default 200 iterations.
    close(ok(run_op("", &[])), VT * 2_f64.ln(), 1e-8, 1e-12);

    // Duplicate setters: last wins, in deck order, across option cards.
    let off_then_on = run_op(
        ".options itl1=4 gminsteps=0 srcsteps=0\n.options srcsteps=20",
        &[],
    );
    close(ok(off_then_on), VT * 2_f64.ln(), 1e-8, 1e-12);
    let on_then_off = run_op(
        ".options itl1=4 gminsteps=0 srcsteps=20\n.options srcsteps=0",
        &[],
    );
    assert!(on_then_off.is_err());
    let netlist = parse(&format!(
        "{DIFFICULT}\n.options srcsteps=0\n.options SRCSTEPS=7 itl1=9\n.options itl1=11"
    ));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    assert_eq!(config.dc().srcsteps, Some(7));
    assert_eq!(config.dc().itl1, Some(11));
    assert_eq!(config.dc().gminsteps, None);
    assert_eq!(config.applied().len(), 4);

    // Explicit request arguments beat the deck.
    let deck = ".options itl1=4 gminsteps=0 srcsteps=0";
    assert!(run_op(deck, &[]).is_err());
    close(
        ok(run_op(deck, &["srcsteps=20"])),
        VT * 2_f64.ln(),
        1e-8,
        1e-12,
    );
    close(
        ok(run_op(deck, &["maxiter=200"])),
        VT * 2_f64.ln(),
        1e-8,
        1e-12,
    );
    let merged = RunConfig::from_netlist(&parse(&format!("{DIFFICULT}\n{deck} gminfactor=5")))
        .unwrap()
        .request(request(&["srcsteps=20", "MAXITER=9"]))
        .unwrap();
    assert_eq!(merged.named("srcsteps"), Some("20"));
    assert_eq!(merged.named("maxiter"), Some("9"));
    assert_eq!(merged.named("gminsteps"), Some("0"));
    assert_eq!(merged.named("gminfactor"), Some("5e0"));
    // A request-only configuration (no deck) works the same way.
    assert!(run_op("", &["maxiter=4", "srcsteps=0", "gminsteps=0"]).is_err());
    close(
        ok(run_op("", &["maxiter=4", "gminsteps=0"])),
        VT * 2_f64.ln(),
        1e-8,
        1e-12,
    );
}

#[test]
fn deck_dc_options_are_validated_and_unimplemented_neighbours_still_fail() {
    let config = |options: &str| RunConfig::from_netlist(&parse(&format!("r1 a 0 1k\n{options}")));
    for bad in [
        ".options itl1=0",
        ".options itl1=10001",
        ".options itl1=2.5",
        ".options itl1=abc",
        ".options srcsteps=-1",
        ".options srcsteps=1001",
        ".options gminsteps=101",
        ".options gminfactor=1",
        ".options gminfactor=0.5",
        ".options gminfactor=abc",
        ".options gminfactor=1e7",
        ".options gminfactor=1e6 gminsteps=100",
        ".options itl1",
        ".options srcsteps",
    ] {
        let error = config(bad).expect_err(bad);
        assert!(!error.is_not_yet_ported(), "{bad}: {error}");
    }
    // Disabling is valid for the count options only.
    let off = config(".options srcsteps=0 gminsteps=0").unwrap();
    assert_eq!((off.dc().srcsteps, off.dc().gminsteps), (Some(0), Some(0)));
    assert_eq!(off.dc().policy().unwrap(), ContinuationPolicy::disabled());
    // Junction gmin, itl2 and itl4 are implemented (tests/run_config.rs);
    // diagonal gshunt remains an explicit gap.
    for pending in [".options gshunt=1e-12", ".options noopiter"] {
        assert!(
            config(pending).unwrap_err().is_not_yet_ported(),
            "{pending}"
        );
    }
    let error = config(".options itl1=5\n.options itl1").unwrap_err();
    assert!(!error.is_not_yet_ported(), "{error}");
}

#[test]
fn companion_transient_reads_dc_options_and_diffsol_rejects_them() {
    let config = RunConfig::from_netlist(&parse(
        "v1 a 0 1\nr1 a 0 1k\n.options itl1=50 srcsteps=4 gminsteps=3 gminfactor=20",
    ))
    .unwrap();
    // dctran.c computes the initial bias with CKTop(CKTdcMaxIter): the companion
    // driver receives the same settings as .op.
    let tran = config
        .request(AnalysisRequest::with_arguments(
            AnalysisKind::Transient,
            ["1u", "10u"],
        ))
        .unwrap();
    assert_eq!(tran.named("maxiter"), Some("50"));
    assert_eq!(tran.named("srcsteps"), Some("4"));
    assert_eq!(tran.named("gminsteps"), Some("3"));
    assert_eq!(tran.named("gminfactor"), Some("2e1"));
    let mut c = circuit("v1 a 0 1\nr1 a 0 1k");
    runner(AnalysisKind::Transient)
        .unwrap()
        .run(&mut c, &tran, &AnalysisContext::default())
        .unwrap();
    let diffsol = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "10u", "backend=diffsol", "method=bdf"],
    );
    let error = config.request(diffsol).unwrap_err();
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
    assert!(error.to_string().contains("itl1"), "{error}");
    // DC analyses accept them; invalid explicit .tran bias settings fail.
    assert!(config.request(request(&[])).is_ok());
    let error = runner(AnalysisKind::Transient)
        .unwrap()
        .run(
            &mut c,
            &AnalysisRequest::with_arguments(AnalysisKind::Transient, ["1u", "10u", "srcsteps=-5"]),
            &AnalysisContext::default(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("srcsteps"), "{error}");
}

#[test]
fn ac_bias_uses_the_configured_continuation() {
    let ac = |extra: &[&str]| {
        let mut c = circuit("i1 0 a dc 1m ac 1\nd1 a 0 dm\n.model dm d(is=1m)");
        let mut arguments = vec!["lin", "1", "1k", "1k"];
        arguments.extend(extra);
        runner(AnalysisKind::Ac).unwrap().run(
            &mut c,
            &AnalysisRequest::with_arguments(AnalysisKind::Ac, arguments),
            &AnalysisContext::default(),
        )
    };
    let solved = ac(&["maxiter=4", "gminsteps=0"]).unwrap();
    let conductance = 1e-3 / VT;
    close(
        solved.value("v(a)", 0).unwrap().re,
        1. / (2. * conductance),
        1e-6,
        1e-9,
    );
    assert!(ac(&["maxiter=4", "gminsteps=0", "srcsteps=0"]).is_err());
    assert!(ac(&["srcsteps=nan"]).is_err());
    assert!(ac(&["maxiter=4", "gminsteps=0", "srcsteps=0", "srcsteps=5"]).is_err());
}
