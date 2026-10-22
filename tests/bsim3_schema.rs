//! BSIM3 B3-0 input plumbing, not a simulation or default/setup claim.
use std::collections::BTreeSet;
use std::path::Path;

use ngspice_rs::devices::bsim3::{
    self, Access, InputType, Inputs, ModelParameters, Value, parameters,
};
use ngspice_rs::devices::{Circuit, ModelContext, ModelResolver};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::ast::{Netlist, ParameterKind};
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::SpiceResult;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("bsim3.cir"),
            &format!("BSIM3 inputs\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn inputs(model: &str, instance: &str) -> SpiceResult<(ModelParameters, Inputs)> {
    let netlist = deck(&format!(
        "m1 drain gate source bulk nn {instance}\n.model nn nmos level=8 {model}"
    ));
    let resolver = ModelResolver::new(&netlist.models)?;
    let resolved = resolver.resolve(&netlist.devices[0])?.unwrap();
    let model = bsim3::model_parameters(&resolved)?;
    let instance = model.instance_parameters(&netlist.devices[0], 1.)?;
    Ok((model, instance))
}
fn real(inputs: &Inputs, name: &str) -> f64 {
    match &inputs
        .get(name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .value
    {
        Value::Real(value) => *value,
        value => panic!("{name} is not real: {value:?}"),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= b.abs() * 1e-14 + 1e-30, "{a:e} != {b:e}");
}

#[test]
fn generated_tables_preserve_every_entry_alias_and_setter_field() {
    assert_eq!(parameters::MODEL.len(), 429);
    assert_eq!(parameters::INSTANCE.len(), 49);
    assert_eq!(
        parameters::INSTANCE
            .iter()
            .filter(|p| p.access != Access::Output)
            .count(),
        19
    );
    assert_eq!(
        parameters::MODEL
            .iter()
            .filter(|p| p.redundant)
            .map(|p| p.name)
            .collect::<Vec<_>>(),
        ["vtho", "lvtho", "wvtho", "pvtho"]
    );
    assert_eq!(
        parameters::MODEL
            .iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>()
            .len(),
        425
    );
    assert_eq!(parameters::MODEL[0].name, "capmod");
    assert_eq!(parameters::MODEL.last().unwrap().name, "pmos");
    for table in [parameters::MODEL, parameters::INSTANCE] {
        assert_eq!(
            table.iter().map(|p| p.name).collect::<BTreeSet<_>>().len(),
            table.len()
        );
        for p in table {
            let canonical = table.iter().find(|q| q.name == p.canonical).unwrap();
            assert_eq!(canonical.id, p.id);
            assert_eq!(canonical.c_field, p.c_field);
            assert!(p.c_id.starts_with("BSIM3_"));
            if p.access != Access::Output && p.input_type != InputType::RealVector {
                assert!(p.c_field.is_some(), "{}", p.name);
            }
        }
    }
    let nch = parameters::MODEL.iter().find(|p| p.name == "nch").unwrap();
    assert_eq!(nch.c_field, Some("BSIM3npeak"));
    assert_eq!(nch.c_given, Some("BSIM3npeakGiven"));
    let off = parameters::INSTANCE
        .iter()
        .find(|p| p.name == "off")
        .unwrap();
    assert_eq!(off.c_field, Some("BSIM3off"));
    assert_eq!(off.c_given, None);
    assert_eq!(
        parameters::SOURCE_REVISION,
        "059066b532f7073fccf4173ef269309eafdc7f0c"
    );
}

#[test]
fn every_model_keyword_is_an_actual_typed_setter() {
    for p in parameters::MODEL {
        let assignment = match p.input_type {
            InputType::Real => format!("{}=0.25", p.name),
            InputType::Integer => format!("{}=1.4", p.name),
            InputType::Flag => p.name.into(),
            InputType::String => "version=3.3.0".into(),
            InputType::RealVector => panic!("model has no vector setter"),
        };
        let (model, _) = inputs(&assignment, "").unwrap_or_else(|e| panic!("{}: {e}", p.name));
        let value = &model.inputs.get(p.name).unwrap().value;
        match p.input_type {
            InputType::Real => assert!(matches!(value, Value::Real(v) if v.is_finite())),
            InputType::Integer => assert_eq!(*value, Value::Integer(1)),
            InputType::Flag => assert_eq!(*value, Value::Flag(true)),
            InputType::String => assert_eq!(*value, Value::String("3.3.0".into())),
            InputType::RealVector => unreachable!(),
        }
    }
}

#[test]
fn instance_keywords_keep_types_and_ask_only_rows_are_not_setters() {
    for p in parameters::INSTANCE {
        let assignment = match p.input_type {
            InputType::Real => format!("{}=0.25", p.name),
            InputType::Integer => format!("{}=1.4", p.name),
            InputType::Flag => p.name.into(),
            InputType::RealVector => "ic=(0.1 0.2 -0.3)".into(),
            InputType::String => unreachable!(),
        };
        if p.access == Access::Output {
            // The textual grammar already refuses ask-only names. The public
            // device API must reject a programmatically constructed AST too.
            let netlist = deck("m1 d g s b nn\n.model nn nmos level=8");
            let resolver = ModelResolver::new(&netlist.models).unwrap();
            let model =
                bsim3::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap())
                    .unwrap();
            let mut device = netlist.devices[0].clone();
            device
                .parameters
                .push(ngspice_rs::netlist::ast::ParameterAssignment {
                    name: p.name.into(),
                    value: "0.25".into(),
                    kind: ParameterKind::Scalar,
                    location: device.location.clone(),
                });
            assert!(
                model
                    .instance_parameters(&device, 1.)
                    .unwrap_err()
                    .to_string()
                    .contains("ask-only"),
                "{}",
                p.name
            );
        } else {
            let (_, values) = inputs("", &assignment).unwrap_or_else(|e| panic!("{}: {e}", p.name));
            if p.input_type == InputType::RealVector {
                close(real(&values, "icvds"), 0.1);
                close(real(&values, "icvgs"), 0.2);
                close(real(&values, "icvbs"), -0.3);
            } else {
                assert!(values.get(p.name).is_some(), "{}", p.name);
            }
        }
    }
}

#[test]
fn aliases_flags_order_and_source_locations_survive_validation() {
    let netlist =
        deck("m1 d g s b nn\n.model nn nmos level=49 vth0=0.3 VTHO=0.4 vth0=0.5 pmos nmos pmos");
    let before = netlist.clone();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let model =
        bsim3::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap()).unwrap();
    assert_eq!(model.polarity, -1);
    assert_eq!(real(&model.inputs, "vth0"), 0.5);
    assert_eq!(real(&model.inputs, "VTHO"), 0.5);
    assert_eq!(
        model
            .inputs
            .applied()
            .iter()
            .map(|p| p.parameter.name)
            .collect::<Vec<_>>(),
        ["vth0", "vtho", "vth0", "pmos", "nmos", "pmos"]
    );
    assert_eq!(
        model.inputs.applied()[1].location,
        netlist.models[0].parameters[2].location
    );
    assert_eq!(netlist, before);
}

#[test]
fn input_projection_does_not_invent_setup_defaults_or_units() {
    let (model, instance) = inputs("", "").unwrap();
    assert_eq!(model.polarity, 1);
    assert!(model.inputs.applied().is_empty());
    assert!(instance.applied().is_empty());
    for name in ["tox", "tnom", "capmod", "u0"] {
        assert!(model.inputs.get(name).is_none());
    }
    let u0 = parameters::MODEL.iter().find(|p| p.name == "u0").unwrap();
    assert_eq!(
        u0.input_unit(),
        None,
        "mobility units depend on setup conventions"
    );
}

#[test]
fn setter_conversions_follow_c_thresholds_and_integer_rounding() {
    let (model, instance) = inputs(
        "tnom=50 nch=1e21 ngate=1.000002e24 lnch=1e21 lngate=1.1e23 capmod=2.5 nqsmod=-1.5",
        "geo=-1.6",
    )
    .unwrap();
    close(real(&model.inputs, "tnom"), 323.15);
    close(real(&model.inputs, "nch"), 1e15);
    close(real(&model.inputs, "ngate"), 1.000002e18);
    close(real(&model.inputs, "lnch"), 1e15);
    close(real(&model.inputs, "lngate"), 1.1e17);
    assert_eq!(model.inputs.get("capmod").unwrap().value, Value::Integer(3));
    assert_eq!(
        model.inputs.get("nqsmod").unwrap().value,
        Value::Integer(-1)
    );
    assert_eq!(instance.get("geo").unwrap().value, Value::Integer(-2));
    // Keep textual literals clear of the branch boundary: INPevaluate uses
    // platform pow(10, exponent), so the spelling 1e23 can be one ulp above
    // the C binary64 threshold on GNU/Linux. Exact setter boundaries are
    // tested separately with binary64 inputs in bsim3::tests.
    let (below, _) = inputs("nch=0.99e20 ngate=1e24 lngate=0.99e23 nsub=1e21", "").unwrap();
    close(real(&below.inputs, "nch"), 0.99e20);
    close(real(&below.inputs, "ngate"), 1e24);
    close(real(&below.inputs, "lngate"), 0.99e23);
    close(real(&below.inputs, "nsub"), 1e21); // NSUB has no setter conversion.
    for assignment in [
        "nqsmod=2147483648",
        "nqsmod=-2147483649",
        "tnom=-273.15",
        "bogus=1",
        "nfactor={missing}",
    ] {
        assert!(inputs(assignment, "").is_err(), "{assignment}");
    }
}

#[test]
fn model_instance_defaults_and_ic_keep_c_precedence_and_scaling_order() {
    let (model, instance) = inputs(
        "l=1u l=2u w=4u m=7",
        "w=5u m=2 ic=(0.1 0.2 -0.3) icvgs=0.4 off",
    )
    .unwrap();
    close(real(&instance, "l"), 1e-6); // create_model prepends: first default wins.
    close(real(&instance, "w"), 5e-6);
    close(real(&instance, "m"), 2.);
    close(real(&instance, "icvgs"), 0.4);
    assert_eq!(instance.get("off").unwrap().value, Value::Flag(true));
    let netlist = deck("m1 d g s b nn ad=1e-200 as=0 pd=1e-200\n.model nn nmos level=8");
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let plain =
        bsim3::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap()).unwrap();
    let scaled = plain
        .instance_parameters(&netlist.devices[0], 1e200)
        .unwrap();
    close(real(&scaled, "ad"), 1e200); // (area * scale) * scale, not area * scale^2.
    assert_eq!(real(&scaled, "as"), 0.);
    assert_eq!(real(&scaled, "pd"), 1.);
    for scale in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(
            model
                .instance_parameters(&netlist.devices[0], scale)
                .is_err()
        );
    }
    let (_, no_m) = inputs("m=7", "").unwrap();
    assert!(
        no_m.get("m").is_none(),
        "model multiplier is discarded, not a default"
    );
}

#[test]
fn version_selection_is_textual_first_wins_and_round_trips() {
    for level in [8, 49] {
        for version in ["3.3", "3.30", "3.3.0", "3.3.1"] {
            let netlist = deck(&format!(
                "m1 d g s b nn\n.model nn nmos level={level} version={version} version=3.2.4"
            ));
            let resolver = ModelResolver::new(&netlist.models).unwrap();
            let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
            assert_eq!(bsim3::select_version(resolved.card()).unwrap(), version);
            let model = bsim3::model_parameters(&resolved).unwrap();
            assert_eq!(
                model.inputs.get("version").unwrap().value,
                Value::String("3.2.4".into())
            );
            let written = ngspice_rs::netlist::writer::write_netlist(&netlist).unwrap();
            let reparsed = Parser::new()
                .parse_deck(&parse_deck_text(Path::new("written.cir"), &written))
                .unwrap();
            assert_eq!(
                reparsed.models[0]
                    .parameters
                    .iter()
                    .map(|p| (&p.name, &p.value, &p.kind))
                    .collect::<Vec<_>>(),
                netlist.models[0]
                    .parameters
                    .iter()
                    .map(|p| (&p.name, &p.value, &p.kind))
                    .collect::<Vec<_>>()
            );
        }
    }
    for (version, directory) in [
        ("3.0", "bsim3v0/"),
        ("3.1", "bsim3v1/"),
        ("3.2.4", "bsim3v32/"),
    ] {
        let error = inputs(&format!("version={version} version=3.3.0"), "").unwrap_err();
        assert!(error.is_not_yet_ported(), "{error}");
        assert!(error.to_string().contains(directory), "{error}");
    }
    let dotted = deck(".model nn nmos level=8 version=3.3.0");
    assert_eq!(dotted.models[0].parameters[1].kind, ParameterKind::Textual);
    for value in ["", "\"3.3.0\"", "{v}", "'3.3.0'"] {
        let result = Parser::new().parse_deck(&parse_deck_text(
            Path::new("bad-version.cir"),
            &format!("bad\n.model nn nmos level=8 version={value}\n.end\n"),
        ));
        assert!(result.is_err(), "version={value}");
    }
}

#[test]
fn pdk_shaped_cards_reach_the_schema_after_real_bin_selection() {
    let text = include_str!("../conformance/parser/bsim3_schema.cir");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("bsim3_schema.cir"), text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    for (name, selected, polarity) in [("m1", "nch.1", 1), ("m2", "pcore", -1)] {
        let instance = netlist.device(name).unwrap();
        let resolved = resolver.resolve(instance).unwrap().unwrap();
        assert_eq!(resolved.card().name, selected);
        let model = bsim3::model_parameters(&resolved).unwrap();
        assert_eq!(model.polarity, polarity);
        let values = model.instance_parameters(instance, 1.).unwrap();
        assert!(values.get("l").is_some());
        close(real(&model.inputs, "tox"), 4.1e-9);
    }
    let written = ngspice_rs::netlist::writer::write_netlist(&netlist).unwrap();
    let reparsed = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("copy.cir"), &written))
        .unwrap();
    assert_eq!(
        ngspice_rs::netlist::writer::write_netlist(&reparsed).unwrap(),
        written
    );
    // Both binned PDK-shaped instances build a DC device (RSH creates the
    // internal primes) instead of failing the factory.
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    assert_eq!(circuit.devices().len(), netlist.devices.len());
}

#[test]
fn validated_levels_build_a_real_device_with_charge_state() {
    for level in [8, 49] {
        let netlist = deck(&format!(
            "m1 newd newg news newb nn l=1u w=2u\n.model nn nmos level={level} version=3.3.0 tox=4n"
        ));
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let mut circuit = Circuit::new();
        circuit
            .add_instance(&netlist.devices[0], &resolver, &ModelContext::default())
            .unwrap();
        assert_eq!(circuit.devices().len(), 1);
        // Ground plus the four external nodes.
        assert_eq!(circuit.nodes().len(), 5);
        // C's `BSIM3numStates` (`bsim3def.h`).
        assert_eq!(circuit.devices()[0].state_count(), 17);
        // Unknowns are assigned when the circuit is finalized.
        assert_eq!(circuit.devices()[0].terminals().len(), 4);
    }
}
