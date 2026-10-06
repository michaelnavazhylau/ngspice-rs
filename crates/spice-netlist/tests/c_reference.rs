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
    ast::{Netlist, ParameterAssignment},
};

const LINEAR: &str = include_str!("../../../conformance/parser/linear_sources.cir");
const DIODES: &str = include_str!("../../../conformance/parser/model_diodes.cir");
const TRANSISTORS: &str = include_str!("../../../conformance/parser/transistor_scalars.cir");

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

fn reference_output(label: &str, text: &str) -> std::process::Output {
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
    Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&scratch.0)
        .output()
        .expect("run reference ngspice")
}

fn assert_reference(label: &str, text: &str, expected: BTreeMap<String, f64>) {
    let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
    let instrumented = text.replace(
        ".end\n",
        &format!(".control\nset numdgt=17\nop\nprint {queries}\nquit\n.endc\n.end\n"),
    );
    let output = reference_output(label, &instrumented);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let mut actual = BTreeMap::new();
    for line in stdout.lines() {
        if let Some((query, value)) = line.split_once(" = ")
            && query.starts_with('@')
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
