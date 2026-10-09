//! Opt-in #100 cross-check of `.noise` against real C batch mode.
//!
//! `ngspice -b -r <file> <deck>` writes the noise plots exactly as `noisean.c`
//! creates them (binary, in natural vector order), which is the layout
//! `spice-rs simulate` reproduces. The committed goldens were captured with a
//! `.control` `write noise1.all noise2.all`, which lists each plot's vectors in
//! C's sorted order; this test ties the port to the batch layout itself: the
//! plot sequence, plot names, flags, every noise vector name **in order**,
//! units and values must match.
//!
//! It is `#[ignore]`d because it needs a built C `ngspice`. Run it with an
//! absolute `NGSPICE_BIN`:
//!
//! ```text
//! NGSPICE_BIN=/path/to/ngspice/build/src/ngspice \
//!   cargo test -p ngspice-rs --test c_noise_reference -- --ignored
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("the workspace root exists")
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs `deck_text` through C batch mode and `spice-rs simulate`; returns
/// (Rust, C) rawfiles.
fn run_both(name: &str, deck_text: &str) -> (RawFile, RawFile) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let directory =
        std::env::temp_dir().join(format!("spice-rs-c-noise-{}-{name}", std::process::id()));
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
    let want = RawFile::parse_bytes(&fs::read(directory.join("c.raw")).unwrap())
        .expect("the C batch rawfile parses");
    let got = RawFile::load(directory.join("rust.raw")).unwrap();
    (got, want)
}

fn is_noise(plot: &Plot) -> bool {
    plot.plotname.contains("Noise")
}

fn compare(name: &str, deck_text: &str) {
    let (got, want) = run_both(name, deck_text);
    let titles = |raw: &RawFile| -> Vec<String> {
        raw.plots.iter().map(|p| p.plot.plotname.clone()).collect()
    };
    assert_eq!(titles(&got), titles(&want), "{name}: plot sequence");
    for (index, (got, want)) in got.plots.iter().zip(&want.plots).enumerate() {
        let (got, want) = (&got.plot, &want.plot);
        assert_eq!(got.flags, want.flags, "{name}: plot {index}");
        assert_eq!(
            got.point_count(),
            want.point_count(),
            "{name}: plot {index}"
        );
        if is_noise(want) {
            // The noise layout is C's own: names, order and units.
            let layout = |plot: &Plot| -> Vec<(String, String)> {
                plot.variables
                    .iter()
                    .map(|v| (v.name.clone(), v.unit.clone()))
                    .collect()
            };
            assert_eq!(layout(got), layout(want), "{name}: plot {index}");
        }
        // Noise: the nonlinear 1 ppm bias bound with a floor far below any
        // physical density (see xtask compare::NOISE_NONLINEAR); other plots:
        // the nonlinear DC/AC bound. Values are compared by C's names (the
        // port writes simulator-internal nodes that C's batch plots omit).
        let floor = if is_noise(want) { 1e-20 } else { 1e-12 };
        for variable in &want.variables {
            let want_column = want.column(&variable.name).unwrap();
            let got_column = got
                .column(&variable.name)
                .unwrap_or_else(|| panic!("{name}: plot {index} lacks {}", variable.name));
            for (point, (a, b)) in got_column.iter().zip(&want_column).enumerate() {
                // C's batch writer leaves the AC scale's imaginary half unset.
                let difference = if variable.name == "frequency" {
                    (a.re - b.re).abs()
                } else {
                    (*a - *b).magnitude()
                };
                assert!(
                    difference <= 1e-6 * b.magnitude() + floor,
                    "{name}: plot {index} '{}' point {point}: {a} != {b}",
                    variable.name
                );
            }
        }
    }
}

fn fixture(name: &str) {
    let deck =
        fs::read_to_string(workspace().join(format!("conformance/netlists/{name}.cir"))).unwrap();
    compare(name, &deck);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn the_noise_fixtures_match_c_batch_mode() {
    for name in [
        "noise_rc",
        "noise_diode",
        "noise_bjt",
        "noise_mos1",
        "noise_multi",
    ] {
        fixture(name);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn mixed_devices_octave_sweeps_and_current_inputs_match_c() {
    compare(
        "mixed",
        "mixed noise
vcc vcc 0 5
v1 in 0 dc 0.7 ac 1
rb in b 10k
q1 c b e qn area=2
re e 0 100
rc vcc c 2k
d1 c x da m=2
d2 x 0 db
m1 out c 0 0 nm w=10u l=2u
rd vcc out 10k
.model qn npn(bf=100 rb=100 rbm=20 irb=1m rc=10 re=1 kf=1e-15 af=1.2 vaf=50 ise=1e-14)
.model da d(is=1e-14 rs=5 kf=1e-14 af=1.1)
.model db d(is=1e-15)
.model nm nmos(vto=1 kp=1e-4 rd=10 rs=5 kf=1e-24 af=1.3 tox=2e-8 lambda=0.02)
.option reltol=1e-6
.noise v(out) v1 oct 2 10 1meg 2
.end
",
    );
    compare(
        "pnp_current",
        "pnp current input
vee vee 0 -5
i1 0 b dc -10u ac 1
rbias b 0 100k
q1 c b e qp
re e 0 1k
rc c vee 2k
.model qp pnp(bf=50 rb=50 re=2 rc=5 kf=1e-16 af=1.1)
.option reltol=1e-6
.noise v(c) i1 dec 3 100 10meg 1
.end
",
    );
}
