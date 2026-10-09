//! Golden-data capture and drift checks.
//!
//! Fixtures are the decks in `conformance/netlists/`; golden data is the ASCII
//! rawfile the C binary produces for each of them, stored in
//! `conformance/golden/`. See `docs/port/VERIFICATION.md` for what the fixtures
//! are allowed to contain and why.

use std::fs;
use std::path::{Path, PathBuf};

use ngspice_rs::analysis::RawFile;

use crate::ngspice::{self, Ngspice};
use crate::workspace_root;

/// Where the fixture decks live, relative to the workspace root.
pub(crate) const NETLIST_DIR: &str = "conformance/netlists";

/// Where the captured rawfiles live, relative to the workspace root.
pub(crate) const GOLDEN_DIR: &str = "conformance/golden";

/// Where instrumented decks and rawfiles are staged, relative to the workspace root.
pub(crate) const SCRATCH_DIR: &str = "target/xtask/golden";

const USAGE: &str = "\
usage: cargo xtask golden <capture|check|list|verify> [OPTIONS]
       cargo xtask golden verify [--netlist <NAME>] (Rust only; no C invocation)

OPTIONS:
    --ngspice <PATH>   the C ngspice binary to drive
                       (default: $NGSPICE_BIN, then build/src/ngspice, then PATH)
    --netlist <NAME>   only this fixture, e.g. --netlist rc_divider
    --verbose          echo each ngspice invocation";

/// Options shared by `capture` and `check`.
#[derive(Debug, Default)]
struct Options {
    ngspice: Option<PathBuf>,
    netlist: Option<String>,
    verbose: bool,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            match arguments[index].as_str() {
                "--ngspice" => {
                    index += 1;
                    let value = arguments
                        .get(index)
                        .ok_or_else(|| "usage: --ngspice needs a path".to_owned())?;
                    options.ngspice = Some(PathBuf::from(value));
                }
                "--netlist" => {
                    index += 1;
                    let value = arguments
                        .get(index)
                        .ok_or_else(|| "usage: --netlist needs a name".to_owned())?;
                    options.netlist = Some(value.clone());
                }
                "--verbose" => options.verbose = true,
                other => return Err(format!("usage: unknown option '{other}'")),
            }
            index += 1;
        }
        Ok(options)
    }
}

/// Entry point for `cargo xtask golden …`.
pub(crate) fn main(arguments: &[String]) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("capture") => capture(&Options::parse(&arguments[1..])?),
        Some("check") => check(&Options::parse(&arguments[1..])?),
        Some("list") => list(),
        Some("verify") => crate::verify::main(&arguments[1..]),
        _ => Err(format!("usage: unknown 'golden' subcommand\n\n{USAGE}")),
    }
}

fn scratch_directory() -> PathBuf {
    workspace_root().join(SCRATCH_DIR)
}

fn netlist_paths(only: Option<&str>) -> Result<Vec<PathBuf>, String> {
    netlist_paths_at(&workspace_root(), only)
}

pub(crate) fn netlist_paths_at(root: &Path, only: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let directory = root.join(NETLIST_DIR);
    let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
        .map_err(|error| format!("reading {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "cir"))
        .collect();
    paths.sort();

    if let Some(name) = only {
        let wanted = name.trim_end_matches(".cir");
        paths.retain(|path| {
            path.file_stem()
                .is_some_and(|stem| stem.eq_ignore_ascii_case(wanted))
        });
        if paths.is_empty() {
            return Err(format!(
                "no fixture named '{name}' in {}",
                directory.display()
            ));
        }
    }
    if paths.is_empty() {
        return Err(format!("no .cir fixtures in {}", directory.display()));
    }
    Ok(paths)
}

fn golden_path(netlist: &Path) -> Result<PathBuf, String> {
    let name = netlist
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("fixture {} has no file name", netlist.display()))?;
    Ok(workspace_root()
        .join(GOLDEN_DIR)
        .join(format!("{name}.raw")))
}

fn locate(options: &Options) -> Result<Ngspice, String> {
    let binary = ngspice::locate(options.ngspice.as_deref(), &workspace_root())?;
    println!("ngspice:  {} ({})", binary.path.display(), binary.version);
    Ok(binary)
}

fn describe(rawfile: &str) -> Result<String, String> {
    let parsed = RawFile::parse(rawfile).map_err(|error| error.to_string())?;
    let mut parts = Vec::new();
    for raw_plot in &parsed.plots {
        let plot = &raw_plot.plot;
        parts.push(format!(
            "{}: {} variable(s), {} point(s), {}",
            plot.plotname,
            plot.variable_count(),
            plot.point_count(),
            if plot.is_finite() {
                "all values finite"
            } else {
                "NON-FINITE VALUES"
            }
        ));
    }
    Ok(parts.join("; "))
}

fn capture(options: &Options) -> Result<(), String> {
    let binary = locate(options)?;
    let scratch = scratch_directory();
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("creating {}: {error}", scratch.display()))?;

    let netlists = netlist_paths(options.netlist.as_deref())?;
    let mut created = 0usize;
    let mut changed = 0usize;
    let mut unchanged = 0usize;

    for netlist in &netlists {
        if options.verbose {
            println!("capturing {}", netlist.display());
        }
        let captured = ngspice::capture(&binary, netlist, &scratch)?;
        let target = golden_path(netlist)?;
        let name = netlist
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("<unnamed>");

        match fs::read_to_string(&target) {
            Ok(existing) if ngspice::rawfiles_match(&existing, &captured.rawfile) => {
                unchanged += 1;
                println!("  unchanged  {name}");
            }
            Ok(existing) => {
                changed += 1;
                let report = ngspice::first_difference(&existing, &captured.rawfile)
                    .unwrap_or_else(|| "differences are only in ignored headers".to_owned());
                println!("  changed    {name}  ({report})");
                write_golden(&target, &captured.rawfile)?;
            }
            Err(_) => {
                created += 1;
                println!("  new        {name}");
                write_golden(&target, &captured.rawfile)?;
            }
        }
        if options.verbose {
            println!("             deck: {}", captured.deck_path.display());
            println!(
                "             ngspice output: {} line(s)",
                captured.log.lines().count()
            );
        }
        println!("             {}", describe(&captured.rawfile)?);
    }

    println!(
        "\n{} fixture(s): {created} new, {changed} changed, {unchanged} unchanged",
        netlists.len()
    );
    Ok(())
}

fn write_golden(target: &Path, rawfile: &str) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    }
    fs::write(target, rawfile).map_err(|error| format!("writing {}: {error}", target.display()))
}

fn check(options: &Options) -> Result<(), String> {
    let binary = locate(options)?;
    let scratch = scratch_directory();
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("creating {}: {error}", scratch.display()))?;

    let netlists = netlist_paths(options.netlist.as_deref())?;
    let mut drifted = Vec::new();

    for netlist in &netlists {
        let captured = ngspice::capture(&binary, netlist, &scratch)?;
        let target = golden_path(netlist)?;
        let name = netlist
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("<unnamed>");

        match fs::read_to_string(&target) {
            Ok(existing) if ngspice::rawfiles_match(&existing, &captured.rawfile) => {
                println!("  ok         {name}");
            }
            Ok(existing) => {
                let report = ngspice::first_difference(&existing, &captured.rawfile)
                    .unwrap_or_else(|| "differences are only in ignored headers".to_owned());
                println!("  DRIFT      {name}  ({report})");
                drifted.push(name.to_owned());
            }
            Err(error) => {
                println!("  MISSING    {name}  ({error})");
                drifted.push(name.to_owned());
            }
        }
    }

    if drifted.is_empty() {
        println!(
            "\n{} fixture(s) reproduce the committed goldens",
            netlists.len()
        );
        Ok(())
    } else {
        Err(format!(
            "drift in {} fixture(s): {}\n\
             If ngspice really did change its output, re-capture with \
             `cargo xtask golden capture` and review the resulting diff.",
            drifted.len(),
            drifted.join(", ")
        ))
    }
}

fn list() -> Result<(), String> {
    let netlists = netlist_paths(None)?;
    println!("{:<22} {:<12} details", "fixture", "golden");
    for netlist in &netlists {
        let name = netlist
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("<unnamed>");
        let target = golden_path(netlist)?;
        match fs::read_to_string(&target) {
            Ok(rawfile) => println!("{name:<22} {:<12} {}", "present", describe(&rawfile)?),
            Err(_) => println!("{name:<22} {:<12} -", "missing"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Options, golden_path, netlist_paths};
    use std::path::PathBuf;

    #[test]
    fn options_are_parsed() {
        let options = Options::parse(&[
            "--ngspice".to_owned(),
            "/tmp/ngspice".to_owned(),
            "--netlist".to_owned(),
            "rc_divider".to_owned(),
            "--verbose".to_owned(),
        ])
        .expect("parses");
        assert_eq!(options.ngspice, Some(PathBuf::from("/tmp/ngspice")));
        assert_eq!(options.netlist.as_deref(), Some("rc_divider"));
        assert!(options.verbose);
    }

    #[test]
    fn missing_option_values_are_rejected() {
        assert!(Options::parse(&["--ngspice".to_owned()]).is_err());
        assert!(Options::parse(&["--netlist".to_owned()]).is_err());
        assert!(Options::parse(&["--nope".to_owned()]).is_err());
    }

    #[test]
    fn fixtures_are_discovered_and_filtered() {
        let paths = netlist_paths(None).expect("fixtures exist");
        assert!(!paths.is_empty());
        let first = paths[0]
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("named")
            .to_owned();
        let filtered = netlist_paths(Some(&first)).expect("filter works");
        assert_eq!(filtered.len(), 1);
        // The `.cir` suffix is optional.
        let filtered = netlist_paths(Some(&format!("{first}.cir"))).expect("filter works");
        assert_eq!(filtered.len(), 1);
        assert!(netlist_paths(Some("definitely-not-a-fixture")).is_err());
    }

    #[test]
    fn goldens_are_named_after_their_fixture() {
        let path =
            golden_path(&PathBuf::from("conformance/netlists/rc_divider.cir")).expect("named");
        assert!(path.ends_with("conformance/golden/rc_divider.raw"));
    }
}
