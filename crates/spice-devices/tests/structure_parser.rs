//! #12/#13 parse structure without flattening; #18 elaborates top-level `.subckt`
//! definitions deliberately. Resolved `.include`/`.lib` content is inlined and
//! elaborates; a deck whose includes were never resolved stays explicitly
//! unavailable.
use spice_devices::{Circuit, ModelContext, ModelResolver};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

#[test]
fn the_committed_subcircuit_deck_now_elaborates_and_includes_resolve() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
    let n = Parser::new()
        .parse_file(root.join("netlists/subckt_divider.cir"))
        .unwrap();
    let circuit = Circuit::from_netlist(&n).unwrap();
    assert_eq!(
        circuit
            .devices()
            .iter()
            .map(|device| device.name())
            .collect::<Vec<_>>(),
        ["v1", "r.x1.r1", "r2"]
    );
    // The subcircuit comes from an include and its resistor model from the
    // selected `.lib typical` section (which itself includes shared/sheet.inc).
    let main = root.join("parser/sources/main.cir");
    let n = Parser::new().parse_file(&main).unwrap();
    let circuit = Circuit::from_netlist(&n).unwrap();
    assert_eq!(
        circuit
            .devices()
            .iter()
            .map(|device| device.name())
            .collect::<Vec<_>>(),
        ["v1", "r.x1.r1", "r2"]
    );
    // The same deck parsed without resolution has no subcircuit or model: it is
    // an explicit error, never a partial circuit or an empty include.
    let text = std::fs::read_to_string(&main).unwrap();
    let n = Parser::new()
        .parse_deck(&parse_deck_text(&main, &text))
        .unwrap();
    assert!(matches!(
        Circuit::from_netlist(&n),
        Err(spice_core::SpiceError::Unsupported { .. })
    ));
}

#[test]
fn x_instance_factory_failure_is_atomic_even_without_a_definition() {
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("x.cir"),
            "title\nr1 old 0 1k\nxnew fresh 0 missing k={base}\n",
        ))
        .unwrap();
    let models = ModelResolver::new(&n.models).unwrap();
    let mut c = Circuit::new();
    c.add_instance(&n.devices[0], &models, &ModelContext::default())
        .unwrap();
    c.finalize().unwrap();
    let before = (
        c.nodes().nodes().to_vec(),
        c.device_count(),
        c.unknown_count(),
        c.branch_rows(0),
    );
    assert!(
        c.add_instance(&n.devices[1], &models, &ModelContext::default())
            .is_err()
    );
    assert_eq!(
        (
            c.nodes().nodes().to_vec(),
            c.device_count(),
            c.unknown_count(),
            c.branch_rows(0)
        ),
        before
    );
}
