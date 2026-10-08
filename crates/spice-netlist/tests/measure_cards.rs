//! `.measure`/`.meas` cards: positioned, typed requests (GitHub #43, frontend
//! half). Nothing here evaluates a measurement; that is `spice-analysis`'s job
//! (`docs/port/MEASURE.md`).
use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::ast::{
    MeasureCard, MeasureEvent, MeasureRequest, MeasureStatistic, MeasureTransition,
    RequestedVector, ScopedCardKind,
};
use spice_netlist::{ParsedDeck, Parser, semantic_eq, source::parse_deck_text, write_netlist};
use std::path::Path;

fn parse(body: &str) -> Result<ParsedDeck, SpiceError> {
    let deck = parse_deck_text(Path::new("measure.cir"), &format!("Title\n{body}"));
    Parser::new().parse_deck_with_output(&deck)
}

fn measures(body: &str) -> Vec<MeasureCard> {
    parse(body).expect("parses").measurements
}

fn one(body: &str) -> MeasureCard {
    let mut cards = measures(body);
    assert_eq!(cards.len(), 1, "{body}");
    cards.remove(0)
}

#[test]
fn a_card_carries_its_analysis_name_and_operation() {
    let card = one(".meas tran tdelay trig v(in) val=0.5 rise=1 targ v(out) val=0.5 rise=1\n");
    assert_eq!(card.analysis, AnalysisKind::Transient);
    assert_eq!(card.name, "tdelay");
    assert_eq!((card.location.line, card.location.column), (2, 1));
    assert_eq!(
        (card.analysis_location.line, card.analysis_location.column),
        (2, 7)
    );
    assert_eq!(
        (card.name_location.line, card.name_location.column),
        (2, 12)
    );
    let MeasureRequest::TrigTarg { trig, targ, window } = card.request else {
        panic!("expected the TRIG/TARG form: {:?}", card.request);
    };
    assert!(window.is_unbounded());
    let MeasureEvent::Crossing {
        operand,
        value,
        transition,
        ..
    } = trig
    else {
        panic!("expected a crossing: {trig:?}");
    };
    assert_eq!(value, 0.5);
    assert_eq!(transition, MeasureTransition::Rise(1));
    assert_eq!(
        operand.vector,
        RequestedVector::Voltage {
            positive: "in".to_owned(),
            negative: None,
        }
    );
    let MeasureEvent::Crossing {
        operand,
        transition,
        ..
    } = targ
    else {
        panic!("expected a crossing: {targ:?}");
    };
    assert_eq!(transition, MeasureTransition::Rise(1));
    assert_eq!(
        operand.vector,
        RequestedVector::Voltage {
            positive: "out".to_owned(),
            negative: None,
        }
    );
}

#[test]
fn every_supported_operation_parses_with_its_window() {
    let card = one(".measure tran vat find v(out,0) at=1m from=2u to=3m\n");
    let MeasureRequest::Find {
        operand,
        at,
        at_location,
        window,
    } = card.request
    else {
        panic!("expected FIND: {:?}", card.request);
    };
    assert_eq!(at, 1e-3);
    assert_eq!((at_location.line, at_location.column), (2, 33));
    assert_eq!(window.from, Some(2e-6));
    assert_eq!(window.to, Some(3e-3));
    assert_eq!(
        operand.vector,
        RequestedVector::Voltage {
            positive: "out".to_owned(),
            negative: Some("0".to_owned()),
        }
    );

    for (card, statistic, spelling) in [
        ("min", MeasureStatistic::Min, "MIN"),
        ("max", MeasureStatistic::Max, "MAX"),
        ("avg", MeasureStatistic::Avg, "AVG"),
        ("rms", MeasureStatistic::Rms, "RMS"),
        ("integ", MeasureStatistic::Integ, "INTEG"),
        ("integral", MeasureStatistic::Integ, "INTEG"),
        ("INTEGRAL", MeasureStatistic::Integ, "INTEG"),
    ] {
        let card = one(&format!(".meas tran vout_max {card} v(out) from=0 to=1m\n"));
        assert_eq!(statistic.name(), spelling);
        let MeasureRequest::Statistic {
            statistic: parsed,
            operand,
            window,
        } = card.request
        else {
            panic!("expected a statistic: {:?}", card.request);
        };
        assert_eq!(parsed, statistic);
        assert_eq!(window.from, Some(0.0));
        assert_eq!(window.to, Some(1e-3));
        assert_eq!(
            operand.vector,
            RequestedVector::Voltage {
                positive: "out".to_owned(),
                negative: None,
            }
        );
    }
}

#[test]
fn the_crossing_selector_covers_counts_and_last_in_both_spellings() {
    for (text, expected) in [
        ("rise=2", MeasureTransition::Rise(2)),
        ("fall=3", MeasureTransition::Fall(3)),
        ("cross=1", MeasureTransition::Cross(1)),
        ("last", MeasureTransition::Last),
        ("rise=last", MeasureTransition::Last),
        ("FALL=LAST", MeasureTransition::Last),
        ("cross=Last", MeasureTransition::Last),
    ] {
        let card = one(&format!(
            ".meas tran t trig v(in) val=1 {text} targ v(out) val=1 rise=1\n"
        ));
        let MeasureRequest::TrigTarg { trig, .. } = card.request else {
            panic!("expected the TRIG/TARG form");
        };
        let MeasureEvent::Crossing { transition, .. } = trig else {
            panic!("expected a crossing");
        };
        assert_eq!(transition, expected, "{text}");
    }
    // No selector is the first crossing in either direction.
    let card = one(".meas tran t trig v(in) val=1 targ v(out) val=1\n");
    let MeasureRequest::TrigTarg { trig, .. } = card.request else {
        panic!("expected the TRIG/TARG form");
    };
    let MeasureEvent::Crossing { transition, .. } = trig else {
        panic!("expected a crossing");
    };
    assert_eq!(transition, MeasureTransition::First);
}

#[test]
fn a_trig_clause_may_be_an_axis_value_and_windows_must_agree() {
    let card = one(".meas tran t trig at=1u targ v(out) val=0.5 rise=1\n");
    let MeasureRequest::TrigTarg { trig, window, .. } = card.request else {
        panic!("expected the TRIG/TARG form");
    };
    assert!(matches!(trig, MeasureEvent::At { .. }));
    assert!(window.is_unbounded());

    let card = one(".meas tran t trig v(a) val=1 from=0 targ v(b) val=1 to=1m\n");
    let MeasureRequest::TrigTarg { window, .. } = card.request else {
        panic!("expected the TRIG/TARG form");
    };
    assert_eq!(window.from, Some(0.0));
    assert_eq!(window.to, Some(1e-3));

    // A bound written twice with different values is a positioned failure, not
    // a silent choice.
    let error = parse(".meas tran t trig v(a) val=1 from=1 targ v(b) val=1 from=2\n")
        .expect_err("conflicting windows");
    assert!(error.to_string().contains("measure.cir:2:1"), "{error}");
    assert!(error.to_string().contains("FROM="), "{error}");
    assert!(error.to_string().contains("given twice"), "{error}");
}

#[test]
fn unsupported_operations_and_parameters_are_not_yet_ported() {
    let not_ported = [
        (".meas tran x when v(out)=2\n", "the when measurement"),
        (".meas tran x pp v(out)\n", "the pp measurement"),
        (".meas tran x deriv v(out) at=1m\n", "the deriv measurement"),
        (".meas tran x find v(out) when v(in)=2\n", "the WHEN form"),
        (".meas tran x avg v(out) td=1u\n", "TD=<value>"),
        (
            ".meas tran x min v(out) to={1m}\n",
            "a {…} expression as the value of to=",
        ),
        (
            ".meas sp x avg v(out)\n",
            "S-parameter analysis has no driver",
        ),
    ];
    for (body, expected) in not_ported {
        let error = parse(body).expect_err(body);
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains(expected), "{body}: {error}");
    }
}

#[test]
fn an_analysis_no_measurement_can_be_taken_on_is_unsupported() {
    for body in [".meas op x avg v(out)\n", ".meas noise x max v(out)\n"] {
        let error = parse(body).expect_err(body);
        assert!(!error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains("measure"), "{body}: {error}");
        assert!(
            error.to_string().contains("measure.cir:2:7"),
            "{body}: {error}"
        );
    }
}

#[test]
fn malformed_cards_are_positioned_parse_errors() {
    let failures = [
        (".measure\n", "expected an analysis name after .measure"),
        (".measure 2 avg v(out)\n", "expected an analysis name"),
        (".measure tran\n", "expected a result name"),
        (
            ".measure tran t frobnicate v(out)\n",
            "no such measurement as 'frobnicate'",
        ),
        (
            ".measure tran t\n",
            "expected an operation after the result name",
        ),
        (".measure tran t avg\n", "AVG needs a vector operand"),
        (".measure tran t avg all\n", "'all' cannot be measured"),
        (
            ".measure tran t avg v(out) foo=1\n",
            "no such .measure parameter",
        ),
        (".measure tran t avg v(out) from\n", "'from' needs a value"),
        (
            ".measure tran t avg v(out) from=x\n",
            "expected a finite numeric value",
        ),
        (
            ".measure tran t avg v(out) from=1 from=2\n",
            "given more than once",
        ),
        (".measure tran t find v(out)\n", "FIND needs AT=<value>"),
        (
            ".measure tran t find v(out) val=1\n",
            "VAL= is not a FIND parameter",
        ),
        (".measure tran t avg v(out) at=1m\n", "not AVG parameters"),
        (".measure tran t min v(out) val=1\n", "not MIN parameters"),
        (
            ".measure tran t trig v(a) val=1 rise=1 fall=1 targ v(b) val=1\n",
            "at most one crossing selector",
        ),
        (
            ".measure tran t trig v(a) val=1 rise=0 targ v(b) val=1\n",
            "whole crossing number of at least 1",
        ),
        (
            ".measure tran t trig v(a) val=1\n",
            "TRIG needs a TARG clause",
        ),
        (
            ".measure tran t trig v(a) targ v(b) val=1\n",
            "TRIG needs AT=<value> or VAL=<value>",
        ),
        (
            ".measure tran t trig at=0 rise=1 targ v(b) val=1\n",
            "AT=<value> takes no RISE",
        ),
    ];
    for (body, expected) in failures {
        let error = parse(body).expect_err(body);
        assert!(!error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains(expected), "{body}: {error}");
        assert!(
            error.to_string().contains("measure.cir:2"),
            "{body}: the message names the card's position: {error}"
        );
    }
}

#[test]
fn the_analysis_word_may_carry_its_dot_and_only_two_card_names_are_known() {
    // `.print` accepts the dotted analysis spelling, and so does `.measure`.
    let card = one(".measure .TRAN vat find v(out) at=1m\n");
    assert_eq!(card.analysis, AnalysisKind::Transient);
    // C's `ciprefix(".meas", …)` also accepts `.measurement`; the port's card
    // classifier knows only the two spellings it publishes.
    // Duplicate result names are two cards, in deck order, not one merged card.
    let cards = measures(".meas tran m max v(out)\n.meas tran m min v(out)\n");
    assert_eq!(cards.len(), 2);
    assert_eq!(
        cards
            .iter()
            .map(|card| card.name.as_str())
            .collect::<Vec<_>>(),
        ["m", "m"]
    );
    let error = parse(".measurement tran x max v(out)\n").expect_err(".measurement");
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains(".measurement"), "{error}");
}

#[test]
fn a_body_local_measure_card_is_rejected_explicitly() {
    let error = parse(".subckt s a b\nr1 a b 1k\n.measure tran x max v(a)\n.ends\n")
        .expect_err("body-local .measure");
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(
        error.to_string().contains("inside a .subckt body"),
        "{error}"
    );
}

#[test]
fn the_netlist_keeps_the_card_and_the_writer_round_trips_it() {
    let body = "r1 a b 1k\nv1 a 0 dc 1\n.meas tran vat find v(b) at=1m\n.tran 1u 1m\n";
    let deck = parse_deck_text(Path::new("measure.cir"), &format!("Title\n{body}"));
    let parsed = Parser::new().parse_deck_with_output(&deck).unwrap();
    // The netlist still describes the circuit; the ordered cards record the
    // measurement as `Measure`, and `parse_deck` still returns the netlist.
    assert!(
        parsed
            .netlist
            .cards
            .iter()
            .any(|card| card.kind == ScopedCardKind::Measure)
    );
    let netlist = Parser::new().parse_deck(&deck).unwrap();
    assert_eq!(netlist.top_level_device_count(), 2);
    let written = write_netlist(&netlist).expect("the writer reproduces the card");
    assert!(
        written.contains(".meas tran vat find v(b) at=1m"),
        "{written}"
    );
    let reparsed = parse_deck_text(Path::new("measure.cir"), &written);
    assert!(semantic_eq(
        &Parser::new().parse_deck(&deck).unwrap(),
        &Parser::new().parse_deck(&reparsed).unwrap()
    ));
    assert_eq!(
        Parser::new()
            .parse_deck_with_output(&reparsed)
            .unwrap()
            .measurements,
        parsed.measurements
    );
}

#[test]
fn measure_cards_are_part_of_semantic_equality_and_survive_a_semantic_form() {
    // The typed request lives in `ParsedDeck::measurements`, so the card's
    // spelling is the netlist's only record of what was asked for: two decks
    // that differ only in their `.measure` request must not compare equal, and
    // the writer must still reproduce the card from a semantic form.
    let left = parse_deck_text(
        Path::new("measure.cir"),
        "Title\n.meas tran vat find v(a) at=1m\n.op\n",
    );
    let right = parse_deck_text(
        Path::new("measure.cir"),
        "Title\n.meas tran vat find v(a) at=2m\n.op\n",
    );
    let left = Parser::new().parse_deck(&left).unwrap();
    let right = Parser::new().parse_deck(&right).unwrap();
    assert!(
        !semantic_eq(&left, &right),
        "different .measure requests are not semantically equal"
    );
    assert!(semantic_eq(&left, &left.clone()));

    let semantic = spice_netlist::semantic::semantic_form(&left);
    let written = write_netlist(&semantic).expect("the writer reproduces the card");
    assert!(
        written.contains(".meas tran vat find v(a) at=1m"),
        "{written}"
    );
    let reparsed = parse_deck_text(Path::new("measure.cir"), &written);
    assert!(semantic_eq(
        &left,
        &Parser::new().parse_deck(&reparsed).unwrap()
    ));
}

#[test]
fn an_operand_uses_the_save_spelling_and_keeps_its_position() {
    let card = one(".meas tran i1 avg i(V1) from=1u to=2u\n");
    let MeasureRequest::Statistic { operand, .. } = card.request else {
        panic!("expected a statistic");
    };
    // Device names are lowercased, so `i(V1)` matches the plot's `i(v1)`.
    assert_eq!(
        operand.vector,
        RequestedVector::Current {
            device: "v1".to_owned(),
        }
    );
    assert_eq!((operand.location.line, operand.location.column), (2, 19));

    let card = one(".meas tran vm1 max vm(OUT)\n");
    let MeasureRequest::Statistic { operand, .. } = card.request else {
        panic!("expected a statistic");
    };
    assert_eq!(
        operand.vector,
        RequestedVector::Component {
            component: spice_netlist::ast::VectorComponent::Magnitude,
            positive: "out".to_owned(),
            negative: None,
        }
    );

    // The save grammar's own failures are reused, position and all.
    let error = parse(".meas tran x avg i(r1)\n").expect_err("a resistor current");
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(
        error
            .to_string()
            .contains("only a voltage source or inductor"),
        "{error}"
    );
    let error = parse(".meas tran x avg power(v1)\n").expect_err("an unknown request");
    assert!(
        error.to_string().contains("unknown vector request"),
        "{error}"
    );
}
