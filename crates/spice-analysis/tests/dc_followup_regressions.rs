//! Cross-lane integration and review regressions for #34 and #35.
use spice_analysis::bias::{AttemptOutcome, ContinuationPolicy, DcOutcome, DcSettings, DcStrategy};
use spice_analysis::sweep::{SweepSpec, SweepTarget};
use spice_analysis::{AnalysisContext, AnalysisRequest, RunConfig, runner};
use spice_core::AnalysisKind;
use spice_devices::{Circuit, ModelContext};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

fn circuit(body: &str) -> Circuit {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("followup.cir"),
            &format!("DC followup\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&netlist).unwrap()
}

#[test]
fn endpoint_roundoff_does_not_include_a_genuinely_unreachable_stop() {
    for (start, stop, step, expected) in [
        (0., 2.999_999_999_9, 1., vec![0., 1., 2.]),
        (3., 0.000_000_000_1, -1., vec![3., 2., 1.]),
    ] {
        let grid = SweepSpec {
            target: SweepTarget::VoltageSource("v1".into()),
            start,
            stop,
            step,
        }
        .grid()
        .unwrap();
        assert_eq!(grid, expected);
    }
    let grid = SweepSpec {
        target: SweepTarget::VoltageSource("v1".into()),
        start: 0.,
        stop: 99_998.999_999,
        step: 1.,
    }
    .grid()
    .unwrap();
    assert_eq!(grid.len(), 99_999);
    assert_eq!(grid.last(), Some(&99_998.));
}

#[test]
fn an_empty_geometric_schedule_still_validates_its_start() {
    for start in [f64::NAN, f64::INFINITY, 0., -1., 2.] {
        assert!(ContinuationPolicy::geometric_gmin(start, 10., 0).is_err());
    }
    assert!(
        ContinuationPolicy::geometric_gmin(1e-3, 10., 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_exact_linear_failure_records_the_attempt_without_continuation() {
    let c = circuit("v1 a 0 1\nv2 a 0 1\nr1 a 0 1k");
    let failure = spice_analysis::bias::solve_dc_with(
        &c,
        &ModelContext::default(),
        &DcSettings::default(),
        &[],
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(failure.report.outcome, DcOutcome::NonRetryableFailure);
    assert_eq!(failure.report.attempts.len(), 1);
    assert_eq!(failure.report.attempts[0].strategy, DcStrategy::Direct);
    assert_eq!(failure.report.attempts[0].outcome, AttemptOutcome::Failed);
    assert_eq!(failure.report.total_iterations, 1);
    assert!(failure.report.stages[0].error.is_some());
    assert!(failure.report.stages[0].is_unregularized());
}

#[test]
fn resistor_preflight_uses_the_actual_point_context_not_the_original_value() {
    // The original 1e308 ohm recipe is valid at 27 C, but overflows at 127 C.
    // Every requested replacement is valid there. Binding and preflight must
    // not reject the valid sweep by assembling the unused original recipe.
    for (args, context) in [
        (
            vec!["r1", "1k", "2k", "1k", "temp", "27", "127", "100"],
            AnalysisContext::default(),
        ),
        (
            vec!["r1", "1k", "2k", "1k"],
            AnalysisContext {
                temperature: 127.,
                ..Default::default()
            },
        ),
    ] {
        let mut c = circuit("v1 a 0 1\nr1 a 0 rm 1e308\n.model rm r(tc1=0.01)");
        let request = AnalysisRequest::with_arguments(AnalysisKind::DcSweep, args);
        let plot = runner(AnalysisKind::DcSweep)
            .unwrap()
            .run(&mut c, &request, &context)
            .unwrap();
        let currents = if plot.point_count() == 4 {
            vec![-1. / 1_000., -1. / 2_000., -1. / 2_000., -1. / 4_000.]
        } else {
            vec![-1. / 2_000., -1. / 4_000.]
        };
        for (row, expected) in currents.into_iter().enumerate() {
            assert!((plot.value("i(v1)", row).unwrap().re - expected).abs() < 1e-15);
        }
        assert_eq!(c.resistor("r1").unwrap().1.supplied, 1e308);
    }
}

#[test]
fn dc_points_consume_deck_continuation_controls_and_request_overrides() {
    let n = Parser::new().parse_deck(&parse_deck_text(Path::new("policy.cir"),
        "Policy sweep\ni1 0 a 1m\nd1 a 0 dm\n.model dm d(is=1m)\n.option gminsteps=0 srcsteps=20\n.dc i1 0 1m 1m maxiter=4\n.end\n"
    )).unwrap();
    // The four-iteration budget is a request argument: a deck `itl1` below 100
    // is C's effective 100 (niiter.c), which this bias needs no continuation for.
    let config = RunConfig::from_netlist(&n).unwrap();
    let request = config.request_for(&n.analyses[0]).unwrap();
    let mut c = config.circuit(&n).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap();
    assert_eq!(plot.point_count(), 2);
    let vt = (1.38064852e-23 / 1.6021766208e-19) * 300.15;
    assert!((plot.value("v(a)", 1).unwrap().re - vt * 2_f64.ln()).abs() < 1e-10);
    let mut explicit = AnalysisRequest::from(&n.analyses[0]);
    explicit.arguments.push("srcsteps=0".into());
    let disabled = config.request(explicit).unwrap();
    let mut c = config.circuit(&n).unwrap();
    let error = runner(disabled.kind)
        .unwrap()
        .run(&mut c, &disabled, &config.context())
        .unwrap_err();
    assert!(
        error.to_string().contains("source stepping: disabled"),
        "{error}"
    );
}
