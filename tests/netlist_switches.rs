//! S/W switch grammar (`inp2s.c`, `inp2w.c`), `sw`/`csw` model cards and the
//! writer round trip (GitHub #81).
use std::path::Path;

use ngspice_rs::netlist::ast::{DeviceInstance, Netlist, ParameterKind};
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::netlist::{Parser, semantic_diff, write_netlist};
use ngspice_rs::primitives::SpiceError;

fn try_parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("in.cir"),
        &format!("switches\n{body}\n.end\n"),
    ))
}

fn device(body: &str) -> DeviceInstance {
    let netlist = try_parse(body).unwrap_or_else(|error| panic!("{error}\n{body}"));
    netlist.devices.last().unwrap().clone()
}

fn setters(device: &DeviceInstance) -> Vec<(String, String, &'static str)> {
    device
        .parameters
        .iter()
        .map(|p| {
            let kind = match p.kind {
                ParameterKind::Flag => "flag",
                ParameterKind::Instance => "instance",
                ParameterKind::Scalar => "scalar",
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
fn s_and_w_cards_keep_terminals_model_and_flag_order() {
    let sw = device("S1 Out GND Ctl 0 SMod OFF on");
    assert_eq!((sw.name.as_str(), sw.designator), ("s1", 's'));
    assert_eq!(sw.nodes, ["out", "0", "ctl", "0"]);
    assert_eq!(sw.model.as_deref(), Some("smod"));
    // SWparam applies the flags in order: the last one wins.
    assert_eq!(setters(&sw), [s("off", "", "flag"), s("on", "", "flag")]);
    assert_eq!(sw.parameters[1].location.column, 27);

    let w = device("W1 a b VSense WMod ON");
    assert_eq!((w.name.as_str(), w.designator), ("w1", 'w'));
    assert_eq!(w.nodes, ["a", "b"]);
    assert_eq!(w.model.as_deref(), Some("wmod"));
    // INP2W sets the controlling source before INPdevParse.
    assert_eq!(
        setters(&w),
        [s("control", "vsense", "instance"), s("on", "", "flag")]
    );
    assert_eq!(w.parameters[0].location.column, 8);
    assert!(device("w2 a 0 v1 m").parameters.len() == 1);
}

#[test]
fn switch_models_accept_their_type_flag_and_scalars() {
    let netlist =
        try_parse(".model sm sw(vt=1 vh=0.5 ron=1 roff=1meg sw)\n.model wm csw it=1m ih=-2m")
            .unwrap();
    assert_eq!(netlist.models[0].base, "sw");
    let names: Vec<_> = netlist.models[0]
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.kind == ParameterKind::Flag))
        .collect();
    assert_eq!(
        names,
        [
            ("vt", false),
            ("vh", false),
            ("ron", false),
            ("roff", false),
            ("sw", true)
        ]
    );
    assert_eq!(netlist.models[1].base, "csw");
    assert_eq!(netlist.models[1].parameters[1].value, "-2m");
}

#[test]
fn malformed_and_unsupported_switch_cards_are_positioned_errors() {
    for (text, column, message, not_yet) in [
        ("s1 a 0 c 0", 11, "switch model name", false),
        ("w1 a 0 v1", 10, "switch model name", false),
        ("s1 a 0 c", 9, "negative controlling node", false),
        ("s1 a 0 c 0 sm 1", 15, "C silently ignores it", false),
        ("w1 a 0 v1 wm off=1", 17, "bare flag", false),
        ("s1 a 0 c 0 sm ic=1", 15, "only bare on/off", false),
    ] {
        let error = try_parse(text).unwrap_err();
        let got = error.to_string();
        assert!(got.contains(message), "{text}: {got}");
        assert_eq!(error.is_not_yet_ported(), not_yet, "{text}: {got}");
        let location = match &error {
            SpiceError::Parse { location, .. } => location.clone(),
            SpiceError::Unsupported {
                location: Some(location),
                ..
            } => location.clone(),
            other => panic!("{text}: {other:?}"),
        };
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
fn the_writer_round_trips_switches_and_their_models() {
    let written = round_trip(
        "v1 c 0 1\nS1 a 0 c 0 sm on off\nw1 b 0 v1 wm ON\n.model sm sw(vt=1 vh=0.5 sw)\n\
         .model wm csw(it=1m ron=2)\n.subckt x p\nw1 p 0 vx wm\nvx p 0 0\n.ends x",
    );
    for line in [
        "s1 a 0 c 0 sm on off",
        "w1 b 0 v1 wm on",
        ".model sm sw(vt=1 vh=0.5 sw)",
        ".model wm csw(it=1m ron=2)",
        "  w1 p 0 vx wm",
    ] {
        assert!(written.lines().any(|l| l == line), "{line}\n{written}");
    }
}

#[test]
fn the_writer_refuses_unrepresentable_switches() {
    let mut netlist = try_parse("v1 c 0 1\nw1 a 0 v1 wm").unwrap();
    netlist.devices[1].parameters.clear();
    let error = write_netlist(&netlist).unwrap_err();
    assert!(error.to_string().contains("controlling source"), "{error}");
    let mut netlist = try_parse("s1 a 0 c 0 sm on").unwrap();
    netlist.devices[0].parameters[0].kind = ParameterKind::Scalar;
    assert!(write_netlist(&netlist).is_err());
    let mut netlist = try_parse("s1 a 0 c 0 sm").unwrap();
    netlist.devices[0].model = None;
    assert!(write_netlist(&netlist).is_err());
}
