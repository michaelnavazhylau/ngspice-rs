//! Opt-in live C cross-check of MOS model binning (#109).
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_binning_reference -- --ignored
//! ```
//!
//! C bins only BSIM3/BSIM4/HiSIM models. Each deck runs `op` and
//! `show all : model` in C, and the bin C bound (or its "could not find a
//! valid modelname" failure) is compared with the card the port's resolver
//! selects after subcircuit expansion (or its parse error). C spells a
//! subcircuit-local model `x1:nch.1`; the port flattens it to `x1.nch.1`.
//!
//! Ordinary `cargo test` never needs C: everything here is `#[ignore]`d.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::ModelResolver;
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::SpiceError;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BINS: &str = "\
.model nch.1 nmos level=8 version=3.3 lmin=0.5u lmax=1u wmin=0.5u wmax=5u vth0=0.3
.model nch.2 nmos level=8 version=3.3 lmin=1u lmax=5u wmin=0.5u wmax=5u vth0=0.5
.model nch.3 nmos level=8 version=3.3 lmin=0.1u lmax=1u wmin=5u wmax=20u vth0=0.7
";

/// The bin C binds for the deck's only MOSFET, or `None` when C rejects it.
fn c_selection(tag: usize, body: &str) -> Option<String> {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-binning-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    let deck =
        format!("c\nvd d 0 1\nvg g 0 1\n{body}.control\nop\nshow all : model\nquit\n.endc\n.end\n");
    fs::write(dir.join("c.cir"), deck).unwrap();
    let output = Command::new(&binary)
        .args(["-b", "c.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if text.contains("could not find a valid modelname") {
        return None;
    }
    let names: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("model "))
        .flat_map(str::split_whitespace)
        .filter(|name| !name.bytes().all(|byte| byte == b'?'))
        .collect();
    assert_eq!(names.len(), 1, "{text}");
    Some(names[0].replace(':', "."))
}

/// The bin the port's device reports, or `None` for its explicit binning
/// parse error. BSIM3 now builds a DC device, so the selected card is read
/// from the instance rather than from a factory diagnostic.
fn rust_selection(body: &str) -> Option<String> {
    let text = format!("r\nvd d 0 1\nvg g 0 1\n{body}.op\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("r.cir"), &text))
        .unwrap();
    // Resolve on the expanded netlist: the circuit factory also rejects model
    // cards no device uses, which is unrelated to bin selection.
    let scope =
        std::sync::Arc::new(ngspice_rs::netlist::eval::ParamScope::root(&netlist.params).unwrap());
    let expanded = ngspice_rs::devices::subckt::expand_subcircuits(
        &netlist,
        &scope,
        ngspice_rs::devices::subckt::SubcircuitLimits::default(),
    )
    .unwrap();
    let resolver = ModelResolver::new(&expanded.models).unwrap();
    let device = expanded
        .devices
        .iter()
        .find(|device| device.designator == 'm')
        .unwrap();
    match resolver.resolve(device) {
        Ok(resolved) => Some(resolved.unwrap().card().name.clone()),
        Err(error) => {
            let message = error.to_string();
            if let SpiceError::NotYetPorted { .. } = error {
                // Unported families (BSIM4/HiSIM) still name the selected bin.
                if let Some(start) = message.find("binned to model '") {
                    let start = start + "binned to model '".len();
                    return Some(message[start..start + message[start..].find('\'')?].to_owned());
                }
                let start = message.find("model '")? + "model '".len();
                return Some(message[start..start + message[start..].find('\'')?].to_owned());
            }
            assert!(message.contains("cannot be binned"), "{message}");
            None
        }
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN"]
fn binning_selection_matches_c() {
    let narrow_local = "\
x1 d g inv
.subckt inv a b
m1 a b 0 0 nch w=4u l=1u
.model nch.1 nmos level=8 version=3.3 lmin=0.5u lmax=2u wmin=0.5u wmax=2u
.ends
";
    let cases = [
        format!("m1 d g 0 0 nch w=1u l=0.7u\n{BINS}"),
        format!("m1 d g 0 0 nch w=1u l=1u\n{BINS}"),
        format!("m1 d g 0 0 nch w=5u l=1u\n{BINS}"),
        format!("m1 d g 0 0 nch w=10u l=0.5u\n{BINS}"),
        format!("m1 d g 0 0 nch w=1u l=5.0009u\n{BINS}"),
        format!("m1 d g 0 0 nch w=1u l=5.0011u\n{BINS}"),
        format!("m1 d g 0 0 nch w=1u l=0.7u m=8\n{BINS}"),
        format!("m1 d g 0 0 nch w=1u l=10u\n{BINS}"),
        format!("m1 d g 0 0 nch l=1u\n{BINS}"),
        format!("{BINS}m1 d g 0 0 nch w=1u l=2u\n"),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.2 nmos level=8 lmin=1u lmax=5u wmin=0.5u wmax=5u\n\
         .model nch.1 nmos level=8 lmin=0.5u lmax=1u wmin=0.5u wmax=5u\n"
            .to_owned(),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.1 nmos level=1 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n"
            .to_owned(),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.1 nmos level=1 vto=0.2 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n\
         .model nch.2 nmos level=49 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n"
            .to_owned(),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u\n"
            .to_owned(),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.01 nmos (level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=5u)\n"
            .to_owned(),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n\
         .model nch nmos level=8\n"
            .to_owned(),
        "m1 d g 0 0 nch w=1u l=1u\n\
         .model nch.1 nmos level=54 lmin=0.5u lmax=2u wmin=0.5u wmax=2u\n\
         .model nch.2 nmos level=54 lmin=0.5u lmax=2u wmin=2u wmax=10u\n"
            .to_owned(),
        format!(
            "x1 d g inv\n.subckt inv a b\nm1 a b 0 0 nch w=1u l=1u\n\
             .model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=20u\n\
             .model nch.2 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=20u\n.ends\n{BINS}"
        ),
        format!("x1 d g inv\n.subckt inv a b\nm1 a b 0 0 nch w=1u l=0.7u\n.ends\n{BINS}"),
        format!("{narrow_local}{BINS}"),
        format!("{narrow_local}.model nch nmos level=1\n"),
    ];
    let mut selections = Vec::new();
    for (tag, body) in cases.iter().enumerate() {
        let c = c_selection(tag, body);
        assert_eq!(rust_selection(body), c, "deck {tag}:\n{body}");
        selections.push(c);
    }
    // Guard against a vacuous comparison: both outcomes must occur.
    let expected: Vec<Option<&str>> = vec![
        Some("nch.1"),
        Some("nch.2"),
        Some("nch.3"),
        Some("nch.3"),
        Some("nch.2"),
        None,
        Some("nch.1"),
        None,
        None,
        Some("nch.2"),
        Some("nch.1"),
        None,
        Some("nch.2"),
        None,
        Some("nch.01"),
        Some("nch"),
        Some("nch.1"),
        Some("x1.nch.2"),
        Some("nch.1"),
        None,
        None,
    ];
    assert_eq!(
        selections.iter().map(Option::as_deref).collect::<Vec<_>>(),
        expected
    );
}
