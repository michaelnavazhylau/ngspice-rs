//! Opt-in BSIM4 input-setter queries against C, not Rust simulation parity.
//!
//! `b4set.c`/`b4temp.c` defaults and the `b4ld.c` load are not ported, so this
//! compares only the **explicit setter projection**: the values `b4mpar.c`/
//! `b4par.c` store, read back from C's model and instance fields.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::{ModelResolver, bsim4};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM4 setter fields, no Rust simulation"]
fn explicit_bsim4_setters_match_c_fields_for_both_level_aliases() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for level in [14, 54] {
        let text = format!(
            "BSIM4 explicit setters\nvd d 0 0.1\nvg g 0 1\n\
             m1 d g 0 0 nn l=1u ad=2p nf=2 sa=1u sb=2u sd=3u min=1 \
             rbdb=4 rbsb=5 xgw=6 ngcon=7 delvto=0.1 mult_i=2\n\
             .model nn nmos level={level} version=4.8 tnom=50 ndep=1e21 \
             nsd=1e24 ngate=1.1e24 lndep=1e21 lnsd=1.1e24 vth0=0.3 vtho=0.4 \
             epsrox=4.2 rdsmod=1 rbodyMod=1 \
             w=2u w=3u m=7\n.end\n"
        );
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .unwrap();
        let device = netlist.device("m1").unwrap();
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let model = bsim4::model_parameters(&resolver.resolve(device).unwrap().unwrap()).unwrap();
        let instance = model.instance_parameters(device, 1.).unwrap();
        let mut expected = BTreeMap::new();
        for name in [
            "tnom", "ndep", "nsd", "ngate", "lndep", "lnsd", "vth0", "epsrox",
        ] {
            let bsim4::Value::Real(value) = model.inputs.get(name).unwrap().value.clone() else {
                panic!("real model field")
            };
            expected.insert(format!("@nn[{name}]"), value);
        }
        for name in [
            // `ngcon` is not here: `b4set.c` re-derives it during setup, so it
            // is not an explicit-setter field.
            "l", "w", "ad", "sa", "sb", "sd", "delvto", "mult_i", "nf", "min", "rbdb", "rbsb", "xgw",
        ] {
            let value = match instance.get(name).unwrap().value {
                bsim4::Value::Real(value) => value,
                bsim4::Value::Integer(value) => f64::from(value),
                ref other => panic!("{name}: numeric instance field, got {other:?}"),
            };
            expected.insert(format!("@m1[{name}]"), value);
        }
        // Model-level `m` is consumed and discarded by inpgmod.c, not a default.
        assert!(instance.get("m").is_none());
        expected.insert("@m1[m]".into(), 1.);
        let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
        let instrumented = text.replace(
            ".end\n",
            &format!(".control\nset numdgt=17\nop\nprint {queries}\nquit\n.endc\n.end\n"),
        );
        let directory = Scratch(
            std::env::temp_dir().join(format!("bsim4-setters-{}-{level}", std::process::id())),
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
