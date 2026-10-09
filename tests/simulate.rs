//! Process-level checks for `spice-rs simulate`: the exit-code contract, the
//! multi-analysis batch order and per-analysis output cards, the ASCII rawfile the command writes and the
//! temporary-file/rename guarantee that a failed run never leaves or destroys
//! the destination.
//!
//! The rawfiles are read back with the production ASCII reader and compared with
//! the committed C goldens variable by variable, by name. Nothing here
//! re-captures a golden.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ngspice_rs::analysis::{PlotFlags, RawFile};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".")
        .canonicalize()
        .expect("the workspace root exists")
}

fn fixture(name: &str) -> PathBuf {
    workspace()
        .join("conformance/netlists")
        .join(format!("{name}.cir"))
}

fn golden(name: &str) -> RawFile {
    RawFile::load(workspace().join(format!("conformance/golden/{name}.raw")))
        .expect("the committed golden parses")
}

/// A private scratch directory. `name` keeps concurrent tests apart.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("spice-rs-simulate-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// Writes `text` as `deck.cir` inside `dir`.
fn write_deck(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("deck.cir");
    fs::write(&path, text).expect("write the deck");
    path
}

/// The names of everything in `dir`, sorted: a leftover temporary file shows up
/// here.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("readable directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn stdout(run: &Output) -> String {
    String::from_utf8_lossy(&run.stdout).into_owned()
}

fn stderr(run: &Output) -> String {
    String::from_utf8_lossy(&run.stderr).into_owned()
}

/// Runs `spice-rs simulate --output <output> <deck>`.
fn simulate(output: &Path, deck: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("simulate")
        .arg("--output")
        .arg(output)
        .arg(deck)
        .output()
        .expect("run spice-rs")
}

/// Runs `spice-rs` with raw arguments.
fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .args(arguments)
        .output()
        .expect("run spice-rs")
}

/// Compares a written rawfile with its committed C golden, variable by name.
///
/// The data must agree within `relative * |C| + absolute`; the plot name, the
/// flags and every variable's name, unit and real/complex flag must match
/// exactly.
fn assert_matches_golden(name: &str, written: &RawFile, relative: f64, absolute: f64) {
    assert_plots_match(name, written, &golden(name), relative, absolute);
}

/// [`assert_matches_golden`] against an explicit single-plot rawfile.
fn assert_plots_match(
    name: &str,
    written: &RawFile,
    committed: &RawFile,
    relative: f64,
    absolute: f64,
) {
    let want = committed.single_plot().expect("one golden plot");
    let got = written.single_plot().expect("one written plot");
    assert_eq!(got.plotname, want.plotname, "{name}: plot name");
    assert_eq!(got.flags, want.flags, "{name}: flags");
    assert_eq!(
        got.variable_count(),
        want.variable_count(),
        "{name}: columns"
    );
    assert_eq!(got.point_count(), want.point_count(), "{name}: points");
    assert_eq!(
        got.variables
            .iter()
            .map(|variable| variable.name.as_str())
            .collect::<Vec<_>>(),
        want.variables
            .iter()
            .map(|variable| variable.name.as_str())
            .collect::<Vec<_>>(),
        "{name}: column order must match the golden, not just the names"
    );
    for variable in &want.variables {
        let Some(index) = got.variable_index(&variable.name) else {
            panic!("{name}: the rawfile has no variable '{}'", variable.name);
        };
        assert_eq!(
            &got.variables[index], variable,
            "{name}: metadata of '{}'",
            variable.name
        );
        let actual = got.column(&variable.name).expect("the column exists");
        let expected = want.column(&variable.name).expect("the column exists");
        for (point, (a, b)) in actual.iter().zip(&expected).enumerate() {
            assert!(
                (*a - *b).magnitude() <= relative * b.magnitude() + absolute,
                "{name}: '{}' point {point}: {a} != {b}",
                variable.name
            );
        }
    }
}

#[test]
fn operating_point_matches_the_committed_golden() {
    let dir = scratch("op");
    let output = dir.join("op.raw");
    let run = simulate(&output, &fixture("rc_divider"));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("analysis:  .op   DC operating point"),
        "{report}"
    );
    assert!(report.contains("variables: v(in) v(out) i(v1)"), "{report}");
    assert!(report.contains(&output.display().to_string()), "{report}");

    let written = RawFile::load(&output).expect("the command wrote a parseable rawfile");
    assert_eq!(written.plots[0].title, "RC divider, operating point");
    assert_eq!(
        written.plots[0].command,
        format!("spice-rs {VERSION} (Rust port), Build")
    );
    assert_eq!(
        written.plots[0].date.len(),
        24,
        "a ctime-style date: {}",
        written.plots[0].date
    );
    assert_matches_golden("rc_divider", &written, 1e-12, 1e-15);
    assert_eq!(
        entries(&dir),
        ["op.raw"],
        "no temporary file is left behind"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_forward_dc_sweep_writes_the_requested_grid() {
    let dir = scratch("dc-forward");
    let deck = write_deck(
        &dir,
        "rc divider dc sweep\nv1 in 0 dc 0\nr1 in out 1k\nr2 out 0 1k\n.dc v1 0 5 1\n.end\n",
    );
    let output = dir.join("forward.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("analysis:  .dc   DC sweep"),
        "{}",
        stdout(&run)
    );

    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    assert_eq!(plot.plotname, "DC transfer characteristic");
    assert_eq!(plot.flags, PlotFlags::Real);
    let names: Vec<&str> = plot.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["sweep", "v(in)", "v(out)", "i(v1)"]);
    assert_eq!(plot.point_count(), 6, "0, 1, 2, 3, 4 and 5 volts");
    for point in 0..plot.point_count() {
        let volts = plot.value("sweep", point).unwrap().re;
        assert!((volts - point as f64).abs() < 1e-12, "point {point}");
        assert!((plot.value("v(in)", point).unwrap().re - volts).abs() < 1e-12);
        assert!((plot.value("v(out)", point).unwrap().re - volts / 2.0).abs() < 1e-12);
        assert!((plot.value("i(v1)", point).unwrap().re + volts / 2000.0).abs() < 1e-15);
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_reverse_dc_sweep_descends_through_the_same_grid() {
    let dir = scratch("dc-reverse");
    let deck = write_deck(
        &dir,
        "rc divider dc sweep\nv1 in 0 dc 0\nr1 in out 1k\nr2 out 0 1k\n.dc v1 5 0 -1\n.end\n",
    );
    let output = dir.join("reverse.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));

    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    let volts: Vec<f64> = plot
        .column("sweep")
        .expect("the sweep column")
        .iter()
        .map(|value| value.re)
        .collect();
    assert_eq!(volts, [5.0, 4.0, 3.0, 2.0, 1.0, 0.0]);
    for (point, volts) in volts.iter().enumerate() {
        assert!((plot.value("v(out)", point).unwrap().re - volts / 2.0).abs() < 1e-12);
        assert!((plot.value("i(v1)", point).unwrap().re + volts / 2000.0).abs() < 1e-15);
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_complex_ac_rawfile_matches_the_committed_golden() {
    let dir = scratch("ac");
    let output = dir.join("ac.raw");
    let run = simulate(&output, &fixture("rc_lowpass_ac"));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("analysis:  .ac   AC small-signal"),
        "{}",
        stdout(&run)
    );

    let written = RawFile::load(&output).expect("parses");
    assert_eq!(written.single_plot().unwrap().flags, PlotFlags::Complex);
    assert_matches_golden("rc_lowpass_ac", &written, 1e-10, 1e-12);
    // The complex ASCII form spells values as `re,im`, as `raw_write()` does.
    let text = fs::read_to_string(&output).unwrap();
    assert!(text.contains("Flags: complex"), "{text}");
    assert!(
        text.contains("1.000000000000000e+02,0.000000000000000e+00"),
        "{text}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_plain_transient_runs_the_engine_default_and_matches_the_golden() {
    // An unadorned `.tran` is the companion trapezoidal/Gear-2 driver the engine
    // defaults to; the command must not reinterpret it as the diffsol backend.
    let dir = scratch("tran");
    let output = dir.join("tran.raw");
    let run = simulate(&output, &fixture("rc_transient"));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let written = RawFile::load(&output).expect("parses");
    assert_matches_golden("rc_transient", &written, 1e-12, 1e-15);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn uic_and_ic_transients_prove_the_companion_driver_not_diffsol() {
    // `.tran uic` and instance `ic=` exist only in the companion driver: the
    // diffsol BDF backend rejects both explicitly. A successful run here shows
    // that `simulate` did not inject `backend=diffsol`.
    let dir = scratch("tran-uic");
    let output = dir.join("uic.raw");
    let run = simulate(&output, &fixture("rc_ic_uic_tran"));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let written = RawFile::load(&output).expect("parses");
    assert_matches_golden("rc_ic_uic_tran", &written, 1e-12, 1e-15);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn explicit_bdf_succeeds_on_a_grounded_capacitor_circuit() {
    let dir = scratch("bdf");
    // A grounded capacitor charges from a pulse source: the explicit BDF backend
    // needs the capacitor to have a resistive DC path on both nodes.
    let deck = write_deck(
        &dir,
        "rc charging\nv1 in 0 pulse(0 1 0 1n 1n 1 2m)\nr1 in out 1k\nc1 out 0 1u\n\
         .tran 1u 100u backend=diffsol method=bdf\n.end\n",
    );
    let output = dir.join("bdf.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));

    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    assert_eq!(plot.plotname, "Transient Analysis");
    let names: Vec<&str> = plot.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["time", "v(in)", "v(out)", "i(v1)"]);
    assert_eq!(plot.point_count(), 101, "the 1 us grid up to 100 us");
    // tau = R C = 1 ms: v(out) = 1 - exp(-t/tau), from a 0 V operating point.
    for point in [0, 50, 100] {
        let time = plot.value("time", point).unwrap().re;
        let volts = plot.value("v(out)", point).unwrap().re;
        let expected = 1.0 - (-time / 1e-3).exp();
        assert!(
            (volts - expected).abs() < 1e-3,
            "point {point}: {volts} != {expected}"
        );
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_explicit_diffsol_selection_is_honoured() {
    // The same circuit and `uic` as `rc_ic_uic_tran`, but with the backend named
    // on the card: the engine's explicit rejection must reach the exit status.
    let dir = scratch("bdf-uic");
    let deck = write_deck(
        &dir,
        "uic on diffsol\nv1 in 0 dc 0\nr1 in out 1k\nc1 out 0 1u ic=2\n\
         .tran 10u 1m uic backend=diffsol method=bdf\n.end\n",
    );
    let output = dir.join("out.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains("uic"), "{}", stderr(&run));
    assert!(!output.exists());
    assert_eq!(entries(&dir), ["deck.cir"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_analysis_without_a_driver_fails_without_writing_a_rawfile() {
    let dir = scratch("sens");
    let deck = write_deck(
        &dir,
        "sens deck\nv1 in 0 dc 1 ac 1\nr1 in out 1k\nc1 out 0 1u\n\
         .sens v(out)\n.end\n",
    );
    let output = dir.join("sens.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(run.stdout.is_empty(), "{}", stdout(&run));
    assert!(
        stderr(&run).contains(".sens analysis has no driver"),
        "{}",
        stderr(&run)
    );
    assert!(!output.exists(), "a failure writes no rawfile");
    assert_eq!(entries(&dir), ["deck.cir"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_unported_model_family_exits_three_and_writes_nothing() {
    let dir = scratch("bsim");
    let deck = write_deck(
        &dir,
        "bsim deck\nv1 d 0 dc 1\nr1 d 0 1k\nm1 d d 0 0 nm w=10u l=1u\n\
         .model nm nmos level=49\n.op\n.end\n",
    );
    let output = dir.join("bsim.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(3), "{}", stderr(&run));
    assert!(stderr(&run).contains("not yet ported"), "{}", stderr(&run));
    assert!(!output.exists());
    assert_eq!(entries(&dir), ["deck.cir"]);
    fs::remove_dir_all(&dir).unwrap();
}

/// The plot names of a written rawfile, in file order.
fn plotnames(rawfile: &RawFile) -> Vec<&str> {
    rawfile
        .plots
        .iter()
        .map(|raw_plot| raw_plot.plot.plotname.as_str())
        .collect()
}

/// The variable names of one plot, in column order.
fn variable_names(plot: &ngspice_rs::analysis::Plot) -> Vec<&str> {
    plot.variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect()
}

#[test]
fn a_multi_analysis_deck_writes_one_plot_per_analysis_in_c_batch_order() {
    // The deck lists `.tran .ac .op .dc`; ngspice batch mode runs `.ac .dc .op
    // .tran` (CKTdoJob walks analInfo[] in its fixed order), and so does the port.
    let dir = scratch("multi");
    let output = dir.join("multi.raw");
    let run = simulate(&output, &fixture("multi_analysis_rc"));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("plots:     ac1 dc1 op1 tran1 (ngspice batch order)"),
        "{report}"
    );
    assert!(
        report.contains("(ngspice ASCII rawfile with 4 plots, no binary support)"),
        "{report}"
    );
    let sections: Vec<usize> = ["[ac1]", "[dc1]", "[op1]", "[tran1]"]
        .iter()
        .map(|section| report.find(section).expect("one section per plot"))
        .collect();
    assert!(
        sections.windows(2).all(|pair| pair[0] < pair[1]),
        "{report}"
    );

    let mut written = RawFile::load(&output).expect("parses");
    let committed = golden("multi_analysis_rc");
    assert_eq!(plotnames(&written), plotnames(&committed));
    assert_eq!(
        plotnames(&written),
        [
            "AC Analysis",
            "DC transfer characteristic",
            "Operating Point",
            "Transient Analysis"
        ]
    );
    for raw_plot in &written.plots {
        assert_eq!(raw_plot.title, "Multi-analysis RC low-pass with load");
        assert!(raw_plot.command.starts_with("spice-rs "), "{raw_plot:?}");
    }
    // The port names the DC scale `sweep`; C writes `v(v-sweep)` (CLI.md).
    assert_eq!(written.plots[1].plot.variables[0].name, "sweep");
    written.plots[1].plot.variables[0].name = "v(v-sweep)".to_owned();
    for (index, (got, want)) in written.plots.iter().zip(&committed.plots).enumerate() {
        let (relative, absolute) = if got.plot.flags == PlotFlags::Complex {
            (1e-10, 1e-12)
        } else {
            (1e-12, 1e-15)
        };
        let single = |raw_plot: &ngspice_rs::analysis::RawPlot| RawFile {
            plots: vec![raw_plot.clone()],
        };
        assert_plots_match(
            &format!("multi_analysis_rc plot {index}"),
            &single(got),
            &single(want),
            relative,
            absolute,
        );
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn same_type_analyses_run_in_reverse_deck_order_with_c_plot_names() {
    // Measured with ngspice-47: two `.dc` cards run last-card-first (the job
    // list is built by prepending), and the plot counter that collision bumped
    // to 2 stays there, so the deck below lists `dc1 dc2 op2 tran2`. C prints a
    // `.print dc` table for both sweeps, and measures `.meas dc` on the plot
    // that ran last (`plot_cur`), i.e. the first card's 0..1 V sweep:
    // ngspice-47 reports `vmax = 5.00000e-01 at= 1.00000e+00` for this circuit.
    let dir = scratch("multi-same-type");
    let deck = write_deck(
        &dir,
        "two sweeps\nv1 in 0 dc 1 pulse(0 1 0 1u 1u 1m 2m)\nr1 in out 1k\nr2 out 0 1k\n\
         .tran 0.1m 0.5m\n.dc v1 0 1 0.5\n.op\n.dc v1 0 4 2\n\
         .print dc v(out)\n.meas dc vmax max v(out)\n.end\n",
    );
    let output = dir.join("same.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("plots:     dc1 dc2 op2 tran2 (ngspice batch order)"),
        "{}",
        stdout(&run)
    );
    let written = RawFile::load(&output).expect("parses");
    assert_eq!(
        plotnames(&written),
        [
            "DC transfer characteristic",
            "DC transfer characteristic",
            "Operating Point",
            "Transient Analysis"
        ]
    );
    let last_sweep = |plot: &ngspice_rs::analysis::Plot| {
        plot.value("sweep", plot.point_count() - 1)
            .expect("a sweep column")
            .re
    };
    assert_eq!(
        last_sweep(&written.plots[0].plot),
        4.0,
        "the later card runs first"
    );
    assert_eq!(last_sweep(&written.plots[1].plot), 1.0);
    let report = stdout(&run);
    assert_eq!(
        report.matches("print: 2 vector(s): sweep v(out)").count(),
        2,
        "one table per dc plot: {report}"
    );
    assert_eq!(report.matches("vmax").count(), 1, "{report}");
    let measured = report.find("vmax").unwrap();
    assert!(
        report.find("[dc2]").unwrap() < measured && measured < report.find("[op2]").unwrap(),
        "the measurement belongs to the last dc plot: {report}"
    );
    assert!(
        report[measured..].contains("5.000000000000000e-1")
            || report[measured..].contains("5.000000000000000e-01"),
        "{report}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn output_cards_apply_per_analysis_type() {
    let dir = scratch("multi-outputs");
    // `.save` narrows every plot; `.print ac` adds `vm(out)` to (and prints) the
    // ac plot only; `.measure` cards go to the plot of their own type and
    // `.four` to the transient plot.
    let deck = write_deck(
        &dir,
        "per-analysis outputs\nv1 in 0 dc 1 ac 1 pulse(0 1 0 10u 10u 490u 1m)\nr1 in out 1k\nc1 out 0 10n\n\
         .tran 10u 3m\n.ac lin 3 100 1k\n.op\n\
         .save v(out)\n.print ac vm(out)\n\
         .meas tran tmax max v(out)\n.meas ac gain find vm(out) at=100\n\
         .four 1k v(out)\n.end\n",
    );
    let output = dir.join("outputs.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let written = RawFile::load(&output).expect("parses");
    assert_eq!(
        plotnames(&written),
        ["AC Analysis", "Operating Point", "Transient Analysis"]
    );
    assert_eq!(
        variable_names(&written.plots[0].plot),
        ["frequency", "v(out)", "vm(out)"]
    );
    assert_eq!(variable_names(&written.plots[1].plot), ["v(out)"]);
    assert_eq!(variable_names(&written.plots[2].plot), ["time", "v(out)"]);

    let report = stdout(&run);
    let ac = report.find("[ac1]").expect("the ac section");
    let op = report.find("[op1]").expect("the op section");
    let tran = report.find("[tran1]").expect("the tran section");
    let only_in = |needle: &str, from: usize, to: usize| {
        let at = report
            .find(needle)
            .unwrap_or_else(|| panic!("{needle}: {report}"));
        assert!(
            from < at && at < to,
            "{needle} belongs to its own plot: {report}"
        );
        assert_eq!(report.matches(needle).count(), 1, "{needle}: {report}");
    };
    only_in("print: 2 vector(s): frequency vm(out)", ac, op);
    only_in("gain", ac, op);
    only_in("tmax", tran, report.len());
    only_in("four: 1 analysis(es)", tran, report.len());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failing_later_analysis_publishes_nothing_from_the_earlier_ones() {
    let dir = scratch("multi-atomic");
    let output = dir.join("keep.raw");
    let base = "atomic\nv1 in 0 dc 1 ac 1 pulse(0 1 0 1u 1u 1m 2m)\nr1 in out 1k\nc1 out 0 1u\n\
                .op\n.ac lin 3 100 1k\n";
    for (extra, status, message) in [
        // `.tran` runs last and exhausts its work budget after `.ac` and `.op`
        // succeeded.
        (".tran 1u 1m maxsteps=3\n", 2, "work limit"),
        // A measurement of the last plot fails after every analysis ran.
        (
            ".tran 10u 1m\n.meas tran late find v(out) at=5\n",
            2,
            "outside the time range",
        ),
        // An analysis without a driver fails before anything runs.
        (".sens v(out)\n", 2, ".sens analysis has no driver"),
        // Output cards naming an analysis the deck does not run.
        (".print dc v(out)\n", 2, "names a different analysis"),
        (
            ".meas tran x max v(out)\n",
            2,
            "the card names a .tran measurement",
        ),
        (".four 1k v(out)\n", 2, "transforms a .tran result"),
    ] {
        fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
        let deck = write_deck(&dir, &format!("{base}{extra}.end\n"));
        let run = simulate(&output, &deck);
        assert_eq!(run.status.code(), Some(status), "{extra}: {}", stderr(&run));
        assert!(stdout(&run).is_empty(), "{extra}: {}", stdout(&run));
        assert!(stderr(&run).contains(message), "{extra}: {}", stderr(&run));
        assert_eq!(fs::read_to_string(&output).unwrap(), "PREVIOUS CONTENT\n");
        assert_eq!(entries(&dir), ["deck.cir", "keep.raw"], "{extra}");
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_deck_without_an_analysis_is_an_input_failure() {
    let dir = scratch("no-analysis");
    let deck = write_deck(&dir, "no analysis\nv1 in 0 dc 1\nr1 out 0 1k\n.end\n");
    let output = dir.join("out.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("requests no analysis"),
        "{}",
        stderr(&run)
    );
    assert!(!output.exists());
    assert_eq!(entries(&dir), ["deck.cir"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_numerical_failure_preserves_the_destination() {
    let dir = scratch("numerical");
    // Two ideal voltage sources across the same nodes: the MNA matrix is
    // singular, so the run fails after a successful parse.
    let deck = write_deck(
        &dir,
        "singular\nv1 a 0 dc 1\nv2 a 0 dc 2\nr1 a 0 1k\n.op\n.end\n",
    );
    let output = dir.join("keep.raw");
    fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(
        stderr(&run).to_lowercase().contains("singular"),
        "{}",
        stderr(&run)
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "PREVIOUS CONTENT\n");
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_missing_output_directory_is_an_output_failure() {
    let dir = scratch("missing-directory");
    let output = dir.join("missing").join("out.raw");
    let run = simulate(&output, &fixture("rc_divider"));
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("output directory"),
        "{}",
        stderr(&run)
    );
    assert!(!output.exists());
    assert_eq!(entries(&dir), Vec::<String>::new(), "nothing is created");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_successful_run_replaces_an_existing_destination() {
    let dir = scratch("overwrite");
    let output = dir.join("out.raw");
    fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
    let first = simulate(&output, &fixture("rc_divider"));
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert!(
        fs::read_to_string(&output)
            .unwrap()
            .starts_with("Title: RC divider, operating point")
    );
    let second = simulate(&output, &fixture("rc_lowpass_ac"));
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    let text = fs::read_to_string(&output).unwrap();
    assert!(text.starts_with("Title: RC low-pass, AC sweep"), "{text}");
    assert!(text.contains("Flags: complex"), "{text}");
    assert_eq!(entries(&dir), ["out.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_output_flag_also_takes_an_equals_form() {
    let dir = scratch("equals-form");
    let output = dir.join("out.raw");
    let argument = format!("--output={}", output.display());
    let deck = fixture("rc_divider");
    let run = cli(&["simulate", &argument, deck.to_str().unwrap()]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(RawFile::load(&output).is_ok(), "the rawfile was written");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn usage_errors_exit_one_and_print_the_usage() {
    for arguments in [
        vec!["simulate", "deck.cir"],
        vec!["simulate", "--output"],
        vec!["simulate", "--output", "", "deck.cir"],
        vec!["simulate", "--output=", "deck.cir"],
        vec![
            "simulate", "--output", "a.raw", "--output", "b.raw", "deck.cir",
        ],
        vec!["--output", "a.raw", "deck.cir"],
        vec!["cards", "--output", "a.raw", "deck.cir"],
    ] {
        let run = cli(&arguments);
        assert_eq!(
            run.status.code(),
            Some(1),
            "{arguments:?}: {}",
            stderr(&run)
        );
        assert!(run.stdout.is_empty(), "{arguments:?}");
        assert!(
            stderr(&run).contains("USAGE:"),
            "{arguments:?}: {}",
            stderr(&run)
        );
    }
}

#[test]
fn the_help_text_documents_simulate_and_the_exit_codes() {
    let run = cli(&["--help"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("simulate (--output <path>)"), "{text}");
    assert!(text.contains("--output <path>"), "{text}");
    assert!(text.contains("also --output=<path>"), "{text}");
    assert!(
        text.contains("2 deck could not be read, simulation,"),
        "{text}"
    );
    assert!(text.contains("3 not ported yet"), "{text}");
}

/// The rawfile text without the `Date:` header, which is the write time.
fn without_date(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("Date:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One value spelled the way the ASCII rawfile spells it.
fn spelled(value: ngspice_rs::primitives::Complex, complex: bool) -> String {
    let real = ngspice_rs::primitives::format_spice_number(value.re);
    if complex {
        format!(
            "{real},{}",
            ngspice_rs::primitives::format_spice_number(value.im)
        )
    } else {
        real
    }
}

#[test]
fn save_narrows_an_operating_point_to_the_requested_vectors_in_request_order() {
    let dir = scratch("save-op");
    let deck = write_deck(
        &dir,
        "rc divider, selected\nv1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op\n\
         .save v(out) i(v1) v(in,out) v(0,out) v(out)\n.end\n",
    );
    let output = dir.join("op.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    // Request order, the duplicate collapsed, and no invented vectors.
    assert!(
        report.contains("variables: v(out) i(v1) v(in,out) v(0,out)"),
        "{report}"
    );
    assert!(
        !report.contains("print:"),
        "a deck without .print has no table: {report}"
    );

    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    let names: Vec<&str> = plot.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["v(out)", "i(v1)", "v(in,out)", "v(0,out)"]);
    let want = golden("rc_divider").single_plot().unwrap().clone();
    // Values still come from the driver, addressed by name.
    assert_eq!(plot.value("v(out)", 0), want.value("v(out)", 0));
    assert_eq!(plot.value("v(in)", 0), None, "v(in) was not requested");
    // A voltage difference, and ground as a terminal.
    assert_eq!(
        plot.value("v(in,out)", 0),
        Some(ngspice_rs::primitives::Complex::real(2.5))
    );
    assert_eq!(
        plot.value("v(0,out)", 0),
        Some(ngspice_rs::primitives::Complex::real(-2.5))
    );
    // The golden's sign convention survives selection: i(v1) is the current
    // into the positive terminal, so 5 V across 2 k is -2.5 mA.
    assert_eq!(
        plot.value("i(v1)", 0),
        Some(ngspice_rs::primitives::Complex::real(-2.5e-3))
    );
    assert_eq!(entries(&dir), ["deck.cir", "op.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn save_all_writes_exactly_the_default_rawfile() {
    let dir = scratch("save-all");
    let plain = write_deck(
        &dir,
        "rc divider, operating point\nv1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op\n.end\n",
    );
    let with_all = dir.join("all.cir");
    fs::write(
        &with_all,
        "rc divider, operating point\nv1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op\n.save all v(out)\n.end\n",
    )
    .unwrap();
    let default = dir.join("default.raw");
    let selected = dir.join("all.raw");
    let run = simulate(&default, &plain);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let run = simulate(&selected, &with_all);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // `.save all` keeps the driver's whole set, so only the date may differ.
    assert_eq!(
        without_date(&fs::read_to_string(&default).unwrap()),
        without_date(&fs::read_to_string(&selected).unwrap()),
        "'.save all' must not reorder, rename or drop a vector"
    );
    assert_eq!(
        entries(&dir),
        ["all.cir", "all.raw", "deck.cir", "default.raw"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_dc_print_card_selects_and_prints_the_requested_vectors() {
    let dir = scratch("print-dc");
    let deck = write_deck(
        &dir,
        "rc divider dc sweep\nv1 in 0 dc 0\nr1 in out 1k\nr2 out 0 1k\n.dc v1 0 5 1\n\
         .print dc v(out) i(v1)\n.end\n",
    );
    let output = dir.join("dc.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("variables: sweep v(out) i(v1)"),
        "a sweep keeps its scale column first: {report}"
    );
    assert!(
        report.contains("print: 3 vector(s): sweep v(out) i(v1)"),
        "{report}"
    );
    assert!(
        report.contains("values: real with 15 fractional digits"),
        "{report}"
    );
    assert!(report.contains("  point  sweep  v(out)  i(v1)"), "{report}");

    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    assert_eq!(plot.variable_count(), 3);
    assert_eq!(plot.point_count(), 6);
    for point in 0..plot.point_count() {
        let volts = plot.value("sweep", point).unwrap().re;
        // The table spells the values exactly as the rawfile does.
        for (name, value) in [
            ("sweep", plot.value("sweep", point).unwrap()),
            ("v(out)", plot.value("v(out)", point).unwrap()),
            ("i(v1)", plot.value("i(v1)", point).unwrap()),
        ] {
            let row = format!("{point:>7}  ");
            let column = report
                .lines()
                .find(|line| line.starts_with(&row))
                .unwrap_or_else(|| panic!("row {point} in {report}"));
            assert!(
                column.contains(&spelled(value, false)),
                "{name} at point {point}: {column}"
            );
        }
        assert!((plot.value("v(out)", point).unwrap().re - volts / 2.0).abs() < 1e-12);
        assert!((plot.value("i(v1)", point).unwrap().re + volts / 2000.0).abs() < 1e-15);
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_ac_print_card_prints_the_complex_components() {
    let dir = scratch("print-ac");
    // The same circuit and sweep as the committed `rc_lowpass_ac` golden, with
    // the AC components the C reference prints spelled out.
    let deck = write_deck(
        &dir,
        "RC low-pass, AC sweep\nv1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\n\
         .ac lin 3 100 1k\n.print ac v(out) vm(out) vp(out) vdb(out) i(v1)\n.end\n",
    );
    let output = dir.join("ac.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("print: 6 vector(s): frequency v(out) vm(out) vp(out) vdb(out) i(v1)"),
        "{report}"
    );
    assert!(
        report.contains("complex as `re,im` with 15 fractional digits"),
        "the table states the convention it prints: {report}"
    );

    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    assert_eq!(plot.flags, PlotFlags::Complex);
    let names: Vec<&str> = plot.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "frequency",
            "v(out)",
            "vm(out)",
            "vp(out)",
            "vdb(out)",
            "i(v1)"
        ]
    );
    // The computed components are real vectors inside a complex plot, so the
    // rawfile writes them as `re,0.0` and the text table spells them `re`.
    assert!(plot.variables[2].is_real);
    assert_eq!(plot.variables[3].unit, "phase");
    assert_eq!(plot.variables[4].unit, "db");
    for point in 0..plot.point_count() {
        let out = plot.value("v(out)", point).unwrap();
        let magnitude = out.magnitude();
        assert!((plot.value("vm(out)", point).unwrap().re - magnitude).abs() < 1e-15);
        assert!(plot.value("vr(out)", point).is_none());
        let phase = plot.value("vp(out)", point).unwrap().re;
        assert!((phase - out.im.atan2(out.re)).abs() < 1e-15);
        assert!(
            (plot.value("vdb(out)", point).unwrap().re - 20.0 * magnitude.log10()).abs() < 1e-12
        );
    }
    // The selected columns still reproduce the committed C golden's data.
    let want = golden("rc_lowpass_ac").single_plot().unwrap().clone();
    for name in ["frequency", "v(out)", "i(v1)"] {
        let got = plot.column(name).expect("selected");
        let expected = want.column(name).expect("the golden has it");
        for (point, (a, b)) in got.iter().zip(&expected).enumerate() {
            assert!(
                (*a - *b).magnitude() <= 1e-10 * b.magnitude() + 1e-12,
                "{name} at point {point}: {a} != {b}"
            );
        }
    }
    // The C golden pins the physical values (C: vm(out) = 8.467330e-01 and
    // vp(out) = -5.60982e-01 rad at 100 Hz).
    assert!((plot.value("vm(out)", 0).unwrap().re - 0.8467330).abs() < 1e-6);
    assert!((plot.value("vp(out)", 0).unwrap().re + 0.5609821).abs() < 1e-6);
    assert!((plot.value("vdb(out)", 0).unwrap().re + 1.4450701).abs() < 1e-6);
    // And the table prints the computed components: a component that is real at
    // every point is one number (the rawfile spells that column `re,0.0`).
    assert!(
        report.contains(&spelled(plot.value("vm(out)", 0).unwrap(), false)),
        "{report}"
    );
    assert!(
        report.contains(&spelled(plot.value("v(out)", 0).unwrap(), true)),
        "{report}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_save_card_and_a_print_card_combine_and_the_analysis_must_match() {
    let dir = scratch("save-print");
    let deck = write_deck(
        &dir,
        "rc divider, op and print\nv1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op\n\
         .save v(in)\n.print op v(out)\n.end\n",
    );
    let output = dir.join("out.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    // The written set is the union in C's `dbs` order; the table is what
    // `.print` asked for.
    assert!(report.contains("variables: v(in) v(out)"), "{report}");
    assert!(report.contains("print: 1 vector(s): v(out)"), "{report}");
    assert!(
        !report.contains("print: 2 vector(s)"),
        "the table is the .print set, not the whole selection: {report}"
    );

    // A `.print` card for an analysis this run is not can never be honoured.
    let wrong = write_deck(
        &dir,
        "rc divider, wrong print\nv1 in 0 dc 5\nr1 out 0 1k\n.op\n.print dc v(out)\n.end\n",
    );
    let run = simulate(&output, &wrong);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("names a different analysis"),
        "{}",
        stderr(&run)
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_print_card_with_all_prints_every_vector_the_run_produced() {
    // `all` on a `.print` card is C's "print everything": the table must list
    // the run's vectors and print their values, not claim zero vectors.
    let dir = scratch("print-all");
    let deck = write_deck(
        &dir,
        "rc divider, print all\nv1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op\n\
         .print op all\n.end\n",
    );
    let output = dir.join("out.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("print: 3 vector(s): v(in) v(out) i(v1)"),
        "{report}"
    );
    assert!(!report.contains("<none>"), "{report}");
    // The values are the ones the rawfile got, with the current's C sign.
    assert!(report.contains("2.500000000000000e+00"), "{report}");
    assert!(report.contains("-2.500000000000000e-03"), "{report}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_unresolvable_save_card_fails_before_anything_is_published() {
    let dir = scratch("save-missing");
    let deck = write_deck(
        &dir,
        "rc divider, missing vector\nv1 in 0 dc 5\nr1 out 0 1k\n.op\n.save v(nosuch)\n.end\n",
    );
    let output = dir.join("keep.raw");
    fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(run.stdout.is_empty(), "{}", stdout(&run));
    let stderr = stderr(&run);
    assert!(stderr.contains("v(nosuch)"), "{stderr}");
    assert!(stderr.contains("deck.cir:5:7"), "{stderr}");
    assert_eq!(fs::read_to_string(&output).unwrap(), "PREVIOUS CONTENT\n");
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unsupported_and_malformed_selections_fail_explicitly() {
    // `.save i(r1)`: a resistor current is not observable in this port.
    let dir = scratch("save-unsupported");
    let cases = [
        (
            "i(r1)",
            3,
            "only a voltage source or inductor branch current",
        ),
        (
            "@r1[resistance]",
            3,
            "instance parameters are not observable",
        ),
        ("vm(out)", 2, "needs a complex plot"),
        ("v(a,a)", 2, "identically zero"),
        ("power(v1)", 2, "unknown vector request"),
    ];
    for (card, status, message) in cases {
        let deck = write_deck(
            &dir,
            &format!(
                "rc divider, unsupported\nv1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op\n.save {card}\n.end\n"
            ),
        );
        let output = dir.join("keep.raw");
        fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
        let run = simulate(&output, &deck);
        assert_eq!(run.status.code(), Some(status), "{card}: {}", stderr(&run));
        assert!(run.stdout.is_empty(), "{card}: {}", stdout(&run));
        assert!(stderr(&run).contains(message), "{card}: {}", stderr(&run));
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "PREVIOUS CONTENT\n",
            "{card}: the destination survives"
        );
    }
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

/// The `.measure` cards of a deck, evaluated over the full plot.
///
/// The measurement block is appended after the report (and after a `.print`
/// table), and a measurement never changes the written rawfile: this test
/// compares the rawfiles of the same deck with and without `.measure`.
#[test]
fn a_measure_card_is_measured_over_the_full_plot_and_leaves_the_rawfile_alone() {
    let dir = scratch("measure-tran");
    let deck = write_deck(
        &dir,
        "rc delay\nv1 in 0 pulse(0 1 0 1n 1n 1 2)\nr1 in out 1k\nc1 out 0 1u\n.tran 1u 5m\n\
         .meas tran tdelay trig v(in) val=0.5 rise=1 targ v(out) val=0.5 rise=1\n\
         .meas tran vavg avg v(out) from=0 to=5m\n.end\n",
    );
    let output = dir.join("with.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("measure: 2 result(s), evaluated on the full plot"),
        "{report}"
    );
    // The RC delay is R*C*ln(2) = 6.93147e-4 s, and the average of the rising
    // exponential over [0, 5ms] is (5m - RC*(1 - e^-5))/5m = 8.01347e-1 V.
    assert!(
        report.contains("tdelay              =  6.93147"),
        "{report}"
    );
    assert!(
        report.contains("vavg                =  8.01347"),
        "{report}"
    );
    assert!(
        report.contains("targ="),
        "the delay echoes both events: {report}"
    );

    let without = dir.join("without.cir");
    fs::write(
        &without,
        "rc delay\nv1 in 0 pulse(0 1 0 1n 1n 1 2)\nr1 in out 1k\nc1 out 0 1u\n.tran 1u 5m\n.end\n",
    )
    .unwrap();
    let plain = dir.join("without.raw");
    let run = simulate(&plain, &without);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        !stdout(&run).contains("measure:"),
        "a deck without .measure keeps today's report: {}",
        stdout(&run)
    );
    assert_eq!(
        without_date(&fs::read_to_string(&output).unwrap()),
        without_date(&fs::read_to_string(&plain).unwrap()),
        "a .measure card must not change the written rawfile"
    );
    assert_eq!(
        entries(&dir),
        ["deck.cir", "with.raw", "without.cir", "without.raw"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_measured_operand_the_output_selection_dropped_is_still_measurable() {
    let dir = scratch("measure-hidden");
    let deck = write_deck(
        &dir,
        "rc delay, measured but not written\nv1 in 0 pulse(0 1 0 1n 1n 1 2)\nr1 in out 1k\n\
         c1 out 0 1u\n.tran 1u 5m\n.save v(in)\n.meas tran vout_max max v(out)\n.end\n",
    );
    let output = dir.join("tran.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(report.contains("variables: time v(in)"), "{report}");
    assert!(
        report.contains("vout_max            =  9.93262"),
        "the measurement reads the full plot: {report}"
    );
    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    assert_eq!(plot.value("v(out)", 0), None, "v(out) was not written");
    assert!(plot.value("v(in)", 0).is_some());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_or_unsupported_measurement_publishes_nothing() {
    let dir = scratch("measure-fail");
    // The destination must survive, exactly as it does for a bad `.save`.
    let output = dir.join("keep.raw");
    for (card, status, message) in [
        (
            ".meas tran vat find v(out) at=1",
            2,
            "is outside the time range",
        ),
        (".meas tran x when v(out)=2", 3, "the when measurement"),
        (".meas tran x frobnicate v(out)", 2, "no such measurement"),
        (
            ".meas dc x max v(out)",
            2,
            "the card names a .dc measurement",
        ),
        (".meas tran x avg v(out) to=0", 2, "has no width"),
    ] {
        let deck = write_deck(
            &dir,
            &format!(
                "rc delay\nv1 in 0 pulse(0 1 0 1n 1n 1 2)\nr1 in out 1k\nc1 out 0 1u\n\
                 .tran 1u 5m\n{card}\n.end\n"
            ),
        );
        fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
        let run = simulate(&output, &deck);
        assert_eq!(run.status.code(), Some(status), "{card}: {}", stderr(&run));
        assert!(run.stdout.is_empty(), "{card}: {}", stdout(&run));
        assert!(stderr(&run).contains(message), "{card}: {}", stderr(&run));
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "PREVIOUS CONTENT\n",
            "{card}: the destination survives"
        );
    }
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_later_failing_measure_card_publishes_none_of_the_other_results() {
    // Atomicity: the first and third cards are valid and the second fails, so a
    // partial-publish regression would leak their values to stdout.
    let dir = scratch("measure-partial");
    let output = dir.join("keep.raw");
    let deck = write_deck(
        &dir,
        "rc delay\nv1 in 0 pulse(0 1 0 1n 1n 1 2)\nr1 in out 1k\nc1 out 0 1u\n\
         .tran 1u 5m\n.meas tran first max v(out)\n.meas tran second find v(out) at=1\n\
         .meas tran third avg v(out)\n.end\n",
    );
    fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(report.is_empty(), "{report}");
    assert!(
        !report.contains("first") && !report.contains("third"),
        "a valid card must not be published when another fails: {report}"
    );
    assert!(
        stderr(&run).contains("outside the time range"),
        "{}",
        stderr(&run)
    );
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "PREVIOUS CONTENT\n",
        "the destination survives"
    );
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

/// A 1 kHz square wave (0 -> 1 V, 50 % duty) through a 1 kohm / 1 uF lowpass.
///
/// `v(in)` is the square wave itself: DC 0.5 and single-sided harmonic
/// amplitudes `2/(k*pi)` for odd `k` (0.63662 at the fundamental, 0.21221 at the
/// third), so every value in its Fourier block has an analytic counterpart.
/// `v(out)` is the filtered trace, scaled by `|H(f)| =
/// 1/sqrt(1 + (2*pi*f*R*C)^2)`, which is 0.157179 at 1 kHz and 0.052952 at
/// 3 kHz. PULSE is the periodic source this front end supports; a `sin(...)`
/// source is not part of the port's subset.
const FOUR_DECK: &str = "rc lowpass\nv1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\n\
     r1 in out 1k\nc1 out 0 1u\n.tran 1u 5m\n.four 1k v(in)\n.four 1k v(out)\n.end\n";

/// The scalar `key` of one Fourier block of a `simulate` report.
fn four_scalar(report: &str, block: &str, key: &str) -> f64 {
    let start = report
        .find(block)
        .unwrap_or_else(|| panic!("no '{block}' in:\n{report}"));
    for line in report[start..].lines() {
        let Some(rest) = line.trim().strip_prefix(key) else {
            continue;
        };
        let Some((_, value)) = rest.split_once('=') else {
            continue;
        };
        return value
            .split_whitespace()
            .next()
            .expect("a value")
            .parse()
            .expect("a number");
    }
    panic!("no '{key}' in '{block}':\n{report}");
}

/// The `(magnitude, phase)` of one harmonic row of a Fourier block.
fn four_harmonic(report: &str, block: &str, order: u32) -> (f64, f64) {
    let start = report
        .find(block)
        .unwrap_or_else(|| panic!("no '{block}' in:\n{report}"));
    for line in report[start..].lines() {
        let mut fields = line.split_whitespace();
        let Some(parsed) = fields.next().and_then(|first| first.parse::<u32>().ok()) else {
            continue;
        };
        if parsed != order {
            continue;
        }
        let _frequency: f64 = fields.next().expect("frequency").parse().expect("a number");
        let magnitude: f64 = fields.next().expect("magnitude").parse().expect("a number");
        let phase: f64 = fields.next().expect("phase").parse().expect("a number");
        return (magnitude, phase);
    }
    panic!("no harmonic {order} in '{block}':\n{report}");
}

/// The `.four` cards of a deck, transformed over the final complete period.
///
/// The Fourier block is appended after the report (and after the `.print` table
/// and `.measure` block), and a transform never changes the written rawfile:
/// this test compares the rawfiles of the same deck with and without `.four`.
#[test]
fn a_four_card_transforms_the_final_period_and_leaves_the_rawfile_alone() {
    let dir = scratch("four-tran");
    let deck = write_deck(&dir, FOUR_DECK);
    let output = dir.join("with.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(
        report.contains("four: 2 analysis(es) of the final complete period"),
        "{report}"
    );
    assert!(report.contains("Fourier analysis for v(in):"), "{report}");
    assert!(report.contains("Fourier analysis for v(out):"), "{report}");

    // The source is the analytic square wave: DC 0.5, fundamental 2/pi and
    // third harmonic 2/(3*pi). On N=200 subintervals the ideal sampled square
    // wave's relative amplitude bias is x/sin(x)-1, x=pi*k/N (below 0.34 %
    // through k=9); the 1 ns ramps are negligible on a 1 ms period.
    let dc = four_scalar(&report, "Fourier analysis for v(in):", "dc");
    assert!((dc - 0.5).abs() < 1e-3, "dc = {dc}");
    let (fundamental, source_phase) = four_harmonic(&report, "Fourier analysis for v(in):", 1);
    assert!(
        (fundamental - 2.0 / std::f64::consts::PI).abs() < 0.01,
        "2/pi = {}, got {fundamental}",
        2.0 / std::f64::consts::PI
    );
    let (third, _) = four_harmonic(&report, "Fourier analysis for v(in):", 3);
    assert!(
        (third - 2.0 / (3.0 * std::f64::consts::PI)).abs() < 0.005,
        "2/(3*pi) = {}, got {third}",
        2.0 / (3.0 * std::f64::consts::PI)
    );

    for order in [7, 9] {
        let (magnitude, _) = four_harmonic(&report, "Fourier analysis for v(in):", order);
        let expected = 2.0 / (f64::from(order) * std::f64::consts::PI);
        assert!((magnitude - expected).abs() < 0.005 * expected + 1e-6);
    }

    // Compare the transfer-function phase, not an absolute output phase:
    // subtract the source's phase to cancel the discrete square wave's phase
    // bias and the common window reference. The lowpass delay is -atan(w*RC).
    let gain =
        |frequency: f64| (1.0 + (2.0 * std::f64::consts::PI * frequency * 1e-3).powi(2)).sqrt();
    let (filtered, phase) = four_harmonic(&report, "Fourier analysis for v(out):", 1);
    let expected = 2.0 / std::f64::consts::PI / gain(1e3);
    assert!(
        (filtered - expected).abs() < 0.003,
        "filtered fundamental = {expected}, got {filtered}"
    );
    let expected_phase = -(2.0 * std::f64::consts::PI * 1e3 * 1e-3).atan();
    assert!(
        (phase - source_phase - expected_phase).abs() < 0.02,
        "transfer phase = {expected_phase}, got {}",
        phase - source_phase
    );
    let (filtered_third, _) = four_harmonic(&report, "Fourier analysis for v(out):", 3);
    let expected_third = 2.0 / (3.0 * std::f64::consts::PI) / gain(3e3);
    assert!(
        (filtered_third - expected_third).abs() < 0.002,
        "filtered third = {expected_third}, got {filtered_third}"
    );

    let without = dir.join("without.cir");
    fs::write(
        &without,
        "rc lowpass\nv1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\nr1 in out 1k\nc1 out 0 1u\n\
         .tran 1u 5m\n.end\n",
    )
    .unwrap();
    let plain = dir.join("without.raw");
    let run = simulate(&plain, &without);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let plain_report = stdout(&run);
    assert!(
        !plain_report.contains("four:"),
        "a deck without .four keeps today's report: {plain_report}"
    );
    assert_eq!(
        without_date(&fs::read_to_string(&output).unwrap()),
        without_date(&fs::read_to_string(&plain).unwrap()),
        "a .four card must not change the written rawfile"
    );
    assert_eq!(
        entries(&dir),
        ["deck.cir", "with.raw", "without.cir", "without.raw"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_transformed_vector_the_output_selection_dropped_is_still_transformed() {
    let dir = scratch("four-hidden");
    let deck = write_deck(
        &dir,
        "rc lowpass, transformed but not written\n\
         v1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\nr1 in out 1k\nc1 out 0 1u\n.tran 1u 5m\n\
         .save v(in)\n.four 1k v(out)\n.end\n",
    );
    let output = dir.join("tran.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let report = stdout(&run);
    assert!(report.contains("variables: time v(in)"), "{report}");
    assert!(report.contains("Fourier analysis for v(out):"), "{report}");
    let written = RawFile::load(&output).expect("parses");
    let plot = written.single_plot().unwrap();
    assert_eq!(plot.value("v(out)", 0), None, "v(out) was not written");
    // The hidden vector is still transformed by the lowpass, not merely listed.
    let (filtered, _) = four_harmonic(&report, "Fourier analysis for v(out):", 1);
    let gain = (1.0 + (2.0 * std::f64::consts::PI * 1e3 * 1e-3).powi(2)).sqrt();
    let expected = 2.0 / std::f64::consts::PI / gain;
    assert!(
        (filtered - expected).abs() < 0.003,
        "filtered fundamental = {expected}, got {filtered}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_or_unsupported_four_card_publishes_nothing() {
    let dir = scratch("four-fail");
    // The destination must survive, exactly as it does for a bad `.save`.
    let output = dir.join("keep.raw");
    for (card, status, message) in [
        // A period longer than the run.
        (".four 100 v(out)", 2, "the run spans"),
        (".four 1k v(nosuch)", 2, "v(nosuch)"),
        (".four 1k v(out) HARMONICS=101", 2, "bounded Fourier budget"),
        (".four 0 v(out)", 2, "greater than zero"),
        (".four 1k vm(out)", 2, "is an AC component"),
        (".four 1k v(out) bogus=1", 2, "no such .four parameter"),
        (".four 1k all", 2, "cannot be transformed"),
        (".four 1k v(out) NFREQS=4", 3, "the nfreqs= .four parameter"),
    ] {
        let deck = write_deck(
            &dir,
            &format!(
                "rc lowpass\nv1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\nr1 in out 1k\nc1 out 0 1u\n\
                 .tran 1u 5m\n{card}\n.end\n"
            ),
        );
        fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
        let run = simulate(&output, &deck);
        assert_eq!(run.status.code(), Some(status), "{card}: {}", stderr(&run));
        assert!(run.stdout.is_empty(), "{card}: {}", stdout(&run));
        assert!(stderr(&run).contains(message), "{card}: {}", stderr(&run));
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "PREVIOUS CONTENT\n",
            "{card}: the destination survives"
        );
    }
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_four_card_in_a_run_that_is_not_transient_is_refused_at_evaluation() {
    let dir = scratch("four-ac");
    let deck = write_deck(
        &dir,
        "rc lowpass\nv1 in 0 ac 1\nr1 in out 1k\nc1 out 0 1u\n.ac dec 10 10 10k\n\
         .four 1k v(out)\n.end\n",
    );
    let output = dir.join("keep.raw");
    fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stdout(&run).is_empty(), "{}", stdout(&run));
    assert!(
        stderr(&run).contains("transforms a .tran result"),
        "{}",
        stderr(&run)
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "PREVIOUS CONTENT\n");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn m6_features_combine_in_one_multi_analysis_deck() {
    // Integration of the M6 slices: `.func` and quoted/braced values feeding
    // E/G/H gains, a SIN source, DC-only options (`itl1`/`itl2`/`srcsteps`/
    // `gminsteps`) that also bound the companion `.tran` initial bias, and two
    // analyses in one rawfile. ngspice-47 batch mode gives the same `op1 tran1`
    // plots, v(o) = v(p) = 6, v(q) = 3, v(s) = -1, i(e1) = -6e-3 and
    // i(h1) = 1e-3 for this deck.
    let dir = scratch("m6-combined");
    let deck = write_deck(
        &dir,
        "m6 combined\n.func g(x) {x*2}\n.param k=3\n\
         v1 a 0 dc 1 sin(0 1 1k)\nr0 a 0 1k\n\
         e1 o 0 a 0 {g(k)}\ne2 p 0 a 0 'k*2'\ng1 0 q a 0 '1m*k'\nrq q 0 1k\n\
         h1 s 0 v1 {g(500)}\nrs s 0 1k\nrp p 0 1k\nro o 0 1k\n\
         .options itl1=60 itl2=70 srcsteps=3 gminsteps=5\n\
         .tran 20u 1m\n.op\n.end\n",
    );
    let output = dir.join("combined.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("plots:     op1 tran1 (ngspice batch order)"),
        "{}",
        stdout(&run)
    );
    let written = RawFile::load(&output).expect("parses");
    assert_eq!(
        plotnames(&written),
        ["Operating Point", "Transient Analysis"]
    );
    let op = &written.plots[0].plot;
    for (name, expected) in [
        ("v(o)", 6.0),
        ("v(p)", 6.0),
        ("v(q)", 3.0),
        ("v(s)", -1.0),
        ("i(e1)", -6e-3),
        ("i(h1)", 1e-3),
    ] {
        let value = op.value(name, 0).expect(name).re;
        assert!(
            (value - expected).abs() <= 1e-12 * expected.abs(),
            "{name}: {value} versus C {expected}"
        );
    }
    let tran = &written.plots[1].plot;
    let last = tran.point_count() - 1;
    assert_eq!(tran.value("time", last).unwrap().re, 1e-3);
    assert!(tran.value("v(o)", last).unwrap().re.abs() < 1e-9);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_m6_gate_deck_runs_every_analysis_end_to_end() {
    // The M6 exit gate: a SIN-driven 1:2 K transformer into a non-inverting
    // G/E op-amp stage (gain {gain} = 10 from `.param`, rf = {(gain-1)*rg}),
    // a `.func` B limiter v(lim) = 2 tanh(v(out)/2) and an H sense
    // v(isense) = 100 i(vsense) = v(lim)/10. `golden verify` compares every
    // plot with C; this checks the circuit relations the port must honour.
    let dir = scratch("m6_gate");
    let output = dir.join("gate.raw");
    let run = simulate(&output, &fixture("m6_gate"));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("plots:     ac1 dc1 op1 tran1 (ngspice batch order)"),
        "{}",
        stdout(&run)
    );
    let written = RawFile::load(&output).expect("parses");
    let committed = golden("m6_gate");
    assert_eq!(plotnames(&written), plotnames(&committed));
    // The same vectors as C (golden verify matches them by name; C's column
    // order differs for this deck).
    for (got, want) in written.plots.iter().zip(&committed.plots) {
        let mut want_names = variable_names(&want.plot);
        if want_names.first() == Some(&"v(v-sweep)") {
            want_names[0] = "sweep";
        }
        let mut got_names = variable_names(&got.plot);
        got_names.sort_unstable();
        want_names.sort_unstable();
        assert_eq!(got_names, want_names, "{}", got.plot.plotname);
    }
    let column = |plot: usize, name: &str| {
        written.plots[plot]
            .plot
            .column(name)
            .unwrap_or_else(|| panic!("plot {plot}: no {name}"))
    };
    let close = |what: &str, got: f64, want: f64, relative: f64| {
        assert!(
            (got - want).abs() <= relative * want.abs() + 1e-9,
            "{what}: {got} != {want}"
        );
    };
    // AC at 10 Hz: the closed-loop gain is 10 (the loop gain is about 1e4
    // there), and the limiter's small-signal slope at the 0 V bias is 1.
    let (s1, out, lim, isense) = (
        column(0, "v(s1)")[0],
        column(0, "v(out)")[0],
        column(0, "v(lim)")[0],
        column(0, "v(isense)")[0],
    );
    close("ac gain", (out / s1).magnitude(), 10., 1e-3);
    close("ac limiter", (lim - out).magnitude(), 0., 1e-9);
    close(
        "ac sense",
        (isense * ngspice_rs::primitives::Complex::real(10.) - lim).magnitude(),
        0.,
        1e-9,
    );
    // DC: the secondary shorts v(s1) to ground, so v(out) = -9 vref, and the
    // limiter and sense follow it exactly at every point.
    let sweep = column(1, "sweep");
    let (out, lim, isense) = (
        column(1, "v(out)"),
        column(1, "v(lim)"),
        column(1, "v(isense)"),
    );
    for point in 0..sweep.len() {
        let vref = sweep[point].re;
        close("dc out", out[point].re, -9. * vref, 1e-3);
        close(
            "dc limiter",
            lim[point].re,
            2. * (out[point].re / 2.).tanh(),
            1e-9,
        );
        close("dc sense", isense[point].re, lim[point].re / 10., 1e-9);
    }
    // The operating point is at rest: every source is 0 V at DC.
    for value in &written.plots[2].plot.points[0] {
        assert!(value.magnitude() < 1e-9, "{value}");
    }
    // Transient: the 0.1 V, 1 kHz drive reaches about 2 V at v(out), where
    // the limiter visibly compresses; it stays algebraic at every point.
    let (out, lim, isense) = (
        column(3, "v(out)"),
        column(3, "v(lim)"),
        column(3, "v(isense)"),
    );
    let peak = out.iter().map(|value| value.re.abs()).fold(0., f64::max);
    assert!((1.5..2.5).contains(&peak), "peak {peak}");
    for point in 0..out.len() {
        close(
            "tran limiter",
            lim[point].re,
            2. * (out[point].re / 2.).tanh(),
            1e-6,
        );
        close("tran sense", isense[point].re, lim[point].re / 10., 1e-6);
    }
    let lim_peak = lim.iter().map(|value| value.re.abs()).fold(0., f64::max);
    assert!(lim_peak < 0.9 * peak, "limiter {lim_peak} vs {peak}");
    fs::remove_dir_all(&dir).unwrap();
}
