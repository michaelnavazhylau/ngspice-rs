//! Opt-in BSIM4 temperature-stage queries against C: every numeric model
//! member after `b4set.c` and `b4temp.c` (with `b4check.c`'s resets), read
//! back with `@model[keyword]`, and the instance asks `b4ask.c` answers from
//! values `b4temp.c` leaves and `b4ld.c` does not overwrite. The instance
//! is the deck's only one, so it is the instance C checks.
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

/// `(label, instance parameters, model card, .temp in Celsius)`.
fn decks() -> Vec<(&'static str, &'static str, &'static str, f64)> {
    vec![
        ("nmos defaults", "l=1u w=2u", ".model nn nmos level=14", 27.),
        (
            "pmos hot",
            "l=0.2u w=3u nf=2",
            ".model nn pmos level=54 version=4.8 tnom=40",
            85.,
        ),
        (
            "explicit nmos",
            "l=0.1u w=1u nf=3 sa=0.5u sb=0.6u sd=0.2u sc=1u rbodymod=1 rgatemod=3",
            ".model nn nmos level=54 version=4.8.3 tnom=50 toxe=2.1e-9 toxp=1.8e-9 \
             epsrox=4.1 rdsmod=1 rbodymod=1 rgatemod=3 igcmod=1 igbmod=1 \
             mobmod=3 capmod=1 tempmod=0 diomod=2 geomod=2 \
             vth0=0.4 k1=0.5 lvth0=1e-9 agidl=1e-9 bgidl=2e9 rgidl=0.5 \
             aigsd=1e-2 laigsd=1e-11 njts=21 tnjtssw=3 dlcig=-2e-9 \
             cjs=1e-3 mjsws=0.3 rsh=5 xgw=1e-7 ngcon=2 wpemod=1 \
             keta=-0.03 drout=0.6 cgidl=0.4 ijthsfwd=0.2 xjbvs=2 bvs=11 \
             saref=1e-6 sbref=1e-6 ku0=1e-8 kvth0=1e-8 stk2=1e-8 steta0=1e-8 \
             kvsat=0.3 tku0=1e-3 llodku0=1 wlodku0=1 web=1e-3 wec=1e-3 \
             kvth0we=1e-3 k2we=1e-3 ku0we=1e-3",
            60.,
        ),
        (
            "body network",
            "l=0.5u w=4u nf=4 rbodymod=2",
            ".model nn nmos level=14 rbodymod=2 rbps0=40 rbpd0=45 rbpbx0=90 rbpby0=95 \
             rbsbx0=100 rbsby0=110 rbdbx0=120 rbdby0=130 rbpsl=0.1 rbpdw=0.2 \
             rbpbxnf=0.3 rbsdbxl=0.1 rbsdbyw=0.2 gbmin=1e-11 tempmod=3 at=1e-4 \
             mtrlmod=1 mtrlcompatmod=1 eot=1.2e-9 phig=4.6 easub=4.05 epsrsub=11.9",
            -20.,
        ),
        (
            "material eot",
            "l=0.15u w=1.5u",
            ".model nn nmos level=14 mtrlmod=1 eot=1.1e-9 vddeot=1.2 tempeot=320 \
             leffeot=0.1u weffeot=1u ados=1.1 bdos=0.9 ngate=1e20 dvtp0=1e-8 \
             nsub=2e23 phig=4.5",
            27.,
        ),
        (
            "pmos selectors",
            "l=1u w=1u",
            ".model nn pmos level=14 mobmod=5 capmod=0 tempmod=3 at=1e-4 version=4.8.1 \
             tnom=10 trnqsmod=1 acnqsmod=1 permod=0 fnoimod=0 tnoimod=2 \
             aigsd=1e-2 bigs=2e-3 dlc=1e-8 cjsws=2e-10 dwc=3e-9 tcj=-0.02 \
             tpb=0.05 jtweff=-1 kvsat=2 wlod=-1",
            150.,
        ),
        (
            "clamps",
            "l=1u w=2u ps=-1u ad=0",
            ".model nn nmos level=14 mobmod=6 capmod=2 tempmod=1 version=4.8.0 \
             pbs=0.05 ijthsrev=-1 bvs=-2 xjbvs=-1 mjswgd=1.2 lambda=1e-8 \
             a2=1.2 prwg=-1 rdsw=-10",
            27.,
        ),
    ]
}

fn probe(binary: &Path, label: &str, text: &str) -> String {
    let directory = Scratch(std::env::temp_dir().join(format!(
        "bsim4-temp-{}-{}",
        std::process::id(),
        label.replace(' ', "-")
    )));
    fs::create_dir_all(&directory.0).unwrap();
    fs::write(directory.0.join("probe.cir"), text).unwrap();
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&directory.0)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM4 temperature-stage fields"]
fn bsim4_temperature_stage_matches_c_fields() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    let mut compared = 0;
    for (label, instance, card, temp) in decks() {
        let text = format!(
            "BSIM4 temperature\nvd d 0 0.1\nvg g 0 1\nm1 d g 0 0 nn {instance}\n{card}\n.end\n"
        );
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .unwrap();
        let device = netlist.device("m1").unwrap();
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let model = bsim4::model_parameters(&resolver.resolve(device).unwrap().unwrap()).unwrap();
        let (members, asks, _messages) = model
            .temperature_values(device, temp, 27.)
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        let mut control = String::from(".control\nset numdgt=17\nop\n");
        for chunk in members.chunks(40) {
            let names: Vec<_> = chunk.iter().map(|(n, _)| format!("@nn[{n}]")).collect();
            control.push_str(&format!("print {}\n", names.join(" ")));
        }
        let names: Vec<_> = asks.iter().map(|(n, _)| format!("@m1[{n}]")).collect();
        control.push_str(&format!("print {}\nquit\n.endc\n.end\n", names.join(" ")));
        // The port's parser does not accept `.temp` yet; only C reads it.
        let c_text = text.replace(".end\n", &format!(".temp {temp}\n{control}"));
        let log = probe(&binary, label, &c_text);
        let actual: BTreeMap<String, f64> = log
            .lines()
            .filter_map(|line| {
                let (name, value) = line.split_once(" = ")?;
                Some((name.trim().to_owned(), value.trim().parse().ok()?))
            })
            .collect();
        let mut failures = Vec::new();
        let expected = members
            .iter()
            .map(|(n, v)| (format!("@nn[{n}]"), *v))
            .chain(asks.iter().map(|(n, v)| (format!("@m1[{n}]"), *v)));
        for (name, expected) in expected {
            let Some(&actual) = actual.get(&name) else {
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
    eprintln!("compared {compared} BSIM4 temperature-stage values with C");
}

/// The check's fatal and warning messages, as C prints them in batch mode
/// (to stderr, after `b4temp.c`'s own stdout/stderr warnings).
#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM4 check messages"]
fn bsim4_check_messages_match_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for (label, instance, card, fatal) in [
        (
            "warnings",
            "l=1u w=2u",
            ".model nn nmos level=14 mjswgd=1.2 lambda=1e-8 a2=1.2 \
             prwg=-1 rdsw=-10 dlcig=-1e-9 wlod=-1 kvsat=2",
            false,
        ),
        (
            "fatal",
            "l=1u w=2u",
            ".model nn nmos level=14 toxe=-1e-9 xj=-1 dvt1=-1 ngate=1e26",
            true,
        ),
    ] {
        let text = format!(
            "BSIM4 check\nvd d 0 0.1\nvg g 0 1\nm1 d g 0 0 nn {instance}\n{card}\n.op\n.end\n"
        );
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .unwrap();
        let device = netlist.device("m1").unwrap();
        let resolver = ModelResolver::new(&netlist.models).unwrap();
        let model = bsim4::model_parameters(&resolver.resolve(device).unwrap().unwrap()).unwrap();
        let result = model.temperature_values(device, 27., 27.);
        let log = probe(&binary, label, &text);
        let messages: Vec<String> = match &result {
            Ok((_, _, messages)) => messages.clone(),
            Err(error) => error
                .to_string()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        };
        assert_eq!(result.is_err(), fatal, "{label}: {result:?}\n{log}");
        // Every Rust message line must appear in C's output.
        for message in &messages {
            // The error's own "circuit:" prefix is the port's, not C's text.
            for line in message
                .lines()
                .filter(|l| !l.trim().is_empty() && l.trim() != "circuit:")
            {
                assert!(
                    log.contains(line.trim()),
                    "{label}: Rust line {line:?} not in C output:\n{log}"
                );
            }
        }
        assert!(!messages.is_empty(), "{label}: no messages");
    }
}
