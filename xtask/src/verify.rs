//! Rust-engine verification against committed C data. Never executes C or
//! writes a deck/rawfile. Extend the registry only with demonstrated coverage.

use std::{fs, path::Path};

use spice_analysis::{AnalysisContext, AnalysisRequest, RawFile, runner};
use spice_core::{AnalysisKind, parse_spice_number};
use spice_devices::Circuit;
use spice_netlist::Parser;

use crate::{compare, golden, tran, workspace_root};

/// How a fixture's Rust result is compared with its C golden.
enum Gate {
    /// Point-wise by variable name (DC/AC), optionally along a sweep axis.
    Points {
        axis: Option<&'static str>,
        tolerance: compare::Tolerance,
    },
    /// Event-aware comparison on a shared physical time grid (`tran.rs`).
    Transient(compare::TranTolerance),
}

struct Supported {
    name: &'static str,
    kind: AnalysisKind,
    gate: Gate,
}

const SUPPORTED: &[Supported] = &[
    Supported {
        name: "rc_divider",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
    },
    Supported {
        name: "rc_lowpass_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
    },
    Supported {
        name: "rc_transient",
        kind: AnalysisKind::Transient,
        gate: Gate::Transient(compare::TRAN),
    },
    Supported {
        name: "rlc_series",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
    },
];
const EXCLUDED: &[(&str, &str)] = &[
    ("bjt_ce", "nonlinear BJT backend unavailable"),
    ("diode_dc", "nonlinear diode backend unavailable"),
    ("mos_inverter", "nonlinear MOS backend unavailable"),
    (
        "subckt_divider",
        "subcircuit flattening/elaboration unavailable",
    ),
];

pub(crate) fn main(arguments: &[String]) -> Result<(), String> {
    let only = match arguments {
        [] => None,
        [flag, name] if flag == "--netlist" && !name.starts_with('-') => Some(name.as_str()),
        _ => return Err("usage: cargo xtask golden verify [--netlist <NAME>]".into()),
    };
    run(&workspace_root(), only)
}

fn run(root: &Path, only: Option<&str>) -> Result<(), String> {
    let paths = golden::netlist_paths_at(root, only)?;
    let mut verified = 0;
    let mut unsupported = 0;
    let mut failures = Vec::new();
    if only.is_none() {
        for fixture in SUPPORTED {
            if !paths
                .iter()
                .any(|path| path.file_stem().is_some_and(|stem| stem == fixture.name))
            {
                failures.push(format!("missing supported fixture '{}'", fixture.name));
            }
        }
    }
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid fixture name")?;
        if let Some(fixture) = SUPPORTED.iter().find(|fixture| fixture.name == name) {
            match fixture_result(root, &path, fixture) {
                Ok(()) => {
                    verified += 1;
                    println!("  verified   {name}");
                }
                Err(error) => {
                    println!("  FAIL       {name}: {error}");
                    failures.push(format!("{name}: {error}"));
                }
            }
        } else if let Some((_, reason)) = EXCLUDED.iter().find(|(fixture, _)| *fixture == name) {
            unsupported += 1;
            println!("  unsupported {name}: {reason}");
            if only.is_some() {
                failures.push(format!("requested unsupported fixture '{name}': {reason}"));
            }
        } else {
            failures.push(format!(
                "fixture '{name}' has no verification registry entry"
            ));
        }
    }
    println!(
        "\n{verified} verified fixture(s), {unsupported} unsupported fixture(s), {} failure(s); bounded coverage, not full corpus parity",
        failures.len()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("verification failed: {}", failures.join("\n")))
    }
}

fn fixture_result(root: &Path, path: &Path, fixture: &Supported) -> Result<(), String> {
    let netlist = Parser::new().parse_file(path).map_err(|e| e.to_string())?;
    if netlist.analyses.len() != 1 {
        return Err(format!(
            "expected exactly one analysis, found {}",
            netlist.analyses.len()
        ));
    }
    let request = AnalysisRequest::from(&netlist.analyses[0]);
    if request.kind != fixture.kind {
        return Err(format!(
            "registry expects {:?}, deck requests {:?}",
            fixture.kind, request.kind
        ));
    }
    let mut circuit = Circuit::from_netlist(&netlist).map_err(|e| e.to_string())?;
    let got = runner(request.kind)
        .and_then(|driver| driver.run(&mut circuit, &request, &AnalysisContext::default()))
        .map_err(|e| e.to_string())?;
    let target = root
        .join(golden::GOLDEN_DIR)
        .join(format!("{}.raw", fixture.name));
    let text =
        fs::read_to_string(&target).map_err(|e| format!("reading {}: {e}", target.display()))?;
    let want = RawFile::parse(&text).map_err(|e| format!("{}: {e}", target.display()))?;
    if want.plots.len() != 1 {
        return Err(format!("expected one C plot, found {}", want.plots.len()));
    }
    match fixture.gate {
        Gate::Points { axis, tolerance } => {
            compare::plots(&got, &want.plots[0].plot, tolerance, axis)
        }
        Gate::Transient(tolerance) => {
            let time = |index: usize, what: &str| {
                request
                    .argument(index)
                    .and_then(parse_spice_number)
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or_else(|| format!(".tran {what} is not a positive number"))
            };
            let (step, stop) = (time(0, "tstep")?, time(1, "tstop")?);
            let breakpoints = tran::breakpoints(&netlist, step, stop)?;
            tran::transient(
                &got,
                &want.plots[0].plot,
                tolerance,
                &tran::Grid { stop, step },
                &breakpoints,
            )
            .map(|summary| {
                println!(
                    "             {} instants + {} breakpoint limits, worst error {:.3} of bound",
                    summary.instants, summary.limits, summary.worst_ratio
                );
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temp(std::path::PathBuf);
    impl Temp {
        fn divider() -> Self {
            static ID: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "spice-verify-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join(golden::NETLIST_DIR)).unwrap();
            fs::create_dir_all(root.join(golden::GOLDEN_DIR)).unwrap();
            for path in [
                "conformance/netlists/rc_divider.cir",
                "conformance/golden/rc_divider.raw",
            ] {
                fs::copy(workspace_root().join(path), root.join(path)).unwrap();
            }
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn production_fixtures_pass_without_modifying_inputs() {
        let paths: Vec<_> = golden::netlist_paths_at(&workspace_root(), None)
            .unwrap()
            .into_iter()
            .flat_map(|p| {
                let raw = workspace_root()
                    .join(golden::GOLDEN_DIR)
                    .join(format!("{}.raw", p.file_stem().unwrap().to_str().unwrap()));
                [p, raw]
            })
            .collect();
        let before: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
        run(&workspace_root(), None).unwrap();
        for fixture in SUPPORTED {
            run(&workspace_root(), Some(fixture.name)).unwrap();
        }
        for (path, bytes) in paths.iter().zip(before) {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn requested_unsupported_unknown_and_bad_options_fail() {
        for (name, _) in EXCLUDED {
            assert!(
                run(&workspace_root(), Some(name))
                    .unwrap_err()
                    .contains("requested unsupported")
            );
        }
        assert!(run(&workspace_root(), Some("unknown")).is_err());
        for args in [
            vec!["--netlist"],
            vec!["--ngspice", "anything"],
            vec!["--verbose"],
            vec!["--netlist", "--verbose"],
            vec!["--netlist", "rc_divider", "extra"],
        ] {
            assert!(main(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }

    #[test]
    fn corrupted_missing_and_multi_plot_goldens_fail_through_verification() {
        let temp = Temp::divider();
        let raw = temp.0.join("conformance/golden/rc_divider.raw");
        let original = fs::read_to_string(&raw).unwrap();
        assert!(
            run(&temp.0, None)
                .unwrap_err()
                .contains("missing supported fixture")
        );
        run(&temp.0, Some("rc_divider")).unwrap();
        for text in [
            original.replace("2.500000000000000e+00", "2.600000000000000e+00"),
            original.replace("v(out)", "v(wrong)"),
            original.replace("2.500000000000000e+00", "NaN"),
            "not a rawfile".into(),
            format!("{original}{original}"),
        ] {
            fs::write(&raw, text).unwrap();
            assert!(run(&temp.0, Some("rc_divider")).is_err());
        }
        fs::remove_file(&raw).unwrap();
        assert!(
            run(&temp.0, Some("rc_divider"))
                .unwrap_err()
                .contains("reading")
        );
    }

    #[test]
    fn deck_errors_are_not_silently_skipped() {
        let temp = Temp::divider();
        let deck = temp.0.join("conformance/netlists/rc_divider.cir");
        let original = fs::read_to_string(&deck).unwrap();
        for text in [
            original.replace(".op", ""),
            original.replace(".op", ".op\n.op"),
            original.replace(".op", ".ac lin 3 100 1k"),
            original.replace("r2 out 0 1k", "r2 out nowhere 1k"),
            original.replace("r2 out 0 1k", "r2 out 0 {expr}"),
        ] {
            fs::write(&deck, text).unwrap();
            assert!(run(&temp.0, Some("rc_divider")).is_err());
        }
        fs::write(
            temp.0.join("conformance/netlists/new_fixture.cir"),
            original,
        )
        .unwrap();
        assert!(
            run(&temp.0, Some("new_fixture"))
                .unwrap_err()
                .contains("no verification registry")
        );
    }
}
