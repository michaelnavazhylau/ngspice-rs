//! Opt-in live C comparison of JFET level 2, Parker-Skellern (#82 part 2),
//! beyond the committed `m10_jfet2_*` goldens.
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_jfet2_reference -- --ignored
//! ```
//!
//! * **Observations.** `.save @j[...]` of both polarities (`jfet2ask.c`:
//!   the multiplicity-scaled `gm`/`gds`/`ggs`/`ggd`/`igd`, the normalized
//!   `id`/`ig`/`is`, `vgs`/`vgd`, the DC filter state `vtrap`/`vpave`,
//!   `area * m`, `temp`), including inverse mode and breakdown.
//! * **Instance-parameter sweeps.** `.dc @j1[area]` nested with `@j1[temp]`
//!   (`DCTsetInstParam` followed by `JFET2temp`).
//! * **`uic` transient.** Instance `ic=` with `uic`: the initial load at the
//!   initial conditions stores the total Statz charge (`PScharge` outside
//!   transient), which then drives the incremental charge and truncation.
//!
//! Ordinary `cargo test` never needs C: everything here is `#[ignore]`d.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile};
use ngspice_rs::cli::simulate;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> (PathBuf, Cleanup) {
    let dir = std::env::temp_dir().join(format!("spice-jfet2-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    (dir.clone(), Cleanup(dir))
}

/// Runs `deck` (title, cards and analysis, no `.end`) in C and in the port's
/// `simulate` front end; returns (port, C) plots.
fn both(tag: &str, deck: &str) -> (Plot, Plot) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let (dir, _cleanup) = scratch(tag);
    fs::write(
        dir.join("c.cir"),
        format!("{deck}.control\nset filetype=ascii\nrun\nwrite c.raw\nquit\n.endc\n.end\n"),
    )
    .unwrap();
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
    assert!(!text.contains("failed"), "{text}");
    let c = fs::read_to_string(dir.join("c.raw")).unwrap_or_else(|_| panic!("{text}"));
    let c = RawFile::parse(&c).unwrap().plots[0].plot.clone();
    fs::write(dir.join("r.cir"), format!("{deck}.end\n")).unwrap();
    simulate::run(&dir.join("r.cir"), &dir.join("r.raw"), true).unwrap();
    let ours = RawFile::load(dir.join("r.raw")).unwrap().plots[0]
        .plot
        .clone();
    (ours, c)
}

/// Every port column (internal nodes excepted) within `1e-6 |C| + absolute`,
/// matched by name through C's `i(@...)`/`v(@...)`/`#branch` spellings.
fn same(ours: &Plot, c: &Plot, absolute: f64) {
    assert_eq!(ours.points.len(), c.points.len());
    for (i, variable) in ours.variables.iter().enumerate() {
        let name = variable.name.as_str();
        if name.contains('#') || name.starts_with("sweep") {
            continue;
        }
        let candidates = [
            name.to_owned(),
            format!("i({name})"),
            format!("v({name})"),
            name.strip_prefix("i(")
                .and_then(|n| n.strip_suffix(')'))
                .map_or_else(String::new, |n| format!("{n}#branch")),
        ];
        let k = candidates
            .iter()
            .find_map(|n| c.variable_index(n))
            .unwrap_or_else(|| panic!("C has no {name}: {:?}", c.variables));
        for (point, (a, b)) in ours.points.iter().zip(&c.points).enumerate() {
            let (x, y) = (a[i], b[k]);
            let error = ((x.re - y.re).powi(2) + (x.im - y.im).powi(2)).sqrt();
            let scale = (y.re * y.re + y.im * y.im).sqrt();
            assert!(
                error <= 1e-6 * scale + absolute,
                "{name}[{point}]: port {x:?}, C {y:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn saved_jfet2_asks_match_c() {
    let (ours, c) = both(
        "observations",
        "jfet2 observations\n\
         vd d 0 4\nvg g 0 -0.5\nj1 d g 0 jm 3 m=2\n\
         .model jm njf(level=2 vto=-2 beta=1m lambda=0.02 rd=10 rs=5 vst=0.06 mvst=0.1 \
         delta=0.2 lfgam=0.04 lfg1=0.01 lfg2=0.005 xi=10 z=0.6 p=2.2 q=2.1 ibd=1n vbd=1.2)\n\
         vdp dp 0 -3\nvgp gp 0 0.4\nj2 dp gp 0 jp 2\n\
         .model jp pjf(level=2 vto=-1.5 beta=1m lambda=0.02 rs=5 delta=0.1)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc vd -1 6 1\n\
         .save v(d) @j1[gm] @j1[gds] @j1[ggs] @j1[ggd] @j1[id] @j1[ig] @j1[is] @j1[vgs] \
         @j1[vgd] @j1[igd] @j1[vtrap] @j1[vpave] @j1[area] @j1[temp] @j2[gm] @j2[gds] \
         @j2[id] @j2[ig] @j2[is] @j2[vgs] @j2[vgd] @j2[igd] @j2[ggd] @j2[vtrap] @j2[vpave]\n",
    );
    same(&ours, &c, 1e-15);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn instance_parameter_sweeps_match_c() {
    let (ours, c) = both(
        "sweep",
        "jfet2 instance sweep\n\
         vd d 0 3\nvg g 0 -0.7\nrl d dd 500\nj1 dd g 0 jm\n\
         .model jm njf(level=2 vto=-2 beta=1m lambda=0.02 rs=20 is=1e-13 delta=0.3 vst=0.05)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc @j1[area] 0.5 2 0.5 @j1[temp] 0 100 50\n",
    );
    same(&ours, &c, 1e-14);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn uic_transient_matches_c() {
    let (ours, c) = both(
        "uic",
        "jfet2 uic\n\
         vdd vdd 0 8\nrl vdd d 2k\nrg g 0 50k\ncg g 0 1p\nj1 d g 0 jm ic=2,-1\n\
         .model jm njf(level=2 vto=-2 beta=1m cgs=3p cgd=1p cds=0.2p taug=20n taud=40n \
         delta=0.2 lfgam=0.03)\n\
         .options reltol=1e-6 vntol=1e-11 abstol=1e-15\n\
         .tran 0.5n 200n 0 0.1n uic\n",
    );
    // Both engines take the same step sequence here (no breakpoints after
    // t = 0); compare point by point under `compare::TRAN`'s bound
    // (1e-3 relative plus vntol/abstol floors).
    assert_eq!(ours.points.len(), c.points.len());
    for (i, variable) in ours.variables.iter().enumerate() {
        let name = variable.name.as_str();
        if name.contains('#') {
            continue;
        }
        let k = [
            name.to_owned(),
            name.strip_prefix("i(")
                .and_then(|n| n.strip_suffix(')'))
                .map_or_else(String::new, |n| format!("{n}#branch")),
        ]
        .iter()
        .find_map(|n| c.variable_index(n))
        .unwrap_or_else(|| panic!("C has no {name}"));
        let floor = if name.starts_with("i(") { 1e-12 } else { 1e-6 };
        for (point, (a, b)) in ours.points.iter().zip(&c.points).enumerate() {
            let (x, y) = (a[i].re, b[k].re);
            assert!(
                (x - y).abs() <= 1e-3 * y.abs() + floor,
                "{name}[{point}]: port {x}, C {y}"
            );
        }
    }
}
