//! Physical resistor metadata and the immutable per-point override (#35).
//! The override replaces the *supplied scalar*; the effective resistance is
//! recomputed from the context, and no device, model recipe or AST changes.
use ngspice_rs::devices::{
    AnalysisMode, Capacitor, Circuit, IndependentSource, Inductor, LoadRequest,
    MAX_RESISTOR_OVERRIDES, ModelContext, Resistor, ResistorOrigin, Waveform,
};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::{Parser, ast::Netlist, source::parse_deck_text};
use ngspice_rs::primitives::{Complex, NodeId};
use std::path::Path;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("override.cir"),
            &format!("override\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn close(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs() + 1e-24,
        "{got} != {want}"
    );
}
/// `v9` is a resistor, `r9` a voltage source, `rc` a capacitor, `r_l` an inductor:
/// names deliberately contradict SPICE's first-letter convention.
fn misnamed() -> Circuit {
    let mut c = Circuit::new();
    let a = c.add_node("a");
    let b = c.add_node("b");
    c.add_device(Box::new(Resistor::new("v9", [a, b], 1e3).unwrap()))
        .unwrap();
    c.add_device(Box::new(
        IndependentSource::new(
            "r9",
            [a, NodeId::GROUND],
            true,
            1.,
            Complex::ZERO,
            Waveform::Constant(1.),
        )
        .unwrap(),
    ))
    .unwrap();
    c.add_device(Box::new(
        Capacitor::new("rc", [b, NodeId::GROUND], 1e-6, None).unwrap(),
    ))
    .unwrap();
    c.add_device(Box::new(
        Inductor::new("r_l", [b, NodeId::GROUND], 1e-3, None).unwrap(),
    ))
    .unwrap();
    c.finalize().unwrap();
    c
}
fn load_a00(c: &Circuit, context: &ModelContext) -> f64 {
    let history = c.state_history();
    let mut trial = history.trial();
    let mut matrix = SparseMatrix::new(c.unknown_count(), c.unknown_count());
    c.load(
        &LoadRequest {
            mode: AnalysisMode::OperatingPoint,
            solution: &Vector::zeros(c.unknown_count()),
            model_context: context,
            integration: None,
            history: &history,
            forcing: None,
        },
        &mut matrix,
        &mut Vector::zeros(c.unknown_count()),
        &mut trial,
    )
    .unwrap();
    matrix.get(0, 0)
}

#[test]
fn metadata_comes_from_the_device_not_the_name_prefix() {
    let c = misnamed();
    let (index, metadata) = c.resistor("V9").unwrap(); // case-insensitive
    assert_eq!(index, 0);
    assert_eq!(metadata.origin, ResistorOrigin::Literal);
    assert_eq!((metadata.supplied, metadata.multiplicity), (1e3, 1.));
    for not_a_resistor in ["r9", "rc", "r_l", "missing", "r1.r", "tc1", ""] {
        assert!(c.resistor(not_a_resistor).is_none(), "{not_a_resistor}");
        assert!(c.resistor_override(not_a_resistor, 1e3).is_err());
    }
    let n = deck("r1 a 0 rm scale=2 m=4\n.model rm r(r=1k)");
    let c = Circuit::from_netlist(&n).unwrap();
    let (_, metadata) = c.resistor("r1").unwrap();
    assert_eq!(metadata.origin, ResistorOrigin::ModelBacked);
    // The supplied scalar excludes temperature, scale and multiplicity.
    assert_eq!((metadata.supplied, metadata.multiplicity), (1e3, 4.));
    // Model-backed capacitors/inductors are not resistors.
    let n = deck("c1 a 0 cm\nl1 a 0 lm\n.model cm c(cap=1u)\n.model lm l(ind=1m)");
    let c = Circuit::from_netlist(&n).unwrap();
    assert!(c.resistor("c1").is_none() && c.resistor("l1").is_none());
}

#[test]
fn override_construction_rejects_unusable_supplied_values() {
    let c = misnamed();
    let ok = c.resistor_override("v9", 500.).unwrap();
    assert_eq!((ok.device(), ok.supplied()), (0, 500.));
    // Negative nonzero scalars stay allowed, as for literal instances.
    assert!(c.resistor_override("v9", -50.).is_ok());
    for bad in [0., -0., f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e-320] {
        assert!(c.resistor_override("v9", bad).is_err(), "{bad}");
    }
}

#[test]
fn context_holds_a_bounded_duplicate_free_set_of_overrides() {
    assert_eq!(MAX_RESISTOR_OVERRIDES, 2);
    let n = deck("r1 a 0 1k\nr2 a 0 1k\nr3 a 0 1k");
    let c = Circuit::from_netlist(&n).unwrap();
    let context = ModelContext::default();
    assert!(context.resistor_overrides.iter().all(Option::is_none));
    let one = context
        .with_resistor_override(c.resistor_override("r1", 1.).unwrap())
        .unwrap();
    let two = one
        .with_resistor_override(c.resistor_override("r2", 2.).unwrap())
        .unwrap();
    assert_eq!(context, ModelContext::default()); // Copy: the original is unchanged
    assert!(
        one.with_resistor_override(c.resistor_override("r1", 9.).unwrap())
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    assert!(
        two.with_resistor_override(c.resistor_override("r3", 3.).unwrap())
            .unwrap_err()
            .to_string()
            .contains("too many")
    );
}

#[test]
fn every_assembly_and_load_path_sees_the_override_without_mutating_devices() {
    let n = deck("r1 a 0 1k");
    let before = n.clone();
    let mut c = Circuit::from_netlist(&n).unwrap();
    let context = ModelContext::default()
        .with_resistor_override(c.resistor_override("r1", 250.).unwrap())
        .unwrap();
    let zero = Vector::zeros(c.unknown_count());
    close(
        c.linear_system_with_context(&context).unwrap().a.get(0, 0),
        1. / 250.,
    );
    close(
        c.small_signal_system(&context, &zero).unwrap().a.get(0, 0),
        1. / 250.,
    );
    close(load_a00(&c, &context), 1. / 250.);
    // The circuit, its resistor and the AST are exactly as before.
    close(c.linear_system().unwrap().a.get(0, 0), 1. / 1e3);
    close(load_a00(&c, &ModelContext::default()), 1. / 1e3);
    assert_eq!(c.resistor("r1").unwrap().1.supplied, 1e3);
    assert_eq!(n, before);
}

#[test]
fn model_backed_override_keeps_temperature_tc_scale_and_multiplicity() {
    // R = supplied * (1 + 0.01*20) * scale / m = supplied * 0.6 at 47 C, tnom 27 C.
    let n = deck("r1 a 0 rm scale=2 m=4\n.model rm r(r=1k tc1=0.01)");
    let hot = ModelContext::new(47., 27.);
    let mut c = Circuit::from_netlist_with_context(&n, &hot).unwrap();
    close(
        c.linear_system_with_context(&hot).unwrap().a.get(0, 0),
        1. / 600.,
    );
    for (supplied, effective) in [(1e3, 600.), (2e3, 1200.), (-3e3, -1800.)] {
        let target = c.resistor_override("r1", supplied).unwrap();
        close(c.effective_resistance(&target, &hot).unwrap(), effective);
        let context = hot.with_resistor_override(target).unwrap();
        close(
            c.linear_system_with_context(&context).unwrap().a.get(0, 0),
            1. / effective,
        );
        close(
            c.small_signal_system(&context, &Vector::zeros(1))
                .unwrap()
                .a
                .get(0, 0),
            1. / effective,
        );
        close(load_a00(&c, &context), 1. / effective);
    }
    // At nominal temperature the same supplied scalar gives scale/m only.
    let nominal = ModelContext::default();
    let target = c.resistor_override("r1", 2e3).unwrap();
    close(c.effective_resistance(&target, &nominal).unwrap(), 1e3);
    // A temperature that makes the factor nonpositive is an error, not a clamp.
    let n = deck("r1 a 0 rm\n.model rm r(r=1k tc1=-0.1)");
    let c = Circuit::from_netlist(&n).unwrap();
    let target = c.resistor_override("r1", 1e3).unwrap();
    assert!(
        c.effective_resistance(&target, &ModelContext::new(87., 27.))
            .is_err()
    );
    assert!(
        c.effective_resistance(&target, &ModelContext::new(f64::NAN, 27.))
            .is_err()
    );
}

#[test]
fn instance_temp_still_overrides_the_swept_circuit_temperature() {
    let n = deck("r1 a 0 rm temp=77\n.model rm r(r=1k tc1=0.01 tnom=27)");
    let c = Circuit::from_netlist(&n).unwrap();
    let target = c.resistor_override("r1", 1e3).unwrap();
    for temperature in [0., 27., 127.] {
        close(
            c.effective_resistance(&target, &ModelContext::new(temperature, 27.))
                .unwrap(),
            1e3 * 1.5,
        );
    }
}

#[test]
fn stale_or_mismatched_overrides_are_errors() {
    let a = Circuit::from_netlist(&deck("r1 x 0 1k\nr2 x 0 1k\nr3 x 0 1k")).unwrap();
    let late = ModelContext::default()
        .with_resistor_override(a.resistor_override("r3", 5.).unwrap())
        .unwrap();
    // Ordinal 2 does not exist in a one-device circuit.
    let mut small = Circuit::from_netlist(&deck("r1 x 0 1k")).unwrap();
    let error = small.linear_system_with_context(&late).unwrap_err();
    assert!(error.to_string().contains("stale"), "{error}");
    assert!(small.small_signal_system(&late, &Vector::zeros(1)).is_err());
    assert!(
        small
            .effective_resistance(
                &a.resistor_override("r3", 5.).unwrap(),
                &ModelContext::default()
            )
            .is_err()
    );
    // Ordinal 0 exists but is a capacitor here.
    let first = ModelContext::default()
        .with_resistor_override(a.resistor_override("r1", 5.).unwrap())
        .unwrap();
    let mut other = Circuit::from_netlist(&deck("c1 x 0 1u\nr1 x 0 1k")).unwrap();
    let error = other.linear_system_with_context(&first).unwrap_err();
    assert!(
        error.to_string().contains("not a two-terminal resistor"),
        "{error}"
    );
}
