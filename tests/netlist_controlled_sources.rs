//! Linear E/F/G/H grammar (`inp2e.c` … `inp2h.c`, `inp_compat`,
//! `inp_check_syntax`) and its writer round trip (GitHub #78).
use std::path::Path;

use ngspice_rs::netlist::ast::{DeviceInstance, Netlist, ParameterKind};
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::netlist::{Parser, semantic_diff, write_netlist};
use ngspice_rs::primitives::SpiceError;

fn try_parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("in.cir"),
        &format!("controlled\n{body}\n.end\n"),
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

#[test]
fn the_four_linear_forms_parse_with_c_terminal_and_setter_order() {
    let e = device("E1 Out 0 In GND 10");
    assert_eq!((e.name.as_str(), e.designator), ("e1", 'e'));
    assert_eq!(e.nodes, ["out", "0", "in", "0"]);
    assert_eq!(e.model, None);
    assert_eq!(setters(&e), [s("gain", "10", "scalar")]);
    // The gain is located at its value; the card at its first column.
    assert_eq!(e.parameters[0].location.column, 17);

    let g = device("g1 0 o in 0 2m m=3");
    assert_eq!(g.nodes, ["0", "o", "in", "0"]);
    // C applies the leading gain after the named setters.
    assert_eq!(
        setters(&g),
        [s("m", "3", "scalar"), s("gain", "2m", "scalar")]
    );

    let f = device("F1 0 o VSense 3");
    assert_eq!(f.nodes, ["0", "o"]);
    assert_eq!(
        setters(&f),
        [s("control", "vsense", "instance"), s("gain", "3", "scalar")]
    );
    assert_eq!(f.parameters[0].location.column, 8);

    let h = device("h1 o 0 vs gain=1k");
    assert_eq!(
        setters(&h),
        [s("control", "vs", "instance"), s("gain", "1k", "scalar")]
    );
    // A named gain is located at its keyword.
    assert_eq!(h.parameters[1].location.column, 11);

    let braced = device("e1 o 0 a b {2*k}");
    assert!(matches!(
        braced.parameters[0].kind,
        ParameterKind::Expression(_)
    ));
}

#[test]
fn named_setter_tails_keep_c_application_order() {
    assert_eq!(
        setters(&device("g1 0 o a b gain=2m m=3")),
        [s("gain", "2m", "scalar"), s("m", "3", "scalar")]
    );
    assert_eq!(
        setters(&device("f1 0 o v1 1 m=2 gain=3")),
        [
            s("control", "v1", "instance"),
            s("m", "2", "scalar"),
            s("gain", "3", "scalar"),
            s("gain", "1", "scalar"),
        ]
    );
}

#[test]
fn parenthesized_controls_are_accepted_like_inpgetnettok() {
    for text in [
        "e1 o 0 (in, 0) 4",
        "e1 o 0 (in 0) 4",
        "e1 o 0 (in) (0) 4",
        "e1 o 0 in,0 4",
    ] {
        let e = device(text);
        assert_eq!(e.nodes, ["o", "0", "in", "0"], "{text}");
        assert_eq!(setters(&e), [s("gain", "4", "scalar")], "{text}");
    }
    let h = device("h1 o 0 (vs) 2");
    assert_eq!(h.parameters[0].value, "vs");
}

#[test]
fn the_hspice_keyword_is_removed_only_where_inp_compat_removes_it() {
    for (text, nodes) in [
        ("e1 o 0 vcvs a b 2", &["o", "0", "a", "b"][..]),
        ("g1 o 0 VCCS a b 2m", &["o", "0", "a", "b"]),
        ("f1 o 0 cccs v1 2", &["o", "0"]),
        ("h1 o 0 ccvs v1 2", &["o", "0"]),
    ] {
        let d = device(text);
        assert_eq!(d.nodes, nodes, "{text}");
        if matches!(d.designator, 'f' | 'h') {
            assert_eq!(d.parameters[0].value, "v1", "{text}");
        }
    }
    // inp_remove_ws() runs first, so `gain = 2` counts as one word.
    for (text, nodes) in [
        ("e1 o 0 vcvs a 0 gain = 2", &["o", "0", "a", "0"][..]),
        ("g1 o 0 vccs a 0 gain= 2m", &["o", "0", "a", "0"]),
        ("f1 o 0 cccs v1 gain =2", &["o", "0"]),
    ] {
        let d = device(text);
        assert_eq!(d.nodes, nodes, "{text}");
        assert_eq!(
            d.parameters
                .last()
                .map(|p| (p.name.as_str(), p.value.as_str())),
            Some(("gain", text.rsplit(['=', ' ']).next().unwrap())),
            "{text}"
        );
    }
    // With another token count C keeps the word as a node name.
    let e = device("e1 o 0 vcvs a 2");
    assert_eq!(e.nodes, ["o", "0", "vcvs", "a"]);
    // The keyword of another device type is an ordinary node, too.
    let e = device("e1 o 0 vccs a 3");
    assert_eq!(e.nodes, ["o", "0", "vccs", "a"]);
    // C would blank four characters of a longer word and rename the node.
    let error = try_parse("e1 o 0 vcvsx a b 2").unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}

#[test]
fn nonlinear_forms_parse_and_the_remaining_gaps_are_not_yet_ported() {
    // POLY/VALUE/TABLE and the implicit POLY(1) are behavioural forms (#79,
    // tests/behavioural_sources.rs).
    for text in [
        "e1 o 0 poly(2) a 0 b 0 0 1 1",
        "g1 o 0 POLY(1) a 0 0 1m",
        "f1 o 0 poly(1) v1 0 2",
        "h1 o 0 poly(1) v1 0 2",
        "e1 o 0 value={v(a)*2}",
        "e1 o 0 vol='v(a)'",
        "g1 o 0 cur={v(a)}",
        "e1 o 0 table {v(a)} = (0,0) (1,1)",
        "e1 o 0 a b table=(0,0,1,1)",
        // inp_poly_2g6_compat() turns extra values into an implicit POLY(1).
        "e1 o 0 a b 1 2",
        "f1 o 0 v1 1 2",
        "g1 o 0 a b 1m 2m 3m",
    ] {
        let device = device(text);
        assert!(
            device
                .parameters
                .iter()
                .any(|p| matches!(p.name.as_str(), "poly" | "value" | "table")),
            "{text}"
        );
    }
    for text in [
        "e1 o 0 laplace {v(a)} = {1/(1+s)}",
        // Sensitivity flags and a named control= after the positional one.
        "g1 o 0 a b 1m m=1 sens_trans",
        "f1 o 0 v1 1 m=1 control=v2",
    ] {
        let error = try_parse(text).unwrap_err();
        assert!(error.is_not_yet_ported(), "{text}: {error}");
    }
}

#[test]
fn malformed_linear_cards_are_positioned_parse_errors() {
    for (text, message, column) in [
        ("e1 o 0 a b", "expected the gain", 11),
        ("g1 o 0 a", "negative controlling node", 9),
        ("f1 o 0", "controlling voltage source name", 7),
        ("h1 o", "negative output terminal", 5),
        // C would build a zero-gain source here; the port refuses.
        ("g1 o 0 a b m=2", "expected the gain as a number", 12),
        ("e1 o 0 a b gain", "expected the gain as a number", 12),
        ("e1 o 0 a b 2 m=2", "unknown parameter 'm'", 14),
        ("h1 o 0 v1 2 ic=1", "unknown parameter 'ic'", 13),
        (
            "g1 o 0 a b 1m m=2 w=1",
            "expected m=value or gain=value",
            19,
        ),
    ] {
        let error = try_parse(text).unwrap_err();
        let SpiceError::Parse {
            location,
            message: got,
        } = &error
        else {
            panic!("{text}: {error:?}");
        };
        assert!(got.contains(message), "{text}: {got}");
        assert_eq!((location.line, location.column), (2, column), "{text}");
    }
}

fn round_trip(body: &str) -> String {
    let first = try_parse(body).unwrap();
    let written = write_netlist(&first).unwrap();
    let second = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("out.cir"), &written))
        .unwrap_or_else(|error| panic!("{error}\n{written}"));
    if let Some(difference) = semantic_diff(&first, &second) {
        panic!("not equivalent: {difference}\n{written}");
    }
    assert_eq!(write_netlist(&second).unwrap(), written, "fixed point");
    written
}

#[test]
fn the_writer_round_trips_every_representable_setter_order() {
    let written = round_trip(
        "v1 in 0 dc 1\nr1 in 0 1k\nE1 o1 0 (in, 0) 2\ne2 o2 0 vcvs in 0 {2*3}\n\
         g1 0 o3 in 0 1m m=2\ng2 0 o4 in 0 gain=1m m=2\nf1 0 o5 v1 1 m=2 gain=3\n\
         h1 o6 0 (v1) gain=5\nf2 0 o7 cccs v1 4\n.subckt s a\nh1 a 0 vx 2\nvx a 0 0\n.ends s",
    );
    for line in [
        "e1 o1 0 in 0 2",
        "e2 o2 0 in 0 {2*3}",
        "g1 0 o3 in 0 1m m=2",
        "g2 0 o4 in 0 gain=1m m=2",
        "f1 0 o5 v1 1 m=2 gain=3",
        "h1 o6 0 v1 5",
        "f2 0 o7 v1 4",
        "  h1 a 0 vx 2",
    ] {
        assert!(written.lines().any(|l| l == line), "{line}\n{written}");
    }
}

#[test]
fn the_writer_refuses_unrepresentable_controlled_sources() {
    let mut netlist = try_parse("v1 a 0 1\ng1 0 o a 0 1m m=2").unwrap();
    // [gain, gain] cannot be re-parsed: C would read a POLY(1) tail.
    let gain = netlist.devices[1].parameters[1].clone();
    netlist.devices[1].parameters = vec![gain.clone(), gain.clone()];
    assert!(write_netlist(&netlist).is_err());
    // No gain at all.
    netlist.devices[1].parameters.clear();
    assert!(write_netlist(&netlist).is_err());
    // An F without its controlling source.
    let mut netlist = try_parse("v1 a 0 1\nf1 0 o v1 2").unwrap();
    netlist.devices[1].parameters.remove(0);
    let error = write_netlist(&netlist).unwrap_err();
    assert!(error.to_string().contains("controlling source"), "{error}");
    // m on an E is not a VCVS parameter.
    let mut netlist = try_parse("v1 a 0 1\ne1 o 0 a 0 2").unwrap();
    let mut m = netlist.devices[1].parameters[0].clone();
    m.name = "m".into();
    netlist.devices[1].parameters.insert(0, m);
    assert!(write_netlist(&netlist).is_err());
}
