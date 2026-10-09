//! Opt-in #44 live C comparison for `.four`.
//!
//! Run it explicitly with the C reference binary:
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_four_reference -- --ignored
//! ```
//!
//! Ordinary `cargo test` runs never need C: every test here is `#[ignore]`d. Both
//! engines read the *same* deck (no `HARMONICS=`, which C does not accept: it
//! takes the harmonic count from an interactive `set nfreqs`), so the port's
//! `DEFAULT_HARMONICS = 9` tabulates exactly C's rows `1..=9` beside its `0` row.
//!
//! Both engines use 200 subintervals per period for the default harmonic count.
//! C uses a half-open grid, the port a closed trapezoid rule, and each engine
//! produces its own transient samples. C prints six significant digits and
//! phases in **degrees** (the port uses radians), so the comparison converts
//! once, explicitly. Analytic checks of the same deck live in
//! `tests/simulate.rs`.
//!
//! Nothing here reads or rewrites a committed golden, and the deck uses PULSE
//! (the periodic source this front end supports) rather than a `sin(...)` source.

use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{RunConfig, fourier, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::Real;

/// A 1 kHz square wave (0 -> 1 V) through a 1 kohm / 1 uF lowpass.
///
/// No title line: both callers prepend their own, because the first line of a
/// deck is its title in both engines.
const DECK: &str = "v1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\n\
     r1 in out 1k\nc1 out 0 1u\n.tran 1u 5m\n";

/// Builds the shared deck text: a title, [`DECK`], the `.four` card, `.end`.
fn deck_text(vector: &str) -> String {
    format!("Fourier oracle\n{DECK}.four 1k {vector}\n.end\n")
}

/// Removes the temporary directory even when the test fails.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The port's own Fourier analysis of `vector`, over its own transient plot.
fn port_analysis(vector: &str) -> fourier::FourierAnalysis {
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("four.cir"), &deck_text(vector)))
        .expect("the deck parses");
    assert_eq!(parsed.fourier.len(), 1, "one .four card");
    let netlist = &parsed.netlist;
    let card = netlist.analyses.first().expect("one analysis");
    let config = RunConfig::from_netlist(netlist).expect("the deck configures");
    let request = config.request_for(card).expect("the analysis request");
    let mut circuit = config.circuit(netlist).expect("the circuit builds");
    let plot = runner(request.kind)
        .expect("a driver")
        .run(&mut circuit, &request, &config.context())
        .expect("the run succeeds");
    let results =
        fourier::resolve(&plot, request.kind, &parsed.fourier).expect("the card resolves");
    assert_eq!(results.len(), 1, "one transformed vector");
    results.into_iter().next().unwrap()
}

/// One row of C's Fourier table.
struct CRow {
    order: u32,
    magnitude: Real,
    phase_degrees: Real,
}

/// C's own Fourier table for `vector`, as `(dc, thd_percent, rows)`.
fn c_analysis(tag: &str, vector: &str) -> (Real, Real, Vec<CRow>) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let dir = std::env::temp_dir().join(format!("spice-four-oracle-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).expect("scratch directory");
    let _cleanup = Cleanup(dir.clone());
    fs::write(dir.join("four.cir"), deck_text(vector)).expect("write the deck");
    let run = Command::new(&binary)
        .args(["-b", "four.cir"])
        .current_dir(&dir)
        .output()
        .expect("run ngspice");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.status.success(), "{tag}: {output}");
    assert!(
        output.contains(&format!("Fourier analysis for {vector}:")),
        "{tag}: C printed no Fourier block:\n{output}"
    );

    let mut dc = None;
    let mut thd = None;
    let mut rows = Vec::new();
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("No. Harmonics:") {
            // `No. Harmonics: 10, THD: 12.2679 %, Gridsize: 200, ...`
            let percent = rest
                .split_once("THD:")
                .and_then(|(_, after)| after.split_whitespace().next())
                .expect("a THD field")
                .parse::<Real>()
                .expect("a number");
            thd = Some(percent);
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(order) = fields.next().and_then(|first| first.parse::<u32>().ok()) else {
            continue;
        };
        let Some(frequency) = fields.next().and_then(|value| value.parse::<Real>().ok()) else {
            continue;
        };
        let Some(magnitude) = fields.next().and_then(|value| value.parse::<Real>().ok()) else {
            continue;
        };
        let Some(phase) = fields.next().and_then(|value| value.parse::<Real>().ok()) else {
            continue;
        };
        if order == 0 || frequency == 0.0 {
            dc = Some(magnitude);
            continue;
        }
        rows.push(CRow {
            order,
            magnitude,
            phase_degrees: phase,
        });
    }
    let dc = dc.expect("C printed a DC row");
    let thd = thd.expect("C printed a THD");
    assert!(
        !rows.is_empty(),
        "{tag}: C printed no harmonic rows:\n{output}"
    );
    (dc, thd, rows)
}

/// Runs both engines on the same deck and compares DC, THD and every harmonic
/// the port tabulated.
///
/// The bounds cover independent transient sampling, interpolation and C's
/// printed precision, not a difference in default grid resolution. All nine
/// magnitudes must be compared, with at least five significant phase checks.
///
/// Phases are only compared for harmonics that carry at least 1 % of the
/// fundamental: the even harmonics of a symmetric square wave are zero in both
/// engines, and the phase of a 1e-4-of-fundamental residue is numerical noise,
/// not a physical disagreement.
fn compare(
    tag: &str,
    vector: &str,
    magnitude_relative: Real,
    magnitude_floor: Real,
    phase_degrees: Real,
) {
    let ours = port_analysis(vector);
    let (c_dc, c_thd_percent, c_rows) = c_analysis(tag, vector);

    // DC is a mean over the same window on two grids; the filter's ripple is
    // ~1e-2 V, so 1e-3 is a grid-level bound, not a physical one.
    assert!(
        (ours.dc - c_dc).abs() <= 1e-3,
        "{tag}: dc: port {}, C {c_dc}",
        ours.dc
    );
    let ours_percent = ours.thd * 100.0;
    assert!(
        (ours_percent - c_thd_percent).abs() <= 0.01 * c_thd_percent.abs() + 0.05,
        "{tag}: THD: port {ours_percent} %, C {c_thd_percent} %"
    );

    let mut compared_magnitudes = 0;
    let mut compared_phases = 0;
    for row in &c_rows {
        let Some(harmonic) = ours
            .harmonics
            .iter()
            .find(|harmonic| harmonic.order == row.order)
        else {
            // C prints 10 rows (`0..=9`); the port tabulates its own count. A
            // harmonic C printed but the port did not is only acceptable when
            // the port's count is smaller, which the loop below checks.
            continue;
        };
        assert_eq!(harmonic.frequency, row.order as Real * ours.fundamental);
        compared_magnitudes += 1;
        let error = (harmonic.amplitude - row.magnitude).abs();
        assert!(
            error <= magnitude_relative * row.magnitude.abs() + magnitude_floor,
            "{tag}: harmonic {} magnitude: port {}, C {} (error {error})",
            row.order,
            harmonic.amplitude,
            row.magnitude
        );
        // Radians with the sin() convention here, degrees there: convert once.
        if row.magnitude < 0.01 * ours.harmonics[0].amplitude {
            continue;
        }
        compared_phases += 1;
        let ours_degrees = harmonic.phase * 180.0 / PI;
        let difference = (ours_degrees - row.phase_degrees).abs();
        let wrapped = if difference > 180.0 {
            360.0 - difference
        } else {
            difference
        };
        eprintln!(
            "{tag}: k={} magnitude error {error:e}, phase difference {wrapped:.6} deg",
            row.order
        );
        assert!(
            wrapped <= phase_degrees,
            "{tag}: harmonic {} phase: port {ours_degrees} deg, C {} deg",
            row.order,
            row.phase_degrees
        );
    }
    assert_eq!(
        compared_magnitudes, 9,
        "{tag}: every default harmonic must be checked"
    );
    assert!(
        compared_phases >= 5,
        "{tag}: only {compared_phases} significant phase checks"
    );
    assert!(
        c_rows
            .iter()
            .filter(|row| ours.harmonics.iter().any(|h| h.order == row.order))
            .count()
            == ours.harmonics.len(),
        "{tag}: the port tabulated {} harmonics, C printed {} of them",
        ours.harmonics.len(),
        c_rows.len()
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn a_filtered_square_wave_transforms_like_c() {
    // The RC output is smooth (the filter removes the edges), so its spectrum
    // decays fast and the two grids agree closely.
    // A degree is less than one grid interval (1.8 degrees), and leaves
    // margin for independently sampled edges without excusing a phase reversal.
    compare("filtered", "v(out)", 0.01, 1e-4, 1.0);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn the_source_square_wave_transforms_like_c() {
    // Finite 1 ns pulse edges approximate the ideal square wave. The discrete
    // projection has relative bias x/sin(x)-1, x=pi*k/200, about 0.33 % at k=9
    // against the continuous series. Both engines share that grid resolution,
    // so every order can be compared without a high-order exclusion.
    compare("source", "v(in)", 0.01, 1e-4, 1.0);
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn a_branch_current_transforms_like_c() {
    // The resistor-limited source current jumps at pulse edges; both engines
    // still use the same grid resolution. The absolute floor is in amperes,
    // well below the fundamental (~6e-4 A), not a milliamperes-wide escape.
    compare("current", "i(v1)", 0.01, 1e-7, 1.0);
}
