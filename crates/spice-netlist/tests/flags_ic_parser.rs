//! Exhaustive initial-engine flag/IC grammar table from dio/bjt/mos1 setters.
//! Accepted syntax does not enable initialization or nonlinear simulation.
use spice_core::SpiceError;
use spice_netlist::{
    Parser,
    ast::{Netlist, ParameterKind},
    source::parse_deck_text,
};
use std::path::Path;

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("flags.cir"),
        &format!("Title\n{body}"),
    ))
}
fn transistor(designator: char, setters: &str) -> String {
    match designator {
        'q' => format!("Q1 c b e mdl {setters}\n.model mdl npn\n"),
        _ => format!("M1 d g s b mdl {setters}\n.model mdl nmos\n"),
    }
}

#[test]
fn supported_instance_flags_are_bare_ordered_and_case_insensitive() {
    for body in [
        "D1 a 0 mdl 2 OFF area=7 off\n.model mdl d",
        "Q1 c b e mdl 2 OFF area=7 off\n.model mdl npn",
        "M1 d g s b mdl OFF w=1u off\n.model mdl nmos",
    ] {
        let netlist = parse(body).unwrap();
        let p = &netlist.devices[0].parameters;
        assert_eq!(p[0].name, "off");
        assert_eq!(p[0].value, "");
        assert_eq!(p[0].kind, ParameterKind::Flag);
        assert_eq!(p[2].name, "off");
        assert_eq!(p[2].kind, ParameterKind::Flag);
        if netlist.devices[0].designator != 'm' {
            assert_eq!((p[3].name.as_str(), p[3].value.as_str()), ("area", "2"));
            assert_eq!(p[3].kind, ParameterKind::Scalar);
        }
    }
}

#[test]
fn model_type_tokens_and_true_tail_flags_are_distinct() {
    for (base, flags) in [
        ("d", vec!["d"]),
        ("npn", vec!["npn", "pnp"]),
        ("pnp", vec!["npn", "pnp"]),
        ("nmos", vec!["nmos", "pmos"]),
        ("pmos", vec!["nmos", "pmos"]),
    ] {
        let netlist = parse(&format!(".model mdl {base}")).unwrap();
        assert!(
            netlist.models[0].parameters.is_empty(),
            "base is not a setter"
        );
        for flag in flags {
            for value in [
                format!("({flag} is=1e-14 {flag})"),
                format!("{flag} is=1e-14 {flag}"),
            ] {
                let netlist =
                    parse(&format!(".MODEL MDL {} {value}", base.to_ascii_uppercase())).unwrap();
                let model = &netlist.models[0];
                assert_eq!(model.base, base);
                assert_eq!(model.parameters.len(), 3);
                assert_eq!(model.parameters[0].name, flag);
                assert_eq!(model.parameters[0].kind, ParameterKind::Flag);
                assert_eq!(model.parameters[1].kind, ParameterKind::Scalar);
                assert_eq!(model.parameters[2].kind, ParameterKind::Flag);
            }
        }
    }
}

#[test]
fn initial_condition_arities_and_component_order_follow_c_setters() {
    for (designator, names, values) in [
        ('q', vec!["icvbe", "icvce"], vec![".6", "2V"]),
        (
            'm',
            vec!["icvds", "icvgs", "icvbs"],
            vec![".1", "2V", "-.2"],
        ),
    ] {
        for count in 1..=names.len() {
            for raw in [
                values[..count].join(","),
                values[..count].join(" "),
                format!("({})", values[..count].join(", ")),
            ] {
                for prefix in ["IC=", "ic "] {
                    let netlist =
                        parse(&transistor(designator, &format!("{prefix}{raw} OFF"))).unwrap();
                    let p = &netlist.devices[0].parameters;
                    assert_eq!(p.len(), 2);
                    assert_eq!(p[0].name, "ic");
                    assert_eq!(p[0].value, raw);
                    let ParameterKind::InitialConditions(components) = &p[0].kind else {
                        panic!("IC vector")
                    };
                    assert_eq!(components.len(), count);
                    for (index, component) in components.iter().enumerate() {
                        assert_eq!(component.name, names[index]);
                        assert_eq!(component.value.text, values[index]);
                        assert_eq!(component.value.location.line, 2);
                    }
                    assert_eq!(p[1].kind, ParameterKind::Flag);
                }
            }
        }
    }
}

#[test]
fn vector_and_scalar_ic_duplicates_remain_interleaved() {
    let netlist = parse(&transistor(
        'q',
        "3 icvbe=.1 IC=(.6,2) icvce=4 ic=.7 area=9 off",
    ))
    .unwrap();
    let p = &netlist.devices[0].parameters;
    assert_eq!(
        p.iter()
            .map(|p| (p.name.as_str(), p.value.as_str()))
            .collect::<Vec<_>>(),
        [
            ("icvbe", ".1"),
            ("ic", "(.6,2)"),
            ("icvce", "4"),
            ("ic", ".7"),
            ("area", "9"),
            ("off", ""),
            ("area", "3")
        ]
    );
    let netlist = parse(&transistor('m', "icvgs=.1 IC=(1 2 3) icvbs=4 ic=5,6 off")).unwrap();
    let p = &netlist.devices[0].parameters;
    assert_eq!(
        p.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["icvgs", "ic", "icvbs", "ic", "off"]
    );
}

#[test]
fn diode_ic_remains_scalar_not_a_vector() {
    let netlist = parse("D1 a 0 dm 2 off IC=.4 ic .5 area=7").unwrap();
    let p = &netlist.devices[0].parameters;
    assert_eq!(p[1].kind, ParameterKind::Scalar);
    assert_eq!(p[2].kind, ParameterKind::Scalar);
    assert_eq!(p[4].value, "2");
    for body in ["D1 a 0 dm ic=(.4)", "D1 a 0 dm ic=.4,.5"] {
        assert!(
            matches!(parse(body), Err(SpiceError::Parse { .. })),
            "{body}"
        );
    }
}

#[test]
fn invalid_flag_forms_and_ic_vectors_are_committed_errors() {
    for setters in [
        "off=0",
        "off=1",
        "off 1",
        "off={x}",
        "off '1'",
        "ic=",
        "ic=()",
        "ic=(,1)",
        "ic=1,",
        "ic=1,,2",
        "ic=(1,2,)",
        "ic=(1 2",
        "ic=(1))",
        "ic=((1))",
        "ic==1",
        "ic=1e999",
        "ic=1 1e999",
    ] {
        for designator in ['q', 'm'] {
            let error = parse(&transistor(designator, setters)).unwrap_err();
            // Expressions are intentionally unsupported, not malformed scalars.
            assert!(
                matches!(error,SpiceError::Parse{ref location,..} if location.line==2),
                "{setters}: {error}"
            );
        }
    }
    for body in [
        transistor('q', "ic=1,2,3"),
        transistor('q', "ic=(1 2 3)"),
        transistor('m', "ic=(1 2 3 4)"),
        transistor('m', "ic=1 2 3 4"),
        ".model dm d(d=1)".into(),
        ".model qm npn(pnp 1)".into(),
        ".model nm nmos(nmos=0)".into(),
    ] {
        assert!(
            matches!(parse(&body), Err(SpiceError::Parse { .. })),
            "{body}"
        );
    }
}

#[test]
fn flag_inventory_keeps_unavailable_or_wrong_family_keywords_explicit() {
    for (body, reference) in [
        ("D1 a 0 mdl thermal", "inp2d.c"),
        ("D1 a 0 mdl sens_area", "inp2d.c"),
        ("Q1 c b e mdl sens_area\n.model mdl npn", "inp2q.c"),
        ("M1 d g s b mdl sens_l\n.model mdl nmos", "inp2m.c"),
        ("M1 d g s b mdl sens_w\n.model mdl nmos", "inp2m.c"),
        (".model dm d(off)", "inpdomod.c"),
        (".model dm d(nmos)", "inpdomod.c"),
        (".model qm npn(d)", "inpdomod.c"),
        (".model nm nmos(npn)", "inpdomod.c"),
        (".model rm r(d)", "inpdomod.c"),
        (".model cm c(nmos)", "inpdomod.c"),
        (".model lm l(pnp)", "inpdomod.c"),
        (".model nm nmos(nchan)", "inpdomod.c"),
        (".model nm pmos(pchan)", "inpdomod.c"),
        ("Q1 c b e mdl ic={vbe}\n.model mdl npn", "inp2q.c"),
        ("M1 d g s b mdl ic=1,{vgs}\n.model mdl nmos", "inp2m.c"),
    ] {
        let error = parse(body).unwrap_err();
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains(reference), "{error}");
    }
}

#[test]
fn continuations_flags_and_components_keep_byte_positions() {
    let netlist = parse("Q1 α b e qm 2 OFF\n+ ic=(.6, 2) icvce=3\n.model qm npn\n").unwrap();
    let p = &netlist.devices[0].parameters;
    assert_eq!(p[0].location.column, 16);
    assert_eq!(p[1].location.column, 20);
    assert_eq!(p[1].location.line, 2);
    let ParameterKind::InitialConditions(values) = &p[1].kind else {
        panic!("IC")
    };
    assert_eq!(values[0].value.location.column, 24);
    assert_eq!(values[1].value.location.column, 28);
    assert_eq!(values[1].value.location.line, 2);
    assert_eq!(p.last().unwrap().location.column, 14);
    let text = "Q1 α b e qm ic=(.6, 1e999)\n.model qm npn";
    let error = parse(text).unwrap_err();
    assert!(
        matches!(error,SpiceError::Parse{location,..} if location.column as usize==text.find("1e999").unwrap()+1)
    );
}

#[test]
fn end_and_first_error_rules_survive_new_optional_branches() {
    let netlist = parse("Q1 c b e qm off ic=.6,2\n.model qm npn\n.end\nQ2 broken {\n").unwrap();
    assert_eq!(netlist.devices.len(), 1);
    let error = parse("Q1 c b e qm ic=(.6,)\n.model qm npn\n.model broken d(is={").unwrap_err();
    assert!(matches!(error,SpiceError::Parse{location,..} if location.line==2));
}
