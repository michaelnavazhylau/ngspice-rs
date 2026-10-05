//! Opt-in live syntax oracle. Compares parsed scalar parameters, not Rust
//! simulation results, with C's `print @instance[parameter]` after setup.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use spice_core::parse_spice_number;
use spice_netlist::Parser;

const DECK: &str = include_str!("../../../conformance/parser/linear_sources.cir");

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_reference -- --ignored"]
fn parsed_scalars_match_live_c_instance_parameters() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    let binary = if binary.is_absolute() {
        binary
    } else {
        workspace.join(binary)
    };
    let binary = binary.canonicalize().expect("reference binary exists");
    let deck = spice_netlist::source::parse_deck_text(Path::new("reference.cir"), DECK);
    let netlist = Parser::new().parse_deck(&deck).expect("Rust parses probe");
    let mut expected = BTreeMap::new();
    for device in &netlist.devices {
        for parameter in &device.parameters {
            // Ordered assignment implements C's last-set value, including its
            // leading-source precedence. Numeric syntax uses the core parser.
            expected.insert(
                format!("@{}[{}]", device.name, parameter.name),
                parse_spice_number(&parameter.value).expect("scalar literal"),
            );
        }
    }
    // An omitted source specification has implicit DC zero in C.
    assert!(netlist.device("i2").unwrap().parameters.is_empty());
    expected.insert("@i2[dc]".to_owned(), 0.0);
    let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
    let instrumented = DECK.replace(
        ".end\n",
        &format!(".control\nop\nprint {queries}\nquit\n.endc\n.end\n"),
    );
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch =
        Scratch(std::env::temp_dir().join(format!("spice-m1a-{}-{nonce}", std::process::id())));
    fs::create_dir(&scratch.0).unwrap();
    fs::write(scratch.0.join("probe.cir"), instrumented).unwrap();
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&scratch.0)
        .output()
        .expect("run reference ngspice");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let mut actual = BTreeMap::new();
    for line in stdout.lines() {
        if let Some((query, value)) = line.split_once(" = ") {
            if query.starts_with('@') {
                actual.insert(
                    query.to_owned(),
                    value.trim().parse::<f64>().expect("C scalar"),
                );
            }
        }
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "missing C queries:\n{stdout}\n{stderr}"
    );
    for (query, rust) in expected {
        let c = actual[&query];
        // `print` uses a short decimal representation, unlike rawfile output.
        // This probe uses exact simple values; broader numeric parity is M2.
        assert!(
            (rust - c).abs() <= 1e-12 * rust.abs().max(1e-12),
            "{query}: Rust {rust}, C {c}"
        );
    }
}
