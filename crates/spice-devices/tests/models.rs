//! Production model resolution/schema interfaces and atomic factory boundaries.
use spice_core::{NodeTable, SourceLoc, SpiceError};
use spice_devices::schema::{
    ScalarDomain, ScalarParameter, ScalarSchema, ScalarUnit, temperature_kelvin,
};
use spice_devices::{Circuit, ModelContext, ModelFamily, ModelResolver, Registry};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::path::Path;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("models.cir"),
            &format!("models\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn instance(d: char) -> &'static str {
    match d {
        'r' => "r1 a 0 mdl",
        'c' => "c1 a 0 mdl",
        'l' => "l1 a 0 mdl",
        'd' => "d1 a 0 mdl",
        'q' => "q1 c b e mdl",
        _ => "m1 drain gate source bulk mdl",
    }
}
fn selection(
    d: char,
    base: &str,
    setters: &str,
) -> Result<spice_devices::LevelSelection, SpiceError> {
    let n = deck(&format!("{}\n.model mdl {base}({setters})", instance(d)));
    ModelResolver::new(&n.models)
        .unwrap()
        .resolve(&n.devices[0])
        .map(|model| model.unwrap().levels())
}

#[test]
fn families_and_forward_resolution_are_not_backend_availability() {
    for (d, base, family) in [
        ('r', "r", ModelFamily::Resistor),
        ('r', "res", ModelFamily::Resistor),
        ('c', "c", ModelFamily::Capacitor),
        ('l', "l", ModelFamily::Inductor),
        ('d', "d", ModelFamily::Diode),
        ('q', "npn", ModelFamily::Npn),
        ('q', "pnp", ModelFamily::Pnp),
        ('m', "nmos", ModelFamily::Nmos),
        ('m', "pmos", ModelFamily::Pmos),
    ] {
        let n = deck(&format!("{}\n.model mdl {base}", instance(d)));
        let resolved = ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .unwrap()
            .unwrap();
        assert_eq!(resolved.family(), family);
        assert_eq!(resolved.levels().selector, 1);
        if matches!(
            family,
            ModelFamily::Diode
                | ModelFamily::Npn
                | ModelFamily::Pnp
                | ModelFamily::Nmos
                | ModelFamily::Pmos
        ) {
            let circuit = Circuit::from_netlist(&n).unwrap();
            assert!(circuit.devices()[0].is_nonlinear());
            continue;
        }
        let error = Circuit::from_netlist(&n).unwrap_err();
        if matches!(
            family,
            ModelFamily::Resistor | ModelFamily::Capacitor | ModelFamily::Inductor
        ) {
            // Recognition alone supplies neither a scalar nor sufficient geometry.
            assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
        } else {
            assert!(error.is_not_yet_ported());
        }
    }
    let n = deck("r1 a 0 1k\nv1 a 0 1");
    let resolver = ModelResolver::new(&n.models).unwrap();
    assert!(resolver.resolve(&n.devices[0]).unwrap().is_none());
    assert!(Circuit::from_netlist(&n).is_ok());
}

#[test]
fn first_declaration_wins_case_insensitively_without_rewriting_ast() {
    let n = deck("d1 a 0 MDL\n.model mdl d(is=1e-14 is=2e-14)\n.model MDL npn(is=9e-14)");
    let original = n.clone();
    let resolver = ModelResolver::new(&n.models).unwrap();
    let model = resolver.resolve(&n.devices[0]).unwrap().unwrap();
    assert_eq!(model.card().location.line, 3);
    assert_eq!(model.card().parameters.len(), 2);
    assert_eq!(
        model
            .diode_parameters(&ModelContext::default())
            .unwrap()
            .saturation_current,
        2e-14
    );
    assert_eq!(resolver.model("MDL").unwrap(), &n.models[0]);
    assert_eq!(n, original);
    let next = deck("d1 a 0 mdl");
    assert!(
        ModelResolver::new(&next.models)
            .unwrap()
            .resolve(&next.devices[0])
            .is_err()
    );
}

#[test]
fn missing_and_wrong_family_errors_retain_sources() {
    let n = deck("d1 a 0 missing");
    let error = ModelResolver::new(&n.models)
        .unwrap()
        .resolve(&n.devices[0])
        .unwrap_err();
    assert!(matches!(error, SpiceError::Parse { ref location, .. } if location.line == 2));
    assert!(error.to_string().contains("missing"));
    for (d, base) in [
        ('r', "d"),
        ('c', "r"),
        ('l', "c"),
        ('d', "npn"),
        ('q', "nmos"),
        ('m', "pnp"),
    ] {
        let n = deck(&format!("{}\n.model mdl {base}", instance(d)));
        let error = ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .unwrap_err();
        assert!(error.to_string().contains("wrong model family"), "{error}");
        assert!(error.to_string().contains("models.cir:3:1"), "{error}");
        assert!(matches!(error, SpiceError::Parse { ref location, .. } if location.line == 2));
    }
    let mut n = deck("d1 a 0 mdl\n.model mdl d");
    n.devices[0].model = None;
    assert!(
        ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .unwrap_err()
            .to_string()
            .contains("requires a model")
    );
}

#[test]
fn model_symbols_do_not_share_numeric_or_ground_node_namespaces() {
    let n = deck(
        "d1 gnd 0 gnd\nd2 gnd 0 0\nd3 123 0 123\n.model gnd d(is=1e-14)\n.model 0 d(is=2e-14)\n.model 123 d(is=3e-14)",
    );
    let resolver = ModelResolver::new(&n.models).unwrap();
    for (i, name) in ["gnd", "0", "123"].iter().enumerate() {
        let resolved = resolver.resolve(&n.devices[i]).unwrap().unwrap();
        assert_eq!(resolved.card().name, *name);
        assert_eq!(n.devices[i].model.as_deref(), Some(*name));
    }
    assert_eq!(n.devices[0].nodes, ["0", "0"]);
    assert_ne!(
        resolver.model("gnd").unwrap().parameters,
        resolver.model("0").unwrap().parameters
    );
}

#[test]
fn first_level_rounding_and_last_diode_integer_setter_are_separate() {
    let mos = selection('m', "nmos", "level=1.49 level=49").unwrap();
    assert_eq!(mos.first_raw, Some(1.49));
    assert_eq!(mos.selector, 1);
    assert_eq!(mos.applied, None); // MOS1 has no model level setter.
    assert_eq!(
        selection('q', "npn", "level=1.5 level=9").unwrap().selector,
        2
    );
    assert_eq!(selection('q', "pnp", "level=0").unwrap().selector, 0);
    assert_eq!(selection('r', "r", "level=0").unwrap().selector, 0);
    let diode = selection('d', "d", "level=3 level=1.49").unwrap();
    assert_eq!(diode.first_raw, Some(3.0));
    assert_eq!(diode.selector, 1); // INPdomodel never scans diode level.
    assert_eq!(diode.applied, Some(1));
    let default = selection('d', "d", "").unwrap();
    assert_eq!(default.first_raw, None);
    assert_eq!(default.applied, Some(1));
    for (d, base, setters) in [
        ('m', "nmos", "level=1.5"),
        ('q', "npn", "level=3"),
        ('r', "r", "level=2"),
        ('r', "res", "level=2"),
        ('c', "c", "level=2"),
        ('l', "l", "level=2"),
        ('d', "d", "level=1 level=1.5"),
    ] {
        assert!(
            selection(d, base, setters).unwrap_err().is_not_yet_ported(),
            "{base} {setters}"
        );
    }
}

#[test]
fn invalid_selector_values_and_programmatic_cache_errors_are_explicit() {
    for value in ["-1", "-0.1", "99.5", "1e100"] {
        for setters in [format!("level={value}"), format!("level=1 level={value}")] {
            assert!(
                matches!(
                    selection('m', "nmos", &setters),
                    Err(SpiceError::Parse { .. })
                ),
                "{setters}"
            );
        }
    }
    for value in ["NaN", "inf", "1e999", "{expr}"] {
        let mut n = deck("m1 drain gate source bulk mdl\n.model mdl nmos(level=1)");
        n.models[0].parameters[0].value = value.into();
        assert!(
            ModelResolver::new(&n.models)
                .unwrap()
                .resolve(&n.devices[0])
                .is_err()
        );
    }
    let mut n = deck("d1 a 0 mdl\n.model mdl d(level=1)");
    for level in [None, Some(2.0), Some(f64::NAN), Some(f64::INFINITY)] {
        n.models[0].level = level;
        assert!(
            ModelResolver::new(&n.models)
                .unwrap()
                .resolve(&n.devices[0])
                .unwrap_err()
                .to_string()
                .contains("raw level cache")
        );
    }
    n.models[0].name.clear();
    assert!(ModelResolver::new(&n.models).is_err());
}

#[test]
fn diode_fixture_and_context_defaults_are_typed_without_simulation() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/netlists/diode_dc.cir");
    let n = Parser::new().parse_file(fixture).unwrap();
    let resolver = ModelResolver::new(&n.models).unwrap();
    let instance = n.device("d1").unwrap();
    let resolved = resolver.resolve(instance).unwrap().unwrap();
    let context = ModelContext::default();
    let model = resolved.diode_parameters(&context).unwrap();
    let device = resolved
        .diode_instance_parameters(instance, &context)
        .unwrap();
    assert_eq!(model.saturation_current, 1e-14);
    assert_eq!(model.emission_coefficient, 1.0);
    assert_eq!(model.series_resistance, 0.0);
    assert_eq!(model.nominal_temperature_kelvin, 300.15);
    assert_eq!(device.temperature_kelvin, 300.15);
    assert_eq!(device.area, 1.0);
    let context = ModelContext::new(50.0, 20.0);
    assert_eq!(
        resolved
            .diode_parameters(&context)
            .unwrap()
            .nominal_temperature_kelvin,
        293.15
    );
    assert_eq!(
        resolved
            .diode_instance_parameters(instance, &context)
            .unwrap()
            .temperature_kelvin,
        323.15
    );
    assert!(
        Circuit::from_netlist(&n)
            .unwrap()
            .devices()
            .iter()
            .any(|device| device.is_nonlinear())
    );
}

#[test]
fn diode_setters_use_ordered_precedence_and_independent_temperatures() {
    let n = deck(
        "d1 a 0 mdl 2 area=3 temp=50 temp=40\n.model mdl d(is=1e-14 is=2e-14 n=1 n=2 rs=1 rs=3 tnom=20 tnom=30)",
    );
    let before = n.clone();
    let resolved = ModelResolver::new(&n.models)
        .unwrap()
        .resolve(&n.devices[0])
        .unwrap()
        .unwrap();
    let context = ModelContext::new(75.0, 10.0);
    let model = resolved.diode_parameters(&context).unwrap();
    let device = resolved
        .diode_instance_parameters(&n.devices[0], &context)
        .unwrap();
    assert_eq!(
        (
            model.saturation_current,
            model.emission_coefficient,
            model.series_resistance
        ),
        (2e-14, 2.0, 3.0)
    );
    assert_eq!(model.nominal_temperature_kelvin, 303.15);
    assert_eq!(device.temperature_kelvin, 313.15);
    assert_eq!(device.area, 2.0); // INP2D's leading area is the last AST setter.
    assert_eq!(n, before);
}

#[test]
fn diode_schema_rejects_unknown_and_invalid_setters_even_if_overwritten() {
    for setter in [
        "is=0",
        "is=-1",
        "n=0",
        "n=-1",
        "rs=-1",
        "tnom=-273.15",
        "tnom=-300",
        "is=0 is=1e-14",
        "cjo=1p",
        "js=1e-14",
        "temp=30",
    ] {
        let n = deck(&format!("d1 a 0 mdl\n.model mdl d({setter})"));
        let model = ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .unwrap()
            .unwrap();
        assert!(
            model.diode_parameters(&ModelContext::default()).is_err(),
            "{setter}"
        );
        // The legacy IS/N/RS input projection stays deliberately narrow;
        // the diode factory additionally implements CJO charge and C's
        // `dio.c` aliases such as JS for IS.
        if setter == "cjo=1p" || setter == "js=1e-14" {
            assert!(Circuit::from_netlist(&n).is_ok());
        } else {
            assert!(Circuit::from_netlist(&n).is_err());
        }
    }
    for setter in [
        "area=0",
        "area=-1",
        "temp=-273.15",
        "dtemp=1",
        "m=2",
        "ic=1",
        "w=1u",
    ] {
        let n = deck(&format!("d1 a 0 mdl {setter}\n.model mdl d"));
        let model = ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .unwrap()
            .unwrap();
        assert!(
            model
                .diode_instance_parameters(&n.devices[0], &ModelContext::default())
                .is_err(),
            "{setter}"
        );
    }
    for value in ["NaN", "inf", "1e999", "{expr}"] {
        let mut n = deck("d1 a 0 mdl\n.model mdl d(is=1e-14)");
        n.models[0].parameters[0].value = value.into();
        let model = ModelResolver::new(&n.models)
            .unwrap()
            .resolve(&n.devices[0])
            .unwrap()
            .unwrap();
        assert!(model.diode_parameters(&ModelContext::default()).is_err());
    }
    let location = SourceLoc::new(Path::new("temperature.cir").to_path_buf(), 2, 9);
    for value in [f64::NAN, f64::INFINITY, -273.15, -300.0] {
        assert!(temperature_kelvin(value, &location).is_err());
    }
}

#[test]
fn context_and_typed_schema_misuse_fail_explicitly() {
    let n = deck("d1 a 0 mdl temp=40\n.model mdl d(tnom=30)");
    let model = ModelResolver::new(&n.models)
        .unwrap()
        .resolve(&n.devices[0])
        .unwrap()
        .unwrap();
    for bad in [f64::NAN, f64::INFINITY, -273.15] {
        for context in [ModelContext::new(bad, 27.0), ModelContext::new(27.0, bad)] {
            assert!(model.diode_parameters(&context).is_err());
            assert!(
                model
                    .diode_instance_parameters(&n.devices[0], &context)
                    .is_err()
            );
        }
    }
    let mut wrong = n.devices[0].clone();
    wrong.model = Some("different".into());
    assert!(
        model
            .diode_instance_parameters(&wrong, &ModelContext::default())
            .is_err()
    );
    let r = deck("r1 a 0 mdl\n.model mdl r");
    let rmodel = ModelResolver::new(&r.models)
        .unwrap()
        .resolve(&r.devices[0])
        .unwrap()
        .unwrap();
    assert!(rmodel.diode_parameters(&ModelContext::default()).is_err());
    let mut nonfinite = n.devices[0].clone();
    nonfinite.parameters[0].value = "NaN".into();
    assert!(
        model
            .diode_instance_parameters(&nonfinite, &ModelContext::default())
            .is_err()
    );
    let scalar = deck("r1 new 0 1k");
    let mut circuit = Circuit::new();
    let context = ModelContext::new(f64::NAN, 27.0);
    assert!(
        circuit
            .add_instance(
                &scalar.devices[0],
                &ModelResolver::new(&[]).unwrap(),
                &context
            )
            .is_err()
    );
    assert!(circuit.nodes().is_empty());
    assert_eq!(circuit.device_count(), 0);
}

#[test]
fn scalar_schema_extension_retains_units_last_set_sources_and_defaults() {
    let n = deck("d1 a 0 mdl\n.model mdl d(is=1e-14 is=2e-14)");
    let schema = ScalarSchema {
        parameters: &[
            ScalarParameter {
                name: "is",
                unit: ScalarUnit::Ampere,
                domain: ScalarDomain::Positive,
                default: Some(3e-14),
            },
            ScalarParameter {
                name: "rs",
                unit: ScalarUnit::Ohm,
                domain: ScalarDomain::NonNegative,
                default: Some(0.0),
            },
            ScalarParameter {
                name: "optional",
                unit: ScalarUnit::Dimensionless,
                domain: ScalarDomain::Finite,
                default: None,
            },
        ],
    };
    let values = schema
        .validate(&n.models[0].parameters, &n.models[0].location)
        .unwrap();
    assert_eq!(values.get("IS").unwrap().value, 2e-14);
    assert_eq!(values.get("is").unwrap().unit, ScalarUnit::Ampere);
    assert_eq!(
        values.get("is").unwrap().location.as_ref(),
        Some(&n.models[0].parameters[1].location)
    );
    assert!(values.get("rs").unwrap().location.is_none());
    assert!(values.get("optional").is_none());
    let parameter = schema.parameters[0];
    for definitions in [
        vec![parameter, parameter],
        vec![ScalarParameter {
            name: "",
            ..parameter
        }],
        vec![ScalarParameter {
            name: "IS",
            ..parameter
        }],
        vec![ScalarParameter {
            default: Some(f64::NAN),
            ..parameter
        }],
        vec![ScalarParameter {
            default: Some(-1.0),
            ..parameter
        }],
    ] {
        assert!(
            ScalarSchema {
                parameters: &definitions
            }
            .validate([], &n.location)
            .is_err()
        );
    }
}

#[test]
fn unavailable_factories_and_failures_leave_existing_circuit_state_unchanged() {
    let mut circuit = Circuit::from_netlist(&deck("v1 old 0 1\nr1 old 0 1k")).unwrap();
    let nodes = circuit.nodes().clone();
    let count = circuit.device_count();
    let unknowns = circuit.unknown_count();
    let branches: Vec<_> = (0..count).map(|i| circuit.branch_rows(i)).collect();
    let names: Vec<_> = circuit
        .devices()
        .iter()
        .map(|device| device.name().to_owned())
        .collect();
    let topology = circuit.topology().unwrap();
    for body in [
        "d1 new 0 missing",
        "d1 new 0 mdl\n.model mdl r",
        "d1 new 0 mdl\n.model mdl d(is=-1)",
        "d1 new 0 mdl temp=-300\n.model mdl d",
        "d1 new 0 mdl\n.model mdl d(rsw=1)",
        "q1 new base emitter mdl\n.model mdl npn(rco=10)",
        "m1 new gate source bulk mdl\n.model mdl nmos(tox=10n)",
        "m1 new gate source bulk mdl\n.model mdl nmos(level=49)",
        "q1 new base emitter mdl\n.model mdl npn(level=3)",
        "d1 new 0 mdl\n.model mdl d(level=2)",
        "r2 new 0 mdl\n.model mdl r(rsh=100 narrow=10u)",
        "c2 new 0 mdl\n.model mdl c(cap=0)",
        "l2 new 0 mdl\n.model mdl l(ind=0)",
        "r1 new 0 1k",
        "r2 new 0 0",
        "r2 new 0 1k tc1=1",
    ] {
        let n = deck(body);
        let resolver = ModelResolver::new(&n.models).unwrap();
        assert!(
            circuit
                .add_instance(&n.devices[0], &resolver, &ModelContext::default())
                .is_err(),
            "{body}"
        );
        assert_eq!(circuit.nodes().nodes(), nodes.nodes());
        assert_eq!(circuit.device_count(), count);
        assert_eq!(circuit.unknown_count(), unknowns);
        assert_eq!(
            (0..count)
                .map(|i| circuit.branch_rows(i))
                .collect::<Vec<_>>(),
            branches
        );
        assert_eq!(
            circuit
                .devices()
                .iter()
                .map(|device| device.name().to_owned())
                .collect::<Vec<_>>(),
            names
        );
        assert_eq!(
            circuit.topology().unwrap().node_count(),
            topology.node_count()
        );
    }
    for d in ['d', 'q', 'm'] {
        let registry = Registry::with_builtins();
        assert!(matches!(
            registry.get(d).unwrap().support,
            spice_devices::DeviceSupport::Bounded { .. }
        ));
        let n = deck(&format!(
            "{}\n.model mdl {}",
            instance(d),
            if d == 'd' {
                "d"
            } else if d == 'q' {
                "npn"
            } else {
                "nmos"
            }
        ));
        let raw = spice_netlist::RawCard::parse(
            &parse_deck_text(Path::new("raw.cir"), &format!("raw\n{}\n", instance(d))).lines[0],
        )
        .unwrap();
        let mut table = NodeTable::new();
        // The card alone has no model: the registry points at deck
        // elaboration, which builds the device.
        assert!(matches!(
            registry.instantiate(&raw, &mut table).unwrap_err(),
            SpiceError::Unsupported { feature, .. } if feature.contains("from_netlist")
        ));
        assert!(table.is_empty());
        assert!(Circuit::from_netlist(&n).is_ok());
    }
    let n = deck("v2 new 0 2");
    circuit
        .add_instance(
            &n.devices[0],
            &ModelResolver::new(&[]).unwrap(),
            &ModelContext::default(),
        )
        .unwrap();
    circuit.finalize().unwrap();
    assert_eq!(circuit.device_count(), count + 1);
    assert_eq!(circuit.branch_rows(count).unwrap().len(), 1);
}

#[test]
fn unused_models_still_fail_instead_of_disappearing_from_successful_simulation() {
    let n = deck("v1 a 0 1\nr1 a 0 1k\n.model unused d(is=1e-14)");
    assert!(
        Circuit::from_netlist(&n)
            .unwrap_err()
            .to_string()
            .contains("unused model declarations")
    );
}
