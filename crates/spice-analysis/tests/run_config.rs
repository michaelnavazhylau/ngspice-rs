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
        ".options lteabstol=1e-6",
        ".options srcsteps=3",
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
fn method_and_maxord_reach_the_companion_driver_and_are_rejected_for_diffsol() {
    let c = config(".options method=Gear maxord=2").unwrap();
    assert_eq!((c.method(), c.maxord()), (Some("gear"), Some(2)));
    let diffsol = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "1m", "backend=diffsol", "method=bdf"],
    );
    let ordinary = AnalysisRequest::with_arguments(AnalysisKind::Transient, ["1u", "1m"]);
    for (options, forwarded) in [
        (".options method=trap", vec!["method=trap"]),
        (".options method=trapezoidal", vec!["method=trapezoidal"]),
        (".options method=gear", vec!["method=gear"]),
        (".options maxord=2", vec!["maxord=2"]),
        (
            ".options method=gear maxord=1",
            vec!["method=gear", "maxord=1"],
        ),
    ] {
        let c = config(options).unwrap();
        let error = c.request(diffsol.clone()).unwrap_err();
        assert!(
            matches!(error, SpiceError::Unsupported { .. }),
            "{options}: {error}"
        );
        let mut want = ordinary.clone();
        want.arguments
            .extend(forwarded.into_iter().map(String::from));
        assert_eq!(c.request(ordinary.clone()).unwrap(), want, "{options}");
        // Analyses that do not integrate are unaffected by the retained selection.
        let ac = AnalysisRequest::new(AnalysisKind::Ac);
        assert_eq!(c.request(ac.clone()).unwrap(), ac);
    }
    // An explicit request selection beats the deck's.
    let c = config(".options method=gear maxord=1").unwrap();
    let explicit = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "1m", "method=trap", "maxord=2"],
    );
    assert_eq!(c.request(explicit.clone()).unwrap(), explicit);
    // Gear orders above the implemented 2 are rejected up front.
    let error = config(".options method=gear maxord=4")
        .unwrap()
        .request(ordinary.clone())
        .unwrap_err();
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
    // trtol/chgtol are companion truncation options.
    let c = config(".options trtol=3 chgtol=1e-13").unwrap();
    assert_eq!(
        (c.transient().trtol, c.transient().chgtol),
        (Some(3.), Some(1e-13))
    );
    let request = c.request(ordinary.clone()).unwrap();
    assert_eq!(
        (request.named("trtol"), request.named("chgtol")),
        (Some("3e0"), Some("1e-13"))
    );
    assert!(c.request(diffsol).is_err());
    assert!(config(".options trtol=0").is_err());
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

#[test]
fn parsed_ic_and_nodeset_cards_travel_with_every_request() {
    // #27: the evaluated cards (braced expressions against .param, duplicates
    // in deck order) are attached to each analysis request instead of rejected.
    let netlist = deck(
        "r1 a 0 1k\n.param half=0.5\n.ic v(a)=1 v(a)={half*3}\n.nodeset v(a)=2\n.op\n.tran 1u 1m",
    );
    let config = RunConfig::from_netlist(&netlist).unwrap();
    for analysis in &netlist.analyses {
        let request = config.request_for(analysis).unwrap();
        let ic: Vec<_> = request
            .initial_conditions
            .iter()
            .map(|c| (c.node.as_str(), c.value))
            .collect();
        assert_eq!(ic, [("a", 1.), ("a", 1.5)]);
        assert_eq!(request.nodesets.len(), 1);
        assert_eq!(request.nodesets[0].value, 2.);
        assert_eq!(request.nodesets[0].location.line, 5);
    }
    // An undefined parameter is an error, not a silently dropped entry.
    let netlist = deck("r1 a 0 1k\n.ic v(a)={nope}\n.tran 1u 1m");
    assert!(RunConfig::from_netlist(&netlist).is_err());
}

#[test]
fn tran_uic_is_a_request_flag_the_diffsol_backend_rejects() {
    let netlist =
        deck("v1 a 0 0\nr1 a b 1k\nc1 b 0 1u\n.tran 1u 10u uic backend=diffsol method=bdf");
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    assert!(request.uic);
    assert!(!request.arguments.iter().any(|a| a == "uic"));
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(AnalysisKind::Transient)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(error.to_string().contains("uic"), "{error}");
}
