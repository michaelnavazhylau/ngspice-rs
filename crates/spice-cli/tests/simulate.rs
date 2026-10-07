//! Process-level checks for `spice-rs simulate`: the exit-code contract, the
//! exactly-one-analysis rule, the ASCII rawfile the command writes and the
//! temporary-file/rename guarantee that a failed run never leaves or destroys
//! the destination.
//!
//! The rawfiles are read back with the production ASCII reader and compared with
//! the committed C goldens variable by variable, by name. Nothing here
//! re-captures a golden.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use spice_analysis::{PlotFlags, RawFile};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
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
    let committed = golden(name);
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
    let dir = scratch("noise");
    let deck = write_deck(
        &dir,
        "noise deck\nv1 in 0 dc 1 ac 1\nr1 in out 1k\nc1 out 0 1u\n\
         .noise v(out) v1 dec 10 1 1k\n.end\n",
    );
    let output = dir.join("noise.raw");
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(run.stdout.is_empty(), "{}", stdout(&run));
    assert!(
        stderr(&run).contains(".noise analysis has no driver"),
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

#[test]
fn two_analysis_cards_are_rejected_and_the_destination_survives() {
    let dir = scratch("two-analyses");
    let deck = write_deck(
        &dir,
        "two analyses\nv1 in 0 dc 1\nr1 out 0 1k\n.op\n.ac dec 2 1 10\n.end\n",
    );
    let output = dir.join("keep.raw");
    fs::write(&output, "PREVIOUS CONTENT\n").unwrap();
    let run = simulate(&output, &deck);
    assert_eq!(run.status.code(), Some(3), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("exactly one analysis"),
        "{}",
        stderr(&run)
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "PREVIOUS CONTENT\n");
    assert_eq!(entries(&dir), ["deck.cir", "keep.raw"]);
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
        text.contains("0 success, 1 bad command line, 2 deck could not be read, 3 not ported yet"),
        "{text}"
    );
}
