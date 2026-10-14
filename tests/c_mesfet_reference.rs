//! Opt-in live C comparison of MESFET level 1 (#83) beyond the committed
//! `m10_mesfet_*` goldens.
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_mesfet_reference -- --ignored
//! ```
//!
//! * **Observations.** `.save @z[...]` of both polarities (`mesask.c`: the
//!   multiplicity-scaled `gm`/`gds`/`ggs`/`ggd`/`cgd`, the normalized
//!   `cd`/`cg`/`is` and `vgs`/`vgd`, `area * m`).
//! * **Instance-parameter sweeps.** `.dc @z1[area]` nested with `@z1[m]`
//!   (`DCTsetInstParam`; MES has no temperature dependence).
//! * **Pole-zero.** `.pz` of a common-source stage (`mespzld.c`, `m = 1`
//!   where C's load is consistent); the port's eigenvalue roots against C's
//!   Muller search, matched in C's order.
//! * **Noise.** `.noise` spectral densities of every `mesnoise.c` generator.
//! * **`uic` and `off`.** A transient from instance initial conditions with an
//!   `off` device (`mesgetic.c`, `MODEINITJCT`), on the output grid.
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
    let dir = std::env::temp_dir().join(format!("spice-mesfet-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    (dir.clone(), Cleanup(dir))
}

/// Runs `deck` (title, cards and analysis, no `.end`) in C and in the port's
/// `simulate` front end; returns the (port, C) plots named `plotname`. C
/// writes the plot `setplot` selects (`None`: the current one).
fn both(tag: &str, deck: &str, setplot: Option<&str>, plotname: &str) -> (Plot, Plot) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let (dir, _cleanup) = scratch(tag);
    let select = setplot.map_or_else(String::new, |plot| format!("setplot {plot}\n"));
    fs::write(
        dir.join("c.cir"),
        format!(
            "{deck}.control\nset filetype=ascii\nrun\n{select}write c.raw\nquit\n.endc\n.end\n"
        ),
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
    let pick = |raw: RawFile| {
        raw.plots
            .into_iter()
            .map(|p| p.plot)
            .find(|p| p.plotname == plotname)
            .unwrap_or_else(|| panic!("no {plotname} plot"))
    };
    let c = pick(RawFile::parse(&c).unwrap());
    fs::write(dir.join("r.cir"), format!("{deck}.end\n")).unwrap();
    simulate::run(&dir.join("r.cir"), &dir.join("r.raw"), true).unwrap();
    let ours = pick(RawFile::load(dir.join("r.raw")).unwrap());
    (ours, c)
}

/// Every port column (internal nodes excepted) within `relative |C| +
/// absolute`, matched by name through C's `i(@...)`/`v(@...)`/`#branch`
/// spellings.
fn same(ours: &Plot, c: &Plot, relative: f64, absolute: f64) {
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
                error <= relative * scale + absolute,
                "{name}[{point}]: port {x:?}, C {y:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn saved_mesfet_asks_match_c() {
    let (ours, c) = both(
        "observations",
        "mesfet observations\n\
         vd d 0 2\nvg g 0 -0.5\nz1 d g 0 zm area=2 m=3\n\
         .model zm nmf(vto=-2 beta=1m lambda=0.02 b=0.4 alpha=2.5 rd=10 rs=5)\n\
         vdp dp 0 -3\nvgp gp 0 0.4\nz2 dp gp 0 zp area=2\n\
         .model zp pmf(vto=-1.5 beta=1m lambda=0.02 rs=5)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc vd -1 3 1\n\
         .save v(d) @z1[cd] @z1[cg] @z1[is] @z1[cgd] @z1[gm] @z1[gds] @z1[ggs] @z1[ggd] \
         @z1[vgs] @z1[vgd] @z1[area] @z2[cd] @z2[cg] @z2[is] @z2[gm] @z2[gds] @z2[vgs] \
         @z2[vgd] @z2[cgd] @z2[ggd]\n",
        None,
        "DC transfer characteristic",
    );
    same(&ours, &c, 1e-6, 1e-15);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn instance_parameter_sweeps_match_c() {
    let (ours, c) = both(
        "sweep",
        "mesfet instance sweep\n\
         vd d 0 3\nvg g 0 -0.7\nrl d dd 500\nz1 dd g 0 zm\n\
         .model zm nmf(vto=-2 beta=1m lambda=0.02 rs=20 is=1e-13)\n\
         .options reltol=1e-7 vntol=1e-12 abstol=1e-15\n\
         .dc @z1[area] 0.5 2 0.5 @z1[m] 1 3 1\n",
        None,
        "DC transfer characteristic",
    );
    same(&ours, &c, 1e-6, 1e-14);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn pole_zero_roots_match_c() {
    let (ours, c) = both(
        "pz",
        "mesfet pz\n\
         vdd vdd 0 8\nvin in 0 dc -0.6 ac 1\nrg in g 10k\nrl vdd d 1k\nrsrc s 0 100\n\
         cs s 0 1n\nz1 d g s zn area=1.5\n\
         .model zn nmf(vto=-1.8 beta=3m lambda=0.05 alpha=2.5 b=0.4 rd=10 rs=8 cgs=1p \
         cgd=0.3p pb=0.8)\n\
         .pz in 0 d 0 vol pz\n",
        None,
        "Pole-Zero Analysis",
    );
    same(&ours, &c, 1e-6, 0.);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn noise_densities_match_c() {
    let (ours, c) = both(
        "noise",
        "mesfet noise\n\
         vdd vdd 0 8\nvin in 0 dc -0.6 ac 1\nrg in g 10k\nrl vdd d 1k\n\
         z1 d g 0 zn area=1.5 m=2\n\
         .model zn nmf(vto=-1.8 beta=3m lambda=0.05 alpha=2.5 b=0.4 rd=10 rs=8 cgs=1p \
         cgd=0.3p pb=0.8 kf=1e-12 af=1.2)\n\
         z2 d g 0 zp2\n.model zp2 nmf(vto=-1.5 beta=1m)\n\
         .noise v(d) vin dec 5 1k 1g 3\n",
        Some("noise1"),
        "Noise Spectral Density Curves",
    );
    same(&ours, &c, 1e-6, 1e-20);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn uic_and_off_transient_matches_c() {
    let (ours, c) = both(
        "uic",
        "mesfet uic\n\
         vdd vdd 0 5\nrl vdd d 1k\nrg g 0 10k\ncg g 0 10p\n\
         z1 d g 0 zn ic=2,-0.5\n\
         .model zn nmf(vto=-1.8 beta=3m lambda=0.05 cgs=2p cgd=0.5p pb=0.8)\n\
         z2 d g2 0 zn off\nrg2 g2 0 1k\n\
         .tran 0.1n 50n 0 0.05n uic\n",
        None,
        "Transient Analysis",
    );
    // compare::TRAN: 1e-3 relative and a 1 uV floor on the node voltages.
    same(&ours, &c, 1e-3, 1e-6);
}
