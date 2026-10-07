//! Decks whose `.include`/`.lib` sources come from an in-memory file map
//! (the browser/WebAssembly path) elaborate and simulate exactly like the same
//! content written inline.
use spice_analysis::{Plot, RunConfig, runner};
use spice_netlist::{MemorySources, Parser, SourceLimits, ast::Netlist, source::parse_deck_text};
use std::path::Path;

fn parse(files: &[(&str, &str)]) -> spice_core::SpiceResult<Netlist> {
    let mut sources = MemorySources::new();
    for (path, text) in files {
        sources.insert(path, *text);
    }
    Parser::new().parse_file_with_sources("deck.cir", &sources, SourceLimits::default())
}

fn run(netlist: &Netlist) -> spice_core::SpiceResult<Plot> {
    let config = RunConfig::from_netlist(netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    let mut circuit = config.circuit(netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

fn inline(deck: &str) -> Plot {
    run(&Parser::new()
        .parse_deck(&parse_deck_text(Path::new("inline.cir"), deck))
        .unwrap())
    .unwrap()
}

#[test]
fn included_params_devices_and_library_sections_match_the_inline_deck() {
    let netlist = parse(&[
        (
            "deck.cir",
            "divider\n.include \"models/values.inc\"\nv1 in 0 dc 10\nr1 in out {rtop}\n\
             .lib 'models/loads.lib' heavy\n.op\n.end\n",
        ),
        // Relative to models/, not to the deck.
        ("models/values.inc", ".include \"more/top.inc\"\n"),
        ("models/more/top.inc", ".param rtop=3k\n"),
        (
            "models/loads.lib",
            ".lib light\nr2 out 0 100k\n.endl light\n.lib heavy\nr2 out 0 1k\nc2 out 0 1n\n.endl heavy\n",
        ),
    ])
    .unwrap();
    assert_eq!(netlist.includes.len(), 3);
    assert!(netlist.includes.iter().all(|i| i.resolved_path.is_some()));
    let got = run(&netlist).unwrap();
    let want = inline(
        "divider\n.param rtop=3k\nv1 in 0 dc 10\nr1 in out {rtop}\nr2 out 0 1k\nc2 out 0 1n\n.op\n.end\n",
    );
    assert_eq!(got, want);
    assert!((got.value("v(out)", 0).unwrap().re - 2.5).abs() < 1e-12);
}

#[test]
fn missing_and_cyclic_in_memory_sources_are_explicit_errors() {
    let missing = parse(&[("deck.cir", "t\n.include \"nope.inc\"\n.op\n.end\n")]).unwrap_err();
    assert!(
        missing
            .to_string()
            .contains("deck.cir:2:1: cannot resolve source 'nope.inc'"),
        "{missing}"
    );
    let cycle = parse(&[
        ("deck.cir", "t\n.include a.inc\n.op\n.end\n"),
        ("a.inc", ".include sub/../b.inc\n"),
        ("b.inc", ".include ./a.inc\n"),
    ])
    .unwrap_err();
    assert!(cycle.to_string().contains("source cycle"), "{cycle}");
    let root = Parser::new()
        .parse_file_with_sources("other.cir", &MemorySources::new(), SourceLimits::default())
        .unwrap_err();
    assert!(root.to_string().contains("other.cir"), "{root}");
}

#[test]
fn in_memory_sources_obey_the_byte_budget() {
    let limits = SourceLimits {
        max_bytes: 40,
        ..SourceLimits::default()
    };
    let mut sources = MemorySources::new();
    sources.insert("deck.cir", "t\n.include big.inc\n.op\n.end\n");
    sources.insert("big.inc", format!("* {}\n", "x".repeat(64)));
    let error = Parser::new()
        .parse_file_with_sources("deck.cir", &sources, limits)
        .unwrap_err();
    assert!(error.to_string().contains("byte work limit"), "{error}");
}

#[test]
fn a_syntax_only_include_is_never_simulated_as_an_empty_file() {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("deck.cir"),
            "t\n.include values.inc\nv1 a 0 dc 1\nr1 a 0 1k\n.op\n.end\n",
        ))
        .unwrap();
    let error = run(&netlist).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unresolved source directive 'values.inc'"),
        "{error}"
    );
}
