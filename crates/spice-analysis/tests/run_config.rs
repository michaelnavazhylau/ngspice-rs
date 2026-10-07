//! #16 run configuration: precedence, validation and per-deck isolation.
use spice_analysis::{AnalysisContext, AnalysisRequest, RunConfig, RunOverrides, runner};
use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::path::Path;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("cfg.cir"),
            &format!("t\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn config(options: &str) -> Result<RunConfig, SpiceError> {
    RunConfig::from_netlist(&deck(&format!("r1 a 0 1k\n{options}")))
}

#[test]
fn defaults_are_unchanged() {
    let c = config("").unwrap();
    assert_eq!(c.context(), AnalysisContext::default());
    assert_eq!(c.context().temperature, 27.0);
    assert_eq!(c.context().nominal_temperature, 27.0);
    assert_eq!(*c.transient(), Default::default());
    assert!(c.applied().is_empty() && c.method().is_none() && c.maxord().is_none());
    let tran = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "1m", "backend=diffsol", "method=bdf"],
    );
    assert_eq!(c.request(tran.clone()).unwrap(), tran);
}

#[test]
fn ordered_overrides_are_case_insensitive_and_last_wins() {
    let c =
        config(".options TEMP=10 tnom=20\n.OPTION Temp=50 RELTOL=1e-4\n.opt reltol=2e-4").unwrap();
    assert_eq!(c.context().temperature, 50.0);
    assert_eq!(c.context().nominal_temperature, 20.0);
    assert_eq!(c.transient().rtol, Some(2e-4));
    assert_eq!(c.applied().len(), 5);
    assert_eq!(c.applied()[0].name, "temp");
    assert_eq!(c.applied()[0].value, "10");
}

#[test]
fn overrides_beat_deck_and_request_arguments_beat_deck() {
    let overrides = RunOverrides {
        temperature: Some(85.0),
        nominal_temperature: None,
    };
    let n = deck("r1 a 0 1k\n.options temp=10 tnom=20 reltol=1e-3 vntol=2u abstol=3p");
    let c = RunConfig::from_options(&n.options, &overrides).unwrap();
    assert_eq!(c.context().temperature, 85.0);
    assert_eq!(c.context().nominal_temperature, 20.0);
    assert!(
        RunConfig::from_options(
            &n.options,
            &RunOverrides {
                temperature: Some(f64::NAN),
                ..Default::default()
            }
        )
        .is_err()
    );

    let request = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "1m", "backend=diffsol", "method=bdf", "RTOL=5e-5"],
    );
    let merged = c.request(request).unwrap();
    assert_eq!(merged.named("rtol"), Some("5e-5"));
    assert_eq!(merged.named("vntol"), Some("2e-6"));
    assert_eq!(merged.named("abstol"), Some("3e-12"));
    // Non-transient requests are untouched.
    let op = AnalysisRequest::new(AnalysisKind::OperatingPoint);
    assert_eq!(c.request(op.clone()).unwrap(), op);
}

#[test]
fn invalid_values_are_errors() {
    for options in [
        ".options temp=-273.15",
        ".options temp=-300",
        ".options temp=hot",
        ".options reltol=0",
        ".options reltol=-1e-3",
        ".options vntol=abc",
        ".options abstol=0",
        ".options maxord=0",
        ".options maxord=7",
        ".options maxord=2.5",
        ".options maxord=gear",
        ".options method=euler",
        ".options method=2",
        ".options temp",
        ".options reltol",
        ".options method",
    ] {
        let error = config(options).expect_err(options);
        assert!(!error.is_not_yet_ported(), "{options}: {error}");
    }
}

#[test]
fn unknown_unimplemented_and_conflicting_options_are_errors() {
    for options in [
        ".options bogus=1",
        ".options no_auto_gnd",
        ".options RELTO=1",
    ] {
        assert!(
            matches!(config(options), Err(SpiceError::Parse { .. })),
            "{options}"
        );
    }
    for options in [
        ".options chgtol=1e-14",
        ".options trtol=7",
        ".options itl4=20",
        ".options gmin=1e-12",
        ".options list",
        ".options reltol=1m noopiter",
    ] {
        assert!(
            config(options).unwrap_err().is_not_yet_ported(),
            "{options}"
        );
    }
    let error = config(".options reltol=1m\n.options reltol").unwrap_err();
    assert!(
        error.to_string().contains("flag and with a value"),
        "{error}"
    );
}

#[test]
fn method_and_maxord_are_retained_but_rejected_for_transient() {
    let c = config(".options method=Gear maxord=2").unwrap();
    assert_eq!((c.method(), c.maxord()), (Some("gear"), Some(2)));
    let tran = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "1m", "backend=diffsol", "method=bdf"],
    );
    for options in [
        ".options method=trap",
        ".options method=trapezoidal",
        ".options method=gear",
        ".options method=gear maxord=4",
        ".options maxord=2",
    ] {
        let c = config(options).unwrap();
        let error = c.request(tran.clone()).unwrap_err();
        assert!(
            matches!(error, SpiceError::Unsupported { .. }),
            "{options}: {error}"
        );
        // Analyses that do not integrate are unaffected by the retained selection.
        let ac = AnalysisRequest::new(AnalysisKind::Ac);
        assert_eq!(c.request(ac.clone()).unwrap(), ac);
    }
}

#[test]
fn deck_options_take_effect_in_a_real_transient_and_do_not_leak() {
    let rc = "v1 in 0 dc 1\nr1 in out 1k\nc1 out 0 1u\n.tran 1m 5m backend=diffsol method=bdf";
    let loose = deck(&format!("{rc}\n.options reltol=0.5 vntol=1 abstol=1"));
    let strict = deck(rc);
    let run = |n: &Netlist| {
        let c = RunConfig::from_netlist(n).unwrap();
        let mut circuit = c.circuit(n).unwrap();
        let request = c.request_for(&n.analyses[0]).unwrap();
        let plot =
            runner(AnalysisKind::Transient)
                .unwrap()
                .run(&mut circuit, &request, &c.context());
        (c, plot)
    };
    let (c_loose, loose_plot) = run(&loose);
    let (c_strict, strict_plot) = run(&strict);
    assert_eq!(c_loose.transient().rtol, Some(0.5));
    assert_eq!(c_strict.transient().rtol, None);
    // Both decks simulate; the strict deck is unaffected by the earlier one.
    assert!(
        loose_plot.is_ok() && strict_plot.is_ok(),
        "{loose_plot:?} {strict_plot:?}"
    );
    assert_eq!(c_strict.context(), AnalysisContext::default());
}

#[test]
fn temperatures_reach_elaboration_and_plain_elaboration_refuses_options() {
    let n = deck("r1 a 0 rm\n.model rm r(r=1k tc1=0.01 tnom=27)\nv1 a 0 1\n.options temp=127");
    let c = RunConfig::from_netlist(&n).unwrap();
    let mut hot = c.circuit(&n).unwrap();
    let mut cold = RunConfig::default().circuit(&n).unwrap();
    let op = |circuit: &mut spice_devices::Circuit, ctx: &AnalysisContext| {
        runner(AnalysisKind::OperatingPoint)
            .unwrap()
            .run(
                circuit,
                &AnalysisRequest::new(AnalysisKind::OperatingPoint),
                ctx,
            )
            .unwrap()
            .value("i(v1)", 0)
            .unwrap()
            .re
    };
    let hot_i = op(&mut hot, &c.context());
    let cold_i = op(&mut cold, &AnalysisContext::default());
    assert!((hot_i - cold_i).abs() > 1e-9, "{hot_i} {cold_i}");
    assert!(matches!(
        spice_devices::Circuit::from_netlist(&n),
        Err(SpiceError::Unsupported { .. })
    ));
}

#[test]
fn top_level_globals_are_accepted_by_flat_elaboration() {
    let n = deck("v1 a 0 1\nr1 a gnd 1k\n.global a gnd");
    assert!(RunConfig::from_netlist(&n).is_ok());
    assert!(spice_devices::Circuit::from_netlist(&n).is_ok());
}
