//! Output selection: the vectors `.save`/`.print` asks an analysis to write.
//!
//! An ngspice deck can narrow what an analysis writes with `.save` (deck-wide)
//! and `.print <analysis> ...` (analysis-specific). Both are parsed by
//! `netlist` into positioned [`OutputCards`]; this module turns those
//! requests into an ordered projection of the **full** in-memory [`Plot`] the
//! driver produced. The full plot is never discarded: the caller keeps it for
//! the measurement work that follows and only the written plot is narrowed.
//!
//! C reference: `ft_dotsaves()`/`ft_savedotargs()` in `src/frontend/dotcards.c`
//! collect the deck's cards into the `dbs` save list via `com_save()`
//! (`src/frontend/breakp2.c`); `beginPlot()` (`src/frontend/outitf.c`) then adds
//! the reference vector (`time`, `frequency`, the swept source) first and only
//! the saves that match the running analysis.
//!
//! The documented semantics — default, ordering, duplicates, missing and
//! unsupported requests, and the text format — are in
//! `docs/port/OUTPUT_SELECTION.md`.

use crate::netlist::ast::{OutputCards, RequestedVector, VectorComponent, VectorRequest};
use crate::primitives::{AnalysisKind, Complex, Real, SpiceError, SpiceResult};

use crate::analysis::results::{Plot, PlotFlags, Variable};

/// The `.save` and `.print` requests that apply to `kind`.
///
/// `.save` cards apply to every analysis; a `.print` card names exactly one and
/// must name `kind`: this is the single-plot form, so a `.print` card for
/// another analysis can never be honoured against this plot and is an explicit
/// [`SpiceError::Unsupported`] rather than a silently dropped request. A
/// multi-analysis deck routes each `.print` card to the plots of its own type
/// first ([`crate::analysis::batch::resolve_outputs`]).
///
/// Order is C's `dbs` order: every `.save` request in card order, then every
/// applicable `.print` request in card order. Duplicates stay visible here;
/// [`Selection::resolve`] collapses them.
///
/// # Errors
///
/// [`SpiceError::Unsupported`] for a `.print` card whose analysis is not `kind`.
pub fn write_requests(cards: &OutputCards, kind: AnalysisKind) -> SpiceResult<Vec<VectorRequest>> {
    applicable(cards, kind, true)
}

/// The `.print` requests that apply to `kind`: what `simulate` also prints as
/// text (`.save` never prints).
///
/// # Errors
///
/// [`SpiceError::Unsupported`] for a `.print` card whose analysis is not `kind`.
pub fn print_requests(cards: &OutputCards, kind: AnalysisKind) -> SpiceResult<Vec<VectorRequest>> {
    applicable(cards, kind, false)
}

fn applicable(
    cards: &OutputCards,
    kind: AnalysisKind,
    include_saves: bool,
) -> SpiceResult<Vec<VectorRequest>> {
    let mut requests = Vec::new();
    if include_saves {
        for save in &cards.saves {
            requests.extend(save.requests.iter().cloned());
        }
    }
    for print in &cards.prints {
        if print.analysis != kind {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    ".print {} names a different analysis; this plot is .{}, so the request \
                     can never be honoured against it",
                    print.analysis.as_str(),
                    kind.as_str()
                ),
                location: Some(print.analysis_location.clone()),
            });
        }
        requests.extend(print.requests.iter().cloned());
    }
    Ok(requests)
}

/// One written column: a signed sum of full-plot columns, then an optional AC
/// component.
///
/// Shared with [`crate::analysis::measure`], which resolves a measurement operand through
/// exactly the same rules (differences, ground and AC components) so that a
/// measured vector and a written vector can never disagree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Column {
    /// The name written to the rawfile.
    name: String,
    /// The rawfile unit.
    unit: String,
    /// The `Variable::is_real` flag of the written column.
    is_real: bool,
    /// `(column index in the full plot, factor)`; the factors are `+1` or `-1`.
    terms: Vec<(usize, Real)>,
    /// An AC component applied after the sum.
    component: Option<VectorComponent>,
}

/// The ordered projection of a full plot that one deck asked for.
///
/// Build it with [`Selection::resolve`]; write it with [`Selection::apply`] and
/// render it with [`Selection::to_text`].
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// True when the deck asked for `all` (or for nothing): the full plot is
    /// written unchanged.
    full: bool,
    /// The written columns, in order. Only used when `!full`.
    columns: Vec<Column>,
}

impl Selection {
    /// Resolves `requests` against the full `plot`.
    ///
    /// * an empty request list, or any `all` request, keeps the driver's whole
    ///   vector set in the driver's order ([`Selection::is_full`]);
    /// * every sweep analysis (`.dc`, `.ac`, `.tran`) keeps its independent
    ///   vector first, as C's `beginPlot()` pass 0 does; an operating-point
    ///   result has none, so only the requests are written;
    /// * requests are written in request order, and a request whose resolved
    ///   vector is already written is dropped (first occurrence wins);
    /// * a request no full-plot column satisfies, and an AC component asked of a
    ///   real plot, are errors — never invented values.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Unsupported`], carrying the request's `SourceLoc`, for a
    /// vector the full plot does not have or for an AC component of a real
    /// (`op`/`dc`/`tran`) plot.
    pub fn resolve(
        plot: &Plot,
        kind: AnalysisKind,
        requests: &[VectorRequest],
    ) -> SpiceResult<Self> {
        if requests.is_empty() {
            return Ok(Self {
                full: true,
                columns: driver_columns(plot),
            });
        }
        // `all` widens the written plot to the driver's whole set, but every
        // other request is still resolved, so an unresolvable vector is an error
        // rather than being dropped because an `all` happened to be present.
        let full = requests
            .iter()
            .any(|request| request.vector == RequestedVector::All);
        let mut columns: Vec<Column> = Vec::new();
        // C's `beginPlot()` pass 0: the reference vector is always written first.
        if kind != AnalysisKind::OperatingPoint
            && let Some(scale) = plot.variables.first()
        {
            columns.push(Column {
                name: scale.name.clone(),
                unit: scale.unit.clone(),
                is_real: scale.is_real,
                terms: vec![(0, 1.0)],
                component: None,
            });
        }
        for request in requests {
            if request.vector == RequestedVector::All {
                continue;
            }
            let column = resolve_request(plot, request)?;
            // The written vector is the plan (the signed sum and the component),
            // not the spelling: `v(out)` and `v(out,0)` are one vector, and the
            // first request's spelling names it.
            if !columns.iter().any(|existing| existing.same_vector(&column)) {
                columns.push(column);
            }
        }
        if full {
            // The whole plot is written in the driver's order, so the resolved
            // columns describe the driver's variables; the report and the text
            // table would otherwise claim a successful run wrote nothing.
            return Ok(Self {
                full: true,
                columns: driver_columns(plot),
            });
        }
        Ok(Self {
            full: false,
            columns,
        })
    }

    /// True when the driver's whole plot is written, in the driver's order.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// The names of the written vectors, in order.
    #[must_use]
    pub fn variable_names(&self) -> Vec<&str> {
        self.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect()
    }

    /// Writes the selection out of `plot`: one variable and one value per
    /// written column, in request order.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when a computed value is not finite (for
    /// instance `vdb` of a zero magnitude), so a selection never makes a
    /// rawfile carry `inf` or `NaN`.
    pub fn apply(&self, plot: &Plot) -> SpiceResult<Plot> {
        if self.full {
            return Ok(plot.clone());
        }
        let mut selected = Plot::new(plot.name.clone(), plot.plotname.clone(), plot.flags);
        for column in &self.columns {
            selected.push_variable(Variable {
                name: column.name.clone(),
                unit: column.unit.clone(),
                is_real: column.is_real,
            });
        }
        for point in &plot.points {
            let mut row = Vec::with_capacity(self.columns.len());
            for column in &self.columns {
                row.push(column.value(point)?);
            }
            selected.push_point(row)?;
        }
        Ok(selected)
    }

    /// Renders the written vectors of `plot` as text, for the CLI's report.
    ///
    /// One line per point, preceded by the header naming the written vectors and
    /// by the convention the values use: they are spelled exactly as the ASCII
    /// rawfile spells them, so `re,im` in a complex plot.
    ///
    /// # Errors
    ///
    /// The same [`SpiceError::Numerical`] as [`Selection::apply`].
    pub fn to_text(&self, plot: &Plot) -> SpiceResult<String> {
        use std::fmt::Write as _;

        let mut out = String::new();
        let names = self.variable_names();
        let _ = writeln!(
            out,
            "print: {} vector(s): {}",
            names.len(),
            if names.is_empty() {
                "<none>".to_owned()
            } else {
                names.join(" ")
            }
        );
        let _ = writeln!(
            out,
            "values: {}; computed from the plot, not the rawfile's own spelling",
            if plot.flags.is_complex() {
                "complex as `re,im` with 15 fractional digits, except that a component which is \
                 real for every point prints as one number (C's print behaviour; the rawfile \
                 spells that column `re,0.0`); vm is |v|, vp is the phase in radians in \
                 (-pi, pi], vr/vi are the real and imaginary parts, vdb is 20*log10|v|"
            } else {
                "real with 15 fractional digits"
            }
        );
        if names.is_empty() {
            return Ok(out);
        }
        let _ = writeln!(out, "{:>7}  {}", "point", names.join("  "));
        for (index, point) in plot.points.iter().enumerate() {
            let mut values = Vec::with_capacity(self.columns.len());
            for column in &self.columns {
                let value = column.value(point)?;
                let spelling = crate::primitives::format_spice_number(value.re);
                values.push(if plot.flags == PlotFlags::Complex && !column.is_real {
                    format!(
                        "{spelling},{}",
                        crate::primitives::format_spice_number(value.im)
                    )
                } else {
                    spelling
                });
            }
            let _ = writeln!(out, "{index:>7}  {}", values.join("  "));
        }
        Ok(out)
    }
}

impl Column {
    /// True when two plans write the same vector, whatever they are spelled.
    ///
    /// The unit and the real flag follow from the plan and the plot, so the
    /// signed sum and the component identify it: `v(out)`, `v(out,0)` and
    /// `v(out)` again are one written vector.
    fn same_vector(&self, other: &Self) -> bool {
        self.terms == other.terms && self.component == other.component
    }

    /// The written name of this column, e.g. `v(out)`.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The unit of this column, e.g. `voltage`, `phase` or `db`.
    pub(crate) fn unit(&self) -> &str {
        &self.unit
    }

    /// The value of this column at `point`.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the computed value is not finite.
    pub(crate) fn value(&self, point: &[Complex]) -> SpiceResult<Complex> {
        let mut sum = Complex::real(0.0);
        for (index, factor) in &self.terms {
            let Some(value) = point.get(*index) else {
                return Err(SpiceError::Numerical {
                    context: format!("selection: {}", self.name),
                    message: format!("the plot has no column {index}"),
                });
            };
            sum = sum + *value * Complex::from(*factor);
        }
        let value = match self.component {
            None => sum,
            Some(component) => component_value(component, sum),
        };
        if !value.is_finite() {
            return Err(SpiceError::Numerical {
                context: format!("selection: {}", self.name),
                message: "the computed value is not finite".to_owned(),
            });
        }
        Ok(value)
    }
}

/// Applies one AC component to a complex value.
fn component_value(component: VectorComponent, value: Complex) -> Complex {
    match component {
        VectorComponent::Magnitude => Complex::real(value.magnitude()),
        VectorComponent::Phase => Complex::real(value.im.atan2(value.re)),
        VectorComponent::Real => Complex::real(value.re),
        VectorComponent::Imaginary => Complex::real(value.im),
        VectorComponent::Decibels => Complex::real(20.0 * value.magnitude().log10()),
    }
}

/// Every variable of the driver's plot, as an identity selection.
///
/// A full selection writes the plot unchanged, but its columns still describe
/// what is written so reports and text tables are not empty.
fn driver_columns(plot: &Plot) -> Vec<Column> {
    plot.variables
        .iter()
        .enumerate()
        .map(|(index, variable)| Column {
            name: variable.name.clone(),
            unit: variable.unit.clone(),
            is_real: variable.is_real,
            terms: vec![(index, 1.0)],
            component: None,
        })
        .collect()
}

/// The column one request resolves to.
///
/// # Errors
///
/// [`SpiceError::Unsupported`], carrying the request's `SourceLoc`, for a vector
/// the plot does not carry or an AC component of a real plot.
pub(crate) fn resolve_request(plot: &Plot, request: &VectorRequest) -> SpiceResult<Column> {
    let name = request.vector.name();
    let unsupported = |detail: String| SpiceError::Unsupported {
        feature: detail,
        location: Some(request.location.clone()),
    };
    match &request.vector {
        // `all` keeps the whole plot, so `Selection::resolve` handles it before
        // asking for a single column.
        RequestedVector::Named {
            name: vector,
            component,
        } => {
            let index = plot
                .variable_index(vector)
                .or_else(|| plot.variable_index(&format!("v({vector})")))
                .or_else(|| plot.variable_index(&format!("i({vector})")))
                .ok_or_else(|| {
                    unsupported(format!(
                        "{name}: the analysis has no named vector {vector}; it carries {}",
                        column_names(plot)
                    ))
                })?;
            let variable = &plot.variables[index];
            let unit = match component {
                Some(VectorComponent::Phase) => "phase".to_owned(),
                Some(VectorComponent::Decibels) => "db".to_owned(),
                _ => variable.unit.clone(),
            };
            Ok(Column {
                name,
                unit,
                is_real: component.is_some() || variable.is_real,
                terms: vec![(index, 1.)],
                component: *component,
            })
        }
        RequestedVector::All => Err(unsupported(format!(
            "{name}: resolved by Selection::resolve, not per request"
        ))),
        RequestedVector::Voltage { positive, negative } => {
            let terms =
                difference(plot, positive, negative.as_deref(), &name).map_err(unsupported)?;
            Ok(Column {
                name,
                unit: if negative.is_none() && terms.len() == 1 {
                    plot.variables[terms[0].0].unit.clone()
                } else {
                    "voltage".to_owned()
                },
                is_real: !plot.flags.is_complex(),
                terms,
                component: None,
            })
        }
        RequestedVector::Current { device } => {
            let column = plot
                .variable_index(&format!("i({device})"))
                .ok_or_else(|| {
                    unsupported(format!(
                        "{name}: the analysis has no branch-current vector; it carries {}",
                        column_names(plot)
                    ))
                })?;
            Ok(Column {
                name,
                unit: "current".to_owned(),
                is_real: plot.variables[column].is_real,
                terms: vec![(column, 1.0)],
                component: None,
            })
        }
        RequestedVector::Component {
            component,
            positive,
            negative,
        } => {
            if !plot.flags.is_complex() {
                return Err(unsupported(format!(
                    "{name} asks for an AC component, which needs a complex plot; this analysis \
                     produced a real one (docs/port/OUTPUT_SELECTION.md)"
                )));
            }
            let terms =
                difference(plot, positive, negative.as_deref(), &name).map_err(unsupported)?;
            Ok(Column {
                name,
                unit: if matches!(
                    component,
                    VectorComponent::Magnitude | VectorComponent::Real | VectorComponent::Imaginary
                ) && negative.is_none()
                    && terms.len() == 1
                {
                    plot.variables[terms[0].0].unit.clone()
                } else {
                    component_unit(*component).to_owned()
                },
                is_real: true,
                terms,
                component: Some(*component),
            })
        }
    }
}

/// The unit of a computed AC component.
const fn component_unit(component: VectorComponent) -> &'static str {
    match component {
        VectorComponent::Magnitude | VectorComponent::Real | VectorComponent::Imaginary => {
            "voltage"
        }
        VectorComponent::Phase => "phase",
        VectorComponent::Decibels => "db",
    }
}

/// The signed sum of full-plot columns for `v(positive)` or
/// `v(positive,negative)`; the error is the diagnostic detail.
///
/// Ground (`0`) is not a column: `v(a,0)` is `v(a)` and `v(0,a)` is `-v(a)`,
/// exactly as C's `fixem()` rewrites them.
fn difference(
    plot: &Plot,
    positive: &str,
    negative: Option<&str>,
    name: &str,
) -> Result<Vec<(usize, Real)>, String> {
    let column_of = |node: &str| -> Result<Option<usize>, String> {
        if node == "0" {
            return Ok(None);
        }
        plot.variable_index(&format!("v({node})"))
            .or_else(|| plot.variable_index(node))
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "{name}: the analysis has no vector for node '{node}'; it carries {}",
                    column_names(plot)
                )
            })
    };
    let positive_column = column_of(positive)?;
    let negative_column = match negative {
        Some(negative) => column_of(negative)?,
        None => None,
    };
    let terms = match (positive_column, negative_column, negative) {
        (Some(positive), Some(negative), _) => vec![(positive, 1.0), (negative, -1.0)],
        (Some(positive), None, _) => vec![(positive, 1.0)],
        (None, Some(negative), _) => vec![(negative, -1.0)],
        // `v(0)` is ground, i.e. identically zero; the port's plots carry no
        // ground column because ground is 0 V by definition.
        (None, None, None) => Vec::new(),
        (None, None, Some(_)) => {
            return Err(format!(
                "{name}: both terminals are ground, so the difference is zero"
            ));
        }
    };
    Ok(terms)
}

/// The full plot's column names, for diagnostics.
fn column_names(plot: &Plot) -> String {
    if plot.variables.is_empty() {
        return "no vectors".to_owned();
    }
    plot.variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Selection, print_requests, write_requests};
    use crate::analysis::results::{Plot, PlotFlags, Variable};
    use crate::netlist::ast::{
        OutputCards, PrintCard, RequestedVector, SaveCard, VectorComponent, VectorRequest,
    };
    use crate::primitives::{AnalysisKind, Complex, SourceLoc};
    use std::path::PathBuf;

    /// A request as the parser would produce it, located on line 2.
    fn request(vector: RequestedVector) -> VectorRequest {
        VectorRequest {
            vector,
            location: SourceLoc::new(PathBuf::from("deck.cir"), 2, 7),
        }
    }

    fn voltage(positive: &str, negative: Option<&str>) -> VectorRequest {
        request(RequestedVector::Voltage {
            positive: positive.to_owned(),
            negative: negative.map(str::to_owned),
        })
    }

    fn all() -> VectorRequest {
        request(RequestedVector::All)
    }

    fn component(function: VectorComponent, positive: &str) -> VectorRequest {
        request(RequestedVector::Component {
            component: function,
            positive: positive.to_owned(),
            negative: None,
        })
    }

    fn current(device: &str) -> VectorRequest {
        request(RequestedVector::Current {
            device: device.to_owned(),
        })
    }

    /// An operating-point-like real plot: `v(in)`, `v(out)`, `i(v1)`.
    fn op_plot() -> Plot {
        let mut plot = Plot::new("op1", "Operating Point", PlotFlags::Real);
        plot.push_variable(Variable::new("v(in)", "voltage"));
        plot.push_variable(Variable::new("v(out)", "voltage"));
        plot.push_variable(Variable::new("i(v1)", "current"));
        plot.push_point(vec![
            Complex::real(5.0),
            Complex::real(2.5),
            Complex::real(-2.5e-3),
        ])
        .unwrap();
        plot
    }

    /// A transient-like real plot with a `time` scale.
    fn tran_plot() -> Plot {
        let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
        plot.push_variable(Variable::new("time", "time"));
        plot.push_variable(Variable::new("v(out)", "voltage"));
        for (time, out) in [(0.0, 0.0), (1e-6, 0.5), (2e-6, 1.0)] {
            plot.push_point(vec![Complex::real(time), Complex::real(out)])
                .unwrap();
        }
        plot
    }

    /// An AC-like complex plot: `frequency`, `v(out)`, `i(v1)`.
    fn ac_plot() -> Plot {
        let mut plot = Plot::new("ac1", "AC Analysis", PlotFlags::Complex);
        // The AC driver flags every column complex, including the scale.
        plot.push_variable(Variable::complex("frequency", "frequency"));
        plot.push_variable(Variable::complex("v(out)", "voltage"));
        plot.push_variable(Variable::complex("i(v1)", "current"));
        // 3, 4 -> magnitude 5, phase atan2(4, 3) = 0.9272952180016122 rad.
        plot.push_point(vec![
            Complex::new(100.0, 0.0),
            Complex::new(3.0, 4.0),
            Complex::new(-2e-3, -1e-3),
        ])
        .unwrap();
        plot
    }

    fn save(requests: Vec<VectorRequest>) -> SaveCard {
        SaveCard {
            requests,
            location: SourceLoc::new(PathBuf::from("deck.cir"), 3, 1),
        }
    }

    fn print(analysis: AnalysisKind, requests: Vec<VectorRequest>) -> PrintCard {
        PrintCard {
            analysis,
            analysis_location: SourceLoc::new(PathBuf::from("deck.cir"), 4, 8),
            requests,
            location: SourceLoc::new(PathBuf::from("deck.cir"), 4, 1),
        }
    }

    fn resolve(plot: &Plot, kind: AnalysisKind, requests: &[VectorRequest]) -> Selection {
        Selection::resolve(plot, kind, requests).expect("resolves")
    }

    fn names(selection: &Selection) -> Vec<String> {
        selection
            .variable_names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect()
    }

    #[test]
    fn applicable_requests_keep_deck_order_and_reject_a_wrong_analysis() {
        let cards = OutputCards {
            saves: vec![save(vec![voltage("in", None), voltage("out", None)])],
            prints: vec![print(AnalysisKind::OperatingPoint, vec![current("v1")])],
        };
        // C's `dbs` order: every `.save` request, then the `.print` requests.
        let requests = write_requests(&cards, AnalysisKind::OperatingPoint).unwrap();
        assert_eq!(
            requests.iter().map(|r| r.vector.name()).collect::<Vec<_>>(),
            ["v(in)", "v(out)", "i(v1)"]
        );
        // `.save` never prints, so the text destination sees only `.print`.
        let printed = print_requests(&cards, AnalysisKind::OperatingPoint).unwrap();
        assert_eq!(
            printed.iter().map(|r| r.vector.name()).collect::<Vec<_>>(),
            ["i(v1)"]
        );

        // A `.print` card for another analysis can never be honoured, so it is
        // an explicit failure and not a silently dropped request.
        let cards = OutputCards {
            saves: vec![save(vec![voltage("out", None)])],
            prints: vec![print(AnalysisKind::Ac, vec![voltage("out", None)])],
        };
        for error in [
            write_requests(&cards, AnalysisKind::OperatingPoint).unwrap_err(),
            print_requests(&cards, AnalysisKind::Transient).unwrap_err(),
        ] {
            assert!(!error.is_not_yet_ported(), "{error}");
            assert!(
                error.to_string().contains("names a different analysis"),
                "{error}"
            );
            assert!(error.to_string().contains("deck.cir:4:8"), "{error}");
        }
    }

    #[test]
    fn an_empty_or_all_request_set_writes_the_full_plot() {
        for requests in [Vec::new(), vec![request(RequestedVector::All)]] {
            let plot = op_plot();
            let selection = resolve(&plot, AnalysisKind::OperatingPoint, &requests);
            assert!(selection.is_full());
            // The written plot is the driver's, and the resolved columns describe
            // it: a full selection is not "no vectors" for the report or the table.
            assert_eq!(selection.variable_names(), ["v(in)", "v(out)", "i(v1)"]);
            assert_eq!(selection.apply(&plot).unwrap(), plot);
        }
        let cards = OutputCards {
            saves: vec![save(vec![request(RequestedVector::All)])],
            prints: vec![],
        };
        assert!(write_requests(&cards, AnalysisKind::Ac).unwrap().len() == 1);
    }

    #[test]
    fn an_operating_point_has_no_scale_and_a_sweep_keeps_one() {
        let op = op_plot();
        let selection = resolve(
            &op,
            AnalysisKind::OperatingPoint,
            &[voltage("out", None), current("v1")],
        );
        assert_eq!(names(&selection), ["v(out)", "i(v1)"]);
        let written = selection.apply(&op).unwrap();
        assert_eq!(written.variables[0].unit, "voltage");
        assert_eq!(written.point_count(), 1);
        assert_eq!(written.points[0][1], Complex::real(-2.5e-3));

        let tran = tran_plot();
        let selection = resolve(&tran, AnalysisKind::Transient, &[voltage("out", None)]);
        assert_eq!(names(&selection), ["time", "v(out)"]);
        let written = selection.apply(&tran).unwrap();
        assert_eq!(written.variables[0].unit, "time");
        assert_eq!(written.points.len(), 3);
        assert_eq!(written.points[2], [Complex::real(2e-6), Complex::real(1.0)]);

        let ac = ac_plot();
        let selection = resolve(&ac, AnalysisKind::Ac, &[voltage("out", None)]);
        assert_eq!(names(&selection), ["frequency", "v(out)"]);
    }

    #[test]
    fn a_voltage_difference_uses_both_columns_and_ground_is_zero() {
        let plot = op_plot();
        let selection = resolve(
            &plot,
            AnalysisKind::OperatingPoint,
            &[
                voltage("in", Some("out")),
                voltage("out", Some("0")),
                voltage("0", Some("out")),
                voltage("0", None),
            ],
        );
        assert_eq!(
            names(&selection),
            ["v(in,out)", "v(out,0)", "v(0,out)", "v(0)"]
        );
        let written = selection.apply(&plot).unwrap();
        assert_eq!(
            written.points[0],
            [
                Complex::real(2.5),
                Complex::real(2.5),
                Complex::real(-2.5),
                Complex::real(0.0),
            ]
        );
        // `v(out,0)` is the same vector as `v(out)`, so only the first survives.
        let selection = resolve(
            &plot,
            AnalysisKind::OperatingPoint,
            &[
                voltage("out", None),
                voltage("out", Some("0")),
                voltage("out", None),
            ],
        );
        assert_eq!(names(&selection), ["v(out)"]);
    }

    #[test]
    fn a_source_current_keeps_the_drivers_sign_convention() {
        let plot = op_plot();
        let selection = resolve(&plot, AnalysisKind::OperatingPoint, &[current("v1")]);
        assert_eq!(names(&selection), ["i(v1)"]);
        let written = selection.apply(&plot).unwrap();
        // C prints `i(v1)` as the current into the positive terminal: -2.5 mA
        // through a 5 V source feeding a 2 k divider.
        assert_eq!(written.points[0][0], Complex::real(-2.5e-3));
        assert_eq!(written.variables[0].unit, "current");
        assert!(written.variables[0].is_real);
    }

    #[test]
    fn an_unknown_vector_or_a_component_on_a_real_plot_is_explicit() {
        let plot = op_plot();
        let error = Selection::resolve(
            &plot,
            AnalysisKind::OperatingPoint,
            &[voltage("nope", None)],
        )
        .unwrap_err();
        assert!(!error.is_not_yet_ported(), "{error}");
        assert!(error.to_string().contains("deck.cir:2:7"), "{error}");
        assert!(error.to_string().contains("v(nope)"), "{error}");
        assert!(error.to_string().contains("v(in) v(out) i(v1)"), "{error}");
        let error = Selection::resolve(
            &plot,
            AnalysisKind::OperatingPoint,
            &[component(VectorComponent::Magnitude, "out")],
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("needs a complex plot"),
            "{error}"
        );
        let error =
            Selection::resolve(&plot, AnalysisKind::OperatingPoint, &[current("l9")]).unwrap_err();
        assert!(
            error.to_string().contains("no branch-current vector"),
            "{error}"
        );
    }

    #[test]
    fn a_complex_plot_resolves_every_ac_component() {
        let plot = ac_plot();
        let selection = resolve(
            &plot,
            AnalysisKind::Ac,
            &[
                component(VectorComponent::Magnitude, "out"),
                component(VectorComponent::Phase, "out"),
                component(VectorComponent::Real, "out"),
                component(VectorComponent::Imaginary, "out"),
                component(VectorComponent::Decibels, "out"),
            ],
        );
        assert_eq!(
            names(&selection),
            [
                "frequency",
                "vm(out)",
                "vp(out)",
                "vr(out)",
                "vi(out)",
                "vdb(out)"
            ]
        );
        let written = selection.apply(&plot).unwrap();
        assert_eq!(written.flags, PlotFlags::Complex);
        assert_eq!(written.variables[1].unit, "voltage");
        assert_eq!(written.variables[2].unit, "phase");
        assert_eq!(written.variables[5].unit, "db");
        // A computed component is a real vector inside a complex plot, which the
        // rawfile writes as `re,0.0`.
        assert!(written.variables[1].is_real);
        assert!(!written.variables[0].is_real);
        let point = &written.points[0];
        assert_eq!(point[1], Complex::real(5.0));
        assert!((point[2].re - 4.0f64.atan2(3.0)).abs() < 1e-15);
        assert_eq!(point[3], Complex::real(3.0));
        assert_eq!(point[4], Complex::real(4.0));
        assert_eq!(point[5], Complex::real(20.0 * 5.0f64.log10()));
    }

    #[test]
    fn a_non_finite_computed_value_is_an_error_not_a_written_infinity() {
        let mut plot = ac_plot();
        plot.points[0][1] = Complex::real(0.0);
        let selection = resolve(
            &plot,
            AnalysisKind::Ac,
            &[component(VectorComponent::Decibels, "out")],
        );
        let error = selection.apply(&plot).unwrap_err();
        assert!(error.to_string().contains("selection: vdb(out)"), "{error}");
        assert!(selection.to_text(&plot).is_err());
    }

    #[test]
    fn the_text_table_states_its_convention_and_prints_computed_components() {
        let real = op_plot();
        let selection = resolve(&real, AnalysisKind::OperatingPoint, &[current("v1")]);
        let text = selection.to_text(&real).unwrap();
        let expected = concat!(
            "print: 1 vector(s): i(v1)\n",
            "values: real with 15 fractional digits; computed from the plot, not the rawfile's own spelling\n",
            "  point  i(v1)\n",
            "      0  -2.500000000000000e-03\n",
        );
        assert_eq!(text, expected);
        let complex = ac_plot();
        let selection = resolve(
            &complex,
            AnalysisKind::Ac,
            &[
                voltage("out", None),
                component(VectorComponent::Magnitude, "out"),
            ],
        );
        let text = selection.to_text(&complex).unwrap();
        assert!(
            text.contains("print: 3 vector(s): frequency v(out) vm(out)"),
            "{text}"
        );
        assert!(
            text.contains("complex as `re,im` with 15 fractional digits"),
            "{text}"
        );
        assert!(
            text.contains("3.000000000000000e+00,4.000000000000000e+00"),
            "{text}"
        );
        assert!(
            text.contains("5.000000000000000e+00"),
            "vm is the magnitude: {text}"
        );
        // The scale column is part of the complex plot, so it is spelled
        // `re,im` as well, exactly as the rawfile writes it.
        assert!(
            text.contains("1.000000000000000e+02,0.000000000000000e+00"),
            "{text}"
        );
        // `vm` is real at every point, so the table prints one number while the
        // rawfile would spell the column `re,0.0`; the convention line says so.
        assert!(
            text.contains("real for every point prints as one number"),
            "{text}"
        );
    }

    #[test]
    fn a_full_selection_resolves_the_drivers_columns_and_still_validates_requests() {
        // `all` widens the written plot, but the resolved columns must describe
        // it: an empty column list made a successful run report "0 vector(s)".
        let plot = op_plot();
        let full = resolve(&plot, AnalysisKind::OperatingPoint, &[all()]);
        assert!(full.is_full());
        assert_eq!(names(&full), ["v(in)", "v(out)", "i(v1)"]);
        let written = full.apply(&plot).unwrap();
        assert_eq!(written.points, plot.points);
        assert_eq!(
            full.to_text(&plot).unwrap().lines().next().unwrap(),
            "print: 3 vector(s): v(in) v(out) i(v1)"
        );
        // A sweep keeps its scale in the resolved columns too.
        let sweep = ac_plot();
        let full = resolve(&sweep, AnalysisKind::Ac, &[all()]);
        assert_eq!(names(&full)[0], "frequency");
        // An unresolvable request is an error even next to `all`.
        let error = Selection::resolve(
            &plot,
            AnalysisKind::OperatingPoint,
            &[all(), voltage("nope", None)],
        )
        .unwrap_err();
        assert!(error.to_string().contains("v(nope)"), "{error}");
    }
}
