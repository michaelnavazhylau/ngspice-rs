//! Opt-in live C comparison of the #94/#95 source functions.
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p spice-analysis --test c_source_functions -- --ignored
//! ```
//!
//! * **Waveform values.** Every source drives its own node (V directly, I through
//!   1 kohm), so C's node voltages *are* `vsrcload.c`/`isrcload.c` evaluations
//!   at C's own timepoints, with C's `CKTstep`/`CKTfinalTime` defaults. Each is
//!   compared with the port's [`spice_devices::Waveform`] at the same instant
//!   (left or right limit: C evaluates a jump instant once). No integration is
//!   involved, so the bound is round-off level.
//! * **Operating point.** The same sources without DC values under `.op` give
//!   C's time-zero values.
//! * **Fourier.** A SIN-driven diode clipper (strong harmonics) is transformed
//!   by both engines' `.four`; THD and harmonic magnitudes are compared.
//!
//! Ordinary `cargo test` never needs C: everything here is `#[ignore]`d.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use spice_analysis::{AnalysisContext, Plot, RawFile, RunConfig, fourier, runner};
use spice_core::Real;
use spice_devices::{Circuit, Limit, SourceKind, TransientTiming};
use spice_netlist::{Parser, source::parse_deck_text};

/// Each source drives node `n<k>`; I sources (`n<k>` to ground) pull current
/// through a 1 kohm load, so `v(n<k>) = -1k * I`.
const SOURCES: &str = "\
vsin1 n1 0 sin(0.5 2 1k 0.1m 300 45)
vsin2 n2 0 sin(1 1)
vsin3 n3 0 sine(0 1 0 0.3m)
vexp1 n4 0 exp(0 1)
vexp2 n5 0 exp(1 -1 0.3m 0.1m)
vexp3 n6 0 exp(0 1 0 0.2m 0.8m)
vsffm1 n7 0 sffm(0.1 1 5k 2 500 0.2m 30 60)
vsffm2 n8 0 sffm(0 1)
vsffm3 n9 0 sffm(0 1 10k 50 1k)
vam1 n10 0 am(0 1 0.5 1k 10k 0.1m 30 45)
vam2 n11 0 am(0.2 1)
vpulse1 n12 0 pulse(0 1 0.1m 20u 20u 0.1m 0.3m 2.5)
vpulse2 n13 0 pulse(0 1 0 20u 20u 0.1m 0.3m 3)
vpwl1 n14 0 pwl(0 0 0.3m 1 0.6m 0.5) r=0.3m td=0.1m
vpwl2 n15 0 pwl(0 0 0.3m 1 0.6m 0.5) td=0.2m
vpwl3 n16 0 pwl(0 0 0.2m 1 0.4m 0) r=0
isin n17 0 sin(0 1m 2k 0.05m 0 90)
iexp n18 0 exp(0 1m 0.1m 0.1m 0.5m 0.2m)
isffm n19 0 sffm(0 1m 4k 1 400)
iam n20 0 am(0 1m 0.5m 500 8k)
ipwl n21 0 pwl(0 0 0.1m 1m 0.2m 0) r=0 td=0.05m
ipulse n22 0 pulse(0 1m 0.1m 10u 10u 50u 0.2m 2)
";

fn loads() -> String {
    (1..=22).map(|k| format!("r{k} n{k} 0 1k\n")).collect()
}

/// Removes the temporary directory even when the test fails.
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs `cards` (title and `.end` added) under `analysis` in C, returning the
/// plot written by `write`, and the full output text.
fn run_c(tag: &str, cards: &str, analysis: &str, extra: &str) -> (Option<Plot>, String) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-sources-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    // An analysis command runs from a `.control` block and writes a rawfile;
    // without one the deck's own cards (e.g. `.tran` + `.four`) run in batch.
    let deck = if analysis.is_empty() {
        format!("c\n{cards}{extra}.end\n")
    } else {
        format!(
            "c\n{cards}{extra}.control\nset filetype=ascii\n{analysis}\nwrite result.raw\nquit\n.endc\n.end\n"
        )
    };
    fs::write(dir.join("c.cir"), deck).unwrap();
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
    let plot = fs::read_to_string(dir.join("result.raw"))
        .ok()
        .map(|raw| RawFile::parse(&raw).unwrap().plots[0].plot.clone());
    (plot, text)
}

fn rust_netlist(text: &str) -> spice_netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(Path::new("c.cir"), text))
        .unwrap()
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn source_function_values_match_c_at_c_timepoints() {
    let (step, stop) = (10e-6, 2e-3);
    let cards = format!("{SOURCES}{}", loads());
    let (plot, text) = run_c("values", &cards, "tran 10u 2m", "");
    let plot = plot.unwrap_or_else(|| panic!("no C rawfile:\n{text}"));
    let mut system = Circuit::from_netlist(&rust_netlist(&format!("t\n{cards}.end\n")))
        .unwrap()
        .linear_system()
        .unwrap();
    system
        .bind_transient_timing(&TransientTiming::new(step, stop).unwrap())
        .unwrap();
    let time: Vec<Real> = plot.column("time").unwrap().iter().map(|v| v.re).collect();
    assert!(time.len() > 200, "C wrote {} rows", time.len());
    let mut compared = 0;
    for (k, source) in system.sources.iter().enumerate() {
        let node = format!("v(n{})", k + 1);
        let scale = match source.kind {
            SourceKind::Voltage => 1.,
            SourceKind::Current => -1e3,
        };
        let c = plot
            .column(&node)
            .unwrap_or_else(|| panic!("C has no {node}"));
        for (t, c) in time.iter().zip(&c) {
            // C's rawfile prints time with 16 significant digits, so at a jump
            // its sample may sit an ulp to either side: the left and right
            // limits and the values 1e-14 relative away are all admissible.
            let delta = 1e-14 * t.abs();
            let candidates = [
                (*t, Limit::Left),
                (*t, Limit::Right),
                (t - delta, Limit::Left),
                (t + delta, Limit::Right),
            ]
            .map(|(at, limit)| scale * source.waveform.value_at(at, limit).unwrap());
            let error = candidates
                .iter()
                .map(|v| (v - c.re).abs())
                .fold(Real::INFINITY, Real::min);
            assert!(
                error <= 1e-9 * c.re.abs().max(1.),
                "{} at t={t:e}: C {:e}, port {candidates:?}",
                source.name,
                c.re
            );
            compared += 1;
        }
    }
    assert_eq!(system.sources.len(), 22);
    println!(
        "{compared} source values compared at {} C timepoints",
        time.len()
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn operating_points_match_c_time_zero_values() {
    let cards = format!("{SOURCES}{}", loads());
    let (plot, text) = run_c("op", &cards, "op", "");
    let c = plot.unwrap_or_else(|| panic!("no C rawfile:\n{text}"));
    let netlist = rust_netlist(&format!("t\n{cards}.op\n.end\n"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let rust = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &AnalysisContext::default())
        .unwrap();
    for k in 1..=22 {
        let name = format!("v(n{k})");
        let (ours, theirs) = (
            rust.value(&name, 0).unwrap().re,
            c.value(&name, 0).unwrap().re,
        );
        assert!(
            (ours - theirs).abs() <= 1e-12 * theirs.abs().max(1.),
            "{name}: port {ours}, C {theirs}"
        );
    }
}

/// A 1 kHz SIN into a resistor-diode clipper: strongly distorted, so the THD
/// and harmonic magnitudes are meaningful comparisons.
const CLIPPER: &str = "v1 in 0 sin(0 2 1k)\nr1 in out 1k\nd1 out 0 dm\nc1 out 0 10n\n\
                       .model dm d(is=1e-14)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn four_of_a_sin_driven_clipper_matches_c() {
    let text = format!("four\n{CLIPPER}.tran 1u 4m\n.four 1k v(out)\n.end\n");
    let parsed = Parser::new()
        .parse_deck_with_output(&parse_deck_text(Path::new("four.cir"), &text))
        .unwrap();
    let netlist = &parsed.netlist;
    let config = RunConfig::from_netlist(netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let ours = fourier::resolve(&plot, request.kind, &parsed.fourier)
        .unwrap()
        .remove(0);

    let (_, output) = run_c("four", CLIPPER, "", ".tran 1u 4m\n.four 1k v(out)\n");
    let mut thd = None;
    let mut rows = Vec::new();
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("No. Harmonics:") {
            thd = rest
                .split_once("THD:")
                .and_then(|(_, after)| after.split_whitespace().next())
                .and_then(|v| v.parse::<Real>().ok());
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if let [order, frequency, magnitude, ..] = fields[..]
            && let (Ok(order), Ok(frequency), Ok(magnitude)) = (
                order.parse::<u32>(),
                frequency.parse::<Real>(),
                magnitude.parse::<Real>(),
            )
            && order > 0
            && frequency > 0.
        {
            rows.push((order, magnitude));
        }
    }
    let thd = thd.unwrap_or_else(|| panic!("C printed no THD:\n{output}"));
    // A clipped sine is far from clean: both engines must see it.
    assert!(thd > 5., "C THD {thd} %");
    let ours_percent = ours.thd * 100.;
    assert!(
        (ours_percent - thd).abs() <= 0.01 * thd + 0.05,
        "THD: port {ours_percent} %, C {thd} %"
    );
    assert_eq!(rows.len(), 9, "{output}");
    for (order, magnitude) in rows {
        let harmonic = ours
            .harmonics
            .iter()
            .find(|h| h.order == order)
            .unwrap_or_else(|| panic!("no harmonic {order}"));
        assert!(
            (harmonic.amplitude - magnitude).abs() <= 0.01 * magnitude + 1e-4,
            "harmonic {order}: port {}, C {magnitude}",
            harmonic.amplitude
        );
    }
}
