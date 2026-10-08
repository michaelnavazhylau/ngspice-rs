//! M1a syntax regressions. Expected AST fields are checked by hand against the
//! decks and `inp2{r,c,l,v,i}.c`; this is not a simulation-parity test.

use std::path::{Path, PathBuf};

use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("test.cir"),
        &format!("Title\n{body}"),
    ))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/netlists")
        .join(format!("{name}.cir"))
}

fn parameters(netlist: &Netlist, name: &str) -> Vec<(String, String)> {
    netlist
        .device(name)
        .expect("device")
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.value.clone()))
        .collect()
}

fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn divider_fixture_builds_real_devices_and_request() {
    let path = fixture("rc_divider");
    let netlist = Parser::new().parse_file(&path).expect("parses");
    assert_eq!(netlist.path, path);
    assert_eq!(netlist.title, "RC divider, operating point");
    assert_eq!(netlist.location.line, 1);
    assert_eq!(netlist.top_level_device_count(), 3);
    assert_eq!(
        netlist
            .devices
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["v1", "r1", "r2"]
    );
    assert_eq!(netlist.device("R1").unwrap().nodes, ["in", "out"]);
    assert_eq!(parameters(&netlist, "v1"), pairs(&[("dc", "5")]));
    assert_eq!(parameters(&netlist, "r1"), pairs(&[("resistance", "1k")]));
    assert_eq!(parameters(&netlist, "r2"), pairs(&[("resistance", "1k")]));
    assert_eq!(
        netlist.analysis_kinds().collect::<Vec<_>>(),
        [AnalysisKind::OperatingPoint]
    );
    assert!(netlist.analyses[0].arguments.is_empty());
    assert_eq!(netlist.device("v1").unwrap().location.line, 3);
    assert_eq!(netlist.analyses[0].location.line, 6);
    assert!(netlist.models.is_empty());
    assert!(netlist.subcircuits.is_empty());
}

#[test]
fn ac_fixture_preserves_source_and_sweep_fields() {
    let netlist = Parser::new().parse_file(fixture("rc_lowpass_ac")).unwrap();
    assert_eq!(
        parameters(&netlist, "v1"),
        pairs(&[("dc", "0"), ("acmag", "1"), ("acphase", "0")])
    );
    assert_eq!(parameters(&netlist, "c1"), pairs(&[("capacitance", "1u")]));
    assert_eq!(netlist.analyses[0].kind, AnalysisKind::Ac);
    assert_eq!(netlist.analyses[0].arguments, ["lin", "3", "100", "1k"]);
}

#[test]
fn rlc_fixture_preserves_terminal_order_and_inductance() {
    let netlist = Parser::new().parse_file(fixture("rlc_series")).unwrap();
    assert_eq!(netlist.top_level_device_count(), 4);
    let inductor = netlist.device("l1").unwrap();
    assert_eq!(inductor.designator, 'l');
    assert_eq!(inductor.nodes, ["mid", "out"]);
    assert!(inductor.model.is_none());
    assert_eq!(parameters(&netlist, "l1"), pairs(&[("inductance", "1m")]));
    assert_eq!(parameters(&netlist, "c1"), pairs(&[("capacitance", "1u")]));
}

#[test]
fn subcircuit_fixture_parses_without_flattening() {
    let n = Parser::new().parse_file(fixture("subckt_divider")).unwrap();
    assert_eq!(n.devices.len(), 3);
    assert_eq!(n.subcircuit("DIV").unwrap().devices.len(), 1);
    assert_eq!(n.device("x1").unwrap().designator, 'x');
    assert_eq!(n.device("x1").unwrap().model.as_deref(), Some("div"));
}

#[test]
fn ground_alias_is_limited_to_node_positions() {
    let deck = parse_deck_text(
        Path::new("gnd.cir"),
        "GND stays in the title\nRGND GND Out 1K\n",
    );
    let netlist = Parser::new().parse_deck(&deck).unwrap();
    assert_eq!(netlist.title, "GND stays in the title");
    assert_eq!(netlist.device("RGND").unwrap().name, "rgnd");
    assert_eq!(netlist.device("rgnd").unwrap().nodes, ["0", "out"]);
    assert_eq!(parameters(&netlist, "rgnd"), pairs(&[("resistance", "1K")]));
    let netlist = Parser::with_auto_gnd(false).parse_deck(&deck).unwrap();
    assert_eq!(netlist.device("rgnd").unwrap().nodes, ["gnd", "out"]);
}

#[test]
fn numeric_node_names_and_value_text_survive() {
    let netlist = parse("R1 01 0 2.2MEGOhm\nC1 01 1 25pF\nL1 1 0 1e-3H\n").unwrap();
    assert_eq!(netlist.device("r1").unwrap().nodes, ["01", "0"]);
    assert_eq!(
        parameters(&netlist, "r1"),
        pairs(&[("resistance", "2.2MEGOhm")])
    );
    assert_eq!(
        parameters(&netlist, "c1"),
        pairs(&[("capacitance", "25pF")])
    );
    assert_eq!(
        parameters(&netlist, "l1"),
        pairs(&[("inductance", "1e-3H")])
    );
}

#[test]
fn scalar_assignments_and_aliases_are_canonicalized() {
    let netlist = parse("R1 a b R=1K TC1=0.01\nC1 b 0 C=1u IC 2\nL1 a 0 L=1m IC=3m\n").unwrap();
    assert_eq!(
        parameters(&netlist, "r1"),
        pairs(&[("resistance", "1K"), ("tc1", "0.01")])
    );
    assert_eq!(
        parameters(&netlist, "c1"),
        pairs(&[("capacitance", "1u"), ("ic", "2")])
    );
    assert_eq!(
        parameters(&netlist, "l1"),
        pairs(&[("inductance", "1m"), ("ic", "3m")])
    );
    // Named-assignment locations point at the parameter, not the value.
    assert_eq!(
        netlist.device("r1").unwrap().parameters[1].location.column,
        13
    );
}

#[test]
fn source_ac_defaults_and_current_sources_match_c_contract() {
    let netlist = parse("V1 a 0 AC\nI1 a 0 2m\nV2 b 0 DC=5 AC 2 90\nI2 b 0\n").unwrap();
    assert_eq!(
        parameters(&netlist, "v1"),
        pairs(&[("acmag", "1"), ("acphase", "0")])
    );
    assert_eq!(parameters(&netlist, "i1"), pairs(&[("dc", "2m")]));
    assert_eq!(
        parameters(&netlist, "v2"),
        pairs(&[("dc", "5"), ("acmag", "2"), ("acphase", "90")])
    );
    assert!(
        parameters(&netlist, "i2").is_empty(),
        "implicit DC zero stays implicit"
    );
}

#[test]
fn leading_source_value_has_c_precedence_over_explicit_dc() {
    let netlist = parse("V1 a 0 5 dc 2 ac 1\n").unwrap();
    let dc: Vec<_> = netlist
        .device("v1")
        .unwrap()
        .parameters
        .iter()
        .filter(|p| p.name == "dc")
        .map(|p| p.value.as_str())
        .collect();
    assert_eq!(dc, ["2", "5"], "INP2V applies leading DC after INPdevParse");
}

#[test]
fn analysis_cards_keep_order_and_unvalidated_arguments() {
    let netlist = parse(".op\n.dc V1 0 5 0.1\n.ac DEC 10 1 1MEG\n.tran 1n 10u uic\n").unwrap();
    assert_eq!(
        netlist.analysis_kinds().collect::<Vec<_>>(),
        [
            AnalysisKind::OperatingPoint,
            AnalysisKind::DcSweep,
            AnalysisKind::Ac,
            AnalysisKind::Transient,
        ]
    );
    assert_eq!(netlist.analyses[1].arguments, ["V1", "0", "5", "0.1"]);
    assert_eq!(netlist.analyses[2].arguments, ["DEC", "10", "1", "1MEG"]);
    assert_eq!(netlist.analyses[3].arguments, ["1n", "10u"]);
    assert!(netlist.analyses[3].uic);
}

#[test]
fn continuations_feed_the_semantic_parser() {
    let netlist = parse("V1 a 0 DC 5\n+ AC 1 90\nR1 a 0 1k\n+ tc1=0.01\n").unwrap();
    assert_eq!(
        parameters(&netlist, "v1"),
        pairs(&[("dc", "5"), ("acmag", "1"), ("acphase", "90")])
    );
    assert_eq!(netlist.device("v1").unwrap().location.line, 2);
    assert_eq!(netlist.device("r1").unwrap().parameters.len(), 2);
}

#[test]
fn end_stops_semantic_parsing_even_before_invalid_tokens() {
    let netlist = parse("R1 a 0 1k\n.END\nR2 broken {\n").unwrap();
    assert_eq!(netlist.top_level_device_count(), 1);
    assert!(parse("").unwrap().devices.is_empty());
}

#[test]
fn malformed_supported_syntax_has_source_locations() {
    for body in [
        "R1",
        "C1 a",
        "L1 a 0",
        "R1 a 0 r=",
        "V1 a 0 dc",
        "R1 a 0 1k tc1=",
        "I1 ( 0 1",
        "R1 a 0 1e999",
        "?invalid",
    ] {
        let error = parse(body).expect_err("bad syntax");
        match error {
            SpiceError::Parse { location, .. } => {
                assert_eq!(location.line, 2, "{body}");
                assert!(location.column >= 1, "{body}");
                assert_eq!(location.path(), Path::new("test.cir"));
            }
            other => panic!("{body}: expected Parse, got {other}"),
        }
    }
}

#[test]
fn unsupported_semantics_never_get_silently_dropped() {
    for (body, reference) in [
        ("R1 a 0 'rval'", "inp2r.c"),
        ("R1 a 0 modelname", "inp2r.c"),
        ("R1 a 0 4k7", "inp2r.c"),
        ("R1 a 0 1k sens_resist", "inp2r.c"),
        ("C1 a 0 1u bad=2", "inp2c.c"),
        ("V1 a 0 trnoise(0 1n)", "inp2v.c"),
        ("I1 a 0 dc 'ival'", "inp2i.c"),
        ("V1 a 0 ac 'gain'", "inp2v.c"),
        (".control\nquit\n.endc", "frontend/inp.c"),
        // `.save`/`.print` now parse into output requests; `.plot` (the ASCII
        // plotting card) is still outside the port's subset.
        (".plot dc v(a)", "inp2dot.c"),
        (".model rm r(rsh='sheet')", "inpdomod.c"),
    ] {
        let error = parse(body).expect_err("not ported");
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains(reference), "{body}: {error}");
    }
}
