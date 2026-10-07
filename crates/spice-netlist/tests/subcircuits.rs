//! #12 syntax/scope tests, not subcircuit elaboration or simulation.
use spice_core::SpiceError;
use spice_netlist::{
    Parser,
    ast::{Netlist, ParameterKind, ScopedCardKind},
    source::parse_deck_text,
};
use std::path::Path;

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("scopes.cir"),
        &format!("Title\n{body}"),
    ))
}

#[test]
fn ordered_cards_are_scope_local_and_nested_definitions_are_retained() {
    let n = parse("X0 A GND OUTER K=2 K={base*2}\n.subckt OUTER A B params: K=1K K=base\n.model dm d\nX1 a b INNER params: T='K + 1'\n.subckt INNER 1 0\nD1 1 0 dm\n.ends inner\nr1 a b 1k\n.ends OUTER\n.op\n.end\nignored {\n").unwrap();
    assert_eq!(n.devices.len(), 1);
    assert_eq!(n.devices[0].nodes, ["a", "0"]);
    assert_eq!(n.devices[0].parameters[1].value, "{base*2}");
    assert_eq!(n.devices[0].parameters[1].kind, ParameterKind::Textual);
    assert_eq!(
        n.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Device(0),
            ScopedCardKind::Subcircuit(0),
            ScopedCardKind::Analysis(0),
            ScopedCardKind::End
        ]
    );
    let outer = &n.subcircuits[0];
    assert_eq!(outer.terminals, ["a", "b"]);
    assert_eq!(
        outer
            .parameters
            .iter()
            .map(|p| p.value.as_str())
            .collect::<Vec<_>>(),
        ["1K", "base"]
    );
    assert_eq!(outer.devices[0].model.as_deref(), Some("inner"));
    assert_eq!(outer.devices[0].parameters[0].value, "'K + 1'");
    assert_eq!(
        outer.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Model(0),
            ScopedCardKind::Device(0),
            ScopedCardKind::Subcircuit(0),
            ScopedCardKind::Device(1),
            ScopedCardKind::Ends
        ]
    );
    assert_eq!(outer.end_location.line, 10);
    assert_eq!(outer.subcircuits[0].devices[0].location.line, 7);
    assert_eq!(outer.cards[1].source.raw, "X1 a b INNER params: T='K + 1'");
}

#[test]
fn model_disambiguation_uses_local_and_ancestor_forward_declarations() {
    let n = parse(".subckt outer a b\nq1 c b e local\nr1 a b ancestor\n.model local npn\n.subckt inner a b\nq2 c b e local\n.ends\n.ends\n.model ancestor r\n").unwrap();
    assert_eq!(n.subcircuits[0].devices[0].nodes, ["c", "b", "e"]);
    assert_eq!(
        n.subcircuits[0].devices[1].model.as_deref(),
        Some("ancestor")
    );
    for body in [
        ".subckt one a b\n.model local npn\n.ends\n.subckt two a b\nq1 c b e local\n.ends",
        ".subckt outer a b\nq1 c b e local\n.subckt inner a b\n.model local npn\n.ends\n.ends",
    ] {
        assert!(parse(body).is_err(), "{body}");
    }
}

#[test]
fn names_are_local_and_x_targets_are_not_resolved_or_flattened() {
    let n = parse(
        ".subckt same\nX1 same\n.ends\n.subckt wrapper\n.subckt same\n.ends\n.ends\nX2 missing\n",
    )
    .unwrap();
    assert_eq!(n.subcircuits.len(), 2);
    assert_eq!(n.subcircuits[1].subcircuits[0].name, "same");
    assert_eq!(n.devices[0].model.as_deref(), Some("missing"));
    assert!(n.devices[0].nodes.is_empty());
}

#[test]
fn malformed_structure_and_assignments_fail_explicitly() {
    for (body, message) in [
        (".ends", "unmatched"),
        (".subckt a\n.ends b", "mismatched"),
        (".subckt a\n", "missing .ends"),
        (".subckt a\n.end", ".end before .ends"),
        (".subckt a\n.ends\n.subckt A\n.ends", "duplicate"),
    ] {
        let e = parse(body).unwrap_err();
        assert!(matches!(e, SpiceError::Parse { .. }), "{e}");
        assert!(e.to_string().contains(message), "{e}");
    }
    for body in [
        ".subckt",
        ".ends a b",
        "X1",
        "X1 a b target params:",
        "X1 a b target k=",
        "X1 target k=1e999",
        ".subckt a params: k=\n.ends",
        ".subckt a params: k=1 junk\n.ends",
        "X1 target k=1,",
    ] {
        assert!(
            matches!(parse(body), Err(SpiceError::Parse { .. })),
            "{body}"
        );
    }
}

#[test]
fn ground_alias_and_byte_positions_survive_structural_grammar() {
    let deck = parse_deck_text(
        Path::new("scopes.cir"),
        "title\n.subckt GND GND ÖUT params: R=1K\n.ends GND\nXGND GND ÖUT GND R=2K\n",
    );
    for auto in [true, false] {
        let n = Parser::with_auto_gnd(auto).parse_deck(&deck).unwrap();
        assert_eq!(n.subcircuits[0].name, "gnd");
        assert_eq!(
            n.subcircuits[0].terminals[0],
            if auto { "0" } else { "gnd" }
        );
        assert_eq!(n.devices[0].model.as_deref(), Some("gnd"));
        assert_eq!(n.devices[0].parameters[0].location.column, 19);
        assert_eq!(n.subcircuits[0].parameters[0].location.column, 30);
    }
}

#[test]
fn nesting_budget_and_first_error_order_are_explicit() {
    let body = format!("{}{}", ".subckt a\n".repeat(65), ".ends\n".repeat(65));
    assert!(
        parse(&body)
            .unwrap_err()
            .to_string()
            .contains("nesting limit")
    );
    let e = parse(".param unsupported=1\n.subckt a\nmalformed {\n").unwrap_err();
    assert!(e.is_not_yet_ported());
    assert!(e.to_string().contains("scopes.cir:2:1"));
}
