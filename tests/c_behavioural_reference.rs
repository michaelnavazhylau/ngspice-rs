//! Opt-in live C validation of behavioural-source values and derivatives
//! (#79). `NGSPICE_BIN` must name the external reference binary; run with
//! `-- --ignored`.
//!
//! One deck holds a B source per function, all driven by `v(in)` (plus a
//! second bias `v(b)`). The operating point gives every function's value and
//! a one-point AC analysis with `ac 1` on `vin` gives its derivative with
//! respect to `v(in)` (`ASRCacLoad` stamps the same partials Newton uses), so
//! both C's function values and its derivative rules — including the
//! `PTdifferentiate` quirks the port reproduces — are compared at several
//! bias points with `|Rust - C| <= 1e-12 |C| + 1e-15`, tight enough to
//! resolve `inp_modify_exp()`'s 11-digit literal rounding (the last case).
use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use std::{fs, path::Path, process::Command};

/// The functions under test, as expressions of `v(in)` and `v(b)`.
const EXPRESSIONS: &[&str] = &[
    "abs(v(in) - 0.1)",
    "acos(0.5*v(in))",
    "acosh(2 + v(in)^2)",
    "asin(0.5*v(in))",
    "asinh(3*v(in))",
    "atan(2*v(in) + v(b))",
    "atanh(0.5*v(in))",
    "cos(4*v(in))",
    "cosh(v(in))",
    "exp(2*v(in))",
    "ln(2 + v(in))",
    "log(2 + v(in))",
    "log10(2 + v(in))",
    "sgn(v(in) - 0.1)",
    "sin(3*v(in))",
    "sinh(v(in))",
    "sqrt(2 + v(in))",
    "tan(0.5*v(in))",
    "tanh(2*v(in))",
    "u(v(in) - 0.1)",
    "uramp(v(in) - 0.1)",
    "ceil(3*v(in))",
    "floor(3*v(in))",
    "nint(3*v(in))",
    "-v(in)*v(b)",
    "u2(0.4*v(in) + 0.3)",
    "pwl(v(in), -1, 2, 0, 0, 0.5, 1, 2, 3)",
    "pwl(v(in), 2, 3, 0.5, 1, 0, 0, -1, 2)",
    "eq0(v(in)) + ne0(v(in)) + gt0(v(in)) + lt0(v(in)) + ge0(v(in)) + le0(v(in))",
    "pow(v(in) + 2, v(b) + 1)",
    "pow(v(b) + 2, v(in))",
    "pow(v(in), 3)",
    "pwr(v(in), 2.5)",
    "pwr(v(in) + 2, v(b))",
    "min(v(in), v(b)) + max(2*v(in), v(b))",
    "ternary_fcn(v(in) > 0, v(in)^2, v(in)^3)",
    "v(in)/(2 + v(b))",
    "(1 + v(b))/(v(in) + 3)",
    "(v(in) + 3)^(v(b) + 1)",
    "v(in)**3",
    "v(in)^2.5",
    "2^v(in)",
    "v(in) > v(b) ? exp(v(in)) : cos(v(in))",
    "(v(in) < 0.5 && v(b) != 0) + (v(in) >= 0 || v(b) <= 0)",
    "temper*v(in) + pi + e",
    "hertz*v(in)^2 + hertz",
    "1.23456789012345*v(in)",
];

/// The deck body without an analysis: `vin` at `bias` with `ac 1`.
fn cards(bias: f64) -> String {
    let mut text = format!(
        "behavioural reference\nvin in 0 dc {bias} ac 1\nrin in 0 1k\nvb b 0 dc 0.35\nrb b 0 1k\n"
    );
    for (index, expression) in EXPRESSIONS.iter().enumerate() {
        text.push_str(&format!(
            "b{index} o{index} 0 v={expression}\nr{index} o{index} 0 1k\n"
        ));
    }
    text
}

/// Runs `cards` in C with `analysis` and returns the written plot.
fn run_c(tag: &str, cards: &str, analysis: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!(
        "spice-behavioural-ref-{}-{tag}",
        std::process::id()
    ));
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
            "{cards}.control\nset filetype=ascii\n{analysis}\nwrite result.raw\nquit\n.endc\n.end\n"
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

fn run_rust(cards: &str, analysis: &str) -> Plot {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("c.cir"),
            &format!("{cards}.{analysis}\n.end\n"),
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

#[test]
#[ignore = "requires NGSPICE_BIN; run with -- --ignored"]
fn every_function_value_and_derivative_matches_live_c() {
    let mut failures = Vec::new();
    for (case, bias) in [-0.45, 0.3, 0.8, 1.7].into_iter().enumerate() {
        let deck = cards(bias);
        for (analysis, part) in [("op", "value"), ("ac lin 1 1k 1k", "derivative")] {
            let c = run_c(&format!("{case}-{part}"), &deck, analysis);
            let rust = run_rust(&deck, analysis);
            for (index, expression) in EXPRESSIONS.iter().enumerate() {
                let name = format!("v(o{index})");
                let ours = rust.value(&name, 0).unwrap();
                let theirs = c.value(&name, 0).unwrap();
                for (a, b) in [(ours.re, theirs.re), (ours.im, theirs.im)] {
                    if (a - b).abs() > 1e-12 * b.abs() + 1e-15 {
                        failures.push(format!(
                            "{expression} at v(in) = {bias}: {part} Rust {a:e}, C {b:e}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
