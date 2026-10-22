//! Opt-in BSIM4 model-setup queries against C: every numeric model member
//! after `b4set.c`'s model pass, read back with `@model[keyword]`.
//!
//! C runs `b4temp.c` before the ask (`b4mask.c` reads the struct members),
//! so members `b4temp.c` rewrites are compared against their documented
//! post-temperature value rather than skipped silently; see
//! [`TEMPERATURE_REWRITTEN`].
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::{ModelResolver, bsim4};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Members `b4temp.c` overwrites in the model struct after setup, so the C
/// ask cannot see `b4set.c`'s value. Each is excluded with the C line that
/// rewrites it; the temperature stage owns their comparison.
const TEMPERATURE_REWRITTEN: &[(&str, &str)] = &[
    ("cf", "b4temp.c:174 (ungiven CF from EPSROX/TOXE)"),
    (
        "cgdo",
        "b4temp.c:183-186 (ungiven CGDO from DLC/XJ and COXE)",
    ),
    (
        "cgso",
        "b4temp.c:190-193 (ungiven CGSO from DLC/XJ and COXE)",
    ),
    ("cgbo", "b4temp.c:196 (ungiven CGBO from DWC and COXE)"),
];

/// `b4temp.c` clamps that rewrite set-up members of one card, with the
/// value C's ask then reports: `(card, keyword, C value, C line)`.
const TEMPERATURE_CLAMPS: &[(&str, &str, f64, &str)] = &[
    ("clamps", "pbs", 0.1, "b4temp.c:97-99 (PBS < 0.1)"),
    ("clamps", "pbd", 0.1, "b4temp.c:110-112 (PBD < 0.1)"),
    ("clamps", "ijthsrev", 0.0, "b4temp.c:384-386 (IJTHSREV < 0)"),
    ("clamps", "ijthdrev", 0.0, "b4temp.c:380-382 (IJTHDREV < 0)"),
    ("clamps", "bvs", 0.0, "b4temp.c:412-414 (BVS < 0)"),
    ("clamps", "bvd", 0.0, "b4temp.c:398-400 (BVD < 0)"),
];

fn cards() -> Vec<(&'static str, String)> {
    vec![
        ("nmos defaults", ".model nn nmos level=14".into()),
        (
            "pmos defaults",
            ".model nn pmos level=54 version=4.8".into(),
        ),
        (
            "explicit nmos",
            ".model nn nmos level=54 version=4.8.3 tnom=50 toxe=2.1e-9 toxp=1.8e-9 \
             epsrox=4.1 mtrlmod=1 rdsmod=1 rbodymod=1 rgatemod=3 igcmod=1 igbmod=1 \
             mobmod=4 capmod=1 cvchargemod=1 tempmod=2 diomod=2 geomod=2 \
             vth0=0.4 k1=0.5 lvth0=1e-9 agidl=1e-9 bgidl=2e9 rgidl=0.5 \
             aigsd=1e-2 laigsd=1e-11 njts=21 tnjtssw=3 dlcig=2e-9 \
             cjs=1e-3 mjsws=0.3 rsh=5 xgw=1e-7 ngcon=2 wpemod=1 \
             keta=-0.03 drout=0.6 cgidl=0.4 ijthsfwd=0.2 xjbvs=2 bvs=11"
                .into(),
        ),
        (
            "pmos selectors",
            ".model nn pmos level=14 mobmod=5 capmod=0 tempmod=3 version=4.8.1 \
             tnom=10 trnqsmod=1 acnqsmod=1 permod=0 fnoimod=0 tnoimod=2 \
             aigsd=1e-2 bigs=2e-3 dlc=1e-8 cjsws=2e-10 dwc=3e-9"
                .into(),
        ),
        (
            "clamps",
            ".model nn nmos level=14 mobmod=6 capmod=2 tempmod=1 version=4.8.0 \
             pbs=0.05 ijthsrev=-1 bvs=-2 xjbvs=-1"
                .into(),
        ),
    ]
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM4 model setup fields"]
fn bsim4_model_setup_matches_c_fields() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    let excluded: BTreeMap<_, _> = TEMPERATURE_REWRITTEN.iter().copied().collect();
    let mut compared = 0;
    for (label, card) in cards() {
        let text =
            format!("BSIM4 setup\nvd d 0 0.1\nvg g 0 1\nm1 d g 0 0 nn l=1u w=2u\n{card}\n.end\n");
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .unwrap();
        let device = netlist.device("m1").unwrap();
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let model = bsim4::model_parameters(&resolver.resolve(device).unwrap().unwrap()).unwrap();
        let expected: Vec<_> = model
            .setup_values(27.)
            .unwrap()
            .into_iter()
            .filter(|(name, _)| !excluded.contains_key(name))
            .map(|(name, value)| {
                let clamp = TEMPERATURE_CLAMPS
                    .iter()
                    .find(|c| c.0 == label && c.1 == name);
                (name, clamp.map_or(value, |c| c.2))
            })
            .collect();
        let mut control = String::from(".control\nset numdgt=17\nop\n");
        for chunk in expected.chunks(40) {
            let names: Vec<_> = chunk.iter().map(|(n, _)| format!("@nn[{n}]")).collect();
            control.push_str(&format!("print {}\n", names.join(" ")));
        }
        control.push_str("quit\n.endc\n.end\n");
        let directory = Scratch(std::env::temp_dir().join(format!(
            "bsim4-setup-{}-{}",
            std::process::id(),
            label.replace(' ', "-")
        )));
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(
            directory.0.join("probe.cir"),
            text.replace(".end\n", &control),
        )
        .unwrap();
        let output = Command::new(&binary)
            .args(["-b", "probe.cir"])
            .current_dir(&directory.0)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        let actual: BTreeMap<String, f64> = log
            .lines()
            .filter_map(|line| {
                let (name, value) = line.split_once(" = ")?;
                let name = name.trim().strip_prefix("@nn[")?.strip_suffix(']')?;
                Some((name.to_owned(), value.trim().parse().ok()?))
            })
            .collect();
        let mut failures = Vec::new();
        for (name, expected) in &expected {
            let Some(&actual) = actual.get(*name) else {
                failures.push(format!("{name}: no C value"));
                continue;
            };
            if (actual - expected).abs() > 1e-12 * expected.abs() + 1e-300 {
                failures.push(format!("{name}: Rust {expected:e}, C {actual:e}"));
            }
            compared += 1;
        }
        assert!(failures.is_empty(), "{label}: {failures:#?}\n{log}");
    }
    eprintln!("compared {compared} BSIM4 model members with C");
}
