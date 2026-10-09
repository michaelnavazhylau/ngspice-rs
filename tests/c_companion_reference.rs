//! Opt-in live C validation of the companion trap/Gear driver (#26).
//! `NGSPICE_BIN` must name the external reference binary.
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
use ngspice_rs::analysis::{AnalysisContext, Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
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
    let dir =
        std::env::temp_dir().join(format!("spice-companion-ref-{}-{tag}", std::process::id()));
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

/// Compares `vectors` on `0, grid, 2 grid, ... , stop` plus `breakpoints`.
fn compare(tag: &str, deck: &str, grid: f64, stop: f64, breakpoints: &[f64], vectors: &[&str]) {
    let rust = run_rust(deck);
    let c = run_c(tag, deck);
    let times = rust.column("time").unwrap();
    for b in breakpoints {
        for (label, plot) in [("Rust", &rust), ("C", &c)] {
            let t = plot.column("time").unwrap();
            assert!(
                t.iter().any(|v| (v.re - b).abs() <= 1e-9 * stop),
                "{label} has no sample at breakpoint {b:e}"
            );
        }
    }
    assert!((times[times.len() - 1].re - stop).abs() <= 1e-9 * stop);
    let mut instants: Vec<f64> = (0..=((stop / grid).round() as usize))
        .map(|i| i as f64 * grid)
        .collect();
    instants.extend(breakpoints);
    let mut worst: f64 = 0.;
    for name in vectors {
        let floor = if name.starts_with('i') { 1e-12 } else { 1e-6 };
        for &t in &instants {
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
#[ignore = "requires NGSPICE_BIN; compares trap against C on a common physical grid"]
fn pulse_rc_trap_matches_c() {
    compare(
        "rc-trap",
        "t\nv1 in 0 pulse(0 1 1m 10u 10u 2m 4m)\nr1 in out 1k\nc1 out 0 0.1u\n.tran 10u 8m 0 1u\n.end\n",
        1e-5,
        8e-3,
        &[1e-3, 1.01e-3, 3.01e-3, 3.02e-3, 5e-3],
        &["v(in)", "v(out)", "i(v1)"],
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; compares Gear-2 against C on a common physical grid"]
fn pulse_rc_gear_matches_c() {
    compare(
        "rc-gear",
        "t\nv1 in 0 pulse(0 1 1m 10u 10u 2m 4m)\nr1 in out 1k\nc1 out 0 0.1u\n.options method=gear\n.tran 10u 8m 0 1u\n.end\n",
        1e-5,
        8e-3,
        &[1e-3, 1.01e-3, 3.01e-3, 3.02e-3, 5e-3],
        &["v(in)", "v(out)", "i(v1)"],
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; compares a PWL-driven RC against C"]
fn pwl_rc_trap_matches_c() {
    compare(
        "pwl",
        "t\nv1 in 0 pwl(0 0 1m 0 1.01m 1 3m 1 3.01m 0.25 8m 0.25)\nr1 in out 1k\nc1 out 0 0.1u\n.tran 10u 8m 0 1u\n.end\n",
        1e-5,
        8e-3,
        &[1e-3, 1.01e-3, 3e-3, 3.01e-3],
        &["v(in)", "v(out)", "i(v1)"],
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; compares a series RLC (ringing) against C"]
fn pulse_series_rlc_matches_c_for_both_methods() {
    for (tag, option) in [("rlc-trap", ""), ("rlc-gear", ".options method=gear\n")] {
        compare(
            tag,
            &format!(
                "t\nv1 in 0 pulse(0 1 100u 5u 5u 400u 1m)\nr1 in a 10\nl1 a out 1m\nc1 out 0 1u\n{option}.tran 2u 1.5m 0 0.5u\n.end\n"
            ),
            2e-6,
            1.5e-3,
            &[100e-6, 105e-6, 505e-6, 510e-6, 1.1e-3],
            &["v(in)", "v(out)", "i(l1)", "i(v1)"],
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; compares maxord 3..=6 (gear and trap) against C"]
fn pulse_series_rlc_with_high_maxord_matches_c() {
    // dctran.c only raises the order from 1 to 2, so every maxord above 1
    // must reproduce C's (order <= 2) run, for both methods (#98).
    for (tag, method) in [("gear", "gear"), ("trap", "trap")] {
        for order in 3..=6 {
            compare(
                &format!("rlc-{tag}-maxord{order}"),
                &format!(
                    "t\nv1 in 0 pulse(0 1 100u 5u 5u 400u 1m)\nr1 in a 10\nl1 a out 1m\nc1 out 0 1u\n.options method={method} maxord={order}\n.tran 2u 1.5m 0 0.5u\n.end\n"
                ),
                2e-6,
                1.5e-3,
                &[100e-6, 105e-6, 505e-6, 510e-6, 1.1e-3],
                &["v(in)", "v(out)", "i(l1)", "i(v1)"],
            );
        }
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; the maxord=6 conformance deck against live C"]
fn conformance_rlc_gear_maxord6_matches_c() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("conformance/netlists/rlc_series_gear_maxord6_tran.cir");
    let deck = fs::read_to_string(path).unwrap();
    compare(
        "rlc_series_gear_maxord6_tran",
        &deck,
        1e-6,
        1e-3,
        &[50e-6, 70e-6, 470e-6, 490e-6],
        &["v(in)", "v(out)", "i(l1)", "i(v1)"],
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; the committed conformance deck against live C"]
fn conformance_rc_transient_matches_c() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/netlists/rc_transient.cir");
    let deck = fs::read_to_string(path).unwrap();
    compare(
        "rc_transient",
        &deck,
        5e-7,
        5e-6,
        &[1e-9],
        &["v(in)", "v(out)", "i(v1)"],
    );
}
