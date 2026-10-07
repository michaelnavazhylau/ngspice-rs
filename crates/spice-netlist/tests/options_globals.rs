//! #16 `.option`/`.global` syntax: positions, order, duplicates and failures.
use spice_core::SpiceError;
use spice_netlist::{
    Parser,
    ast::{Netlist, ScopedCardKind},
    source::parse_deck_text,
};
use std::path::Path;

fn parse_with(parser: Parser, body: &str) -> Result<Netlist, SpiceError> {
    parser.parse_deck(&parse_deck_text(
        Path::new("opts.cir"),
        &format!("Title\n{body}"),
    ))
}

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    parse_with(Parser::new(), body)
}

#[test]
fn settings_keep_order_duplicates_case_and_positions() {
    let n = parse(
        ".OPTIONS RelTol=1e-4 temp=30 NOOPIter\n.option reltol = 2m method=GEAR\n.opt TEMP=40\n.end\n",
    )
    .unwrap();
    assert_eq!(n.options.len(), 3);
    let names: Vec<_> = n
        .options
        .iter()
        .flat_map(|c| &c.settings)
        .map(|s| (s.name.as_str(), s.value.as_ref().map(|v| v.text.as_str())))
        .collect();
    assert_eq!(
        names,
        [
            ("reltol", Some("1e-4")),
            ("temp", Some("30")),
            ("noopiter", None),
            ("reltol", Some("2m")),
            ("method", Some("GEAR")),
            ("temp", Some("40")),
        ]
    );
    let first = &n.options[0].settings[0];
    assert_eq!((first.location.line, first.location.column), (2, 10));
    assert_eq!(first.value.as_ref().unwrap().location.column, 17);
    assert_eq!(n.options[1].location.line, 3);
    assert_eq!(
        n.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Options(0),
            ScopedCardKind::Options(1),
            ScopedCardKind::Options(2),
            ScopedCardKind::End
        ]
    );
}

#[test]
fn malformed_option_cards_are_errors() {
    for body in [
        ".options",
        ".options reltol=",
        ".options reltol= temp=30",
        ".options reltol=1e-3 =5",
        ".options 1e-3",
        ".options reltol=1e999",
        ".options temp={t}",
        ".options method=(gear)",
        ".options reltol==1",
    ] {
        let result = parse(&format!("{body}\n.end\n"));
        assert!(
            matches!(
                result,
                Err(SpiceError::Parse { .. } | SpiceError::NotYetPorted { .. })
            ),
            "{body}: {result:?}"
        );
    }
}

#[test]
fn globals_normalize_gnd_with_and_without_auto_gnd() {
    let body = ".GLOBAL VDD Gnd vdd\n.global vss\n.end\n";
    let aliased = parse(body).unwrap();
    assert_eq!(aliased.globals.len(), 2);
    let nodes: Vec<_> = aliased.globals[0]
        .nodes
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    assert_eq!(nodes, ["vdd", "0", "vdd"]);
    assert_eq!(aliased.globals[0].nodes[1].location.column, 13);
    assert_eq!(aliased.global_node_names(), ["vdd", "0", "vss"]);
    assert!(aliased.is_global_node("0") && aliased.is_global_node("VDD"));
    assert!(!aliased.is_global_node("gnd"));
    assert_eq!(
        aliased.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Global(0),
            ScopedCardKind::Global(1),
            ScopedCardKind::End
        ]
    );

    let plain = parse_with(Parser::with_auto_gnd(false), body).unwrap();
    assert_eq!(plain.global_node_names(), ["vdd", "gnd", "vss"]);
    assert!(plain.is_global_node("gnd") && plain.is_global_node("0"));
}

#[test]
fn ground_is_always_global_and_empty_global_cards_fail() {
    let n = parse("r1 a 0 1k\n.end\n").unwrap();
    assert!(n.is_global_node("0"));
    assert!(!n.is_global_node("a"));
    assert!(n.global_node_names().is_empty());
    assert!(matches!(
        parse(".global\n.end\n"),
        Err(SpiceError::Parse { .. })
    ));
    assert!(parse(".global a=1\n.end\n").is_err());
}

#[test]
fn subcircuit_bodies_do_not_silently_drop_options_or_globals() {
    for card in [".options reltol=1m", ".global vdd"] {
        let error = parse(&format!(".subckt s a b\nr1 a b 1\n{card}\n.ends\n.end\n")).unwrap_err();
        assert!(error.is_not_yet_ported(), "{card}: {error}");
    }
}

#[test]
fn separate_parses_share_no_option_state() {
    let first = parse(".options temp=90\n.global x\n.end\n").unwrap();
    let second = parse("r1 a 0 1\n.end\n").unwrap();
    assert_eq!(first.options.len(), 1);
    assert!(second.options.is_empty() && second.globals.is_empty());
}
