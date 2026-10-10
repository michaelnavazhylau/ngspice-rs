//! Explicit Fourier settings on analytic traces and C's exact saved sample grid.
use ngspice_rs::analysis::{
    Plot, PlotFlags, Variable,
    fourier::{self, FourierSettings},
};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, Complex};
use std::{fs, path::Path, process::Command};

fn cards() -> Vec<ngspice_rs::netlist::ast::FourierCard> {
    Parser::new()
        .parse_deck_with_output(&parse_deck_text(
            Path::new("four.cir"),
            "Four\n.four 1k v(out)\n.end\n",
        ))
        .unwrap()
        .fourier
}
#[test]
fn configured_fourier_recovers_multiple_periods_and_polynomial_interpolation() {
    let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
    plot.push_variable(Variable::new("time", "time"));
    plot.push_variable(Variable::new("v(out)", "voltage"));
    for i in 0..=4000 {
        let t = i as f64 * 1e-6;
        let value = 0.5
            + (std::f64::consts::TAU * 1000. * t).sin()
            + 0.25 * (std::f64::consts::TAU * 2000. * t).sin();
        plot.push_point(vec![Complex::real(t), Complex::real(value)])
            .unwrap();
    }
    for degree in [1, 2, 3, 5] {
        let settings = FourierSettings {
            nfreqs: 5,
            nperiods: 2,
            polydegree: degree,
            gridsize: 256,
        };
        let results = fourier::resolve_with_stable_settings(
            &plot,
            AnalysisKind::Transient,
            &cards(),
            settings,
        )
        .unwrap();
        let result = &results[0];
        assert_eq!(result.harmonics.len(), 4);
        assert_eq!(result.divisions, 512);
        assert!((result.dc - 0.5).abs() < 1e-7);
        assert!((result.harmonics[0].amplitude - 1.).abs() < 4e-6);
        assert!((result.thd - 0.25).abs() < 4e-6);
    }
    let invalid = FourierSettings {
        gridsize: 100000,
        nperiods: 100,
        ..Default::default()
    };
    assert!(
        fourier::resolve_with_settings(&plot, AnalysisKind::Transient, &cards(), invalid).is_err()
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; compare all settings on the same physical samples"]
fn explicit_fourier_settings_match_c_on_c_samples() {
    use ngspice_rs::analysis::RawFile;
    let binary = std::env::var("NGSPICE_BIN").expect("NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let dir = std::env::temp_dir().join(format!("m9-fourier-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let raw = dir.join("samples.raw");
    let deck = dir.join("deck.cir");
    fs::write(
        &deck,
        "Four samples\nV1 out 0 sin(.5 1 1k)\nR1 out 0 1k\n.tran 1u 4m\n.end\n",
    )
    .unwrap();
    let run = Command::new(&binary)
        .args(["-b", "-r"])
        .arg(&raw)
        .arg(&deck)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let plot = &RawFile::load(&raw).unwrap().plots[0].plot;
    for (index, settings) in [
        FourierSettings::default(),
        FourierSettings {
            nfreqs: 5,
            ..Default::default()
        },
        FourierSettings {
            nperiods: 2,
            ..Default::default()
        },
        FourierSettings {
            polydegree: 3,
            ..Default::default()
        },
        FourierSettings {
            polydegree: 2,
            ..Default::default()
        },
        FourierSettings {
            polydegree: 5,
            ..Default::default()
        },
        FourierSettings {
            polydegree: 7,
            ..Default::default()
        },
        FourierSettings {
            polydegree: 0,
            ..Default::default()
        },
        FourierSettings {
            gridsize: 512,
            ..Default::default()
        },
    ]
    .into_iter()
    .enumerate()
    {
        let control = dir.join(format!("read{index}.cir"));
        fs::write(&control,format!("Four compare\n.control\nload {}\nset nfreqs={} nperiods={} polydegree={} fourgridsize={}\nfourier 1k v(out)\nquit\n.endc\n.end\n",raw.display(),settings.nfreqs,settings.nperiods,settings.polydegree,settings.gridsize)).unwrap();
        let run = Command::new(&binary)
            .arg("-b")
            .arg(&control)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&run.stdout);
        assert!(run.status.success(), "{text}");
        let ours =
            fourier::resolve_with_settings(plot, AnalysisKind::Transient, &cards(), settings)
                .unwrap();
        let rows: Vec<_> = text
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let order = fields.next()?.parse::<usize>().ok()?;
                let _frequency = fields.next()?.parse::<f64>().ok()?;
                let magnitude = fields.next()?.parse::<f64>().ok()?;
                Some((order, magnitude))
            })
            .collect();
        assert_eq!(rows.len(), settings.nfreqs as usize, "{text}");
        for (order, magnitude) in rows {
            let value = if order == 0 {
                ours[0].dc
            } else {
                ours[0].harmonics[order - 1].amplitude
            };
            assert!(
                (value - magnitude).abs() <= 5e-6 * magnitude.abs() + 1e-8,
                "settings {settings:?}, row {order}: Rust {value}, C {magnitude}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fundamental_expressions_are_resolved_before_simulation() {
    use ngspice_rs::netlist::eval::ParamScope;
    let parsed = Parser::new().parse_deck_with_output(&parse_deck_text(
        Path::new("expr.cir"),
        "Expressions\n.param base=500\n.func twice(x) {2*x}\n.four {twice(base)} v(out)\n.four 'base*2' v(out)\n.end\n",
    )).unwrap();
    let written = ngspice_rs::netlist::write_netlist(&parsed.netlist).unwrap();
    let again = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("again.cir"), &written))
        .unwrap();
    assert!(ngspice_rs::netlist::semantic_eq(
        &parsed.netlist,
        &again.netlist
    ));
    let scope = ParamScope::for_netlist(&parsed.netlist).unwrap();
    let resolved = fourier::resolve_cards(&parsed.fourier, &scope).unwrap();
    assert_eq!(resolved.len(), 2);
    for card in resolved {
        assert_eq!(card.fundamental, 1000.);
        assert!(card.fundamental_expression.is_none());
    }
    for expression in ["missing", "0", "-base", "1/0"] {
        let parsed = Parser::new()
            .parse_deck_with_output(&parse_deck_text(
                Path::new("expr.cir"),
                &format!("Expressions\n.four {{{expression}}} v(out)\n.end\n"),
            ))
            .unwrap();
        assert!(fourier::resolve_cards(&parsed.fourier, &scope).is_err());
    }
}

#[test]
fn cli_evaluates_fourier_parameters_and_preserves_output_on_failure() {
    let dir = std::env::temp_dir().join(format!("m9-four-expr-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    let raw = dir.join("out.raw");
    fs::write(&deck, "Expressions\n.param base=500\nV1 out 0 sin(0 1 1k)\nR1 out 0 1k\n.tran 1u 4m\n.four {2*base} v(out)\n.end\n").unwrap();
    let report = ngspice_rs::cli::simulate::run(&deck, &raw, true).unwrap();
    assert_eq!(
        report.plots.last().unwrap().fourier_results[0].fundamental,
        1000.
    );
    let original = fs::read(&raw).unwrap();
    fs::write(
        &deck,
        "Expressions\nV1 out 0 1\nR1 out 0 1k\n.tran 1u 4m\n.four {missing} v(out)\n.end\n",
    )
    .unwrap();
    assert!(ngspice_rs::cli::simulate::run(&deck, &raw, true).is_err());
    assert_eq!(fs::read(&raw).unwrap(), original);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn control_fourier_command_runs_through_the_process_interface() {
    let dir = std::env::temp_dir().join(format!("m9-four-control-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    let raw = dir.join("out.raw");
    let text = "Control Fourier\nV1 out 0 sin(.5 1 1k)\nR1 out 0 1k\n.tran 1u 4m\n.control\nset nfreqs=5 nperiods=2 polydegree=3 fourgridsize=256\nrun\nfourier 1k v(out)\nquit\n.endc\n.end\n";
    fs::write(&deck, text).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("simulate")
        .arg("--output")
        .arg(&raw)
        .arg(&deck)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("Fourier analysis for v(out)"));
    let report = ngspice_rs::cli::simulate::run(&deck, &raw, true).unwrap();
    let result = &report.plots[0].fourier_results[0];
    assert_eq!(result.harmonics.len(), 4);
    assert_eq!(result.divisions, 512);
    let parser = Parser::new();
    let parsed = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("a.cir"), text))
        .unwrap();
    let written = ngspice_rs::netlist::write_netlist(&parsed.netlist).unwrap();
    let again = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("b.cir"), &written))
        .unwrap();
    assert!(ngspice_rs::netlist::semantic_eq(
        &parsed.netlist,
        &again.netlist
    ));
    for commands in [
        "fourier 1k v(out)",
        "run\nrun",
        "quit",
        "run\n.endc\n.control\nrun",
        "run\nset nfreqs=5",
        "run\nquit\nfourier 1k v(out)",
    ] {
        let invalid = format!(
            "Bad\nV1 out 0 1\nR1 out 0 1k\n.tran 1u 4m\n.control\n{commands}\n.endc\n.end\n"
        );
        fs::write(&deck, invalid).unwrap();
        fs::write(&raw, b"previous").unwrap();
        assert!(ngspice_rs::cli::simulate::run(&deck, &raw, true).is_err());
        assert_eq!(fs::read(&raw).unwrap(), b"previous");
    }
    fs::remove_dir_all(dir).unwrap();
}
