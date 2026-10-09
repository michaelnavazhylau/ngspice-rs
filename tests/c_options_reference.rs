//! Opt-in live C validation of `.option` settings with a numerical effect
//! (#110) and of `{expr}`/`'expr'` option values (#107). `NGSPICE_BIN` must
//! name the external reference binary; run with `-- --ignored`.
//!
//! Rust and C run the *same* deck text (C through a `.control` block writing an
//! ASCII rawfile). DC/AC values are compared by name with 1 ppm + floor
//! (`xtask::compare::NONLINEAR`); transients on a common physical grid with
//! ngspice's default accuracy floors (`reltol` 1e-3, `vntol` 1 uV, `abstol`
//! 1 pA, as `xtask::compare::TRAN`). Each test also shows that the option
//! changes the C result beyond that bound, so agreement is not vacuous.
use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use std::{fs, path::Path, process::Command};

/// Run `cards` (title first, no analysis, no `.end`) in C with the control
/// `command` and return the first plot of the written rawfile.
fn run_c(tag: &str, cards: &str, command: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-options-ref-{}-{tag}", std::process::id()));
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
        format!(
            "{cards}\n.control\nset filetype=ascii\n{command}\nwrite result.raw\nquit\n.endc\n.end\n"
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

/// Run `cards` plus the analysis card `analysis` through `RunConfig`.
fn run_rust(cards: &str, analysis: &str) -> Plot {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("c.cir"),
            &format!("{cards}\n.{analysis}\n.end\n"),
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

fn floor(name: &str) -> f64 {
    if name.starts_with('i') { 1e-12 } else { 1e-6 }
}

/// Point-wise DC/AC comparison by name, per complex component:
/// `|Rust - C| <= 1e-6 |C| + 1e-12`.
fn points_match(rust: &Plot, c: &Plot, names: &[&str]) -> Result<(), String> {
    if rust.point_count() != c.point_count() {
        return Err(format!(
            "point counts differ: Rust {}, C {}",
            rust.point_count(),
            c.point_count()
        ));
    }
    for name in names {
        for point in 0..c.point_count() {
            let ours = rust.value(name, point).unwrap();
            let theirs = c.value(name, point).unwrap();
            for (a, b) in [(ours.re, theirs.re), (ours.im, theirs.im)] {
                let bound = 1e-6 * b.abs() + 1e-12;
                if (a - b).abs() > bound {
                    return Err(format!(
                        "{name}[{point}]: Rust {ours:?}, C {theirs:?}, bound {bound:e}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// DC comparison at ngspice's Newton convergence tolerances (`reltol` 1e-3,
/// `vntol` 1 uV, `abstol` 1 pA): two Newton runs that stop at different
/// iterates of the same solution differ by up to this, independent of the
/// iteration limits under test.
fn converged_match(rust: &Plot, c: &Plot, names: &[&str]) -> Result<(), String> {
    if rust.point_count() != c.point_count() {
        return Err("point counts differ".into());
    }
    for name in names {
        for point in 0..c.point_count() {
            let (ours, theirs) = (
                rust.value(name, point).unwrap().re,
                c.value(name, point).unwrap().re,
            );
            let bound = 1e-3 * theirs.abs() + floor(name);
            if (ours - theirs).abs() > bound {
                return Err(format!(
                    "{name}[{point}]: Rust {ours:e}, C {theirs:e}, bound {bound:e}"
                ));
            }
        }
    }
    Ok(())
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

/// Transient comparison on `0, grid, ..., stop` plus `extra` instants.
fn waveforms_match(
    rust: &Plot,
    c: &Plot,
    grid: f64,
    stop: f64,
    extra: &[f64],
    names: &[&str],
) -> Result<f64, String> {
    let mut instants: Vec<f64> = (0..=((stop / grid).round() as usize))
        .map(|i| i as f64 * grid)
        .collect();
    instants.extend(extra);
    let mut worst: f64 = 0.;
    for name in names {
        for &t in &instants {
            let (ours, theirs) = (at(rust, name, t), at(c, name, t));
            let bound = 1e-3 * theirs.abs() + floor(name);
            worst = worst.max((ours - theirs).abs() / bound);
            if (ours - theirs).abs() > bound {
                return Err(format!(
                    "{name} t={t:e}: Rust {ours:e}, C {theirs:e}, bound {bound:e}"
                ));
            }
        }
    }
    Ok(worst)
}

const REVERSE_JUNCTIONS: &str = "options gmin\n.param gj=1u\nvd d 0 -10\nd1 d 0 dm\n\
    vc c 0 10\nq1 c 0 0 qm\nvp p 0 -10\nq2 p 0 0 qp\nvs s 0 10\nq3 s 0 0 x qm\nrx x 0 1k\n\
    vm m 0 5\nm1 m 0 0 0 mm\nvk k 0 10\nq4 k 0 0 qm m=2 area=3\n.model dm d(is=1e-14)\n\
    .model qm npn(is=1e-16)\n.model qp pnp(is=1e-16)\n.model mm nmos(vto=1)";

#[test]
#[ignore = "requires NGSPICE_BIN; junction gmin of diode/BJT/MOS1 against live C"]
fn gmin_matches_c_for_reverse_junctions() {
    // NPN substrate gmin ties to the collector (vertical), PNP to the base
    // (lateral); q3 has an explicit substrate node; q4's gmin terms scale
    // with m=2 but not with area=3 (bjtload.c).
    let names = ["i(vd)", "i(vc)", "i(vp)", "i(vs)", "v(x)", "i(vm)", "i(vk)"];
    for options in [
        ".options gmin={gj}",
        ".options gmin='gj*10'",
        ".options gmin=0",
    ] {
        let cards = format!("{REVERSE_JUNCTIONS}\n{options}");
        let rust = run_rust(&cards, "op");
        let c = run_c("gmin-op", &cards, "op");
        points_match(&rust, &c, &names).unwrap_or_else(|e| panic!("{options}: {e}"));
    }
    // Sensitivity: the default gmin changes C's currents far beyond the bound.
    let c_default = run_c("gmin-default", REVERSE_JUNCTIONS, "op");
    let rust_gmin = run_rust(&format!("{REVERSE_JUNCTIONS}\n.options gmin={{gj}}"), "op");
    assert!(points_match(&rust_gmin, &c_default, &names).is_err());
}

#[test]
#[ignore = "requires NGSPICE_BIN; gmin through a .dc sweep and .ac against live C"]
fn gmin_matches_c_in_sweeps_and_ac() {
    let cards = "gmin sweep\nv1 in 0 dc 0 ac 1\nr1 in out 1k\nd1 out 0 dm\n\
                 .model dm d(is=1e-14)\n.options gmin=1u itl2=20";
    let rust = run_rust(cards, "dc v1 -10 0 1");
    let c = run_c("gmin-dc", cards, "dc v1 -10 0 1");
    points_match(&rust, &c, &["v(out)", "i(v1)"]).unwrap();
    let ac_cards = "gmin ac\nv1 in 0 dc -5 ac 1\nr1 in out 1k\nd1 out 0 dm\n\
                    .model dm d(is=1e-14)\n.options gmin=1u";
    let rust_ac = run_rust(ac_cards, "ac dec 2 1k 1meg");
    let c_ac = run_c("gmin-ac", ac_cards, "ac dec 2 1k 1meg");
    points_match(&rust_ac, &c_ac, &["v(out)", "i(v1)"]).unwrap();
    // Sensitivity: C at the default gmin differs from Rust at gmin=1u beyond
    // the bound, in the sweep and in the small-signal response alike.
    let plain = |cards: &str| cards.replace(".options gmin=1u", ".options");
    let c_dc_default = run_c("gmin-dc-default", &plain(cards), "dc v1 -10 0 1");
    assert!(points_match(&rust, &c_dc_default, &["v(out)", "i(v1)"]).is_err());
    let c_ac_default = run_c("gmin-ac-default", &plain(ac_cards), "ac dec 2 1k 1meg");
    assert!(points_match(&rust_ac, &c_ac_default, &["v(out)", "i(v1)"]).is_err());
}

const XMU_RC: &str = "options xmu\nv1 in 0 pulse(0 10 100u 10u 10u 300u 800u)\nr1 in out 1k\n\
                      c1 out 0 0.1u";

#[test]
#[ignore = "requires NGSPICE_BIN; trapezoidal xmu against live C on a common grid"]
fn xmu_matches_c_on_a_common_grid() {
    let breakpoints = [100e-6, 110e-6, 410e-6, 420e-6, 900e-6, 910e-6];
    for xmu in ["0.2", "0.5", "{0.1*3}"] {
        let cards = format!("{XMU_RC}\n.options xmu={xmu} itl4=20");
        let rust = run_rust(&cards, "tran 2u 1.2m");
        let c = run_c("xmu", &cards, "tran 2u 1.2m");
        let worst = waveforms_match(&rust, &c, 5e-6, 1.2e-3, &breakpoints, &["v(out)", "i(v1)"])
            .unwrap_or_else(|e| panic!("xmu={xmu}: {e}"));
        println!("xmu={xmu}: worst error {worst:.3e} of bound");
    }
    // Sensitivity: Rust at the default xmu does not match C at xmu = 0.
    let rust = run_rust(XMU_RC, "tran 2u 1.2m");
    let c = run_c(
        "xmu-0",
        &format!("{XMU_RC}\n.options xmu=0"),
        "tran 2u 1.2m",
    );
    assert!(waveforms_match(&rust, &c, 5e-6, 1.2e-3, &breakpoints, &["v(out)", "i(v1)"]).is_err());
}

/// Exact equality of two plots (same points, same values).
fn identical(a: &Plot, b: &Plot) -> bool {
    a.point_count() == b.point_count() && a.points == b.points
}

#[test]
#[ignore = "requires NGSPICE_BIN; itl1/itl2/itl4 below 100 are no-ops in C and here"]
fn iteration_limits_below_c_floor_match_c() {
    // niiter.c raises every Newton limit below 100 to 100, so C's results are
    // bit-identical with and without these options; the port stores the same
    // effective limit and is bit-identical too, and both agree at compare::TRAN.
    // (The m4_diode_tran circuit, C parity at compare::TRAN.)
    let base = "options itl\nv1 in 0 pulse(0.5 0.7 10n 10n 10n 30n 80n)\nr1 in out 1k\n\
                d1 out 0 dm\n.model dm d(is=1e-14 n=1 rs=10 cjo=20p vj=0.7 m=0.5 tt=1n)";
    let low = format!("{base}\n.options itl1=2 itl4=3 itl3=4 itl5=0");
    let analysis = "tran 1n 100n 0 0.1n";
    let (c_base, c_low) = (
        run_c("itl-base", base, analysis),
        run_c("itl-low", &low, analysis),
    );
    assert!(identical(&c_base, &c_low), "C changed with itl below 100");
    let (rust_base, rust_low) = (run_rust(base, analysis), run_rust(&low, analysis));
    assert!(
        identical(&rust_base, &rust_low),
        "Rust changed with itl below 100"
    );
    let worst = waveforms_match(
        &rust_low,
        &c_low,
        1e-9,
        100e-9,
        &[10e-9, 20e-9, 50e-9, 60e-9, 90e-9],
        &["v(out)", "i(v1)"],
    )
    .unwrap();
    println!("itl below 100: worst error {worst:.3e} of bound");

    // DC: itl1=2 without continuation converges in C (v(b) = 0.69289) and here.
    let op = "options itl1\nv1 a 0 5\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n\
              .options itl1=2 gminsteps=0 srcsteps=0";
    converged_match(
        &run_rust(op, "op"),
        &run_c("itl1-op", op, "op"),
        &["v(b)", "i(v1)"],
    )
    .unwrap();

    // .dc: a 0 -> 5 V jump the port's warm start needs more than 5 iterations
    // for; itl2=5 is 100 in C and here, so the sweep agrees.
    let sweep = "options itl2\nv1 a 0 0\nr1 a b 1k\nd1 b 0 dm\n.model dm d(is=1e-14)\n\
                 .options itl2=5 itl1=2";
    let rust = run_rust(sweep, "dc v1 0 5 5");
    let c = run_c("itl2-dc", sweep, "dc v1 0 5 5");
    converged_match(&rust, &c, &["v(b)", "i(v1)"]).unwrap();
    let c_plain = run_c(
        "itl2-dc-plain",
        "options itl2\nv1 a 0 0\nr1 a b 1k\nd1 b 0 dm\n\
                         .model dm d(is=1e-14)",
        "dc v1 0 5 5",
    );
    assert!(
        identical(&c, &c_plain),
        "C changed with itl1/itl2 below 100"
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; {expr}/'expr' temp and tnom option values against live C"]
fn expression_option_values_match_c() {
    let base = "options expr\n.param t=40\nv1 a 0 1\nr1 a 0 rm\n.model rm r(r=1k tc1=0.01)";
    let cards = format!("{base}\n.options temp={{t+10}} tnom='t-5'");
    let rust = run_rust(&cards, "op");
    let c = run_c("expr", &cards, "op");
    points_match(&rust, &c, &["i(v1)"]).unwrap();
    // 1/(1k * (1 + 0.01 * (50 - 35))) = 1/1150.
    let current = rust.value("i(v1)", 0).unwrap().re;
    assert!((current + 1. / 1150.).abs() < 1e-12, "{current}");
    let c_plain = run_c("expr-plain", base, "op");
    assert!(points_match(&rust, &c_plain, &["i(v1)"]).is_err());
}
