//! Model recipe, geometry, contextual arithmetic and atomic elaboration contracts.
use spice_core::{NodeId, SpiceError};
use spice_devices::{
    AnalysisMode, Circuit, ModelContext, ModelFamily, ModelResolver, PassiveParameters,
    StampContext,
};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::path::Path;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("passive.cir"),
            &format!("passives\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn parameters(n: &Netlist, index: usize) -> Result<PassiveParameters, SpiceError> {
    ModelResolver::new(&n.models)?
        .resolve(&n.devices[index])?
        .unwrap()
        .passive_parameters(&n.devices[index])
}
fn effective(body: &str) -> Result<f64, SpiceError> {
    parameters(&deck(body), 0)?.effective_value(&ModelContext::default())
}
fn close(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs() + 1e-24,
        "{got} != {want}"
    );
}

#[test]
fn scalar_models_and_ordered_instance_setters_preserve_raw_ast() {
    for (d, keyword, primary, scalar, family) in [
        ('r', "r", "resistance", "2k", ModelFamily::Resistor),
        ('c', "cap", "capacitance", "2u", ModelFamily::Capacitor),
        ('l', "ind", "inductance", "2m", ModelFamily::Inductor),
    ] {
        let n = deck(&format!(
            "{d}1 a 0 mdl\n{d}2 a 0 3 mdl {primary}=4 {primary}=5\n{d}3 a 0 mdl 6 {primary}=7\n{d}4 a 0 8 mdl 9 {primary}=10\n.model mdl {d}({keyword}=1 {keyword}={scalar})\n.model mdl {d}({keyword}=99)"
        ));
        let before = n.clone();
        let model = parameters(&n, 0).unwrap();
        assert_eq!(model.family(), family);
        close(
            model.nominal_value(),
            spice_core::parse_spice_number(scalar).unwrap(),
        );
        for (index, want) in [(1, 5.0), (2, 6.0), (3, 9.0)] {
            close(parameters(&n, index).unwrap().nominal_value(), want);
        }
        assert!(Circuit::from_netlist(&n).is_ok());
        assert_eq!(n, before);
    }
    close(effective("r1 a 0 mdl\n.model mdl res(r=2k)").unwrap(), 2e3);
}

#[test]
fn resistor_sheet_precedence_defaults_and_two_sided_corrections() {
    // RSH selects geometry ahead of model R; instance scalar selects ahead of both.
    let n = deck(
        "r1 a 0 mdl l=8u w=4u\nr2 a 0 mdl\nr3 a 0 7k mdl l=1u w=1u\n.model mdl r(r=9k rsh=100 l=12u defw=6u short=1u narrow=0.5u)",
    );
    close(parameters(&n, 0).unwrap().nominal_value(), 200.0);
    close(parameters(&n, 1).unwrap().nominal_value(), 200.0);
    close(parameters(&n, 2).unwrap().nominal_value(), 7e3);
    close(
        effective("r1 a 0 mdl\n.model mdl r(rsh=100)").unwrap(),
        100.0,
    );
    close(
        effective("r1 a 0 mdl\n.model mdl r(r=2k rsh=0)").unwrap(),
        2e3,
    );
}

#[test]
fn capacitor_area_sidewall_geometry_and_scalar_precedence() {
    let n = deck(
        "c1 a 0 mdl l=10u w=6u\nc2 a 0 mdl l=12u\nc3 a 0 7u mdl\n.model mdl c(cj=2m cjsw=3n defw=8u short=2u narrow=1u)",
    );
    close(
        parameters(&n, 0).unwrap().nominal_value(),
        2e-3 * 5e-6 * 8e-6 + 3e-9 * 2.0 * 13e-6,
    );
    close(
        parameters(&n, 1).unwrap().nominal_value(),
        2e-3 * 7e-6 * 10e-6 + 3e-9 * 2.0 * 17e-6,
    );
    close(parameters(&n, 2).unwrap().nominal_value(), 7e-6);
    close(
        effective("c1 a 0 mdl l=10u w=6u\n.model mdl c(cap=2u cj=2m)").unwrap(),
        2e-6,
    );
    close(
        effective("c1 a 0 mdl l=4u\n.model mdl c(cjsw=1n)").unwrap(),
        1e-9 * 2.0 * 14e-6,
    );
}

#[test]
fn temperature_coefficients_scale_and_parallel_multiplicity() {
    let context = ModelContext {
        temperature: 77.0,
        nominal_temperature: 22.0,
    };
    for (d, keyword, primary, want) in [
        ('r', "r", "resistance", 180.0),
        ('c', "cap", "capacitance", 720.0),
        ('l', "ind", "inductance", 180.0),
    ] {
        let n = deck(&format!(
            "{d}1 a 0 mdl {primary}=100 tc1=0.01 tc2=0.001 temp=40 scale=3 m=2\n.model mdl {d}({keyword}=90 tc1=99 tc2=99 tnom=30)"
        ));
        let p = parameters(&n, 0).unwrap();
        close(p.effective_value(&context).unwrap(), want); // dt=10, factor=1.2
        close(p.effective_value(&context).unwrap(), want);
        assert_eq!(p.multiplicity(), 2.0);
    }
    // Independent per-coefficient override; no override/default is cached into AST.
    let n = deck("r1 a 0 mdl tc1=0\n.model mdl r(r=100 tc1=1 tc2=0.001)");
    let p = parameters(&n, 0).unwrap();
    close(
        p.effective_value(&context).unwrap(),
        100.0 * (1.0 + 0.001 * 55.0 * 55.0),
    );
    close(p.effective_value(&ModelContext::default()).unwrap(), 100.0);
    assert!(n.models[0].parameters.iter().all(|p| p.name != "tnom"));
}

#[test]
fn negative_instance_resistance_is_retained_but_models_and_cl_are_positive() {
    close(effective("r1 a 0 -2k mdl m=2\n.model mdl r").unwrap(), -1e3);
    for body in [
        "r1 a 0 mdl\n.model mdl r(r=-2k)",
        "c1 a 0 -2u mdl\n.model mdl c",
        "l1 a 0 -2m mdl\n.model mdl l",
    ] {
        assert!(effective(body).is_err(), "{body}");
    }
}

#[test]
fn missing_values_and_invalid_geometry_fail_without_c_fallbacks() {
    for body in [
        "r1 a 0 mdl\n.model mdl r",
        "r1 a 0 mdl\n.model mdl r(rsh=0)",
        "r1 a 0 mdl l=2u\n.model mdl r(rsh=100 short=1u)",
        "r1 a 0 mdl w=2u\n.model mdl r(rsh=100 narrow=1u)",
        "c1 a 0 mdl\n.model mdl c(cj=1m)",
        "c1 a 0 mdl l=1u\n.model mdl c",
        "c1 a 0 mdl l=2u\n.model mdl c(cj=1m short=2u)",
        "c1 a 0 mdl l=2u w=1u\n.model mdl c(cj=1m narrow=2u)",
        "l1 a 0 mdl\n.model mdl l",
    ] {
        let error = Circuit::from_netlist(&deck(body)).unwrap_err();
        assert!(matches!(error, SpiceError::Parse { .. }), "{body}: {error}");
        assert!(error.to_string().contains("passive.cir:"), "{error}");
    }
}

#[test]
fn all_raw_setters_are_validated_even_if_overwritten_or_unused_by_precedence() {
    for setter in [
        "r=0",
        "r=-1 r=2k",
        "rsh=-1",
        "defw=0",
        "l=-1",
        "short=-1u",
        "narrow=-1u",
        "tnom=-273.15",
        "kf=1",
        "res=2k",
        "tce=1",
        "w=1u",
    ] {
        let n = deck(&format!("r1 a 0 1k mdl\n.model mdl r({setter})"));
        assert!(parameters(&n, 0).is_err(), "{setter}");
    }
    for (d, setter) in [
        ('r', "resistance=0 resistance=1k"),
        ('r', "w=0"),
        ('r', "m=0"),
        ('c', "capacitance=-1 capacitance=1u"),
        ('l', "inductance=0"),
        ('l', "scale=-1"),
        ('r', "temp=-273.15"),
        ('r', "dtemp=1"),
        ('r', "ac=1k"),
        ('r', "tc=1"),
        ('c', "bv_max=1"),
        ('l', "nt=2"),
    ] {
        let n = deck(&format!("{d}1 a 0 mdl {setter}\n.model mdl {d}"));
        assert!(parameters(&n, 0).is_err(), "{d} {setter}");
    }
    for (base, setters) in [
        ("c", "defl=12u"),
        ("c", "cox=1m"),
        ("c", "del=1u"),
        ("c", "thick=1n"),
        ("l", "csect=1u"),
        ("l", "nt=2"),
        ("l", "mu=1"),
    ] {
        assert!(
            parameters(
                &deck(&format!("{base}1 a 0 1 mdl\n.model mdl {base}({setters})")),
                0
            )
            .is_err()
        );
    }
}

#[test]
fn nonfinite_programmatic_values_and_arithmetic_overflow_are_rejected() {
    for text in ["NaN", "inf", "1e999", "{expr}"] {
        let mut n = deck("r1 a 0 mdl\n.model mdl r(r=2k)");
        n.models[0].parameters[0].value = text.into();
        assert!(parameters(&n, 0).is_err());
        let mut n = deck("r1 a 0 mdl m=2\n.model mdl r(r=2k)");
        n.devices[0].parameters[0].value = text.into();
        assert!(parameters(&n, 0).is_err());
    }
    for body in [
        "r1 a 0 mdl\n.model mdl r(rsh=1e300 l=1e300 defw=1e-300)",
        "r1 a 0 mdl\n.model mdl r(rsh=1 narrow=1e308)",
        "c1 a 0 mdl l=1e300 w=1e300\n.model mdl c(cj=1)",
        "c1 a 0 mdl l=1e308 w=1e308\n.model mdl c(cjsw=1)",
        "r1 a 0 mdl temp=40\n.model mdl r(r=1 tc1=1e308 tnom=20)",
        "c1 a 0 mdl temp=40\n.model mdl c(cap=1 tc2=1e308 tnom=20)",
        "l1 a 0 mdl scale=2\n.model mdl l(ind=1e308)",
        "c1 a 0 mdl m=2\n.model mdl c(cap=1e308)",
        "r1 a 0 mdl m=1e308\n.model mdl r(r=1e-300)",
        "l1 a 0 mdl m=1e308\n.model mdl l(ind=1e-300)",
        "r1 a 0 mdl\n.model mdl r(r=1e-320)",
        "r1 a 0 mdl temp=30\n.model mdl r(r=1 tc1=-1 tnom=20)",
        "c1 a 0 mdl temp=30\n.model mdl c(cap=1 tc1=-0.1 tnom=20)",
    ] {
        assert!(Circuit::from_netlist(&deck(body)).is_err(), "{body}");
    }
}

#[test]
fn failed_instance_elaboration_preserves_all_circuit_namespaces() {
    let mut circuit =
        Circuit::from_netlist(&deck("v1 old 0 1\nl1 old mid 1m\nr1 mid 0 1k")).unwrap();
    let nodes = circuit.nodes().nodes().to_vec();
    let names: Vec<_> = circuit
        .devices()
        .iter()
        .map(|d| d.name().to_owned())
        .collect();
    let rows: Vec<_> = (0..circuit.device_count())
        .map(|i| circuit.branch_rows(i))
        .collect();
    let unknowns = circuit.unknown_count();
    for body in [
        "r2 new 0 missing\n.model missing r(r=1k)",
        "r2 new 0 mdl\n.model mdl c(cap=1u)",
        "r2 new 0 mdl\n.model mdl r(rsh=100 narrow=10u)",
        "c2 new 0 mdl\n.model mdl c(cj=1m)",
        "l2 new 0 mdl\n.model mdl l(ind=1m mu=2)",
        "r2 new 0 mdl temp=40\n.model mdl r(r=1 tc1=-1 tnom=20)",
        "r2 new 0 mdl\n.model mdl r(level=2 r=1)",
        "v1 new 0 1",
        "l2 new 0 mdl\n.model mdl l(ind=1e308)",
    ] {
        let mut n = deck(body);
        if n.devices[0].model.as_deref() == Some("missing") {
            n.models.clear();
        }
        if body.contains("1e308") {
            n.devices[0].nodes.push("extra".into());
        }
        let resolver = ModelResolver::new(&n.models).unwrap();
        assert!(
            circuit
                .add_instance(&n.devices[0], &resolver, &ModelContext::default())
                .is_err(),
            "{body}"
        );
        assert_eq!(circuit.nodes().nodes(), nodes);
        assert_eq!(
            circuit
                .devices()
                .iter()
                .map(|d| d.name().to_owned())
                .collect::<Vec<_>>(),
            names
        );
        assert_eq!(
            (0..circuit.device_count())
                .map(|i| circuit.branch_rows(i))
                .collect::<Vec<_>>(),
            rows
        );
        assert_eq!(circuit.unknown_count(), unknowns);
    }
}

#[test]
fn repeated_contextual_assembly_is_immutable_and_numbering_stays_bound() {
    let n = deck(
        "v1 a 0 1\nr1 a b rm\nl1 b 0 lm\nc1 a 0 cm\n.model rm r(r=1k tc1=0.01)\n.model lm l(ind=1m tc1=0.01)\n.model cm c(cap=1u tc1=0.01)",
    );
    let mut circuit = Circuit::from_netlist(&n).unwrap();
    let nodes = circuit.nodes().nodes().to_vec();
    let rows = circuit.branch_rows(2).unwrap();
    let cold = circuit.linear_system().unwrap();
    let hot = circuit
        .linear_system_with_context(&ModelContext {
            temperature: 77.0,
            nominal_temperature: 27.0,
        })
        .unwrap();
    let restored = circuit.linear_system().unwrap();
    assert_eq!(cold.a, restored.a);
    assert_eq!(cold.e, restored.e);
    assert_ne!(cold.a, hot.a);
    assert_ne!(cold.e, hot.e);
    assert_eq!(rows, circuit.branch_rows(2).unwrap());
    assert_eq!(circuit.nodes().nodes(), nodes);
    let error = circuit.linear_system_with_context(&ModelContext {
        temperature: f64::NAN,
        nominal_temperature: 27.0,
    });
    assert!(error.is_err());
    assert_eq!(circuit.nodes().nodes(), nodes);
}

#[test]
fn real_stamp_uses_explicit_temperature_and_initial_conditions_remain_visible() {
    let n = deck("r1 a 0 mdl\n.model mdl r(r=1k tc1=0.01)");
    let mut circuit = Circuit::from_netlist(&n).unwrap();
    let unknowns = circuit.unknowns().clone();
    let nodes = circuit.nodes().clone();
    let mut matrix = SparseMatrix::new(1, 1);
    let mut rhs = Vector::zeros(1);
    let solution = Vector::zeros(1);
    let mut context = StampContext {
        matrix: &mut matrix,
        rhs: &mut rhs,
        unknowns: &unknowns,
        nodes: &nodes,
        solution: &solution,
        temperature: 77.0,
        nominal_temperature: 27.0,
        mode: AnalysisMode::OperatingPoint,
        branch: None,
    };
    circuit.devices_mut()[0].stamp(&mut context).unwrap();
    close(matrix.get(0, 0), 1.0 / 1500.0);
    for (d, setter, model) in [('c', "cap", "1u"), ('l', "ind", "1m")] {
        let n = deck(&format!(
            "{d}1 a 0 mdl ic=2\n.model mdl {d}({setter}={model})"
        ));
        let p = parameters(&n, 0).unwrap();
        assert_eq!(p.initial_condition(), Some(2.0));
        assert!(
            Circuit::from_netlist(&n)
                .unwrap()
                .linear_system()
                .unwrap()
                .has_initial_conditions
        );
    }
    // No change to literal factories or negative scalar resistance contract.
    let r = spice_devices::Resistor::new("r", [NodeId::new(1), NodeId::GROUND], -1.0).unwrap();
    assert_eq!(r.conductance(), -1.0);
}

#[test]
fn unused_models_and_unknown_setters_cannot_disappear_behind_supported_instances() {
    let n = deck("r1 a 0 mdl\n.model mdl r(r=2k)\n.model unused r(kf=1)");
    assert!(
        Circuit::from_netlist(&n)
            .unwrap_err()
            .to_string()
            .contains("unused model declarations")
    );
    let mut n = deck("r1 a 0 mdl\n.model mdl r(r=2k)");
    n.devices[0].model = Some("wrong".into());
    assert!(
        ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .is_err()
    );
}
