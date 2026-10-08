//! K mutual-inductance grammar (`inp2k.c`, the multi-inductor rewrite of
//! `inp_compat()`) and its writer round trip (GitHub #80).
use std::path::Path;

use spice_core::SpiceError;
use spice_netlist::ast::{DeviceInstance, Netlist, ParameterKind};
use spice_netlist::source::parse_deck_text;
use spice_netlist::{Parser, semantic_diff, semantic_eq, write_netlist};

fn try_parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("in.cir"),
        &format!("mutual\n{body}\n.end\n"),
    ))
}

fn device(body: &str) -> DeviceInstance {
    let netlist = try_parse(body).unwrap_or_else(|error| panic!("{error}\n{body}"));
    netlist.devices.last().unwrap().clone()
}

/// `(name, value, kind)` of each setter, in application order.
fn setters(device: &DeviceInstance) -> Vec<(String, String, &'static str)> {
    device
        .parameters
        .iter()
        .map(|p| {
            let kind = match p.kind {
                ParameterKind::Scalar => "scalar",
                ParameterKind::Expression(_) => "expression",
                ParameterKind::Instance => "instance",
                _ => "other",
            };
            (p.name.clone(), p.value.clone(), kind)
        })
        .collect()
}

fn s(name: &str, value: &str, kind: &'static str) -> (String, String, &'static str) {
    (name.to_owned(), value.to_owned(), kind)
}

fn parse_error(body: &str) -> (u32, String) {
    match try_parse(body) {
        Err(SpiceError::Parse { location, message }) => (location.column, message),
        other => panic!("expected a parse error for {body:?}, got {other:?}"),
    }
}

#[test]
fn positional_and_named_couplings_parse_to_one_coefficient_setter() {
    let k = device("K1 L1 Lout 0.5");
    assert_eq!((k.name.as_str(), k.designator), ("k1", 'k'));
    assert!(k.nodes.is_empty());
    assert_eq!(k.model, None);
    assert_eq!(
        setters(&k),
        [
            s("inductor1", "l1", "instance"),
            s("inductor2", "lout", "instance"),
            s("coefficient", "0.5", "scalar"),
        ]
    );
    // Instance references are located at their names, a positional value at
    // the value.
    assert_eq!(k.parameters[0].location.column, 4);
    assert_eq!(k.parameters[1].location.column, 7);
    assert_eq!(k.parameters[2].location.column, 12);

    for (text, column) in [
        ("k1 l1 l2 k=-0.25", 10),
        ("k1 l1 l2 K = -0.25", 10),
        ("k1 l1 l2 coefficient=-0.25", 10),
    ] {
        let k = device(text);
        assert_eq!(k.parameters[2].name, "coefficient", "{text}");
        assert_eq!(k.parameters[2].value, "-0.25", "{text}");
        // A named setter is located at its keyword.
        assert_eq!(k.parameters[2].location.column, column, "{text}");
    }
    let braced = device("k1 l1 l2 {kk*2}");
    assert!(matches!(
        braced.parameters[2].kind,
        ParameterKind::Expression(_)
    ));
}

#[test]
fn every_word_before_the_coupling_is_an_inductor() {
    let k = device("k1 l1 l2 l3 l.x1.la 0.9");
    assert_eq!(
        setters(&k),
        [
            s("inductor1", "l1", "instance"),
            s("inductor2", "l2", "instance"),
            s("inductor3", "l3", "instance"),
            s("inductor4", "l.x1.la", "instance"),
            s("coefficient", "0.9", "scalar"),
        ]
    );
    assert_eq!(device("k1 l1 l2 l3 k=0.9").parameters.len(), 4);
}

#[test]
fn malformed_cards_are_parse_errors_at_the_offending_token() {
    // C silently couples with k = 0 when the value is missing.
    let (column, message) = parse_error("k1 l1 l2");
    assert_eq!(column, 9);
    assert!(message.contains("coupling coefficient"), "{message}");
    let (_, message) = parse_error("k1 l1");
    assert!(message.contains("at least two inductors"), "{message}");
    let (column, message) = parse_error("k1 l1 0.5");
    assert_eq!(column, 7);
    assert!(message.contains("at least two inductors"), "{message}");
    // C reads extra words as inductor names or unknown parameters.
    let (column, message) = parse_error("k1 l1 l2 0.5 k=0.1");
    assert_eq!(column, 14);
    assert!(message.contains("after the coupling"), "{message}");
    let (column, _) = parse_error("k1 l1 l2 k=0.1 0.5");
    assert_eq!(column, 16);
    let (_, message) = parse_error("k1 (l1 l2) 0.5");
    assert!(message.contains("inductor name"), "{message}");
    assert!(try_parse("k1 l1 l2 1e999").is_err());
    assert!(try_parse("k1").is_err());
}

#[test]
fn k_cards_round_trip_through_the_writer() {
    let deck = "l1 a 0 1m\nl2 b 0 1m\nl3 c 0 1m\nr1 a 0 1\nr2 b 0 1\nr3 c 0 1\n\
                K1 L1 l2 0.5\nk2 l1 l3 k=-0.2\nk3 l1 l2 l3 coefficient={0.1*2}\n\
                .subckt xf p\nla p 0 1m\nlb p 0 2m\nkab la lb 0.3\n.ends\n.op";
    let netlist = try_parse(deck).unwrap();
    let text = write_netlist(&netlist).unwrap();
    assert!(text.contains("k1 l1 l2 0.5"), "{text}");
    assert!(text.contains("k2 l1 l3 -0.2"), "{text}");
    assert!(text.contains("k3 l1 l2 l3 {0.1*2}"), "{text}");
    let reparsed = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("out.cir"), &text))
        .unwrap();
    assert!(
        semantic_eq(&netlist, &reparsed),
        "{:?}",
        semantic_diff(&netlist, &reparsed)
    );
    assert_eq!(write_netlist(&reparsed).unwrap(), text);
}

#[test]
fn unrepresentable_k_asts_are_refused_by_the_writer() {
    let netlist = try_parse("l1 a 0 1m\nl2 b 0 1m\nk1 l1 l2 0.5").unwrap();
    let refuse = |edit: &dyn Fn(&mut DeviceInstance)| {
        let mut netlist = netlist.clone();
        edit(netlist.devices.last_mut().unwrap());
        write_netlist(&netlist).expect_err("unrepresentable")
    };
    refuse(&|k| k.parameters.pop().map(|_| ()).unwrap());
    refuse(&|k| {
        k.parameters.remove(1);
    });
    refuse(&|k| k.parameters[2].name = "gain".to_owned());
    refuse(&|k| k.parameters[0].kind = ParameterKind::Scalar);
    refuse(&|k| k.parameters[1].value = "1.5".to_owned());
    refuse(&|k| k.nodes.push("a".to_owned()));
    refuse(&|k| k.model = Some("m".to_owned()));
}
