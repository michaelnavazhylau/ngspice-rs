//! ngspice rawfile I/O — the ASCII variant, read and write.
//!
//! Ported from `src/frontend/rawfile.c` (`ft_rawfile()`, `raw_read()`,
//! `raw_write()`). ngspice writes rawfiles in **binary** by default; the ASCII
//! form is selected with `set filetype=ascii`, which is what
//! `cargo xtask golden capture` does so that the golden files are diffable and
//! reviewable.
//!
//! An ASCII rawfile is a sequence of plots, each looking like:
//!
//! ```text
//! Title: rc divider
//! Date: Mon Oct  5 17:53:58 2026
//! Command: ngspice-47+, Build
//! Plotname: Operating Point
//! Flags: real
//! No. Variables: 3
//! No. Points: 1
//! Variables:
//! 	0	v(in)	voltage
//! 	1	v(out)	voltage
//! 	2	i(v1)	current
//! Values:
//!  0	5.000000000000000e+00
//! 	2.500000000000000e+00
//! 	-2.500000000000000e-03
//! ```
//!
//! Two details of the `Values:` section matter for parsing. The first value of
//! each point carries its point index, written as a space and the digits with no
//! tab; every point is followed by a blank line; and complex values are written
//! as `re,im`. Because every value is written with `%-.15e`, a leading run of
//! digits followed by whitespace can only be a point index — never a value —
//! which is how [`split_index_and_value`] tells them apart.
//!
//! **Binary rawfiles are not supported** and are reported as
//! [`SpiceError::Unsupported`].

// The `Values:` section is tab-separated, so the doc example above uses tabs.
// They are intentional: they show the exact byte layout ngspice writes.
#![allow(clippy::tabs_in_doc_comments)]

use std::fs;
use std::path::Path;

use spice_core::{Complex, Real, SpiceError, SpiceResult};

use crate::results::{Plot, PlotFlags, Variable};

fn unsupported(feature: impl Into<String>) -> SpiceError {
    SpiceError::Unsupported {
        feature: feature.into(),
        location: None,
    }
}

fn missing_header(key: &str) -> SpiceError {
    unsupported(format!("rawfile plot without a '{key}:' header"))
}

const BINARY_UNSUPPORTED: &str =
    "binary rawfile; only the ASCII form written by 'set filetype=ascii' is supported";

/// A plot together with the headers ngspice writes around it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawPlot {
    /// The `Title:` header, which is the deck's title line.
    pub title: String,
    /// The `Date:` header.
    pub date: String,
    /// The `Command:` header.
    pub command: String,
    /// The data.
    pub plot: Plot,
}

/// A rawfile: the plots it contains, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct RawFile {
    /// The plots, in file order.
    pub plots: Vec<RawPlot>,
}

impl RawFile {
    /// A rawfile holding a single plot with empty headers.
    #[must_use]
    pub fn single(plot: Plot) -> Self {
        Self {
            plots: vec![RawPlot {
                title: String::new(),
                date: String::new(),
                command: String::new(),
                plot,
            }],
        }
    }

    /// Number of plots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.plots.len()
    }

    /// True when the rawfile holds no plots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plots.is_empty()
    }

    /// The only plot, when the rawfile holds exactly one.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Unsupported`] when there are zero or several plots, because
    /// the caller asked for one and the file is ambiguous.
    pub fn single_plot(&self) -> SpiceResult<&Plot> {
        match self.plots.as_slice() {
            [only] => Ok(&only.plot),
            other => Err(unsupported(format!(
                "expected exactly one plot in the rawfile, found {}",
                other.len()
            ))),
        }
    }

    /// Parses ASCII rawfile text.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Unsupported`] for a binary rawfile, a malformed header, a
    /// truncated file, or values that are not numbers.
    pub fn parse(text: &str) -> SpiceResult<Self> {
        let mut cursor = Cursor::new(text);
        let mut plots = Vec::new();
        while let Some(plot) = parse_plot(&mut cursor)? {
            plots.push(plot);
        }
        Ok(Self { plots })
    }

    /// Reads and parses a rawfile.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Io`] when the file cannot be read, plus everything
    /// [`RawFile::parse`] reports.
    pub fn load(path: impl AsRef<Path>) -> SpiceResult<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|error| SpiceError::io(path, &error))?;
        Self::parse(&text)
    }

    /// Checks that every plot is internally consistent.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when a point does not have one value per
    /// variable.
    pub fn validate(&self) -> SpiceResult<()> {
        for raw_plot in &self.plots {
            let expected = raw_plot.plot.variables.len();
            for (index, point) in raw_plot.plot.points.iter().enumerate() {
                if point.len() != expected {
                    return Err(SpiceError::Numerical {
                        context: format!("plot {}", raw_plot.plot.name),
                        message: format!(
                            "point {index} has {} value(s) but the plot has {expected} variable(s)",
                            point.len()
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// Renders the rawfile in ngspice's ASCII format.
    ///
    /// The output round-trips through [`RawFile::parse`] and reproduces
    /// `raw_write()`'s layout byte for byte: the point index is a space and the
    /// digits with no tab, every point is followed by a blank line, a `real`
    /// plot carries only real parts, and a vector flagged real inside a
    /// `complex` plot is written as `re,0.0` (see [`Variable::is_real`]).
    ///
    /// Trailing whitespace in header values is not reproduced, because parsing
    /// trims it.
    #[must_use]
    pub fn to_ascii(&self) -> String {
        let mut out = String::new();
        for raw_plot in &self.plots {
            let plot = &raw_plot.plot;
            out.push_str(&format!("Title: {}\n", raw_plot.title));
            out.push_str(&format!("Date: {}\n", raw_plot.date));
            out.push_str(&format!("Command: {}\n", raw_plot.command));
            out.push_str(&format!("Plotname: {}\n", plot.plotname));
            out.push_str(&format!("Flags: {}\n", plot.flags.as_rawfile()));
            out.push_str(&format!("No. Variables: {}\n", plot.variables.len()));
            out.push_str(&format!("No. Points: {}\n", plot.points.len()));
            out.push_str("Variables:\n");
            for (index, variable) in plot.variables.iter().enumerate() {
                out.push_str(&format!(
                    "\t{index}\t{}\t{}\n",
                    variable.name, variable.unit
                ));
            }
            out.push_str("Values:\n");
            for (point_index, point) in plot.points.iter().enumerate() {
                out.push_str(&format!(" {point_index}"));
                for (column, value) in point.iter().enumerate() {
                    let is_real = plot
                        .variables
                        .get(column)
                        .is_some_and(|variable| variable.is_real);
                    out.push_str(&format!(
                        "\t{}\n",
                        format_value(*value, plot.flags, is_real)
                    ));
                }
                out.push('\n');
            }
        }
        out
    }

    /// Writes the rawfile in ASCII format.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] from [`RawFile::validate`], or
    /// [`SpiceError::Io`] when the file cannot be written.
    pub fn write(&self, path: impl AsRef<Path>) -> SpiceResult<()> {
        self.validate()?;
        let path = path.as_ref();
        fs::write(path, self.to_ascii()).map_err(|error| SpiceError::io(path, &error))
    }
}

/// A line cursor over rawfile text.
struct Cursor<'a> {
    lines: Vec<&'a str>,
    index: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            lines: text.lines().collect(),
            index: 0,
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.index).copied()
    }

    fn next(&mut self) -> Option<&'a str> {
        let line = self.peek();
        if line.is_some() {
            self.index += 1;
        }
        line
    }
}

fn parse_count(key: &str, value: &str) -> SpiceResult<usize> {
    value
        .trim()
        .parse::<usize>()
        .map_err(|_| unsupported(format!("rawfile header '{key}: {value}' is not a count")))
}

/// Parses one plot, or returns `None` at the end of the file.
fn parse_plot(cursor: &mut Cursor<'_>) -> SpiceResult<Option<RawPlot>> {
    while cursor.peek().is_some_and(|line| line.trim().is_empty()) {
        cursor.next();
    }
    if cursor.peek().is_none() {
        return Ok(None);
    }

    let mut title = String::new();
    let mut date = String::new();
    let mut command = String::new();
    let mut plotname = None;
    let mut flags = None;
    let mut declared_variables = None;
    let mut declared_points = None;

    // Header, up to and including the `Variables:` line.
    loop {
        let Some(line) = cursor.next() else {
            return Err(unsupported("truncated rawfile: no 'Variables:' section"));
        };
        let Some((key, value)) = line.split_once(':') else {
            return Err(unsupported(format!(
                "rawfile header line without a colon: '{line}'"
            )));
        };
        let key = key.trim();
        // ngspice writes exactly one space after the colon, so strip one and keep
        // the rest verbatim: `Command: ngspice-47+, Build ` really does end in a
        // space, and reproducing the file byte for byte depends on keeping it.
        let value = value.strip_prefix(' ').unwrap_or(value);
        match key {
            "Title" => title = value.to_owned(),
            "Date" => date = value.to_owned(),
            "Command" => command = value.to_owned(),
            "Plotname" => plotname = Some(value.to_owned()),
            "Flags" => flags = Some(PlotFlags::parse(value)),
            "No. Variables" => declared_variables = Some(parse_count(key, value.trim())?),
            "No. Points" => declared_points = Some(parse_count(key, value.trim())?),
            "Variables" => break,
            "Binary" => return Err(unsupported(BINARY_UNSUPPORTED)),
            other => {
                return Err(unsupported(format!("rawfile header key '{other}'")));
            }
        }
    }

    let variable_count = declared_variables.ok_or_else(|| missing_header("No. Variables"))?;
    let point_count = declared_points.ok_or_else(|| missing_header("No. Points"))?;
    let plotname = plotname.ok_or_else(|| missing_header("Plotname"))?;
    let flags = flags.ok_or_else(|| missing_header("Flags"))?;

    let mut variables = Vec::with_capacity(variable_count);
    for index in 0..variable_count {
        let Some(line) = cursor.next() else {
            return Err(unsupported(format!(
                "truncated rawfile: variable {index} of {variable_count} is missing"
            )));
        };
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(declared), Some(name)) = (fields.first(), fields.get(1)) else {
            return Err(unsupported(format!(
                "rawfile variable line with fewer than two fields: '{line}'"
            )));
        };
        let declared: usize = declared.parse().map_err(|_| {
            unsupported(format!(
                "rawfile variable index '{declared}' is not a number"
            ))
        })?;
        if declared != index {
            return Err(unsupported(format!(
                "rawfile variable index {declared} out of order, expected {index}"
            )));
        }
        variables.push(Variable::new(*name, fields.get(2).copied().unwrap_or("")));
    }

    let Some(line) = cursor.next() else {
        return Err(unsupported("truncated rawfile: no 'Values:' section"));
    };
    let key = line
        .split_once(':')
        .map_or(line.trim(), |(key, _)| key.trim())
        .to_owned();
    match key.as_str() {
        "Values" => {}
        "Binary" => return Err(unsupported(BINARY_UNSUPPORTED)),
        other => {
            return Err(unsupported(format!(
                "rawfile values section starts with '{other}', expected 'Values'"
            )));
        }
    }

    let mut points: Vec<Vec<Complex>> = Vec::with_capacity(point_count);
    let mut row: Vec<Complex> = Vec::with_capacity(variable_count);
    // `raw_write()` writes the `re,0.0` short form only for vectors it has
    // flagged real, so the spelling in the file recovers the flag. In a real
    // plot every vector is real; in a complex plot a vector is real unless one
    // of its values used the full `re,im` spelling.
    let mut column_is_real: Vec<bool> = vec![true; variable_count];

    for flat_index in 0..point_count * variable_count {
        // `raw_write()` emits a blank line after every point.
        while cursor.peek().is_some_and(|line| line.trim().is_empty()) {
            cursor.next();
        }
        let Some(line) = cursor.next() else {
            return Err(unsupported(format!(
                "truncated rawfile: expected {point_count} point(s) of {variable_count} value(s)"
            )));
        };
        let expected_point = flat_index / variable_count.max(1);
        let column = flat_index % variable_count.max(1);
        let (declared_point, value_text) = split_index_and_value(line);
        if column == 0 {
            if let Some(declared_point) = declared_point {
                if declared_point != expected_point {
                    return Err(unsupported(format!(
                        "rawfile point index {declared_point} out of order, expected {expected_point}"
                    )));
                }
            }
            if !row.is_empty() {
                points.push(std::mem::take(&mut row));
            }
        }
        let parsed = parse_value(value_text)?;
        // A real plot carries no commas at all, so the flag is only meaningful
        // for a complex plot.
        if flags.is_complex() && !parsed.real_short_form {
            if let Some(is_real) = column_is_real.get_mut(column) {
                *is_real = false;
            }
        }
        row.push(parsed.value);
    }
    if !row.is_empty() {
        points.push(row);
    }
    for (index, variable) in variables.iter_mut().enumerate() {
        variable.is_real = column_is_real.get(index).copied().unwrap_or(true);
    }

    let mut plot = Plot::new(plotname.clone(), plotname, flags);
    plot.variables = variables;
    plot.points = points;

    Ok(Some(RawPlot {
        title,
        date,
        command,
        plot,
    }))
}

/// Splits the optional point index ngspice writes before the first value of each
/// point.
///
/// Values are always written with `%-.15e`, so they contain a `.` or an `e`; a
/// leading run of digits followed by whitespace is therefore an index.
fn split_index_and_value(line: &str) -> (Option<usize>, &str) {
    let trimmed = line.trim_start();
    if let Some(position) = trimmed.find(|character: char| character.is_ascii_whitespace()) {
        let head = &trimmed[..position];
        let rest = trimmed[position..].trim_start();
        if !head.is_empty() && !rest.is_empty() && head.bytes().all(|byte| byte.is_ascii_digit()) {
            if let Ok(index) = head.parse::<usize>() {
                return (Some(index), rest);
            }
        }
    }
    (None, line.trim())
}

/// Formats one value the way `raw_write()` does.
///
/// `column_is_real` is the column's [`Variable::is_real`] flag, which selects
/// the `re,0.0` short form for a vector ngspice has flagged real.
fn format_value(value: Complex, flags: PlotFlags, column_is_real: bool) -> String {
    let real = spice_core::format_spice_number(value.re);
    match flags {
        PlotFlags::Real => real,
        PlotFlags::Complex if column_is_real => format!("{real},0.0"),
        PlotFlags::Complex => format!("{real},{}", spice_core::format_spice_number(value.im)),
    }
}

fn parse_value(text: &str) -> SpiceResult<ParsedValue> {
    let text = text.trim();
    match text.split_once(',') {
        Some((real, imaginary)) => Ok(ParsedValue {
            value: Complex::new(parse_real(real)?, parse_real(imaginary)?),
            real_short_form: imaginary.trim() == "0.0",
        }),
        None => Ok(ParsedValue {
            value: Complex::real(parse_real(text)?),
            real_short_form: false,
        }),
    }
}

/// One parsed value, and whether it used the `re,0.0` short form.
struct ParsedValue {
    value: Complex,
    /// True when the imaginary part was written as the literal `0.0`, which
    /// `raw_write()` only does for a vector flagged real.
    real_short_form: bool,
}

fn parse_real(text: &str) -> SpiceResult<Real> {
    text.trim()
        .parse::<Real>()
        .map_err(|_| unsupported(format!("rawfile value '{}'", text.trim())))
}

#[cfg(test)]
mod tests {
    use super::{RawFile, split_index_and_value};
    use crate::results::PlotFlags;
    use spice_core::Complex;

    const OP_RAWFILE: &str = "\
Title: rc divider
Date: Mon Oct  5 17:53:58 2026
Command: ngspice-47+, Build
Plotname: Operating Point
Flags: real
No. Variables: 3
No. Points: 1
Variables:
\t0\tv(in)\tvoltage
\t1\tv(out)\tvoltage
\t2\ti(v1)\tcurrent
Values:
 0\t5.000000000000000e+00
\t2.500000000000000e+00
\t-2.500000000000000e-03
";

    const AC_RAWFILE: &str = "\
Title: rc lowpass
Date: Mon Oct  5 18:00:00 2026
Command: ngspice-47+, Build
Plotname: AC Analysis
Flags: complex
No. Variables: 3
No. Points: 2
Variables:
\t0\tfrequency\tfrequency
\t1\tv(out)\tvoltage
\t2\ti(v1)\tcurrent
Values:
 0\t1.000000000000000e+02,0.000000000000000e+00
\t9.900000000000000e-01,-1.000000000000000e-02
\t-1.000000000000000e-03,1.000000000000000e-05
 1\t1.000000000000000e+03,0.000000000000000e+00
\t9.000000000000000e-01,-1.000000000000000e-01
\t-1.000000000000000e-03,1.000000000000000e-04
";

    #[test]
    fn parses_ngspice_operating_point_output() {
        let rawfile = RawFile::parse(OP_RAWFILE).expect("parses");
        assert_eq!(rawfile.len(), 1);
        let raw_plot = &rawfile.plots[0];
        assert_eq!(raw_plot.title, "rc divider");
        assert_eq!(raw_plot.plot.plotname, "Operating Point");
        assert_eq!(raw_plot.plot.flags, PlotFlags::Real);
        assert_eq!(raw_plot.plot.variables.len(), 3);
        assert_eq!(raw_plot.plot.variables[1].name, "v(out)");
        assert_eq!(raw_plot.plot.variables[1].unit, "voltage");
        assert_eq!(raw_plot.plot.points.len(), 1);
        assert_eq!(raw_plot.plot.value("v(out)", 0), Some(Complex::real(2.5)));
        assert_eq!(raw_plot.plot.value("v(in)", 0), Some(Complex::real(5.0)));
        assert_eq!(
            raw_plot.plot.value("i(v1)", 0),
            Some(Complex::real(-2.5e-3))
        );
    }

    #[test]
    fn parses_complex_values_and_point_indices() {
        let rawfile = RawFile::parse(AC_RAWFILE).expect("parses");
        let plot = &rawfile.single_plot().expect("one plot");
        assert_eq!(plot.flags, PlotFlags::Complex);
        assert_eq!(plot.points.len(), 2);
        assert_eq!(plot.value("frequency", 1), Some(Complex::new(1e3, 0.0)));
        assert_eq!(plot.value("v(out)", 1), Some(Complex::new(0.9, -0.1)));
        assert_eq!(plot.value("i(v1)", 0), Some(Complex::new(-1e-3, 1e-5)));
    }

    #[test]
    fn values_round_trip_through_the_writer() {
        for text in [OP_RAWFILE, AC_RAWFILE] {
            let parsed = RawFile::parse(text).expect("parses");
            let rendered = parsed.to_ascii();
            let reparsed = RawFile::parse(&rendered).expect("reparses");
            assert_eq!(parsed, reparsed);
        }
    }

    #[test]
    fn the_writer_matches_ngspice_layout() {
        let rawfile = RawFile::parse(OP_RAWFILE).expect("parses");
        let rendered = rawfile.to_ascii();
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines[0], "Title: rc divider");
        assert_eq!(lines[3], "Plotname: Operating Point");
        assert_eq!(lines[4], "Flags: real");
        assert_eq!(lines[5], "No. Variables: 3");
        assert_eq!(lines[6], "No. Points: 1");
        assert_eq!(lines[7], "Variables:");
        assert_eq!(lines[8], "\t0\tv(in)\tvoltage");
        assert_eq!(lines[11], "Values:");
        assert_eq!(lines[12], " 0\t5.000000000000000e+00");
        assert_eq!(lines[13], "\t2.500000000000000e+00");
        assert_eq!(lines[14], "\t-2.500000000000000e-03");
        // `raw_write()` emits a blank line after every point.
        assert_eq!(lines[15], "");
    }

    #[test]
    fn parses_concatenated_plots() {
        let text = format!("{OP_RAWFILE}\n{AC_RAWFILE}");
        let rawfile = RawFile::parse(&text).expect("parses");
        assert_eq!(rawfile.len(), 2);
        assert_eq!(rawfile.plots[0].plot.plotname, "Operating Point");
        assert_eq!(rawfile.plots[1].plot.plotname, "AC Analysis");
        assert!(rawfile.single_plot().is_err());
    }

    #[test]
    fn rejects_binary_rawfiles() {
        let text = OP_RAWFILE.replace("Values:", "Binary:");
        let error = RawFile::parse(&text).expect_err("binary is unsupported");
        assert!(error.to_string().contains("binary rawfile"));
    }

    #[test]
    fn rejects_truncated_and_malformed_input() {
        assert!(RawFile::parse("Title: x\n").is_err());
        assert!(RawFile::parse("nonsense\n").is_err());
        let mut text = OP_RAWFILE.to_owned();
        text.truncate(text.len() - 30);
        assert!(RawFile::parse(&text).is_err());
    }

    #[test]
    fn rejects_misnumbered_variables_and_points() {
        let text = OP_RAWFILE.replace("\t1\tv(out)", "\t7\tv(out)");
        let error = RawFile::parse(&text).expect_err("index out of order");
        assert!(error.to_string().contains("variable index 7"), "{error}");

        let text = AC_RAWFILE.replace("\n 1\t1.000000000000000e+03", "\n 5\t1.000000000000000e+03");
        let error = RawFile::parse(&text).expect_err("point out of order");
        assert!(error.to_string().contains("point index 5"), "{error}");
    }

    #[test]
    fn the_index_heuristic_distinguishes_values_from_indices() {
        assert_eq!(
            split_index_and_value(" 0\t5.000000000000000e+00"),
            (Some(0), "5.000000000000000e+00")
        );
        assert_eq!(
            split_index_and_value("\t2.500000000000000e+00"),
            (None, "2.500000000000000e+00")
        );
        assert_eq!(split_index_and_value("12\t-1.0e-3"), (Some(12), "-1.0e-3"));
        // A bare number with nothing after it is a value, not an index.
        assert_eq!(split_index_and_value("5"), (None, "5"));
        assert_eq!(
            split_index_and_value("\t-2.500000000000000e-03"),
            (None, "-2.500000000000000e-03")
        );
    }

    const MIXED_RAWFILE: &str = "\
Title: noise
Date: Mon Oct  5 18:00:00 2026
Command: ngspice-47+, Build
Plotname: Noise Analysis
Flags: complex
No. Variables: 2
No. Points: 1
Variables:
\t0\tfrequency\tfrequency
\t1\tinoise_spectrum\tnoise-spectral-density
Values:
 0\t1.000000000000000e+03,0.0
\t1.000000000000000e-18,0.000000000000000e+00

";

    #[test]
    fn the_real_short_form_recovers_the_vector_flag() {
        // `raw_write()` writes `re,0.0` for a vector it has flagged real and
        // `re,im` otherwise, so the spelling in the file tells the two apart.
        let rawfile = RawFile::parse(MIXED_RAWFILE).expect("parses");
        let plot = &rawfile.plots[0].plot;
        assert!(plot.variables[0].is_real, "frequency was written as re,0.0");
        assert!(!plot.variables[1].is_real);
        assert_eq!(rawfile.to_ascii(), MIXED_RAWFILE);
    }

    #[test]
    fn a_real_plot_flags_every_column_real() {
        let rawfile = RawFile::parse(OP_RAWFILE).expect("parses");
        assert!(rawfile.plots[0].plot.variables.iter().all(|v| v.is_real));
    }

    #[test]
    fn an_ac_plot_flags_no_column_real() {
        // ngspice stores AC results as complex vectors, so even `frequency` is
        // written as `re,0.000000000000000e+00`.
        let rawfile = RawFile::parse(AC_RAWFILE).expect("parses");
        assert!(rawfile.plots[0].plot.variables.iter().all(|v| !v.is_real));
    }

    #[test]
    fn empty_input_parses_to_an_empty_rawfile() {
        let rawfile = RawFile::parse("\n\n").expect("parses");
        assert!(rawfile.is_empty());
        assert!(rawfile.single_plot().is_err());
    }

    #[test]
    fn validate_catches_ragged_plots() {
        let mut rawfile = RawFile::parse(OP_RAWFILE).expect("parses");
        rawfile.plots[0].plot.points[0].pop();
        assert!(rawfile.validate().is_err());
    }
}
