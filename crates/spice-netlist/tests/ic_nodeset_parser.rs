//! `.ic`/`.nodeset` cards and the `.tran` `uic` flag (GitHub #27, frontend half).
//! Syntax and provenance only: nothing here initializes a circuit.
use spice_core::SpiceError;
use spice_netlist::ast::{Netlist, NodeHintValue, ScopedCardKind};
use spice_netlist::elaborate::{SiteKind, literalize};
use spice_netlist::{Parser, semantic_eq, source::parse_deck_text, write_netlist};
use std::path::Path;

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("ic.cir"),
        &format!("Title\n{body}"),
    ))
}

fn message(body: &str) -> String {
    parse(body).expect_err(body).to_string()
}

#[test]
fn entries_keep_order_duplicates_and_locations() {
    let netlist = parse(
        "r1 a b 1k\n.ic v(a)=1 V(b)=2m v(a)=3\n.nodeset v(b) = 4\n.ic v(b)=5\n.nodeset v(a)={1+1}\n",
    )
    .unwrap();
    let ic: Vec<_> = netlist
        .initial_conditions()
        .map(|h| (h.node.as_str(), h.literal()))
        .collect();
    assert_eq!(
        ic,
        [
            ("a", Some(1.0)),
            ("b", Some(2e-3)),
            ("a", Some(3.0)),
            ("b", Some(5.0))
        ]
    );
    let first = netlist.initial_conditions().next().unwrap();
    assert_eq!((first.location.line, first.location.column), (3, 5));
    assert_eq!(first.node_location.column, 7);
    assert_eq!(first.value_location.column, 10);
    let nodesets: Vec<_> = netlist.nodesets().collect();
    assert_eq!(nodesets.len(), 2);
    assert!(matches!(nodesets[1].value, NodeHintValue::Expression(_)));
    let kinds: Vec<_> = netlist.cards.iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&ScopedCardKind::InitialCondition(1)));
    assert!(kinds.contains(&ScopedCardKind::Nodeset(1)));
}

#[test]
fn equals_is_optional_as_in_c_and_nodes_may_be_numeric_or_aliased() {
    let netlist = parse(".ic v(1) 2 v(x1.out)=3\n").unwrap();
    let nodes: Vec<_> = netlist
        .initial_conditions()
        .map(|h| h.node.clone())
        .collect();
    assert_eq!(nodes, ["1", "x1.out"]);
    // gnd aliases to ground and is rejected; without auto-gnd it is a node.
    assert!(message(".ic v(gnd)=1\n").contains("ground"));
    let plain = Parser::with_auto_gnd(false)
        .parse_deck(&parse_deck_text(Path::new("g.cir"), "T\n.ic v(gnd)=1\n"))
        .unwrap();
    assert_eq!(plain.initial_conditions().next().unwrap().node, "gnd");
}

#[test]
fn malformed_forms_are_explicit_errors() {
    for (body, expect) in [
        (".ic\n", "expected at least one V(node)=value entry"),
        (".nodeset\n", "expected at least one V(node)=value entry"),
        (".ic v(a)\n", "missing value"),
        (".ic v(a)=\n", "missing value"),
        (".ic v(a)= v(b)=1\n", "expected a finite numeric literal"),
        (".ic v()=1\n", "expected a node name"),
        (".ic v=1\n", "'(' after V"),
        (".ic v(a=1\n", "')' after the node name"),
        (".ic v(a,b)=1\n", "differential"),
        (".ic i(v1)=1\n", "only voltage entries"),
        (".ic a=1\n", "only voltage entries"),
        (".ic all=1\n", "only voltage entries"),
        (".ic v(0)=1\n", "ground"),
        (".nodeset v(0)=1\n", "ground"),
        (".ic v(a)=abc\n", "expected a finite numeric literal"),
        (".ic v(a)=1e999\n", "finite"),
        // A single-quoted value is an expression (C: inp_change_quotes); a
        // double-quoted string is not a value.
        (".ic v(a)=\"1\"\n", "expected a finite numeric literal"),
        (".ic v(a)={1\n", "unterminated"),
        (".ic v(a)=1 v(b)\n", "missing value"),
    ] {
        let text = message(body);
        assert!(text.contains(expect), "{body:?}: {text}");
    }
    assert!(matches!(
        parse(".nodeset all=1\n").unwrap_err(),
        SpiceError::NotYetPorted { .. }
    ));
}

#[test]
fn hints_inside_subcircuit_bodies_are_not_yet_ported() {
    for card in [".ic v(a)=1", ".nodeset v(a)=1"] {
        let error = parse(&format!(".subckt s a b\nr1 a b 1k\n{card}\n.ends s\n")).unwrap_err();
        assert!(matches!(error, SpiceError::NotYetPorted { .. }), "{error}");
    }
}

#[test]
fn writer_round_trips_cards_values_and_uic() {
    let netlist = parse(
        ".param p=2\nr1 a b 1k\n.ic v(a)=1.5 v(b)={p*2} v(a)=2\n.nodeset V(b)=3\n.tran 1u 10u 0 1n uic\n.end\n",
    )
    .unwrap();
    let written = write_netlist(&netlist).unwrap();
    assert!(
        written.contains(".ic v(a)=1.5 v(b)={p*2} v(a)=2\n"),
        "{written}"
    );
    assert!(written.contains(".nodeset v(b)=3\n"));
    assert!(written.contains(".tran 1u 10u 0 1n uic\n"));
    let again = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("w.cir"), &written))
        .unwrap();
    assert!(semantic_eq(&netlist, &again));
    // Semantic comparison sees duplicates, order and the flag.
    let swapped = parse(
        ".param p=2\nr1 a b 1k\n.ic v(a)=2 v(b)={p*2} v(a)=1.5\n.nodeset V(b)=3\n.tran 1u 10u 0 1n uic\n.end\n",
    )
    .unwrap();
    assert!(!semantic_eq(&netlist, &swapped));
    let no_uic = parse(
        ".param p=2\nr1 a b 1k\n.ic v(a)=1.5 v(b)={p*2} v(a)=2\n.nodeset V(b)=3\n.tran 1u 10u 0 1n\n.end\n",
    )
    .unwrap();
    assert!(!semantic_eq(&netlist, &no_uic));
}

#[test]
fn elaboration_evaluates_expression_values_in_order() {
    let netlist =
        parse(".param vdd=3\nr1 a b 1k\n.ic v(a)={vdd/2} v(b)=1\n.nodeset v(a)={vdd}\n").unwrap();
    let elaborated = literalize(&netlist).unwrap();
    let ic = elaborated.initial_conditions();
    assert_eq!(ic.len(), 2);
    assert_eq!((ic[0].node, ic[0].value), ("a", 1.5));
    assert_eq!((ic[1].node, ic[1].value), ("b", 1.0));
    assert_eq!(ic[0].location.line, 4);
    assert_eq!(elaborated.nodesets()[0].value, 3.0);
    assert!(
        elaborated
            .sites
            .iter()
            .any(|s| matches!(s.kind, SiteKind::InitialCondition { card: 0, entry: 0 }))
    );
    assert!(
        elaborated
            .sites
            .iter()
            .any(|s| matches!(s.kind, SiteKind::Nodeset { card: 0, entry: 0 }))
    );
    let undefined = parse("r1 a b 1k\n.ic v(a)={nope}\n").unwrap();
    assert!(literalize(&undefined).is_err());
    let zero = parse("r1 a b 1k\n.ic v(a)={1/0}\n").unwrap();
    assert!(literalize(&zero).is_err());
}

#[test]
fn tran_uic_is_a_separate_flag() {
    let netlist = parse(".tran 1n 10u uic\n.tran 1n 10u 0 1n UIC\n.tran 1n 10u\n").unwrap();
    assert_eq!(netlist.analyses[0].arguments, ["1n", "10u"]);
    assert!(netlist.analyses[0].uic);
    assert_eq!(
        netlist.analyses[0].uic_location.as_ref().unwrap().column,
        14
    );
    assert_eq!(netlist.analyses[1].arguments, ["1n", "10u", "0", "1n"]);
    assert!(netlist.analyses[1].uic);
    assert!(!netlist.analyses[2].uic && netlist.analyses[2].uic_location.is_none());
    assert_eq!(netlist.transient_uic().unwrap().location.line, 2);
    // Driver options after the flag stay options; expression indexes follow.
    let netlist = parse(".param s=1\n.tran 1n {s} uic backend=diffsol\n").unwrap();
    let card = &netlist.analyses[0];
    assert_eq!(card.arguments, ["1n", "{s}", "backend", "=", "diffsol"]);
    assert_eq!(card.expressions.len(), 1);
    assert_eq!(card.expressions[0].index, 1);
}

#[test]
fn tran_uic_misuse_is_rejected() {
    for (body, expect) in [
        (".tran 1n 10u uic uic\n", "duplicate uic"),
        (".tran 1n 10u uic 5\n", "uic must follow"),
        (".tran 1n uic 10u\n", "uic must follow"),
        (".tran 1n 10u uic=1\n", "bare flag"),
    ] {
        let text = message(body);
        assert!(text.contains(expect), "{body:?}: {text}");
    }
    // Other analyses keep `uic` as an ordinary argument.
    let netlist = parse(".dc v1 0 1 0.1 uic\n").unwrap();
    assert!(!netlist.analyses[0].uic);
    assert!(netlist.analyses[0].arguments.contains(&"uic".to_owned()));
}
