//! `.measure`/`.meas` evaluation over an accepted plot (GitHub #43).
//!
//! `.measure` is post-processing: after a run, the cards are evaluated against
//! the **full** plot the driver produced, before (and independently of) the
//! `.save`/`.print` selection that narrows what is written. An operand the
//! output selection dropped is still measurable, and a deck with a `.measure`
//! card writes exactly the rawfile it would have written without one.
//!
//! C: `get_measure2()` and its helpers in `src/frontend/com_measure2.c`,
//! driven by `do_measure()` (`src/frontend/measure.c`). The bounded grammar is
//! parsed by `netlist` (`crates/netlist/src/parser/measure.rs`); the
//! supported subset, the axis/interpolation/crossing/window rules and every
//! divergence from C are documented in `docs/port/MEASURE.md`.
//!
//! The layer is a pure function of a [`Plot`] plus the deck's
//! [`MeasureCard`]s, so it is unit-tested over hand-built plots with analytic
//! results.
//!
//! # Failure policy
//!
//! A measurement that cannot be computed is an error, never a fabricated
//! `NaN`/`0`:
//!
//! * [`SpiceError::Unsupported`], positioned at the card, when the request
//!   cannot be honoured by this plot (an operand the plot lacks, a window that
//!   covers no data or has no width, an `AT=` outside the axis, a crossing that
//!   does not occur, a discontinuity at an exact query point, a descending or
//!   nested axis);
//! * [`SpiceError::Numerical`] when the plot's own values do not permit the
//!   arithmetic (a non-finite operand value at a point the measurement uses, or
//!   a non-finite result).

use crate::netlist::ast::{
    MeasureCard, MeasureEvent, MeasureRequest, MeasureStatistic, MeasureTransition, MeasureWindow,
    VectorRequest,
};
use crate::primitives::{AnalysisKind, Real, SpiceError, SpiceResult};

use crate::analysis::results::Plot;
use crate::analysis::selection::{self, Column};

/// One named measurement result.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    /// The result name from the card, as written.
    pub name: String,
    /// The measured value.
    pub value: Real,
    /// The value's unit: the operand's unit for `FIND`/`MIN`/`MAX`/`AVG`/`RMS`
    /// (`voltage`, `current`, `phase`, `db`), `<operand>*<axis>` for `INTEG`,
    /// and the axis' unit (`time`, `frequency`) for a `TRIG`/`TARG` distance.
    pub unit: String,
    /// The axis value a single-position result sits at: the `AT=` query
    /// position for `FIND`, and the position of the extremum for `MIN`/`MAX`
    /// (C prints both as `at=`).
    pub at: Option<Real>,
    /// The axis window a whole-window statistic covered, as C echoes with
    /// `from=`/`to=`. The covered window is the requested window clipped to the
    /// axis, including the interpolated boundaries (see [`MeasureSpan`]).
    pub window: Option<MeasureSpan>,
    /// The trig and target axis positions of a `TRIG`/`TARG` measurement.
    pub events: Option<MeasureEvents>,
}

/// The axis window a whole-window statistic covered, in ascending order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasureSpan {
    /// The window's lower bound on the axis.
    pub from: Real,
    /// The window's upper bound on the axis.
    pub to: Real,
}

/// The two axis positions of a `TRIG`/`TARG` measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasureEvents {
    /// The trigger event's axis position.
    pub trig: Real,
    /// The target event's axis position.
    pub targ: Real,
}

/// Evaluates every card against `plot`, in card order.
///
/// Returns an empty list for a deck without a `.measure` card, so a caller that
/// prints only when the list is non-empty leaves today's output unchanged.
///
/// # Errors
///
/// * [`SpiceError::Unsupported`], positioned at the card, for a card naming
///   another analysis than `kind`, for a run whose plot has no measurement axis
///   (`op` and the analysis kinds without a driver), and for every measurement
///   that cannot be computed (see the module docs);
/// * [`SpiceError::Numerical`] for a non-finite operand value or result;
/// * whatever [`selection::resolve_request`] reports for an operand the plot
///   cannot resolve.
pub fn resolve(
    plot: &Plot,
    kind: AnalysisKind,
    cards: &[MeasureCard],
) -> SpiceResult<Vec<Measurement>> {
    if cards.is_empty() {
        return Ok(Vec::new());
    }
    let axis = Axis::of(plot, kind)?;
    let mut results = Vec::with_capacity(cards.len());
    for card in cards {
        if card.analysis != kind {
            return Err(unsupported(
                card,
                format!(
                    ".measure {} {}: the card names a .{} measurement; this plot is .{}, \
                     so the card can never be honoured against it",
                    card.analysis.as_str(),
                    card.name,
                    card.analysis.as_str(),
                    kind.as_str()
                ),
            ));
        }
        results.push(evaluate(plot, &axis, card)?);
    }
    Ok(results)
}

/// Renders results as the bounded text block the CLI appends to its report.
///
/// The shape follows C's `do_measure()` printout (`%-20s=  %.*e` plus the
/// operation's own fields), with the port's 15-fractional-digit spelling. The
/// `unit` of a [`Measurement`] is part of the API, not of this block, exactly
/// as in C.
#[must_use]
pub fn to_text(results: &[Measurement]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "measure: {} result(s), evaluated on the full plot before any .save/.print selection",
        results.len()
    );
    for result in results {
        let _ = write!(
            out,
            "{:<20}=  {}",
            result.name,
            crate::primitives::format_spice_number(result.value)
        );
        if let Some(events) = &result.events {
            let _ = write!(
                out,
                " targ=  {} trig=  {}",
                crate::primitives::format_spice_number(events.targ),
                crate::primitives::format_spice_number(events.trig)
            );
        }
        if let Some(at) = result.at {
            let _ = write!(out, " at=  {}", crate::primitives::format_spice_number(at));
        }
        if let Some(window) = &result.window {
            let _ = write!(
                out,
                " from=  {} to=  {}",
                crate::primitives::format_spice_number(window.from),
                crate::primitives::format_spice_number(window.to)
            );
        }
        let _ = writeln!(out);
    }
    out
}

/// A positioned failure for a request this plot cannot answer.
fn unsupported(card: &MeasureCard, feature: String) -> SpiceError {
    SpiceError::Unsupported {
        feature,
        location: Some(card.location.clone()),
    }
}

/// A non-finite value the measurement would have to use.
fn nonfinite(card: &MeasureCard, context: String, message: String) -> SpiceError {
    SpiceError::Numerical {
        context: format!("measure {}: {context}", card.name),
        message,
    }
}

/// The ordered physical axis of a plot: the driver's scale vector.
///
/// The companion and diffsol transient drivers keep every accepted time point
/// and land exactly on every source breakpoint, so consecutive samples never
/// straddle a source discontinuity (`crates/analysis/src/companion.rs`,
/// "Output"). This module therefore interpolates only between consecutive
/// samples, and treats two samples that share an axis value as a jump (see
/// [`value_at`] and [`crossing`]).
struct Axis {
    /// The axis' column name, e.g. `time`.
    name: &'static str,
    /// One physical value per plot point.
    values: Vec<Real>,
}

impl Axis {
    fn of(plot: &Plot, kind: AnalysisKind) -> SpiceResult<Self> {
        let name = match kind {
            AnalysisKind::Transient => "time",
            AnalysisKind::Ac | AnalysisKind::SParameter => "frequency",
            AnalysisKind::DcSweep => "sweep",
            other => {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "a .measure result needs a .tran, .dc or .ac plot with an ordered axis; \
                         this run is .{} (docs/port/MEASURE.md)",
                        other.as_str()
                    ),
                    location: None,
                });
            }
        };
        let Some(column) = plot.variable_index(name) else {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "the {} plot carries no '{name}' axis vector; it carries {}",
                    plot.plotname,
                    column_names(plot)
                ),
                location: None,
            });
        };
        let mut values = Vec::with_capacity(plot.points.len());
        for (index, point) in plot.points.iter().enumerate() {
            let Some(value) = point.get(column) else {
                return Err(SpiceError::Numerical {
                    context: format!("measure axis {name}"),
                    message: format!("point {index} has no {name} value"),
                });
            };
            if !value.is_finite() {
                return Err(SpiceError::Numerical {
                    context: format!("measure axis {name}"),
                    message: format!("point {index} is not finite"),
                });
            }
            values.push(value.re);
        }
        if let Some(index) = values.windows(2).position(|window| window[1] < window[0]) {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "the {name} axis is not non-decreasing (it descends at point {}); a \
                     measurement needs an ordered axis, so a descending or nested .dc sweep is \
                     not measurable",
                    index + 1
                ),
                location: None,
            });
        }
        Ok(Self { name, values })
    }

    fn first(&self) -> Real {
        self.values.first().copied().unwrap_or(0.0)
    }

    fn last(&self) -> Real {
        self.values.last().copied().unwrap_or(0.0)
    }
}

/// The requested window, clipped to the axis and validated.
#[derive(Debug, Clone, Copy)]
struct Window {
    from: Real,
    to: Real,
}

impl Window {
    fn resolve(window: &MeasureWindow, axis: &Axis, card: &MeasureCard) -> SpiceResult<Self> {
        if axis.values.is_empty() {
            return Err(unsupported(
                card,
                format!(".measure {}: the plot has no points", card.name),
            ));
        }
        let from = window.from.unwrap_or_else(|| axis.first());
        let to = window.to.unwrap_or_else(|| axis.last());
        if from > to {
            return Err(unsupported(
                card,
                format!(
                    ".measure {}: FROM= is above TO= (a {} window must ascend; C swaps an \
                     inverted window for a .dc measurement)",
                    card.name, axis.name
                ),
            ));
        }
        if to < axis.first() || from > axis.last() {
            return Err(unsupported(
                card,
                format!(
                    ".measure {}: the window [{}, {}] lies outside the {} range [{}, {}]",
                    card.name,
                    crate::primitives::format_spice_number(from),
                    crate::primitives::format_spice_number(to),
                    axis.name,
                    crate::primitives::format_spice_number(axis.first()),
                    crate::primitives::format_spice_number(axis.last())
                ),
            ));
        }
        Ok(Self {
            from: from.max(axis.first()),
            to: to.min(axis.last()),
        })
    }

    /// The index range of the samples inside the window, inclusive.
    fn samples(&self, axis: &Axis, card: &MeasureCard) -> SpiceResult<(usize, usize)> {
        let first = axis.values.partition_point(|value| *value < self.from);
        let last = axis.values.partition_point(|value| *value <= self.to);
        if first >= last {
            return Err(unsupported(
                card,
                format!(
                    ".measure {}: the window [{}, {}] covers no {} sample",
                    card.name,
                    crate::primitives::format_spice_number(self.from),
                    crate::primitives::format_spice_number(self.to),
                    axis.name
                ),
            ));
        }
        Ok((first, last - 1))
    }

    /// The window's samples, with both boundaries interpolated exactly.
    ///
    /// A boundary strictly inside a bracket is interpolated, as C's
    /// Enhancement-302 does, so `AVG` over `[from, to]` equals
    /// `INTEG/(to - from)` exactly. The bracket always has positive width here:
    /// a zero-width bracket is a jump and its value *is* the boundary.
    fn clipped(
        &self,
        axis: &Axis,
        operand: &Operand<'_>,
        card: &MeasureCard,
    ) -> SpiceResult<Vec<(Real, Real)>> {
        let (first, last) = self.samples(axis, card)?;
        let mut points = Vec::with_capacity(last - first + 3);
        if axis.values[first] > self.from {
            points.push((
                self.from,
                interpolate_value(
                    axis.values[first - 1],
                    operand.value(first - 1)?,
                    axis.values[first],
                    operand.value(first)?,
                    self.from,
                ),
            ));
        }
        for index in first..=last {
            points.push((axis.values[index], operand.value(index)?));
        }
        if axis.values[last] < self.to {
            points.push((
                self.to,
                interpolate_value(
                    axis.values[last],
                    operand.value(last)?,
                    axis.values[last + 1],
                    operand.value(last + 1)?,
                    self.to,
                ),
            ));
        }
        Ok(points)
    }
}

/// One measurement operand: the column a `.measure` vector resolves to, plus the
/// result name, so that a value the measurement cannot use is reported as a
/// measurement failure rather than as an output-selection one.
struct Operand<'a> {
    plot: &'a Plot,
    name: &'a str,
    column: Column,
}

impl<'a> Operand<'a> {
    fn resolve(plot: &'a Plot, request: &VectorRequest, name: &'a str) -> SpiceResult<Self> {
        Ok(Self {
            plot,
            name,
            column: selection::resolve_request(plot, request)?,
        })
    }

    /// The operand's unit, e.g. `voltage`.
    fn unit(&self) -> &str {
        self.column.unit()
    }

    /// The operand's value at one plot point.
    fn value(&self, index: usize) -> SpiceResult<Real> {
        let Some(point) = self.plot.points.get(index) else {
            return Err(SpiceError::Numerical {
                context: format!("measure {}", self.name),
                message: format!("the plot has no point {index}"),
            });
        };
        self.column
            .value(point)
            .map_err(|error| match error {
                SpiceError::Numerical { message, .. } => SpiceError::Numerical {
                    context: format!("measure {}: {}", self.name, self.column.name()),
                    message,
                },
                other => other,
            })
            .map(|value| value.re)
    }
}

fn evaluate(plot: &Plot, axis: &Axis, card: &MeasureCard) -> SpiceResult<Measurement> {
    match &card.request {
        MeasureRequest::Find {
            operand,
            at,
            window,
            ..
        } => {
            let operand = Operand::resolve(plot, operand, &card.name)?;
            let window = Window::resolve(window, axis, card)?;
            check_range(axis, *at, card, "AT=")?;
            if *at < window.from || *at > window.to {
                return Err(unsupported(
                    card,
                    format!(
                        ".measure {}: AT={} is outside the measurement window [{}, {}]",
                        card.name,
                        crate::primitives::format_spice_number(*at),
                        crate::primitives::format_spice_number(window.from),
                        crate::primitives::format_spice_number(window.to)
                    ),
                ));
            }
            Ok(Measurement {
                name: card.name.clone(),
                value: value_at(axis, &operand, *at, card)?,
                unit: operand.unit().to_owned(),
                at: Some(*at),
                window: None,
                events: None,
            })
        }
        MeasureRequest::Statistic {
            statistic,
            operand,
            window,
        } => {
            let operand = Operand::resolve(plot, operand, &card.name)?;
            let window = Window::resolve(window, axis, card)?;
            let (value, at, span) = match statistic {
                MeasureStatistic::Min | MeasureStatistic::Max => {
                    let (value, at) = extremum(
                        axis,
                        &operand,
                        &window,
                        *statistic == MeasureStatistic::Max,
                        card,
                    )?;
                    (value, Some(at), None)
                }
                MeasureStatistic::Avg | MeasureStatistic::Rms | MeasureStatistic::Integ => {
                    let span = MeasureSpan {
                        from: window.from,
                        to: window.to,
                    };
                    // Every one of these three is an integral over the covered
                    // window; a window without width has no mean and is a
                    // failure rather than a division by zero.
                    let width = span_width(span, card)?;
                    let points = window.clipped(axis, &operand, card)?;
                    let squared = *statistic == MeasureStatistic::Rms;
                    let area = integral(&points, squared, card)?;
                    let mean = if *statistic == MeasureStatistic::Integ {
                        area
                    } else {
                        area / width
                    };
                    let value = if squared { mean.sqrt() } else { mean };
                    (value, None, Some(span))
                }
            };
            if !value.is_finite() {
                return Err(nonfinite(
                    card,
                    statistic.name().to_owned(),
                    "the computed result is not finite".to_owned(),
                ));
            }
            Ok(Measurement {
                name: card.name.clone(),
                value,
                unit: statistic_unit(statistic, &operand, axis),
                at,
                window: span,
                events: None,
            })
        }
        MeasureRequest::TrigTarg { trig, targ, window } => {
            let window = Window::resolve(window, axis, card)?;
            let trig = event_at(plot, axis, trig, &window, card)?;
            let targ = event_at(plot, axis, targ, &window, card)?;
            let value = targ - trig;
            if !value.is_finite() {
                return Err(nonfinite(
                    card,
                    "TRIG/TARG".to_owned(),
                    "the computed distance is not finite".to_owned(),
                ));
            }
            Ok(Measurement {
                name: card.name.clone(),
                value,
                unit: axis.name.to_owned(),
                at: None,
                window: None,
                events: Some(MeasureEvents { trig, targ }),
            })
        }
    }
}

/// The operand's value at one physical axis value.
///
/// Interpolation is linear between the samples bracketing `x`; the bracket has
/// positive width, so it never spans a source discontinuity (the drivers keep a
/// sample on every breakpoint). An axis value carried by two samples is a jump:
/// asking for its value is ambiguous, so it is an explicit failure rather than
/// a silent choice between the left and right limits (C instead divides by a
/// zero span and reports "out of interval").
fn value_at(axis: &Axis, operand: &Operand<'_>, x: Real, card: &MeasureCard) -> SpiceResult<Real> {
    check_range(axis, x, card, "AT=")?;
    let index = axis.values.partition_point(|value| *value < x);
    if axis.values[index] == x {
        if index + 1 < axis.values.len()
            && axis.values[index + 1] == x
            && operand.value(index + 1)? != operand.value(index)?
        {
            return Err(unsupported(
                card,
                format!(
                    ".measure {}: the operand is discontinuous at {} = {}: the plot has two \
                     values at that {} value, so the value there is not defined",
                    card.name,
                    axis.name,
                    crate::primitives::format_spice_number(x),
                    axis.name
                ),
            ));
        }
        return operand.value(index);
    }
    Ok(interpolate_value(
        axis.values[index - 1],
        operand.value(index - 1)?,
        axis.values[index],
        operand.value(index)?,
        x,
    ))
}

/// Rejects an axis value outside the plot's own range.
fn check_range(axis: &Axis, x: Real, card: &MeasureCard, what: &str) -> SpiceResult<()> {
    if axis.values.is_empty() {
        return Err(unsupported(
            card,
            format!(".measure {}: the plot has no points", card.name),
        ));
    }
    if x < axis.first() || x > axis.last() {
        return Err(unsupported(
            card,
            format!(
                ".measure {}: {what}{} is outside the {} range [{}, {}] (out of interval)",
                card.name,
                crate::primitives::format_spice_number(x),
                axis.name,
                crate::primitives::format_spice_number(axis.first()),
                crate::primitives::format_spice_number(axis.last())
            ),
        ));
    }
    Ok(())
}

/// The axis value of the window's smallest (or largest) operand sample.
fn extremum(
    axis: &Axis,
    operand: &Operand<'_>,
    window: &Window,
    largest: bool,
    card: &MeasureCard,
) -> SpiceResult<(Real, Real)> {
    let (first, last) = window.samples(axis, card)?;
    let mut best: Option<(Real, Real)> = None;
    for index in first..=last {
        let value = operand.value(index)?;
        let replace = match best {
            None => true,
            // C's `value <= mValue` / `value >= mValue`: ties keep the last
            // sample that attained the extremum.
            Some((current, _)) => {
                if largest {
                    value >= current
                } else {
                    value <= current
                }
            }
        };
        if replace {
            best = Some((value, axis.values[index]));
        }
    }
    best.ok_or_else(|| {
        unsupported(
            card,
            format!(".measure {}: the window covers no sample", card.name),
        )
    })
}

/// The trapezoidal integral of the window's samples, or of their squares.
///
/// The trapezoid rule integrates linear data exactly and is the rule C's
/// `measure_minMaxAvg()` uses for `AVG`; the port uses it for `INTEG` and `RMS`
/// as well, where C mixes in Simpson's rules **only** for uniformly spaced
/// samples (`AlmostEqualUlps(width[i], width[i+1], 100)` in
/// `measure_rms_integral()`), which an adaptive transient grid essentially
/// never satisfies.
fn integral(points: &[(Real, Real)], square: bool, card: &MeasureCard) -> SpiceResult<Real> {
    let mut area = 0.0;
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        let (y0, y1) = if square { (y0 * y0, y1 * y1) } else { (y0, y1) };
        area += 0.5 * (y0 + y1) * (x1 - x0);
    }
    if !area.is_finite() {
        return Err(nonfinite(
            card,
            "INTEG".to_owned(),
            "the integrated value is not finite".to_owned(),
        ));
    }
    Ok(area)
}

/// The width of a covered window, which a mean needs to be defined.
fn span_width(span: MeasureSpan, card: &MeasureCard) -> SpiceResult<Real> {
    let width = span.to - span.from;
    if width <= 0.0 || !width.is_finite() {
        return Err(unsupported(
            card,
            format!(
                ".measure {}: the window [{}, {}] has no width, so the mean is undefined",
                card.name,
                crate::primitives::format_spice_number(span.from),
                crate::primitives::format_spice_number(span.to)
            ),
        ));
    }
    Ok(width)
}

/// The axis position of one `TRIG`/`TARG` event.
fn event_at(
    plot: &Plot,
    axis: &Axis,
    event: &MeasureEvent,
    window: &Window,
    card: &MeasureCard,
) -> SpiceResult<Real> {
    match event {
        MeasureEvent::At { at, .. } => {
            check_range(axis, *at, card, "the event AT=")?;
            if *at < window.from || *at > window.to {
                return Err(unsupported(
                    card,
                    format!(
                        ".measure {}: the event AT={} is outside the measurement window [{}, {}]",
                        card.name,
                        crate::primitives::format_spice_number(*at),
                        crate::primitives::format_spice_number(window.from),
                        crate::primitives::format_spice_number(window.to)
                    ),
                ));
            }
            Ok(*at)
        }
        MeasureEvent::Crossing {
            operand,
            value,
            transition,
            ..
        } => crossing(plot, axis, operand, *value, *transition, window, card),
    }
}

/// The axis position where an operand crosses a threshold.
///
/// The scan starts at the last sample at or before the window's lower bound, so
/// the side the operand is on when it enters the window is known, and it ends at
/// the last sample inside the window. The side is `value >= threshold` (the high
/// side), as C's `com_measure_when()` section test; a transition from one side
/// to the other is one crossing, in the direction the operand moved. The
/// crossing's axis position is interpolated between the pair, or, when the pair
/// shares an axis value (a jump), that axis value exactly — never a division by
/// a zero span.
fn crossing(
    plot: &Plot,
    axis: &Axis,
    request: &VectorRequest,
    value: Real,
    transition: MeasureTransition,
    window: &Window,
    card: &MeasureCard,
) -> SpiceResult<Real> {
    let operand = Operand::resolve(plot, request, &card.name)?;
    let (first, last) = window.samples(axis, card)?;
    let start = first.saturating_sub(1);
    let mut crossings: Vec<(Real, bool)> = Vec::new();
    for index in start + 1..=last {
        let previous = index - 1;
        let before = operand.value(previous)?;
        let after = operand.value(index)?;
        let before_high = before >= value;
        let after_high = after >= value;
        if before_high == after_high {
            continue;
        }
        let x = if axis.values[index] == axis.values[previous] {
            axis.values[index]
        } else {
            interpolate_axis(
                axis.values[previous],
                before,
                axis.values[index],
                after,
                value,
            )
        };
        crossings.push((x, after_high));
    }
    let selected = match transition {
        MeasureTransition::First => crossings.first(),
        MeasureTransition::Last => crossings.last(),
        MeasureTransition::Rise(n) => crossings
            .iter()
            .filter(|(_, rising)| *rising)
            .nth(n as usize - 1),
        MeasureTransition::Fall(n) => crossings
            .iter()
            .filter(|(_, rising)| !*rising)
            .nth(n as usize - 1),
        MeasureTransition::Cross(n) => crossings.get(n as usize - 1),
    };
    selected.map(|(x, _)| *x).ok_or_else(|| {
        unsupported(
            card,
            format!(
                ".measure {}: no {} crossing of {} through {} found from the window's \
                 lower bound [{}, {}]",
                card.name,
                transition.name(),
                request.vector.name(),
                crate::primitives::format_spice_number(value),
                crate::primitives::format_spice_number(window.from),
                crate::primitives::format_spice_number(window.to)
            ),
        )
    })
}

/// The unit of a statistic's result.
fn statistic_unit(statistic: &MeasureStatistic, operand: &Operand<'_>, axis: &Axis) -> String {
    match statistic {
        MeasureStatistic::Integ => format!("{}*{}", operand.unit(), axis.name),
        _ => operand.unit().to_owned(),
    }
}

/// The value of the line through `(x0, y0)` and `(x1, y1)` at `x`.
///
/// Only used with `x` inside the bracket, whose width is positive (`clipped()`),
/// so there is never a division by a zero span.
fn interpolate_value(x0: Real, y0: Real, x1: Real, y1: Real, x: Real) -> Real {
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

/// The axis value at which the line through `(x0, y0)` and `(x1, y1)` reaches
/// `y`, i.e. the crossing position.
///
/// The caller guarantees `y0 != y1` (the two samples are on opposite sides of
/// the threshold) and a positive bracket width, so there is never a division by
/// a zero span.
fn interpolate_axis(x0: Real, y0: Real, x1: Real, y1: Real, y: Real) -> Real {
    x0 + (x1 - x0) * (y - y0) / (y1 - y0)
}

/// The plot's column names, for diagnostics.
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
    use super::{MeasureSpan, Measurement, resolve, to_text};
    use crate::analysis::results::{Plot, PlotFlags, Variable};
    use crate::netlist::ast::{
        MeasureCard, MeasureEvent, MeasureRequest, MeasureStatistic, MeasureTransition,
        MeasureWindow, RequestedVector, VectorRequest,
    };
    use crate::primitives::{AnalysisKind, Complex, Real, SourceLoc};
    use std::path::PathBuf;

    fn loc() -> SourceLoc {
        SourceLoc::new(PathBuf::from("deck.cir"), 2, 1)
    }

    /// A real sweep plot: `time` then `v(out)`, from `(time, value)` rows.
    fn tran(rows: &[(Real, Real)]) -> Plot {
        let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
        plot.push_variable(Variable::new("time", "time"));
        plot.push_variable(Variable::new("v(out)", "voltage"));
        for (time, value) in rows {
            plot.push_point(vec![Complex::real(*time), Complex::real(*value)])
                .unwrap();
        }
        plot
    }

    /// `(time, value)` rows of an evenly spaced sweep.
    fn sweep(
        from: Real,
        to: Real,
        steps: usize,
        value: impl Fn(Real) -> Real,
    ) -> Vec<(Real, Real)> {
        (0..=steps)
            .map(|step| {
                let time = from + (to - from) * (step as Real) / (steps as Real);
                (time, value(time))
            })
            .collect()
    }

    fn request(
        statistic: MeasureStatistic,
        from: Option<Real>,
        to: Option<Real>,
    ) -> MeasureRequest {
        MeasureRequest::Statistic {
            statistic,
            operand: out(),
            window: MeasureWindow { from, to },
        }
    }

    fn out() -> VectorRequest {
        VectorRequest {
            vector: RequestedVector::Voltage {
                positive: "out".to_owned(),
                negative: None,
            },
            location: loc(),
        }
    }

    fn card(name: &str, request: MeasureRequest) -> MeasureCard {
        MeasureCard {
            analysis: AnalysisKind::Transient,
            analysis_location: loc(),
            name: name.to_owned(),
            name_location: loc(),
            request,
            location: loc(),
        }
    }

    fn measure(plot: &Plot, name: &str, request: MeasureRequest) -> Measurement {
        let mut results = resolve(plot, AnalysisKind::Transient, &[card(name, request)])
            .expect("the measurement resolves");
        assert_eq!(results.len(), 1);
        results.remove(0)
    }

    fn failure(plot: &Plot, name: &str, request: MeasureRequest) -> crate::primitives::SpiceError {
        resolve(plot, AnalysisKind::Transient, &[card(name, request)])
            .expect_err("the measurement fails")
    }

    fn crossing(value: Real, transition: MeasureTransition) -> MeasureEvent {
        MeasureEvent::Crossing {
            operand: out(),
            value,
            value_location: loc(),
            transition,
        }
    }

    #[test]
    fn a_ramp_has_analytic_statistics_and_boundaries_are_interpolated() {
        // v(t) = 2t on [0, 1], sampled every 1 ms. AVG/INTEG are exact (the
        // trapezoid rule integrates linear data exactly); RMS integrates v^2,
        // which is quadratic, so the trapezoid rule leaves its known O(h^2)
        // error of (h^2/12) * 8 = 6.7e-7.
        let plot = tran(&sweep(0.0, 1.0, 1000, |time| 2.0 * time));
        let whole = |statistic| measure(&plot, "m", request(statistic, None, None));
        assert_eq!(whole(MeasureStatistic::Min).value, 0.0);
        assert_eq!(whole(MeasureStatistic::Max).value, 2.0);
        assert!((whole(MeasureStatistic::Avg).value - 1.0).abs() < 1e-15);
        assert!((whole(MeasureStatistic::Integ).value - 1.0).abs() < 1e-15);
        assert!((whole(MeasureStatistic::Rms).value - (4.0f64 / 3.0).sqrt()).abs() < 1e-6);

        // [0.25, 0.75] is bounded by samples here, so MIN/MAX see them whole.
        let window = |statistic| measure(&plot, "m", request(statistic, Some(0.25), Some(0.75)));
        assert!((window(MeasureStatistic::Avg).value - 1.0).abs() < 1e-15);
        assert!((window(MeasureStatistic::Integ).value - 0.5).abs() < 1e-15);
        let mean_square = 8.0 * (0.75f64.powi(3) - 0.25f64.powi(3)) / 3.0;
        assert!((window(MeasureStatistic::Rms).value - mean_square.sqrt()).abs() < 1e-6);
        assert_eq!(window(MeasureStatistic::Min).value, 0.5);
        assert_eq!(window(MeasureStatistic::Max).value, 1.5);
        assert_eq!(
            window(MeasureStatistic::Avg).window,
            Some(MeasureSpan {
                from: 0.25,
                to: 0.75
            })
        );
        // MIN/MAX report where they found the extremum (C's `at=`).
        assert_eq!(window(MeasureStatistic::Min).at, Some(0.25));
        assert_eq!(window(MeasureStatistic::Max).at, Some(0.75));
        assert_eq!(window(MeasureStatistic::Min).unit, "voltage");
    }

    #[test]
    fn a_window_boundary_off_the_samples_is_interpolated_for_the_means() {
        // v(t) = 2t sampled every 0.1, window [0.25, 0.75]: neither boundary is a
        // sample, so AVG/INTEG/RMS are only exact because the boundaries are
        // interpolated.
        let plot = tran(&sweep(0.0, 1.0, 10, |time| 2.0 * time));
        let window = |statistic| measure(&plot, "m", request(statistic, Some(0.25), Some(0.75)));
        assert!((window(MeasureStatistic::Avg).value - 1.0).abs() < 1e-15);
        assert!((window(MeasureStatistic::Integ).value - 0.5).abs() < 1e-15);
        // The trapezoid rule's own error on v^2 (h = 0.1) is ~7e-3, so the exact
        // mean square is only approached to that order.
        let mean_square = 8.0 * (0.75f64.powi(3) - 0.25f64.powi(3)) / 3.0;
        assert!((window(MeasureStatistic::Rms).value - mean_square.sqrt()).abs() < 0.01);
        // MIN/MAX keep whole samples (C's semantics), so they see 0.3 .. 0.7.
        assert_eq!(window(MeasureStatistic::Min).value, 0.6);
        assert_eq!(window(MeasureStatistic::Max).value, 1.4);
        assert_eq!(window(MeasureStatistic::Min).at, Some(0.3));
    }

    #[test]
    fn a_constant_is_its_own_mean_rms_and_area() {
        let plot = tran(&sweep(0.0, 1.0, 4, |_| 3.0));
        for statistic in [MeasureStatistic::Min, MeasureStatistic::Max] {
            let result = measure(&plot, "m", request(statistic, None, None));
            assert_eq!(result.value, 3.0);
        }
        assert_eq!(
            measure(&plot, "m", request(MeasureStatistic::Avg, None, None)).value,
            3.0
        );
        assert_eq!(
            measure(&plot, "m", request(MeasureStatistic::Rms, None, None)).value,
            3.0
        );
        assert_eq!(
            measure(&plot, "m", request(MeasureStatistic::Integ, None, None)).value,
            3.0
        );
        // An integral's unit is the operand's times the axis'.
        let integral = measure(&plot, "m", request(MeasureStatistic::Integ, None, None));
        assert_eq!(integral.unit, "voltage*time");
        assert_eq!(integral.window.map(|span| span.to), Some(1.0));
    }

    #[test]
    fn a_min_max_tie_keeps_the_last_sample_like_c() {
        // A flat top: C's `value <= mValue` / `value >= mValue` update on ties.
        let plot = tran(&[(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0)]);
        assert_eq!(
            measure(&plot, "m", request(MeasureStatistic::Max, None, None)).at,
            Some(2.0)
        );
        assert_eq!(
            measure(&plot, "m", request(MeasureStatistic::Min, None, None)).at,
            Some(3.0)
        );
    }

    #[test]
    fn a_sinusoid_has_analytic_crossings_and_rms() {
        let plot = tran(&sweep(0.0, 2.0, 2000, |time| {
            (2.0 * std::f64::consts::PI * time).sin()
        }));
        // Crossing -0.5: rising at 7/12, falling at 11/12.
        let event = |transition| MeasureRequest::TrigTarg {
            trig: crossing(-0.5, transition),
            targ: crossing(-0.5, MeasureTransition::Rise(2)),
            window: MeasureWindow::default(),
        };
        // -0.5 is crossed falling at 7/12 and rising at 11/12, one period apart.
        let result = measure(&plot, "delay", event(MeasureTransition::Fall(1)));
        assert!((result.events.unwrap().trig - 7.0 / 12.0).abs() < 1e-6);
        let result = measure(&plot, "delay", event(MeasureTransition::Rise(1)));
        let events = result.events.unwrap();
        assert!((events.trig - 11.0 / 12.0).abs() < 1e-6);
        assert!((events.targ - (11.0 / 12.0 + 1.0)).abs() < 1e-6);
        assert!((result.value - 1.0).abs() < 1e-6);
        assert_eq!(result.unit, "time");

        // sin >= 0 at t = 0, so the first transition is the fall through 0 at
        // 0.5 and the first rise is at 1.0, exactly as C's section test reads it.
        let first = MeasureRequest::TrigTarg {
            trig: crossing(0.0, MeasureTransition::First),
            targ: crossing(0.0, MeasureTransition::Rise(1)),
            window: MeasureWindow::default(),
        };
        let result = measure(&plot, "delay", first);
        let events = result.events.unwrap();
        assert!((events.trig - 0.5).abs() < 1e-6);
        assert!((events.targ - 1.0).abs() < 1e-6);

        // RMS of a full period of sin is 1/sqrt(2); the trapezoid rule leaves an
        // O(dt^2) error.
        let rms = measure(&plot, "rms", request(MeasureStatistic::Rms, None, None));
        assert!((rms.value - 0.5f64.sqrt()).abs() < 1e-6, "{}", rms.value);
        // ... and the mean over a period is zero.
        let avg = measure(
            &plot,
            "avg",
            request(MeasureStatistic::Avg, Some(0.0), Some(1.0)),
        );
        assert!(avg.value.abs() < 1e-6, "{}", avg.value);
    }

    #[test]
    fn last_takes_the_last_crossing_and_counts_select_the_nth() {
        let plot = tran(&sweep(0.0, 4.0, 400, |time| {
            (std::f64::consts::PI * time).sin()
        }));
        // sin(pi t) is sampled over [0, 4]: its transitions are the fall at
        // t ~ 1, the rise at t ~ 2 and the fall at t ~ 3 (at t = 1, 2, 3 the
        // sample is an ulp off zero and belongs to the side it approaches).
        let event = |transition| MeasureRequest::TrigTarg {
            trig: crossing(0.0, transition),
            targ: MeasureEvent::At {
                at: 0.0,
                location: loc(),
            },
            window: MeasureWindow::default(),
        };
        let trig_of = |transition| {
            let result = measure(&plot, "delay", event(transition));
            result.events.unwrap().trig
        };
        assert!((trig_of(MeasureTransition::Cross(1)) - 1.0).abs() < 1e-2);
        assert!((trig_of(MeasureTransition::Cross(2)) - 2.0).abs() < 1e-2);
        assert!((trig_of(MeasureTransition::Cross(3)) - 3.0).abs() < 1e-2);
        assert!((trig_of(MeasureTransition::Last) - 3.0).abs() < 1e-2);
        assert!((trig_of(MeasureTransition::Rise(1)) - 2.0).abs() < 1e-2);
        assert!((trig_of(MeasureTransition::Fall(2)) - 3.0).abs() < 1e-2);
        // There is no second rising crossing, so asking for one fails explicitly.
        let error = failure(&plot, "delay", event(MeasureTransition::Rise(2)));
        assert!(error.to_string().contains("no RISE=2 crossing"), "{error}");
    }

    #[test]
    fn find_at_interpolates_the_bracket_at_and_inside_a_window() {
        let plot = tran(&sweep(0.0, 1.0, 4, |time| 2.0 * time));
        let find = |at, from, to| MeasureRequest::Find {
            operand: out(),
            at,
            at_location: loc(),
            window: MeasureWindow { from, to },
        };
        let result = measure(&plot, "vat", find(0.35, None, None));
        assert!((result.value - 0.7).abs() < 1e-15);
        assert_eq!(result.at, Some(0.35));
        assert_eq!(result.unit, "voltage");
        // A sample position is read exactly.
        assert_eq!(measure(&plot, "vat", find(0.5, None, None)).value, 1.0);
        // A query inside the window works; one outside it does not.
        assert_eq!(
            measure(&plot, "vat", find(0.5, Some(0.25), Some(0.75))).value,
            1.0
        );
        let error = failure(&plot, "vat", find(0.8, Some(0.25), Some(0.75)));
        assert!(
            error.to_string().contains("outside the measurement window"),
            "{error}"
        );
    }

    #[test]
    fn queries_outside_the_axis_and_windows_without_data_are_explicit_failures() {
        let plot = tran(&sweep(0.0, 1.0, 4, |time| 2.0 * time));
        let find = |at| MeasureRequest::Find {
            operand: out(),
            at,
            at_location: loc(),
            window: MeasureWindow::default(),
        };
        for at in [-1.0, 1.5] {
            let error = failure(&plot, "vat", find(at));
            assert!(!error.is_not_yet_ported(), "{error}");
            assert!(error.to_string().contains("out of interval"), "{error}");
            assert!(error.to_string().contains("time range"), "{error}");
            assert!(error.to_string().contains("deck.cir:2:1"), "{error}");
        }
        // A window wholly outside the axis, and an inverted one.
        let error = failure(
            &plot,
            "m",
            request(MeasureStatistic::Avg, Some(5.0), Some(6.0)),
        );
        assert!(error.to_string().contains("lies outside"), "{error}");
        let error = failure(
            &plot,
            "m",
            request(MeasureStatistic::Avg, Some(0.8), Some(0.2)),
        );
        assert!(error.to_string().contains("above TO="), "{error}");
        // A window that only reaches past the axis is clipped, not rejected: a
        // transient's stop time is a sample, so this is the requested window
        // intersected with the data.
        let clipped = measure(
            &plot,
            "m",
            request(MeasureStatistic::Integ, Some(0.0), Some(5.0)),
        );
        assert_eq!(clipped.value, 1.0);
        assert_eq!(clipped.window.unwrap().to, 1.0);
    }

    #[test]
    fn an_empty_or_zero_width_window_is_never_a_fabricated_mean() {
        let plot = tran(&sweep(0.0, 1.0, 4, |time| 2.0 * time));
        for statistic in [
            MeasureStatistic::Avg,
            MeasureStatistic::Rms,
            MeasureStatistic::Integ,
        ] {
            let error = failure(&plot, "m", request(statistic, Some(0.5), Some(0.5)));
            assert!(error.to_string().contains("has no width"), "{error}");
        }
        // A plot with no points cannot be measured at all.
        let empty = tran(&[]);
        let error = failure(&empty, "m", request(MeasureStatistic::Avg, None, None));
        assert!(error.to_string().contains("no points"), "{error}");
    }

    #[test]
    fn a_duplicate_time_is_a_jump_with_a_defined_crossing_but_no_defined_value() {
        // t = 1 carries both the left limit 1 and the right limit 10: the plot
        // records a source jump the way the drivers never do.
        let plot = tran(&[(0.0, 0.0), (1.0, 1.0), (1.0, 10.0), (2.0, 12.0)]);
        // The jump crosses 5 exactly at t = 1, with no interpolation across it.
        let crossing = MeasureEvent::Crossing {
            operand: out(),
            value: 5.0,
            value_location: loc(),
            transition: MeasureTransition::Rise(1),
        };
        let result = measure(
            &plot,
            "delay",
            MeasureRequest::TrigTarg {
                trig: crossing,
                targ: MeasureEvent::At {
                    at: 0.0,
                    location: loc(),
                },
                window: MeasureWindow::default(),
            },
        );
        assert_eq!(result.events.unwrap().trig, 1.0);
        // The operand's value at the jump is not defined...
        let error = failure(
            &plot,
            "vat",
            MeasureRequest::Find {
                operand: out(),
                at: 1.0,
                at_location: loc(),
                window: MeasureWindow::default(),
            },
        );
        assert!(error.to_string().contains("discontinuous"), "{error}");
        // ... while a duplicated time with the same value is harmless.
        let smooth = tran(&[(0.0, 0.0), (1.0, 1.0), (1.0, 1.0), (2.0, 2.0)]);
        let value = measure(
            &smooth,
            "vat",
            MeasureRequest::Find {
                operand: out(),
                at: 1.0,
                at_location: loc(),
                window: MeasureWindow::default(),
            },
        );
        assert_eq!(value.value, 1.0);
        // Interpolation still works inside the continuous brackets around it.
        assert_eq!(
            measure(
                &plot,
                "vat",
                MeasureRequest::Find {
                    operand: out(),
                    at: 0.5,
                    at_location: loc(),
                    window: MeasureWindow::default(),
                },
            )
            .value,
            0.5
        );
    }

    #[test]
    fn a_nonfinite_operand_or_result_is_numerical_not_a_written_value() {
        let plot = tran(&[(0.0, 0.0), (1.0, Real::NAN), (2.0, 2.0)]);
        let error = failure(&plot, "m", request(MeasureStatistic::Max, None, None));
        assert!(
            matches!(error, crate::primitives::SpiceError::Numerical { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("measure m: v(out)"), "{error}");
        assert!(error.to_string().contains("not finite"), "{error}");

        // An operand that is finite everywhere still cannot be integrated when
        // squaring overflows.
        let huge = tran(&[(0.0, 0.0), (1.0, 1e200)]);
        let error = failure(&huge, "m", request(MeasureStatistic::Rms, None, None));
        assert!(error.to_string().contains("not finite"), "{error}");
    }

    #[test]
    fn a_missing_axis_operand_or_analysis_is_an_explicit_failure() {
        let plot = tran(&sweep(0.0, 1.0, 2, |time| 2.0 * time));
        // An operand the plot does not carry.
        let missing = MeasureRequest::Statistic {
            statistic: MeasureStatistic::Max,
            operand: VectorRequest {
                vector: RequestedVector::Voltage {
                    positive: "nosuch".to_owned(),
                    negative: None,
                },
                location: loc(),
            },
            window: MeasureWindow::default(),
        };
        let error = failure(&plot, "m", missing);
        assert!(error.to_string().contains("v(nosuch)"), "{error}");

        // An axis the plot does not carry.
        let no_time = Plot::new("op1", "Operating Point", PlotFlags::Real);
        let error = resolve(
            &plot,
            AnalysisKind::Ac,
            &[card("m", request(MeasureStatistic::Max, None, None))],
        )
        .expect_err("wrong axis");
        assert!(error.to_string().contains("frequency"), "{error}");
        let error = resolve(
            &no_time,
            AnalysisKind::OperatingPoint,
            &[card("m", request(MeasureStatistic::Max, None, None))],
        )
        .expect_err("no axis");
        assert!(error.to_string().contains("ordered axis"), "{error}");

        // A card naming another analysis than the run.
        let mut other = card("m", request(MeasureStatistic::Max, None, None));
        other.analysis = AnalysisKind::Ac;
        let error = resolve(&plot, AnalysisKind::Transient, &[other]).expect_err("wrong analysis");
        assert!(error.to_string().contains("names a .ac"), "{error}");

        // A crossing that does not occur.
        let error = failure(
            &plot,
            "delay",
            MeasureRequest::TrigTarg {
                trig: crossing(5.0, MeasureTransition::Rise(1)),
                targ: crossing(5.0, MeasureTransition::Fall(1)),
                window: MeasureWindow::default(),
            },
        );
        assert!(error.to_string().contains("no RISE=1 crossing"), "{error}");
        assert!(error.to_string().contains("v(out)"), "{error}");
    }

    #[test]
    fn the_axis_must_ascend_and_a_complex_plot_measures_its_real_part() {
        let descending = tran(&[(1.0, 0.0), (0.5, 1.0), (0.0, 2.0)]);
        let error = failure(&descending, "m", request(MeasureStatistic::Max, None, None));
        assert!(error.to_string().contains("not non-decreasing"), "{error}");
        assert!(error.to_string().contains("descends at point 1"), "{error}");

        // An AC plot: the frequency axis and the real part of a complex vector,
        // exactly as C's `get_value()` defaults.
        let mut ac = Plot::new("ac1", "AC Analysis", PlotFlags::Complex);
        ac.push_variable(Variable::complex("frequency", "frequency"));
        ac.push_variable(Variable::complex("v(out)", "voltage"));
        for frequency in [100.0, 200.0, 400.0] {
            ac.push_point(vec![
                Complex::real(frequency),
                Complex::new(frequency / 100.0, -1.0),
            ])
            .unwrap();
        }
        let mut ac_card = card("vm", request(MeasureStatistic::Max, None, None));
        ac_card.analysis = AnalysisKind::Ac;
        let result =
            resolve(&ac, AnalysisKind::Ac, &[ac_card]).expect("the AC measurement resolves");
        assert_eq!(result[0].value, 4.0);
        assert_eq!(result[0].unit, "voltage");
    }

    #[test]
    fn a_deck_without_measure_cards_produces_no_results_and_no_failure() {
        // The CLI relies on this: a plot with no measurement axis (`op`) and no
        // `.measure` card must not fail.
        let op = Plot::new("op1", "Operating Point", PlotFlags::Real);
        assert_eq!(
            resolve(&op, AnalysisKind::OperatingPoint, &[]).unwrap(),
            Vec::new()
        );
        assert_eq!(
            to_text(&[]),
            "measure: 0 result(s), evaluated on the full plot before any .save/.print selection\n"
        );
    }

    #[test]
    fn duplicate_result_names_are_kept_in_card_order() {
        let plot = tran(&sweep(0.0, 1.0, 4, |time| 2.0 * time));
        let results = resolve(
            &plot,
            AnalysisKind::Transient,
            &[
                card("m", request(MeasureStatistic::Min, None, None)),
                card("m", request(MeasureStatistic::Max, None, None)),
            ],
        )
        .expect("both cards resolve");
        assert_eq!(
            results
                .iter()
                .map(|result| (result.name.as_str(), result.value))
                .collect::<Vec<_>>(),
            [("m", 0.0), ("m", 2.0)],
            "a repeated name is two results, not one overwritten result"
        );
    }

    #[test]
    fn the_text_block_follows_c_with_the_ports_spelling() {
        let plot = tran(&sweep(0.0, 1.0, 4, |time| 2.0 * time));
        let results = resolve(
            &plot,
            AnalysisKind::Transient,
            &[
                card("vmax", request(MeasureStatistic::Max, None, None)),
                card("vavg", request(MeasureStatistic::Avg, None, None)),
                card(
                    "delay",
                    MeasureRequest::TrigTarg {
                        trig: crossing(1.0, MeasureTransition::Rise(1)),
                        targ: crossing(2.0, MeasureTransition::Rise(1)),
                        window: MeasureWindow::default(),
                    },
                ),
            ],
        )
        .expect("the measurements resolve");
        let text = to_text(&results);
        let expected = concat!(
            "measure: 3 result(s), evaluated on the full plot before any .save/.print selection\n",
            "vmax                =  2.000000000000000e+00 at=  1.000000000000000e+00\n",
            "vavg                =  1.000000000000000e+00 from=  0.000000000000000e+00 to=  1.000000000000000e+00\n",
            "delay               =  5.000000000000000e-01 targ=  1.000000000000000e+00 trig=  5.000000000000000e-01\n",
        );
        assert_eq!(text, expected);
    }
}
