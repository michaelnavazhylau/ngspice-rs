//! BSIM4 input plumbing: the generated tables, typed setters and the factory
//! boundary. Setup, temperature and the loads are compared with C in the
//! `c_bsim4_*_reference` tests and `bsim4_device.rs`.
use std::collections::BTreeSet;
use std::path::Path;

use ngspice_rs::devices::bsim4::{
    self, Access, InputType, Inputs, ModelParameters, Value, parameters,
};
use ngspice_rs::devices::{Circuit, ModelContext, ModelResolver};
use ngspice_rs::netlist::ast::{Netlist, ParameterKind};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::SpiceResult;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("bsim4.cir"),
            &format!("BSIM4 inputs\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn inputs(model: &str, instance: &str) -> SpiceResult<(ModelParameters, Inputs)> {
    let netlist = deck(&format!(
        "m1 drain gate source bulk nn {instance}\n.model nn nmos level=14 {model}"
    ));
    let resolver = ModelResolver::new(&netlist.models)?;
    let resolved = resolver.resolve(&netlist.devices[0])?.unwrap();
    let model = bsim4::model_parameters(&resolved)?;
    let instance = model.instance_parameters(&netlist.devices[0], 1.)?;
    Ok((model, instance))
}
fn real(inputs: &Inputs, name: &str) -> f64 {
    match inputs
        .get(name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .value
        .clone()
    {
        Value::Real(value) => value,
        value => panic!("{name} is not real: {value:?}"),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= b.abs() * 1e-14 + 1e-30, "{a:e} != {b:e}");
}

#[test]
fn generated_tables_preserve_every_entry_alias_and_setter_field() {
    assert_eq!(parameters::MODEL.len(), 897);
    assert_eq!(parameters::INSTANCE.len(), 98);
    assert_eq!(
        parameters::INSTANCE
            .iter()
            .filter(|p| p.access != Access::Output)
            .count(),
        44
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
        parameters::INSTANCE
            .iter()
            .filter(|p| p.redundant)
            .map(|p| p.name)
            .collect::<Vec<_>>(),
        ["delvt0"]
    );
    assert_eq!(
        parameters::MODEL
            .iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>()
            .len(),
        893
    );
    assert_eq!(parameters::MODEL[0].name, "cvchargemod");
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
            assert!(p.c_id.starts_with("BSIM4_"));
            if p.access != Access::Output && p.input_type != InputType::RealVector {
                assert!(p.c_field.is_some(), "{}", p.name);
            }
        }
    }
    // `b4.c` uses C's IF_UNINTERESTING macro (`IOPU`) for exactly one row; the
    // flag is recorded rather than dropped.
    assert_eq!(bsim4::uninteresting(), ["INSTANCE:dtemp"]);
    assert_eq!(
        parameters::SOURCE_REVISION,
        "059066b532f7073fccf4173ef269309eafdc7f0c"
    );
}

#[test]
fn literal_defaults_are_the_generated_c_facts() {
    let literals = bsim4::literal_defaults();
    assert_eq!(literals.len(), 777);
    let value = |name: &str| {
        literals
            .iter()
            .find(|(keyword, _)| *keyword == name)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("no literal default for {name}"))
    };
    // Spot checks that pin C's setup order and a few load-bearing defaults.
    assert_eq!(literals[0].0, "mobmod");
    assert_eq!(value("capmod"), 2.0);
    assert_eq!(value("rdsmod"), 0.0);
    assert_eq!(value("epsrox"), 3.9);
    assert_eq!(value("ndep"), 1.7e17);
    assert_eq!(value("ngate"), 0.0);
    // Dependent defaults (`tox` from `toxm`, `vth0` from `vfb`, ...) are not
    // literals: `b4set.c`'s dependent-default code (`set.rs`) computes them.
    for dependent in ["tox", "vth0", "u0", "tnom"] {
        assert!(
            literals.iter().all(|(keyword, _)| *keyword != dependent),
            "{dependent}"
        );
    }
}

#[test]
fn every_model_keyword_is_an_actual_typed_setter() {
    for p in parameters::MODEL {
        let assignment = match p.input_type {
            InputType::Real => format!("{}=0.25", p.name),
            InputType::Integer => format!("{}=1.4", p.name),
            InputType::Flag => p.name.into(),
            InputType::String => "version=4.8".into(),
            InputType::RealVector => panic!("model has no vector setter"),
        };
        let (model, _) = inputs(&assignment, "").unwrap_or_else(|e| panic!("{}: {e}", p.name));
        let value = &model.inputs.get(p.name).unwrap().value;
        match p.input_type {
            InputType::Real => assert!(matches!(value, Value::Real(v) if v.is_finite())),
            InputType::Integer => assert_eq!(*value, Value::Integer(1)),
            InputType::Flag => assert_eq!(*value, Value::Flag(true)),
            InputType::String => assert_eq!(*value, Value::String("4.8".into())),
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
            let netlist = deck("m1 d g s b nn\n.model nn nmos level=14");
            let resolver = ModelResolver::new(&netlist.models).unwrap();
            let model =
                bsim4::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap())
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
        deck("m1 d g s b nn\n.model nn nmos level=54 vth0=0.3 VTHO=0.4 vth0=0.5 pmos nmos pmos");
    let before = netlist.clone();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let model =
        bsim4::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap()).unwrap();
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
        "mobility units need setup conventions"
    );
}

#[test]
fn setter_conversions_follow_c_thresholds_and_integer_rounding() {
    let (model, instance) = inputs(
        "tnom=50 ndep=1e21 nsd=1e24 ngate=1.000002e24 lndep=1e21 lnsd=1.1e24 \
         capmod=2.5 rdsmod=-1.5",
        "min=-1.6",
    )
    .unwrap();
    close(real(&model.inputs, "tnom"), 323.15);
    close(real(&model.inputs, "ndep"), 1e15);
    close(real(&model.inputs, "nsd"), 1e18);
    close(real(&model.inputs, "ngate"), 1.000002e18);
    close(real(&model.inputs, "lndep"), 1e15);
    close(real(&model.inputs, "lnsd"), 1.1e18);
    assert_eq!(model.inputs.get("capmod").unwrap().value, Value::Integer(3));
    assert_eq!(
        model.inputs.get("rdsmod").unwrap().value,
        Value::Integer(-1)
    );
    assert_eq!(instance.get("min").unwrap().value, Value::Integer(-2));
}

#[test]
fn instance_geometry_scales_once_and_areas_left_to_right() {
    let netlist = deck("m1 d g s b nn l=1u w=2u ad=1p as=2p pd=3u ps=4u\n.model nn nmos level=14");
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let model =
        bsim4::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap()).unwrap();
    let instance = model.instance_parameters(&netlist.devices[0], 2.).unwrap();
    close(real(&instance, "l"), 2e-6);
    close(real(&instance, "w"), 4e-6);
    close(real(&instance, "ad"), 4e-12);
    close(real(&instance, "as"), 8e-12);
    close(real(&instance, "pd"), 6e-6);
    close(real(&instance, "ps"), 8e-6);
}

#[test]
fn the_card_takes_four_terminals_as_c_inp2m_does() {
    // C's `INP2M`/`model_numnodes` returns 4 for BSIM4, so neither a
    // three-terminal form nor a fifth node is accepted; the fifth
    // `BSIM4names` entry ("Charge") is C's internal node.
    let netlist = deck("m1 d g s b nn\n.model nn nmos level=14");
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let model =
        bsim4::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap()).unwrap();
    assert!(model.instance_parameters(&netlist.devices[0], 1.).is_ok());
    // The textual grammar rejects the other shapes before the device sees them.
    for nodes in ["d g s", "d g s b q"] {
        assert!(
            Parser::new()
                .parse_deck(&parse_deck_text(
                    Path::new("bsim4.cir"),
                    &format!("BSIM4 inputs\nm1 {nodes} nn\n.model nn nmos level=14\n.end\n"),
                ))
                .is_err(),
            "{nodes}"
        );
    }
}

#[test]
fn version_strings_select_or_refuse_the_right_c_directory() {
    for (version, reference) in [
        ("4.5", "bsim4v5"),
        ("4.6", "bsim4v6"),
        ("4.7", "bsim4v7"),
        ("4.4", "inpdomod.c"),
    ] {
        let netlist = deck(&format!(
            "m1 d g s b nn\n.model nn nmos level=14 version={version}"
        ));
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let error = resolver
            .resolve(&netlist.devices[0])
            .and_then(|resolved| bsim4::model_parameters(&resolved.unwrap()))
            .unwrap_err();
        assert!(error.is_not_yet_ported(), "{version}: {error}");
        assert!(error.to_string().contains(reference), "{version}: {error}");
    }
    for version in ["4.8", "4.8.2"] {
        let netlist = deck(&format!(
            "m1 d g s b nn\n.model nn nmos level=14 version={version}"
        ));
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        assert!(
            bsim4::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap())
                .is_ok(),
            "{version}"
        );
    }
    // Quoted and expression versions are not bare tokens: the card grammar
    // refuses them, as it does for BSIM3.
    assert!(
        Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("bsim4.cir"),
                "BSIM4 inputs\nm1 d g s b nn\n.model nn nmos level=14 version=\"4.8\"\n.end\n",
            ))
            .is_err()
    );
}

#[test]
fn the_factory_builds_the_device_with_cs_internal_nodes() {
    for level in [14, 54] {
        let netlist = deck(&format!(
            "m1 d g s b nn l=1u w=2u\n.model nn nmos level={level} rgatemod=1 rbodymod=1"
        ));
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let mut circuit = Circuit::new();
        circuit
            .add_instance(&netlist.devices[0], &resolver, &ModelContext::default())
            .unwrap();
        assert_eq!(circuit.devices().len(), 1, "level {level}");
        // `b4set.c` creates the gate node, then the body network, with
        // `CKTmkVolt`'s `<instance>#<suffix>` names, after the terminals.
        let names: Vec<&str> = circuit
            .nodes()
            .nodes()
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "0", "d", "g", "s", "b", "m1#gate", "m1#dbody", "m1#body", "m1#sbody"
            ],
            "level {level}"
        );
    }
}

#[test]
fn a_fatal_parameter_check_creates_no_device() {
    // `b4check.c`: a nonpositive TOXE is fatal, and `b4temp.c` returns
    // E_BADPARM before C simulates anything.
    let netlist = deck("m1 d g s b nn l=1u w=2u\n.model nn nmos level=14 toxe=-1e-9");
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let mut circuit = Circuit::new();
    let error = circuit
        .add_instance(&netlist.devices[0], &resolver, &ModelContext::default())
        .unwrap_err();
    assert!(!error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("Toxe"), "{error}");
    assert_eq!(circuit.devices().len(), 0);
    assert_eq!(circuit.nodes().nodes().len(), 1);
}

#[test]
fn an_invalid_setter_is_reported_before_any_setup() {
    let netlist = deck("m1 d g s b nn l=1u\n.model nn nmos level=14 notaparameter=1");
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let mut circuit = Circuit::new();
    let error = circuit
        .add_instance(&netlist.devices[0], &resolver, &ModelContext::default())
        .unwrap_err();
    assert!(!error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("notaparameter"), "{error}");
}
