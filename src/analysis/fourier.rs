//! `.four` Fourier/THD evaluation over the final complete period of a
//! transient plot (GitHub #44).
//!
//! `.four` is post-processing: after a run, the deck's cards are evaluated
//! against the **full** plot the driver produced, before (and independently of)
//! the `.save`/`.print` selection that narrows what is written, exactly as
//! `.measure` is (`crates/analysis/src/measure.rs`, `docs/port/MEASURE.md`).
//! A vector the output selection dropped is still transformed, and a deck with a
//! `.four` card writes exactly the rawfile it would have written without one.
//!
//! C: `fourier()` and `CKTfour()` in `src/frontend/fourier.c`, reached from
//! `ft_cktcoms()` (`src/frontend/dotcards.c`) for each `.four` line of the deck.
//! The bounded grammar is parsed by `netlist`
//! (`crates/netlist/src/parser/fourier.rs`); the supported subset, the
//! window/resampling/quadrature/normalization rules and every divergence from C
//! are documented in `docs/port/FOURIER.md`.
//!
//! The layer is a pure function of a [`Plot`] plus the deck's [`FourierCard`]s,
//! so it is unit-tested over hand-built plots with analytic results.
//!
//! # The window
//!
//! Every vector is transformed over the **final complete period** on the
//! physical time axis: `[to - 1/fundamental, to]`, where `to` is the plot's last
//! time sample (C: `dp[1] = ft_minmax(time, TRUE)[1]`, `dp[0] = dp[1] - d` with
//! `d = nperiods/fundamental` and `nperiods = 1`). A run shorter than one period
//! is refused, never partially transformed, and the endpoint `to` is included as
//! the window's upper grid point rather than dropped (C's half-open grid).
//!
//! # Resampling and quadrature
//!
//! The window is resampled onto a **uniform closed grid** of `divisions + 1`
//! points, `t_i = from + i/fundamental/divisions` for `i = 0..=divisions` with
//! `divisions = 4 * max(harmonics, 50)`; the adaptive steps of the transient
//! driver are never treated as a uniform grid, and no FFT is used. Each grid
//! point is read with `.measure`'s sample model: linear interpolation inside the
//! two samples bracketing it, and an explicit failure — never a silent limit
//! choice — when it lands on a time carried by two samples with different values
//! (an unrepresented discontinuity, `docs/port/MEASURE.md`). The complex
//! amplitudes are then the trapezoid quadrature of the trace against
//! `sin(k*2*pi*w)` and `cos(k*2*pi*w)` on that grid (`w = i/divisions` is the
//! point's phase within the period by construction). The discrete projection
//! is exact for band-limited grid values below `divisions/2` cycles per period;
//! interpolating the original trace can still introduce error.
//!
//! # Normalization and phase
//!
//! For each `k` in `1..=harmonics` the two coefficients
//! `A_k = 2/divisions * sum(w_i * y_i * sin(2*pi*k*w_i))` and
//! `B_k = 2/divisions * sum(w_i * y_i * cos(2*pi*k*w_i))` (trapezoid weights
//! `w_i = 1/2` at both ends, `1` inside) give the **single-sided peak
//! amplitude** `sqrt(A_k^2 + B_k^2)` and the **phase** `atan2(B_k, A_k)` in
//! **radians** in `[-pi, pi]`, i.e. the trace's component is
//! `amplitude * sin(k * 2*pi*fundamental * (t - window.from) + phase)`
//! in the vector's unit. Phase is referenced to the window's start, not time zero.
//! Phase `0` is a pure sine relative to that start, `±pi/2` a pure cosine; C prints the same phase in
//! **degrees** (`atan2(cosine, sine) * 180/pi`), so a comparison with C converts
//! once, explicitly. `dc` is the mean of the resampled trace over the window
//! (`sum(w_i * y_i)/divisions`, C's row `0`), and the **total harmonic
//! distortion** is the fraction
//! `sqrt(sum_{k>=2} (amplitude_k / amplitude_1)^2)` — harmonics `2..=harmonics`
//! relative to the fundamental, exactly C's `thd = 100*sqrt(...)` sum without
//! the percent factor.
//!
//! # Failure policy
//!
//! A Fourier result that cannot be computed is an error, never a fabricated
//! `NaN`/`0`/clamped value:
//!
//! * [`SpiceError::Unsupported`], positioned at the card, for a card in a run
//!   that is not `.tran`, a plot without a `time` axis or with fewer than two
//!   points, a descending axis, a run shorter than one period, a window with too
//!   few samples for the requested harmonics, a grid point on an unrepresented
//!   discontinuity, and a card whose harmonic count is outside this port's
//!   bounded resampling budget (a hand-built card can carry one the parser would
//!   have rejected);
//! * [`SpiceError::Numerical`] when the plot's own values do not permit the
//!   arithmetic: a non-finite axis value, a non-finite operand value at a grid
//!   point, a non-finite result, or a zero fundamental amplitude, which leaves
//!   the THD undefined;
//! * whatever [`selection::resolve_request`] reports for a vector the plot
//!   cannot resolve (an unknown node, a branch current the analysis has no
//!   vector for).

use crate::netlist::ast::{FourierCard, MAX_HARMONICS, VectorRequest};
use crate::primitives::{AnalysisKind, Real, SpiceError, SpiceResult};

use crate::analysis::results::Plot;
use crate::analysis::selection::{self, Column};

/// The grid's harmonic-count floor: `4 * 50 = 200` subintervals per period,
/// matching C's default `fourgridsize`. Higher counts retain four subintervals
/// per requested harmonic, with the same maximum of 400 subintervals.
const GRID_FLOOR: u32 = 50;

/// How many ULPs of the quadrature's own sums a fundamental amplitude must
/// exceed before a total harmonic distortion can be formed from it.
///
/// The sums accumulate at most `4 * MAX_HARMONICS + 1 = 401` terms, so a trace
/// with no fundamental content leaves an amplitude of a few `f64::EPSILON`
/// times the trace's own magnitude there; a THD formed from that residue would
/// be pure noise (C prints whatever it computes). This is the arithmetic's
/// resolution, not a physical tolerance.
const NOISE_ULPS: Real = 16.0;

/// One tabulated harmonic of a Fourier analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Harmonic {
    /// The harmonic index `k`, `1..=harmonics`. C's row number.
    pub order: u32,
    /// `k * fundamental`, in hertz.
    pub frequency: Real,
    /// The single-sided peak amplitude `sqrt(A_k^2 + B_k^2)` in the vector's
    /// unit: the trace contributes `amplitude * sin(k*omega*(t-window.from) + phase)`.
    pub amplitude: Real,
    /// The phase in **radians**, `atan2(B_k, A_k)`, in `[-pi, pi]`: `0` for a
    /// pure sine relative to `window.from`, `+pi/2` for a pure cosine.
    /// C uses the same window reference but prints degrees.
    pub phase: Real,
}

/// The physical window one analysis transformed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FourierWindow {
    /// The lower bound on the time axis: `to - 1/fundamental`, in seconds.
    pub from: Real,
    /// The upper bound: the plot's last time sample, in seconds.
    pub to: Real,
}

/// The result of one `.four` request for one vector.
#[derive(Debug, Clone, PartialEq)]
pub struct FourierAnalysis {
    /// The vector's canonical name, e.g. `v(out)`, `v(in,out)`, `i(v1)`.
    pub vector: String,
    /// The vector's unit: `voltage` or `current`.
    pub unit: String,
    /// The fundamental frequency the card named, in hertz.
    pub fundamental: Real,
    /// The mean of the resampled trace over the window, in the vector's unit.
    pub dc: Real,
    /// The harmonics `1..=harmonics`, in ascending order.
    pub harmonics: Vec<Harmonic>,
    /// Total harmonic distortion as a **fraction**:
    /// `sqrt(sum_{k>=2} (amplitude_k/amplitude_1)^2)`. C prints `100 * thd` %.
    pub thd: Real,
    /// The window that was transformed.
    pub window: FourierWindow,
    /// The number of grid subintervals the period was resampled onto.
    pub divisions: usize,
    /// The number of plot samples the window covered (the samples at or after
    /// `window.from`, including the closing sample).
    pub samples: usize,
}

/// Evaluates every card against `plot`, in card order, one result per vector.
///
/// Returns an empty list for a deck without a `.four` card, so a caller that
/// prints only when the list is non-empty leaves today's output unchanged.
///
/// # Errors
///
/// * [`SpiceError::Unsupported`], positioned at the card, for a card that names
///   another analysis than `kind` (C selects the `tran` plot; a multi-analysis
///   deck routes `.four` to its last `.tran` plot through
///   [`crate::analysis::batch::resolve_outputs`]), for every condition listed in the module's
///   failure policy, and for a harmonic count outside this port's work budget;
/// * [`SpiceError::Numerical`] for a non-finite axis, operand or result value,
///   or a zero fundamental amplitude;
/// * whatever [`selection::resolve_request`] reports for a vector the plot
///   cannot resolve.
pub fn resolve(
    plot: &Plot,
    kind: AnalysisKind,
    cards: &[FourierCard],
) -> SpiceResult<Vec<FourierAnalysis>> {
    if cards.is_empty() {
        return Ok(Vec::new());
    }
    let mut results = Vec::new();
    for card in cards {
        if card.frontend_command {
            results.extend(resolve_with_settings(
                plot,
                kind,
                std::slice::from_ref(card),
                FourierSettings::default(),
            )?);
            continue;
        }
        if kind != AnalysisKind::Transient {
            return Err(unsupported(
                card,
                format!(
                    ".four: the card transforms a .tran result (C selects the tran plot); this plot \
                     is .{}, so the card can never be honoured against it",
                    kind.as_str()
                ),
            ));
        }
        check_budget(card)?;
        let axis = TimeAxis::of(plot, card)?;
        for vector in &card.vectors {
            results.push(evaluate(plot, &axis, card, vector)?);
        }
    }
    Ok(results)
}

/// Evaluates Fourier fundamental expressions against the deck's parameter scope.
/// Rust extension: the reference C front end rejects braced `.four` frequencies.
///
/// # Errors
/// Parameter evaluation failures and nonpositive fundamental frequencies.
pub fn resolve_cards(
    cards: &[FourierCard],
    scope: &crate::netlist::eval::ParamScope,
) -> SpiceResult<Vec<FourierCard>> {
    let mut budget = crate::netlist::eval::EvalBudget::default();
    cards
        .iter()
        .map(|card| {
            let mut out = card.clone();
            if let Some(expression) = &card.fundamental_expression {
                out.fundamental = scope.evaluate(expression, &mut budget)?;
                out.fundamental_expression = None;
            }
            check_budget(&out)?;
            Ok(out)
        })
        .collect()
}

/// Renders results as the bounded text block the CLI appends to its report.
///
/// The shape follows C's `fourier()` printout (`Fourier analysis for <vec>:`
/// plus the harmonic table), with the port's 15-fractional-digit spelling, the
/// phase in radians, and the THD both as a fraction and in percent. The `unit`
/// of a [`FourierAnalysis`] is part of the API, not of this block, exactly as
/// for `.measure`.
#[must_use]
pub fn to_text(results: &[FourierAnalysis]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "four: {} analysis(es) of the final complete period, evaluated on the full plot before \
         any .save/.print selection",
        results.len()
    );
    for result in results {
        let _ = writeln!(out, "Fourier analysis for {}:", result.vector);
        let _ = writeln!(
            out,
            "  fundamental         =  {} Hz",
            crate::primitives::format_spice_number(result.fundamental)
        );
        let _ = writeln!(
            out,
            "  window              =  [{} , {}] s",
            crate::primitives::format_spice_number(result.window.from),
            crate::primitives::format_spice_number(result.window.to)
        );
        let _ = writeln!(
            out,
            "  grid                =  {} subinterval(s) per period, {} sample(s) in the period",
            result.divisions, result.samples
        );
        let _ = writeln!(
            out,
            "  dc                  =  {}",
            crate::primitives::format_spice_number(result.dc)
        );
        let _ = writeln!(
            out,
            "  thd                 =  {} ({} %)",
            crate::primitives::format_spice_number(result.thd),
            crate::primitives::format_spice_number(100.0 * result.thd)
        );
        let _ = writeln!(
            out,
            "  harmonic  frequency                magnitude                phase (rad)"
        );
        for harmonic in &result.harmonics {
            let _ = writeln!(
                out,
                "  {:<8}  {:<24} {:<24} {}",
                harmonic.order,
                crate::primitives::format_spice_number(harmonic.frequency),
                crate::primitives::format_spice_number(harmonic.amplitude),
                crate::primitives::format_spice_number(harmonic.phase)
            );
        }
    }
    out
}

/// A positioned failure for a request this plot cannot answer.
fn unsupported(card: &FourierCard, feature: String) -> SpiceError {
    SpiceError::Unsupported {
        feature,
        location: Some(card.location.clone()),
    }
}

/// A value the transform would have to use but cannot.
fn nonfinite(context: String, message: String) -> SpiceError {
    SpiceError::Numerical {
        context: format!(".four {context}"),
        message,
    }
}

/// The port's bounded harmonic count, enforced where the grid is built as well
/// as in the grammar, so a hand-built card cannot ask for unbounded work.
fn check_budget(card: &FourierCard) -> SpiceResult<()> {
    if card.fundamental_expression.is_some() {
        return Err(unsupported(card, "unresolved .four fundamental expression; call resolve_cards with the deck parameter scope".into()));
    }
    if !(card.fundamental.is_finite() && card.fundamental > 0.0) {
        return Err(unsupported(
            card,
            format!(
                ".four: the fundamental frequency must be a finite value greater than zero (found \
                 {})",
                crate::primitives::format_spice_number(card.fundamental)
            ),
        ));
    }
    if card.harmonics == 0 || card.harmonics > MAX_HARMONICS {
        return Err(unsupported(
            card,
            format!(
                ".four: HARMONICS={} is outside this port's bounded resampling budget of 1..={} \
                 harmonics ({} grid subintervals per vector); C has no such bound \
                 (docs/port/FOURIER.md)",
                card.harmonics,
                MAX_HARMONICS,
                4 * MAX_HARMONICS
            ),
        ));
    }
    Ok(())
}

/// The ordered physical time axis of a plot: the driver's scale vector.
///
/// The plot holds every accepted time point and lands exactly on every source
/// breakpoint, so consecutive samples never straddle a source discontinuity
/// (`crates/analysis/src/companion.rs`, "Output"). This module therefore
/// interpolates only between consecutive samples, and treats two samples that
/// share an axis value as a jump (see [`sample`]).
struct TimeAxis {
    /// One physical time per plot point.
    values: Vec<Real>,
}

impl TimeAxis {
    fn of(plot: &Plot, card: &FourierCard) -> SpiceResult<Self> {
        let Some(column) = plot.variable_index("time") else {
            return Err(unsupported(
                card,
                format!(
                    ".four: the {} plot carries no 'time' axis vector; it carries {}",
                    plot.plotname,
                    column_names(plot)
                ),
            ));
        };
        if plot.points.len() < 2 {
            return Err(unsupported(
                card,
                format!(
                    ".four: a Fourier transform needs at least 2 data points (the plot has {}); C \
                     reports the same refusal",
                    plot.points.len()
                ),
            ));
        }
        let mut values = Vec::with_capacity(plot.points.len());
        for (index, point) in plot.points.iter().enumerate() {
            let Some(value) = point.get(column) else {
                return Err(nonfinite(
                    "time".to_owned(),
                    format!("point {index} has no time value"),
                ));
            };
            if !value.is_finite() {
                return Err(nonfinite(
                    "time".to_owned(),
                    format!("point {index} is not finite"),
                ));
            }
            values.push(value.re);
        }
        if let Some(index) = values.windows(2).position(|window| window[1] < window[0]) {
            return Err(unsupported(
                card,
                format!(
                    ".four: the time axis is not non-decreasing (it descends at point {}); a \
                     Fourier transform needs an ordered axis",
                    index + 1
                ),
            ));
        }
        Ok(Self { values })
    }
}

/// One transformed vector: the column a `.four` request resolves to, plus the
/// plot it reads, so a value the transform cannot use is reported as a Fourier
/// failure rather than as an output-selection one.
struct Trace<'a> {
    plot: &'a Plot,
    column: Column,
}

impl<'a> Trace<'a> {
    fn resolve(plot: &'a Plot, request: &VectorRequest) -> SpiceResult<Self> {
        Ok(Self {
            plot,
            column: selection::resolve_request(plot, request)?,
        })
    }

    /// The vector's canonical name, e.g. `v(out)`.
    fn name(&self) -> &str {
        self.column.name()
    }

    /// The vector's unit, e.g. `voltage`.
    fn unit(&self) -> &str {
        self.column.unit()
    }

    /// The vector's value at one plot point, as a real.
    fn value(&self, index: usize) -> SpiceResult<Real> {
        let Some(point) = self.plot.points.get(index) else {
            return Err(nonfinite(
                self.name().to_owned(),
                format!("the plot has no point {index}"),
            ));
        };
        self.column
            .value(point)
            .map_err(|error| match error {
                SpiceError::Numerical { message, .. } => {
                    nonfinite(format!("{}: {}", self.name(), self.column.name()), message)
                }
                other => other,
            })
            .map(|value| value.re)
    }
}

/// One `.four` request, transformed over the final complete period of `axis`.
fn evaluate(
    plot: &Plot,
    axis: &TimeAxis,
    card: &FourierCard,
    request: &VectorRequest,
) -> SpiceResult<FourierAnalysis> {
    let trace = Trace::resolve(plot, request)?;
    let first = axis.values.first().copied().unwrap_or(0.0);
    let to = axis.values.last().copied().unwrap_or(0.0);
    let period = 1.0 / card.fundamental;
    let from = to - period;
    if from < first {
        return Err(unsupported(
            card,
            format!(
                ".four {}: the run spans {} s (from {} to {}), shorter than one period ({} s) of \
                 the {} Hz fundamental; C reports the same refusal",
                trace.name(),
                crate::primitives::format_spice_number(to - first),
                crate::primitives::format_spice_number(first),
                crate::primitives::format_spice_number(to),
                crate::primitives::format_spice_number(period),
                crate::primitives::format_spice_number(card.fundamental)
            ),
        ));
    }
    let harmonics = card.harmonics as usize;
    let first_index = axis.values.partition_point(|value| *value < from);
    let samples = axis.values.len() - first_index;
    let needed = 2 * harmonics + 1;
    if samples < needed {
        return Err(unsupported(
            card,
            format!(
                ".four {}: the final period [{}, {}] covers only {} sample(s); {} harmonics need \
                 at least {} (two per cycle of the highest harmonic, plus the endpoint) to be \
                 well posed",
                trace.name(),
                crate::primitives::format_spice_number(from),
                crate::primitives::format_spice_number(to),
                samples,
                harmonics,
                needed
            ),
        ));
    }
    let divisions = 4 * card.harmonics.max(GRID_FLOOR) as usize;
    let step = period / divisions as Real;
    let mut values = Vec::with_capacity(divisions + 1);
    for i in 0..=divisions {
        // The closing point is the window's upper bound exactly, so the grid
        // ends on the plot's own last sample instead of on accumulated rounding.
        let x = if i == divisions {
            to
        } else {
            from + (i as Real) * step
        };
        values.push(sample(axis, &trace, x, card)?);
    }

    let mut sine = vec![0.0; harmonics + 1];
    let mut cosine = vec![0.0; harmonics + 1];
    for (i, y) in values.iter().enumerate() {
        let weight = if i == 0 || i == divisions { 0.5 } else { 1.0 };
        let weighted = weight * y;
        let phase = i as Real / divisions as Real;
        for k in 0..=harmonics {
            let angle = std::f64::consts::TAU * (k as Real) * phase;
            sine[k] += weighted * angle.sin();
            cosine[k] += weighted * angle.cos();
        }
    }
    let scale = 2.0 / divisions as Real;
    let dc = cosine[0] / divisions as Real;
    // The trace's own magnitude, from the same sums: the noise floor the
    // fundamental amplitude is compared against below.
    let magnitude = values.iter().enumerate().fold(0.0, |sum, (i, y)| {
        let weight = if i == 0 || i == divisions { 0.5 } else { 1.0 };
        sum + weight * y.abs()
    }) / divisions as Real;
    let mut tabulated = Vec::with_capacity(harmonics);
    for (k, (sine, cosine)) in sine.iter().zip(&cosine).enumerate().skip(1) {
        let amplitude = (scale * sine).hypot(scale * cosine);
        let phase = (scale * cosine).atan2(scale * sine);
        if !amplitude.is_finite() || !phase.is_finite() {
            return Err(nonfinite(
                trace.name().to_owned(),
                format!("harmonic {k} is not finite"),
            ));
        }
        tabulated.push(Harmonic {
            order: u32::try_from(k).unwrap_or(u32::MAX),
            frequency: k as Real * card.fundamental,
            amplitude,
            phase,
        });
    }
    let fundamental_amplitude = tabulated.first().map_or(0.0, |harmonic| harmonic.amplitude);
    // Amplitudes were checked finite above. At or below the quadrature's
    // rounding noise, THD has no denominator. An overflowed magnitude also
    // refuses the transform instead of producing a spurious denominator.
    if fundamental_amplitude <= NOISE_ULPS * f64::EPSILON * magnitude {
        return Err(nonfinite(
            trace.name().to_owned(),
            format!(
                "the fundamental amplitude ({}) is at or below the quadrature's own rounding \
                 noise, so the total harmonic distortion has no denominator",
                crate::primitives::format_spice_number(fundamental_amplitude)
            ),
        ));
    }
    let thd = tabulated
        .iter()
        .skip(1)
        .map(|harmonic| (harmonic.amplitude / fundamental_amplitude).powi(2))
        .sum::<Real>()
        .sqrt();
    if !dc.is_finite() || !thd.is_finite() {
        return Err(nonfinite(
            trace.name().to_owned(),
            "the DC component or the total harmonic distortion is not finite".to_owned(),
        ));
    }
    Ok(FourierAnalysis {
        vector: trace.name().to_owned(),
        unit: trace.unit().to_owned(),
        fundamental: card.fundamental,
        dc,
        harmonics: tabulated,
        thd,
        window: FourierWindow { from, to },
        divisions,
        samples,
    })
}

/// The vector's value at one physical time, with `.measure`'s sample model.
///
/// Interpolation is linear between the samples bracketing `x`, never across a
/// source discontinuity (the drivers keep a sample on every breakpoint). A time
/// carried by two samples with different values is a jump: its value is
/// ambiguous, so it is an explicit failure rather than a silent choice between
/// the left and right limits. A window boundary that falls strictly inside a
/// bracket is interpolated from the bracketing samples, which may lie just
/// outside the window.
fn sample(axis: &TimeAxis, trace: &Trace<'_>, x: Real, card: &FourierCard) -> SpiceResult<Real> {
    let index = axis.values.partition_point(|value| *value < x);
    if axis.values[index] == x {
        let left = trace.value(index)?;
        if index + 1 < axis.values.len()
            && axis.values[index + 1] == x
            && trace.value(index + 1)? != left
        {
            return Err(unsupported(
                card,
                format!(
                    ".four {}: the vector is discontinuous at time = {}: the plot has two values \
                     at that time, so the resampling grid cannot read it there",
                    trace.name(),
                    crate::primitives::format_spice_number(x)
                ),
            ));
        }
        return Ok(left);
    }
    // `x` is strictly inside the bracket `index - 1 .. index`: `x >= from` is at
    // or after the first sample, so the bracket exists and has positive width
    // (a repeated value would have moved `index` back).
    let x0 = axis.values[index - 1];
    let y0 = trace.value(index - 1)?;
    let x1 = axis.values[index];
    let y1 = trace.value(index)?;
    Ok(y0 + (y1 - y0) * (x - x0) / (x1 - x0))
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
    use super::{FourierAnalysis, resolve, to_text};
    use crate::analysis::results::{Plot, PlotFlags, Variable};
    use crate::netlist::ast::{FourierCard, MAX_HARMONICS, RequestedVector, VectorRequest};
    use crate::primitives::{AnalysisKind, Complex, Real, SourceLoc, SpiceError};
    use std::f64::consts::{PI, TAU};
    use std::path::PathBuf;

    /// The default card's period: 1 kHz, with 200 grid subintervals of 5e-6 s.
    const FUNDAMENTAL: Real = 1.0e3;
    const PERIOD: Real = 1.0e-3;
    const DIVISIONS: usize = 200;

    fn loc() -> SourceLoc {
        SourceLoc::new(PathBuf::from("deck.cir"), 2, 1)
    }

    /// A real transient plot: `time`, `v(out)` and `i(v1)`, from rows.
    fn tran(rows: &[(Real, Real)]) -> Plot {
        let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
        plot.push_variable(Variable::new("time", "time"));
        plot.push_variable(Variable::new("v(out)", "voltage"));
        plot.push_variable(Variable::new("i(v1)", "current"));
        for (time, value) in rows {
            plot.push_point(vec![
                Complex::real(*time),
                Complex::real(*value),
                Complex::real(-*value),
            ])
            .unwrap();
        }
        plot
    }

    /// An evenly spaced sweep of `steps` intervals.
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

    /// The samples the resampling grid itself lands on: an exact-grid trace, so
    /// the quadrature's own error is the only one left.
    fn on_grid(value: impl Fn(Real) -> Real) -> Vec<(Real, Real)> {
        let step = PERIOD / DIVISIONS as Real;
        (0..=DIVISIONS)
            .map(|i| {
                let time = (i as Real) * step;
                (time, value(time))
            })
            .collect()
    }

    fn sine(amplitude: Real, harmonic: u32, phase: Real) -> impl Fn(Real) -> Real {
        move |time: Real| amplitude * (TAU * (harmonic as Real) * FUNDAMENTAL * time + phase).sin()
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

    fn current() -> VectorRequest {
        VectorRequest {
            vector: RequestedVector::Current {
                device: "v1".to_owned(),
            },
            location: loc(),
        }
    }

    fn card(vectors: Vec<VectorRequest>, harmonics: u32) -> FourierCard {
        FourierCard {
            frontend_command: false,
            fundamental_expression: None,
            fundamental: FUNDAMENTAL,
            fundamental_location: loc(),
            harmonics,
            harmonics_location: None,
            vectors,
            location: loc(),
        }
    }

    fn analyze(plot: &Plot, segments: &[VectorRequest], harmonics: u32) -> Vec<FourierAnalysis> {
        resolve(
            plot,
            AnalysisKind::Transient,
            &[card(segments.to_vec(), harmonics)],
        )
        .expect("the transform resolves")
    }

    fn one(plot: &Plot) -> FourierAnalysis {
        let mut results = analyze(plot, &[out()], 9);
        assert_eq!(results.len(), 1);
        results.remove(0)
    }

    fn failed(plot: &Plot, segments: &[VectorRequest], harmonics: u32) -> SpiceError {
        resolve(
            plot,
            AnalysisKind::Transient,
            &[card(segments.to_vec(), harmonics)],
        )
        .expect_err("the transform fails")
    }

    /// The amplitude of one harmonic.
    fn amplitude(result: &FourierAnalysis, order: u32) -> Real {
        result.harmonics[(order - 1) as usize].amplitude
    }

    /// The phase of one harmonic, in radians.
    fn phase(result: &FourierAnalysis, order: u32) -> Real {
        result.harmonics[(order - 1) as usize].phase
    }

    /// The largest amplitude of the harmonics the trace does not carry.
    fn leakage(result: &FourierAnalysis, skip: &[u32]) -> Real {
        result
            .harmonics
            .iter()
            .filter(|harmonic| !skip.contains(&harmonic.order))
            .map(|harmonic| harmonic.amplitude)
            .fold(0.0, Real::max)
    }

    #[test]
    fn a_card_without_vectors_or_without_cards_has_no_result() {
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        assert!(
            resolve(&plot, AnalysisKind::Transient, &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_pure_sine_has_an_analytic_amplitude_phase_and_no_leakage() {
        // A 1 V sine sampled exactly on the 200-subinterval resampling grid: the
        // quadrature is exact for it, so the only error is the rounding of the
        // `sin` values and of the sums.
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        let result = one(&plot);
        assert!((amplitude(&result, 1) - 1.0).abs() < 1e-12, "{result:?}");
        assert!(phase(&result, 1).abs() < 1e-12, "{result:?}");
        assert!(result.dc.abs() < 1e-15, "{result:?}");
        // Every other tabulated harmonic is pure numerical noise.
        assert!(leakage(&result, &[1]) < 1e-14, "{result:?}");
        // A pure sine has no distortion beyond the first harmonic.
        assert!(result.thd < 1e-14, "{result:?}");
        assert_eq!(result.vector, "v(out)");
        assert_eq!(result.unit, "voltage");
        assert_eq!(result.divisions, DIVISIONS);
        assert_eq!(result.samples, DIVISIONS + 1);
        assert_eq!(result.window.from, 0.0);
        assert_eq!(result.window.to, PERIOD);
        assert_eq!(result.harmonics[1].frequency, 2.0 * FUNDAMENTAL);
    }

    #[test]
    fn the_phase_convention_is_positive_for_a_leading_sine_and_a_cosine_leads_by_half_pi() {
        // `amplitude * sin(k*omega*t + phase)`: a negative phase lags.
        for expected in [0.7, -0.7, 2.5, -2.5, 3.0] {
            let plot = tran(&on_grid(sine(1.0, 1, expected)));
            let result = one(&plot);
            assert!(
                (phase(&result, 1) - expected).abs() < 1e-12,
                "expected {expected}, got {result:?}"
            );
        }
        // A pure cosine is a sine a quarter period early.
        let plot = tran(&on_grid(|time| (TAU * FUNDAMENTAL * time).cos()));
        let result = one(&plot);
        assert!((phase(&result, 1) - PI / 2.0).abs() < 1e-12, "{result:?}");
    }

    #[test]
    fn a_dc_offset_and_a_known_two_harmonic_signal_recover_both_amplitudes_and_the_thd() {
        // v(t) = 0.5 + sin(wt) + 0.25*sin(2wt + pi/3): analytic DC 0.5,
        // amplitudes 1 and 0.25, phases 0 and pi/3, THD = 0.25.
        let plot = tran(&on_grid(|time| {
            0.5 + sine(1.0, 1, 0.0)(time) + sine(0.25, 2, PI / 3.0)(time)
        }));
        let result = one(&plot);
        assert!((result.dc - 0.5).abs() < 1e-13, "{result:?}");
        assert!((amplitude(&result, 1) - 1.0).abs() < 1e-12, "{result:?}");
        assert!(phase(&result, 1).abs() < 1e-12, "{result:?}");
        assert!((amplitude(&result, 2) - 0.25).abs() < 1e-12, "{result:?}");
        assert!((phase(&result, 2) - PI / 3.0).abs() < 1e-12, "{result:?}");
        assert!(leakage(&result, &[1, 2]) < 1e-14, "{result:?}");
        // THD is relative to the fundamental and includes every harmonic above
        // it: sqrt(0.25^2 + 0^2 ...) = 0.25.
        assert!((result.thd - 0.25).abs() < 1e-12, "{result:?}");
    }

    #[test]
    fn a_nonuniform_adaptive_grid_is_resampled_and_still_recovers_the_sine() {
        // A deliberately nonuniform trace — the shape an adaptive transient
        // step sequence has: small steps early, large steps later, none of them
        // aligned with the resampling grid. The amplitude and phase still come
        // out analytic because the trace is resampled, never FFT-ed in place.
        let mut rows = Vec::new();
        let mut time = 0.0;
        let mut step = 1e-6;
        let mut index = 0_u32;
        while time <= PERIOD {
            rows.push((time, sine(1.0, 1, 0.0)(time)));
            index += 1;
            if index.is_multiple_of(40) {
                step *= 1.35;
            }
            time += step;
        }
        let last = rows.last().expect("rows").0;
        rows.push((PERIOD, sine(1.0, 1, 0.0)(PERIOD)));
        assert!(last <= PERIOD);
        // Linear interpolation over the *largest* steps is the only error here:
        // `step <= 5e-5` is a twentieth of a period, so the relative amplitude
        // error is below (2*pi/20)^2/8 = 1.2e-2 ... measured below 2e-3.
        let result = one(&tran(&rows));
        assert!((amplitude(&result, 1) - 1.0).abs() < 2e-3, "{result:?}");
        assert!(phase(&result, 1).abs() < 2e-2, "{result:?}");
        assert!(leakage(&result, &[1]) < 2e-3, "{result:?}");
    }

    #[test]
    fn the_window_is_the_final_complete_period_ending_on_the_last_sample() {
        // 5 periods, the final one beginning exactly at 4 ms.
        let plot = tran(&sweep(0.0, 5.0 * PERIOD, 5_000, sine(1.0, 1, 0.0)));
        let result = one(&plot);
        assert_eq!(result.window.to, 5.0 * PERIOD);
        assert_eq!(result.window.from, 4.0 * PERIOD);
        assert!(result.samples < plot.points.len(), "{result:?}");
        assert!(result.samples > 2 * 9, "{result:?}");
    }

    #[test]
    fn phase_is_referenced_to_the_window_start_not_absolute_time_zero() {
        let offset = PERIOD / 4.0;
        // Same absolute-time sine, but stop a quarter period later: its phase
        // relative to the final window's start must lead by pi/2.
        let rows: Vec<_> = on_grid(|time| sine(1.0, 1, 0.0)(time + offset))
            .into_iter()
            .map(|(time, value)| (time + offset, value))
            .collect();
        let result = one(&tran(&rows));
        assert!((result.window.from - offset).abs() < 1e-15);
        assert!((amplitude(&result, 1) - 1.0).abs() < 1e-12);
        assert!((phase(&result, 1) - PI / 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_run_shorter_than_one_period_is_refused_rather_than_partially_transformed() {
        let plot = tran(&sweep(0.0, 0.5 * PERIOD, 200, sine(1.0, 1, 0.0)));
        let error = failed(&plot, &[out()], 9);
        assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
        assert!(
            error.to_string().contains("shorter than one period"),
            "{error}"
        );

        // Exactly one period is enough: the window is the whole plot.
        let plot = tran(&sweep(0.0, PERIOD, 200, sine(1.0, 1, 0.0)));
        assert_eq!(one(&plot).window.from, 0.0);
    }

    #[test]
    fn a_period_with_too_few_samples_for_the_harmonics_is_refused() {
        // 5 samples in the period cannot resolve 9 harmonics, which need 19.
        let plot = tran(&sweep(0.0, PERIOD, 4, sine(1.0, 1, 0.0)));
        let error = failed(&plot, &[out()], 9);
        assert!(
            error.to_string().contains("covers only 5 sample(s)"),
            "{error}"
        );
        assert!(error.to_string().contains("need at least 19"), "{error}");
        // The same trace resolves one harmonic, which needs three samples.
        let result = analyze(&plot, &[out()], 1).remove(0);
        assert_eq!(result.harmonics.len(), 1);
    }

    #[test]
    fn the_resampling_grid_is_four_subintervals_per_harmonic_with_a_floor_of_two_hundred() {
        let plot = tran(&sweep(0.0, PERIOD, 400, sine(1.0, 1, 0.0)));
        assert_eq!(analyze(&plot, &[out()], 1)[0].divisions, 200);
        assert_eq!(analyze(&plot, &[out()], 50)[0].divisions, 200);
        assert_eq!(analyze(&plot, &[out()], 51)[0].divisions, 204);
        assert_eq!(analyze(&plot, &[out()], 100)[0].divisions, 400);
    }

    #[test]
    fn a_jump_at_a_grid_point_is_refused_instead_of_choosing_a_limit() {
        // A duplicated time whose values differ is an unrepresented
        // discontinuity; the grid point at half a period lands exactly on it.
        let half = 100.0 * (PERIOD / DIVISIONS as Real);
        let mut rows = on_grid(sine(1.0, 1, 0.0));
        let at = rows
            .iter()
            .position(|(time, _)| *time == half)
            .expect("the sweep lands on half a period");
        assert!(
            rows[at].1.abs() < 1e-15,
            "a sine crosses zero at half a period"
        );
        rows.insert(at + 1, (half, 0.25));
        let error = failed(&tran(&rows), &[out()], 9);
        assert!(error.to_string().contains("discontinuous"), "{error}");

        // A repeated time with the *same* value is not a discontinuity.
        let mut rows = on_grid(sine(1.0, 1, 0.0));
        rows.insert(at + 1, rows[at]);
        let result = analyze(&tran(&rows), &[out()], 9).remove(0);
        assert!((amplitude(&result, 1) - 1.0).abs() < 1e-9, "{result:?}");
    }

    #[test]
    fn a_plot_without_a_time_axis_or_with_one_point_is_refused() {
        let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
        plot.push_variable(Variable::new("v(out)", "voltage"));
        plot.push_point(vec![Complex::real(1.0)]).unwrap();
        plot.push_point(vec![Complex::real(2.0)]).unwrap();
        let error = failed(&plot, &[out()], 9);
        assert!(error.to_string().contains("no 'time' axis"), "{error}");

        let mut plot = tran(&[(0.0, 0.0)]);
        plot.points.truncate(1);
        let error = failed(&plot, &[out()], 9);
        assert!(
            error.to_string().contains("at least 2 data points"),
            "{error}"
        );
    }

    #[test]
    fn a_descending_time_axis_and_a_non_finite_sample_are_refused() {
        let plot = tran(&[(0.0, 0.0), (0.5e-3, 1.0), (0.25e-3, 1.0), (1.0e-3, 0.0)]);
        let error = failed(&plot, &[out()], 9);
        assert!(error.to_string().contains("not non-decreasing"), "{error}");

        let mut rows = sweep(0.0, PERIOD, 400, sine(1.0, 1, 0.0));
        rows[200].1 = Real::NAN;
        let error = failed(&tran(&rows), &[out()], 9);
        assert!(matches!(error, SpiceError::Numerical { .. }), "{error}");
        assert!(error.to_string().contains("not finite"), "{error}");
    }

    #[test]
    fn a_vector_the_plot_does_not_carry_is_unsupported_at_its_own_position() {
        let plot = tran(&sweep(0.0, PERIOD, 400, sine(1.0, 1, 0.0)));
        let missing = VectorRequest {
            vector: RequestedVector::Voltage {
                positive: "nosuch".to_owned(),
                negative: None,
            },
            location: loc(),
        };
        let error = failed(&plot, &[missing], 9);
        assert!(matches!(error, SpiceError::Unsupported { .. }), "{error}");
        assert!(error.to_string().contains("v(nosuch)"), "{error}");
    }

    #[test]
    fn a_branch_current_resolves_with_its_own_unit() {
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        let mut results = analyze(&plot, &[current()], 9);
        let result = results.remove(0);
        assert_eq!(result.vector, "i(v1)");
        assert_eq!(result.unit, "current");
        // The plot's `i(v1)` column is the negated voltage.
        assert!((amplitude(&result, 1) - 1.0).abs() < 1e-12, "{result:?}");
        // atan2 can return either representation of the pi branch cut.
        assert!((phase(&result, 1).abs() - PI).abs() < 1e-12, "{result:?}");
    }

    #[test]
    fn a_card_is_only_honoured_by_the_transient_run_it_names() {
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        let error =
            resolve(&plot, AnalysisKind::Ac, &[card(vec![out()], 9)]).expect_err("an .ac run");
        assert!(
            error.to_string().contains("transforms a .tran result"),
            "{error}"
        );
    }

    #[test]
    fn a_hand_built_card_outside_the_bounded_budget_or_with_a_bad_fundamental_is_refused() {
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        let error = failed(&plot, &[out()], MAX_HARMONICS + 1);
        assert!(
            error.to_string().contains("bounded resampling budget"),
            "{error}"
        );
        let error = failed(&plot, &[out()], 0);
        assert!(
            error.to_string().contains("bounded resampling budget"),
            "{error}"
        );

        for fundamental in [0.0, -1.0e3, Real::INFINITY] {
            let card = FourierCard {
                fundamental,
                ..card(vec![out()], 9)
            };
            let error = resolve(&plot, AnalysisKind::Transient, &[card])
                .expect_err("a bad fundamental frequency");
            assert!(
                error.to_string().contains("greater than zero"),
                "{fundamental}: {error}"
            );
        }
    }

    #[test]
    fn a_trace_without_a_fundamental_leaves_the_distortion_undefined() {
        // A pure DC trace carries no fundamental, so the THD has no denominator:
        // the residue of the quadrature's own rounding is refused rather than
        // divided through (C prints whatever that residue produces).
        let plot = tran(&on_grid(|_| 2.0));
        let error = failed(&plot, &[out()], 9);
        assert!(matches!(error, SpiceError::Numerical { .. }), "{error}");
        assert!(error.to_string().contains("rounding noise"), "{error}");
        // The DC component itself is the trace's mean; a vector with no
        // fundamental is refused as a whole rather than reported without a THD.
        for value in [
            // Exactly constant: the amplitude is pure rounding residue.
            |_time: Real| 2.0,
            // A fundamental below the quadrature's own resolution.
            |time: Real| 2.0 + 1e-18 * (TAU * FUNDAMENTAL * time).sin(),
        ] {
            let error = failed(&tran(&on_grid(value)), &[out()], 9);
            assert!(matches!(error, SpiceError::Numerical { .. }), "{error}");
            assert!(error.to_string().contains("rounding noise"), "{error}");
        }
        // A genuine fundamental is far above that noise floor.
        let genuine = one(&tran(&on_grid(sine(1.0, 1, 0.0))));
        assert!(genuine.thd < 1e-14, "{genuine:?}");
    }

    #[test]
    fn every_vector_of_a_card_is_transformed_in_order() {
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        let results = analyze(&plot, &[out(), current()], 3);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].vector, "v(out)");
        assert_eq!(results[1].vector, "i(v1)");
        assert_eq!(results[0].harmonics.len(), 3);
    }

    /// The first number printed after `label` on its line.
    fn printed(text: &str, label: &str) -> Real {
        let line = text
            .lines()
            .find(|line| line.contains(label))
            .unwrap_or_else(|| panic!("no {label} line in:\n{text}"));
        line.split('=')
            .nth(1)
            .unwrap_or_else(|| panic!("no value after {label} in: {line}"))
            .split_whitespace()
            .next()
            .unwrap_or_else(|| panic!("no value after {label} in: {line}"))
            .parse()
            .unwrap_or_else(|_| panic!("not a number after {label} in: {line}"))
    }

    #[test]
    fn the_text_block_names_the_vector_the_window_the_grid_and_each_harmonic() {
        let plot = tran(&on_grid(sine(1.0, 1, 0.0)));
        let text = to_text(&analyze(&plot, &[out()], 2));
        assert!(
            text.contains("four: 1 analysis(es) of the final complete period"),
            "{text}"
        );
        assert!(text.contains("Fourier analysis for v(out):"), "{text}");
        assert_eq!(printed(&text, "fundamental"), FUNDAMENTAL);
        assert!(printed(&text, "dc").abs() < 1e-15, "{text}");
        assert!(
            text.contains(
                "window              =  [0.000000000000000e+00 , 1.000000000000000e-03] s"
            ),
            "{text}"
        );
        assert!(
            text.contains("grid                =  200 subinterval(s) per period, 201 sample(s)"),
            "{text}"
        );
        assert!(
            text.contains("harmonic  frequency                magnitude"),
            "{text}"
        );
        assert!(text.contains("phase (rad)"), "{text}");
        assert!(
            text.contains("1         1.000000000000000e+03    1.000000000000000e+00"),
            "{text}"
        );
        assert!(text.contains("2         2.000000000000000e+03"), "{text}");
        assert!(printed(&text, "thd").abs() < 1e-14, "{text}");
    }
}

/// C's `fourier()` front-end variables. `nfreqs` includes the DC row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FourierSettings {
    /// Number of frequency rows, including DC (1..=101).
    pub nfreqs: u32,
    /// Number of final periods (1..=100).
    pub nperiods: u32,
    /// Polynomial interpolation degree (0..=16); zero uses sample order.
    pub polydegree: u32,
    /// Half-open grid samples per period (1..=100000).
    pub gridsize: u32,
}
impl Default for FourierSettings {
    fn default() -> Self {
        Self {
            nfreqs: 10,
            nperiods: 1,
            polydegree: 1,
            gridsize: 200,
        }
    }
}

/// Evaluate explicit front-end settings using C's half-open DFT grid.
/// C: `frontend/fourier.c::{fourier,CKTfour}`, `maths/poly/interpolate.c`.
///
/// # Errors
/// Invalid settings, exceeded work budget, insufficient samples or failed fits,
/// missing vectors, nonfinite arithmetic or undefined THD.
pub fn resolve_with_settings(
    plot: &Plot,
    kind: AnalysisKind,
    cards: &[FourierCard],
    settings: FourierSettings,
) -> SpiceResult<Vec<FourierAnalysis>> {
    resolve_settings(plot, kind, cards, settings, true)
}

/// Evaluate the configured DFT with well-conditioned local Lagrange interpolation.
/// This library alternative avoids C's absolute-coordinate fit fallback and its
/// stale high-degree coefficient tail; it deliberately does not reproduce that error.
///
/// # Errors
/// As [`resolve_with_settings`]; this alternative also rejects repeated timestamps.
pub fn resolve_with_stable_settings(
    plot: &Plot,
    kind: AnalysisKind,
    cards: &[FourierCard],
    settings: FourierSettings,
) -> SpiceResult<Vec<FourierAnalysis>> {
    resolve_settings(plot, kind, cards, settings, false)
}

fn resolve_settings(
    plot: &Plot,
    kind: AnalysisKind,
    cards: &[FourierCard],
    settings: FourierSettings,
    compatible: bool,
) -> SpiceResult<Vec<FourierAnalysis>> {
    if cards.is_empty() {
        return Ok(Vec::new());
    }
    let first = &cards[0];
    if kind != AnalysisKind::Transient
        || !(1..=101).contains(&settings.nfreqs)
        || !(1..=100).contains(&settings.nperiods)
        || settings.polydegree > 16
        || !(1..=100_000).contains(&settings.gridsize)
    {
        return Err(unsupported(
            first,
            "invalid Fourier settings or analysis".into(),
        ));
    }
    let count = (settings.gridsize as usize)
        .checked_mul(settings.nperiods as usize)
        .filter(|n| *n <= 100_000)
        .ok_or_else(|| unsupported(first, "Fourier grid exceeds 100000 samples".into()))?;
    let mut results = Vec::new();
    for card in cards {
        check_budget(card)?;
        let axis = TimeAxis::of(plot, card)?;
        let periods = settings.nperiods as Real;
        let to = *axis
            .values
            .last()
            .ok_or_else(|| unsupported(card, "empty Fourier axis".into()))?;
        let from = if settings.polydegree == 0 {
            axis.values[0]
        } else {
            to - periods / card.fundamental
        };
        if from < axis.values[0] {
            return Err(unsupported(
                card,
                "Fourier window is longer than the run".into(),
            ));
        }
        let harmonics = if card.harmonics_location.is_some() {
            card.harmonics as usize
        } else {
            settings.nfreqs.saturating_sub(1) as usize
        };
        if harmonics == 0 {
            return Err(unsupported(
                card,
                "nfreqs must include a fundamental for THD".into(),
            ));
        }
        for request in &card.vectors {
            let trace = Trace::resolve(plot, request)?;
            let mut values = Vec::new();
            if settings.polydegree == 0 {
                if axis.values.len() > 100_000 {
                    return Err(unsupported(card, "Fourier sample budget exceeded".into()));
                }
                for i in 0..axis.values.len() {
                    values.push(trace.value(i)?);
                }
            } else {
                let samples = (0..axis.values.len())
                    .map(|i| trace.value(i))
                    .collect::<SpiceResult<Vec<_>>>()?;
                let grid: Vec<_> = (0..count)
                    .map(|i| from + (to - from) * i as Real / count as Real)
                    .collect();
                let degree = settings.polydegree as usize;
                if compatible {
                    values =
                        super::fourier_resample::resample(&axis.values, &samples, &grid, degree)?;
                } else {
                    if axis.values.len() <= degree || axis.values.windows(2).any(|w| w[0] == w[1]) {
                        return Err(unsupported(
                            card,
                            "stable polynomial interpolation needs distinct samples".into(),
                        ));
                    }
                    for x in grid {
                        let right = axis.values.partition_point(|t| *t < x);
                        let start = right
                            .saturating_sub(degree.div_ceil(2))
                            .min(axis.values.len() - degree - 1);
                        let mut value = 0.;
                        for k in 0..=degree {
                            let mut weight = 1.;
                            for j in 0..=degree {
                                if j != k {
                                    weight *= (x - axis.values[start + j])
                                        / (axis.values[start + k] - axis.values[start + j]);
                                }
                            }
                            value += weight * samples[start + k];
                        }
                        if !value.is_finite() {
                            return Err(nonfinite(
                                trace.name().into(),
                                "nonfinite stable polynomial interpolation".into(),
                            ));
                        }
                        values.push(value);
                    }
                }
            }
            let n = values.len();
            if n < 2 * harmonics + 1 {
                return Err(unsupported(
                    card,
                    "Fourier grid undersamples requested harmonics".into(),
                ));
            }
            let dc = values.iter().sum::<Real>() / n as Real;
            let mut tabulated = Vec::new();
            for k in 1..=harmonics {
                let mut sine = 0.;
                let mut cosine = 0.;
                for (i, y) in values.iter().enumerate() {
                    let phase = std::f64::consts::TAU * k as Real * periods * i as Real / n as Real;
                    sine += y * phase.sin();
                    cosine += y * phase.cos();
                }
                tabulated.push(Harmonic {
                    order: k as u32,
                    frequency: k as Real * card.fundamental,
                    amplitude: 2. * sine.hypot(cosine) / n as Real,
                    phase: cosine.atan2(sine),
                });
            }
            let fundamental = tabulated[0].amplitude;
            let magnitude = values.iter().map(|v| v.abs()).sum::<Real>() / n as Real;
            if fundamental <= NOISE_ULPS * f64::EPSILON * magnitude {
                return Err(nonfinite(
                    trace.name().into(),
                    "fundamental at rounding noise; undefined THD".into(),
                ));
            }
            let thd = tabulated
                .iter()
                .skip(1)
                .map(|h| (h.amplitude / fundamental).powi(2))
                .sum::<Real>()
                .sqrt();
            if !dc.is_finite()
                || !thd.is_finite()
                || tabulated.iter().any(|h| {
                    !h.frequency.is_finite() || !h.amplitude.is_finite() || !h.phase.is_finite()
                })
            {
                return Err(nonfinite(
                    trace.name().into(),
                    "nonfinite Fourier result".into(),
                ));
            }
            results.push(FourierAnalysis {
                vector: trace.name().into(),
                unit: trace.unit().into(),
                fundamental: card.fundamental,
                dc,
                harmonics: tabulated,
                thd,
                window: FourierWindow { from, to },
                divisions: n,
                samples: axis.values.len(),
            });
        }
    }
    Ok(results)
}
