//! Locating and driving the C `ngspice` binary.
//!
//! The port never links against the C library; comparison data is produced by
//! running the reference binary out of process. This module owns the two
//! details that make that work:
//!
//! 1. **ASCII output.** ngspice writes binary rawfiles unless the command
//!    `set filetype=ascii` has been issued, and the `-r` option writes before
//!    any control script runs. So the fixture is instrumented with a
//!    `.control` block that sets the file type explicitly and then issues
//!    `write`, rather than relying on `-r`.
//! 2. **A scratch directory per fixture.** The instrumented deck is written
//!    into `target/xtask/golden/<name>/` together with the rawfile, so that the
//!    `write` argument can be a bare file name and never needs quoting.
//! 3. **Multi-analysis decks.** `write` alone writes only the *current* plot.
//!    A deck with several analysis cards is instrumented to write every plot by
//!    its C name (`write f.raw ac1.all dc1.all op1.all tran1.all`), in batch
//!    order, with the names and order computed by
//!    [`spice_analysis::batch::schedule`]; a wrong name or order makes ngspice
//!    fail or the plot-count check below reject the capture. The block ends with
//!    `quit`: without it, batch mode re-runs every analysis after `.endc` for a
//!    deck with `.op` (`ft_savedotargs()` registers an op-only save list, so the
//!    other analyses then fail with "no data saved").

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The C binary, and what it says it is.
#[derive(Debug, Clone)]
pub(crate) struct Ngspice {
    /// Path to the binary. May be a bare name to be resolved through `PATH`.
    pub(crate) path: PathBuf,
    /// The version string scraped from `--version`, e.g. `ngspice-47+`.
    pub(crate) version: String,
}

/// Finds the C binary.
///
/// `workspace` is the port's workspace root, where a `build/src/ngspice` from
/// building the C tree in this worktree would live.
///
/// # Errors
///
/// A message listing everywhere it looked.
pub(crate) fn locate(explicit: Option<&Path>, workspace: &Path) -> Result<Ngspice, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = explicit {
        candidates.push(path.to_path_buf());
    }
    if let Some(from_env) = std::env::var_os("NGSPICE_BIN") {
        candidates.push(PathBuf::from(from_env));
    }
    candidates.push(workspace.join("build/src/ngspice"));
    candidates.push(PathBuf::from("build/src/ngspice"));
    candidates.push(PathBuf::from("ngspice"));

    let mut tried = Vec::new();
    for candidate in candidates {
        // A bare name is resolved through `PATH`; anything else is taken
        // relative to the workspace root, not to the caller's directory, and is
        // made absolute so that it still resolves when ngspice runs in a scratch
        // directory.
        let is_bare_name = candidate.components().count() == 1;
        let resolved = if is_bare_name || candidate.is_absolute() {
            candidate
        } else {
            workspace.join(candidate)
        };
        let resolved = fs::canonicalize(&resolved).unwrap_or(resolved);
        if !is_bare_name && !resolved.is_file() {
            tried.push(format!("{} (not a file)", resolved.display()));
            continue;
        }
        match version_of(&resolved) {
            Ok(version) => {
                return Ok(Ngspice {
                    path: resolved,
                    version,
                });
            }
            Err(error) => tried.push(format!("{} ({error})", resolved.display())),
        }
    }

    Err(format!(
        "could not find a usable ngspice binary; tried:\n  {}\n\
         Pass --ngspice <PATH> or set NGSPICE_BIN.",
        tried.join("\n  ")
    ))
}

fn version_of(binary: &Path) -> Result<String, String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    text.split_whitespace()
        .find(|word| word.starts_with("ngspice-"))
        .map(|word| word.trim_end_matches(':').to_owned())
        .ok_or_else(|| "no 'ngspice-<version>' token in --version output".to_owned())
}

/// The result of capturing one fixture.
#[derive(Debug, Clone)]
pub(crate) struct Captured {
    /// The rawfile text, exactly as ngspice wrote it.
    pub(crate) rawfile: String,
    /// Where the instrumented deck was written, for debugging.
    pub(crate) deck_path: PathBuf,
    /// ngspice's stdout and stderr combined, for diagnostics.
    pub(crate) log: String,
}

/// Builds the instrumented deck: the fixture plus a `.control` block that writes
/// an ASCII rawfile called `plot_file`.
///
/// # Errors
///
/// A message when the fixture already contains a `.control` section, because
/// golden fixtures must be pure decks.
pub(crate) fn instrument(netlist: &str, plot_file: &str) -> Result<String, String> {
    instrument_with(
        netlist,
        &format!(".control\nset filetype=ascii\nrun\nwrite {plot_file}\n.endc\n"),
    )
}

/// Builds the instrumented deck for a multi-analysis fixture: every plot is
/// written by name, in `plots` order, into one ASCII rawfile `plot_file`.
///
/// # Errors
///
/// As [`instrument`].
pub(crate) fn instrument_plots(
    netlist: &str,
    plot_file: &str,
    plots: &[String],
) -> Result<String, String> {
    let vectors: Vec<String> = plots.iter().map(|name| format!("{name}.all")).collect();
    instrument_with(
        netlist,
        &format!(
            ".control\nset filetype=ascii\nrun\nwrite {plot_file} {}\nquit\n.endc\n",
            vectors.join(" ")
        ),
    )
}

fn instrument_with(netlist: &str, control: &str) -> Result<String, String> {
    if netlist
        .lines()
        .any(|line| line.trim().to_ascii_lowercase().starts_with(".control"))
    {
        return Err(
            "fixture already has a .control section; golden fixtures must be pure decks".to_owned(),
        );
    }

    let mut out = String::with_capacity(netlist.len() + control.len());
    let mut inserted = false;
    for line in netlist.lines() {
        if !inserted && line.trim().eq_ignore_ascii_case(".end") {
            out.push_str(control);
            inserted = true;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !inserted {
        out.push_str(control);
    }
    Ok(out)
}

/// Runs the C binary on one fixture and returns the ASCII rawfile.
///
/// # Errors
///
/// Any failure to prepare the scratch directory, run the binary, or find a
/// non-empty rawfile afterwards. The message includes ngspice's output.
pub(crate) fn capture(
    ngspice: &Ngspice,
    netlist: &Path,
    scratch: &Path,
) -> Result<Captured, String> {
    let name = netlist
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("fixture {} has no file name", netlist.display()))?;
    let directory = scratch.join(name);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("creating {}: {error}", directory.display()))?;

    let plot_file = format!("{name}.raw");
    let raw_path = directory.join(&plot_file);
    let _ = fs::remove_file(&raw_path);

    let text = fs::read_to_string(netlist)
        .map_err(|error| format!("reading {}: {error}", netlist.display()))?;
    let plots = batch_plot_names(netlist)?;
    let deck = if plots.len() > 1 {
        instrument_plots(&text, &plot_file, &plots)?
    } else {
        instrument(&text, &plot_file)?
    };
    let deck_path = directory.join(format!("{name}.cir"));
    fs::write(&deck_path, &deck)
        .map_err(|error| format!("writing {}: {error}", deck_path.display()))?;

    let output = Command::new(&ngspice.path)
        .arg("-b")
        .arg(&deck_path)
        .current_dir(&directory)
        .output()
        .map_err(|error| format!("running {}: {error}", ngspice.path.display()))?;

    let log = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "{} failed on {name} with {}\n--- ngspice output ---\n{}",
            ngspice.path.display(),
            output.status,
            tail(&log, 30)
        ));
    }

    let rawfile = fs::read_to_string(&raw_path).map_err(|error| {
        format!(
            "{name}: no rawfile at {} after a successful run ({error})\n\
             --- ngspice output ---\n{}",
            raw_path.display(),
            tail(&log, 30)
        )
    })?;
    if rawfile.trim().is_empty() {
        return Err(format!("{name}: ngspice wrote an empty rawfile"));
    }
    if !rawfile.contains("Values:") {
        return Err(format!(
            "{name}: the rawfile is not in ASCII form; the instrumented deck should have \
             issued 'set filetype=ascii'"
        ));
    }

    if plots.len() > 1 {
        let parsed = spice_analysis::RawFile::parse(&rawfile)
            .map_err(|error| format!("{name}: the captured rawfile does not parse: {error}"))?;
        if parsed.len() != plots.len() {
            return Err(format!(
                "{name}: expected {} plots ({}), ngspice wrote {}",
                plots.len(),
                plots.join(" "),
                parsed.len()
            ));
        }
    }

    Ok(Captured {
        rawfile,
        deck_path,
        log,
    })
}

/// The C plot names of the fixture's analyses in batch order (one entry per
/// analysis card).
fn batch_plot_names(netlist: &Path) -> Result<Vec<String>, String> {
    let parsed = spice_netlist::Parser::new()
        .parse_file(netlist)
        .map_err(|error| format!("parsing {}: {error}", netlist.display()))?;
    Ok(spice_analysis::batch::schedule(&parsed.analyses)
        .into_iter()
        .map(|entry| entry.plot_name)
        .collect())
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let skip = all.len().saturating_sub(lines);
    all[skip..].join("\n")
}

/// Compares two rawfiles, ignoring the `Date:` header.
///
/// The date is the only header ngspice fills in from the clock, so it is the
/// only one that cannot be reproduced. Everything else — including `Command:`,
/// which carries the ngspice version — is compared.
#[must_use]
pub(crate) fn rawfiles_match(left: &str, right: &str) -> bool {
    comparable(left) == comparable(right)
}

fn comparable(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.starts_with("Date:"))
        .collect()
}

/// The first differing line, for a drift report.
#[must_use]
pub(crate) fn first_difference(left: &str, right: &str) -> Option<String> {
    let left = comparable(left);
    let right = comparable(right);
    for index in 0..left.len().max(right.len()) {
        match (left.get(index), right.get(index)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                return Some(format!("line {}: golden {a:?} vs fresh {b:?}", index + 1));
            }
            (Some(a), None) => return Some(format!("line {}: golden {a:?} is missing", index + 1)),
            (None, Some(b)) => return Some(format!("line {}: extra line {b:?}", index + 1)),
            (None, None) => break,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{first_difference, instrument, instrument_plots, rawfiles_match};

    const DECK: &str = "RC divider\nv1 in 0 dc 5\n.op\n.end\n";

    #[test]
    fn instrument_inserts_before_the_end_card() {
        let deck = instrument(DECK, "plot.raw").expect("instrumented");
        let lines: Vec<&str> = deck.lines().collect();
        assert_eq!(lines[0], "RC divider");
        assert_eq!(lines[1], "v1 in 0 dc 5");
        assert_eq!(lines[2], ".op");
        assert_eq!(lines[3], ".control");
        assert_eq!(lines[4], "set filetype=ascii");
        assert_eq!(lines[5], "run");
        assert_eq!(lines[6], "write plot.raw");
        assert_eq!(lines[7], ".endc");
        assert_eq!(lines[8], ".end");
    }

    #[test]
    fn multi_analysis_decks_write_every_plot_by_name_and_quit() {
        let deck = instrument_plots(
            "t\n.tran 1u 1m\n.op\n.end\n",
            "plot.raw",
            &["op1".to_owned(), "tran1".to_owned()],
        )
        .expect("instrumented");
        let lines: Vec<&str> = deck.lines().collect();
        assert_eq!(
            lines[3..],
            [
                ".control",
                "set filetype=ascii",
                "run",
                "write plot.raw op1.all tran1.all",
                "quit",
                ".endc",
                ".end",
            ]
        );
    }

    #[test]
    fn instrument_appends_when_there_is_no_end_card() {
        let deck = instrument("title\n.op\n", "plot.raw").expect("instrumented");
        assert!(deck.ends_with(".endc\n"));
        assert_eq!(deck.lines().count(), 7);
    }

    #[test]
    fn instrument_refuses_a_deck_that_already_has_control() {
        let error = instrument("t\n.control\nrun\n.endc\n.end\n", "plot.raw").unwrap_err();
        assert!(error.contains(".control"), "{error}");
    }

    #[test]
    fn comparison_ignores_only_the_date() {
        let a = "Title: t\nDate: Mon\nCommand: ngspice-47+\nValues:\n 0\t1.0\n";
        let b = "Title: t\nDate: Tue\nCommand: ngspice-47+\nValues:\n 0\t1.0\n";
        assert!(rawfiles_match(a, b));

        let c = "Title: t\nDate: Mon\nCommand: ngspice-48\nValues:\n 0\t1.0\n";
        assert!(!rawfiles_match(a, c));
        assert!(first_difference(a, c).is_some_and(|text| text.contains("ngspice-48")));
    }

    #[test]
    fn difference_reports_missing_and_extra_lines() {
        let a = "Title: t\nValues:\n 0\t1.0\n";
        let b = "Title: t\nValues:\n 0\t1.0\n 1\t2.0\n";
        assert!(first_difference(a, b).is_some_and(|text| text.contains("extra line")));
        assert!(first_difference(b, a).is_some_and(|text| text.contains("missing")));
        assert_eq!(first_difference(a, a), None);
    }
}
