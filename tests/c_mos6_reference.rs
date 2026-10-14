//! Opt-in live C comparison of MOS level 6 (#89), beyond the committed
//! `m10_mos6_*` goldens.
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_mos6_reference -- --ignored
//! ```
//!
//! * **Observations.** `.save @m[id/ig/is/ib]` through a drain sweep that
//!   crosses into inverse mode, for an NMOS with `M = 3` (C's `M`-scaled
//!   asks and per-unit `gmin`), RD and RSH with explicit NRS, and the
//!   supply current of a PMOS. C's PMOS asks are polarity-normalized
//!   (`mos6ask.c` returns `MOS6cd` without the type sign) while the shell
//!   reports physical terminal currents for every MOS level, so PMOS asks
//!   are not compared here.
//! * **Instance-parameter sweeps.** `.dc @m1[w]` nested with `@m1[temp]`
//!   (`DCTsetInstParam` followed by `MOS6temp`).
//! * **`uic` transient.** Instance `ic=` with `uic` and Meyer/junction
//!   charge.
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
    let dir = std::env::temp_dir().join(format!("spice-mos6-{}-{tag}", std::process::id()));
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
fn saved_mos6_terminal_currents_match_c() {
    let (ours, c) = both(
        "observations",
        "mos6 observations\n\
         vd d 0 3\nvg g 0 2\nvb b 0 -0.5\nm1 d g 0 b mn w=6u l=1u m=3 nrs=2\n\
         .model mn nmos(level=6 vto=0.7 kc=6e-5 kv=1.4 nv=0.8 nc=1.3 gamma=0.5 phi=0.7 \
         gamma1=0.03 sigma=0.02 lambda0=0.04 lambda1=0.01 rd=40 rsh=25)\n\
         vdp dp 0 -2\nvgp gp 0 -2.2\nvbp bp 0 0.2\nm2 dp gp 0 bp mp w=10u l=1u ad=10p as=10p\n\
         .model mp pmos(level=6 vto=-0.8 tox=20n u0=200 kv=2 nv=0.6 nc=1.2 lambda=0.03 \
         js=1e-5)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-18\n\
         .dc vd -1 4 0.25\n\
         .save v(d) i(vdp) @m1[id] @m1[ig] @m1[is] @m1[ib]\n",
    );
    same(&ours, &c, 1e-15);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn instance_parameter_sweeps_match_c() {
    let (ours, c) = both(
        "sweep",
        "mos6 instance sweep\n\
         vdd vdd 0 3\nvg g 0 1.8\nrl vdd d 2k\nm1 d g 0 0 mn l=1u\n\
         .model mn nmos(level=6 vto=0.6 tox=15n u0=400 kv=1.5 nv=0.7 nc=1.25 gamma=0.4 \
         lambda0=0.03 rs=15)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc @m1[w] 2u 10u 2u @m1[temp] 0 100 50\n",
    );
    same(&ours, &c, 1e-14);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn uic_transient_matches_c() {
    let (ours, c) = both(
        "uic",
        "mos6 uic\n\
         vdd vdd 0 3\nrl vdd d 5k\nrg in g 20k\nvin in 0 1.5\ncl d 0 50f\n\
         m1 d g 0 0 mn w=4u l=0.5u ad=4p as=4p pd=10u ps=10u ic=3,0\n\
         .model mn nmos(level=6 vto=0.6 tox=10n u0=400 kv=1.2 nv=0.7 nc=1.3 \
         lambda0=0.04 cgso=0.3n cgdo=0.3n cgbo=0.1n cj=0.8m cjsw=0.3n)\n\
         .options reltol=1e-6 vntol=1e-11 abstol=1e-15\n\
         .tran 0.05n 10n 0 0.01n uic\n",
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
