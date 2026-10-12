//! M1b's bounded model/diode syntax slice; no Rust device simulation is implied.

use std::path::Path;

use ngspice_rs::netlist::{
    Parser,
    ast::{Netlist, ParameterAssignment},
    source::parse_deck_text,
};
use ngspice_rs::primitives::{AnalysisKind, SpiceError};

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("model.cir"),
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
fn diode_dc_fixture_builds_devices_model_and_sweep() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/netlists/diode_dc.cir");
    let netlist = Parser::new().parse_file(&path).unwrap();
    assert_eq!(netlist.path, path);
    assert_eq!(netlist.title, "Diode DC sweep");
    assert_eq!(netlist.top_level_device_count(), 3);
    let diode = netlist.device("D1").unwrap();
    assert_eq!(diode.designator, 'd');
    assert_eq!(diode.nodes, ["out", "0"]);
    assert_eq!(diode.model.as_deref(), Some("dmod"));
    assert!(diode.parameters.is_empty());
    assert_eq!(diode.location.line, 4);
    let model = netlist.model("DMOD").unwrap();
    assert_eq!(model.base, "d");
    assert_eq!(model.level, None);
    assert_eq!(pairs(&model.parameters), [("is", "1e-14"), ("n", "1.0")]);
    assert_eq!(model.location.line, 5);
    assert_eq!(netlist.analyses[0].kind, AnalysisKind::DcSweep);
    assert_eq!(netlist.analyses[0].arguments, ["v1", "0", "1", "0.25"]);
}

#[test]
fn model_forms_accept_scalar_assignments_and_c_delimiters() {
    let netlist = parse(".MODEL DM D(,IS=2e-14, N 1.2, RS=2K,)\n.model plain d is 1e-14 n=1\n.model empty d()\n.model bare d\n").unwrap();
    assert_eq!(netlist.models.len(), 4);
    assert_eq!(
        pairs(&netlist.model("dm").unwrap().parameters),
        [("is", "2e-14"), ("n", "1.2"), ("rs", "2K")]
    );
    assert_eq!(
        pairs(&netlist.model("plain").unwrap().parameters),
        [("is", "1e-14"), ("n", "1")]
    );
    assert!(netlist.model("empty").unwrap().parameters.is_empty());
    assert!(netlist.model("bare").unwrap().parameters.is_empty());
}

#[test]
fn ordinary_model_families_are_retained_without_backend_availability_claims() {
    for base in ["D", "NPN", "PNP", "NMOS", "PMOS", "R", "RES", "C", "L"] {
        let netlist = parse(&format!(".model GND {base} arbitrary_scalar=2\n")).unwrap();
        let model = netlist.model("gnd").unwrap();
        assert_eq!(model.base, base.to_ascii_lowercase());
        // Model keyword validity and instance defaults belong to elaboration.
        assert_eq!(pairs(&model.parameters), [("arbitrary_scalar", "2")]);
    }
}

#[test]
fn level_keeps_first_raw_value_without_rounding_or_defaults() {
    let netlist =
        parse(".model NM NMOS (LEVEL=4.900e1 VTO=1 level=54)\n.model raw pnp level=2.5\n").unwrap();
    let model = netlist.model("NM").unwrap();
    assert_eq!(model.level, Some(49.0));
    assert_eq!(
        pairs(&model.parameters),
        [("level", "4.900e1"), ("vto", "1"), ("level", "54")]
    );
    assert_eq!(netlist.model("raw").unwrap().level, Some(2.5));
}

#[test]
fn duplicates_and_model_order_remain_visible() {
    let netlist = parse(".model dm d(is=1e-14 is=2e-14)\n.model second d\n").unwrap();
    assert_eq!(
        netlist
            .models
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["dm", "second"]
    );
    assert_eq!(
        pairs(&netlist.models[0].parameters),
        [("is", "1e-14"), ("is", "2e-14")]
    );
}

#[test]
fn forward_and_unresolved_model_references_are_syntax_not_elaboration() {
    let netlist = parse("D1 A 0 DM\nD2 B 0 missing\n.model DM d area=4\n").unwrap();
    assert_eq!(netlist.device("d1").unwrap().model.as_deref(), Some("dm"));
    assert_eq!(
        netlist.device("d2").unwrap().model.as_deref(),
        Some("missing")
    );
    assert!(
        netlist.device("d1").unwrap().parameters.is_empty(),
        "do not apply model defaults during parsing"
    );
}

#[test]
fn diode_scalar_parameters_and_perimeter_alias_are_canonicalized() {
    let netlist = parse(
        "D1 A B DM AREA=2 PERIM 3 M=4 IC=.1 TEMP=30 DTEMP=2 W=10u L=20u LM=1u LP=2u WM=3u WP=4u\n",
    )
    .unwrap();
    let diode = netlist.device("d1").unwrap();
    assert_eq!(diode.nodes, ["a", "b"]);
    assert_eq!(
        pairs(&diode.parameters),
        [
            ("area", "2"),
            ("pj", "3"),
            ("m", "4"),
            ("ic", ".1"),
            ("temp", "30"),
            ("dtemp", "2"),
            ("w", "10u"),
            ("l", "20u"),
            ("lm", "1u"),
            ("lp", "2u"),
            ("wm", "3u"),
            ("wp", "4u"),
        ]
    );
    assert_eq!(diode.parameters[0].location.column, 11);
    assert_eq!(diode.parameters[1].location.column, 18);
}

#[test]
fn positional_area_is_applied_after_named_parameters() {
    let netlist = parse("D1 A 0 DM 2 AREA=7 area=3\nD2 A 0 DM 5\n").unwrap();
    assert_eq!(
        pairs(&netlist.device("d1").unwrap().parameters),
        [("area", "7"), ("area", "3"), ("area", "2")]
    );
    assert_eq!(
        pairs(&netlist.device("d2").unwrap().parameters),
        [("area", "5")]
    );
    assert_eq!(
        netlist.device("d1").unwrap().parameters[2].location.column,
        11
    );
}

#[test]
fn ground_aliasing_never_changes_model_names() {
    let text = "Title\nDGND ANODE GND GND\n.model GND D\n";
    let deck = parse_deck_text(Path::new("alias.cir"), text);
    for (auto_gnd, cathode) in [(true, "0"), (false, "gnd")] {
        let netlist = Parser::with_auto_gnd(auto_gnd).parse_deck(&deck).unwrap();
        let diode = netlist.device("dgnd").unwrap();
        assert_eq!(diode.nodes, ["anode", cathode]);
        assert_eq!(diode.model.as_deref(), Some("gnd"));
        assert_eq!(netlist.models[0].name, "gnd");
    }
}

#[test]
fn numeric_models_are_names_not_positional_areas() {
    let netlist = parse("D1 01 0 123 2\n.model 123 d\n").unwrap();
    let diode = netlist.device("d1").unwrap();
    assert_eq!(diode.nodes, ["01", "0"]);
    assert_eq!(diode.model.as_deref(), Some("123"));
    assert_eq!(pairs(&diode.parameters), [("area", "2")]);
    assert_eq!(netlist.models[0].name, "123");
}

#[test]
fn continuations_keep_model_and_assignment_provenance() {
    let netlist = parse(".model DM D(IS=1e-14\n+ N=1)\nD1 A 0 DM\n+ AREA=2\n").unwrap();
    let model = &netlist.models[0];
    assert_eq!(model.location.line, 2);
    assert_eq!(model.parameters[1].location.line, 2);
    assert_eq!(model.parameters[1].location.column, 22);
    assert_eq!(netlist.devices[0].location.line, 4);
    assert_eq!(netlist.devices[0].parameters[0].location.line, 4);
    assert_eq!(netlist.devices[0].parameters[0].location.column, 11);
}

#[test]
fn malformed_models_and_diodes_have_location_bearing_parse_errors() {
    for body in [
        ".model",
        ".model dm",
        ".model dm d(is=)",
        ".model dm d(is=1e-14",
        ".model dm d)",
        ".model dm d((is=1e-14))",
        ".model dm d is==1",
        ".model dm d 2",
        ".model dm d() trailing=2",
        "D1",
        "D1 a",
        "D1 a 0",
        "D1 ( 0 dm",
        "D1 a 0 dm area=",
        "D1 a 0 dm area==2",
        "D1 a 0 dm,",
    ] {
        match parse(body).expect_err("malformed") {
            SpiceError::Parse { location, .. } => {
                assert_eq!(location.line, 2, "{body}");
                assert_eq!(location.path(), Path::new("model.cir"));
                assert!(location.column >= 1);
            }
            other => panic!("{body}: expected Parse, got {other}"),
        }
    }
}

#[test]
fn numeric_overflow_cannot_escape_optional_or_repeated_parsers() {
    for body in [
        "D1 a 0 dm 1e999",
        "D1 a 0 dm area=1e999",
        ".model dm d(is=1e999)",
        ".model nm nmos level=1e999",
    ] {
        assert!(
            matches!(parse(body), Err(SpiceError::Parse { .. })),
            "{body}"
        );
    }
}

#[test]
fn unsupported_model_shapes_preserve_specific_gaps() {
    for body in [
        ".model dm d(is=\"saturation\")",
        ".model dm d(is=\"1e-14\")",
        ".model dm d is=parameter",
        ".model nm nmos version=3.3.0",
        ".model dm d(nchan)",
        ".model xm unknown foo=2",
        ".model mf nmf beta=1m",
        ".model vd vdmos nchan",
        ".model cd numd",
    ] {
        match parse(body).expect_err("gap") {
            SpiceError::NotYetPorted { what, c_reference } => {
                assert!(what.contains("model.cir:2:"), "{body}: {what}");
                assert_eq!(c_reference, "src/spicelib/parser/inpdomod.c", "{body}");
            }
            other => panic!("{body}: expected gap, got {other}"),
        }
    }
}

#[test]
fn unsupported_diode_shapes_preserve_specific_gaps() {
    for body in [
        "D1 a 0 dm sens_area",
        "D1 a 0 dm thermal",
        "D1 a 0 dm area=\"size\"",
        "D1 a 0 dm area=\"2\"",
        "D1 a 0 heat dm",
        "D1 a 0 dm unknown=2",
        "D1 a 0 dm 2 3",
        "D1 a 0 dm area=4k7",
    ] {
        match parse(body).expect_err("gap") {
            SpiceError::NotYetPorted { what, c_reference } => {
                assert!(what.contains("model.cir:2:"), "{body}: {what}");
                assert_eq!(c_reference, "src/spicelib/parser/inp2d.c", "{body}");
            }
            other => panic!("{body}: expected gap, got {other}"),
        }
    }
}

#[test]
fn end_ignores_subsequent_invalid_model_and_diode_cards() {
    let netlist = parse("D1 a 0 dm\n.model dm d\n.END\n.model broken {\nD2\n").unwrap();
    assert_eq!(netlist.models.len(), 1);
    assert_eq!(netlist.devices.len(), 1);
}
