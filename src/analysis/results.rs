//! Result plots: the data an analysis produces.
//!
//! Mirrors ngspice's `struct plot` from `src/include/ngspice/plot.h` and the
//! `struct dvec` it holds. ngspice stores an array of data vectors plus scale
//! information for the interactive plotter; the port keeps a flat table of
//! variables and points, which is what a rawfile contains, and leaves plotting
//! to whoever consumes the result.

use crate::primitives::{Complex, Real, SpiceError, SpiceResult};

/// Whether a plot's data is real or complex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotFlags {
    /// Real data, as produced by `.op`, `.dc` and `.tran`.
    Real,
    /// Complex data, as produced by `.ac`, `.noise` and `.disto`.
    Complex,
}

impl PlotFlags {
    /// Interprets a rawfile `Flags:` line.
    ///
    /// ngspice writes `real`, `complex`, and for pole-zero analysis
    /// `real poles`. Only the presence of the word `complex` matters.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        if raw.to_ascii_lowercase().contains("complex") {
            Self::Complex
        } else {
            Self::Real
        }
    }

    /// The spelling used in a rawfile `Flags:` line.
    #[must_use]
    pub const fn as_rawfile(self) -> &'static str {
        match self {
            Self::Real => "real",
            Self::Complex => "complex",
        }
    }

    /// True when the data is complex.
    #[must_use]
    pub const fn is_complex(self) -> bool {
        matches!(self, Self::Complex)
    }
}

/// One column of a plot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// The name ngspice reports, e.g. `v(out)`, `i(v1)`, `frequency`.
    pub name: String,
    /// The unit ngspice reports: `voltage`, `current`, `time`, `frequency`,
    /// `device`, …
    pub unit: String,
    /// True when ngspice flags this data vector as real, i.e. `isreal(v)` in
    /// `src/frontend/rawfile.c`.
    ///
    /// Inside a `complex` plot, a vector flagged real is written as `re,0.0`,
    /// while a complex vector whose imaginary parts happen to be zero is written
    /// as `re,0.000000000000000e+00`. `.ac` and `.noise` produce complex vectors
    /// throughout, so their columns are not flagged real even when the imaginary
    /// part is zero — `frequency` and `v(in)` in an AC sweep, for instance.
    pub is_real: bool,
}

impl Variable {
    /// Builds a variable, flagged real.
    #[must_use]
    pub fn new(name: impl Into<String>, unit: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            unit: unit.into(),
            is_real: true,
        }
    }

    /// Builds a variable that ngspice does not flag as real: its values are
    /// written as `re,im` even when the imaginary part is zero.
    #[must_use]
    pub fn complex(name: impl Into<String>, unit: impl Into<String>) -> Self {
        Self {
            is_real: false,
            ..Self::new(name, unit)
        }
    }
}

/// The result of one analysis run.
#[derive(Debug, Clone, PartialEq)]
pub struct Plot {
    /// The plot's name. A rawfile does not record ngspice's internal plot name
    /// (`tran1`, `ac2`, …), so this is set from the plot name when one is loaded.
    pub name: String,
    /// The `Plotname:` header, e.g. `Operating Point`, `Transient Analysis`.
    pub plotname: String,
    /// Whether the data is real or complex.
    pub flags: PlotFlags,
    /// The columns.
    pub variables: Vec<Variable>,
    /// The rows: one entry per point, each holding `variables.len()` values.
    pub points: Vec<Vec<Complex>>,
}

impl Plot {
    /// An empty plot.
    #[must_use]
    pub fn new(name: impl Into<String>, plotname: impl Into<String>, flags: PlotFlags) -> Self {
        let plotname = plotname.into();
        Self {
            name: name.into(),
            plotname,
            flags,
            variables: Vec::new(),
            points: Vec::new(),
        }
    }

    /// Number of points.
    #[must_use]
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Number of columns.
    #[must_use]
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }

    /// True when the plot has no columns or no points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty() || self.points.is_empty()
    }

    /// Adds a column.
    pub fn push_variable(&mut self, variable: Variable) {
        self.variables.push(variable);
    }

    /// Appends a point.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the point does not have one value per
    /// variable.
    pub fn push_point(&mut self, values: Vec<Complex>) -> SpiceResult<()> {
        if values.len() != self.variables.len() {
            return Err(SpiceError::Numerical {
                context: format!("plot {}", self.name),
                message: format!(
                    "point has {} value(s) but the plot has {} variable(s)",
                    values.len(),
                    self.variables.len()
                ),
            });
        }
        self.points.push(values);
        Ok(())
    }

    /// The index of a variable, matched case-insensitively.
    #[must_use]
    pub fn variable_index(&self, name: &str) -> Option<usize> {
        self.variables
            .iter()
            .position(|variable| variable.name.eq_ignore_ascii_case(name))
    }

    /// One value, addressed by variable name and point index.
    #[must_use]
    pub fn value(&self, variable: &str, point: usize) -> Option<Complex> {
        let column = self.variable_index(variable)?;
        self.points.get(point)?.get(column).copied()
    }

    /// Every value of one variable.
    #[must_use]
    pub fn column(&self, variable: &str) -> Option<Vec<Complex>> {
        let column = self.variable_index(variable)?;
        self.points
            .iter()
            .map(|point| point.get(column).copied())
            .collect()
    }

    /// Largest absolute value anywhere in the plot.
    #[must_use]
    pub fn max_abs(&self) -> Real {
        self.points
            .iter()
            .flat_map(|point| point.iter())
            .fold(0.0, |acc, value| acc.max(value.magnitude()))
    }

    /// True when every value is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.points
            .iter()
            .flat_map(|point| point.iter())
            .all(|value| value.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::{Plot, PlotFlags, Variable};
    use crate::primitives::Complex;

    fn plot() -> Plot {
        let mut plot = Plot::new("op1", "Operating Point", PlotFlags::Real);
        plot.push_variable(Variable::new("v(out)", "voltage"));
        plot.push_variable(Variable::new("i(v1)", "current"));
        plot.push_point(vec![Complex::real(2.5), Complex::real(-2.5e-3)])
            .unwrap();
        plot
    }

    #[test]
    fn values_are_addressed_by_name() {
        let plot = plot();
        assert_eq!(plot.value("V(OUT)", 0), Some(Complex::real(2.5)));
        assert_eq!(plot.value("v(out)", 1), None);
        assert_eq!(plot.variable_index("i(v1)"), Some(1));
        assert_eq!(plot.column("i(v1)"), Some(vec![Complex::real(-2.5e-3)]));
        assert_eq!(plot.point_count(), 1);
        assert_eq!(plot.variable_count(), 2);
        assert!(!plot.is_empty());
        assert!(plot.is_finite());
        assert_eq!(plot.max_abs(), 2.5);
    }

    #[test]
    fn points_must_match_the_columns() {
        let mut plot = plot();
        let error = plot
            .push_point(vec![Complex::real(1.0)])
            .expect_err("arity mismatch");
        assert!(error.to_string().contains("1 value(s)"));
    }

    #[test]
    fn flags_are_read_from_the_rawfile_spelling() {
        assert_eq!(PlotFlags::parse("real"), PlotFlags::Real);
        assert_eq!(PlotFlags::parse("complex"), PlotFlags::Complex);
        assert_eq!(PlotFlags::parse("real poles"), PlotFlags::Real);
        assert_eq!(PlotFlags::parse("COMPLEX"), PlotFlags::Complex);
        assert_eq!(PlotFlags::Complex.as_rawfile(), "complex");
        assert!(PlotFlags::Complex.is_complex());
        assert!(!PlotFlags::Real.is_complex());
    }
}
