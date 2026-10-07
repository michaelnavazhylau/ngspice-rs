//! `.save`/`.print` output cards: positioned, typed requests (GitHub #42,
//! frontend half). Nothing here selects vectors; that is
//! `spice-analysis`'s job (`docs/port/OUTPUT_SELECTION.md`).
use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::ast::{OutputCards, RequestedVector, ScopedCardKind, VectorComponent};
use spice_netlist::{ParsedDeck, Parser, semantic_eq, source::parse_deck_text, write_netlist};
use std::path::Path;

fn parse(body: &str) -> Result<ParsedDeck, SpiceError> {
    let deck = parse_deck_text(Path::new("save.cir"), &format!("Title\n{body}"));
    Parser::new().parse_deck_with_output(&deck)
}

fn output(body: &str) -> OutputCards {
    parse(body).expect("parses").output
}

#[test]
fn save_requests_keep_order_spelling_and_positions() {
    let cards = output("r1 a b 1k\n.save all v(out) i(V1) v(in,out) vm(out) vdb(a,b)\n");
    assert_eq!(cards.len(), 1);
    let save = &cards.saves[0];
    assert_eq!((save.location.line, save.location.column), (3, 1));
    let spelled: Vec<String> = save.requests.iter().map(|r| r.vector.name()).collect();
    assert_eq!(
        spelled,
        [
            "all",
            "v(out)",
            // Device names are lowercased: `.save` matches the plot's `i(v1)`.
            "i(v1)",
            "v(in,out)",
            "vm(out)",
            "vdb(a,b)"
        ]
    );
    assert_eq!(
        save.requests[2].vector,
        RequestedVector::Current {
            device: "v1".to_owned(),
        }
    );
    assert_eq!(
        save.requests[3].vector,
        RequestedVector::Voltage {
            positive: "in".to_owned(),
            negative: Some("out".to_owned()),
        }
    );
    assert_eq!(
        save.requests[4].vector,
        RequestedVector::Component {
            component: VectorComponent::Magnitude,
            positive: "out".to_owned(),
            negative: None,
        }
    );
    assert_eq!(
        save.requests[5].vector,
        RequestedVector::Component {
            component: VectorComponent::Decibels,
            positive: "a".to_owned(),
            negative: Some("b".to_owned()),
        }
    );
    assert_eq!(
        (
            save.requests[1].location.line,
            save.requests[1].location.column
        ),
        (3, 11),
        "the location points at the request's first character"
    );
    assert_eq!(
        (
            save.requests[2].location.line,
            save.requests[2].location.column
        ),
        (3, 18)
    );
    // `v(in,out)` is a difference; `i(v1)` is not.
    assert!(save.requests[3].vector.is_difference());
    assert!(!save.requests[2].vector.is_difference());
}

#[test]
fn print_cards_carry_their_analysis_and_its_position() {
    let cards = output(".print ac v(out) i(v1)\n.print tran v(out)\n.save v(in)\n");
    assert_eq!(cards.prints.len(), 2);
    assert_eq!(cards.saves.len(), 1);
    assert_eq!(cards.prints[0].analysis, AnalysisKind::Ac);
    assert_eq!(
        (
            cards.prints[0].location.line,
            cards.prints[0].location.column
        ),
        (2, 1)
    );
    assert_eq!(
        (
            cards.prints[0].analysis_location.line,
            cards.prints[0].analysis_location.column
        ),
        (2, 8)
    );
    assert_eq!(cards.prints[1].analysis, AnalysisKind::Transient);
    // The analysis name may carry the dot the card usually omits.
    assert_eq!(
        output(".print .dc v(a)\n").prints[0].analysis,
        AnalysisKind::DcSweep
    );
}

#[test]
fn nodes_are_canonicalised_like_device_nodes() {
    let cards = output(".save v(OUT,In) v(GND)\n");
    assert_eq!(
        cards.saves[0].requests[0].vector,
        RequestedVector::Voltage {
            positive: "out".to_owned(),
            negative: Some("in".to_owned()),
        }
    );
    // `gnd` is ground under the default rule, and a plain node without it.
    assert_eq!(
        cards.saves[0].requests[1].vector,
        RequestedVector::Voltage {
            positive: "0".to_owned(),
            negative: None,
        }
    );
    let deck = parse_deck_text(Path::new("save.cir"), "Title\n.save v(gnd)\n");
    let cards = Parser::with_auto_gnd(false)
        .parse_deck_with_output(&deck)
        .unwrap();
    assert_eq!(
        cards.output.saves[0].requests[0].vector,
        RequestedVector::Voltage {
            positive: "gnd".to_owned(),
            negative: None,
        }
    );
}

#[test]
fn unsupported_requests_are_positioned_failures_not_dropped_cards() {
    let not_ported = [
        (
            ".save i(r1)",
            "only a voltage source or inductor branch current",
        ),
        (
            ".save i(q1)",
            "only a voltage source or inductor branch current",
        ),
        (
            ".save @r1[resistance]",
            "instance parameters are not observable",
        ),
    ];
    for (body, expected) in not_ported {
        let error = parse(body).expect_err(body);
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains(expected), "{body}: {error}");
    }
    let parse_errors = [
        (".save v(a,a)", "identically zero"),
        (".save v(0,0)", "identically zero"),
        (".save v(a,b,c)", "expected ')' after the node name(s)"),
        (".save v(a", "expected ')' after the node name(s)"),
        (".save v()", "expected a node or device name"),
        (".save v", "needs a parenthesised argument"),
        (".save power(v1)", "unknown vector request 'power'"),
        (".save im(v1)", "unknown vector request 'im'"),
        (".save i(v1,v2)", "takes one device name"),
        (".save i(2)", "expected a device name in i(...)"),
        (".save", "needs at least one vector request"),
        (".print", "expected an analysis name after .print"),
        (".print v(out)", "expected an analysis name after .print"),
    ];
    for (body, expected) in parse_errors {
        let error = parse(body).expect_err(body);
        assert!(!error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains(expected), "{body}: {error}");
        assert!(
            error.to_string().contains("save.cir:2"),
            "{body}: the message names the card's position: {error}"
        );
    }
}

#[test]
fn a_body_local_output_card_is_rejected_explicitly() {
    for body in [
        ".subckt s a b\nr1 a b 1k\n.save v(a)\n.ends\n",
        ".subckt s a b\n.print op v(a)\n.ends\n",
    ] {
        let error = parse(body).expect_err(body);
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(
            error.to_string().contains("inside a .subckt body"),
            "{body}: {error}"
        );
    }
}

#[test]
fn the_netlist_keeps_the_card_and_the_writer_round_trips_it() {
    let body = "r1 a b 1k\n.save v(b) i(v1)\n.print op v(b)\n.op\n";
    let deck = parse_deck_text(Path::new("save.cir"), &format!("Title\n{body}"));
    let parsed = Parser::new().parse_deck_with_output(&deck).unwrap();
    // The netlist still describes the circuit; the ordered cards record the
    // output cards as `Output`, and `parse_deck` still returns the netlist.
    assert!(
        parsed
            .netlist
            .cards
            .iter()
            .any(|card| card.kind == ScopedCardKind::Output)
    );
    let netlist = Parser::new().parse_deck(&deck).unwrap();
    assert_eq!(netlist.top_level_device_count(), 1);
    let written = write_netlist(&netlist).expect("the writer reproduces the cards");
    assert!(written.contains(".save v(b) i(v1)"), "{written}");
    assert!(written.contains(".print op v(b)"), "{written}");
    let reparsed = parse_deck_text(Path::new("save.cir"), &written);
    assert!(semantic_eq(
        &Parser::new().parse_deck(&deck).unwrap(),
        &Parser::new().parse_deck(&reparsed).unwrap()
    ));
    assert_eq!(
        Parser::new()
            .parse_deck_with_output(&reparsed)
            .unwrap()
            .output,
        parsed.output
    );
}

#[test]
fn output_cards_are_part_of_semantic_equality_and_survive_a_semantic_form() {
    // The typed requests live in `OutputCards`, so the card's spelling is the
    // netlist's only record of what was asked for: two decks that differ only in
    // their `.save` requests must not compare equal, and the writer must still
    // reproduce the card from a semantic form (the round-trip gate's contract).
    let left = parse_deck_text(Path::new("save.cir"), "Title\n.save v(a)\n.op\n");
    let right = parse_deck_text(Path::new("save.cir"), "Title\n.save v(b)\n.op\n");
    let left = Parser::new().parse_deck(&left).unwrap();
    let right = Parser::new().parse_deck(&right).unwrap();
    assert!(
        !semantic_eq(&left, &right),
        "different .save requests are not semantically equal"
    );
    assert!(semantic_eq(&left, &left.clone()));

    let semantic = spice_netlist::semantic::semantic_form(&left);
    let written = write_netlist(&semantic).expect("the writer reproduces the card");
    assert!(written.contains(".save v(a)"), "{written}");
    let reparsed = parse_deck_text(Path::new("save.cir"), &written);
    assert!(semantic_eq(
        &left,
        &Parser::new().parse_deck(&reparsed).unwrap()
    ));
}
