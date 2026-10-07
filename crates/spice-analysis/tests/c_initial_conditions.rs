//! Opt-in live C validation of `.ic`, `.nodeset`, instance `ic=` and `uic`
//! (#27). `NGSPICE_BIN` must name the external reference binary.
//!
//! Rust (ordinary `.tran`, companion backend) and C run the *same* deck text.
//! Their internal timesteps are never compared: both waveforms are evaluated at
//! common physical times (a regular grid plus every source breakpoint), each by
//! linear interpolation inside its own plot, which never spans a breakpoint
//! because both plots carry a sample on every breakpoint.
//!
//! Tolerance `|Rust - C| <= 1e-3 |C| + floor`, floor 1 uV / 1 pA, are the C
//! defaults `reltol`, `vntol` and `abstol` (the same policy as
//! `xtask::compare::TRAN`): two correct trap/Gear solutions of one circuit can
//! legitimately differ by about the accuracy ngspice itself targets. The decks
//! use `tmax = tau/100` so linear resampling adds far less than that.
use spice_analysis::{AnalysisContext, Plot, RawFile, RunConfig, runner};
use spice_netlist::{Parser, source::parse_deck_text};
use std::{fs, path::Path, process::Command};

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

fn run_c(tag: &str, deck: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-ic-ref-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    // The .tran card is replaced by a .control block that writes an ASCII raw.
    let (cards, tran) = deck.split_once("\n.tran ").expect("deck has a .tran card");
    let (tran, rest) = tran.split_once('\n').unwrap_or((tran, ""));
    fs::write(
        dir.join("c.cir"),
        format!(
            "{cards}\n{rest}\n.control\nset filetype=ascii\ntran {tran}\nwrite result.raw\nquit\n.endc\n.end\n"
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

fn run_rust(deck: &str) -> Plot {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("c.cir"), deck))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &AnalysisContext::default())
        .unwrap()
}

/// Compares `vectors` on `first, first + grid, ... , stop`. `first = 0` only
/// where C and Rust both write a `t = 0` row without a 1e10-conductance
/// artifact; under `uic` C writes no `t = 0` row at all.
fn compare(tag: &str, deck: &str, first: f64, grid: f64, stop: f64, vectors: &[&str]) {
    let rust = run_rust(deck);
    let c = run_c(tag, deck);
    let times = rust.column("time").unwrap();
    assert!((times[times.len() - 1].re - stop).abs() <= 1e-9 * stop);
    let count = ((stop - first) / grid).round() as usize;
    let mut worst: f64 = 0.;
    for name in vectors {
        let floor = if name.starts_with('i') { 1e-12 } else { 1e-6 };
        for k in 0..=count {
            let t = first + k as f64 * grid;
            let (ours, theirs) = (at(&rust, name, t), at(&c, name, t));
            let bound = 1e-3 * theirs.abs() + floor;
            worst = worst.max((ours - theirs).abs() / bound);
            assert!(
                (ours - theirs).abs() <= bound,
                "{tag} {name} t={t:e}: Rust {ours:e}, C {theirs:e}, bound {bound:e}"
            );
        }
    }
    println!(
        "{tag}: Rust {} points, C {} points, worst error {worst:.3e} of bound",
        rust.point_count(),
        c.point_count()
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; uic with instance ic= and node .ic against C"]
fn uic_capacitor_initial_conditions_match_c() {
    for (tag, extra, cap) in [
        ("uic-inst", "", " ic=0.5"),
        ("uic-node", ".ic v(out)=0.5\n", ""),
        // C quirk reproduced on purpose: CKTic copies .nodeset into the node
        // vector and CAPgetic derives the capacitor ic from it.
        ("uic-nodeset", ".nodeset v(out)=0.5\n", ""),
        ("uic-inst-wins", ".ic v(out)=0.9\n", " ic=0.5"),
    ] {
        compare(
            tag,
            &format!(
                "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u{cap}\n{extra}.tran 100u 5m uic\n.end\n"
            ),
            1e-4,
            1e-4,
            5e-3,
            &["v(in)", "v(out)", "i(v1)"],
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; uic inductor/RLC initial currents against C"]
fn uic_inductor_and_rlc_initial_conditions_match_c() {
    compare(
        "uic-rl",
        "t\nv1 in 0 1\nr1 in a 10\nl1 a 0 10m ic=0.5\n.tran 10u 5m uic\n.end\n",
        1e-5,
        1e-5,
        5e-3,
        &["v(a)", "i(l1)", "i(v1)"],
    );
    compare(
        "uic-rlc",
        "t\nv1 a 0 0\nl1 a b 1m ic=0.04\nr1 b out 10\nc1 out 0 1u ic=1.5\n.tran 0.5u 400u uic\n.end\n",
        2e-6,
        2e-6,
        4e-4,
        &["v(out)", "i(l1)", "v(b)"],
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; .ic in the transient bias point against C"]
fn bias_ic_without_uic_matches_c() {
    // The .ic node is not attached to a source, so C's row replacement is the
    // exact one and the t = 0 row (including i(v1)) must agree too.
    for (tag, cards) in [
        ("ic-bias", ".ic v(out)=0.25\n"),
        ("ic-dup", ".ic v(out)=0.9 v(out)=0.25\n"),
        (
            "ic-instance-ignored",
            ".ic v(out)=0.25\n.nodeset v(out)=0.8\n",
        ),
    ] {
        compare(
            tag,
            &format!("t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u ic=0.7\n{cards}.tran 10u 5m\n.end\n"),
            0.,
            1e-5,
            5e-3,
            &["v(in)", "v(out)", "i(v1)"],
        );
    }
    // Consistent .ic on the source node: C's 1e10 conductance gives a wrong
    // i(v1) at t = 0 only (documented divergence), so compare after t = 0.
    compare(
        "ic-source-node",
        "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u\n.ic v(in)=1 v(out)=0.25\n.tran 10u 5m\n.end\n",
        1e-5,
        1e-5,
        5e-3,
        &["v(in)", "v(out)", "i(v1)"],
    );
    // A nodeset alone leaves a linear transient unchanged in C as well.
    compare(
        "nodeset-only",
        "t\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u\n.nodeset v(out)=0.8\n.tran 10u 5m\n.end\n",
        0.,
        1e-5,
        5e-3,
        &["v(in)", "v(out)", "i(v1)"],
    );
}
