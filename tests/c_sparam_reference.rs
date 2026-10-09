//! Opt-in #105 cross-check of `.sp` S-parameter analysis and RF port sources
//! against a live C `ngspice` (an `RFSPICE` build) in genuine batch mode.
//!
//! `ngspice -b -r` writes C's own plot (binary rawfile, the spellings
//! `S_1_1`, `Y_1_1`, `Z_1_1` and `v(Rbase)` that `spice-rs simulate` reproduces)
//! and `spice-rs simulate` runs the same deck. Plot order, names and flags must
//! agree, the variable **sets** must be identical (C interleaves each port's
//! `#res` node with its branch current; the port allocates node rows first)
//! and every value must agree by name within `1e-9 |C| + 1e-12`, the policy of
//! `tests/c_batch_reference.rs`. The decks cover one, two and three ports,
//! equal and unequal `z0`, `lin`/`dec`/`oct` grids, a frequency-dependent LC
//! filter, a later waveform overriding `pwr`, and port sources in `.op`,
//! `.ac` and `.tran`.
//!
//! The one documented divergence is exercised too: for a series resistor the
//! Z matrix does not exist (`E - S` is singular). C's `cinverse` zero-fills only
//! an exactly singular matrix, so its rounding-level pivot gives huge
//! rounding-dependent values; the port writes zeros (C's contract for the
//! singular case). Everything else in that plot is compared.
//!
//! `#[ignore]`d because it needs a built C `ngspice`:
//!
//! ```text
//! NGSPICE_BIN=/path/to/ngspice/build/src/ngspice \
//!   cargo test -p ngspice-rs --test c_sparam_reference -- --ignored
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile};

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("the workspace root exists")
}

/// Runs C batch mode and `spice-rs simulate` on `deck_text`.
fn run_both(name: &str, deck_text: &str) -> (RawFile, RawFile) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let directory =
        std::env::temp_dir().join(format!("spice-rs-c-sparam-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let _cleanup = Cleanup(directory.clone());
    fs::write(directory.join("deck.cir"), deck_text).unwrap();
    let c = Command::new(&binary)
        .args(["-b", "-r", "c.raw", "deck.cir"])
        .current_dir(&directory)
        .output()
        .expect("the C ngspice runs");
    assert!(
        c.status.success(),
        "{name}: ngspice failed:\n{}\n{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    let rust = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("simulate")
        .arg("--output")
        .arg(directory.join("rust.raw"))
        .arg(directory.join("deck.cir"))
        .output()
        .expect("spice-rs runs");
    assert!(
        rust.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&rust.stderr)
    );
    let want = RawFile::parse_bytes(&fs::read(directory.join("c.raw")).unwrap())
        .expect("the C rawfile parses");
    let got = RawFile::load(directory.join("rust.raw")).unwrap();
    (got, want)
}

fn sorted_names(plot: &Plot) -> Vec<String> {
    let mut names: Vec<_> = plot.variables.iter().map(|v| v.name.clone()).collect();
    names.sort();
    names
}

/// Compares every plot; `singular_z` names plots whose Z block does not exist.
fn compare(name: &str, deck_text: &str, singular_z: bool) {
    let (got, want) = run_both(name, deck_text);
    assert_eq!(got.len(), want.len(), "{name}: plot count");
    for (index, (got, want)) in got.plots.iter().zip(&want.plots).enumerate() {
        let (got, want) = (&got.plot, &want.plot);
        assert_eq!(got.plotname, want.plotname, "{name}: plot {index}");
        assert_eq!(got.flags, want.flags, "{name}: plot {index}");
        assert_eq!(
            sorted_names(got),
            sorted_names(want),
            "{name}: {} variables",
            want.plotname
        );
        assert_eq!(got.point_count(), want.point_count(), "{name}: points");
        for variable in &want.variables {
            let want_column = want.column(&variable.name).unwrap();
            let got_column = got.column(&variable.name).unwrap();
            let unit = &got.variables[got.variable_index(&variable.name).unwrap()].unit;
            assert_eq!(*unit, variable.unit, "{name}: {} unit", variable.name);
            for (point, (a, b)) in got_column.iter().zip(&want_column).enumerate() {
                if singular_z && variable.name.starts_with("Z_") {
                    assert_eq!(
                        a.magnitude(),
                        0.,
                        "{name}: Rust {} is zero-filled",
                        variable.name
                    );
                    assert!(
                        b.magnitude() == 0. || b.magnitude() > 1e12,
                        "{name}: C {} is zero or rounding-sized, got {b}",
                        variable.name
                    );
                    continue;
                }
                // C's batch writer leaves a complex scale's imaginary half
                // uninitialised (see c_batch_reference.rs).
                let difference = if variable.name == "frequency" {
                    (a.re - b.re).abs()
                } else {
                    (*a - *b).magnitude()
                };
                assert!(
                    difference <= 1e-9 * b.magnitude() + 1e-12,
                    "{name}: {} '{}' point {point}: {a} != {b}",
                    want.plotname,
                    variable.name
                );
            }
        }
    }
}

fn fixture(name: &str) -> String {
    fs::read_to_string(workspace().join(format!("conformance/netlists/{name}.cir"))).unwrap()
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN (RFSPICE build); runs C out of process"]
fn the_sp_fixtures_match_c_batch_mode() {
    for name in ["sp_attenuator", "sp_rc", "sp_multi"] {
        compare(name, &fixture(name), false);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN (RFSPICE build); runs C out of process"]
fn one_and_three_port_decks_match_c() {
    compare(
        "one_port",
        "one port\nv1 1 0 dc 0 ac 1 portnum 1 z0 50\nr1 1 2 25\nc1 2 0 1n\n\
         .sp dec 5 1e5 1e9\n.end\n",
        false,
    );
    compare(
        "three_ports",
        "three ports\nV1 in 0 dc 0 ac 1 portnum 1 z0 100\nRpt in x 100\nC1 x 0 1e-9\n\
         R2 x out 10\nV2 out 0 dc 0 ac 0 portnum 2 z0 50\nV3 x 0 portnum 3 z0 200\n\
         .sp lin 20 1e8 1e9 0\n.end\n",
        false,
    );
    compare(
        "reordered",
        "reordered ports\nvb b 0 dc 0 portnum 2 z0 75\nra a b 100\nrb b 0 300\n\
         va a 0 dc 0 portnum 1 z0 50\n.sp oct 2 1k 8k\n.end\n",
        false,
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN (RFSPICE build); runs C out of process"]
fn a_frequency_dependent_filter_matches_c() {
    compare(
        "chebyshev",
        "chebyshev low-pass\nC1 in 0 33.2p\nL1 in 2 99.2n\nC2 2 0 57.2p\nL2 2 out 99.2n\n\
         C3 out 0 33.2p\nV1 in 0 dc 0 ac 1 portnum 1 z0 50\nV2 out 0 dc 0 ac 0 portnum 2 z0 50\n\
         .sp lin 40 2.5MEG 250MEG\n.end\n",
        false,
    );
    compare(
        "powered",
        "power port settings\nV1 in 0 dc 0 ac 1 portnum 1 z0 100 pwr 0.001 freq 2.3e9\n\
         Rpt in x 100\nC1 x 0 1e-9\nR2 x out 10\nV2 out 0 dc 0 ac 0 portnum 2 z0 50 pwr 0.002\n\
         .sp lin 10 1e8 1e9\n.end\n",
        false,
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN (RFSPICE build); runs C out of process"]
fn a_series_resistor_matches_c_except_its_nonexistent_z_matrix() {
    compare(
        "series",
        "series r\nv1 1 0 dc 0 ac 1 portnum 1 z0 50\nr1 1 2 50\nv2 2 0 dc 0 ac 0 portnum 2 z0 50\n\
         .sp lin 3 1k 1meg\n.end\n",
        true,
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN (RFSPICE build); runs C out of process"]
fn port_sources_match_c_in_op_and_transient() {
    compare(
        "port_tran",
        "port transient\nv1 1 0 pulse(0 1 0 1u 1u 5u 20u) portnum 1 z0 50\nr1 1 2 50\n\
         c1 2 0 10n\nv2 2 0 dc 0 portnum 2 z0 75\n.tran 0.1u 20u\n.op\n.end\n",
        false,
    );
}
