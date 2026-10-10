//! `spice-rs simulate` — run every analysis of a deck and write an ASCII rawfile.
//!
//! C: the batch path of `src/ngspice.c` (`ngspice -b -r`: `ft_dorun()` →
//! `CKTdoJob()`) plus the writer in `src/frontend/rawfile.c` (`raw_write`, ASCII
//! only). The command is deliberately narrow:
//!
//! * the deck is loaded by the ordinary [`crate::netlist::Parser`] and elaborated
//!   by the ordinary production runner, so no device or solver logic is
//!   duplicated here;
//! * **every** analysis card runs, in ngspice batch order (see
//!   [`crate::analysis::batch`]: `.ac`, `.dc`, `.op`, `.tran`, `.noise`,
//!   same-type cards in reverse deck order), each on a freshly elaborated
//!   circuit, and each result becomes one plot of a single multi-plot rawfile
//!   in that order (`.noise` contributes its spectrum and, for a frequency
//!   range, its integrated-noise plot);
//! * a deck's `.save` cards narrow every plot, and its `.print <type>` cards
//!   narrow (and print a table for) the plots of that type only, through
//!   [`crate::analysis::selection`]; the selection is resolved against the full
//!   plot the driver produced, so an unresolvable request fails before anything
//!   is written or printed;
//! * a deck's `.measure`/`.meas` cards are evaluated through
//!   [`crate::analysis::measure`] against the **full** last plot of their own
//!   analysis type, so an operand the output selection excluded is still
//!   measurable and a measurement never changes the written rawfile. A deck with
//!   no `.measure` card prints exactly what it printed before this work
//!   ([`Report`] carries only what was written and measured);
//! * a deck's `.four` cards are evaluated through [`crate::analysis::fourier`]
//!   against the same **full** last `.tran` plot, so a `.save`/`.print`
//!   selection never hides a transformed vector. A deck with no `.four` card
//!   prints exactly what it printed before this work, and a `.four` card never
//!   changes the written rawfile (C registers the named vectors for the
//!   transient plot instead);
//! * publication is atomic: every analysis runs and every output card resolves
//!   before anything is printed or written, so one failing analysis publishes
//!   nothing (C would keep the plots that succeeded);
//! * the rawfile is written through a temporary file in the destination's
//!   directory and renamed into place, so a failed run never truncates,
//!   replaces or removes an existing destination, and never leaves a partial
//!   rawfile behind (the temporary file is removed on every failure path that
//!   can run; only process death or a failing cleanup leaves one);
//! * an unadorned `.tran` runs the companion trap/Gear driver the engine
//!   defaults to; this command never injects `backend=diffsol`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::analysis::batch;
use crate::analysis::fourier::{self, FourierAnalysis};
use crate::analysis::measure::{self, Measurement};
use crate::analysis::{RawFile, RawPlot, RunConfig, runner};
use crate::netlist::Parser;
use crate::primitives::{AnalysisKind, SpiceError, SpiceResult};

use crate::cli::args::kind_name;

/// What one successful run produced, for the report printed afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The deck that was simulated.
    pub deck: PathBuf,
    /// The deck's title line, as written to the rawfile.
    pub title: String,
    /// The rawfile that was written.
    pub output: PathBuf,
    /// One entry per plot, in rawfile (ngspice batch) order.
    pub plots: Vec<PlotReport>,
}

/// What one analysis of a run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotReport {
    /// The name ngspice gives the plot in memory, e.g. `tran1` (not written to
    /// the rawfile; see [`crate::analysis::batch`]).
    pub name: String,
    /// The analysis that ran.
    pub analysis: AnalysisKind,
    /// The rawfile's `Plotname:` header, e.g. `Transient Analysis`.
    pub plotname: String,
    /// The rawfile's variable names, in column order (after `.save`/`.print`
    /// selection, when the deck has any).
    pub variables: Vec<String>,
    /// How many points the plot has.
    pub points: usize,
    /// The `.print` table, when the deck asked for one for this analysis type.
    /// `None` without an applicable `.print` card, which keeps the report
    /// unchanged.
    pub printed: Option<String>,
    /// The `.measure` results, when the deck asked for any of this analysis
    /// type. `None` otherwise, which keeps the report unchanged.
    pub measured: Option<String>,
    /// The measurement results themselves, in card order.
    pub measurements: Vec<Measurement>,
    /// The `.four` block, when the deck asked for one and this is the last
    /// `.tran` plot. `None` otherwise, which keeps the report unchanged.
    pub fourier: Option<String>,
    /// The Fourier results themselves, in card order, one per transformed
    /// vector.
    pub fourier_results: Vec<FourierAnalysis>,
}

/// Simulates every analysis of the deck and writes them as one ASCII rawfile.
///
/// # Errors
///
/// * [`SpiceError::Io`] when the deck cannot be read or the rawfile cannot be
///   written;
/// * [`SpiceError::Parse`] when the deck cannot be understood, or requests no
///   analysis at all;
/// * [`SpiceError::Unsupported`] when a `.print`, `.measure` or `.four` card
///   names an analysis type the deck does not run, when a `.save`/`.print`
///   request cannot be resolved against a plot (an unknown vector, an AC
///   component of a real plot), when a `.measure` request cannot be evaluated
///   against its plot (an operand the plot lacks, a window that covers no data,
///   a crossing that does not occur, …; see `docs/port/MEASURE.md`), or when a
///   `.four` request cannot be evaluated (a run shorter than one period, a
///   period with too few samples for the requested harmonics, a harmonic count
///   beyond the port's budget; see `docs/port/FOURIER.md`), and
///   [`SpiceError::Numerical`] when a computed vector, measurement or Fourier
///   value is not finite;
/// * whatever the production runner reports for unsupported devices, analyses,
///   options or numerically failed runs ([`SpiceError::Unsupported`],
///   [`SpiceError::NotYetPorted`], [`SpiceError::Numerical`], …), for **any**
///   of the deck's analyses.
///
/// Every analysis request and driver is validated, and every output card is
/// checked against the analysis types the deck runs, before the first analysis
/// starts. The destination is written only after every analysis returned a
/// plot, every selection resolved and every measurement and Fourier card was
/// evaluated, and the `.print` tables and measurement blocks are only returned
/// with the report, so a failure publishes nothing at all.
pub fn run(deck: &Path, output: &Path, auto_gnd: bool) -> SpiceResult<Report> {
    let parsed = Parser::with_auto_gnd(auto_gnd).parse_file_with_output(deck)?;
    let netlist = &parsed.netlist;
    if netlist.analyses.is_empty() {
        return Err(SpiceError::parse(
            netlist.location.clone(),
            format!(
                "the deck requests no analysis: 'simulate' needs at least one {} card",
                crate::analysis::driver::DRIVERS
                    .iter()
                    .map(|kind| format!(".{}", kind.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    // Options are validated before anything runs, exactly as `parse` does:
    // unknown or unsupported settings are errors, never ignored.
    let config = RunConfig::from_netlist(netlist)?;
    let schedule = batch::schedule_evaluated(&netlist.analyses, &config)?;
    // Every request and driver is validated before the first analysis runs, so
    // an unsupported last analysis cannot waste the earlier ones.
    let mut jobs = Vec::with_capacity(schedule.len());
    for entry in &schedule {
        let request = config.request_for(&netlist.analyses[entry.card_index])?;
        let driver = runner(request.kind)?;
        jobs.push((entry, request, driver));
    }
    let mut first_circuit = Some(config.circuit(netlist)?);
    batch::check_targets(
        &schedule,
        &parsed.output,
        &parsed.measurements,
        &parsed.fourier,
    )?;

    let date = now_header();
    let mut raw_plots = Vec::with_capacity(jobs.len());
    let mut plots = Vec::with_capacity(jobs.len());
    for (entry, request, driver) in jobs {
        // Each analysis starts from a freshly elaborated circuit: no device
        // state from an earlier analysis leaks into a later one.
        let mut circuit = match first_circuit.take() {
            Some(circuit) => circuit,
            None => config.circuit(netlist)?,
        };
        let produced = driver.run_plots(&mut circuit, &request, &config.context())?;
        let names: Vec<&str> = entry.plot_names().collect();
        if produced.len() != names.len() {
            return Err(SpiceError::Numerical {
                context: format!(".{} analysis", entry.kind.as_str()),
                message: format!(
                    "the driver produced {} plot(s), the batch schedule names {}",
                    produced.len(),
                    names.len()
                ),
            });
        }
        for (plot, name) in produced.iter().zip(names) {
            // The selection, the measurements and the Fourier cards are
            // resolved against the **full** plot and before anything is
            // written or printed: an unresolvable request leaves stdout empty
            // and an existing destination untouched.
            let outputs = batch::resolve_outputs(
                plot,
                entry,
                &parsed.output,
                &parsed.measurements,
                &parsed.fourier,
            )?;
            let raw_plot = raw_plot_for(&netlist.title, outputs.written, &date);
            plots.push(PlotReport {
                name: name.to_owned(),
                analysis: entry.kind,
                plotname: raw_plot.plot.plotname.clone(),
                variables: raw_plot
                    .plot
                    .variables
                    .iter()
                    .map(|variable| variable.name.clone())
                    .collect(),
                points: raw_plot.plot.point_count(),
                printed: outputs.printed,
                measured: (!outputs.measurements.is_empty())
                    .then(|| measure::to_text(&outputs.measurements)),
                measurements: outputs.measurements,
                fourier: (!outputs.fourier.is_empty()).then(|| fourier::to_text(&outputs.fourier)),
                fourier_results: outputs.fourier,
            });
            raw_plots.push(raw_plot);
        }
    }

    let rawfile = RawFile { plots: raw_plots };
    let report = Report {
        deck: netlist.path.clone(),
        title: rawfile.plots[0].title.clone(),
        output: output.to_path_buf(),
        plots,
    };
    write_rawfile(&rawfile, output)?;
    Ok(report)
}

/// One rawfile plot: ngspice's three headers, then the data.
///
/// `title` is the deck's title line and `command` names the writer, as
/// `raw_write()` does for ngspice itself; `date` is the write time in UTC (see
/// [`now_header`]). Every plot of one run carries the same three headers.
fn raw_plot_for(title: &str, plot: crate::analysis::Plot, date: &str) -> RawPlot {
    RawPlot {
        title: title.trim().to_owned(),
        date: date.to_owned(),
        command: format!("spice-rs {} (Rust port), Build", env!("CARGO_PKG_VERSION")),
        plot,
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

/// The `Date:` header for a rawfile written now: standard `ctime` spelling
/// (`Mon Oct  5 18:07:26 2026`) in **UTC**.
///
/// This is not byte-identical to ngspice's `datestring()`, which writes local
/// time and leaves an extra pre-year space (`Mon Oct  5 18:06:31  2026`); the
/// committed comparators ignore `Date:` entirely.
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
///
/// A single-analysis deck prints exactly the report it printed before
/// multi-analysis support: the analysis, plot and variable lines, the output
/// line, then the `.print` table, the `.measure` block and the `.four` block.
/// A multi-analysis deck prints the deck, title and output lines, the plot
/// names in rawfile order, and then one section per plot (headed by its C plot
/// name, e.g. `[tran1]`) with that plot's lines and blocks.
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
    if let [plot] = report.plots.as_slice() {
        plot_lines(&mut out, plot);
        let _ = writeln!(
            out,
            "output:    {} (ngspice ASCII rawfile, no binary support)",
            report.output.display()
        );
        plot_blocks(&mut out, plot);
        return out;
    }
    let _ = writeln!(
        out,
        "output:    {} (ngspice ASCII rawfile with {} plots, no binary support)",
        report.output.display(),
        report.plots.len()
    );
    let names: Vec<&str> = report.plots.iter().map(|plot| plot.name.as_str()).collect();
    let _ = writeln!(out, "plots:     {} (ngspice batch order)", names.join(" "));
    for plot in &report.plots {
        let _ = writeln!(out, "[{}]", plot.name);
        plot_lines(&mut out, plot);
        plot_blocks(&mut out, plot);
    }
    out
}

/// The analysis, plot and variable lines of one plot.
fn plot_lines(out: &mut String, plot: &PlotReport) {
    use std::fmt::Write as _;

    let _ = writeln!(
        out,
        "analysis:  .{:<4} {}",
        plot.analysis.as_str(),
        kind_name(plot.analysis)
    );
    let _ = writeln!(
        out,
        "plot:      {} ({} variable(s), {} point(s))",
        plot.plotname,
        plot.variables.len(),
        plot.points
    );
    let _ = writeln!(out, "variables: {}", plot.variables.join(" "));
}

/// The `.print`, `.measure` and `.four` blocks of one plot, in that order.
///
/// Only a plot with an applicable `.print` card has a table, only one with
/// `.measure` results has a measurement block, and only one with `.four`
/// results has a Fourier block; a deck without any of them keeps the report
/// unchanged.
fn plot_blocks(out: &mut String, plot: &PlotReport) {
    for block in [&plot.printed, &plot.measured, &plot.fourier]
        .into_iter()
        .flatten()
    {
        out.push_str(block);
    }
}

#[cfg(test)]
mod tests {
    use super::{PlotReport, Report, civil_from_days, format_ctime, report_text};
    use crate::primitives::AnalysisKind;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    fn plot(
        analysis: AnalysisKind,
        plotname: String,
        variables: Vec<String>,
        points: usize,
        printed: Option<String>,
        measured: Option<String>,
    ) -> PlotReport {
        PlotReport {
            name: format!("{}1", crate::analysis::batch::plot_abbreviation(analysis)),
            analysis,
            plotname,
            variables,
            points,
            printed,
            measured,
            measurements: Vec::new(),
            fourier: None,
            fourier_results: Vec::new(),
        }
    }

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
            output: PathBuf::from("out.raw"),
            plots: vec![plot(
                AnalysisKind::OperatingPoint,
                "Operating Point".to_owned(),
                vec!["v(in)".to_owned(), "v(out)".to_owned()],
                1,
                None,
                None,
            )],
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
        assert!(
            !text.contains("print:"),
            "a deck without .print has no table: {text}"
        );
        assert!(
            !text.contains("measure:"),
            "a deck without .measure has no measurement block: {text}"
        );
    }

    #[test]
    fn a_print_table_is_appended_after_the_report() {
        let report = Report {
            deck: PathBuf::from("rc.cir"),
            title: "RC divider".to_owned(),
            output: PathBuf::from("out.raw"),
            plots: vec![plot(
                AnalysisKind::OperatingPoint,
                "Operating Point".to_owned(),
                vec!["v(out)".to_owned()],
                1,
                Some("print: 1 vector(s): v(out)\nvalues: real\n".to_owned()),
                Some("measure: 1 result(s)\n".to_owned()),
            )],
        };
        let text = report_text(&report);
        let report_end = text.find("output:    out.raw").expect("the report");
        let table = text.find("print: 1 vector(s): v(out)").expect("the table");
        let measured = text.find("measure: 1 result(s)").expect("the block");
        assert!(report_end < table, "the table follows the report: {text}");
        assert!(table < measured, "the block follows the table: {text}");
    }

    #[test]
    fn an_empty_title_is_named_like_the_summary_does() {
        let report = Report {
            deck: PathBuf::from("rc.cir"),
            title: String::new(),
            output: PathBuf::from("out.raw"),
            plots: vec![plot(
                AnalysisKind::Transient,
                "Transient Analysis".to_owned(),
                Vec::new(),
                0,
                None,
                None,
            )],
        };
        assert!(report_text(&report).contains("title:     <empty title line>"));
    }

    #[test]
    fn a_multi_plot_report_has_one_section_per_plot_in_rawfile_order() {
        let mut tran = plot(
            AnalysisKind::Transient,
            "Transient Analysis".to_owned(),
            vec!["time".to_owned(), "v(out)".to_owned()],
            70,
            None,
            Some("measure: 1 result(s)\n".to_owned()),
        );
        tran.name = "tran2".to_owned();
        let mut ac = plot(
            AnalysisKind::Ac,
            "AC Analysis".to_owned(),
            vec!["frequency".to_owned(), "v(out)".to_owned()],
            3,
            Some("print: 1 vector(s): vm(out)\n".to_owned()),
            None,
        );
        ac.name = "ac1".to_owned();
        let report = Report {
            deck: PathBuf::from("rc.cir"),
            title: "RC".to_owned(),
            output: PathBuf::from("out.raw"),
            plots: vec![ac, tran],
        };
        let text = report_text(&report);
        assert!(
            text.contains("output:    out.raw (ngspice ASCII rawfile with 2 plots"),
            "{text}"
        );
        assert!(
            text.contains("plots:     ac1 tran2 (ngspice batch order)"),
            "{text}"
        );
        let ac = text.find("[ac1]\nanalysis:  .ac ").expect("the ac section");
        let table = text.find("print: 1 vector(s): vm(out)").expect("the table");
        let tran = text
            .find("[tran2]\nanalysis:  .tran")
            .expect("the tran section");
        let measured = text.find("measure: 1 result(s)").expect("the block");
        assert!(ac < table && table < tran && tran < measured, "{text}");
    }
}
