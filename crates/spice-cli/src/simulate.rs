//! `spice-rs simulate` — run one analysis of a deck and write an ASCII rawfile.
//!
//! C: the batch path of `src/ngspice.c` (`CKTdoJob()` through `ft_dotsim`/
//! `ft_dorun`) plus the writer in `src/frontend/rawfile.c` (`raw_write`, ASCII
//! only). The command is deliberately narrow:
//!
//! * the deck is loaded by the ordinary [`spice_netlist::Parser`] and elaborated
//!   by the ordinary production runner, so no device or solver logic is
//!   duplicated here;
//! * **exactly one** analysis card is required (see [`run`]);
//! * the rawfile is written through a temporary file in the destination's
//!   directory and renamed into place, so a failed run never truncates,
//!   replaces or removes an existing destination, and never leaves a partial
//!   rawfile behind;
//! * an unadorned `.tran` runs the companion trap/Gear driver the engine
//!   defaults to; this command never injects `backend=diffsol`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use spice_analysis::{Plot, RawFile, RawPlot, RunConfig, runner};
use spice_core::{AnalysisKind, SpiceError, SpiceResult};
use spice_netlist::Parser;
use spice_netlist::ast::{AnalysisCard, Netlist};

use crate::cli::kind_name;

/// The C files this command's contract comes from, quoted in "not ported" errors.
const C_REFERENCE: &str = "src/ngspice.c (batch job control), src/frontend/rawfile.c (raw_write)";

/// What one successful run produced, for the report printed afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The deck that was simulated.
    pub deck: PathBuf,
    /// The deck's title line, as written to the rawfile.
    pub title: String,
    /// The analysis that ran.
    pub analysis: AnalysisKind,
    /// The rawfile's `Plotname:` header, e.g. `Transient Analysis`.
    pub plotname: String,
    /// The rawfile's variable names, in column order.
    pub variables: Vec<String>,
    /// How many points the plot has.
    pub points: usize,
    /// The rawfile that was written.
    pub output: PathBuf,
}

/// Simulates the deck's single analysis and writes it as an ASCII rawfile.
///
/// # Errors
///
/// * [`SpiceError::Io`] when the deck cannot be read or the rawfile cannot be
///   written;
/// * [`SpiceError::Parse`] when the deck cannot be understood, or requests no
///   analysis at all;
/// * [`SpiceError::NotYetPorted`] when the deck requests more than one analysis,
///   because this command runs one analysis per invocation;
/// * whatever the production runner reports for unsupported devices, analyses,
///   options or numerically failed runs ([`SpiceError::Unsupported`],
///   [`SpiceError::NotYetPorted`], [`SpiceError::Numerical`], …).
///
/// The destination is written only after the runner returned a plot.
pub fn run(deck: &Path, output: &Path, auto_gnd: bool) -> SpiceResult<Report> {
    let netlist = Parser::with_auto_gnd(auto_gnd).parse_file(deck)?;
    let card = only_analysis(&netlist)?;
    // Options are validated before anything runs, exactly as `parse` does:
    // unknown or unsupported settings are errors, never ignored.
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(card)?;
    let mut circuit = config.circuit(&netlist)?;
    let plot = runner(request.kind)?.run(&mut circuit, &request, &config.context())?;

    let rawfile = rawfile_for(&netlist, plot, &now_header());
    let raw_plot = &rawfile.plots[0];
    let report = Report {
        deck: netlist.path.clone(),
        title: raw_plot.title.clone(),
        analysis: card.kind,
        plotname: raw_plot.plot.plotname.clone(),
        variables: raw_plot
            .plot
            .variables
            .iter()
            .map(|variable| variable.name.clone())
            .collect(),
        points: raw_plot.plot.point_count(),
        output: output.to_path_buf(),
    };
    write_rawfile(&rawfile, output)?;
    Ok(report)
}

/// The deck's analysis card, requiring exactly one.
///
/// # Errors
///
/// [`SpiceError::Parse`] when the deck requests none, and
/// [`SpiceError::NotYetPorted`] when it requests several: scheduling more than
/// one analysis per invocation is a gap in this command, not an implicit
/// success.
fn only_analysis(netlist: &Netlist) -> SpiceResult<&AnalysisCard> {
    match netlist.analyses.as_slice() {
        [only] => Ok(only),
        [] => Err(SpiceError::parse(
            netlist.location.clone(),
            "the deck requests no analysis: 'simulate' needs exactly one .op, .dc, .ac or .tran card",
        )),
        several => {
            let kinds: Vec<String> = several
                .iter()
                .map(|card| format!(".{}", card.kind.as_str()))
                .collect();
            Err(SpiceError::not_yet_ported(
                format!(
                    "{} analysis cards in one deck ({}); 'simulate' runs exactly one analysis \
                     per invocation (see docs/port/CLI.md)",
                    several.len(),
                    kinds.join(" "),
                ),
                C_REFERENCE,
            ))
        }
    }
}

/// The rawfile holding one plot: ngspice's three headers, then the data.
///
/// `title` is the deck's title line and `command` names the writer, as
/// `raw_write()` does for ngspice itself; `date` is the write time in UTC (see
/// [`now_header`]).
fn rawfile_for(netlist: &Netlist, plot: Plot, date: &str) -> RawFile {
    RawFile {
        plots: vec![RawPlot {
            title: netlist.title.trim().to_owned(),
            date: date.to_owned(),
            command: format!("spice-rs {} (Rust port), Build", env!("CARGO_PKG_VERSION")),
            plot,
        }],
    }
}

/// Writes `rawfile` to `output` through a temporary file in the same directory.
///
/// The temporary file is renamed into place only after the whole rawfile was
/// written, so a failure elsewhere in the run cannot leave a truncated or
/// partial destination, and an existing destination survives untouched. The
/// temporary file is removed when either step fails.
///
/// # Errors
///
/// [`SpiceError::Io`] when the destination names no file, when its directory is
/// missing, or when the write or the rename fails; [`SpiceError::Numerical`]
/// from [`RawFile::write`]'s consistency check.
fn write_rawfile(rawfile: &RawFile, output: &Path) -> SpiceResult<()> {
    let (directory, temporary) = temporary_path(output)?;
    if !directory.is_dir() {
        return Err(SpiceError::io(
            output,
            &std::io::Error::new(
                ErrorKind::NotFound,
                format!(
                    "the output directory {} does not exist or is not a directory",
                    directory.display()
                ),
            ),
        ));
    }
    if let Err(error) = rawfile.write(&temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, output) {
        let _ = fs::remove_file(&temporary);
        return Err(SpiceError::io(output, &error));
    }
    Ok(())
}

/// The directory `output` lives in and a unique temporary path beside it.
///
/// The temporary file must share the destination's directory so that the final
/// rename stays inside one filesystem and cannot copy a partial file.
///
/// # Errors
///
/// [`SpiceError::Io`] when `output` names no file (for instance `.` or `/`).
fn temporary_path(output: &Path) -> SpiceResult<(PathBuf, PathBuf)> {
    let directory = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let Some(name) = output.file_name() else {
        return Err(SpiceError::io(
            output,
            &std::io::Error::new(ErrorKind::InvalidInput, "the output path names no file"),
        ));
    };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.subsec_nanos());
    let temporary = directory.join(format!(
        ".{}.spice-rs-{}-{nanos}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    Ok((directory, temporary))
}

/// The `Date:` header for a rawfile written now, in ngspice's `ctime` spelling
/// (`Mon Oct  5 18:07:26 2026`), in **UTC**.
///
/// The port has no clock or timezone crate (and none may be added for this), so
/// the calendar fields are derived from the Unix epoch directly; a clock before
/// the epoch is reported as `unknown` instead of panicking.
fn now_header() -> String {
    format_ctime(SystemTime::now())
}

/// Renders a `ctime`-style UTC timestamp, e.g. `Thu Jan  1 00:00:00 1970`.
fn format_ctime(now: SystemTime) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let Ok(since_epoch) = now.duration_since(UNIX_EPOCH) else {
        return "unknown".to_owned();
    };
    let seconds = since_epoch.as_secs();
    let days = seconds / 86_400;
    let time_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    // 1970-01-01 was a Thursday, index 4 in `WEEKDAYS`.
    let weekday = WEEKDAYS[((days + 4) % 7) as usize];
    format!(
        "{weekday} {} {day:2} {:02}:{:02}:{:02} {year}",
        MONTHS[month - 1],
        time_of_day / 3600,
        time_of_day % 3600 / 60,
        time_of_day % 60,
    )
}

/// The civil date (`year`, 1-based `month`, 1-based `day`) of a day count since
/// 1970-01-01. Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: u64) -> (u64, usize, u64) {
    let days = days as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as u64, month as usize, day as u64)
}

/// Renders the report printed after a successful run.
#[must_use]
pub fn report_text(report: &Report) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let title = if report.title.trim().is_empty() {
        "<empty title line>"
    } else {
        report.title.trim()
    };
    let _ = writeln!(out, "deck:      {}", report.deck.display());
    let _ = writeln!(out, "title:     {title}");
    let _ = writeln!(
        out,
        "analysis:  .{:<4} {}",
        report.analysis.as_str(),
        kind_name(report.analysis)
    );
    let _ = writeln!(
        out,
        "plot:      {} ({} variable(s), {} point(s))",
        report.plotname,
        report.variables.len(),
        report.points
    );
    let _ = writeln!(out, "variables: {}", report.variables.join(" "));
    let _ = writeln!(
        out,
        "output:    {} (ngspice ASCII rawfile, no binary support)",
        report.output.display()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{Report, civil_from_days, format_ctime, report_text};
    use spice_core::AnalysisKind;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn the_timestamp_is_a_ctime_style_utc_string() {
        assert_eq!(
            format_ctime(UNIX_EPOCH),
            "Thu Jan  1 00:00:00 1970",
            "the epoch is a Thursday, with a space-padded day"
        );
        assert_eq!(
            format_ctime(UNIX_EPOCH + Duration::from_secs(1_791_223_646)),
            "Mon Oct  5 18:07:26 2026"
        );
        // A clock before the epoch is reported, not panicked on.
        assert_eq!(format_ctime(UNIX_EPOCH - Duration::from_secs(1)), "unknown");
    }

    #[test]
    fn the_civil_date_conversion_handles_leap_years_and_millennia() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_677), (2026, 8, 12));
        assert_eq!(civil_from_days(365), (1971, 1, 1));
    }

    #[test]
    fn the_report_names_the_deck_analysis_plot_and_output() {
        let report = Report {
            deck: PathBuf::from("rc.cir"),
            title: "RC divider".to_owned(),
            analysis: AnalysisKind::OperatingPoint,
            plotname: "Operating Point".to_owned(),
            variables: vec!["v(in)".to_owned(), "v(out)".to_owned()],
            points: 1,
            output: PathBuf::from("out.raw"),
        };
        let text = report_text(&report);
        assert!(text.contains("deck:      rc.cir"), "{text}");
        assert!(text.contains("title:     RC divider"), "{text}");
        assert!(
            text.contains("analysis:  .op   DC operating point"),
            "{text}"
        );
        assert!(
            text.contains("plot:      Operating Point (2 variable(s), 1 point(s))"),
            "{text}"
        );
        assert!(text.contains("variables: v(in) v(out)"), "{text}");
        assert!(text.contains("output:    out.raw"), "{text}");
    }

    #[test]
    fn an_empty_title_is_named_like_the_summary_does() {
        let report = Report {
            deck: PathBuf::from("rc.cir"),
            title: String::new(),
            analysis: AnalysisKind::Transient,
            plotname: "Transient Analysis".to_owned(),
            variables: Vec::new(),
            points: 0,
            output: PathBuf::from("out.raw"),
        };
        assert!(report_text(&report).contains("title:     <empty title line>"));
    }
}
