//! Opt-in #104 cross-check of `.disto` against real C batch mode.
//!
//! `ngspice -b -r <file> <deck>` writes every distortion plot exactly as
//! `distoan.c` creates them, which is the sequence `spice-rs simulate`
//! reproduces: the plot titles in batch order, the complex flags, the point
//! counts and every C vector's values, matched by name (the port also writes
//! simulator-internal nodes that C's batch plots omit, and C's binary writer
//! leaves the frequency scale's imaginary half unset).
//!
//! It is `#[ignore]`d because it needs a built C `ngspice`. Run it with an
//! absolute `NGSPICE_BIN`:
//!
//! ```text
//! NGSPICE_BIN=/path/to/ngspice/build/src/ngspice \
//!   cargo test -p ngspice-rs --test c_disto_reference -- --ignored
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
        std::env::temp_dir().join(format!("spice-rs-c-disto-{}-{name}", std::process::id()));
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

fn compare(name: &str, deck_text: &str) {
    let (got, want) = run_both(name, deck_text);
    let titles = |raw: &RawFile| -> Vec<String> {
        raw.plots.iter().map(|p| p.plot.plotname.clone()).collect()
    };
    assert_eq!(titles(&got), titles(&want), "{name}: plot sequence");
    for (index, (got, want)) in got.plots.iter().zip(&want.plots).enumerate() {
        let (got, want): (&Plot, &Plot) = (&got.plot, &want.plot);
        assert_eq!(got.flags, want.flags, "{name}: plot {index}");
        assert_eq!(
            got.point_count(),
            want.point_count(),
            "{name}: plot {index}"
        );
        // The nonlinear 1 ppm bias bound with xtask's compare::DISTORTION
        // floor for distortion plots, the nonlinear DC/AC bound otherwise.
        let floor = if want.plotname.starts_with("DISTORTION") {
            1e-18
        } else {
            1e-12
        };
        for variable in &want.variables {
            let want_column = want.column(&variable.name).unwrap();
            let got_column = got
                .column(&variable.name)
                .unwrap_or_else(|| panic!("{name}: plot {index} lacks {}", variable.name));
            assert_eq!(
                got.variables[got.variable_index(&variable.name).unwrap()].unit,
                variable.unit,
                "{name}: plot {index} '{}'",
                variable.name
            );
            for (point, (a, b)) in got_column.iter().zip(&want_column).enumerate() {
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
fn the_distortion_fixtures_match_c_batch_mode() {
    for name in ["disto_diode", "disto_bjt", "disto_mos1", "disto_multi"] {
        fixture(name);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn reverse_bias_inverse_mode_and_negative_difference_frequencies_match_c() {
    // A reverse-biased diode driven by a current input, an NMOS in inverse
    // mode, a BJT with RB/RBM and a substrate terminal, a zero-step linear
    // sweep, and f2 = 1.7 fstart above the first f1 (so f1 - f2 < 0).
    compare(
        "mixed",
        "mixed disto
vcc vcc 0 5
v1 in 0 dc 0 ac 1 distof1 0.02 distof2 0.01 90
i1 0 r dc 5u distof1 1u
rr r 0 100k
d1 0 r dvar
cin in g 100n
rg1 vcc g 200k
rg2 g 0 100k
m1 s g d 0 nm w=10u l=2u
rd vcc d 10k
rs s 0 2k
q1 c in2 e 0 qn
rin in in2 1k
vb in2x 0 0.75
rbx in2x in2 10k
rc vcc c 2k
re e 0 100
.model dvar d(is=1e-14 cjo=5p m=0.5 vj=0.7 tt=1n)
.model nm nmos(vto=0.7 kp=60u gamma=0.4 lambda=0.04 cbd=10f cbs=10f tox=20n cgso=1e-10 cgdo=1e-10)
.model qn npn(is=1e-15 bf=100 vaf=50 rb=50 rbm=10 cje=1p cjc=1p xcjc=0.7 tf=0.5n cjs=0.2p)
.option reltol=1e-7
.disto lin 0 10k 30k 1.7
.disto oct 1 1k 8k
.end
",
    );
    // A PNP with M, AREA and its own temperature, and an NPN without base
    // resistance and XCJC < 1 (bjtdisto.c's vbe + vbc B-C' kernel).
    compare(
        "pnp",
        "pnp disto
vee vee 0 -12
vin in 0 dc 0 ac 1 distof1 0.02 30 distof2 0.01
cin in b 10u
r1 vee b 100k
r2 b 0 20k
rc vee c 4.7k
re e 0 1k
q1 c b e 0 qm area=1.5 m=2 temp=50
q2 c2 b e2 qn
rc2 vee c2 2k
re2 e2 0 2k
.model qm pnp(is=1e-15 bf=150 vaf=60 ikf=20m ise=1e-14 br=3 rb=100 re=1 rc=10 cje=2p mje=0.4 tf=0.4n xtf=2 itf=50m cjc=1p xcjc=0.6 cjs=0.5p)
.model qn pnp(is=2e-15 bf=80 cjc=2p xcjc=0.5 cje=1p tf=1n xtf=1 vtf=2)
.option reltol=1e-7
.disto oct 2 1k 64k 0.85
.disto lin 3 10k 100k
.end
",
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN"]
fn distortion_retained_bias_matches_c() {
    let deck =
        fs::read_to_string(workspace().join("conformance/netlists/disto_diode.cir")).unwrap();
    compare(
        "disto-keepopinfo",
        &deck.replace(".end", ".options keepopinfo\n.end"),
    );
}
