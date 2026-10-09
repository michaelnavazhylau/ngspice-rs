//! Parsed syntax must not silently become runtime support (#8/#10); PULSE/PWL
//! source waveforms are enabled by #9 and invalid ones fail atomically.
use spice_core::NodeTable;
use spice_devices::{Circuit, ModelContext, ModelResolver, Registry};
use spice_netlist::{Parser, RawCard, source::parse_deck_text};
use std::path::Path;

#[test]
fn invalid_source_waveforms_are_rejected_before_mutating_nodes_or_circuit() {
    for card in [
        "Vnew fresh 0 7 dc 2 pwl(1u 1 1u 2)",
        "Inew fresh 0 pwl(-1u 1 1u 2)",
        "Vnew fresh 0 pulse(0 1 -1) dc 7",
    ] {
        let deck = parse_deck_text(
            Path::new("setters.cir"),
            &format!("Title\nr1 old 0 1k\n{card}\n"),
        );
        let netlist = Parser::new().parse_deck(&deck).unwrap();
        let original = netlist.clone();
        let models = ModelResolver::new(&netlist.models).unwrap();
        let mut circuit = Circuit::new();
        circuit
            .add_instance(&netlist.devices[0], &models, &ModelContext::default())
            .unwrap();
        circuit.finalize().unwrap();
        let before = (
            circuit.nodes().nodes().to_vec(),
            circuit.device_count(),
            circuit.unknown_count(),
            circuit.branch_rows(0),
        );
        let error = circuit
            .add_instance(&netlist.devices[1], &models, &ModelContext::default())
            .unwrap_err();
        assert!(!error.is_not_yet_ported(), "{card}: {error}");
        assert_eq!(
            (
                circuit.nodes().nodes().to_vec(),
                circuit.device_count(),
                circuit.unknown_count(),
                circuit.branch_rows(0)
            ),
            before
        );
        assert_eq!(netlist, original);
        assert!(Circuit::from_netlist(&netlist).is_err());
        let raw = RawCard::parse(&deck.lines[1]).unwrap();
        let mut nodes = NodeTable::new();
        nodes.intern("old");
        let before = nodes.nodes().to_vec();
        let error = match Registry::with_builtins().instantiate(&raw, &mut nodes) {
            Err(error) => error,
            Ok(_) => panic!("invalid waveform"),
        };
        assert!(!error.is_not_yet_ported());
        assert_eq!(nodes.nodes(), before);
    }
}

#[test]
fn off_flags_and_initial_condition_vectors_build_nonlinear_devices() {
    // #99: D/Q/M `off` and `ic` setters are consumed by their factories
    // (dioload.c, bjtload.c/bjtgetic.c, mos1load.c/mos1ic.c).
    for body in [
        "Dnew fresh 0 mdl OFF IC=.5\n.model mdl d",
        "Qnew c b e mdl OFF IC=.6,2 icvce=3\n.model mdl npn",
        "Mnew drain gate source bulk mdl OFF IC=(1 2 3)\n.model mdl nmos",
    ] {
        let deck = parse_deck_text(Path::new("setters.cir"), &format!("Title\n{body}\n"));
        let netlist = Parser::new().parse_deck(&deck).unwrap();
        let circuit = Circuit::from_netlist(&netlist).unwrap();
        assert_eq!(circuit.device_count(), 1, "{body}");
        assert!(circuit.devices()[0].has_start_settings(), "{body}");
    }
}

#[test]
fn parsed_flags_vectors_and_model_flags_do_not_enable_nonlinear_factories() {
    for body in [
        "Dnew fresh 0 mdl\n.model mdl d(d)",
        "Qnew c b e mdl\n.model mdl npn(npn)",
        "Mnew drain gate source bulk mdl\n.model mdl nmos(pmos)",
    ] {
        let deck = parse_deck_text(
            Path::new("setters.cir"),
            &format!("Title\nr1 old 0 1k\n{body}\n"),
        );
        let netlist = Parser::new().parse_deck(&deck).unwrap();
        let original = netlist.clone();
        let models = ModelResolver::new(&netlist.models).unwrap();
        let mut circuit = Circuit::new();
        circuit
            .add_instance(&netlist.devices[0], &models, &ModelContext::default())
            .unwrap();
        circuit.finalize().unwrap();
        let before = (
            circuit.nodes().nodes().to_vec(),
            circuit.device_count(),
            circuit.unknown_count(),
            circuit.branch_rows(0),
        );
        let error = circuit
            .add_instance(&netlist.devices[1], &models, &ModelContext::default())
            .unwrap_err();
        assert!(
            error.is_not_yet_ported()
                || matches!(error, spice_core::SpiceError::Unsupported { .. }),
            "{body}: {error}"
        );
        assert_eq!(
            (
                circuit.nodes().nodes().to_vec(),
                circuit.device_count(),
                circuit.unknown_count(),
                circuit.branch_rows(0)
            ),
            before
        );
        assert_eq!(netlist, original);
        assert!(Circuit::from_netlist(&netlist).is_err());
    }
}

#[test]
fn scalar_consumers_reject_forged_non_scalar_kinds_even_with_numeric_text() {
    use spice_netlist::ast::ParameterKind;
    let deck = parse_deck_text(
        Path::new("setters.cir"),
        "Title\nr1 a 0 1k\nr2 b 0 rm resistance=1k\n.model rm r(r=1k)\n",
    );
    for kind in [
        ParameterKind::Flag,
        ParameterKind::InitialConditions(Vec::new()),
        ParameterKind::Textual,
    ] {
        let mut netlist = Parser::new().parse_deck(&deck).unwrap();
        netlist.devices[0].parameters[0].kind = kind.clone();
        assert!(
            Circuit::from_netlist(&netlist)
                .unwrap_err()
                .is_not_yet_ported()
        );
        let mut netlist = Parser::new().parse_deck(&deck).unwrap();
        netlist.devices[1].parameters[0].kind = kind.clone();
        assert!(Circuit::from_netlist(&netlist).is_err());
        let mut netlist = Parser::new().parse_deck(&deck).unwrap();
        netlist.models[0].parameters[0].kind = kind;
        assert!(Circuit::from_netlist(&netlist).is_err());
    }
}
