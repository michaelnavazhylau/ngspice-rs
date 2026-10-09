//! Opt-in #96 cross-check of multi-analysis decks against real C batch mode.
//!
//! `ngspice -b -r <file> <deck>` is exactly the mode `spice-rs simulate`
//! reproduces: every analysis card runs in `CKTdoJob()` order and each result is
//! appended to one rawfile (binary, ngspice's default filetype). This test runs
//! the C binary that way, runs `spice-rs simulate` on the same deck, and
//! requires the same plot count, plot order, plot names, flags, variable names
//! and values. The committed golden `multi_analysis_rc.raw` was captured with a
//! `.control` `write` instead (so that it is ASCII and reviewable); this test is
//! what ties that capture route to genuine batch-mode output.
//!
//! It is `#[ignore]`d because it needs a built C `ngspice`. Run it with an
//! absolute `NGSPICE_BIN`:
//!
//! ```text
//! NGSPICE_BIN=/path/to/ngspice/build/src/ngspice \
//!   cargo test -p spice-cli --test c_batch_reference -- --ignored
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use spice_analysis::{RawFile, RawFormat};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root exists")
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A deck with two `.dc` cards (same-type order) besides `.tran` and `.op`.
const SAME_TYPE: &str = "\
two sweeps
v1 in 0 dc 1 pulse(0 1 100u 10u 10u 200u 1m)
r1 in out 1k
r2 out 0 1k
c1 out 0 100n
.tran 10u 500u
.dc v1 0 1 0.5
.op
.dc v1 0 4 2
.end
";

fn compare_with_c_batch(name: &str, deck_text: &str) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let directory =
        std::env::temp_dir().join(format!("spice-rs-c-batch-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let _cleanup = Cleanup(directory.clone());
    let deck = directory.join("deck.cir");
    fs::write(&deck, deck_text).unwrap();

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
        .arg(&deck)
        .output()
        .expect("spice-rs runs");
    assert!(
        rust.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&rust.stderr)
    );

    let bytes = fs::read(directory.join("c.raw")).unwrap();
    assert_eq!(RawFormat::detect(&bytes).unwrap(), RawFormat::Binary);
    let want = RawFile::parse_bytes(&bytes).expect("the C batch rawfile parses");
    let got = RawFile::load(directory.join("rust.raw")).unwrap();
    assert_eq!(got.len(), want.len(), "{name}: plot count");
    for (index, (got, want)) in got.plots.iter().zip(&want.plots).enumerate() {
        let (got, want) = (&got.plot, &want.plot);
        assert_eq!(got.plotname, want.plotname, "{name}: plot {index}");
        assert_eq!(got.flags, want.flags, "{name}: plot {index}");
        let names = |plot: &spice_analysis::Plot| -> Vec<String> {
            plot.variables
                .iter()
                .map(|variable| match variable.name.as_str() {
                    // The port's DC scale name; C wraps the source name.
                    "sweep" => "v(v-sweep)".to_owned(),
                    other => other.to_owned(),
                })
                .collect()
        };
        assert_eq!(names(got), names(want), "{name}: plot {index}");
        assert_eq!(
            got.point_count(),
            want.point_count(),
            "{name}: plot {index} ({})",
            got.plotname
        );
        for (column, variable) in got.variables.iter().enumerate() {
            let want_name = &names(want)[column];
            let want_column = want.column(&want.variables[column].name).unwrap();
            let got_column = got.column(&variable.name).unwrap();
            for (point, (a, b)) in got_column.iter().zip(&want_column).enumerate() {
                // C's incremental batch writer leaves the AC scale's imaginary
                // half uninitialised, so the scale is compared by real part.
                let difference = if column == 0 && want_name == "frequency" {
                    (a.re - b.re).abs()
                } else {
                    (*a - *b).magnitude()
                };
                assert!(
                    difference <= 1e-9 * b.magnitude() + 1e-12,
                    "{name}: plot {index} '{want_name}' point {point}: {a} != {b}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn the_multi_analysis_fixture_matches_c_batch_mode() {
    let deck =
        fs::read_to_string(workspace().join("conformance/netlists/multi_analysis_rc.cir")).unwrap();
    compare_with_c_batch("multi_analysis_rc", &deck);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn same_type_analyses_match_c_batch_order() {
    compare_with_c_batch("same_type", SAME_TYPE);
}
