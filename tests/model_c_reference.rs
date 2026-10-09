//! Opt-in C setup/model queries; no Rust device simulation is implied.
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const PROBE: &str = include_str!("../conformance/parser/model_schemas.cir");

#[test]
#[ignore = "requires NGSPICE_BIN; validates model schemas/selectors against C setup"]
fn model_resolver_and_diode_defaults_match_c_setup() {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("model_schemas.cir"), PROBE))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let context = ModelContext::default();
    let mut expected = BTreeMap::new();
    for instance in &netlist.devices {
        if instance.designator != 'd' {
            continue;
        }
        let resolved = resolver.resolve(instance).unwrap().unwrap();
        let model = resolved.diode_parameters(&context).unwrap();
        let device = resolved
            .diode_instance_parameters(instance, &context)
            .unwrap();
        let name = &resolved.card().name;
        expected.insert(format!("@{name}[is]"), model.saturation_current);
        expected.insert(format!("@{name}[n]"), model.emission_coefficient);
        expected.insert(format!("@{name}[rs]"), model.series_resistance);
        expected.insert(
            format!("@{name}[tnom]"),
            model.nominal_temperature_kelvin - 273.15,
        );
        expected.insert(
            format!("@{name}[level]"),
            f64::from(resolved.levels().applied.unwrap()),
        );
        expected.insert(
            format!("@{}[temp]", instance.name),
            device.temperature_kelvin - 273.15,
        );
        expected.insert(format!("@{}[area]", instance.name), device.area);
    }
    assert_eq!(expected["@duplicate[is]"], 4e-14);
    assert_eq!(expected["@explicit[level]"], 1.0);
    assert_eq!(expected["@dexplicit[temp]"], 40.0);
    assert_eq!(
        resolver
            .resolve(netlist.device("q1").unwrap())
            .unwrap()
            .unwrap()
            .levels()
            .selector,
        2
    );
    assert_eq!(
        resolver
            .resolve(netlist.device("m1").unwrap())
            .unwrap()
            .unwrap()
            .levels()
            .selector,
        1
    );
    // These C-only queries ensure repeated selector-only level values did not
    // switch to the unavailable VBIC/BSIM backend. They are not Rust schemas.
    expected.insert("@qmod[bf]".into(), 100.0);
    expected.insert("@mmod[kp]".into(), 1e-3);
    let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
    let text = PROBE.replace(
        ".end\n",
        &format!(".control\nset numdgt=17\nop\nprint {queries}\nquit\n.endc\n.end\n"),
    );
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch(
        std::env::temp_dir().join(format!("spice-model-schema-{}-{nonce}", std::process::id())),
    );
    fs::create_dir(&scratch.0).unwrap();
    fs::write(scratch.0.join("probe.cir"), text).unwrap();
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute(), "NGSPICE_BIN must be an absolute path");
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let mut actual = BTreeMap::new();
    for line in stdout.lines() {
        if let Some((query, value)) = line.split_once(" = ")
            && query.starts_with('@')
        {
            actual.insert(query.to_owned(), value.trim().parse::<f64>().unwrap());
        }
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "missing queries:\n{stdout}\n{stderr}"
    );
    for (query, want) in expected {
        let got = actual[&query];
        assert!(
            got.is_finite() && (got - want).abs() <= 1e-12 * want.abs() + 1e-24,
            "{query}: Rust {want}, C {got}"
        );
    }
}
