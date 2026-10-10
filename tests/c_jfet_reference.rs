//! Opt-in live C comparison of JFET level 1 (#82) beyond the committed
//! `m10_jfet_*` goldens.
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_jfet_reference -- --ignored
//! ```
//!
//! * **Observations.** `.save @j[...]` of both polarities (`jfetask.c`: the
//!   multiplicity-scaled `gm`/`gds`/`ggs`/`ggd`/`igd`, the normalized
//!   `id`/`ig`/`is` and `vgs`/`vgd`, `area * m`, `temp`).
//! * **Instance-parameter sweeps.** `.dc @j1[area]` nested with `@j1[temp]`
//!   (`DCTsetInstParam` followed by `JFETtemp`).
//! * **Pole-zero.** `.pz` of a common-source stage (`jfetpzld.c`); the port's
//!   eigenvalue roots against C's Muller search, matched in C's order.
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
    let dir = std::env::temp_dir().join(format!("spice-jfet-{}-{tag}", std::process::id()));
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
fn saved_jfet_asks_match_c() {
    let (ours, c) = both(
        "observations",
        "jfet observations\n\
         vd d 0 4\nvg g 0 -0.5\nj1 d g 0 jm 3 m=2\n\
         .model jm njf(vto=-2 beta=1m lambda=0.02 rd=10 rs=5 b=0.8)\n\
         vdp dp 0 -3\nvgp gp 0 0.4\nj2 dp gp 0 jp 2\n\
         .model jp pjf(vto=-1.5 beta=1m lambda=0.02 rs=5)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc vd -1 4 1\n\
         .save v(d) @j1[gm] @j1[gds] @j1[ggs] @j1[ggd] @j1[id] @j1[ig] @j1[is] @j1[vgs] \
         @j1[vgd] @j1[igd] @j1[area] @j1[temp] @j2[gm] @j2[gds] @j2[id] @j2[ig] @j2[is] \
         @j2[vgs] @j2[vgd] @j2[igd] @j2[ggd]\n",
    );
    same(&ours, &c, 1e-15);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn instance_parameter_sweeps_match_c() {
    let (ours, c) = both(
        "sweep",
        "jfet instance sweep\n\
         vd d 0 3\nvg g 0 -0.7\nrl d dd 500\nj1 dd g 0 jm\n\
         .model jm njf(vto=-2 beta=1m lambda=0.02 rs=20 is=1e-13 xti=3 tcv=2m bex=-1.5)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc @j1[area] 0.5 2 0.5 @j1[temp] 0 100 50\n",
    );
    same(&ours, &c, 1e-14);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn pole_zero_roots_match_c() {
    let (ours, c) = both(
        "pz",
        "jfet pz\n\
         vdd vdd 0 15\nvin in 0 dc 0 ac 1\nrg in g 50k\nrl vdd d 4.7k\nrsrc s 0 1k\n\
         cs s 0 10u\nj1 d g s jn 1.5 m=2\n\
         .model jn njf(vto=-2.2 beta=0.9m lambda=0.015 rd=30 rs=25 cgs=4p cgd=1.5p pb=0.7 \
         fc=0.4 b=0.85)\n\
         .pz in 0 d 0 vol pz\n",
    );
    same(&ours, &c, 0.);
}
