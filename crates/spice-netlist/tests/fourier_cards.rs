//! `.four` cards: positioned, typed Fourier requests (GitHub #44, frontend
//! half). Nothing here transforms a trace; that is `spice-analysis`'s job
//! (`docs/port/FOURIER.md`).
use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::ast::{
    DEFAULT_HARMONICS, FourierCard, MAX_HARMONICS, RequestedVector, ScopedCardKind, VectorComponent,
};
use spice_netlist::{ParsedDeck, Parser, semantic_eq, source::parse_deck_text, write_netlist};
use std::path::Path;

fn parse(body: &str) -> Result<ParsedDeck, SpiceError> {
    let deck = parse_deck_text(Path::new("four.cir"), &format!("Title\n{body}"));
    Parser::new().parse_deck_with_output(&deck)
}

fn cards(body: &str) -> Vec<FourierCard> {
    parse(body).expect("parses").fourier
}

fn one(body: &str) -> FourierCard {
    let mut cards = cards(body);
    assert_eq!(cards.len(), 1, "{body}");
    cards.remove(0)
}

#[test]
fn a_card_carries_its_fundamental_default_harmonics_and_vectors() {
    let card = one(".four 1k v(out)\n");
    assert_eq!(card.fundamental, 1.0e3);
    assert_eq!(card.harmonics, DEFAULT_HARMONICS);
    assert_eq!(card.harmonics_location, None);
    assert_eq!(card.vectors.len(), 1);
    assert_eq!(
        card.vectors[0].vector,
        RequestedVector::Voltage {
            positive: "out".to_owned(),
            negative: None,
        }
    );
    assert_eq!((card.location.line, card.location.column), (2, 1));
    assert_eq!(
        (
            card.fundamental_location.line,
            card.fundamental_location.column
        ),
        (2, 7)
    );
    assert_eq!(
        (
            card.vectors[0].location.line,
            card.vectors[0].location.column
        ),
        (2, 10)
    );
}

#[test]
fn the_harmonic_count_is_a_typed_setter_anywhere_after_the_frequency() {
    let card = one(".four 2.5e3 HARMONICS=3 v(a) v(a,b) i(V1)\n");
    assert_eq!(card.fundamental, 2.5e3);
    assert_eq!(card.harmonics, 3);
    assert_eq!(
        card.harmonics_location.as_ref().map(|at| at.column),
        Some(13)
    );
    assert_eq!(card.vectors.len(), 3);
    assert_eq!(
        card.vectors[1].vector,
        RequestedVector::Voltage {
            positive: "a".to_owned(),
            negative: Some("b".to_owned()),
        }
    );
    // Device names are lowercased, so `i(V1)` matches the plot's `i(v1)`.
    assert_eq!(
        card.vectors[2].vector,
        RequestedVector::Current {
            device: "v1".to_owned(),
        }
    );

    // The same setter after the vectors, in either spelling.
    let card = one(".four 1k v(out) harmonics=12\n");
    assert_eq!(card.harmonics, 12);
    assert_eq!(card.vectors.len(), 1);
}

#[test]
fn the_widest_accepted_harmonic_count_is_the_ports_budget() {
    let card = one(&format!(".four 1k HARMONICS={MAX_HARMONICS} v(out)\n"));
    assert_eq!(card.harmonics, MAX_HARMONICS);
    let error = parse(&format!(
        ".four 1k HARMONICS={} v(out)\n",
        MAX_HARMONICS + 1
    ))
    .expect_err("beyond the budget");
    assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
    assert!(
        error.to_string().contains("bounded Fourier budget"),
        "{error}"
    );
    assert!(error.to_string().contains("four.cir:2:20"), "{error}");
}

#[test]
fn a_frequency_that_is_not_a_positive_literal_is_a_positioned_parse_error() {
    for body in [
        ".four\n",
        ".four v(out)\n",
        ".four 0 v(out)\n",
        ".four -1k v(out)\n",
    ] {
        let error = parse(body).expect_err(body);
        assert!(matches!(error, SpiceError::Parse { .. }), "{body}: {error}");
        assert!(error.to_string().contains("four.cir:2:"), "{body}: {error}");
    }
    // A braced expression is valid numparam the port does not evaluate here.
    let error = parse(".four {1/(2m)} v(out)\n").expect_err("a braced frequency");
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(
        error
            .to_string()
            .contains("as the .four fundamental frequency"),
        "{error}"
    );
}

#[test]
fn a_harmonic_count_that_is_not_a_whole_positive_number_is_rejected() {
    for (body, message) in [
        (".four 1k HARMONICS=\n", "has no value"),
        (
            ".four 1k HARMONICS= v(out)\n",
            "whole number after 'HARMONICS='",
        ),
        (
            ".four 1k HARMONICS=x v(out)\n",
            "whole number after 'HARMONICS='",
        ),
        (".four 1k HARMONICS=0 v(out)\n", "at least 1"),
        (".four 1k HARMONICS=1.5 v(out)\n", "at least 1"),
        (".four 1k HARMONICS=-2 v(out)\n", "at least 1"),
        (
            ".four 1k HARMONICS=3 HARMONICS=4 v(out)\n",
            "HARMONICS= is given more than once",
        ),
    ] {
        let error = parse(body).expect_err(body);
        assert!(matches!(error, SpiceError::Parse { .. }), "{body}: {error}");
        assert!(error.to_string().contains(message), "{body}: {error}");
    }
    let error = parse(".four 1k HARMONICS={n} v(out)\n").expect_err("a braced count");
    assert!(error.is_not_yet_ported(), "{error}");
}

#[test]
fn the_parameters_c_takes_from_the_shell_and_unknown_ones_are_named() {
    for name in ["nfreqs", "nperiods", "polydegree", "fourgridsize"] {
        let error = parse(&format!(".four 1k {name}=4 v(out)\n")).expect_err(name);
        assert!(error.is_not_yet_ported(), "{name}: {error}");
        assert!(
            error
                .to_string()
                .contains("from an interactive `set` variable"),
            "{name}: {error}"
        );
    }
    let error = parse(".four 1k FROBNICATE=4 v(out)\n").expect_err("an unknown parameter");
    assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
    assert!(
        error.to_string().contains("no such .four parameter"),
        "{error}"
    );
    assert!(error.to_string().contains("HARMONICS=<n>"), "{error}");
}

#[test]
fn a_card_needs_at_least_one_vector_and_rejects_all_and_ac_components() {
    let error = parse(".four 1k\n").expect_err("no vector");
    assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
    assert!(
        error.to_string().contains("expected at least one vector"),
        "{error}"
    );
    let error = parse(".four 1k HARMONICS=2\n").expect_err("no vector");
    assert!(
        error.to_string().contains("expected at least one vector"),
        "{error}"
    );

    let error = parse(".four 1k all\n").expect_err("all");
    assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
    assert!(
        error.to_string().contains("cannot be transformed"),
        "{error}"
    );

    for spelling in ["vm(out)", "vp(out,0)", "vdb(out)"] {
        let error = parse(&format!(".four 1k {spelling}\n")).expect_err(spelling);
        assert!(
            matches!(error, SpiceError::Parse { .. }),
            "{spelling}: {error}"
        );
        assert!(
            error.to_string().contains("is an AC component"),
            "{spelling}: {error}"
        );
    }
}

#[test]
fn the_save_grammars_own_failures_are_reused_position_and_all() {
    let error = parse(".four 1k i(r1)\n").expect_err("a resistor current");
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(
        error
            .to_string()
            .contains("only a voltage source or inductor"),
        "{error}"
    );
    let error = parse(".four 1k power(v1)\n").expect_err("an unknown request");
    assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
    assert!(
        error.to_string().contains("unknown vector request"),
        "{error}"
    );
    let error = parse(".four 1k v(out\n").expect_err("a missing )");
    assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
    assert!(error.to_string().contains("')' after the node"), "{error}");
}

#[test]
fn four_is_a_post_processing_card_and_not_an_analysis() {
    // Before this work a `.four` line was classified as an analysis card, so a
    // deck with one had two analyses; now the transient is the only one.
    let parsed = parse(".four 1k v(out)\nr1 a b 1k\nv1 a 0 1\n.tran 1u 1m\n").expect("parses");
    assert_eq!(parsed.netlist.analyses.len(), 1);
    assert_eq!(
        parsed.netlist.analyses[0].kind,
        AnalysisKind::Transient,
        "the .four card is not an analysis"
    );
    assert_eq!(parsed.fourier.len(), 1);
    assert!(
        parsed
            .netlist
            .cards
            .iter()
            .any(|card| card.kind == ScopedCardKind::Fourier)
    );
    assert!(
        parsed.output.is_empty(),
        "`.four` is not a .save/.print card"
    );
    // `.print four` still names the analysis it writes, exactly as before.
    let parsed = parse(".print four v(out)\n.tran 1u 1m\n").expect("parses");
    assert_eq!(parsed.output.prints.len(), 1);
    assert_eq!(parsed.output.prints[0].analysis, AnalysisKind::Fourier);
    assert!(parsed.fourier.is_empty());
}

#[test]
fn a_body_local_four_card_is_rejected_explicitly() {
    let error =
        parse(".subckt s a b\nr1 a b 1k\n.four 1k v(a)\n.ends\n").expect_err("body-local .four");
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(
        error.to_string().contains("inside a .subckt body"),
        "{error}"
    );
}

#[test]
fn the_netlist_keeps_the_card_and_the_writer_round_trips_it() {
    let body = "r1 a b 1k\nv1 a 0 dc 1\n.four 1k v(b) i(v1) HARMONICS=4\n.tran 1u 1m\n";
    let deck = parse_deck_text(Path::new("four.cir"), &format!("Title\n{body}"));
    let parsed = Parser::new().parse_deck_with_output(&deck).unwrap();
    let netlist = Parser::new().parse_deck(&deck).unwrap();
    assert_eq!(netlist.top_level_device_count(), 2);
    let written = write_netlist(&netlist).expect("the writer reproduces the card");
    assert!(
        written.contains(".four 1k v(b) i(v1) HARMONICS=4"),
        "{written}"
    );
    let reparsed = parse_deck_text(Path::new("four.cir"), &written);
    assert!(semantic_eq(
        &Parser::new().parse_deck(&deck).unwrap(),
        &Parser::new().parse_deck(&reparsed).unwrap()
    ));
    assert_eq!(
        Parser::new()
            .parse_deck_with_output(&reparsed)
            .unwrap()
            .fourier,
        parsed.fourier
    );
}

#[test]
fn four_cards_are_part_of_semantic_equality_and_survive_a_semantic_form() {
    // The typed request lives in `ParsedDeck::fourier`, so the card's spelling
    // is the netlist's only record of what was asked for: two decks that differ
    // only in their `.four` request must not compare equal, and the writer must
    // still reproduce the card from a semantic form.
    let left = parse_deck_text(Path::new("four.cir"), "Title\n.four 1k v(a)\n.tran 1u 1m\n");
    let right = parse_deck_text(Path::new("four.cir"), "Title\n.four 2k v(a)\n.tran 1u 1m\n");
    let left = Parser::new().parse_deck(&left).unwrap();
    let right = Parser::new().parse_deck(&right).unwrap();
    assert!(
        !semantic_eq(&left, &right),
        "different .four requests are not semantically equal"
    );
    assert!(semantic_eq(&left, &left.clone()));

    let semantic = spice_netlist::semantic::semantic_form(&left);
    let written = write_netlist(&semantic).expect("the writer reproduces the card");
    assert!(written.contains(".four 1k v(a)"), "{written}");
    let reparsed = parse_deck_text(Path::new("four.cir"), &written);
    assert!(semantic_eq(
        &left,
        &Parser::new().parse_deck(&reparsed).unwrap()
    ));
}

#[test]
fn a_component_spelling_of_an_ac_card_is_recognised_from_its_function_word() {
    // The AC spellings are the `.save` grammar's; `.four` refuses them all, and
    // the diagnostic names the spelling that was written.
    assert_eq!(VectorComponent::Magnitude.function(), "vm");
    let error = parse(".four 1k vr(out)\n").expect_err("a real part");
    assert!(error.to_string().contains("vr(...)"), "{error}");
}
