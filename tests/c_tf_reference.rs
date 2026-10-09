//! Opt-in live C validation of `.tf` (#101) beyond the committed `m8_tf_*`
//! goldens.
//!
//! Each deck runs through `ngspice -b -r` (C's batch mode, binary rawfile, in
//! which the vector names keep `TFanal()`'s spelling, e.g.
//! `v(Transfer_function)`) and through `spice-rs simulate`. Plot count, order,
//! plot names, flags, **exact** variable names (case included) and every value
//! must agree: linear decks at `1e-9 |C| + 1e-12`, nonlinear decks (which set
//! `.options reltol=1e-8` so that C's Newton stopping error and its
//! one-iterate-old Jacobian stay far below the bound) at `1e-6 |C| + 1e-12`,
//! the `xtask::compare::NONLINEAR` bound.
//!
//! `#[ignore]`d because it needs a built C `ngspice`:
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice cargo test -p ngspice-rs --test c_tf_reference -- --ignored
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

const LINEAR: f64 = 1e-9;
const NONLINEAR: f64 = 1e-6;

fn compare_with_c(name: &str, deck_text: &str, relative: f64) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let directory =
        std::env::temp_dir().join(format!("spice-rs-c-tf-{}-{name}", std::process::id()));
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
        !log.contains("aborted") && !log.to_ascii_lowercase().contains("error"),
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
        // C's default save set omits simulator-created internal nodes (series
        // resistances), which the port's `.op`/`.ac` plots carry as
        // `v(dev#node)`; the `.tf` plot has no such rows and is compared whole.
        let transfer = want.plotname == "Transfer Function";
        let names = |plot: &ngspice_rs::analysis::Plot| -> Vec<(String, String)> {
            let mut names: Vec<_> = plot
                .variables
                .iter()
                .filter(|variable| transfer || !variable.name.contains('#'))
                .map(|variable| (variable.name.clone(), variable.unit.clone()))
                .collect();
            names.sort();
            names
        };
        assert_eq!(names(got), names(want), "{name}: plot {index}");
        assert_eq!(
            got.point_count(),
            want.point_count(),
            "{name}: plot {index}"
        );
        for variable in &want.variables {
            let want_column = want.column(&variable.name).unwrap();
            let got_column = got.column(&variable.name).unwrap();
            for (point, (a, b)) in got_column.iter().zip(&want_column).enumerate() {
                let difference = (*a - *b).magnitude();
                assert!(
                    difference <= relative * b.magnitude() + 1e-12,
                    "{name}: plot {index} '{}' point {point}: Rust {a} != C {b}",
                    variable.name
                );
            }
        }
    }
}

/// A V-driven divider with an inductor (a DC short) and a capacitor (open):
/// voltage, differential and current outputs, the same-source current output
/// (C copies the input resistance) and an output across a ground-aliased node.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn passive_networks_match_c() {
    compare_with_c(
        "passive",
        "\
tf passive network
v1 in gnd dc 5 ac 1
r1 in a 1k
l1 a b 1m
r2 b 0 3k
r3 b out 2k
r4 out 0 6k
c1 out 0 1u
.tf v(out) v1
.tf V(A, Out) v1
.tf i(v1) v1
.tf v(out,gnd) v1
.end
",
        LINEAR,
    );
}

/// A current-source input (with a parallel conductance so every output sees a
/// finite resistance), current outputs through a sensing V source and an E
/// branch, and the C sign of the current-source input resistance.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn current_inputs_and_current_outputs_match_c() {
    compare_with_c(
        "current",
        "\
tf current source input
i1 0 in dc 1m
rp in 0 5k
r1 in out 1k
r2 out mid 3k
vm mid 0 0
e1 o2 0 out 0 5
ro o2 x 1k
vx x 0 0
i2 n2 0 dc 2m
r5 n2 0 2k
.tf i(vm) i1
.tf i(e1) i1
.tf v(o2) vm
.tf i(vx) vm
.tf v(in, out) i1
.tf v(n2) i2
.end
",
        LINEAR,
    );
}

/// E/F/G/H amplifiers: a finite-gain non-inverting E amplifier with output
/// resistance, a G transconductor, F and H current-controlled stages.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn controlled_source_amplifiers_match_c() {
    compare_with_c(
        "controlled",
        "\
tf controlled sources
vin in 0 dc 1
rs in p 1k
rin p n 100k
e1 oi 0 p n 1e4
rout oi out 75
r1 n 0 1k
r2 out n 9k
rl out 0 10k
g1 0 o2 out 0 2m
rg o2 0 1k
vs o2 o3 0
r3 o3 0 4k
f1 0 o4 vs 3
r4 o4 0 1k
h1 o5 0 vs 500
r5 o5 0 2k
.tf v(out) vin
.tf v(o4) vin
.tf i(h1) vin
.tf v(o5, o4) vin
.end
",
        LINEAR,
    );
}

/// Nonlinear bias points: a diode, a Gummel-Poon CE amplifier whose base
/// resistance depends on bias (RB/RBM/IRB: C's `bjtload.c` matrix stamps
/// `gx` only, which `.tf` reproduces) and a MOS1 common-source stage, in one
/// deck with `.op` and `.ac` (batch order and plot naming), so every `.tf`
/// linearises at its own operating point.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn nonlinear_bias_points_match_c() {
    compare_with_c(
        "nonlinear",
        "\
tf nonlinear bias
vd vdin 0 dc 2
rd vdin d 1k
d1 d 0 dmod
vin in 0 dc 1.4 ac 1
vcc vcc 0 9
rs in nb 600
rb1 vcc nb 82k
rb2 nb 0 15k
rc vcc nc 3.3k
re ne 0 330
q1 nc nb ne 0 qamp
vg g 0 dc 2
vdd vdd 0 5
rdm vdd dm 10k
m1 dm g 0 0 nmos w=10u l=2u
.model dmod d(is=1e-14 n=1.2 rs=5)
.model qamp npn(is=5e-16 bf=180 br=5 vaf=70 var=15 ikf=40m ikr=4m ise=1e-14 ne=1.5 isc=2e-14 nc=1.8 rb=120 rbm=12 irb=0.5m re=0.8 rc=15)
.model nmos nmos(level=1 vto=0.8 kp=60u lambda=0.02 gamma=0.4)
.options reltol=1e-8
.tf v(d) vd
.tf v(nc) vin
.tf i(vcc) vin
.tf v(dm) vg
.op
.ac dec 2 10 1k
.end
",
        NONLINEAR,
    );
}

/// Switches stamp their operating-point state with `swload.c`'s rules (on
/// only for REALLY_ON/HYST_ON), not `SWacLoad`'s (any non-zero code): a
/// control voltage inside the hysteresis band, with and without the ON/OFF
/// initial flags, and a W switch.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs C batch mode out of process"]
fn switch_states_match_c() {
    for (tag, flag) in [("band", ""), ("band_on", "on"), ("band_off", "off")] {
        compare_with_c(
            tag,
            &format!(
                "\
tf switch in its hysteresis band
v1 in 0 dc 1
vc c 0 dc 1
r1 in out 1k
s1 out 0 c 0 smod {flag}
r2 out 0 2k
vw w 0 dc 1
rw w x 1k
vs x 0 0
w1 out2 0 vs wmod
r3 in out2 1k
.model smod sw(vt=1 vh=0.5 ron=10 roff=1meg)
.model wmod csw(it=1m ih=0.2m ron=5 roff=100k)
.tf v(out) v1
.tf v(out2) v1
.tf i(vs) vw
.end
"
            ),
            NONLINEAR,
        );
    }
}
