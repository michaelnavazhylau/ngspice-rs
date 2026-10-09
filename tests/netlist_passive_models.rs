//! Model-aware syntax only; neither schema/default validation nor simulation.
use ngspice_rs::netlist::{Parser, ast::Netlist, source::parse_deck_text};
use ngspice_rs::primitives::SpiceError;
use std::path::Path;

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("passives.cir"),
        &format!("passives\n{body}\n.end\n"),
    ))
}
fn parameters(netlist: &Netlist, name: &str) -> Vec<(String, String)> {
    netlist
        .device(name)
        .unwrap()
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.value.clone()))
        .collect()
}

#[test]
fn scalar_model_geometry_and_omitted_values_are_retained() {
    for (designator, primary) in [
        ('r', "resistance"),
        ('c', "capacitance"),
        ('l', "inductance"),
    ] {
        let geometry = if designator == 'l' {
            "nt=2"
        } else {
            "l=4u w=2u"
        };
        let netlist = parse(&format!("{designator}1 A gnd 2m Model {geometry}\n{designator}2 A 0 Model\n{designator}3 A 0 Model {geometry}\n.model Model {designator}")).unwrap();
        assert_eq!(netlist.devices.len(), 3);
        for device in &netlist.devices {
            assert_eq!(device.model.as_deref(), Some("model"));
            assert_eq!(device.nodes, ["a", "0"]);
        }
        assert_eq!(
            parameters(&netlist, &format!("{designator}1"))[0],
            (primary.into(), "2m".into())
        );
        assert!(parameters(&netlist, &format!("{designator}2")).is_empty());
        assert!(
            !parameters(&netlist, &format!("{designator}3"))
                .iter()
                .any(|(name, _)| name == primary)
        );
    }
}

#[test]
fn setters_keep_before_model_and_after_model_precedence() {
    for (d, p) in [
        ('r', "resistance"),
        ('c', "capacitance"),
        ('l', "inductance"),
    ] {
        let n = parse(&format!("{d}1 a 0 1m mdl {p}=2m {p}=3m\n{d}2 a 0 mdl 4m {p}=5m {p}=6m\n{d}3 a 0 7m mdl 8m {p}=9m\n.model mdl {d}")).unwrap();
        assert_eq!(
            parameters(&n, &format!("{d}1")),
            vec![
                (p.into(), "1m".into()),
                (p.into(), "2m".into()),
                (p.into(), "3m".into())
            ]
        );
        assert_eq!(
            parameters(&n, &format!("{d}2")),
            vec![
                (p.into(), "5m".into()),
                (p.into(), "6m".into()),
                (p.into(), "4m".into())
            ]
        );
        assert_eq!(
            parameters(&n, &format!("{d}3")),
            vec![
                (p.into(), "7m".into()),
                (p.into(), "9m".into()),
                (p.into(), "8m".into())
            ]
        );
    }
}

#[test]
fn declarations_do_not_change_literal_or_named_only_passives() {
    let n = parse("r1 a 0 123\nr2 a 0 r=2k r=3k\nc1 a 0 c=2u\nl1 a 0 inductance=2m\n.model 123 r\n.model r r\n.model c c\n.model inductance l").unwrap();
    assert!(n.devices.iter().all(|device| device.model.is_none()));
    assert_eq!(parameters(&n, "r1"), [("resistance".into(), "123".into())]);
    assert_eq!(parameters(&n, "r2").last().unwrap().1, "3k");
}

#[test]
fn bare_keyword_model_and_equals_assignment_are_distinct() {
    let n = parse("r1 a 0 tc1 2k\nr2 a 0 r=1k tc1=2\n.model tc1 r").unwrap();
    assert_eq!(n.device("r1").unwrap().model.as_deref(), Some("tc1"));
    assert_eq!(parameters(&n, "r1"), [("resistance".into(), "2k".into())]);
    assert!(n.device("r2").unwrap().model.is_none());
    assert_eq!(parameters(&n, "r2").last().unwrap().0, "tc1");
}

#[test]
fn numeric_names_remain_scalars_and_ambiguous_model_references_fail() {
    let n = parse("r1 a 0 123\n.model 123 r").unwrap();
    assert!(n.device("r1").unwrap().model.is_none());
    let error = parse("r2 a 0 1k 123\n.model 123 r").unwrap_err();
    assert!(error.is_not_yet_ported());
    assert!(
        error
            .to_string()
            .contains("numeric-looking passive model references")
    );
}

#[test]
fn lookup_is_deck_local_stops_at_end_and_excludes_scopes() {
    for body in [
        "r1 a 0 missing",
        "r1 a 0 mdl\n.end\n.model mdl r",
        "r1 a 0 mdl\n.subckt sub a b\n.model mdl r\n.ends sub",
        "r1 a 0 mdl\n.control\n.model mdl r\n.endc",
    ] {
        let error = parse(body).unwrap_err();
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains("passives.cir:2:"), "{error}");
    }
    parse("r1 a 0 mdl\n.model mdl r").unwrap();
    assert!(parse("r1 a 0 mdl").is_err());
}

#[test]
fn forward_lookup_does_not_validate_family_or_apply_defaults() {
    let n = parse("r1 a 0 mdl\n.model mdl d(is=1e-14)\n.model mdl r(rsh=100)").unwrap();
    assert_eq!(n.models.len(), 2);
    assert_eq!(n.device("r1").unwrap().model.as_deref(), Some("mdl"));
    assert!(n.device("r1").unwrap().parameters.is_empty());
}

#[test]
fn ground_aliases_do_not_rewrite_model_names() {
    let text = "ground\nr1 gnd 0 gnd\nr2 gnd 0 0\n.model gnd r\n.model 0 r\n.end\n";
    for auto in [true, false] {
        let n = Parser::with_auto_gnd(auto)
            .parse_deck(&parse_deck_text(Path::new("ground.cir"), text))
            .unwrap();
        assert_eq!(n.devices[0].nodes[0], if auto { "0" } else { "gnd" });
        assert_eq!(n.devices[0].model.as_deref(), Some("gnd"));
        assert!(n.devices[1].model.is_none());
        assert_eq!(n.models[1].name, "0");
    }
}

#[test]
fn geometry_without_a_model_or_primary_is_not_implicit_success() {
    for card in ["r1 a 0 l=1u w=2u", "c1 a 0 l=1u", "l1 a 0 nt=2", "r1 a 0"] {
        assert!(
            matches!(parse(card), Err(SpiceError::Parse { .. })),
            "{card}"
        );
    }
}

#[test]
fn malformed_extended_and_overflow_forms_commit_errors() {
    for tail in [
        "mdl 1e999",
        "1e999 mdl",
        "mdl r=1e999",
        "mdl r=",
        "mdl w=",
        "mdl w",
        "mdl =",
        "mdl 1k 2k",
        "mdl extra",
        "mdl \"expr\"",
        "\"expr\" mdl",
        "unresolved r=1k",
        "mdl w=\"expr\"",
        "mdl r=1k mdl",
    ] {
        let error = parse(&format!("r1 a 0 {tail}\n.model mdl r")).unwrap_err();
        assert!(
            matches!(error, SpiceError::Parse { .. }) || error.is_not_yet_ported(),
            "{tail}: {error}"
        );
        assert!(error.to_string().contains("passives.cir:2:"), "{error}");
        if tail.contains("1e999") || tail.ends_with('=') || tail == "mdl w" {
            assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
        }
    }
}

#[test]
fn continuations_and_unicode_columns_keep_original_assignment_positions() {
    let n = parse("R1 α 0 1k Rm\n+ w=2u r=3k\n.model Rm r").unwrap();
    let device = &n.devices[0];
    assert_eq!(device.location.line, 2);
    assert_eq!(device.nodes[0], "α");
    assert_eq!(device.model.as_deref(), Some("rm"));
    let p = &device.parameters;
    assert_eq!(p[0].value, "1k");
    assert_eq!(p[0].location.column, 9); // UTF-8 byte columns, not character count.
    assert_eq!(p[1].value, "2u");
    assert_eq!(p[2].value, "3k");
    assert!(p[1].location.column > p[0].location.column);
}
