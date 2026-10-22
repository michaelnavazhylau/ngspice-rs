//! Opt-in live C validation of the BSIM3 `capMod=3` charge model wired through
//! `.dc`, `.ac` and `.tran`: `b3ld.c`'s charge block with `b3acld.c`'s
//! small-signal load.
//!
//! Rust and C run the same deck text. `.dc` and `.ac` compare every node
//! voltage and branch current C writes by name. Transient waveforms have
//! different internal timesteps in the two runs, so they are compared at common
//! physical times (a regular grid plus the source breakpoints) by linear
//! interpolation inside each plot, with the C-default `reltol`-scale bound
//! `1e-3 |C| + 1 uV/1 pA`; `tmax` is small enough that resampling contributes
//! far less than that.
use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use std::fs;
use std::path::Path;
use std::process::Command;

fn run_c(tag: &str, deck: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-bsim3-ac-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    fs::write(
        dir.join("c.cir"),
        format!("{deck}\n.control\nset filetype=ascii\nrun\nwrite result.raw\nquit\n.endc\n.end\n"),
    )
    .unwrap();
    let result = Command::new(binary)
        .args(["-b", "c.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let raw = RawFile::parse(&fs::read_to_string(dir.join("result.raw")).unwrap()).unwrap();
    raw.plots[0].plot.clone()
}

fn run_rust(deck: &str) -> Plot {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("c.cir"),
            &format!("{deck}\n.end\n"),
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}

/// Compare every node-voltage and branch-current variable at every point.
fn compare_by_name(tag: &str, deck: &str, relative: f64, absolute: f64) {
    let c = run_c(tag, deck);
    let rust = run_rust(deck);
    assert_eq!(rust.point_count(), c.point_count(), "{tag}: point counts");
    let mut compared = 0;
    for variable in &c.variables {
        let name = variable.name.to_ascii_lowercase();
        if !(name.starts_with("v(") || name.starts_with("i(")) {
            continue;
        }
        compared += 1;
        for point in 0..c.point_count() {
            let ours = rust
                .value(&name, point)
                .unwrap_or_else(|| panic!("{tag}: Rust has no {name}"));
            let theirs = c.value(&name, point).unwrap();
            for (a, b) in [(ours.re, theirs.re), (ours.im, theirs.im)] {
                let bound = relative * b.abs() + absolute;
                assert!(
                    (a - b).abs() <= bound,
                    "{tag}: {name}[{point}] Rust {ours:?}, C {theirs:?}, bound {bound:e}"
                );
            }
        }
    }
    assert!(compared >= 2, "{tag}: nothing compared");
}

/// A plot's `name` column at time `t`, linearly interpolated.
fn at(plot: &Plot, name: &str, t: f64) -> f64 {
    let time = plot.column("time").unwrap();
    let column = plot.column(name).unwrap_or_else(|| panic!("no {name}"));
    let upper = time.partition_point(|v| v.re < t).min(time.len() - 1);
    let lower = upper.saturating_sub(1);
    if upper == lower || time[upper].re == t {
        return column[upper].re;
    }
    let f = (t - time[lower].re) / (time[upper].re - time[lower].re);
    (1. - f) * column[lower].re + f * column[upper].re
}

#[test]
#[ignore = "requires NGSPICE_BIN; BSIM3 .dc against live C"]
fn dc_sweeps_match_c() {
    // A plain transfer sweep, then a card whose charges and junction diodes are
    // exercised with series resistance and areas.
    compare_by_name(
        "gate",
        "bsim3 dc gate\nvd d 0 1.5\nvg g 0 0\nvs s 0 0\n\
         m1 d g s 0 nn l=0.5u w=5u\n.model nn nmos level=8\n\
         .options reltol=1e-8\n.dc vg 0 3 0.25",
        1e-6,
        1e-12,
    );
    compare_by_name(
        "triode",
        "bsim3 dc triode\nvd d 0 0.1\nvg g 0 0\nvs s 0 0\n\
         m1 d g s 0 nn l=0.4u w=2u\n\
         .model nn nmos level=8 rsh=10 nrd=2 nrs=3 ad=2p as=1p pd=4u ps=3u \
         cgdo=1e-10 cgso=1e-10 cgdl=1e-10 cgsl=1e-10 cj=1e-3 cjsw=1e-10\n\
         .options reltol=1e-8\n.dc vg 0 2 0.1",
        1e-6,
        1e-12,
    );
    compare_by_name(
        "pmos",
        "bsim3 dc pmos\nvd d 0 -1.5\nvg g 0 0\nvs s 0 0\n\
         m1 d g s 0 nn l=0.5u w=5u\n.model nn pmos level=8\n\
         .options reltol=1e-8\n.dc vg 0 -3 -0.25",
        1e-6,
        1e-12,
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; BSIM3 .ac against live C"]
fn ac_sweeps_match_c() {
    // Common-source stage. The bias resistor network alone would give a flat
    // response; the transistor's own charges and overlaps produce the roll-off
    // this compares.
    compare_by_name(
        "cs",
        "bsim3 ac common source\nvin g 0 dc 1.2 ac 1\nvdd d 0 1.8\nvs s 0 0\n\
         rl d 0 10k\nm1 d g s 0 nn l=0.25u w=2u\n.model nn nmos level=8\n\
         .options reltol=1e-9\n.ac dec 5 1k 100g",
        1e-6,
        1e-15,
    );
    // Overlap and junction capacitances dominate here, and a 0/100 XPART
    // partition changes the drain charge split.
    compare_by_name(
        "caps",
        "bsim3 ac capacitances\nvin g 0 dc 1.0 ac 1\nvdd d 0 1.8\nvs s 0 0\n\
         rl d 0 10k\nrb b 0 0\n\
         m1 d g s b nn l=0.18u w=1u\n\
         .model nn nmos level=8 xpart=1 cgdo=2e-10 cgso=2e-10 cgbo=1e-10 cgdl=1e-10 \
         cgsl=1e-10 ckappa=1 cj=1e-3 cjsw=1e-10 cjswg=2e-10 mj=0.4 mjsw=0.2 \
         pb=0.9 pbsw=0.85 ad=2p as=1p pd=4u ps=3u\n\
         .options reltol=1e-9\n.ac dec 5 100 10g",
        1e-6,
        1e-15,
    );
    // PMOS, inverse-mode bias not used by AC (Vds > 0 after polarity), plus a
    // non-default mobility model whose intermediates feed the C-V.
    compare_by_name(
        "pmos-mobmod2",
        "bsim3 ac pmos\nvin g 0 dc -1.5 ac 1\nvdd d 0 -1.8\nvs s 0 0\n\
         rl d 0 10k\nm1 d g s 0 nn l=0.5u w=5u\n\
         .model nn pmos level=8 mobmod=2 ua=1e-9 ub=1e-18 uc=1e-11\n\
         .options reltol=1e-9\n.ac dec 5 1k 10g",
        1e-6,
        1e-15,
    );
}

/// Run C's transient and return its plot.
fn run_c_tran(tag: &str, deck: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-bsim3-tran-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let (cards, tran) = deck.split_once("\n.tran ").expect("deck has a .tran card");
    let (tran, _) = tran.split_once('\n').unwrap_or((tran, ""));
    fs::write(
        dir.join("c.cir"),
        format!(
            "{cards}\n.control\nset filetype=ascii\ntran {tran}\nwrite result.raw\nquit\n.endc\n.end\n"
        ),
    )
    .unwrap();
    let result = Command::new(binary)
        .args(["-b", "c.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let raw = RawFile::parse(&fs::read_to_string(dir.join("result.raw")).unwrap()).unwrap();
    raw.plots[0].plot.clone()
}

/// Compare a transient on a common grid of explicit `times`.
///
/// C and the port choose different internal timesteps, so neither the step
/// sequence nor the raw sample count is compared. The times below sit away
/// from the source edges, where linear resampling is essentially exact; the
/// bound is the C-default `reltol`-scale `1e-3 |C| + 1 uV / 1 pA`.
/// `min_current` skips a branch current while it is below C's own
/// charge-derivative residual, which is numerical noise rather than physics.
fn compare_tran(tag: &str, deck: &str, times: &[f64], min_current: f64) {
    let c = run_c_tran(tag, deck);
    let rust = run_rust(deck);
    let mut compared = 0;
    for variable in &c.variables {
        let name = variable.name.to_ascii_lowercase();
        if !(name.starts_with("v(") || name.starts_with("i(")) {
            continue;
        }
        compared += 1;
        for time in times {
            let ours = at(&rust, &name, *time);
            let theirs = at(&c, &name, *time);
            if theirs.abs() < min_current {
                continue;
            }
            let bound = 1e-3 * theirs.abs() + if name.starts_with("v(") { 1e-6 } else { 1e-12 };
            assert!(
                (ours - theirs).abs() <= bound,
                "{tag}: {name} at {time:e}: Rust {ours:e}, C {theirs:e}, bound {bound:e}"
            );
        }
    }
    assert!(compared >= 2, "{tag}: nothing compared");
}

#[test]
#[ignore = "requires NGSPICE_BIN; BSIM3 .tran against live C on a common grid"]
fn transient_waveforms_match_c() {
    // A slow gate ramp through a large resistor: the gate charge (intrinsic
    // plus overlap) sets `v(g)`, and the drain follows through `Cgd`, both
    // junction capacitances and the channel. All internal steps are
    // quasi-static, so the two solutions track tightly.
    let gate = concat!(
        "bsim3 tran gate charge\n",
        "vin drv 0 pulse(0 2.5 1n 100n 100n 500n 2000n)\n",
        "rg drv g 1meg\nvdd vdd 0 1.8\nrd vdd d 10k\nvs s 0 0\n",
        "m1 d g s 0 nn l=0.25u w=2u ad=2p as=1p pd=4u ps=3u\n",
        ".model nn nmos level=8 cgdo=1e-10 cgso=1e-10 cgdl=1e-10 cgsl=1e-10 ",
        "cj=1e-3 cjsw=1e-10\n",
        ".options reltol=1e-5\n.tran 1n 400n 0 5n"
    );
    compare_tran(
        "gate",
        gate,
        &[
            5e-9, 35e-9, 65e-9, 130e-9, 160e-9, 190e-9, 220e-9, 250e-9, 280e-9, 310e-9, 340e-9,
        ],
        1e-9,
    );

    // A drain step with the gate held high: the drain current is the channel
    // current plus the displacement through `Cgd` and both junctions, and the
    // source current must stay its negative (no bulk current).
    let drain = concat!(
        "bsim3 tran drain step\n",
        "vg g 0 2.5\nvd d 0 pulse(0.1 1.8 2n 2n 2n 20n 60n)\nvs s 0 0\n",
        "m1 d g s 0 nn l=0.25u w=2u ad=2p as=1p pd=4u ps=3u\n",
        ".model nn nmos level=8 cgdo=1e-10 cgso=1e-10 cgdl=1e-10 cgsl=1e-10 ",
        "cj=1e-3 cjsw=1e-10\n",
        ".options reltol=1e-5\n.tran 0.1n 40n 0 1n"
    );
    compare_tran(
        "drain",
        drain,
        &[0.2e-9, 5e-9, 10e-9, 15e-9, 20e-9, 28e-9, 34e-9, 38e-9],
        1e-9,
    );
}
