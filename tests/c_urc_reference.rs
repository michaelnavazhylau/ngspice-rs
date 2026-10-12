//! Opt-in live C comparison of the URC expansion (#85).
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_urc_reference -- --ignored
//! ```
//!
//! Decks beyond the committed goldens (`m10_urc_*`) run in both engines at an
//! operating point: `n=` rounding, `K < 1` (sections shrinking toward the
//! middle), a long FMAX-rule line, a line in a subcircuit, a non-ground
//! reference and the diode ladder with and without `RSPERL`. Every saved
//! vector is compared by name, and the generated elements' values are read
//! back from C with `save @u1#rlo1[resistance]`-style asks and compared with
//! the port's [`ngspice_rs::devices::Device::observation_parameter`].
//!
//! Ordinary `cargo test` never needs C: everything here is `#[ignore]`d.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::Real;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs `cards` at an operating point in C, also saving the `asks`.
fn run_c(tag: &str, cards: &str, asks: &[&str]) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-urc-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    let deck = format!(
        "c\n{cards}.control\nset filetype=ascii\nsave all {}\nop\nwrite result.raw\nquit\n\
         .endc\n.end\n",
        asks.join(" ")
    );
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
    assert!(output.status.success(), "{text}");
    let raw = fs::read_to_string(dir.join("result.raw")).unwrap_or_else(|_| panic!("{text}"));
    RawFile::parse(&raw).unwrap().plots[0].plot.clone()
}

/// Runs `cards` at an operating point through the production runner.
fn run_rust(cards: &str) -> (Plot, Circuit) {
    let text = format!("r\n{cards}.op\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("r.cir"), &text))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    (plot, circuit)
}

/// Compares every C node voltage and branch current with the port's, by name,
/// and every ask with the generated element's own parameter.
fn compare(tag: &str, cards: &str, asks: &[&str], relative: Real) {
    let c = run_c(tag, cards, asks);
    let (rust, circuit) = run_rust(cards);
    let mut compared = 0;
    for (index, variable) in c.variables.iter().enumerate() {
        let want = c.points[0][index].re;
        let name = variable.name.as_str();
        // C writes an ask as `v(@dev[key])` or bare `@dev[key]` by its unit.
        let ask = name
            .strip_prefix("v(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(name)
            .strip_prefix('@')
            .and_then(|s| s.strip_suffix(']'));
        let got = if let Some(ask) = ask {
            let (device, key) = ask.split_once('[').unwrap();
            circuit
                .device(device)
                .unwrap_or_else(|| panic!("{tag}: no generated {device}"))
                .observation_parameter(key, &ngspice_rs::devices::ModelContext::default())
                .unwrap()
                .unwrap_or_else(|| panic!("{tag}: {device} has no {key}"))
        } else {
            rust.value(name, 0)
                .unwrap_or_else(|| panic!("{tag}: the port has no {name}"))
                .re
        };
        // Asks are element values computed by the same arithmetic: they must
        // agree to the 16 digits C writes. Solutions get the deck's bound.
        let (rel, abs) = if ask.is_some() {
            (1e-15, 0.)
        } else {
            (relative, 1e-15)
        };
        assert!(
            (got - want).abs() <= rel * want.abs() + abs,
            "{tag}: {name}: port {got:e}, C {want:e}"
        );
        compared += 1;
    }
    assert_eq!(compared, c.variables.len());
    assert!(compared > asks.len(), "{tag}: nothing but asks compared");
}

#[test]
#[ignore = "needs NGSPICE_BIN"]
fn rc_ladders_match_c() {
    // Linear: the 1e-12 DC bound.
    compare(
        "rounded",
        "v1 in 0 1\nu1 in out 0 m l=1m n=2.5\nr1 out 0 1k\n.model m urc rperl=2meg\n",
        &[
            "@u1#rlo1[resistance]",
            "@u1#rhi3[resistance]",
            "@u1#clo2[capacitance]",
            "@u1#chi1[capacitance]",
            "@u1[l]",
            "@u1[n]",
        ],
        1e-12,
    );
    compare(
        "shrinking",
        "v1 in 0 1\nvr ref 0 -0.4\nu1 in out ref m l=3m\nr1 out ref 10k\n\
         .model m urc k=0.6 rperl=1meg cperl=1n fmax=10g\n",
        &["@u1#rlo1[resistance]", "@u1#clo3[capacitance]", "@u1[n]"],
        1e-12,
    );
    // K = 1.25 is exact in both engines: C's INPevaluate reads "1.2" as
    // 12 * 0.1 = 1.2000000000000002, one ulp from the correctly rounded value
    // the port parses, which moves p^28 (and so every element) by ~1e-15.
    compare(
        "long",
        "v1 in 0 1\nu1 in out 0 m l=1\nr1 out 0 1k\n.model m urc k=1.25 rperl=1k cperl=1n fmax=1g\n",
        &[
            "@u1#rlo1[resistance]",
            "@u1#rhi20[resistance]",
            "@u1#clo21[capacitance]",
        ],
        1e-12,
    );
    compare(
        "subcircuit",
        "x1 in out line\n.subckt line a b\nu1 a b 0 lm l=1m n=2\n.model lm urc(rperl=1k)\n\
         .ends\nv1 in 0 1\nr1 out 0 1k\n",
        &["@u.x1.u1#rlo2[resistance]"],
        1e-12,
    );
}

#[test]
#[ignore = "needs NGSPICE_BIN"]
fn diode_ladders_match_c() {
    // Nonlinear: independently converged Newton solutions, the 1 ppm bound.
    compare(
        "diodes",
        "v1 in 0 0.9\nu1 in out 0 m l=1m n=3\nr1 out 0 1k\n\
         .model m urc k=0.7 isperl=1e-11 rsperl=3e3\n",
        &["@u1#rlo1[resistance]", "@u1#dlo2[area]", "@u1#dhi1[area]"],
        1e-6,
    );
    compare(
        "diodes-no-rs",
        "v1 in 0 0.75\nvr ref 0 -0.1\nu1 in out ref m l=2m\nr1 out 0 10k\n\
         .model m urc isperl=1e-12\n",
        &["@u1#rhi2[resistance]", "@u1#dlo3[area]"],
        1e-6,
    );
}
