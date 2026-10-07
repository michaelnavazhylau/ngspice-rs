//! Combinator-specific regressions: committed failures, lookahead and complete
//! consumption must preserve the semantic parser's public error contract.

use std::path::Path;

use spice_core::SpiceError;
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};

fn parse(card: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("winnow.cir"),
        &format!("Title\n{card}\n"),
    ))
}

#[test]
fn device_prefix_commits_terminal_errors() {
    for (card, column, expected) in [
        ("V1 = 0 1", 4, "positive terminal"),
        ("C1 a = 1u", 6, "negative terminal"),
        ("R1 a", 5, "negative terminal"),
    ] {
        match parse(card).unwrap_err() {
            SpiceError::Parse { location, message } => {
                assert_eq!(location.line, 2);
                assert_eq!(location.column, column);
                assert!(message.contains(expected), "{message}");
            }
            other => panic!("{card}: terminal error was replaced with {other}"),
        }
    }
}

#[test]
fn repeat_cannot_swallow_a_missing_assignment_value() {
    for card in [
        "R1 a 0 1k tc1=",
        "C1 a 0 c=1u ic=",
        "V1 a 0 dc",
        "I1 a 0 2m dc=",
    ] {
        match parse(card).unwrap_err() {
            SpiceError::Parse { location, message } => {
                assert_eq!(location.column as usize, card.len() + 1, "{card}");
                assert!(message.contains("finite numeric literal"), "{message}");
            }
            other => panic!("{card}: incomplete assignment was replaced with {other}"),
        }
    }
}

#[test]
fn optional_numeric_slots_commit_non_finite_values() {
    for card in [
        "R1 a 0 1e999",
        "V1 a 0 1e999",
        "V1 a 0 ac 1e999",
        "I1 a 0 ac 1 1e999",
        "R1 a 0 1k tc1=1e999",
    ] {
        let expected_column = card.find("1e999").unwrap() + 1;
        match parse(card).unwrap_err() {
            SpiceError::Parse { location, .. } => {
                assert_eq!(location.column as usize, expected_column, "{card}");
            }
            other => panic!("{card}: non-finite literal became {other}"),
        }
    }
}

#[test]
fn optional_and_repeated_branches_preserve_domain_errors() {
    for (card, text, reference) in [
        ("V1 a 0 ac {gain}", "{gain}", "inp2v.c"),
        ("I1 a 0 dc 2m ac 1 {phase}", "{phase}", "inp2i.c"),
        ("R1 a 0 1k tc1={tc}", "{tc}", "inp2r.c"),
        ("C1 a 0 1u unknown=2", "unknown", "inp2c.c"),
        ("V1 a 0 dc 5 sin(0 1 1k)", "sin", "inp2v.c"),
    ] {
        match parse(card).unwrap_err() {
            SpiceError::NotYetPorted { what, c_reference } => {
                let column = card.find(text).unwrap() + 1;
                assert!(what.contains(&format!("winnow.cir:2:{column}:")), "{what}");
                assert!(c_reference.ends_with(reference), "{c_reference}");
            }
            other => panic!("{card}: domain error became {other}"),
        }
    }
}

#[test]
fn ac_lookahead_leaves_following_keywords_for_the_next_branch() {
    let netlist = parse("V1 a 0 ac 2 dc 5 ac 3 90").unwrap();
    let parameters: Vec<_> = netlist.devices[0]
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(
        parameters,
        [
            ("acmag", "2"),
            ("acphase", "0"),
            ("dc", "5"),
            ("acmag", "3"),
            ("acphase", "90"),
        ]
    );
    let netlist = parse("I1 a 0 ac dc 2m").unwrap();
    assert_eq!(netlist.devices[0].parameters.len(), 3);
    assert_eq!(netlist.devices[0].parameters[2].value, "2m");
}

#[test]
fn trailing_tokens_are_never_ignored_on_device_cards() {
    for card in [
        "R1 a 0 1k extra",
        "V1 a 0 dc 5 extra",
        "L1 a 0 1m ic=0 extra",
    ] {
        let error = parse(card).unwrap_err();
        assert!(error.is_not_yet_ported(), "{card}: {error}");
        let column = card.find("extra").unwrap() + 1;
        assert!(
            error
                .to_string()
                .contains(&format!("winnow.cir:2:{column}:")),
            "{error}"
        );
    }
}

#[test]
fn error_columns_remain_byte_based_for_unicode_nodes() {
    let card = "R1 α 0 1k tc1=";
    match parse(card).unwrap_err() {
        SpiceError::Parse { location, .. } => assert_eq!(location.column as usize, card.len() + 1),
        other => panic!("expected located parse error, got {other}"),
    }
}

#[test]
fn independent_calls_do_not_share_backtracking_state() {
    let parser = Parser::new();
    let deck = |body: &str| parse_deck_text(Path::new("state.cir"), &format!("Title\n{body}\n"));
    assert!(parser.parse_deck(&deck("R1 a 0 1k tc1=")).is_err());
    let valid = deck("V1 a 0 dc 5\nR1 a gnd 1k\n.op");
    let first = parser.parse_deck(&valid).unwrap();
    assert!(parser.parse_deck(&deck("V1 a 0 ac {gain}")).is_err());
    assert_eq!(parser.parse_deck(&valid).unwrap(), first);
}
