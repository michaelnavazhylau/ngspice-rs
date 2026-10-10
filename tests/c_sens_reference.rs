//! Opt-in live C validation of `.sens` (#102) beyond the committed `sens_*`
//! goldens.
//!
//! Each deck runs through `ngspice -b -r` (C's batch mode, binary rawfile,
//! whose vectors keep `sens_sens()`'s creation order) and through `spice-rs
//! simulate`. Plot count, plot names, flags, the variable names **in order**
//! with their units and every value must agree: linear decks within
//! `1e-8 |C| + 1e-12` (`xtask::compare::SENSITIVITY`), nonlinear decks within
//! `1e-6 |C| + 1e-9` (`SENSITIVITY_NONLINEAR`); C's NaN must be NaN.
//!
//! `#[ignore]`d because it needs a built C `ngspice`:
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice cargo test -p ngspice-rs --test c_sens_reference -- --ignored
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{RawFile, RawFormat};

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const LINEAR: (f64, f64) = (1e-8, 1e-12);
const NONLINEAR: (f64, f64) = (1e-6, 1e-9);

fn compare_with_c(name: &str, deck_text: &str, (relative, absolute): (f64, f64)) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let directory =
        std::env::temp_dir().join(format!("spice-rs-c-sens-{}-{name}", std::process::id()));
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
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    assert!(c.status.success(), "{name}: ngspice failed:\n{log}");
    assert!(
        !log.contains("aborted") && !log.contains("Error"),
        "{name}: ngspice reported a problem:\n{log}"
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
        if want.plotname != "Sensitivity Analysis" {
            continue;
        }
        let names = |plot: &ngspice_rs::analysis::Plot| -> Vec<(String, String)> {
            plot.variables
                .iter()
                .map(|variable| (variable.name.clone(), variable.unit.clone()))
                .collect()
        };
        assert_eq!(
            names(got),
            names(want),
            "{name}: plot {index} names in order"
        );
        assert_eq!(got.point_count(), want.point_count(), "{name}");
        for (point, (a, b)) in got.points.iter().zip(&want.points).enumerate() {
            for ((x, y), variable) in a.iter().zip(b).zip(&want.variables) {
                if y.re.is_nan() || y.im.is_nan() {
                    assert!(
                        x.re.is_nan(),
                        "{name}: '{}' point {point}: C NaN, Rust {x}",
                        variable.name
                    );
                    continue;
                }
                let difference = (*x - *y).magnitude();
                assert!(
                    difference <= relative * y.magnitude() + absolute,
                    "{name}: '{}' point {point}: Rust {x} != C {y}",
                    variable.name
                );
            }
        }
    }
}

/// Literal and model-backed resistors, V/I sources and E/F/G/H, with the
/// sticky `m` of G/F sources and a filtered parameter list.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn linear_dc_decks_match_c() {
    compare_with_c(
        "linear",
        "\
sens linear
v1 in 0 dc 5
i1 0 n2 dc 1m
r1 in a 1k
r2 a 0 rm 2k
.model rm r tc1=1e-3 tc2=1e-6
c1 a 0 1n
l1 a b 1u
rb b 0 3k
l2 n2 0 1u
rl2 n2 0 1k
k1 l1 l2 0.5
e1 e 0 a 0 2
re e 0 1k
g1 0 g a 0 1m m=3
rg g 0 1k
f1 0 f v1 2 m=2
rf f 0 1k
h1 h 0 v1 100
rh h 0 1k
.sens v(a)
.end
",
        LINEAR,
    );
    compare_with_c(
        "filtered",
        "\
sens filtered
v1 in 0 dc 5
r1 in a 1k
r2 a 0 2k
r3 a 0 3k
.sens v(a) r?_m *:tc1 v1 dc
.end
",
        LINEAR,
    );
}

/// Away from TNOM: TC1/TC2, a geometry resistor whose resistance follows a
/// sheet resistance, a model `r` perturbed from 0, a PULSE source without a
/// DC value, a B source and S/W switches, read through a current output.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn hot_and_behavioural_decks_match_c() {
    compare_with_c(
        "hot",
        "\
sens hot
.options temp=60
v1 in 0 pulse(2 5 1u 1u 1u 10u 20u)
vm in2 0 dc 1
r0 in2 x 500
r1 in a 1k
r2 a 0 rm 2k m=2 scale=1.5 tc1=2e-3
r3 a c rm l=20u w=2u
r4 c 0 rm2 temp=80
.model rm r tc1=1e-3 tc2=1e-6 rsh=100 tnom=20
.model rm2 r r=3k tc1=-1e-3
e1 x 0 c 0 0.5
b1 bb 0 i=v(a)*1m tc1=1e-3 m=2
rbb bb a 1k
s1 a s a 0 swm
rs s 0 1k
.model swm sw vt=1 vh=0.1 ron=10 roff=1meg
w1 a w vm cswm
rw w 0 1k
.model cswm csw it=1m ih=0.1m ron=10 roff=1meg
.sens i(vm)
.end
",
        NONLINEAR,
    );
}

/// Diodes: an unset IKF (C's NaN), ISR/JTUN turned on by their own
/// perturbation, sidewall current, a reverse-biased diode in breakdown and
/// `.op` composition.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn diode_decks_match_c() {
    compare_with_c(
        "diode",
        "\
sens diode
.options reltol=1e-9
v1 in 0 dc 5
r1 in a 1k
d1 a 0 dmod
.model dmod d is=1e-14 rs=10 n=1.5
.op
.sens v(a)
.end
",
        NONLINEAR,
    );
    compare_with_c(
        "diodes",
        "\
sens diodes
.options reltol=1e-9 temp=50
v1 in 0 dc 5
r1 in a 1k
d1 a 0 dmod area=2
d2 a b dmod2 m=2
rb b 0 500
d3 0 c dmod
i3 0 c dc -1m
rc c 0 10k
.model dmod d is=1e-14 rs=10 n=1.5 ikf=0.1 bv=10 isr=1e-12 jtun=1e-15 tnom=40 trs=1e-3 xti=2
.model dmod2 d is=1e-15 jsw=1e-16 pj=2 ns=1.2 eg=1.2 tlev=1 ikr=1m ikp=1m
.sens v(c)
.end
",
        NONLINEAR,
    );
}

/// AC: literal and model-backed R/C/L, AC phasors, controlled sources, and
/// C's `lin` stepping (which multiplies by the step).
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn ac_decks_match_c() {
    compare_with_c(
        "ac",
        "\
sens ac
.options temp=40
v1 in 0 dc 1 ac 1 30
i1 0 n2 ac 0.5
r1 in a 1k
c1 a 0 1n
r2 a b rm 2k tc1=1e-3
.model rm r tc1=1e-3
l1 b 0 10u
c2 b 0 cm 2n m=2 scale=1.2
.model cm c tc1=1e-3 tnom=30
l2 n2 0 lm 1m
.model lm l tc1=2e-3
rl2 n2 0 1k
e1 e 0 a 0 2
re e 0 1k
g1 0 g a 0 1m m=3
rg g b 1k
f1 0 f v1 2
rf f b 1k
.sens v(a,b) ac oct 2 1k 16k
.end
",
        LINEAR,
    );
    compare_with_c(
        "ac-lin",
        "\
sens ac lin
v1 in 0 dc 1 ac 1
r1 in out 1k
c1 out 0 1n
.sens i(v1) ac lin 3 10 100
.end
",
        LINEAR,
    );
}
