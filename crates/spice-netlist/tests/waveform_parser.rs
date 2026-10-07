//! GitHub #8: numeric source syntax only, not time evaluation/solver parity.
use std::path::Path;

use spice_core::{AnalysisKind, SpiceError};
use spice_netlist::{
    Parser,
    ast::{Netlist, ParameterKind, SourceWaveform},
    source::parse_deck_text,
};

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("wave.cir"),
        &format!("Title\n{body}"),
    ))
}

#[test]
fn transient_fixture_has_positioned_pulse_and_unvalidated_request() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/netlists/rc_transient.cir");
    let netlist = Parser::new().parse_file(path).unwrap();
    assert_eq!(netlist.devices.len(), 3);
    let source = netlist.device("v1").unwrap();
    assert_eq!(source.nodes, ["in", "0"]);
    let setter = &source.parameters[0];
    assert_eq!(setter.name, "pulse");
    assert_eq!(setter.location.line, 2);
    let ParameterKind::Waveform(SourceWaveform::Pulse(pulse)) = &setter.kind else {
        panic!("PULSE AST")
    };
    assert_eq!(pulse.initial.text, "0");
    assert_eq!(pulse.pulsed.text, "5");
    assert!(pulse.period.is_some());
    assert_eq!(netlist.analyses[0].kind, AnalysisKind::Transient);
    assert_eq!(netlist.analyses[0].arguments, ["0.5u", "5u"]);
}

#[test]
fn all_pulse_optional_fields_remain_omitted_or_positioned() {
    let fields = ["0", "5V", "1n", "2n", "3n", "4u", "5u"];
    for count in 2..=7 {
        for spelling in [
            fields[..count].join(" "),
            format!("({})", fields[..count].join(", ")),
        ] {
            let netlist = parse(&format!("V1 a 0 PULSE {spelling}\n")).unwrap();
            let setter = &netlist.devices[0].parameters[0];
            assert_eq!(setter.value, spelling);
            let ParameterKind::Waveform(SourceWaveform::Pulse(pulse)) = &setter.kind else {
                panic!("PULSE AST")
            };
            assert_eq!(pulse.initial.text, fields[0]);
            assert_eq!(pulse.pulsed.text, fields[1]);
            let timing = [
                &pulse.delay,
                &pulse.rise,
                &pulse.fall,
                &pulse.width,
                &pulse.period,
            ];
            for (index, field) in timing.iter().enumerate() {
                assert_eq!(
                    field.as_ref().map(|v| v.text.as_str()),
                    fields.get(index + 2).filter(|_| index + 2 < count).copied()
                );
            }
        }
    }
}

#[test]
fn mixed_source_setters_keep_duplicates_and_leading_dc_last() {
    let netlist = parse("V1 a 0 7 AC PULSE(0 5) DC=2 PWL(0,1,2u,3) AC=2 90 PULSE(1 4) dc 3\nI1 a 0 DC 2m PWL=0 1m 2u -3m AC\n").unwrap();
    let p = &netlist.devices[0].parameters;
    assert_eq!(
        p.iter()
            .map(|p| (p.name.as_str(), p.value.as_str()))
            .collect::<Vec<_>>(),
        [
            ("acmag", "1"),
            ("acphase", "0"),
            ("pulse", "(0 5)"),
            ("dc", "2"),
            ("pwl", "(0,1,2u,3)"),
            ("acmag", "2"),
            ("acphase", "90"),
            ("pulse", "(1 4)"),
            ("dc", "3"),
            ("dc", "7")
        ]
    );
    assert_eq!(p[9].kind, ParameterKind::Scalar);
    let p = &netlist.devices[1].parameters;
    assert_eq!(
        p.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["dc", "pwl", "acmag", "acphase"]
    );
    let ParameterKind::Waveform(SourceWaveform::Pwl(points)) = &p[1].kind else {
        panic!("PWL AST")
    };
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].value.text, "1m");
    assert_eq!(points[1].time.text, "2u");
    assert_eq!(points[1].value.text, "-3m");
}

#[test]
fn syntax_does_not_sort_validate_or_repair_pwl_times() {
    let netlist = parse("I1 a 0 pwl(-1 2 0 3 0 4 -2 5)\nV1 a 0 pwl(0 1)\n").unwrap();
    let ParameterKind::Waveform(SourceWaveform::Pwl(points)) =
        &netlist.devices[0].parameters[0].kind
    else {
        panic!("PWL AST")
    };
    assert_eq!(
        points
            .iter()
            .map(|p| p.time.text.as_str())
            .collect::<Vec<_>>(),
        ["-1", "0", "0", "-2"]
    );
    assert_eq!(netlist.devices[1].parameters.len(), 1);
}

#[test]
fn continuations_and_unicode_nodes_keep_byte_locations_and_raw_spelling() {
    let netlist = parse("V1 α GND PULSE(0 5V\n+ 1n, 2n)\n").unwrap();
    let setter = &netlist.devices[0].parameters[0];
    assert_eq!(setter.location.line, 2);
    assert_eq!(setter.location.column, 11);
    assert_eq!(setter.value, "(0 5V 1n, 2n)");
    let ParameterKind::Waveform(SourceWaveform::Pulse(pulse)) = &setter.kind else {
        panic!("PULSE AST")
    };
    assert_eq!(pulse.initial.location.column, 17);
    assert_eq!(pulse.delay.as_ref().unwrap().location.line, 2);
    assert_eq!(pulse.delay.as_ref().unwrap().location.column, 22);
    assert_eq!(netlist.devices[0].nodes, ["α", "0"]);
    for (body, bad) in [
        ("V1 α 0 pulse(0 1e999)", "1e999"),
        ("V1 α 0 pulse(0 1) dc=1e999", "1e999"),
        ("I1 α 0 pwl(0 1 2 1e999)", "1e999"),
        ("V1 α 0 pulse(0 1,,2)", ",,2"),
    ] {
        let expected = body.find(bad).unwrap() + 1 + usize::from(bad == ",,2");
        assert!(
            matches!(parse(body), Err(SpiceError::Parse { location, .. })
            if location.column as usize == expected),
            "{body}"
        );
    }
}

#[test]
fn malformed_waveforms_are_committed_parse_errors() {
    for body in [
        "pulse",
        "pulse()",
        "pulse(0)",
        "pulse 0",
        "pulse=",
        "pulse==0 1",
        "pulse(0 1",
        "pulse(0 1))",
        "pulse((0 1))",
        "pulse(0 1,)",
        "pulse(,0 1)",
        "pulse(0,,1)",
        "pulse(0 1 2 3 4 5 6 7)",
        "pulse 0 1 2 3 4 5 6 7",
        "pulse(0 1e999)",
        "pulse 0 1e999",
        "pulse(0 1 1e999)",
        "pwl(0 1 2)",
        "pwl 0 1 2",
        "pwl(0)",
        "pwl(0,)",
        "pwl(0 1 2 1e999)",
        "pwl 0 1 2 1e999",
        "pwl(0 1) =2",
    ] {
        let error = parse(&format!("V1 a 0 {body}\n")).unwrap_err();
        assert!(
            matches!(error, SpiceError::Parse{ ref location,..} if location.line==2 && location.path()==Path::new("wave.cir")),
            "{body}: {error}"
        );
    }
}

#[test]
fn unsupported_waveform_extensions_never_partially_succeed() {
    for body in [
        "pulse({low} 1)",
        "pulse(0 {high})",
        "pulse(0 '1')",
        "pwl(0 1) r=0",
        "pwl(0 1) td=1u",
        "pwl file=\"values.txt\"",
        "pwl(0 {value})",
        "sin(0 1)",
        "exp(0 1)",
        "sffm(0 1)",
        "am(0 1)",
        "trnoise(0 1)",
        "pulse(0 1) junk",
    ] {
        let error = parse(&format!("I1 a 0 {body}\n")).unwrap_err();
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        assert!(error.to_string().contains("inp2i.c"), "{error}");
    }
}

#[test]
fn vector_work_limit_is_explicit() {
    let fields = "0 1 ".repeat(2048);
    let netlist = parse(&format!("V1 a 0 pwl({fields})")).unwrap();
    let ParameterKind::Waveform(SourceWaveform::Pwl(points)) =
        &netlist.devices[0].parameters[0].kind
    else {
        panic!("PWL AST")
    };
    assert_eq!(points.len(), 2048);
    let fields = "0 1 ".repeat(2049);
    assert!(matches!(
        parse(&format!("V1 a 0 pwl({fields})")),
        Err(SpiceError::Parse { .. })
    ));
}
