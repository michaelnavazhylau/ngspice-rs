//! Opt-in `b3check.c` diagnostic parity against C. The check messages embed
//! the computed size-dependent parameters, so exact text equality also
//! compares those numbers the way C reports them. Not a load/simulation test.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::bsim3;
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Check-log lines C writes to stderr, excluding its header and unrelated
/// messages. C's version warning goes to stdout, so it cannot appear here.
fn c_check_lines(log: &str) -> Vec<String> {
    // Startup and solver noise is unrelated to `b3check.c`; a solvable deck
    // removes the solver cases, and this list keeps the intent explicit.
    const NOISE: [&str; 6] = [
        "Warning: can't find the initialization file",
        "Warning: singular matrix",
        "Warning: Dynamic gmin stepping failed",
        "Warning: True gmin stepping failed",
        "Warning: source stepping failed",
        "Warning: gmin stepping failed",
    ];
    log.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("Warning:") || line.starts_with("Fatal:"))
        .filter(|line| !NOISE.iter().any(|prefix| line.starts_with(prefix)))
        .map(str::to_owned)
        .collect()
}

fn model_card(card: &str) -> String {
    format!(
        "probe\nvd d 0 0.1\nvg g 0 1\nm1 d g 0 0 nn l=1u w=2u {}\n.model nn nmos level=8 {card}\n.options temp=27 tnom=27\n.control\nop\nquit\n.endc\n.end\n",
        "pd=4u ps=4u"
    )
}

fn run_c(binary: &Path, deck: &str, tag: &str) -> (String, bool) {
    let directory =
        Scratch(std::env::temp_dir().join(format!("bsim3-check-{}-{tag}", std::process::id())));
    fs::create_dir_all(&directory.0).unwrap();
    fs::write(directory.0.join("probe.cir"), deck).unwrap();
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&directory.0)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    (log, output.status.success())
}

fn run_rust(card: &str, instance: &str) -> Result<Vec<String>, String> {
    let text = format!("probe\nm1 d g 0 0 nn {instance}\n.model nn nmos level=8 {card}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    match bsim3::model_parameters(&resolved).unwrap().setup_instance(
        &netlist.devices[0],
        1.,
        &ModelContext::new(27., 27.),
    ) {
        Ok(setup) => Ok(setup
            .warnings
            .iter()
            .filter(|d| d.message.starts_with("Warning:"))
            .map(|d| d.message.clone())
            .collect()),
        Err(error) => Err(format!("{error}")),
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; compares b3check.c diagnostics with C"]
fn parameter_check_diagnostics_match_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    let warnings = [
        ("paramchk=1", "l=0.05u w=0.1u"),
        (
            "noff=0.05 voffcv=0.8 moin=100 capmod=3 acde=0.1",
            "l=1u w=2u",
        ),
        (
            "paramchk=1 a2=5 rdsw=-1 pdiblc1=-0.1 cgdo=-1p cgso=-2p cgbo=-3p",
            "l=1u w=2u",
        ),
        (
            "paramchk=1 nlx=-1n dvt0=-1 nfactor=-0.1 cdsc=-1e-4 cdscd=-1e-4 eta0=-0.01 pscbe2=-1 vsat=1e4",
            "l=1u w=2u",
        ),
        ("", "l=1u w=2u"),
        (
            "paramchk=1 nch=1e14 nsub=1e13 ngate=1e17 tox=1n",
            "l=1u w=2u",
        ),
    ];
    for (index, (card, instance)) in warnings.into_iter().enumerate() {
        let deck = model_card(card);
        let deck = deck.replace(
            "m1 d g 0 0 nn l=1u w=2u pd=4u ps=4u",
            &format!("m1 d g 0 0 nn {instance} pd=4u ps=4u"),
        );
        let (log, _exit_ok) = run_c(&binary, &deck, &format!("warning-{index}"));
        let success = !log.contains("Fatal error:") && !log.contains("aborted");
        assert!(
            success,
            "case {index} should reach the operating point: {log}"
        );
        let expected = c_check_lines(&log);
        let actual = run_rust(card, &format!("{instance} pd=4u ps=4u")).unwrap();
        assert_eq!(actual, expected, "case {index}: {card}");
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; shared-model resets across instances"]
fn model_resets_persist_across_instances_like_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    let card = "paramchk=1 calcacm=3 cgdo=-1p cgso=-2p cgbo=-3p";
    // Two instances of one model: C reports the resets once, then reuses the
    // corrected model for the second instance.
    let deck = format!(
        "probe\nvd d 0 0.1\nvg g 0 1\n\
         m1 d g 0 0 nn l=1u w=2u pd=4u ps=4u\n\
         m2 d g 0 0 nn l=2u w=2u pd=4u ps=4u\n\
         .model nn nmos level=8 {card}\n.options temp=27 tnom=27\n\
         .control\nop\nquit\n.endc\n.end\n"
    );
    let (log, _exit_ok) = run_c(&binary, &deck, "shared");
    assert!(!log.contains("Fatal error:"), "{log}");
    let expected = c_check_lines(&log);

    let text = format!(
        "shared\nm1 d g 0 0 nn l=1u w=2u pd=4u ps=4u\n\
         m2 d g 0 0 nn l=2u w=2u pd=4u ps=4u\n\
         .model nn nmos level=8 {card}\n.end\n"
    );
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("shared.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let context = ModelContext::new(27., 27.);
    let mut shared =
        bsim3::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap())
            .unwrap()
            .model_setup(&context, &netlist.devices[0].location)
            .unwrap();
    let mut actual = Vec::new();
    for device in &netlist.devices {
        let resolved = resolver.resolve(device).unwrap().unwrap();
        let inputs = bsim3::model_parameters(&resolved)
            .unwrap()
            .instance_parameters(device, 1.)
            .unwrap();
        let setup = shared.setup_instance(&inputs, &device.location).unwrap();
        actual.extend(
            setup
                .warnings
                .iter()
                .filter(|d| d.message.starts_with("Warning:"))
                .map(|d| d.message.clone()),
        );
    }
    assert_eq!(actual, expected);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; differential b3check.c check parity"]
fn fatal_parameter_checks_match_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for (index, card) in [
        "clc=-1e-7",
        "xj=0",
        "delta=-1",
        "pclm=0",
        "dvt1=-1",
        "dvt1w=-1",
        "dsub=-1",
        "b1=-2u",
        "w0=-2u",
        "nlx=-2u",
        "u0=0",
        "vsat=0",
        "nch=0",
        "nsub=0",
        "ngate=-1",
        // NGATE above the setter's 1e23 threshold is converted to cm^-3
        // units, so 1e26 is legal; C reports no check message either.
        "ngate=1e26",
        "lintnoi=1u",
        "drout=-1",
        "tox=0",
        "toxm=0",
        "ijth=-1",
    ]
    .into_iter()
    .enumerate()
    {
        let deck = model_card(card);
        let (log, _exit_ok) = run_c(&binary, &deck, &format!("fatal-{index}"));
        // C's batch mode still exits 0 after an aborted analysis, so the
        // observable failure is its own message rather than the status.
        let c_lines = c_check_lines(&log);
        let c_fatal: Vec<_> = c_lines
            .iter()
            .filter(|line| line.starts_with("Fatal:"))
            .cloned()
            .collect();
        let rust = run_rust(card, "l=1u w=2u pd=4u ps=4u");
        assert_eq!(
            !c_fatal.is_empty(),
            rust.is_err(),
            "case {index} ({card}) disagreed on fatality: C {c_fatal:?}, Rust {rust:?}"
        );
        match rust {
            Ok(warnings) => assert_eq!(warnings, c_lines, "case {index}: {card}"),
            Err(error) => {
                let actual: Vec<_> = error
                    .split("Fatal error(s) detected during BSIM3V3.3 parameter checking: ")
                    .nth(1)
                    .unwrap_or_else(|| panic!("case {index}: unexpected error {error}"))
                    .split(" Fatal: ")
                    .map(|part| {
                        if part.starts_with("Fatal:") {
                            part.to_owned()
                        } else {
                            format!("Fatal: {part}")
                        }
                    })
                    .collect();
                assert_eq!(actual, c_fatal, "case {index}: {card}");
            }
        }
    }
}
