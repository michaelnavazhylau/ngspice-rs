//! Offline BSIM3 device tests: DC `.op` through the solver, terminal/state
//! wiring and explicit refusals for the unported physics. C parity lives in the
//! opt-in `c_bsim3_device_reference` suite.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::{Circuit, ModelContext};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::SpiceError;

fn deck(body: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("bsim3-device.cir"),
            &format!("BSIM3 device\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&deck(body)).unwrap()
}
fn operating(body: &str) -> Plot {
    let netlist = deck(&format!("{body}\n.op"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}

/// Bias of the fixed-point tests: C's bare BSIM3 default model is above
/// threshold at Vgs = 2.5 V.
const CORE: &str = "vd d 0 1\nvg g 0 2.5\nvs s 0 0\n";

#[test]
fn operating_point_solves_the_drain_current_in_both_polarities() {
    for (family, polarity) in [("nmos", 1.), ("pmos", -1.)] {
        let body = format!(
            "vd d 0 {}e\nvg g 0 {}e\nvs s 0 0\n\
             m1 d g s 0 nn l=1u w=10u\n.model nn {family} level=8",
            1.0 * polarity,
            2.5 * polarity
        );
        let plot = operating(&body);
        let current = plot.value("i(vd)", 0).unwrap().re;
        // C's bare BSIM3 default model conducts about 1.2e-4 A here; the sign
        // follows the polarity because the drain source value does.
        assert!(current.is_finite(), "{family}: {current}");
        // `i(vd)` is the current the source drives into the drain, so it is
        // negative of the device's drain current for both polarities.
        assert!(
            current * polarity < -1e-5 && current * polarity > -1e-3,
            "{family}: {current}"
        );
        assert!(plot.value("i(vg)", 0).unwrap().re.abs() < 1e-15, "{family}");
    }
}

#[test]
fn device_state_and_observations_expose_the_c_quantities() {
    let netlist = deck(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8"
    ));
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let device = circuit.device("m1").expect("m1");
    assert_eq!(device.designator(), 'm');
    assert_eq!(device.terminals().len(), 4);
    assert_eq!(device.state_count(), 19);
    assert!(device.is_nonlinear());
    assert!(!device.has_start_settings());

    let plot = operating(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8"
    ));
    let solution: Vec<f64> = circuit
        .nodes()
        .nodes()
        .iter()
        .map(|node| {
            plot.value(&format!("v({})", node.name), 0)
                .map_or(0., |value| value.re)
        })
        .collect();
    let voltage = |node: ngspice_rs::primitives::NodeId| solution[node.index()];
    let context = ModelContext::default();
    let device = circuit.device("m1").unwrap();
    let id = device
        .observation_operating("id", &context, &voltage, false)
        .unwrap()
        .unwrap();
    let gm = device
        .observation_operating("gm", &context, &voltage, false)
        .unwrap()
        .unwrap();
    let vth = device
        .observation_operating("vth", &context, &voltage, false)
        .unwrap()
        .unwrap();
    assert!(id > 0. && gm > 0.);
    assert!(vth > 1.0 && vth < 3.0, "{vth}");
    // Charges are not ported, so the charge asks stay unavailable.
    for ask in ["cbs", "cbd", "qg", "cqg", "cgg"] {
        assert!(
            device
                .observation_operating(ask, &context, &voltage, false)
                .unwrap()
                .is_none(),
            "{ask}"
        );
    }
}

#[test]
fn series_resistance_creates_the_internal_primes() {
    let netlist = deck(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u nrd=2 nrs=3\n.model nn nmos level=8 rsh=10"
    ));
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let device = circuit.device("m1").unwrap();
    assert_eq!(device.terminals().len(), 6);
    let plot = operating(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u nrd=2 nrs=3\n.model nn nmos level=8 rsh=10"
    ));
    let current = plot.value("i(vd)", 0).unwrap().re;
    assert!(current.is_finite() && current.abs() > 1e-6, "{current}");
    // The internal nodes are unknowns of the solve.
    assert!(circuit.unknown_count() >= 6);
}

#[test]
fn unported_analyses_refuse_instead_of_loading_zero() {
    let body = format!("{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8");
    // AC needs b3acld.c.
    let netlist = deck(&format!("{body}\n.ac dec 1 1k 10k"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(matches!(error, SpiceError::NotYetPorted { .. }), "{error}");
    assert!(error.to_string().contains("b3acld.c"), "{error}");

    // Transient needs the charges of b3ld.c.
    let netlist = deck(&format!("{body}\n.tran 1u 2u"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(matches!(error, SpiceError::NotYetPorted { .. }), "{error}");
    // The transient driver assembles the bias-linearized system first, so the
    // missing charge/AC load may be reported through either reference.
    let text = error.to_string();
    assert!(
        text.contains("b3ld.c") || text.contains("b3acld.c"),
        "{error}"
    );
}

#[test]
fn off_and_ic_start_settings_are_reported() {
    let plain = circuit(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8"
    ));
    assert!(!plain.device("m1").unwrap().has_start_settings());
    for extra in ["off", "ic=(0.1,0.2,-0.3)", "icvds=0.1"] {
        let netlist = deck(&format!(
            "{CORE}\nm1 d g s 0 nn l=1u w=10u {extra}\n.model nn nmos level=8"
        ));
        let circuit = Circuit::from_netlist(&netlist).unwrap();
        assert!(
            circuit.device("m1").unwrap().has_start_settings(),
            "{extra}"
        );
    }
}

#[test]
fn acm_geometry_is_refused_by_the_factory() {
    let netlist = deck(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8 acm=1"
    ));
    let error = Circuit::from_netlist(&netlist).unwrap_err();
    assert!(matches!(error, SpiceError::NotYetPorted { .. }), "{error}");
    assert!(
        error.to_string().contains("ACM_SourceDrainResistances"),
        "{error}"
    );
}
