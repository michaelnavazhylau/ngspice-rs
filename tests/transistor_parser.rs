//! Bounded Q/M syntax and declared-model terminal disambiguation, not simulation.

use std::path::Path;

use ngspice_rs::netlist::{
    Parser,
    ast::{Netlist, ParameterAssignment},
    source::parse_deck_text,
};
use ngspice_rs::primitives::{AnalysisKind, SpiceError};

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("transistor.cir"),
        &format!("Title\n{body}"),
    ))
}

fn pairs(parameters: &[ParameterAssignment]) -> Vec<(&str, &str)> {
    parameters
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect()
}

#[test]
fn bjt_fixture_retains_three_terminals_model_and_operating_point() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/netlists/bjt_ce.cir");
    let netlist = Parser::new().parse_file(&path).unwrap();
    assert_eq!(netlist.path, path);
    assert_eq!(netlist.top_level_device_count(), 4);
    let device = netlist.device("Q1").unwrap();
    assert_eq!(device.designator, 'q');
    assert_eq!(device.nodes, ["coll", "base", "0"]);
    assert_eq!(device.model.as_deref(), Some("qmod"));
    assert!(device.parameters.is_empty());
    assert_eq!(device.location.line, 5);
    assert_eq!(netlist.model("qmod").unwrap().base, "npn");
    assert_eq!(netlist.analyses[0].kind, AnalysisKind::OperatingPoint);
}

#[test]
fn mos_fixture_retains_four_terminals_model_and_geometry() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/netlists/mos_inverter.cir");
    let netlist = Parser::new().parse_file(&path).unwrap();
    assert_eq!(netlist.path, path);
    assert_eq!(netlist.top_level_device_count(), 4);
    let device = netlist.device("M1").unwrap();
    assert_eq!(device.designator, 'm');
    assert_eq!(device.nodes, ["drain", "gate", "0", "0"]);
    assert_eq!(device.model.as_deref(), Some("nmos"));
    assert_eq!(pairs(&device.parameters), [("w", "10u"), ("l", "1u")]);
    assert_eq!(device.location.line, 5);
    assert_eq!(netlist.model("nmos").unwrap().level, Some(1.0));
    assert_eq!(netlist.analyses[0].kind, AnalysisKind::OperatingPoint);
}

#[test]
fn bjt_substrate_disambiguation_uses_forward_and_backward_declarations() {
    let netlist =
        parse("Q3 C B E QM\nQ4 C B E SUB QM\n.model QM NPN\nQlater C B E 0 QM\n").unwrap();
    assert_eq!(netlist.device("q3").unwrap().nodes, ["c", "b", "e"]);
    assert_eq!(netlist.device("q4").unwrap().nodes, ["c", "b", "e", "sub"]);
    assert_eq!(
        netlist.device("qlater").unwrap().nodes,
        ["c", "b", "e", "0"]
    );
    for device in &netlist.devices {
        assert_eq!(device.model.as_deref(), Some("qm"));
    }
}

#[test]
fn earliest_declared_model_wins_over_optional_substrate_interpretation() {
    let netlist = parse("Q1 C B E AREA 2\n.model AREA NPN\n.model 2 PNP\n").unwrap();
    let device = netlist.device("q1").unwrap();
    assert_eq!(device.nodes, ["c", "b", "e"]);
    assert_eq!(device.model.as_deref(), Some("area"));
    assert_eq!(pairs(&device.parameters), [("area", "2")]);
}

#[test]
fn numeric_nodes_and_model_names_with_digits_keep_terminal_roles() {
    let netlist = parse(
        "Q1 123 B E QM123 2\nQ2 C B E 01 QM123\nM1 123 G S 01 NM456\n.model QM123 NPN\n.model NM456 PMOS\n",
    )
    .unwrap();
    let q1 = netlist.device("q1").unwrap();
    assert_eq!(q1.nodes, ["123", "b", "e"]);
    assert_eq!(q1.model.as_deref(), Some("qm123"));
    assert_eq!(pairs(&q1.parameters), [("area", "2")]);
    assert_eq!(netlist.device("q2").unwrap().nodes, ["c", "b", "e", "01"]);
    assert_eq!(netlist.device("m1").unwrap().nodes, ["123", "g", "s", "01"]);
    assert_eq!(
        netlist.device("m1").unwrap().model.as_deref(),
        Some("nm456")
    );
}

#[test]
fn leading_bjt_area_is_applied_last() {
    let netlist = parse("Q1 C B E QM 2 AREA=7 area=3\n.model QM NPN\n").unwrap();
    let parameters = &netlist.device("q1").unwrap().parameters;
    assert_eq!(
        pairs(parameters),
        [("area", "7"), ("area", "3"), ("area", "2")]
    );
    assert_eq!(parameters[2].location.column, 13);
}

#[test]
fn bjt_scalar_parameters_are_ordered_and_textual() {
    let netlist = parse("Q1 C B E 0 QM AREA=2 AREAB 3 AREAC=4 M=5 ICVBE=.6 ICVCE=2 TEMP=30 DTEMP=2\n.model QM PNP\n").unwrap();
    assert_eq!(
        pairs(&netlist.device("q1").unwrap().parameters),
        [
            ("area", "2"),
            ("areab", "3"),
            ("areac", "4"),
            ("m", "5"),
            ("icvbe", ".6"),
            ("icvce", "2"),
            ("temp", "30"),
            ("dtemp", "2"),
        ]
    );
}

#[test]
fn mos_scalar_geometry_and_ic_components_preserve_application_order() {
    let netlist = parse("M1 D G S B NM L=1U W 10U W=20U AD=2p AS=3p PD=4u PS=5u NRD=2 NRS=3 M=4 ICVDS=.1 ICVGS=2 ICVBS=-.1 TEMP=30 DTEMP=2\n.model NM NMOS\n").unwrap();
    assert_eq!(
        pairs(&netlist.device("m1").unwrap().parameters),
        [
            ("l", "1U"),
            ("w", "10U"),
            ("w", "20U"),
            ("ad", "2p"),
            ("as", "3p"),
            ("pd", "4u"),
            ("ps", "5u"),
            ("nrd", "2"),
            ("nrs", "3"),
            ("m", "4"),
            ("icvds", ".1"),
            ("icvgs", "2"),
            ("icvbs", "-.1"),
            ("temp", "30"),
            ("dtemp", "2"),
        ]
    );
    assert_eq!(
        netlist.device("m1").unwrap().parameters[0].location.column,
        15
    );
}

#[test]
fn ground_aliasing_is_after_model_disambiguation_and_only_for_ports() {
    let text = "Title\nQ3 C B GND GND\nQ4 C B GND GND QM\nM1 D G GND GND NM\n.model GND NPN\n.model QM NPN\n.model NM NMOS\n";
    // Q4's substrate token GND is a declared model, so C chooses it as Q4's
    // model and the following QM is an unsupported parameter, not a port.
    let error = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("alias.cir"), text))
        .unwrap_err();
    assert!(error.is_not_yet_ported());
    for (auto_gnd, ground) in [(true, "0"), (false, "gnd")] {
        let text = "Title\nQ3 C B GND GND\n.model GND NPN\n";
        let netlist = Parser::with_auto_gnd(auto_gnd)
            .parse_deck(&parse_deck_text(Path::new("alias.cir"), text))
            .unwrap();
        assert_eq!(netlist.device("q3").unwrap().nodes, ["c", "b", ground]);
        assert_eq!(netlist.device("q3").unwrap().model.as_deref(), Some("gnd"));
        let text = "Title\nQ4 C B GND GND QM\nM1 D G GND GND NM\n.model QM NPN\n.model NM NMOS\n";
        let netlist = Parser::with_auto_gnd(auto_gnd)
            .parse_deck(&parse_deck_text(Path::new("alias.cir"), text))
            .unwrap();
        assert_eq!(
            netlist.device("q4").unwrap().nodes,
            ["c", "b", ground, ground]
        );
        assert_eq!(
            netlist.device("m1").unwrap().nodes,
            ["d", "g", ground, ground]
        );
    }
}

#[test]
fn mos_bulk_model_collision_is_not_silently_reinterpreted() {
    let error = parse("M1 D G S NM OTHER\n.model NM NMOS\n.model OTHER PMOS\n").unwrap_err();
    match error {
        SpiceError::Parse { location, message } => {
            assert_eq!(location.column, 10);
            assert!(message.contains("bulk terminal"), "{message}");
        }
        other => panic!("expected missing bulk, got {other}"),
    }
}

#[test]
fn declaration_presence_does_not_claim_valid_model_type_or_backend() {
    let netlist =
        parse("Q1 C B E WRONG\nM1 D G S B ADV\n.model WRONG D\n.model ADV NMOS LEVEL=54\n")
            .unwrap();
    assert_eq!(
        netlist.device("q1").unwrap().model.as_deref(),
        Some("wrong")
    );
    assert_eq!(netlist.device("m1").unwrap().model.as_deref(), Some("adv"));
    assert_eq!(netlist.model("adv").unwrap().level, Some(54.0));
}

#[test]
fn missing_declarations_and_malformed_supported_syntax_have_parse_errors() {
    for body in [
        "Q1",
        "Q1 C",
        "Q1 C B",
        "Q1 C B E",
        "Q1 C B E UNKNOWN",
        "Q1 C B E 123\n.model 123 NPN",
        "Q1 C B E QM AREA=\n.model QM NPN",
        "Q1 C B E QM AREA==2\n.model QM NPN",
        "M1",
        "M1 D G",
        "M1 D G S",
        "M1 D G S B",
        "M1 D G S B UNKNOWN",
        "M1 D G S B NM L=\n.model NM NMOS",
        "M1 D G S B NM 2\n.model NM NMOS",
        "M1 D G S B NM W=1u 2\n.model NM NMOS",
        "M1 D G S B NM,\n.model NM NMOS",
    ] {
        match parse(body).expect_err("malformed") {
            SpiceError::Parse { location, .. } => {
                assert_eq!(location.line, 2, "{body}");
                assert_eq!(location.path(), Path::new("transistor.cir"));
            }
            other => panic!("{body}: expected Parse, got {other}"),
        }
    }
}

#[test]
fn overflow_cannot_be_swallowed_by_optional_or_repeated_parameters() {
    for body in [
        "Q1 C B E QM 1e999\n.model QM NPN",
        "Q1 C B E QM AREA=1e999\n.model QM NPN",
        "M1 D G S B NM W=1e999\n.model NM NMOS",
        "M1 D G S B NM 1e999\n.model NM NMOS",
    ] {
        assert!(
            matches!(parse(body), Err(SpiceError::Parse { .. })),
            "{body}"
        );
    }
}

#[test]
fn unsupported_variants_keep_device_specific_c_references() {
    for (body, reference) in [
        ("Q1 C B E QM SENS_AREA\n.model QM NPN", "inp2q.c"),
        ("Q1 C B E 123n 3\n.model 123n NPN", "inp2q.c"),
        ("M1 D G S B 456\n.model 456 NMOS", "inp2m.c"),
        ("Q1 C B E QM IC={vbe},2\n.model QM NPN", "inp2q.c"),
        ("Q1 C B E SUB HEAT QM\n.model QM NPN LEVEL=2", "inp2q.c"),
        ("Q1 C B E QM AREA=\"size\"\n.model QM NPN", "inp2q.c"),
        ("Q1 C B E QM UNKNOWN=2\n.model QM NPN", "inp2q.c"),
        ("M1 D G S B NM SENS_L\n.model NM NMOS", "inp2m.c"),
        ("M1 D G S B NM IC=1,{vgs},3\n.model NM NMOS", "inp2m.c"),
        ("M1 D G S B HEAT NM\n.model NM NMOS", "inp2m.c"),
        ("M1 D G S B NM W=\"width\"\n.model NM NMOS", "inp2m.c"),
        ("M1 D G S B NM NF=2\n.model NM NMOS", "inp2m.c"),
        ("M1 D G S B NM W=4k7\n.model NM NMOS", "inp2m.c"),
        (
            "M1 D G S B NM W=1u L=1u\n.model NM.1 NMOS LMIN=0 LMAX=2u",
            "inp2m.c",
        ),
    ] {
        match parse(body).expect_err("gap") {
            SpiceError::NotYetPorted { what, c_reference } => {
                assert!(what.contains("transistor.cir:2:"), "{body}: {what}");
                assert!(c_reference.ends_with(reference), "{body}: {c_reference}");
            }
            other => panic!("{body}: expected gap, got {other}"),
        }
    }
}

#[test]
fn model_index_does_not_look_past_end_or_into_scoped_bodies() {
    for body in [
        "Q1 C B E QM\n.end\n.model QM NPN",
        "M1 D G S B NM\n.end\n.model NM NMOS",
        "Q1 C B E INNER\n.subckt pair a b\n.model INNER NPN\n.ends pair",
        "Q1 C B E INNER\n.control\n.model INNER NPN\n.endc",
    ] {
        assert!(
            matches!(parse(body), Err(SpiceError::Parse { location, .. }) if location.line == 2),
            "{body}"
        );
    }
}

#[test]
fn declaration_prepass_preserves_semantic_and_lexical_error_order() {
    // An unported `.` card on line 2 still wins over the malformed `{` on line
    // 3; `.width` stands in for a card outside the port's subset (`.save` and
    // `.print` are parsed now).
    let error = parse(".width 80\n.model later d(is={\n").unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("transistor.cir:2:1"));
    let error = parse("R1\n.model later d(is={\n").unwrap_err();
    assert!(matches!(error, SpiceError::Parse { location, .. } if location.line == 2));
    let error = parse("R1 a 0 {\n.model later d\n").unwrap_err();
    assert!(matches!(error, SpiceError::Parse { location, .. } if location.line == 2));
    let netlist = parse("Q1 C B E QM\n.model QM NPN\n.end\n.model ignored {\n").unwrap();
    assert_eq!(netlist.devices.len(), 1);
    assert_eq!(netlist.models.len(), 1);
}

#[test]
fn continuations_and_unicode_nodes_preserve_byte_locations() {
    let netlist = parse(
        "Q1 α B E QM\n+ AREA=2\nM1 D G S B NM\n+ W=10u L=1u\n.model QM NPN\n.model NM NMOS\n",
    )
    .unwrap();
    let q = netlist.device("q1").unwrap();
    assert_eq!(q.nodes, ["α", "b", "e"]);
    assert_eq!(q.location.line, 2);
    assert_eq!(q.parameters[0].location.line, 2);
    assert_eq!(q.parameters[0].location.column, 14);
    let m = netlist.device("m1").unwrap();
    assert_eq!(m.location.line, 4);
    assert_eq!(m.parameters[0].location.line, 4);
    assert_eq!(m.parameters[0].location.column, 15);
}

#[test]
fn declarations_do_not_leak_between_parser_calls() {
    let parser = Parser::new();
    let first = parse_deck_text(
        Path::new("first.cir"),
        "Title\nQ1 C B E QM\n.model QM NPN\n",
    );
    parser.parse_deck(&first).unwrap();
    let second = parse_deck_text(Path::new("second.cir"), "Title\nQ1 C B E QM\n");
    assert!(matches!(
        parser.parse_deck(&second),
        Err(SpiceError::Parse { .. })
    ));
}
