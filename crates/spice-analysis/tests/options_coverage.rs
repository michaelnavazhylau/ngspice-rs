//! #110 / #107: every `.option` name this port accepts either takes effect,
//! is a documented no-op (`RunConfig::ignored`) or fails explicitly; option
//! values may be `{expr}`/`'expr'` evaluated against top-level `.param`.
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, RunOverrides, runner};
use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
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
    let c = config(".options itl1=37").unwrap();
    assert_eq!(c.dc().itl1, Some(37));
    for kind in [AnalysisKind::OperatingPoint, AnalysisKind::Ac] {
        let request = c.request(AnalysisRequest::new(kind)).unwrap();
        assert_eq!(request.named("maxiter"), Some("37"));
    }
    assert_eq!(c.request(dc()).unwrap().named("maxiter"), Some("37"));
    assert_eq!(c.request(tran()).unwrap().named("maxiter"), Some("37"));
    assert!(c.request(diffsol()).is_err());
    // Too few iterations for the initial bias of a nonlinear transient fail
    // with the bounded DC solve's diagnostics, not silently.
    let deck = "v1 a 0 5\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n.tran 1u 10u";
    assert!(simulate(deck).is_ok());
    let error = simulate(&format!("{deck}\n.options itl1=1 gminsteps=0 srcsteps=0")).unwrap_err();
    assert!(error.to_string().contains("Newton"), "{error}");
}

#[test]
fn itl2_bounds_the_warm_started_dc_sweep_points() {
    let c = config(".options itl2=7").unwrap();
    assert_eq!(c.dc().itl2, Some(7));
    assert_eq!(c.request(dc()).unwrap().named("trcvmaxiter"), Some("7"));
    // C reads it only for .dc points (and its dynamic stepping stages).
    for request in [
        AnalysisRequest::new(AnalysisKind::OperatingPoint),
        AnalysisRequest::new(AnalysisKind::Ac),
        tran(),
    ] {
        assert_eq!(c.request(request).unwrap().named("trcvmaxiter"), None);
    }
    // A one-iteration warm start fails at most points and falls back to the
    // full solve: the swept curve is unchanged.
    let sweep = "v1 a 0 0\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n.dc v1 0 5 0.5";
    let plain = simulate(sweep).unwrap();
    for itl2 in ["1", "3", "100"] {
        let bounded = simulate(&format!("{sweep}\n.options itl2={itl2}")).unwrap();
        assert_eq!(bounded.point_count(), plain.point_count());
        for point in 0..plain.point_count() {
            close(
                bounded.value("v(b)", point).unwrap().re,
                plain.value("v(b)", point).unwrap().re,
                1e-6,
                1e-9,
            );
        }
    }
    for bad in [
        ".options itl2=0",
        ".options itl2=1.5",
        ".options itl2=10001",
    ] {
        assert!(!config(bad).unwrap_err().is_not_yet_ported(), "{bad}");
    }
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
fn itl4_is_the_companion_newton_limit_per_timepoint() {
    let c = config(".options itl4=25").unwrap();
    assert_eq!(c.transient().itl4, Some(25));
    assert_eq!(c.request(tran()).unwrap().named("tranmaxiter"), Some("25"));
    assert_eq!(
        c.request(AnalysisRequest::new(AnalysisKind::OperatingPoint))
            .unwrap()
            .named("tranmaxiter"),
        None
    );
    let error = c.request(diffsol()).unwrap_err();
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
    // Fewer Newton iterations per timepoint force more rejected trials (C
    // dctran.c cuts the step by 8 on non-convergence).
    let deck_text = "v1 a 0 pulse(0 5 1u 1u 1u 5u 20u)\nr1 a b 1k\nd1 b 0 dm\n\
                     .model dm d(is=1e-14)\n.tran 0.1u 3u";
    let stats = |options: &str| {
        let n = deck(&format!("{deck_text}\n{options}"));
        let c = RunConfig::from_netlist(&n).unwrap();
        let request = c.request_for(&n.analyses[0]).unwrap();
        let mut circuit = c.circuit(&n).unwrap();
        spice_analysis::companion_transient(&mut circuit, &request, &c.context())
            .unwrap()
            .1
    };
    let default = stats("");
    assert_eq!(stats(".options itl4=10"), default);
    let starved = stats(".options itl4=3");
    assert!(
        starved.rejected > default.rejected,
        "{starved:?} vs {default:?}"
    );
    for bad in [".options itl4=0", ".options itl4=2.5", ".options itl4"] {
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
fn unread_frontend_variables_bypass_and_pivots_are_documented_no_ops() {
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
    let c = config(".options pivtol=1e-13 pivrel=1e-3").unwrap();
    assert_eq!(c.ignored().len(), 2);
    assert!(c.ignored()[1].reason.contains("pivot"));
    assert!(config(".options pivtol=0 pivrel=1").is_ok());
    for bad in [
        ".options pivtol=-1",
        ".options pivrel=0",
        ".options pivrel=1.5",
        ".options pivtol",
        ".options pivrel=abc",
    ] {
        let error = config(bad).unwrap_err();
        assert!(!error.is_not_yet_ported(), "{bad}: {error}");
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
        ".options noopiter",
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
         reltol={2*1m} itl1={10*5} xmu={half/2} pivrel='half'",
    )
    .unwrap();
    assert_eq!(c.context().temperature, 50.);
    assert_eq!(c.context().nominal_temperature, 35.);
    close(c.context().gmin, 1e-7, 1e-12, 0.);
    close(c.transient().rtol.unwrap(), 2e-3, 1e-12, 0.);
    assert_eq!(c.dc().itl1, Some(50));
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
