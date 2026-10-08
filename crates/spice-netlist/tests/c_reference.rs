//! Opt-in live syntax oracles. Compare parsed scalar parameters, not Rust
//! simulation results, with C's `print @instance[parameter]` / model queries.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use spice_core::{NodeTable, parse_spice_number};
use spice_netlist::{
    Parser,
    ast::{Netlist, ParameterAssignment, ParameterKind},
};

const LINEAR: &str = include_str!("../../../conformance/parser/linear_sources.cir");
const DIODES: &str = include_str!("../../../conformance/parser/model_diodes.cir");
const TRANSISTORS: &str = include_str!("../../../conformance/parser/transistor_scalars.cir");
const PASSIVES: &str = include_str!("../../../conformance/parser/passive_models.cir");
const FLAGS_IC: &str = include_str!("../../../conformance/parser/flags_ic.cir");
const WAVEFORMS: &str = include_str!("../../../conformance/parser/source_waveforms.cir");
const FUNCTIONS: &str = include_str!("../../../conformance/parser/source_functions.cir");
const CONTROLLED: &str = include_str!("../../../conformance/parser/controlled_sources.cir");

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn parse(text: &str) -> Netlist {
    let deck = spice_netlist::source::parse_deck_text(Path::new("reference.cir"), text);
    Parser::new().parse_deck(&deck).expect("Rust parses probe")
}

fn assignments(
    expected: &mut BTreeMap<String, f64>,
    name: &str,
    parameters: &[ParameterAssignment],
) {
    for parameter in parameters {
        // Ordered assignment implements C's last-set value, including leading
        // source/diode precedence. Numeric syntax uses the core parser.
        expected.insert(
            format!("@{name}[{}]", parameter.name),
            parse_spice_number(&parameter.value).expect("scalar literal"),
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_scalars_match_live_c_instance_parameters() {
    let netlist = parse(LINEAR);
    let mut expected = BTreeMap::new();
    for device in &netlist.devices {
        assignments(&mut expected, &device.name, &device.parameters);
    }
    // An omitted source specification has implicit DC zero in C.
    assert!(netlist.device("i2").unwrap().parameters.is_empty());
    expected.insert("@i2[dc]".to_owned(), 0.0);
    assert_reference("linear", LINEAR, expected);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_controlled_source_setters_match_live_c_gains() {
    let netlist = parse(CONTROLLED);
    let mut expected = BTreeMap::new();
    for device in &netlist.devices {
        if !matches!(device.designator, 'e' | 'f' | 'g' | 'h') {
            continue;
        }
        // VCCSparam/CCCSparam: a gain is scaled by an m given *before* it.
        let (mut gain, mut multiplier) = (None, None::<f64>);
        for parameter in &device.parameters {
            match (&parameter.kind, parameter.name.as_str()) {
                (ParameterKind::Instance, "control") => {}
                (ParameterKind::Scalar, "gain") => {
                    let value = parse_spice_number(&parameter.value).unwrap();
                    gain = Some(value * multiplier.unwrap_or(1.0));
                }
                (ParameterKind::Scalar, "m") => {
                    multiplier = parse_spice_number(&parameter.value);
                }
                // e3's {2*5} is evaluated by numparam in C; literal here.
                (ParameterKind::Expression(_), "gain") => gain = Some(10.0),
                other => panic!("unexpected setter {other:?}"),
            }
        }
        expected.insert(format!("@{}[gain]", device.name), gain.unwrap());
    }
    // Independent hand-checked setter-order expectations.
    assert_eq!(expected["@g2[gain]"], 6e-3);
    assert_eq!(expected["@g3[gain]"], 2e-3);
    assert_eq!(expected["@f2[gain]"], 2.0);
    assert_eq!(expected.len(), 13);
    assert_reference("controlled", CONTROLLED, expected);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_model_and_diode_scalars_match_live_c_parameters() {
    let netlist = parse(DIODES);
    let mut expected = BTreeMap::new();
    for device in &netlist.devices {
        assignments(&mut expected, &device.name, &device.parameters);
    }
    for model in &netlist.models {
        assignments(&mut expected, &model.name, &model.parameters);
    }
    // Assert the probes pin the precedence/default contracts independently.
    assert_eq!(expected["@dlead[area]"], 2.0);
    assert_eq!(expected["@dlead[pj]"], 3.0);
    assert_eq!(expected["@dm[is]"], 2e-14);
    assert!(netlist.device("dimplicit").unwrap().parameters.is_empty());
    expected.insert("@dimplicit[area]".to_owned(), 1.0);
    assert_reference("diodes", DIODES, expected);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_transistor_scalars_and_terminal_bindings_match_live_c() {
    let netlist = parse(TRANSISTORS);
    let mut expected = BTreeMap::new();
    let mut nodes = NodeTable::new();
    // The probe introduces external nodes before setup creates internal ones.
    // Compare C's external terminal IDs with parsed terminal order, not MNA.
    for device in &netlist.devices {
        for node in &device.nodes {
            nodes.intern(node);
        }
        assignments(&mut expected, &device.name, &device.parameters);
    }
    for model in &netlist.models {
        assignments(&mut expected, &model.name, &model.parameters);
    }
    assert_eq!(expected["@qthree[area]"], 2.0);
    assert_eq!(expected["@qkeyword[area]"], 2.0);
    assert_eq!(expected["@qdigits[area]"], 3.0);
    assert_eq!(expected["@m1[w]"], parse_spice_number("20u").unwrap());
    for device in &netlist.devices {
        let queries: &[&str] = match device.designator {
            'q' => &["colnode", "basenode", "emitnode", "substnode"],
            'm' => &["dnode", "gnode", "snode", "bnode"],
            _ => continue,
        };
        for (parameter, node) in queries.iter().zip(
            device
                .nodes
                .iter()
                .map(String::as_str)
                .chain(std::iter::once("0")),
        ) {
            // INP2Q grounds an omitted substrate. The AST keeps it omitted;
            // the engine will need to apply this rule during elaboration.
            let id = u32::try_from(nodes.intern(node).index()).expect("bounded probe");
            expected.insert(format!("@{}[{parameter}]", device.name), f64::from(id));
        }
    }
    assert_reference("transistors", TRANSISTORS, expected);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_model_backed_passives_match_c_setup_and_setter_order() {
    let netlist = parse(PASSIVES);
    let mut expected = BTreeMap::new();
    for device in &netlist.devices {
        assignments(&mut expected, &device.name, &device.parameters);
    }
    assert_eq!(expected["@rpre[resistance]"], 3e3);
    assert_eq!(expected["@rpost[resistance]"], 4e3);
    assert_eq!(expected["@rboth[resistance]"], 8e3);
    assert_eq!(expected["@cpost[capacitance]"], 4e-6);
    assert_eq!(expected["@lpost[inductance]"], 4e-3);
    assert_eq!(expected["@rnumeric[resistance]"], 123.0);
    assert_eq!(
        netlist.device("rnumref").unwrap().model.as_deref(),
        Some("rm123")
    );
    assert_eq!(
        netlist.device("rkeyword").unwrap().model.as_deref(),
        Some("tc1")
    );
    assert!(netlist.device("rassigned").unwrap().model.is_none());
    // Independent hand-checked C setup values, not Rust geometry arithmetic.
    // RES: rsh*l/w; CAP: cj*l*w. Omitted scalars stay omitted in the AST.
    expected.insert("@rgeom[resistance]".into(), 200.0);
    expected.insert("@cgeom[capacitance]".into(), 8e-15);
    assert_reference("passive-models", PASSIVES, expected);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn numeric_bjt_model_names_are_not_silently_accepted() {
    for model in ["123", "123n"] {
        let text = format!(
            "Numeric BJT model rejection\nv1 c 0 dc 2\nv2 b 0 dc .1\nq1 c b 0 {model} 3\n.model {model} npn\n.op\n.end\n"
        );
        let deck = spice_netlist::source::parse_deck_text(Path::new("numeric.cir"), &text);
        let error = Parser::new()
            .parse_deck(&deck)
            .expect_err("unsupported model identifier");
        if model == "123" {
            assert!(matches!(error, spice_core::SpiceError::Parse { .. }));
        } else {
            assert!(error.is_not_yet_ported());
        }
        let output = reference_output(&format!("numeric-{model}"), &text);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "C unexpectedly accepted {model}");
        assert!(
            stderr.contains("could not find a valid modelname"),
            "{model}: {stderr}"
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_flags_and_ic_vectors_match_live_c_setter_order() {
    let netlist = parse(FLAGS_IC);
    let mut expected = BTreeMap::new();
    for device in &netlist.devices {
        for parameter in &device.parameters {
            match &parameter.kind {
                ParameterKind::Scalar => {
                    assignments(&mut expected, &device.name, std::slice::from_ref(parameter))
                }
                ParameterKind::Flag => {
                    // MOS1 OFF is input-only (IP, not IOP) despite an ask
                    // switch; the frontend cannot expose @m[off]. C setup
                    // checks its syntax; D/Q OFF can be queried directly.
                    if device.designator != 'm' {
                        expected.insert(format!("@{}[{}]", device.name, parameter.name), 1.0);
                    }
                }
                ParameterKind::InitialConditions(values) => {
                    for component in values {
                        expected.insert(
                            format!("@{}[{}]", device.name, component.name),
                            parse_spice_number(&component.value.text).unwrap(),
                        );
                    }
                }
                ParameterKind::Waveform(_)
                | ParameterKind::Textual
                | ParameterKind::Instance
                | ParameterKind::Expression(_) => panic!("not a scalar probe"),
            }
        }
    }
    // Independent expectations pin fallthrough/omission and precedence.
    assert_eq!(expected["@dlead[area]"], 2.0);
    assert_eq!(expected["@qfull[area]"], 2.0);
    assert_eq!(expected["@qfull[icvbe]"], 0.7);
    assert_eq!(expected["@qfull[icvce]"], 4.0);
    assert_eq!(expected["@qpartial[icvce]"], 5.0);
    assert_eq!(expected["@mfull[icvgs]"], 4.0);
    assert_eq!(expected["@mpartial[icvds]"], 0.4);
    assert_eq!(expected["@mpartial[icvgs]"], 6.0);
    assert_eq!(expected["@mpartial[icvbs]"], -0.3);
    // Model type flags are input-only; successful C setup checks their syntax,
    // not a fictitious scalar query or Rust model polarity implementation.
    assert_reference("flags-ic", FLAGS_IC, expected);
}

/// Expected `@dev[function]`, coefficient vectors and queryable scalars of
/// every source's last waveform setter, plus the `let` commands that read C's
/// coefficient vector. PWL `r=`/`td=` are input-only (`IP`) in C and cannot be
/// queried; they are checked by the transient comparisons instead.
fn waveform_expectations(netlist: &Netlist) -> (BTreeMap<String, f64>, String) {
    use spice_netlist::ast::{SourceFunction, SourceWaveform};
    let mut expected = BTreeMap::new();
    let mut commands = String::new();
    for device in &netlist.devices {
        for parameter in device
            .parameters
            .iter()
            .filter(|p| p.kind == ParameterKind::Scalar && !matches!(p.name.as_str(), "r" | "td"))
        {
            assignments(&mut expected, &device.name, std::slice::from_ref(parameter));
        }
        let Some(waveform) = device.parameters.iter().rev().find_map(|p| match &p.kind {
            ParameterKind::Waveform(w) => Some(w),
            _ => None,
        }) else {
            continue;
        };
        // vsrcdefs.h: PULSE = 1, SINE, EXP, SFFM, PWL, AM.
        let (function, fields): (f64, Vec<&str>) = match waveform {
            SourceWaveform::Pulse(p) => (
                1.0,
                std::iter::once(p.initial.text.as_str())
                    .chain(std::iter::once(p.pulsed.text.as_str()))
                    .chain(
                        [&p.delay, &p.rise, &p.fall, &p.width, &p.period, &p.count]
                            .into_iter()
                            .filter_map(|v| v.as_ref().map(|v| v.text.as_str())),
                    )
                    .collect(),
            ),
            SourceWaveform::Pwl(points) => (
                5.0,
                points
                    .iter()
                    .flat_map(|p| [p.time.text.as_str(), p.value.text.as_str()])
                    .collect(),
            ),
            SourceWaveform::Function(f) => (
                match f.function {
                    SourceFunction::Sin => 2.0,
                    SourceFunction::Exp => 3.0,
                    SourceFunction::Sffm => 4.0,
                    SourceFunction::Am => 6.0,
                },
                f.values.iter().map(|v| v.text.as_str()).collect(),
            ),
        };
        expected.insert(format!("@{}[function]", device.name), function);
        let count = format!("oracle_{}_count", device.name);
        commands.push_str(&format!("let {count} = length(@{}[coeffs])\n", device.name));
        expected.insert(count, fields.len() as f64);
        for (index, field) in fields.iter().enumerate() {
            let name = format!("oracle_{}_{index}", device.name);
            commands.push_str(&format!("let {name} = @{}[coeffs][{index}]\n", device.name));
            expected.insert(name, parse_spice_number(field).unwrap());
        }
    }
    (expected, commands)
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_waveform_coefficients_omissions_and_setter_order_match_live_c() {
    let netlist = parse(WAVEFORMS);
    let (expected, commands) = waveform_expectations(&netlist);
    assert_eq!(expected["@vpulse[dc]"], 7.0);
    assert_eq!(expected["oracle_ipulse_count"], 2.0);
    assert_eq!(expected["@vpwl[function]"], 5.0);
    assert_eq!(expected["@ipwl[function]"], 1.0);
    assert_reference_commands("waveforms", WAVEFORMS, expected, &commands);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_source_functions_match_live_c_coefficients() {
    // SIN/SINE/EXP/SFFM/AM, the PULSE eighth field and PWL with r=/td= (#94, #95).
    let netlist = parse(FUNCTIONS);
    let (expected, commands) = waveform_expectations(&netlist);
    assert_eq!(expected["@vsin[function]"], 2.0);
    assert_eq!(expected["oracle_vsin_count"], 6.0);
    assert_eq!(expected["@isine[function]"], 2.0);
    assert_eq!(expected["@vexp[function]"], 3.0);
    assert_eq!(expected["@isffm[function]"], 4.0);
    assert_eq!(expected["@vam[function]"], 6.0);
    assert_eq!(expected["oracle_vcount_count"], 8.0);
    assert_eq!(expected["@ipwl[function]"], 5.0);
    assert_reference_commands("functions", FUNCTIONS, expected, &commands);
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_source_structure_matches_live_c_setup() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/parser/sources/main.cir");
    let n = Parser::new().parse_file(path).unwrap();
    assert_eq!(n.subcircuits[0].name, "div");
    assert_eq!(n.device("x1").unwrap().model.as_deref(), Some("div"));
    let rsh = parse_spice_number(&n.model("sheet").unwrap().parameters[0].value).unwrap();
    let text = include_str!("../../../conformance/parser/sources/main.cir").replace(
        ".end\n",
        ".control\nset numdgt=17\nop\nprint @sheet[rsh]\nquit\n.endc\n.end\n",
    );
    let output = reference_output_sources(
        "structure",
        &text,
        &[
            (
                "parts/divider.inc",
                include_str!("../../../conformance/parser/sources/parts/divider.inc"),
            ),
            (
                "parts/passives.lib",
                include_str!("../../../conformance/parser/sources/parts/passives.lib"),
            ),
            (
                "shared/sheet.inc",
                include_str!("../../../conformance/parser/sources/shared/sheet.inc"),
            ),
        ],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let actual = stdout
        .lines()
        .find_map(|line| line.strip_prefix("@sheet[rsh] = "))
        .expect("C model query")
        .trim()
        .parse::<f64>()
        .unwrap();
    assert!(
        (actual - rsh).abs() <= 1e-12 * rsh.abs(),
        "{stdout}\n{stderr}"
    );
}

fn reference_output(label: &str, text: &str) -> std::process::Output {
    reference_output_sources(label, text, &[])
}

fn reference_output_sources(
    label: &str,
    text: &str,
    sources: &[(&str, &str)],
) -> std::process::Output {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    let binary = if binary.is_absolute() {
        binary
    } else {
        workspace.join(binary)
    };
    let binary = binary.canonicalize().expect("reference binary exists");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "spice-parser-{label}-{}-{nonce}",
        std::process::id()
    )));
    fs::create_dir(&scratch.0).unwrap();
    fs::write(scratch.0.join("probe.cir"), text).unwrap();
    for (path, content) in sources {
        let path = scratch.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&scratch.0)
        .output()
        .expect("run reference ngspice")
}

fn assert_reference(label: &str, text: &str, expected: BTreeMap<String, f64>) {
    assert_reference_commands(label, text, expected, "");
}

fn assert_reference_commands(
    label: &str,
    text: &str,
    expected: BTreeMap<String, f64>,
    commands: &str,
) {
    let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
    let instrumented = text.replace(
        ".end\n",
        &format!(".control\nset numdgt=17\nop\n{commands}print {queries}\nquit\n.endc\n.end\n"),
    );
    let output = reference_output(label, &instrumented);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let mut actual = BTreeMap::new();
    for line in stdout.lines() {
        if let Some((query, value)) = line.split_once(" = ")
            && expected.contains_key(query)
        {
            actual.insert(
                query.to_owned(),
                value.trim().parse::<f64>().expect("C scalar"),
            );
        }
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "missing C queries:\n{stdout}\n{stderr}"
    );
    for (query, rust) in expected {
        let c = actual[&query];
        assert!(
            (rust - c).abs() <= 1e-12 * rust.abs().max(1e-12),
            "{query}: Rust {rust}, C {c}"
        );
    }
}
