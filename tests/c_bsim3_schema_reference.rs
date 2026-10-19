//! Opt-in BSIM3 input-setter queries against C, not Rust simulation parity.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::{ModelResolver, bsim3};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

use ngspice_rs::devices::ModelContext;
use ngspice_rs::devices::bsim3::{InputType, parameters};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Query every canonical numeric model field, not just hand-picked defaults.
/// C alone runs OP to expose its model fields; Rust only computes model defaults.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; full BSIM3 model-default field queries"]
fn model_defaults_match_all_c_numeric_fields() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for (index, (family, card, nominal)) in [
        ("nmos", "", 27.),
        ("pmos", "", 27.),
        ("nmos", "mobmod=3 tox=8n drout=0.8 lint=20n wint=30n ll=1e-18 lw=2e-18 lwl=3e-25 wl=4e-18 ww=5e-18 wwl=6e-25 cjsw=7e-10 pbsw=0.9 mjsw=0.2 nqsmod=2 acnqsmod=-1", 50.),
        ("pmos", "dlc=10n dwc=20n cgsl=1e-12 cgdl=2e-12 cf=3e-11 cgbo=4e-11 tnom=40", 50.),
        ("nmos", "lint=20n dlc=0", 27.),
        ("pmos", "nmos", 27.),
    ].into_iter().enumerate() {
        let model_line = format!(".model nn {family} level=8 {card}");
        let parsed = Parser::new().parse_deck(&parse_deck_text(Path::new("defaults.cir"), &format!("defaults\nm1 d g 0 0 nn l=1u w=2u\n{model_line}\n.end\n"))).unwrap();
        let resolver = ModelResolver::new(&parsed.models).unwrap();
        let input = bsim3::model_parameters(&resolver.resolve(&parsed.devices[0]).unwrap().unwrap()).unwrap();
        let model = input.model_defaults(&ModelContext::new(37., nominal)).unwrap();
        let expected: BTreeMap<_, _> = parameters::MODEL.iter()
            .filter(|p| !p.redundant && matches!(p.input_type, InputType::Real | InputType::Integer))
            .map(|p| (format!("@nn[{}]", p.name), model.get(p.name).unwrap())).collect();
        assert_eq!(expected.len(), 422);
        let queries: Vec<_> = expected.keys().cloned().collect();
        let commands = queries.chunks(24).map(|chunk| format!("print {}\n", chunk.join(" "))).collect::<String>();
        let polarity = f64::from(model.polarity);
        let text = format!("BSIM3 model defaults\nvd d 0 {}\nvg g 0 {}\nm1 d g 0 0 nn l=1u w=2u\n{model_line}\n.options tnom={nominal} temp=37\n.control\nset numdgt=17\nop\n{commands}quit\n.endc\n.end\n", 0.1 * polarity, polarity);
        let directory = Scratch(std::env::temp_dir().join(format!("bsim3-defaults-{}-{index}", std::process::id())));
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(directory.0.join("probe.cir"), text).unwrap();
        let output = Command::new(&binary).args(["-b", "probe.cir"]).current_dir(&directory.0).output().unwrap();
        let log = String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success() && !log.contains("Error") && !log.contains("aborted"), "case {index}: {log}");
        let actual: BTreeMap<_, _> = log.lines().filter_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            name.starts_with('@').then(|| (name.to_owned(), value.trim().parse::<f64>().unwrap()))
        }).collect();
        assert_eq!(actual.len(), expected.len(), "case {index}: {log}");
        for (name, expected) in expected {
            let actual = actual[&name];
            assert!(actual.is_finite() && (actual-expected).abs() <= 1e-12 * expected.abs() + 1e-24,
                "case {index}, {name}: Rust defaults {expected:e}, C {actual:e}");
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM3 setter fields, no Rust simulation"]
fn explicit_bsim3_setters_match_c_fields_for_both_level_aliases() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for level in [8, 49] {
        let text = format!(
            "BSIM3 explicit setters\nvd d 0 0.1\nvg g 0 1\n\
             m1 d g 0 0 nn l=1u ad=2p nqsmod=0.6 geo=0 delvto=0.1 mulu0=0.8\n\
             .model nn nmos level={level} version=3.3.0 tnom=50 nch=1e21 \
             ngate=1e24 lnch=1e21 lngate=1e23 vth0=0.3 vtho=0.4 \
             w=2u w=3u m=7\n.end\n"
        );
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .unwrap();
        let device = netlist.device("m1").unwrap();
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let model = bsim3::model_parameters(&resolver.resolve(device).unwrap().unwrap()).unwrap();
        let instance = model.instance_parameters(device, 1.).unwrap();
        let mut expected = BTreeMap::new();
        for name in ["tnom", "nch", "ngate", "lnch", "lngate", "vth0"] {
            let bsim3::Value::Real(value) = model.inputs.get(name).unwrap().value else {
                panic!("real model field")
            };
            expected.insert(format!("@nn[{name}]"), value);
        }
        for name in ["l", "w", "ad", "delvto", "mulu0", "nqsmod", "geo"] {
            let value = match instance.get(name).unwrap().value {
                bsim3::Value::Real(v) => v,
                bsim3::Value::Integer(v) => f64::from(v),
                _ => panic!("numeric instance field"),
            };
            expected.insert(format!("@m1[{name}]"), value);
        }
        // Model-level m is consumed and discarded by inpgmod.c, not a default.
        assert!(instance.get("m").is_none());
        expected.insert("@m1[m]".into(), 1.);
        let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
        let instrumented = text.replace(
            ".end\n",
            &format!(".control\nset numdgt=17\nop\nprint {queries}\nquit\n.endc\n.end\n"),
        );
        let directory = Scratch(
            std::env::temp_dir().join(format!("bsim3-setters-{}-{level}", std::process::id())),
        );
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(directory.0.join("probe.cir"), instrumented).unwrap();
        let output = Command::new(&binary)
            .args(["-b", "probe.cir"])
            .current_dir(&directory.0)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && !log.contains("Error") && !log.contains("aborted"),
            "{log}"
        );
        let actual: BTreeMap<_, _> = log
            .lines()
            .filter_map(|line| {
                let (name, value) = line.split_once(" = ")?;
                name.starts_with('@')
                    .then(|| (name.to_owned(), value.trim().parse::<f64>().unwrap()))
            })
            .collect();
        assert_eq!(actual.len(), expected.len(), "{log}");
        for (name, expected) in expected {
            let actual = actual[&name];
            assert!(
                actual.is_finite() && (actual - expected).abs() <= 1e-12 * expected.abs() + 1e-24,
                "level {level}, {name}: Rust input {expected:e}, C {actual:e}"
            );
        }
    }
}
