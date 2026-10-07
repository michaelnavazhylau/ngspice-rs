//! #12/#13 must not silently enable subcircuit flattening or simulation.
use spice_devices::{Circuit, ModelContext, ModelResolver};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

#[test]
fn structure_decks_remain_explicitly_unavailable_for_linear_elaboration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
    for path in [
        root.join("netlists/subckt_divider.cir"),
        root.join("parser/sources/main.cir"),
    ] {
        let n = Parser::new().parse_file(path).unwrap();
        assert!(matches!(
            Circuit::from_netlist(&n),
            Err(spice_core::SpiceError::Unsupported { .. })
        ));
    }
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
