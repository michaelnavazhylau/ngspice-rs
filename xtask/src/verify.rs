//! Rust-engine verification against committed C data. Never executes C or
//! writes a deck/rawfile. Extend the registry only with demonstrated coverage.

use std::{fs, path::Path};

use spice_analysis::{AnalysisRequest, RawFile, RunConfig, runner};
use spice_core::{AnalysisKind, parse_spice_number};
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

/// An additional Rust-only run of the same deck against the same C golden.
///
/// `extra` tokens are appended to the request after the deck's own settings,
/// so a variant can select a backend that C itself cannot parse (for example
/// the explicit diffsol BDF `backend=diffsol method=bdf`, which ngspice
/// rejects). The deck text, the golden and the tolerance are unchanged, so the
/// variant must reproduce the very same C waveform; it never loosens a bound.
struct Variant {
    label: &'static str,
    extra: &'static [&'static str],
    /// Overrides the fixture's transient tolerance for this run.
    tolerance: compare::TranTolerance,
}

/// The explicit adaptive BDF backend (not ngspice trapezoidal/Gear-2) under the
/// default C-parity bound: for decks whose C reference has no restart artefact
/// exceeding it.
const DIFFSOL_BDF: Variant = Variant {
    label: "diffsol-bdf",
    extra: &["backend=diffsol", "method=bdf"],
    tolerance: compare::TRAN,
};

/// The same backend under `compare::TRAN_RESTART` (peak-scaled), for decks with
/// source corners where C's backward-Euler restart error exceeds `TRAN` at small
/// values; see that constant for the justification.
const DIFFSOL_BDF_RESTART: Variant = Variant {
    label: "diffsol-bdf, peak-scaled",
    extra: &["backend=diffsol", "method=bdf"],
    tolerance: compare::TRAN_RESTART,
};

struct Supported {
    name: &'static str,
    kind: AnalysisKind,
    gate: Gate,
    /// Extra runs against the same golden, in addition to the deck's own run.
    variants: &'static [Variant],
}

/// Transient registry entry: `compare::TRAN`, no variants unless listed.
const fn tran(name: &'static str, variants: &'static [Variant]) -> Supported {
    Supported {
        name,
        kind: AnalysisKind::Transient,
        gate: Gate::Transient(compare::TRAN),
        variants,
    }
}

const SUPPORTED: &[Supported] = &[
    Supported {
        name: "bjt_ce",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "mos_inverter",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "diode_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::LEGACY_DIODE_DC,
        },
        variants: &[],
    },
    Supported {
        name: "m4_diode_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m4_bjt_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m4_mos1_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("m4_diode_tran", &[]),
    tran("m4_bjt_tran", &[]),
    tran("m4_mos1_tran", &[]),
    Supported {
        name: "rc_divider",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    Supported {
        name: "rc_lowpass_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    },
    tran("rc_transient", &[]),
    Supported {
        name: "rlc_series",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    // Subcircuit elaboration (#18): one `X` instance is flattened through the
    // production `.op` path. The deck is a purely resistive divider, so it keeps
    // the same 1e-12 relative bound as the other linear operating points.
    Supported {
        name: "subckt_divider",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    // `.func` definitions and single-quoted values (#107): top-level and
    // body-local functions, quoted device/instance values, flattened through
    // the production `.op` path. Purely resistive, so the linear DC bound.
    Supported {
        name: "func_quotes",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    // M3 exit-gate fixtures (#48). All use `compare::TRAN`; the physical bound is
    // the simulator's own default accuracy (see `compare.rs`), no fixture-specific
    // tolerance exists. The companion driver (trap, or Gear-2 via
    // `.options method=gear`) runs each deck as written. `DIFFSOL_BDF` variants
    // add the Rust-only BDF tokens: used only where every source corner lies on
    // the `.tran` output grid (the BDF backend emits the requested grid, and the
    // comparator demands a sample at each breakpoint) and only for decks with no
    // `.options method=` (the deck's trap/Gear choice cannot be combined with
    // diffsol and is rejected explicitly by `RunConfig::request`).
    tran("rl_pulse_tran", &[DIFFSOL_BDF_RESTART]),
    tran("rc_gear_tran", &[]),
    tran("rc_pwl_tran", &[DIFFSOL_BDF_RESTART]),
    // The RLC pulse corners restart C's trapezoidal rule with a backward-Euler
    // step, whose error (about 3e-6 V / 2e-6 A near the ringing zero crossings,
    // measured against an independent RK4 solution) exceeds `compare::TRAN`'s
    // 1e-6 V / 1e-12 A near-zero floor; BDF is the more accurate side, so it
    // runs under `compare::TRAN_RESTART`.
    tran("rlc_series_tran", &[DIFFSOL_BDF_RESTART]),
    tran("rlc_series_gear_tran", &[]),
    tran("floating_cap_tran", &[DIFFSOL_BDF]),
    tran("coupled_cap_tran", &[DIFFSOL_BDF_RESTART]),
    // Initialized-state fixtures (#27, #48): `uic` / instance `ic=` / `.ic`. No
    // BDF variants: the diffsol backend deliberately rejects `.ic`, `uic` and
    // instance `ic=` (asserted by a test below). With `uic` C writes no `t = 0`
    // row and adds a breakpoint at the `.tran` step; the Rust driver reproduces
    // both and the comparator (which only demands samples at source breakpoints)
    // needs no special case.
    tran("rc_ic_uic_tran", &[]),
    tran("rlc_ic_uic_tran", &[]),
    tran("rc_ic_node_tran", &[]),
    tran("floating_cap_ic_tran", &[]),
    // M6 source waveforms (#94, #95), companion trap driver under `compare::TRAN`.
    // C sets no breakpoints for SIN/EXP/SFFM/AM, so those decks are compared on
    // the grid only (`tran::breakpoints`); PULSE count and repeated PWL corners
    // are C breakpoints.
    tran("rc_sin_tran", &[]),
    tran("rc_exp_tran", &[]),
    tran("rc_sffm_am_tran", &[]),
    tran("rc_pwl_repeat_tran", &[]),
    tran("rc_pulse_count_tran", &[]),
    Supported {
        name: "rlc_series_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    },
];
/// Fixtures whose deck the Rust engine deliberately does not run yet. Empty:
/// every committed deck, including `subckt_divider`, is verified through its
/// own production path. A requested excluded fixture still fails the run, so a
/// future entry cannot be reported as a success by accident.
const EXCLUDED: &[(&str, &str)] = &[];

pub(crate) fn main(arguments: &[String]) -> Result<(), String> {
    let only = match arguments {
        [] => None,
        [flag, name] if flag == "--netlist" && !name.starts_with('-') => Some(name.as_str()),
        _ => return Err("usage: cargo xtask golden verify [--netlist <NAME>]".into()),
    };
    run(&workspace_root(), only)
}

fn run(root: &Path, only: Option<&str>) -> Result<(), String> {
    run_with_registry(root, only, SUPPORTED, EXCLUDED)
}

/// The verification loop, parameterized by the fixture registry.
///
/// `EXCLUDED` is empty while every committed deck verifies, so the
/// `requested unsupported fixture` path has no committed fixture to exercise it;
/// this signature lets a unit test drive that branch with a synthetic entry
/// instead of leaving it untested until an excluded fixture reappears.
fn run_with_registry(
    root: &Path,
    only: Option<&str>,
    supported: &[Supported],
    excluded: &[(&str, &str)],
) -> Result<(), String> {
    let paths = golden::netlist_paths_at(root, only)?;
    let mut verified = 0;
    let mut unsupported = 0;
    let mut failures = Vec::new();
    if only.is_none() {
        for fixture in supported {
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
        if let Some(fixture) = supported.iter().find(|fixture| fixture.name == name) {
            match fixture_result(root, &path, fixture) {
                Ok(details) => {
                    verified += 1;
                    println!("  verified   {name}");
                    for detail in details {
                        println!("             {detail}");
                    }
                }
                Err(error) => {
                    println!("  FAIL       {name}: {error}");
                    failures.push(format!("{name}: {error}"));
                }
            }
        } else if let Some((_, reason)) = excluded.iter().find(|(fixture, _)| *fixture == name) {
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

/// Detail lines for a verified fixture: one per run (the deck's own, then each
/// variant).
fn fixture_result(root: &Path, path: &Path, fixture: &Supported) -> Result<Vec<String>, String> {
    let mut details = vec![run_variant(root, path, fixture, None)?];
    for variant in fixture.variants {
        let detail = run_variant(root, path, fixture, Some(variant))
            .map_err(|error| format!("variant {}: {error}", variant.label))?;
        details.push(format!("[{}] {detail}", variant.label));
    }
    Ok(details)
}

/// One Rust run of the deck (optionally with a [`Variant`]'s extra request
/// tokens) compared with the committed C golden. Deck `.option` cards are
/// applied through `RunConfig`, exactly as for an ordinary run.
fn run_variant(
    root: &Path,
    path: &Path,
    fixture: &Supported,
    variant: Option<&Variant>,
) -> Result<String, String> {
    let netlist = Parser::new().parse_file(path).map_err(|e| e.to_string())?;
    if netlist.analyses.len() != 1 {
        return Err(format!(
            "expected exactly one analysis, found {}",
            netlist.analyses.len()
        ));
    }
    if !netlist.analyses[0].expressions.is_empty() {
        return Err("braced analysis arguments are not supported in verification fixtures".into());
    }
    let config = RunConfig::from_netlist(&netlist).map_err(|e| e.to_string())?;
    let mut request: AnalysisRequest = AnalysisRequest::from(&netlist.analyses[0]);
    if let Some(variant) = variant {
        request
            .arguments
            .extend(variant.extra.iter().map(|token| (*token).to_owned()));
    }
    let request = config.request(request).map_err(|e| e.to_string())?;
    if request.kind != fixture.kind {
        return Err(format!(
            "registry expects {:?}, deck requests {:?}",
            fixture.kind, request.kind
        ));
    }
    let mut circuit = config.circuit(&netlist).map_err(|e| e.to_string())?;
    let mut got = runner(request.kind)
        .and_then(|driver| driver.run(&mut circuit, &request, &config.context()))
        .map_err(|e| e.to_string())?;
    // C's default save set omits simulator-created internal nodes (e.g. a
    // diode's series-resistance anode). Project only those known internal rows;
    // every externally visible variable still goes through exact set checks.
    let internal: Vec<_> = circuit
        .nodes()
        .nodes()
        .iter()
        .filter(|node| node.kind == spice_core::NodeKind::Internal)
        .map(|node| format!("v({})", node.name))
        .collect();
    for column in (0..got.variables.len()).rev() {
        if internal.contains(&got.variables[column].name) {
            got.variables.remove(column);
            for row in &mut got.points {
                row.remove(column);
            }
        }
    }
    if request.kind == AnalysisKind::DcSweep
        && got.variables.first().is_some_and(|v| v.name == "sweep")
    {
        // Rust's public DC scale name predates the nonlinear gate; C wraps its
        // independent-source scale in the voltage/current naming convention.
        got.variables[0].name = if got.variables[0].unit == "voltage" {
            "v(v-sweep)"
        } else {
            "i(i-sweep)"
        }
        .into();
    }
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
                .map(|()| format!("{} point(s)", got.point_count()))
        }
        Gate::Transient(tolerance) => {
            let tolerance = variant.map_or(tolerance, |variant| variant.tolerance);
            let time = |index: usize, what: &str| {
                request
                    .argument(index)
                    .and_then(parse_spice_number)
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or_else(|| format!(".tran {what} is not a positive number"))
            };
            let (step, stop) = (time(0, "tstep")?, time(1, "tstop")?);
            let breakpoints = tran::breakpoints(&netlist, step, stop)?;
            // With `uic` C writes no t = 0 row: its first row is the first
            // accepted step. The comparison then starts at that time, which the
            // Rust plot must reproduce (`tran::Series::new`); without `uic`
            // both plots start at 0 as always.
            let start = if request.uic {
                want.plots[0]
                    .plot
                    .value("time", 0)
                    .map(|t| t.re)
                    .filter(|t| t.is_finite() && *t > 0.0)
                    .ok_or("uic golden has no positive first time")?
            } else {
                0.0
            };
            tran::transient(
                &got,
                &want.plots[0].plot,
                tolerance,
                &tran::Grid { start, stop, step },
                &breakpoints,
            )
            .map(|summary| {
                format!(
                    "{} instants + {} breakpoint limits, worst error {:.3} of bound",
                    summary.instants, summary.limits, summary.worst_ratio
                )
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
            Self::with(&["rc_divider"])
        }
        fn with(names: &[&str]) -> Self {
            static ID: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "spice-verify-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join(golden::NETLIST_DIR)).unwrap();
            fs::create_dir_all(root.join(golden::GOLDEN_DIR)).unwrap();
            for name in names {
                for path in [
                    format!("conformance/netlists/{name}.cir"),
                    format!("conformance/golden/{name}.raw"),
                ] {
                    fs::copy(workspace_root().join(&path), root.join(&path)).unwrap();
                }
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
    fn rust_only_variants_select_the_explicit_diffsol_bdf_backend() {
        let mut with_variants = 0;
        for fixture in SUPPORTED {
            for variant in fixture.variants {
                with_variants += 1;
                assert_eq!(fixture.kind, AnalysisKind::Transient, "{}", fixture.name);
                assert_eq!(variant.extra, ["backend=diffsol", "method=bdf"]);
                // Trap/Gear selections in the deck cannot be combined with BDF
                // (`RunConfig::request` rejects them); the fixture must not set one.
                let deck = fs::read_to_string(
                    workspace_root().join(format!("conformance/netlists/{}.cir", fixture.name)),
                )
                .unwrap();
                assert!(!deck.to_ascii_lowercase().contains("method"), "{deck}");
            }
        }
        assert_eq!(with_variants, 5);
        // The same deck with a deck-level Gear selection and the BDF tokens is an
        // explicit error, never a silent downgrade.
        let temp = Temp::with(&["rc_gear_tran"]);
        let fixture = Supported {
            variants: &[DIFFSOL_BDF],
            ..tran("rc_gear_tran", &[])
        };
        let path = temp.0.join("conformance/netlists/rc_gear_tran.cir");
        let error = fixture_result(&temp.0, &path, &fixture).unwrap_err();
        assert!(error.contains("variant diffsol-bdf"), "{error}");
    }

    #[test]
    fn bdf_variants_of_initialized_state_decks_are_rejected_explicitly() {
        // The diffsol backend deliberately has no `.ic`/`uic`/`ic=` support, so
        // the initialized-state fixtures register no BDF variant, and asking for
        // one is an explicit error rather than a silent downgrade.
        for name in [
            "rc_ic_uic_tran",
            "rlc_ic_uic_tran",
            "rc_ic_node_tran",
            "floating_cap_ic_tran",
        ] {
            let entry = SUPPORTED.iter().find(|f| f.name == name).unwrap();
            assert!(entry.variants.is_empty(), "{name}");
            let path = workspace_root().join(format!("conformance/netlists/{name}.cir"));
            let fixture = Supported {
                variants: &[DIFFSOL_BDF],
                ..tran(name, &[])
            };
            let error = fixture_result(&workspace_root(), &path, &fixture).unwrap_err();
            assert!(error.contains("variant diffsol-bdf"), "{name}: {error}");
            assert!(
                error.contains("unsupported") || error.contains("uic") || error.contains(".ic"),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn corrupted_transient_goldens_fail_both_the_companion_and_bdf_runs() {
        let temp = Temp::with(&["floating_cap_tran"]);
        let raw = temp.0.join("conformance/golden/floating_cap_tran.raw");
        let original = fs::read_to_string(&raw).unwrap();
        run(&temp.0, Some("floating_cap_tran")).unwrap();
        // Perturb v(b) of point 600 (t ~ 6 ms, a smooth interior sample) by 1 mV:
        // far above the 1e-3 relative + 1 uV bound at v(b) ~ 1e-3 V.
        let mut lines: Vec<String> = original.lines().map(String::from).collect();
        let start = lines
            .iter()
            .position(|line| line.starts_with(" 600\t"))
            .unwrap();
        let name = lines
            .iter()
            .position(|l| l.ends_with("v(b)\tvoltage"))
            .unwrap();
        let column: usize = lines[name]
            .trim_start()
            .split('\t')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let line = &mut lines[start + column];
        let value: f64 = line.trim().parse().unwrap();
        *line = format!("\t{:.15e}", value + 1e-3);
        fs::write(&raw, lines.join("\n") + "\n").unwrap();
        let error = run(&temp.0, Some("floating_cap_tran")).unwrap_err();
        assert!(error.contains("transient mismatch"), "{error}");
        // The BDF variant alone also detects it.
        let path = temp.0.join("conformance/netlists/floating_cap_tran.cir");
        let fixture = tran("floating_cap_tran", &[]);
        assert!(run_variant(&temp.0, &path, &fixture, Some(&DIFFSOL_BDF)).is_err());
    }

    #[test]
    fn requested_unsupported_unknown_and_bad_options_fail() {
        // `EXCLUDED` is empty now that every committed deck is verified. Drive
        // the excluded branch with a synthetic entry so the
        // `requested unsupported fixture` report stays tested, rather than
        // waiting for a future excluded fixture to exercise it.
        let error = run_with_registry(
            &workspace_root(),
            Some("rc_divider"),
            &[],
            &[("rc_divider", "synthetic exclusion")],
        )
        .unwrap_err();
        assert!(
            error.contains("requested unsupported fixture 'rc_divider': synthetic exclusion"),
            "{error}"
        );
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
