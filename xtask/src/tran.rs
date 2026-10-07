//! Event-aware transient comparison on a shared physical time grid.
//!
//! C (adaptive trap/Gear) and Rust (the companion trap/Gear driver, or diffsol
//! BDF) choose different internal timepoints, so sample sequences are never
//! compared. Both plots are evaluated at common output times instead:
//!
//! * **Smooth interval**: a value at `t` is the linear interpolation of the two
//!   neighbouring samples of that plot. Interpolation is refused (an error, not
//!   a pass) if a source breakpoint lies strictly between those two samples.
//! * **Breakpoint**: a discontinuity or slope change of a source (PWL corner,
//!   PULSE edge, from the deck AST via [`breakpoints`], never guessed from
//!   data). The left and right limits are compared separately. A plot carries
//!   two samples at the instant for a true jump (first = left, second = right);
//!   a single sample is taken as both limits, which is exact for the continuous
//!   quantities (capacitor voltage, inductor current) and fails loudly for a
//!   discontinuous one that the other plot resolves into two distinct limits.
//!   A plot with no sample at a breakpoint cannot supply a limit and is
//!   rejected: ngspice lands a step on every breakpoint (`CKTbreak`,
//!   `dctran.c`) and the Rust driver must as well.
//!
//! Tolerances are in [`compare::TRAN`]. Interpolating the denser plot adds
//! `O(h^2 v'')` error, so the comparison is only meaningful when the sample
//! spacing is small against the circuit time constants; that is the caller's
//! fixture design responsibility and is not hidden by a looser tolerance.

use std::collections::BTreeMap;

use spice_analysis::{Plot, PlotFlags};
use spice_core::parse_spice_number;
use spice_netlist::ast::{DeviceInstance, Netlist, ParameterKind, PositionedValue, SourceWaveform};

use crate::compare::{self, TranTolerance};

/// Time coincidence is judged within `TIME_EPS_REL * stop`: far below any
/// physical timestep, far above the 17-digit rawfile rounding of `t` and of
/// `td + n*per + corner` sums.
const TIME_EPS_REL: f64 = 1e-9;
/// Upper bound on comparison instants and generated breakpoints.
const MAX_POINTS: usize = 2_000_000;

/// The shared output grid: `start, step, 2*step, ... , stop` (each time is
/// `i*step`, not an accumulated sum). Breakpoints inside `(start, stop)` are
/// inserted by [`transient`] and compared as left/right limit pairs.
///
/// `start` is 0 for an ordinary run. With `uic` C writes no `t = 0` row (its
/// first row is the first accepted step); the caller then passes that first
/// time, and **both** plots must begin exactly there: the instant is compared as
/// an ordinary sample, and nothing before it is interpolated or extrapolated.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Grid {
    pub(crate) start: f64,
    pub(crate) stop: f64,
    pub(crate) step: f64,
}

/// What a successful comparison covered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Summary {
    /// Interior (smooth) or end-point instants compared.
    pub(crate) instants: usize,
    /// One-sided breakpoint limits compared (two per breakpoint).
    pub(crate) limits: usize,
    /// Largest `|error| / bound` seen over every compared value (<= 1).
    pub(crate) worst_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side {
    Interior,
    Left,
    Right,
}

/// One validated transient plot.
struct Series<'a> {
    label: &'static str,
    plot: &'a Plot,
    columns: BTreeMap<String, usize>,
    times: Vec<f64>,
    breakpoints: &'a [f64],
    eps: f64,
}

impl<'a> Series<'a> {
    fn new(
        label: &'static str,
        plot: &'a Plot,
        breakpoints: &'a [f64],
        grid: &Grid,
        eps: f64,
    ) -> Result<Self, String> {
        let columns = compare::validate(plot, label)?;
        if plot.flags != PlotFlags::Real {
            return Err(format!("{label}: transient data must be a real plot"));
        }
        let &time = columns
            .get("time")
            .ok_or_else(|| format!("{label}: missing axis 'time'"))?;
        if plot.variables[time].unit != "time" {
            return Err(format!("{label}: axis 'time' must have unit 'time'"));
        }
        let times: Vec<f64> = plot.points.iter().map(|row| row[time].re).collect();
        if (times[0] - grid.start).abs() > eps {
            return Err(format!(
                "{label}: transient starts at {:e}, not {:e}",
                times[0], grid.start
            ));
        }
        let end = times[times.len() - 1];
        if (end - grid.stop).abs() > eps {
            return Err(format!(
                "{label}: end time {end:e} differs from requested stop {:e}",
                grid.stop
            ));
        }
        let series = Self {
            label,
            plot,
            columns,
            times,
            breakpoints,
            eps,
        };
        series.check_times()?;
        Ok(series)
    }

    /// Times must not decrease; two samples may share an instant only at a
    /// declared breakpoint (left/right limits), and never three.
    fn check_times(&self) -> Result<(), String> {
        for (index, pair) in self.times.windows(2).enumerate() {
            let step = pair[1] - pair[0];
            if step < -self.eps {
                return Err(format!(
                    "{}: time decreases at point {}: {:e} -> {:e}",
                    self.label,
                    index + 1,
                    pair[0],
                    pair[1]
                ));
            }
            if step.abs() <= self.eps {
                if self.breakpoint_near(pair[0]).is_none() {
                    return Err(format!(
                        "{}: repeated time {:e} at point {} is not a declared breakpoint",
                        self.label,
                        pair[0],
                        index + 1
                    ));
                }
                if index > 0 && (self.times[index - 1] - pair[0]).abs() <= self.eps {
                    return Err(format!(
                        "{}: more than two samples at breakpoint {:e}",
                        self.label, pair[0]
                    ));
                }
            }
        }
        Ok(())
    }

    fn breakpoint_near(&self, t: f64) -> Option<f64> {
        self.breakpoints
            .iter()
            .copied()
            .find(|b| (b - t).abs() <= self.eps)
    }

    /// Value of column `column` at `t` on `side`.
    fn value(&self, column: usize, t: f64, side: Side) -> Result<f64, String> {
        let at = |index: usize| self.plot.points[index][column].re;
        if let Some(b) = self.breakpoint_near(t) {
            let near: Vec<usize> = (0..self.times.len())
                .filter(|&i| (self.times[i] - b).abs() <= self.eps)
                .collect();
            let (Some(&first), Some(&last)) = (near.first(), near.last()) else {
                return Err(format!(
                    "{}: no sample at breakpoint {b:e}; cannot form a one-sided limit",
                    self.label
                ));
            };
            return match side {
                Side::Left => Ok(at(first)),
                Side::Right => Ok(at(last)),
                Side::Interior => Err(format!(
                    "{}: interior evaluation requested at breakpoint {b:e}",
                    self.label
                )),
            };
        }
        let upper = self.times.partition_point(|&x| x < t - self.eps);
        if upper == self.times.len() {
            return Err(format!("{}: time {t:e} is beyond the samples", self.label));
        }
        if (self.times[upper] - t).abs() <= self.eps {
            return Ok(at(upper));
        }
        if upper == 0 {
            return Err(format!("{}: time {t:e} precedes the samples", self.label));
        }
        let (t0, t1) = (self.times[upper - 1], self.times[upper]);
        if let Some(b) = self
            .breakpoints
            .iter()
            .find(|&&b| b > t0 + self.eps && b < t1 - self.eps)
        {
            return Err(format!(
                "{}: samples [{t0:e}, {t1:e}] straddle breakpoint {b:e}; refusing to interpolate across it",
                self.label
            ));
        }
        let fraction = (t - t0) / (t1 - t0);
        Ok((1.0 - fraction) * at(upper - 1) + fraction * at(upper))
    }
}

fn comparison_instants(
    grid: &Grid,
    breakpoints: &[f64],
    eps: f64,
) -> Result<Vec<(f64, Side)>, String> {
    if !(grid.stop.is_finite() && grid.stop > 0.0 && grid.step.is_finite() && grid.step > 0.0) {
        return Err("grid stop and step must be finite and positive".into());
    }
    if !(grid.start.is_finite() && grid.start >= 0.0 && grid.start < grid.stop - eps) {
        return Err("grid start must be finite and lie in [0, stop)".into());
    }
    let count = (grid.stop / grid.step + 1e-9).floor();
    if count >= MAX_POINTS as f64 {
        return Err(format!("grid needs more than {MAX_POINTS} points"));
    }
    let mut previous = grid.start;
    for &b in breakpoints {
        if !(b.is_finite() && b > eps && b < grid.stop - eps && b > previous + eps) {
            return Err(format!(
                "breakpoint {b:e} must be finite, strictly inside (start, stop) and strictly increasing"
            ));
        }
        previous = b;
    }
    let mut instants: Vec<(f64, Side)> = Vec::new();
    // The first instant is `start` itself (0 for an ordinary run); grid times
    // before it do not exist in either plot.
    instants.push((grid.start, Side::Interior));
    for index in 0..=(count as usize) {
        let t = index as f64 * grid.step;
        if t > grid.start + eps
            && t < grid.stop - eps
            && !breakpoints.iter().any(|b| (b - t).abs() <= eps)
        {
            instants.push((t, Side::Interior));
        }
    }
    instants.push((grid.stop, Side::Interior));
    for &b in breakpoints {
        instants.push((b, Side::Left));
        instants.push((b, Side::Right));
    }
    // Stable sort keeps Left before Right at equal times.
    instants.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(instants)
}

/// Compare Rust `got` with C `want` on `grid`, treating `breakpoints` (sorted,
/// strictly inside `(0, stop)`; see [`breakpoints`]) as events. Variables match
/// by case-insensitive name; every `want` signal other than `time` must have a
/// `voltage` or `current` unit and satisfy
/// `|got - want| <= relative*|want| + absolute(kind)` at every instant and at
/// both limits of every breakpoint. Both plots must end at `grid.stop`.
pub(crate) fn transient(
    got: &Plot,
    want: &Plot,
    tolerance: TranTolerance,
    grid: &Grid,
    breakpoints: &[f64],
) -> Result<Summary, String> {
    let eps = TIME_EPS_REL * grid.stop;
    let instants = comparison_instants(grid, breakpoints, eps)?;
    let rust = Series::new("Rust", got, breakpoints, grid, eps)?;
    let c = Series::new("C", want, breakpoints, grid, eps)?;
    compare::check_structure(got, want, &rust.columns, &c.columns)?;
    let mut signals = Vec::new();
    for (name, &column) in &c.columns {
        if name == "time" {
            continue;
        }
        let absolute = match want.variables[column].unit.as_str() {
            "voltage" => tolerance.voltage_absolute,
            "current" => tolerance.current_absolute,
            other => return Err(format!("signal '{name}' has unsupported unit '{other}'")),
        };
        let peak = (0..want.point_count())
            .map(|row| want.points[row][column].re.abs())
            .fold(0.0, f64::max);
        let absolute = absolute + tolerance.peak_relative * peak;
        signals.push((name, rust.columns[name], column, absolute));
    }
    let mut summary = Summary {
        instants: 0,
        limits: 0,
        worst_ratio: 0.0,
    };
    let mut first = None;
    let mut worst = None;
    let mut mismatches = 0usize;
    for &(t, side) in &instants {
        if side == Side::Interior {
            summary.instants += 1;
        } else {
            summary.limits += 1;
        }
        for &(name, got_column, want_column, absolute) in &signals {
            let actual = rust.value(got_column, t, side)?;
            let expected = c.value(want_column, t, side)?;
            let limit = tolerance.relative * expected.abs() + absolute;
            let error = (actual - expected).abs();
            let ratio = error / limit;
            summary.worst_ratio = summary.worst_ratio.max(ratio);
            if error > limit {
                mismatches += 1;
                let report = format!(
                    "t={t:.9e} ({side:?}), {name}: Rust {actual:.17e}, C {expected:.17e}, |error| {error:.6e} > bound {limit:.6e}"
                );
                if first.is_none() {
                    first = Some(report.clone());
                }
                if worst.as_ref().is_none_or(|(r, _)| ratio > *r) {
                    worst = Some((ratio, report));
                }
            }
        }
    }
    match (first, worst) {
        (Some(first), Some((ratio, worst))) => Err(format!(
            "{mismatches} transient mismatch(es)\nfirst: {first}\nworst ({ratio:.6e}x bound): {worst}"
        )),
        _ => Ok(summary),
    }
}

fn number(value: &PositionedValue, what: &str, device: &str) -> Result<f64, String> {
    parse_spice_number(&value.text)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{device}: {what} '{}' is not a finite number", value.text))
}

/// Source breakpoints in `(0, tstop)` from the deck AST, sorted and deduplicated.
///
/// PWL: every knot time (a repeated time is one breakpoint with a jump).
/// PULSE (C `VSRCaccept`, `vsrcacct.c`): `TD + n*PER + {0, TR, TR+PW, TR+PW+TF}`,
/// with C's defaults: `TR`/`TF` <= 0 or omitted -> `tstep`; `PW` omitted ->
/// `tstop`, except exactly five values given -> 0; `PER` <= 0 or omitted ->
/// `tstop`. Only top-level V/I sources are inspected; a deck with subcircuit
/// definitions is rejected because waveforms inside them are not enumerated.
/// Expression-valued or non-numeric waveform fields are errors.
pub(crate) fn breakpoints(netlist: &Netlist, tstep: f64, tstop: f64) -> Result<Vec<f64>, String> {
    if !(tstep.is_finite() && tstep > 0.0 && tstop.is_finite() && tstop > 0.0) {
        return Err("tstep and tstop must be finite and positive".into());
    }
    if !netlist.subcircuits.is_empty() {
        return Err("breakpoints: subcircuit waveforms are not enumerated".into());
    }
    let eps = TIME_EPS_REL * tstop;
    let mut times = Vec::new();
    for device in &netlist.devices {
        if !matches!(device.designator, 'v' | 'i') {
            continue;
        }
        for parameter in &device.parameters {
            if let ParameterKind::Waveform(waveform) = &parameter.kind {
                collect(device, waveform, tstep, tstop, &mut times)?;
            }
        }
    }
    times.retain(|&t| t > eps && t < tstop - eps);
    times.sort_by(f64::total_cmp);
    times.dedup_by(|b, a| (*b - *a).abs() <= eps);
    Ok(times)
}

fn collect(
    device: &DeviceInstance,
    waveform: &SourceWaveform,
    tstep: f64,
    tstop: f64,
    out: &mut Vec<f64>,
) -> Result<(), String> {
    let name = device.name.as_str();
    match waveform {
        SourceWaveform::Pwl(points) => {
            for point in points {
                out.push(number(&point.time, "PWL time", name)?);
            }
        }
        SourceWaveform::Pulse(pulse) => {
            let optional = [
                &pulse.delay,
                &pulse.rise,
                &pulse.fall,
                &pulse.width,
                &pulse.period,
            ];
            let given = optional.iter().take_while(|v| v.is_some()).count();
            let get = |value: &Option<PositionedValue>, what| -> Result<Option<f64>, String> {
                value.as_ref().map(|v| number(v, what, name)).transpose()
            };
            let delay = get(&pulse.delay, "PULSE delay")?.unwrap_or(0.0);
            let rise = get(&pulse.rise, "PULSE rise")?
                .filter(|&v| v > 0.0)
                .unwrap_or(tstep);
            let fall = get(&pulse.fall, "PULSE fall")?
                .filter(|&v| v > 0.0)
                .unwrap_or(tstep);
            let width = if given == 3 {
                0.0
            } else {
                get(&pulse.width, "PULSE width")?
                    .filter(|&v| v >= 0.0)
                    .unwrap_or(tstop)
            };
            let period = get(&pulse.period, "PULSE period")?
                .filter(|&v| v > 0.0)
                .unwrap_or(tstop);
            // Validate level spellings too: an unparseable level is an error.
            number(&pulse.initial, "PULSE v1", name)?;
            number(&pulse.pulsed, "PULSE v2", name)?;
            if (tstop - delay) / period > MAX_POINTS as f64 / 4.0 {
                return Err(format!("{name}: PULSE would create too many breakpoints"));
            }
            let mut start = delay;
            while start < tstop {
                for corner in [0.0, rise, rise + width, rise + width + fall] {
                    out.push(start + corner);
                }
                start += period;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spice_analysis::Variable;
    use spice_core::Complex;
    use spice_netlist::{Parser, source::parse_deck_text};
    use std::path::Path;

    const STOP: f64 = 10.0;

    fn plot(rows: &[(f64, f64)]) -> Plot {
        let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
        plot.variables = vec![
            Variable::new("time", "time"),
            Variable::new("v(out)", "voltage"),
        ];
        plot.points = rows
            .iter()
            .map(|&(t, v)| vec![Complex::real(t), Complex::real(v)])
            .collect();
        plot
    }
    /// Samples of `f` at the given times.
    fn sampled(times: &[f64], f: impl Fn(f64) -> f64) -> Plot {
        plot(&times.iter().map(|&t| (t, f(t))).collect::<Vec<_>>())
    }
    fn times(step: f64) -> Vec<f64> {
        (0..=(STOP / step).round() as usize)
            .map(|i| i as f64 * step)
            .collect()
    }
    fn grid() -> Grid {
        Grid {
            start: 0.0,
            stop: STOP,
            step: 1.0,
        }
    }
    fn run(got: &Plot, want: &Plot, breakpoints: &[f64]) -> Result<Summary, String> {
        transient(got, want, compare::TRAN, &grid(), breakpoints)
    }
    /// 0 for t<5, 3 + t after: a true jump of 3 V plus a ramp, at t = 5.
    fn jump(t: f64, right: bool) -> f64 {
        if t < 5.0 || (t == 5.0 && !right) {
            0.0
        } else {
            3.0 + t
        }
    }
    fn jump_plot(step: f64) -> Plot {
        let mut rows = Vec::new();
        for t in times(step) {
            if t == 5.0 {
                rows.push((t, jump(t, false)));
                rows.push((t, jump(t, true)));
            } else {
                rows.push((t, jump(t, true)));
            }
        }
        plot(&rows)
    }

    #[test]
    fn different_timesteps_agree_on_the_common_grid() {
        // Linear data is reproduced exactly by linear interpolation, so two
        // unrelated, non-nested sample sequences must pass.
        let line = |t: f64| 2.0 * t + 1.0;
        let got = sampled(&times(0.25), line);
        let mut odd: Vec<f64> = (0..=40).map(|i| (i as f64 * 0.2437).min(STOP)).collect();
        odd.dedup();
        odd.push(STOP);
        odd.dedup();
        let want = sampled(&odd, line);
        let summary = run(&got, &want, &[]).unwrap();
        assert_eq!(summary.instants, 11);
        assert_eq!(summary.limits, 0);
        assert!(summary.worst_ratio < 1e-9);
    }

    #[test]
    fn interpolation_stays_within_a_smooth_segment_and_is_checked() {
        // 1 V at t=0 to 3 V at t=10: grid point 1.0 is not a sample of `want`.
        let got = plot(&[(0.0, 1.0), (1.0, 1.2), (10.0, 3.0)]);
        let want = plot(&[(0.0, 1.0), (10.0, 3.0)]);
        run(&got, &want, &[]).unwrap();
        let mut wrong = want.clone();
        wrong.points[1][1] = Complex::real(3.1);
        let error = run(&got, &wrong, &[]).unwrap_err();
        assert!(
            error.contains("first: t=") && error.contains("worst"),
            "{error}"
        );
    }

    #[test]
    fn interpolation_across_a_breakpoint_is_refused() {
        // Samples 3 and 6 bracket grid instant 4 but the event at 5 lies
        // between them: refused, even though a straight line would "fit".
        let coarse = plot(&[(0.0, 0.0), (3.0, 0.0), (6.0, 9.0), (10.0, 13.0)]);
        let fine = jump_plot(0.5);
        for (got, want) in [(&coarse, &fine), (&fine, &coarse)] {
            let error = run(got, want, &[5.0]).unwrap_err();
            assert!(error.contains("refusing to interpolate"), "{error}");
        }
        // A plot with the bracket split by a sample at 5 but no jump pair is a
        // single-sample (continuous) limit, so it is usable for kinks.
        let kink = |t: f64| if t < 5.0 { t } else { 5.0 + 2.0 * (t - 5.0) };
        let rows = |step| sampled(&times(step), kink);
        assert_eq!(run(&rows(0.5), &rows(0.25), &[5.0]).unwrap().limits, 2);
        // Without a sample at the event no limit exists.
        let skipped = plot(&[(0.0, 0.0), (4.0, 4.0), (6.0, 7.0), (10.0, 15.0)]);
        let error = run(&skipped, &rows(1.0), &[5.0]);
        assert!(error.is_err());
    }

    #[test]
    fn interior_bracket_containing_a_breakpoint_is_rejected_directly() {
        // Series-level check of the refusal path: bracket [3, 6] contains 5.
        let p = plot(&[(0.0, 0.0), (3.0, 0.0), (6.0, 9.0), (10.0, 9.0)]);
        let bps = [5.0];
        let series = Series::new("Rust", &p, &bps, &grid(), 1e-8).unwrap();
        let error = series.value(1, 4.0, Side::Interior).unwrap_err();
        assert!(error.contains("refusing to interpolate"), "{error}");
        assert_eq!(series.value(1, 1.5, Side::Interior).unwrap(), 0.0);
        assert_eq!(series.value(1, 8.0, Side::Interior).unwrap(), 9.0);
    }

    #[test]
    fn left_and_right_limits_are_compared_separately() {
        let got = jump_plot(0.5);
        let want = jump_plot(0.25);
        let summary = run(&got, &want, &[5.0]).unwrap();
        assert_eq!(summary.limits, 2);
        // Interpolating across the jump would pass 4.5..5.5 values wrongly; the
        // plots above only agree because each side is handled on its own.
        let mut bad_right = jump_plot(0.5);
        let last_at_five = bad_right
            .points
            .iter()
            .rposition(|row| row[0].re == 5.0)
            .unwrap();
        bad_right.points[last_at_five][1] = Complex::real(8.5);
        let error = run(&bad_right, &want, &[5.0]).unwrap_err();
        assert!(error.contains("(Right)"), "{error}");
        let mut bad_left = jump_plot(0.5);
        let first_at_five = bad_left
            .points
            .iter()
            .position(|row| row[0].re == 5.0)
            .unwrap();
        bad_left.points[first_at_five][1] = Complex::real(0.5);
        let error = run(&bad_left, &want, &[5.0]).unwrap_err();
        assert!(error.contains("(Left)"), "{error}");
    }

    #[test]
    fn a_jump_cannot_be_hidden_by_interpolating_across_it() {
        // Same continuous-looking trace without the repeated instant: a single
        // sample at 5 is both limits, so the C jump (0 vs 8) must be caught.
        let want = jump_plot(0.5);
        let mut smeared = jump_plot(0.5);
        let first = smeared.points.iter().position(|r| r[0].re == 5.0).unwrap();
        smeared.points.remove(first);
        let error = run(&smeared, &want, &[5.0]).unwrap_err();
        assert!(error.contains("(Left)"), "{error}");
        // And with no declared event the repeated instant is invalid input.
        let error = run(&want, &want, &[]).unwrap_err();
        assert!(error.contains("not a declared breakpoint"), "{error}");
    }

    #[test]
    fn a_continuous_single_sample_serves_both_limits() {
        let line = |t: f64| t;
        let got = sampled(&times(0.5), line);
        let want = sampled(&times(0.25), line);
        assert_eq!(run(&got, &want, &[5.0]).unwrap().limits, 2);
    }

    #[test]
    fn end_time_and_start_time_must_match() {
        let full = sampled(&times(1.0), |t| t);
        let short = sampled(&times(1.0)[..10], |t| t);
        for (got, want) in [(&short, &full), (&full, &short)] {
            assert!(run(got, want, &[]).unwrap_err().contains("end time"));
        }
        let late = plot(&[(0.5, 0.0), (10.0, 0.0)]);
        assert!(run(&late, &full, &[]).unwrap_err().contains("not 0"));
    }

    #[test]
    fn missing_extra_and_unsupported_signals_fail() {
        let want = sampled(&times(1.0), |t| t);
        let mut missing_time = want.clone();
        missing_time.variables[0].name = "t".into();
        assert!(run(&missing_time, &want, &[]).unwrap_err().contains("time"));
        let mut missing = want.clone();
        missing.variables[1].name = "v(other)".into();
        let error = run(&missing, &want, &[]).unwrap_err();
        assert!(
            error.contains("missing") && error.contains("v(out)"),
            "{error}"
        );
        assert!(run(&want, &missing, &[]).unwrap_err().contains("extra"));
        let mut odd_unit = want.clone();
        odd_unit.variables[1].unit = "charge".into();
        assert!(
            run(&odd_unit, &odd_unit, &[])
                .unwrap_err()
                .contains("unsupported unit")
        );
        let mut complex = want.clone();
        complex.flags = PlotFlags::Complex;
        assert!(run(&complex, &complex, &[]).is_err());
    }

    #[test]
    fn nonfinite_and_malformed_data_fail_on_either_side() {
        let good = sampled(&times(1.0), |t| t);
        for value in [f64::NAN, f64::INFINITY] {
            for column in [0, 1] {
                let mut bad = good.clone();
                bad.points[3][column] = Complex::real(value);
                assert!(run(&bad, &good, &[]).is_err());
                assert!(run(&good, &bad, &[]).is_err());
            }
        }
        let mut backwards = good.clone();
        backwards.points[4][0] = Complex::real(1.0);
        assert!(
            run(&backwards, &good, &[])
                .unwrap_err()
                .contains("decreases")
        );
        let mut empty = good.clone();
        empty.points.clear();
        assert!(run(&empty, &good, &[]).is_err());
    }

    #[test]
    fn current_uses_its_own_absolute_floor_and_relative_scales_with_value() {
        let mut want = sampled(&times(1.0), |_| 0.0);
        want.variables[1] = Variable::new("i(v1)", "current");
        let mut got = want.clone();
        for row in &mut got.points {
            row[1] = Complex::real(0.5 * compare::TRAN.current_absolute);
        }
        run(&got, &want, &[]).unwrap();
        for row in &mut got.points {
            row[1] = Complex::real(2.0 * compare::TRAN.current_absolute);
        }
        assert!(run(&got, &want, &[]).is_err());
        // Voltage uses relative 1e-3 of a 1 V value: 0.5 mV passes, 2 mV fails.
        let mut volts = sampled(&times(1.0), |_| 1.0);
        let reference = volts.clone();
        for row in &mut volts.points {
            row[1] = Complex::real(1.0 + 0.5e-3);
        }
        run(&volts, &reference, &[]).unwrap();
        for row in &mut volts.points {
            row[1] = Complex::real(1.0 + 2e-3);
        }
        assert!(run(&volts, &reference, &[]).is_err());
    }

    #[test]
    fn invalid_grids_and_breakpoints_are_rejected() {
        let p = sampled(&times(1.0), |t| t);
        for g in [
            Grid {
                start: 0.0,
                stop: STOP,
                step: 0.0,
            },
            Grid {
                start: 0.0,
                stop: STOP,
                step: f64::NAN,
            },
            Grid {
                start: 0.0,
                stop: 0.0,
                step: 1.0,
            },
            Grid {
                start: 0.0,
                stop: STOP,
                step: 1e-12,
            },
        ] {
            assert!(transient(&p, &p, compare::TRAN, &g, &[]).is_err());
        }
        for bps in [
            vec![0.0],
            vec![STOP],
            vec![f64::NAN],
            vec![6.0, 5.0],
            vec![5.0, 5.0],
        ] {
            assert!(run(&p, &p, &bps).is_err(), "{bps:?}");
        }
    }

    fn deck(body: &str) -> Netlist {
        Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("t.cir"),
                &format!("title\n{body}\nr1 in 0 1k\n.end\n"),
            ))
            .unwrap()
    }

    #[test]
    fn pwl_corners_become_breakpoints() {
        let netlist = deck("v1 in 0 pwl(0 0 1m 0 1.01m 1 1.01m 2 6m 2 9m 0)");
        let found = breakpoints(&netlist, 1e-5, 8e-3).unwrap();
        assert_eq!(found, vec![1e-3, 1.01e-3, 6e-3]);
        assert!(breakpoints(&netlist, 0.0, 8e-3).is_err());
    }

    #[test]
    fn pulse_edges_follow_c_defaults_and_period() {
        // td=1, tr=1, tf=2, pw=3, per=10 over 0..25.
        let netlist = deck("v1 in 0 pulse(0 5 1 1 2 3 10)");
        let found = breakpoints(&netlist, 0.1, 25.0).unwrap();
        assert_eq!(
            found,
            vec![1.0, 2.0, 5.0, 7.0, 11.0, 12.0, 15.0, 17.0, 21.0, 22.0]
        );
        // Omitted tr/tf -> tstep; omitted pw with five values -> 0; per -> tstop.
        let netlist = deck("i1 in 0 pulse(0 1 0 0 0)");
        assert_eq!(breakpoints(&netlist, 0.5, 4.0).unwrap(), vec![0.5, 1.0]);
        let netlist = deck("i1 in 0 pulse(0 1)");
        assert_eq!(breakpoints(&netlist, 0.5, 4.0).unwrap(), vec![0.5]);
    }

    #[test]
    fn non_source_devices_and_out_of_range_corners_are_ignored() {
        let netlist = deck("v1 in 0 dc 1");
        assert!(breakpoints(&netlist, 1.0, 5.0).unwrap().is_empty());
        let netlist = deck("v1 in 0 pwl(0 0 9 1)");
        assert!(breakpoints(&netlist, 1.0, 5.0).unwrap().is_empty());
    }

    #[test]
    fn breakpoints_feed_the_comparator_end_to_end() {
        let netlist = deck("v1 in 0 pwl(0 0 5 0 5 3 10 13)");
        let bps = breakpoints(&netlist, 0.1, STOP).unwrap();
        assert_eq!(bps, vec![5.0]);
        run(&jump_plot(0.5), &jump_plot(0.2), &bps).unwrap();
    }

    /// The only transient engine today is explicit diffsol BDF. PWL deck
    /// evaluation (#9) is not merged, so the AST supplies the breakpoints and
    /// knots while the circuit's source is swapped for the runtime waveform
    /// (as `c_linear_reference.rs` does). The reference is the analytic RC
    /// response sampled on an unrelated, denser grid. Today's engine emits only
    /// its requested output grid (no extra sample at an off-grid event), so the
    /// corners 1 ms and 1.1 ms are chosen on that grid; a future driver must
    /// emit samples at every breakpoint (see the module docs).
    #[test]
    fn diffsol_bdf_rc_ramp_matches_analytic_on_a_common_grid() {
        use spice_analysis::{AnalysisContext, AnalysisRequest, runner};
        use spice_core::AnalysisKind;
        use spice_devices::{Circuit, IndependentSource, Waveform};

        let (tau, stop, t0, tr, high) = (1e-3, 6e-3, 1e-3, 1e-4, 5.0);
        let pwl = deck("v1 in 0 pwl(0 0 1m 0 1.1m 5 6m 5)");
        let bps = breakpoints(&pwl, 1e-4, stop).unwrap();
        assert_eq!(bps.len(), 2);
        let Some(SourceWaveform::Pwl(knots)) =
            pwl.devices[0]
                .parameters
                .iter()
                .find_map(|p| match &p.kind {
                    ParameterKind::Waveform(w) => Some(w.clone()),
                    _ => None,
                })
        else {
            panic!("no PWL waveform")
        };
        let knots: Vec<(f64, f64)> = knots
            .iter()
            .map(|k| {
                (
                    parse_spice_number(&k.time.text).unwrap(),
                    parse_spice_number(&k.value.text).unwrap(),
                )
            })
            .collect();
        let dc = Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("rc.cir"),
                "rc\nv1 in 0 dc 0\nr1 in out 1k\nc1 out 0 1u\n.end\n",
            ))
            .unwrap();
        let mut circuit = Circuit::from_netlist(&dc).unwrap();
        let terminals = circuit.devices()[0].terminals();
        let nodes = [terminals[0], terminals[1]];
        circuit.devices_mut()[0] = Box::new(
            IndependentSource::new("v1", nodes, true, 0.0, Complex::ZERO, Waveform::Pwl(knots))
                .unwrap(),
        );
        let request = AnalysisRequest::with_arguments(
            AnalysisKind::Transient,
            ["1e-4", "6e-3", "0", "5e-5", "backend=diffsol", "method=bdf"],
        );
        let got = runner(request.kind)
            .unwrap()
            .run(&mut circuit, &request, &AnalysisContext::default())
            .unwrap();
        let slope = high / tr;
        let ramp = |x: f64| slope * (x - tau * (1.0 - (-x / tau).exp()));
        let analytic = |t: f64| {
            if t <= t0 {
                0.0
            } else if t <= t0 + tr {
                ramp(t - t0)
            } else {
                high + (ramp(tr) - high) * (-(t - t0 - tr) / tau).exp()
            }
        };
        let mut reference_times: Vec<f64> = (0..=3000).map(|i| i as f64 * 2e-6).collect();
        reference_times.push(t0 + tr);
        reference_times.sort_by(f64::total_cmp);
        reference_times.dedup_by(|b, a| (*b - *a).abs() < 1e-12);
        let mut want = sampled(&reference_times, analytic);
        want.variables[1].name = "v(out)".into();
        let mut got_v = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
        got_v.variables = vec![
            Variable::new("time", "time"),
            Variable::new("v(out)", "voltage"),
        ];
        got_v.points = (0..got.point_count())
            .map(|i| {
                vec![
                    got.value("time", i).unwrap(),
                    got.value("v(out)", i).unwrap(),
                ]
            })
            .collect();
        let grid = Grid {
            start: 0.0,
            stop,
            step: 1e-4,
        };
        let summary = transient(&got_v, &want, compare::TRAN, &grid, &bps).unwrap();
        assert_eq!(summary.limits, 4);
    }

    #[test]
    fn a_uic_style_start_after_zero_is_compared_from_that_instant_only() {
        // C with `uic` has no t = 0 row: both plots begin at the first accepted
        // step (here 0.4). The grid starts there and nothing earlier exists.
        let line = |t: f64| 2.0 * t + 1.0;
        let times_from = |step: f64| -> Vec<f64> {
            let mut times = vec![0.4];
            times.extend(
                (1..=(STOP / step).round() as usize)
                    .map(|i| i as f64 * step)
                    .filter(|&t| t > 0.4),
            );
            times
        };
        let grid = Grid {
            start: 0.4,
            ..grid()
        };
        let got = sampled(&times_from(0.25), line);
        let want = sampled(&times_from(0.5), line);
        let summary = transient(&got, &want, compare::TRAN, &grid, &[]).unwrap();
        // 0.4, then 1..=9 on the unit grid, and the stop time.
        assert_eq!(summary.instants, 11);
        // A plot starting elsewhere (including the ordinary t = 0) is refused,
        // as is a nonsensical start.
        let from_zero = sampled(&times(0.5), line);
        let error = transient(&from_zero, &want, compare::TRAN, &grid, &[]).unwrap_err();
        assert!(error.contains("starts at"), "{error}");
        for bad in [-1.0, f64::NAN, STOP] {
            let grid = Grid { start: bad, ..grid };
            assert!(transient(&got, &want, compare::TRAN, &grid, &[]).is_err());
        }
        // A breakpoint at or before the start is rejected.
        assert!(transient(&got, &want, compare::TRAN, &grid, &[0.4]).is_err());
    }

    #[test]
    fn peak_scaled_policy_widens_only_by_the_signal_peak() {
        // C data: a 1 V ramp (peak 1). Rust is offset by 3e-4 V at mid-scale and
        // by the same amount near zero.
        let want = sampled(&times(0.5), |t| t / STOP);
        let near_zero = sampled(&times(0.5), |t| t / STOP + 3e-4 * (-t).exp());
        // Pointwise bound at t = 0 is only 1e-6 V: TRAN rejects, the peak-scaled
        // bound (1e-3 * 1 V) accepts.
        assert!(run(&near_zero, &want, &[]).is_err());
        let scaled = compare::TRAN_RESTART;
        assert!(transient(&near_zero, &want, scaled, &grid(), &[]).is_ok());
        // An error above reltol * peak is still rejected, and the bound follows
        // the peak of C's data rather than a fixed constant.
        let off = sampled(&times(0.5), |t| t / STOP + 2e-3);
        assert!(transient(&off, &want, scaled, &grid(), &[]).is_err());
        let small = sampled(&times(0.5), |t| 1e-3 * t / STOP);
        let small_off = sampled(&times(0.5), |t| 1e-3 * t / STOP + 3e-6);
        assert!(transient(&small_off, &small, scaled, &grid(), &[]).is_err());
        assert_eq!(compare::TRAN.peak_relative, 0.0);
    }
}
