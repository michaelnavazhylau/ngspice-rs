//! M9 front-end cards in subcircuits: preserve syntax, expand per instance.
use ngspice_rs::analysis::RunConfig;
use ngspice_rs::devices::subckt::{SubcircuitLimits, expand_with_output};
use ngspice_rs::netlist::{Parser, elaborate, semantic_eq, source::parse_deck_text, write_netlist};
use std::path::Path;

const DECK: &str = "Scoped frontend
V1 out 0 1
.option reltol=.001
.subckt cell a params: hint=.25
R1 a local 1k
R2 local rail 1k
.global rail
.option reltol={hint/100}
.ic v(local)={hint}
.nodeset v(a)={hint*2}
.save v(local) v(a,rail) i(vlocal)
Vlocal other 0 0
R3 other 0 1k
.measure tran mm max v(out)
.four 1k v(out)
.ends
X1 out cell hint=.3
.option reltol=.004
X2 out cell hint=.5
Vrail rail 0 0
.tran 1u 1m
.end
";

#[test]
fn scoped_cards_round_trip_and_expand_in_instance_order() {
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("scoped.cir"), DECK))
        .unwrap();
    let text = write_netlist(&parsed.netlist).unwrap();
    let roundtrip = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("roundtrip.cir"), &text))
        .unwrap();
    assert!(semantic_eq(&parsed.netlist, &roundtrip));
    let literal = elaborate::literalize(&parsed.netlist).unwrap();
    let expanded = expand_with_output(
        &literal.netlist,
        &literal.scope,
        SubcircuitLimits::default(),
        &parsed.output,
        &parsed.measurements,
        &parsed.fourier,
    )
    .unwrap();
    let saved: Vec<_> = expanded
        .output
        .saves
        .iter()
        .flat_map(|save| save.requests.iter().map(|request| request.vector.name()))
        .collect();
    assert_eq!(
        saved,
        [
            "v(x1.local)",
            "v(out,rail)",
            "i(v.x1.vlocal)",
            "v(x2.local)",
            "v(out,rail)",
            "i(v.x2.vlocal)"
        ]
    );
    assert_eq!(
        expanded
            .options
            .iter()
            .map(|card| card.settings[0]
                .value
                .as_ref()
                .unwrap()
                .text
                .parse::<f64>()
                .unwrap())
            .collect::<Vec<_>>(),
        [0.001, 0.003, 0.004, 0.005]
    );
    assert_eq!(
        expanded
            .initial_conditions
            .iter()
            .map(|card| (
                card.entries[0].node.as_str(),
                card.entries[0].literal().unwrap()
            ))
            .collect::<Vec<_>>(),
        [("x1.local", 0.3), ("x2.local", 0.5)]
    );
    assert_eq!(
        expanded
            .nodesets
            .iter()
            .map(|card| (
                card.entries[0].node.as_str(),
                card.entries[0].literal().unwrap()
            ))
            .collect::<Vec<_>>(),
        [("out", 0.6), ("out", 1.0)]
    );
    assert_eq!(expanded.measurements.len(), 2);
    assert_eq!(expanded.fourier.len(), 1);
    assert_eq!(expanded.fourier[0].vectors[0].vector.name(), "v(out)");
    assert!(
        expanded
            .devices
            .iter()
            .all(|device| !device.nodes.iter().any(|node| node.ends_with(".rail")))
    );
    let config = RunConfig::from_netlist(&parsed.netlist).unwrap();
    let request = config.request_for(&parsed.netlist.analyses[0]).unwrap();
    assert_eq!(request.initial_conditions.len(), 2);
    assert_eq!(request.initial_conditions[0].node, "x1.local");
    assert_eq!(request.nodesets.len(), 2);
}

#[test]
fn unused_definitions_do_not_emit_frontend_requests() {
    let text = DECK
        .replace("X1 out cell hint=.3\n", "")
        .replace("X2 out cell hint=.5\n", "");
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("unused.cir"), &text))
        .unwrap();
    let literal = elaborate::literalize(&parsed.netlist).unwrap();
    let expanded = expand_with_output(
        &literal.netlist,
        &literal.scope,
        SubcircuitLimits::default(),
        &parsed.output,
        &parsed.measurements,
        &parsed.fourier,
    )
    .unwrap();
    assert!(expanded.output.is_empty());
    assert!(expanded.measurements.is_empty());
    assert!(expanded.fourier.is_empty());
    assert!(expanded.initial_conditions.is_empty());
    assert!(expanded.nodesets.is_empty());
    assert_eq!(expanded.options.len(), 2);
}

struct Scratch(std::path::PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!("spice-m9-frontend-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

#[test]
fn cli_expands_save_measure_and_fourier_cards_before_running() {
    let scratch = scratch("cli");
    // A nonzero fundamental exercises the complete Fourier path.
    let deck = DECK
        .replace("i(vlocal)", "i(vlocal) v(a)")
        .replace("V1 out 0 1", "V1 out 0 sin(0 1 1k)");
    let input = scratch.0.join("deck.cir");
    let output = scratch.0.join("output.raw");
    std::fs::write(&input, deck).unwrap();
    let report = ngspice_rs::cli::simulate::run(&input, &output, true).unwrap();
    assert_eq!(report.plots[0].measurements.len(), 2);
    assert_eq!(report.plots[0].fourier_results.len(), 1);
    assert!(
        report.plots[0]
            .variables
            .contains(&"v(x1.local)".to_owned())
    );
    assert!(
        report.plots[0]
            .variables
            .contains(&"v(x2.local)".to_owned())
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; scoped front-end C behavior"]
fn scoped_frontend_translation_and_repetition_match_c() {
    let binary = std::env::var_os("NGSPICE_BIN").expect("absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let scratch = scratch("c");
    let input = scratch.0.join("deck.cir");
    let deck = DECK.replace(
        ".end\n",
        ".control\nlisting e\nrun\nwrite output.raw\nquit\n.endc\n.end\n",
    );
    std::fs::write(&input, deck).unwrap();
    let run = std::process::Command::new(&binary)
        .args(["-b"])
        .arg(&input)
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.status.success(), "{text}");
    for expected in [
        ".ic v(x1.local)",
        ".ic v(x2.local)",
        ".nodeset v(out)",
        "r.x1.r2 x1.local rail",
        "r.x2.r2 x2.local rail",
        ".option reltol=",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert_eq!(
        text.lines()
            .filter(|line| line.trim_start().starts_with("mm "))
            .count(),
        2,
        "{text}"
    );
    let raw = ngspice_rs::analysis::RawFile::load(scratch.0.join("output.raw")).unwrap();
    let variables: Vec<_> = raw.plots[0]
        .plot
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect();
    for expected in [
        "v(x1.local)",
        "v(x2.local)",
        "i(v.x1.vlocal)",
        "i(v.x2.vlocal)",
    ] {
        assert!(
            variables.contains(&expected),
            "missing {expected}: {variables:?}"
        );
    }
    // Batch Fourier post-processing runs after the control block; quit above
    // deliberately ends it, so run the original deck separately for .four.
    std::fs::write(&input, DECK).unwrap();
    let run = std::process::Command::new(&binary)
        .args(["-b"])
        .arg(&input)
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.status.success(), "{text}");
    assert_eq!(text.matches("Fourier analysis for").count(), 1, "{text}");
}

#[test]
fn unused_global_declarations_do_not_change_other_instances() {
    let deck = "Globals\n.subckt unused a\n.global rail\nR1 a rail 1k\n.ends\n.subckt used a\nR1 a rail 1k\nR2 rail 0 1k\n.ends\nV1 out 0 1\nX1 out used\n.op\n.end\n";
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("globals.cir"), deck))
        .unwrap();
    assert!(!parsed.netlist.is_global_node("rail"));
    let literal = elaborate::literalize(&parsed.netlist).unwrap();
    let expanded = expand_with_output(
        &literal.netlist,
        &literal.scope,
        SubcircuitLimits::default(),
        &parsed.output,
        &parsed.measurements,
        &parsed.fourier,
    )
    .unwrap();
    assert_eq!(expanded.devices[1].nodes, ["out", "x1.rail"]);
}

#[test]
fn front_end_expansion_has_a_budget_even_without_body_devices() {
    let deck =
        "Budget\n.subckt empty a\n.save v(a)\n.save v(a)\n.save v(a)\n.ends\nX1 out empty\n.end\n";
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("budget.cir"), deck))
        .unwrap();
    let literal = elaborate::literalize(&parsed.netlist).unwrap();
    let error = expand_with_output(
        &literal.netlist,
        &literal.scope,
        SubcircuitLimits {
            max_depth: 32,
            max_devices: 2,
        },
        &parsed.output,
        &parsed.measurements,
        &parsed.fourier,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("front-end card budget"),
        "{error}"
    );
}

#[test]
fn body_measurement_names_are_not_silently_reinterpreted_as_local_nodes() {
    let scratch = scratch("missing-local");
    let input = scratch.0.join("deck.cir");
    let output = scratch.0.join("output.raw");
    std::fs::write(&input, "Scope\n.subckt cell a\nR1 a local 1k\nR2 local 0 1k\n.measure tran m max v(local)\n.ends\nX1 out cell\nV1 out 0 1\n.tran 1u 10u\n.end\n").unwrap();
    std::fs::write(&output, "keep me").unwrap();
    let error = ngspice_rs::cli::simulate::run(&input, &output, true).unwrap_err();
    assert!(error.to_string().contains("local"), "{error}");
    assert_eq!(std::fs::read_to_string(output).unwrap(), "keep me");
}
