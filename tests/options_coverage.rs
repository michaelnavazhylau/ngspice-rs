//! #110 / #107: every `.option` name this port accepts either takes effect,
//! is a documented no-op (`RunConfig::ignored`) or fails explicitly; option
//! values may be `{expr}`/`'expr'` evaluated against top-level `.param`.
use ngspice_rs::analysis::{
    AnalysisContext, AnalysisRequest, Plot, RunConfig, RunOverrides, runner,
};
use ngspice_rs::netlist::{Parser, ast::Netlist, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, SpiceError};
use std::path::Path;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("opts.cir"),
            &format!("t\n{body}\n.end\n"),
        ))
        .unwrap_or_else(|error| panic!("{error}\n{body}"))
}

fn config(options: &str) -> Result<RunConfig, SpiceError> {
    RunConfig::from_netlist(&deck(&format!("r1 a 0 1k\n{options}")))
}

/// Run the deck's first analysis through `RunConfig`, as the CLI does.
fn simulate(body: &str) -> Result<Plot, SpiceError> {
    let n = deck(body);
    let c = RunConfig::from_netlist(&n)?;
    let request = c.request_for(&n.analyses[0])?;
    let mut circuit = c.circuit(&n)?;
    runner(request.kind)?.run(&mut circuit, &request, &c.context())
}

fn tran() -> AnalysisRequest {
    AnalysisRequest::with_arguments(AnalysisKind::Transient, ["1u", "1m"])
}

fn diffsol() -> AnalysisRequest {
    AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["1u", "1m", "backend=diffsol", "method=bdf"],
    )
}

fn dc() -> AnalysisRequest {
    AnalysisRequest::with_arguments(AnalysisKind::DcSweep, ["v1", "0", "1", "0.1"])
}

fn close(a: f64, b: f64, rel: f64, abs: f64) {
    assert!(
        (a - b).abs() <= rel * a.abs().max(b.abs()) + abs,
        "{a:e} vs {b:e}"
    );
}

// ---- options with an effect -------------------------------------------------

#[test]
fn gmin_is_the_junction_gmin_of_every_analysis() {
    let c = config(".options gmin=1u").unwrap();
    assert_eq!(c.context().gmin, 1e-6);
    assert_eq!(c.context().model_context().gmin, 1e-6);
    assert_eq!(config("").unwrap().context().gmin, 1e-12);
    // Reverse-biased diode: |I| = IS + gmin * 10 V (dioload.c adds CKTgmin).
    let reverse = |options: &str| {
        simulate(&format!(
            "v1 a 0 -10\nd1 a 0 dm\n.model dm d(is=1e-14)\n{options}\n.op"
        ))
        .unwrap()
        .value("i(v1)", 0)
        .unwrap()
        .re
    };
    close(reverse(".options gmin=1u").abs(), 1e-14 + 1e-5, 1e-6, 0.);
    close(reverse("").abs(), 1e-14 + 1e-11, 1e-6, 0.);
    close(reverse(".options gmin=0").abs(), 1e-14, 1e-6, 0.);
    // Small-signal: the same gmin is the reverse junction's conductance.
    let ac = simulate(
        "v1 a 0 -10 ac 1\nd1 a 0 dm\n.model dm d(is=1e-14)\n.options gmin=1u\n.ac lin 1 1k 1k",
    )
    .unwrap();
    close(ac.value("i(v1)", 0).unwrap().re.abs(), 1e-6, 1e-6, 0.);
    // Transient (companion and diffsol) see it through the same context.
    let tran =
        simulate("v1 a 0 -10\nd1 a 0 dm\n.model dm d(is=1e-14)\n.options gmin=1u\n.tran 1u 10u")
            .unwrap();
    let last = tran.point_count() - 1;
    close(
        tran.value("i(v1)", last).unwrap().re.abs(),
        1e-14 + 1e-5,
        1e-6,
        0.,
    );
    // BJT/MOS1 junctions use it as well (reverse-biased base-collector).
    let bjt = |options: &str| {
        simulate(&format!(
            "vc c 0 10\nq1 c 0 0 qm\n.model qm npn(is=1e-16)\n{options}\n.op"
        ))
        .unwrap()
        .value("i(vc)", 0)
        .unwrap()
        .re
        .abs()
    };
    // bjtload.c: gmin across base-collector plus the substrate gmin, which a
    // vertical NPN connects to the collector: 2 * 1 uS * 10 V.
    close(bjt(".options gmin=1u"), 2e-5, 1e-6, 1e-12);
    // A lateral PNP's substrate gmin connects to the base instead.
    let pnp = simulate("vc c 0 -10\nq1 c 0 0 qm\n.model qm pnp(is=1e-16)\n.options gmin=1u\n.op")
        .unwrap()
        .value("i(vc)", 0)
        .unwrap()
        .re
        .abs();
    close(pnp, 1e-5, 1e-6, 1e-12);
    assert!(bjt("") < 1e-9, "{}", bjt(""));
    let mos = |options: &str| {
        simulate(&format!(
            "vd d 0 5\nm1 d 0 0 0 mm\n.model mm nmos(vto=1)\n{options}\n.op"
        ))
        .unwrap()
        .value("i(vd)", 0)
        .unwrap()
        .re
        .abs()
    };
    assert!(
        mos(".options gmin=1u") > 4e-6,
        "{}",
        mos(".options gmin=1u")
    );
    for bad in [".options gmin=-1e-12", ".options gmin=abc", ".options gmin"] {
        let error = config(bad).unwrap_err();
        assert!(!error.is_not_yet_ported(), "{bad}: {error}");
    }
}

#[test]
fn itl1_reaches_dc_analyses_and_the_companion_initial_bias() {
    // niiter.c raises any limit below 100 to 100: the deck value is stored and
    // forwarded as C's effective limit.
    let c = config(".options itl1=37").unwrap();
    assert_eq!(c.dc().itl1, Some(100));
    for kind in [AnalysisKind::OperatingPoint, AnalysisKind::Ac] {
        let request = c.request(AnalysisRequest::new(kind)).unwrap();
        assert_eq!(request.named("maxiter"), Some("100"));
    }
    assert_eq!(c.request(dc()).unwrap().named("maxiter"), Some("100"));
    assert_eq!(c.request(tran()).unwrap().named("maxiter"), Some("100"));
    assert!(c.request(diffsol()).is_err());
    let above = config(".options itl1=250").unwrap();
    assert_eq!(above.request(dc()).unwrap().named("maxiter"), Some("250"));
    for low in ["0", "1", "99", "100"] {
        let c = config(&format!(".options itl1={low}")).unwrap();
        assert_eq!(c.dc().itl1, Some(100), "itl1={low}");
    }
    // C converges this deck with `itl1=2 gminsteps=0 srcsteps=0` (v(b) =
    // 0.69289, 5 iterations): so does the port, at the same bias.
    let deck = "v1 a 0 5\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n.op";
    let plain = simulate(deck).unwrap();
    let low = simulate(&format!("{deck}\n.options itl1=2 gminsteps=0 srcsteps=0")).unwrap();
    close(
        low.value("v(b)", 0).unwrap().re,
        plain.value("v(b)", 0).unwrap().re,
        1e-9,
        1e-12,
    );
    close(low.value("v(b)", 0).unwrap().re, 0.69289, 1e-4, 0.);
    // The same holds for the initial bias of a nonlinear transient.
    let tran_deck = "v1 a 0 5\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n.tran 1u 10u";
    assert!(
        simulate(&format!(
            "{tran_deck}\n.options itl1=1 gminsteps=0 srcsteps=0"
        ))
        .is_ok()
    );
    // The forwarded limit is a real bound: the literal request budget of one
    // iteration (below C's floor, so a port-only knob) fails with diagnostics.
    let n = deck_with_request(deck, &["maxiter=1", "gminsteps=0", "srcsteps=0"]);
    let error = n.unwrap_err();
    assert!(error.to_string().contains("Newton"), "{error}");
}

/// Run `body`'s first analysis with extra explicit request arguments.
fn deck_with_request(body: &str, extra: &[&str]) -> Result<Plot, SpiceError> {
    let n = deck(body);
    let c = RunConfig::from_netlist(&n)?;
    let mut request = AnalysisRequest::from(&n.analyses[0]);
    request
        .arguments
        .extend(extra.iter().map(|a| (*a).to_owned()));
    let request = c.request(request)?;
    let mut circuit = c.circuit(&n)?;
    runner(request.kind)?.run(&mut circuit, &request, &c.context())
}

#[test]
fn itl2_bounds_the_warm_started_dc_sweep_points() {
    let c = config(".options itl2=7").unwrap();
    assert_eq!(c.dc().itl2, Some(100));
    assert_eq!(c.request(dc()).unwrap().named("trcvmaxiter"), Some("100"));
    let above = config(".options itl2=300").unwrap();
    assert_eq!(
        above.request(dc()).unwrap().named("trcvmaxiter"),
        Some("300")
    );
    // The warm start exists only in .dc; the continuation stage limit reaches
    // every analysis that runs the DC bias (tests below).
    for request in [
        AnalysisRequest::new(AnalysisKind::OperatingPoint),
        AnalysisRequest::new(AnalysisKind::Ac),
        tran(),
    ] {
        let request = c.request(request).unwrap();
        assert_eq!(request.named("trcvmaxiter"), None);
        assert_eq!(request.named("stagemaxiter"), Some("100"));
    }
    // One large sweep step (0 V -> 5 V) on a diode: the first point is
    // trivial, the second needs several damped Newton iterations from the
    // previous point. With a three-iteration full-solve budget and no
    // continuation, the point fails unless the warm start takes it.
    let sweep = "v1 a 0 0\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n.dc v1 0 5 5";
    // `limiting=global continuation=ladder`: the legacy damped Newton and
    // ladder policies this budget was chosen for.
    let tight = [
        "maxiter=3",
        "gminsteps=0",
        "srcsteps=0",
        "limiting=global",
        "continuation=ladder",
    ];
    let error = deck_with_request(sweep, &tight).unwrap_err();
    assert!(error.to_string().contains("Newton"), "{error}");
    // A warm-start bound of 2 is too small too: it fails, falls back to the
    // three-iteration full solve, and that fails as well.
    let mut bounded = tight.to_vec();
    bounded.push("trcvmaxiter=2");
    assert!(deck_with_request(sweep, &bounded).is_err());
    // Deck itl2=5 is C's effective 100: the warm start converges the point,
    // although a literal bound of 5 (or the full-solve budget of 3) would not.
    let mut literal = tight.to_vec();
    literal.push("trcvmaxiter=5");
    assert!(deck_with_request(sweep, &literal).is_err());
    let plot = deck_with_request(&format!("{sweep}\n.options itl2=5"), &tight).unwrap();
    let reference = simulate(sweep).unwrap();
    assert_eq!(plot.point_count(), 2);
    close(
        plot.value("v(b)", 1).unwrap().re,
        reference.value("v(b)", 1).unwrap().re,
        1e-6,
        1e-9,
    );
    for bad in [".options itl2=1.5", ".options itl2=10001"] {
        assert!(!config(bad).unwrap_err().is_not_yet_ported(), "{bad}");
    }
    assert_eq!(config(".options itl2=0").unwrap().dc().itl2, Some(100));
    // The request key is validated by the sweep driver.
    let mut circuit = RunConfig::from_netlist(&deck(sweep))
        .unwrap()
        .circuit(&deck(sweep))
        .unwrap();
    let request = AnalysisRequest::with_arguments(
        AnalysisKind::DcSweep,
        ["v1", "0", "1", "0.5", "trcvmaxiter=0"],
    );
    let error = runner(AnalysisKind::DcSweep)
        .unwrap()
        .run(&mut circuit, &request, &AnalysisContext::default())
        .unwrap_err();
    assert!(error.to_string().contains("trcvmaxiter"), "{error}");
}

#[test]
fn itl2_bounds_every_continuation_stage_of_the_dc_bias() {
    // cktop.c: the direct solve runs NIiter(itl1), every dynamic/spice3 gmin
    // and source-stepping stage NIiter(itl2); CKTop serves .op, .ac, the .dc
    // first point and the .tran initial bias alike.
    let op = AnalysisRequest::new(AnalysisKind::OperatingPoint);
    let ac = AnalysisRequest::with_arguments(AnalysisKind::Ac, ["dec", "1", "1k", "10k"]);
    assert_eq!(
        config("")
            .unwrap()
            .request(op.clone())
            .unwrap()
            .named("stagemaxiter"),
        None
    );
    for (options, stage) in [
        (".options itl2=300", "300"),
        (".options itl2=7", "100"),
        // itl1 alone: C keeps the stages at its default itl2 (50 -> 100).
        (".options itl1=5000", "100"),
        (".options itl1=5000 itl2=250", "250"),
    ] {
        let c = config(options).unwrap();
        for request in [op.clone(), ac.clone(), dc(), tran()] {
            let kind = request.kind;
            let request = c.request(request).unwrap();
            assert_eq!(
                request.named("stagemaxiter"),
                Some(stage),
                "{options} {kind:?}"
            );
        }
    }
    // backend=diffsol runs no continuation ladder, so it cannot honour itl2.
    let error = config(".options itl2=300")
        .unwrap()
        .request(diffsol())
        .unwrap_err();
    assert!(error.to_string().contains("itl2"), "{error}");
    let c = config(".options itl1=5000").unwrap();
    assert_eq!(
        c.request(op.clone()).unwrap().named("maxiter"),
        Some("5000")
    );
    // Explicit request entries win.
    let mut explicit = op.clone();
    explicit.arguments.push("stagemaxiter=7".into());
    assert_eq!(
        c.request(explicit).unwrap().named("stagemaxiter"),
        Some("7")
    );

    // Observable: a diode the damped direct Newton cannot reach in three
    // iterations. With maxiter=3 and no deck limits every gmin stage gets
    // three iterations too and the bias fails; deck itl2=5 (C's effective
    // 100) gives the stages 100 and gmin stepping converges, in .op, .ac and
    // the .tran initial bias alike.
    let body = "v1 a 0 5 ac 1\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)";
    // `limiting=global continuation=ladder`: the legacy damped Newton and
    // ladder policies this budget was chosen for.
    let tight = [
        "maxiter=3",
        "srcsteps=0",
        "limiting=global",
        "continuation=ladder",
    ];
    let reference = simulate(&format!("{body}\n.op")).unwrap();
    let expected = reference.value("v(b)", 0).unwrap().re;
    for analysis in [".op", ".ac dec 1 1k 10k", ".tran 1u 2u"] {
        let plain = format!("{body}\n{analysis}");
        let error = deck_with_request(&plain, &tight).unwrap_err();
        assert!(error.to_string().contains("gmin"), "{analysis}: {error}");
        let limited = format!("{body}\n{analysis}\n.options itl2=5");
        let plot = deck_with_request(&limited, &tight).unwrap();
        if analysis == ".op" {
            close(plot.value("v(b)", 0).unwrap().re, expected, 1e-6, 1e-9);
        }
        // The stage limit is the bound, not the direct-solve limit: a literal
        // stagemaxiter=3 fails as before.
        let mut literal = tight.to_vec();
        literal.push("stagemaxiter=3");
        assert!(deck_with_request(&limited, &literal).is_err(), "{analysis}");
    }
}

#[test]
fn stage_iteration_limit_is_separate_from_the_direct_limit() {
    use ngspice_rs::analysis::bias::{DcSettings, DcStrategy, solve_dc_with};
    let n = deck("v1 a 0 5\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n.op");
    let circuit = RunConfig::from_netlist(&n).unwrap().circuit(&n).unwrap();
    let settings = DcSettings::from_request(&AnalysisRequest::with_arguments(
        AnalysisKind::OperatingPoint,
        [
            "maxiter=3",
            "stagemaxiter=40",
            "srcsteps=0",
            "continuation=ladder",
        ],
    ))
    .unwrap();
    assert_eq!(settings.newton.max_iterations, 3);
    assert_eq!(settings.continuation.stage_max_iterations, Some(40));
    let context = AnalysisContext::default().model_context();
    let solved = solve_dc_with(&circuit, &context, &settings, &[], None, None).unwrap();
    let stages = &solved.report.stages;
    assert_eq!(stages[0].strategy, DcStrategy::Direct);
    assert_eq!(stages[0].iterations, 3);
    assert!(
        stages[1..]
            .iter()
            .all(|s| s.strategy == DcStrategy::GminStepping)
    );
    assert!(stages[1..].iter().all(|s| s.iterations <= 40));
    assert!(stages[1..].iter().any(|s| s.iterations > 3), "{stages:?}");
    // Gmin stepping's closing zero-gmin solve is bounded by the direct limit
    // (itl1), not the stage limit: C's `spice3_gmin`/`dynamic_gmin`/`new_gmin`
    // end with `NIiter(ckt, iterlim)` (`cktop.c`).
    let closing = stages.last().unwrap();
    assert_eq!(closing.strategy, DcStrategy::GminStepping);
    assert_eq!(closing.gmin, 0.);
    assert!(closing.iterations <= 3, "{closing:?}");
    // Budget: the direct limit for the direct and closing solves, plus the
    // stage limit for every gmin stage in between.
    assert_eq!(solved.report.budget, 2 * 3 + (stages.len() - 2) * 40);
    for bad in ["stagemaxiter=0", "stagemaxiter=10001", "stagemaxiter=2.5"] {
        let request = AnalysisRequest::with_arguments(AnalysisKind::OperatingPoint, [bad]);
        assert!(DcSettings::from_request(&request).is_err(), "{bad}");
    }
}

#[test]
fn itl4_is_the_companion_newton_limit_per_timepoint() {
    let c = config(".options itl4=250").unwrap();
    assert_eq!(c.transient().itl4, Some(250));
    assert_eq!(c.request(tran()).unwrap().named("tranmaxiter"), Some("250"));
    // niiter.c: below 100 (including C's nominal default 10) means 100.
    for low in ["0", "3", "10", "99"] {
        let c = config(&format!(".options itl4={low}")).unwrap();
        assert_eq!(c.transient().itl4, Some(100), "itl4={low}");
    }
    assert_eq!(
        c.request(AnalysisRequest::new(AnalysisKind::OperatingPoint))
            .unwrap()
            .named("tranmaxiter"),
        None
    );
    let error = c.request(diffsol()).unwrap_err();
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
    let deck_text = "v1 a 0 pulse(0 5 1u 1u 1u 5u 20u)\nr1 a b 1k\nd1 b 0 dm\n\
                     .model dm d(is=1e-14)\n.tran 0.1u 3u";
    let stats = |options: &str, extra: &[&str]| {
        let n = deck(&format!("{deck_text}\n{options}"));
        let c = RunConfig::from_netlist(&n).unwrap();
        let mut request = AnalysisRequest::from(&n.analyses[0]);
        request
            .arguments
            .extend(extra.iter().map(|a| (*a).to_owned()));
        let request = c.request(request).unwrap();
        let mut circuit = c.circuit(&n).unwrap();
        ngspice_rs::analysis::companion_transient(&mut circuit, &request, &c.context())
            .unwrap()
            .1
    };
    // As in C (identical iteration/timepoint/rejection counts for itl4 =
    // 1/3/10/99), deck values below 100 change nothing.
    let default = stats("", &[]);
    for low in ["1", "3", "10", "99", "100"] {
        assert_eq!(stats(&format!(".options itl4={low}"), &[]), default);
    }
    // The limit is a real per-timepoint bound: the literal request knob below
    // C's floor forces more rejected trials (dctran.c cuts the step by 8 on
    // non-convergence), and explicit request arguments beat the deck.
    let starved = stats(".options itl4=200", &["tranmaxiter=3"]);
    assert!(
        starved.rejected > default.rejected,
        "{starved:?} vs {default:?}"
    );
    for bad in [".options itl4=2.5", ".options itl4", ".options itl4=10001"] {
        assert!(!config(bad).unwrap_err().is_not_yet_ported(), "{bad}");
    }
}

#[test]
fn xmu_weights_the_companion_trapezoidal_rule() {
    let c = config(".options xmu=0.3").unwrap();
    assert_eq!(c.transient().xmu, Some(0.3));
    assert_eq!(c.request(tran()).unwrap().named("xmu"), Some("3e-1"));
    assert!(c.request(diffsol()).is_err());
    let rc = "v1 a 0 pulse(0 1 0 1n 1n 1 2)\nr1 a b 1k\nc1 b 0 1u\n.tran 10u 2m";
    let at = |plot: &Plot, t: f64| {
        let time = plot.column("time").unwrap();
        let i = time.iter().position(|v| v.re >= t).unwrap();
        plot.value("v(b)", i).unwrap().re
    };
    let default = simulate(rc).unwrap();
    let explicit = simulate(&format!("{rc}\n.options xmu=0.5")).unwrap();
    assert_eq!(default, explicit);
    let damped = simulate(&format!("{rc}\n.options xmu=0.2")).unwrap();
    assert_ne!(damped, default);
    // Both remain accurate RC charging curves.
    for plot in [&default, &damped] {
        close(at(plot, 1e-3), 1. - (-1f64).exp(), 0.02, 0.);
    }
    for bad in [".options xmu=0.6", ".options xmu=-0.1", ".options xmu=abc"] {
        assert!(!config(bad).unwrap_err().is_not_yet_ported(), "{bad}");
    }
}

#[test]
fn itl6_is_srcsteps() {
    let c = config(".options itl6=3").unwrap();
    assert_eq!(c.dc().srcsteps, Some(3));
    assert_eq!(c.applied()[0].name, "itl6");
    assert_eq!(
        c.request(AnalysisRequest::new(AnalysisKind::OperatingPoint))
            .unwrap()
            .named("srcsteps"),
        Some("3")
    );
    // One setting: last occurrence wins across both spellings.
    assert_eq!(
        config(".options itl6=3 srcsteps=5").unwrap().dc().srcsteps,
        Some(5)
    );
    assert_eq!(
        config(".options srcsteps=5 itl6=0").unwrap().dc().srcsteps,
        Some(0)
    );
    assert!(config(".options itl6=1001").is_err());
}

#[test]
fn existing_effective_options_are_still_applied() {
    let c = config(
        ".options temp=50 tnom=20 reltol=1e-4 vntol=2u abstol=3p chgtol=1e-13 trtol=5 \
         method=gear maxord=1 srcsteps=2 gminsteps=4 gminfactor=5",
    )
    .unwrap();
    assert_eq!(c.context().temperature, 50.);
    assert_eq!(c.context().nominal_temperature, 20.);
    let t = c.transient();
    assert_eq!(
        (t.rtol, t.vntol, t.abstol, t.chgtol, t.trtol),
        (Some(1e-4), Some(2e-6), Some(3e-12), Some(1e-13), Some(5.))
    );
    assert_eq!((c.method(), c.maxord()), (Some("gear"), Some(1)));
    assert_eq!(
        (c.dc().srcsteps, c.dc().gminsteps, c.dc().gminfactor),
        (Some(2), Some(4), Some(5.))
    );
    assert_eq!(c.applied().len(), 12);
    assert!(c.ignored().is_empty());
}

// ---- documented no-ops ------------------------------------------------------

#[test]
fn print_control_flags_are_documented_no_ops() {
    for flag in [
        "acct",
        "noacct",
        "list",
        "nomod",
        "nopage",
        "node",
        "opts",
        "noinit",
        "norefvalue",
    ] {
        let c = config(&format!(".options {flag}")).unwrap();
        assert!(c.applied().is_empty(), "{flag}");
        assert_eq!(c.ignored().len(), 1, "{flag}");
        assert_eq!(c.ignored()[0].name, flag);
        assert_eq!(c.ignored()[0].value, None);
        assert!(c.ignored()[0].reason.contains("print"), "{flag}");
        // Nothing reaches any request.
        for request in [tran(), AnalysisRequest::new(AnalysisKind::OperatingPoint)] {
            assert_eq!(c.request(request.clone()).unwrap(), request, "{flag}");
        }
        let error = config(&format!(".options {flag}=1")).unwrap_err();
        assert!(matches!(error, SpiceError::Parse { .. }), "{flag}: {error}");
    }
    // A run with every flag is identical to one without.
    let rc = "v1 a 0 1\nr1 a b 1k\nc1 b 0 1u\n.tran 10u 1m";
    assert_eq!(
        simulate(&format!(
            "{rc}\n.options acct noacct list nomod nopage node opts noinit norefvalue"
        ))
        .unwrap(),
        simulate(rc).unwrap()
    );
}

#[test]
fn options_c_ignores_are_documented_no_ops() {
    for (name, value) in [
        ("itl3", "4"),
        ("itl5", "5000"),
        ("cptime", "1e3"),
        ("limtim", "2"),
        ("limpts", "201"),
        ("lvlcod", "1"),
        ("lvltim", "2"),
    ] {
        let c = config(&format!(".options {name}={value}")).unwrap();
        assert!(c.applied().is_empty(), "{name}");
        assert_eq!(c.ignored()[0].value.as_deref(), Some(value));
        assert!(
            c.ignored()[0].reason.contains("ignored by ngspice"),
            "{name}"
        );
        assert!(config(&format!(".options {name}=abc")).is_err(), "{name}");
        assert!(config(&format!(".options {name}")).is_err(), "{name}");
    }
    assert!(config(".options itl3=2.5").is_err());
    assert!(config(".options cptime=2.5").is_ok());
}

#[test]
fn unread_frontend_variables_and_bypass_are_documented_no_ops() {
    for options in [
        ".options post",
        ".options post=2",
        ".options ingold=2",
        ".options post=csdf",
    ] {
        let c = config(options).unwrap();
        assert_eq!(c.ignored().len(), 1, "{options}");
        assert!(c.ignored()[0].reason.contains("nothing reads"), "{options}");
    }
    let c = config(".options bypass=0").unwrap();
    assert!(c.ignored()[0].reason.contains("never bypasses"));
    assert!(config(".options bypass=1").unwrap_err().is_not_yet_ported());
    assert!(!config(".options bypass").unwrap_err().is_not_yet_ported());
}

#[test]
fn pivot_thresholds_are_not_yet_ported() {
    // C sets Sparse's TSKpivotAbsTol/TSKpivotRelTol from them (cktsopt.c);
    // this port's LU has no matching knob, so they are explicit gaps rather
    // than no-ops.
    for options in [
        ".options pivtol=1e-13",
        ".options pivrel=1e-3",
        ".options pivtol={1e-13}",
    ] {
        let error = config(options).unwrap_err();
        assert!(error.is_not_yet_ported(), "{options}: {error}");
        assert!(error.to_string().contains("piv"), "{error}");
    }
}

#[test]
fn unknown_and_unported_options_still_fail() {
    for unknown in [".options bogus=1", ".options postt", ".options itl7=1"] {
        assert!(
            matches!(config(unknown), Err(SpiceError::Parse { .. })),
            "{unknown}"
        );
    }
    for pending in [
        ".options gshunt=1e-12",
        ".options cshunt=1p",
        ".options minbreak=1n",
        ".options numdgt=8",
        ".options filetype=ascii",
        ".options savecurrents",
        ".options scale=1u",
        ".options seed=5",
        ".options klu",
        ".options keepopinfo",
    ] {
        assert!(
            config(pending).unwrap_err().is_not_yet_ported(),
            "{pending}"
        );
    }
}

// ---- expression values (#107) -------------------------------------------------

#[test]
fn braced_and_quoted_option_values_evaluate_against_params() {
    let c = config(
        ".param t=40 g=1u half=0.5\n.options temp={t+10} tnom='t-5' gmin={g/10} \
         reltol={2*1m} itl1={10*15} xmu={half/2} cptime='half'",
    )
    .unwrap();
    assert_eq!(c.context().temperature, 50.);
    assert_eq!(c.context().nominal_temperature, 35.);
    close(c.context().gmin, 1e-7, 1e-12, 0.);
    close(c.transient().rtol.unwrap(), 2e-3, 1e-12, 0.);
    assert_eq!(c.dc().itl1, Some(150));
    assert_eq!(c.transient().xmu, Some(0.25));
    assert_eq!(c.applied()[0].value, "{t+10}");
    assert_eq!(c.applied()[1].value, "'t-5'");
    assert_eq!(c.ignored()[0].value.as_deref(), Some("'half'"));
    // Parameters defined after the option card are visible (one root scope).
    let late = config(".options temp={t}\n.param t=60").unwrap();
    assert_eq!(late.context().temperature, 60.);
}

#[test]
fn option_expression_failures_are_explicit() {
    let undefined = config(".options temp={nope}").unwrap_err();
    assert!(matches!(undefined, SpiceError::Parse { .. }), "{undefined}");
    assert!(
        undefined.to_string().contains("option 'temp'"),
        "{undefined}"
    );
    // Range checks apply to the evaluated value.
    assert!(config(".param n=2.5\n.options itl1={n}").is_err());
    assert!(config(".param n=-1\n.options reltol={n}").is_err());
    assert!(config(".options temp={-300}").is_err());
    // method takes a word.
    let error = config(".param m=1\n.options method={m}").unwrap_err();
    assert!(error.to_string().contains("method"), "{error}");
    // Unknown and unported names fail as such, before evaluation.
    assert!(matches!(
        config(".options bogus={nope}"),
        Err(SpiceError::Parse { message, .. }) if message.contains("unknown option")
    ));
    assert!(
        config(".options gshunt={nope}")
            .unwrap_err()
            .is_not_yet_ported()
    );
    // Without a parameter scope the value cannot be evaluated.
    let n = deck("r1 a 0 1k\n.param t=40\n.options temp={t}");
    let error = RunConfig::from_options(&n.options, &RunOverrides::default()).unwrap_err();
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
}
