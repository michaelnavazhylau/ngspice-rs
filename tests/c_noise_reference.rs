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
    if deck_text.contains("set ngbehavior=s3") {
        // newcompat is set before device setup in C, via its initialization file.
        fs::write(directory.join(".spiceinit"), "set ngbehavior=s3\n").unwrap();
    }
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
    // RF port sources (#105): C's VSRC has no noise routine, so the z0
    // terminations are noiseless; a port can be the input reference.
    compare(
        "ports",
        "port noise
v1 in 0 dc 0 ac 1 portnum 1 z0 50
r1 in out 100
r2 out 0 200
c1 out 0 1n
v2 out2 0 dc 0 ac 0 portnum 2 z0 75
r3 out out2 1k
.noise v(out) v1 dec 2 1k 10meg 1
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

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn equal_braced_noise_bounds_match_c() {
    compare(
        "braced_bounds",
        "noise expressions\n.param f0=1k\nv1 in 0 ac 1\nr1 in out 1k\nr2 out 0 1k\n.noise v(out) v1 dec 2 {f0} {2*f0/2}\n.noise v(out) v1 dec 2 1k 10k\n.end\n",
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn large_bjt_bypass_matches_c_at_high_frequency() {
    let deck = fs::read_to_string(workspace().join("conformance/netlists/noise_bjt.cir"))
        .unwrap()
        .replace("ce e 0 1n", "ce e 0 10u")
        .replace(
            ".noise v(c) i1 dec 4 10 100meg 3",
            ".ac dec 4 10meg 100meg\n.noise v(c) i1 dec 4 10meg 100meg 3",
        );
    compare("large_bypass", &deck);
}

#[test]
#[ignore = "requires NGSPICE_BIN"]
fn squared_noise_and_retained_bias_match_c() {
    for (name, input) in [
        ("voltage", "v1 in 0 ac 1\nr1 in out 1k\nr2 out 0 1k"),
        ("current", "i1 0 out ac 1\nr1 out 0 1k"),
    ] {
        compare(
            &format!("squared-{name}"),
            &format!(
                "squared noise\n{input}\n.options keepopinfo\n.noise v(out) {} dec 2 10 1k 1\n.control\nset sqrnoise\n.endc\n.end\n",
                if name == "voltage" { "v1" } else { "i1" }
            ),
        );
    }
}

/// MOS3 (#89): `mos3noi.c` on the shared MOS shell, every NLEV law, a PMOS
/// at its own TEMP, NFS/VMAX/CLM bias and XL/WD/XW geometry (C's flicker and
/// NLEV 3 laws use `W - 2 WD`, `L - 2 LD` and the drawn width), with and
/// without the SPICE3 flicker form.
const MOS3_NOISE: &str = "MOS3 noise
vdd vdd 0 5
v1 in 0 dc 1.6 ac 1
m1 out in 0 0 n0 w=10u l=2u
m2 out in vdd vdd p3 w=20u l=2u temp=50
m3 o2 in 0 0 n1 w=10u l=2u m=2
m4 o2 in 0 0 n3 w=5u l=1u
m5 o2 in 0 0 n2 w=4u l=1u
r1 vdd out 10k
r2 vdd o2 20k
.model n0 nmos(level=3 vto=1 kp=1e-4 kf=1e-24 af=1.3 nlev=0 tox=2e-8)
.model n1 nmos(level=3 vto=0.8 kf=1e-24 af=1.1 nlev=1 tox=1e-8 nsub=1e16 xj=0.2u kappa=0.4 ld=0.1u xl=0.05u)
.model n2 nmos(level=3 vto=0.9 kp=1e-4 kf=1e-25 af=0.9 rd=20 rs=10 tox=2e-8 cgso=1n cgdo=1n vmax=1e5 nfs=1e11 wd=0.2u xw=0.1u)
.model n3 nmos(level=3 vto=0.8 kp=1e-4 kf=1e-24 af=1.1 nlev=3 gdsnoi=2 eta=0.1 theta=0.05 wd=0.1u)
.model p3 pmos(level=3 vto=-1 kp=5e-5 kf=1e-24 nlev=3 rd=20 rs=10 nsub=1e16 xj=0.2u)
.option reltol=1e-7
.noise v(out,o2) v1 lin 6 1k 100k 1
.end
";

#[test]
#[ignore = "requires NGSPICE_BIN"]
fn mos3_noise_matches_c() {
    compare("mos3", MOS3_NOISE);
    compare(
        "mos3-spice3",
        &MOS3_NOISE.replace(".end\n", ".control\nset ngbehavior=s3\n.endc\n.end\n"),
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN"]
fn spice3_mos1_flicker_matches_c() {
    let deck = fs::read_to_string(workspace().join("conformance/netlists/noise_mos1.cir")).unwrap();
    compare(
        "mos1-spice3",
        &deck.replace(".end", ".control\nset ngbehavior=s3\n.endc\n.end"),
    );
}
