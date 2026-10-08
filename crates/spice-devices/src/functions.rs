//! Analytic SIN, EXP, SFFM and AM forcing, and PWL with `td=` delay and `r=`
//! repetition, for independent V and I sources.
//!
//! Upstream behavior: `src/spicelib/devices/vsrc/vsrcload.c` (values, `case
//! SINE`/`EXP`/`SFFM`/`AM`/`PWL`), `vsrcpar.c` (`VSRC_R`/`VSRC_TD` setters) and
//! the identical `isrc/isrcload.c`/`isrcpar.c`. Defaults that depend on the
//! analysis (`CKTstep`, `CKTfinalTime`) stay in the `*Spec` types until
//! [`FunctionSpec::resolve`] binds a [`TransientTiming`]; the resolved
//! [`SourceFunction`] is fully specified.
//!
//! C defaults, per field (an omitted field takes the default; "zero" means an
//! explicit `0` is replaced as well, exactly as C tests the coefficient):
//!
//! | Function | Field | Default |
//! | --- | --- | --- |
//! | SIN | FREQ | `1 / CKTfinalTime` (omitted or zero) |
//! | SIN | TD, THETA, PHASE | 0 |
//! | EXP | TD1, TAU1 | `CKTstep` (omitted or zero) |
//! | EXP | TD2 | `TD1 + CKTstep` (omitted or zero) |
//! | EXP | TAU2 | `CKTstep` (omitted or zero) |
//! | SFFM | FC | `5 / CKTfinalTime` (omitted only) |
//! | SFFM | MDI | 90 (omitted only), then limited to `[0, FC/FM]` |
//! | SFFM | FM | `500 / CKTfinalTime` (omitted or zero) |
//! | SFFM/AM | TD, PHASEM, PHASEC | 0 |
//! | AM | VMA | 1 (omitted only) |
//! | AM | FM | `5 / CKTfinalTime` (omitted only) |
//! | AM | FC | `500 / CKTfinalTime` (omitted only) |
//!
//! Phases are in degrees. Before its delay a SIN holds `VO + VA sin(PHASE)`, an
//! EXP holds V1, and SFFM/AM hold **zero** (not VO): C returns 0 for
//! `time <= TD`, so SFFM/AM generally jump at their delay. OP/DC analyses
//! without an explicit DC value use the C time-zero value
//! ([`FunctionSpec::time_zero`]).
//!
//! Breakpoints: C's `VSRCaccept` sets none for SIN/EXP/SFFM/AM. The port
//! deliberately lands steps on the corners it can name exactly (SIN/SFFM/AM
//! delay, EXP `TD1`/`TD2`) so its drivers never integrate across a slope change
//! or the SFFM/AM delay jump; values are unaffected. PWL breakpoints are every
//! (delayed, repeated) knot, as `VSRCaccept` sets them.
//!
//! Deliberate differences from C: negative delays (SIN/SFFM/AM `TD`, EXP
//! `TD1`/`TD2`) are rejected, as for PULSE, rather than shifting the waveform
//! earlier (C would then also need `CKTstep`/`CKTfinalTime` in a plain `.op`,
//! where they are unset); SFFM's MDI limiting is silent (C prints a one-time
//! warning); and every value is checked to be finite.

use std::f64::consts::PI;

use crate::{Limit, TransientTiming};
use spice_core::{Real, SpiceError, SpiceResult};

/// The largest cycle index whose start time is still resolvable in `f64`.
const MAX_CYCLE: Real = 4_503_599_627_370_496.0; // 2^52

fn invalid(kind: &str, message: impl Into<String>) -> SpiceError {
    SpiceError::circuit(format!("invalid {kind} waveform: {}", message.into()))
}

/// Checks that optional fields are a contiguous, finite prefix and returns how
/// many were supplied.
fn prefix(kind: &str, fields: &[Real], optional: &[Option<Real>]) -> SpiceResult<usize> {
    let given = optional.iter().take_while(|v| v.is_some()).count();
    if optional[given..].iter().any(Option::is_some) {
        return Err(invalid(kind, "optional fields must be a contiguous prefix"));
    }
    if fields
        .iter()
        .chain(optional.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(invalid(kind, "nonfinite field"));
    }
    Ok(given)
}

fn nonnegative_delay(kind: &str, name: &str, value: Option<Real>) -> SpiceResult<()> {
    if value.is_some_and(|v| v < 0.) {
        return Err(invalid(
            kind,
            format!("negative {name} is not supported (C would shift the waveform earlier)"),
        ));
    }
    Ok(())
}

fn finite(kind: &str, value: Real) -> SpiceResult<Real> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid(kind, "evaluation is not finite"))
    }
}

/// `value` unless it is omitted (or, with `zero`, exactly zero).
fn or_default(value: Option<Real>, zero: bool, default: Real) -> Real {
    match value {
        Some(v) if !(zero && v == 0.) => v,
        _ => default,
    }
}

/// `SIN(VO VA [FREQ [TD [THETA [PHASE]]]])` as written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SineSpec {
    /// VO, the offset.
    pub offset: Real,
    /// VA, the amplitude.
    pub amplitude: Real,
    /// FREQ in hertz (omitted or zero: `1 / CKTfinalTime`).
    pub frequency: Option<Real>,
    /// TD in seconds (default 0; negative rejected).
    pub delay: Option<Real>,
    /// THETA, the damping factor in 1/s (default 0).
    pub damping: Option<Real>,
    /// PHASE in degrees (default 0).
    pub phase: Option<Real>,
}

/// `EXP(V1 V2 [TD1 [TAU1 [TD2 [TAU2]]]])` as written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpSpec {
    /// V1, the initial level.
    pub initial: Real,
    /// V2, the pulsed level.
    pub pulsed: Real,
    /// TD1, rise delay (omitted or zero: `CKTstep`; negative rejected).
    pub rise_delay: Option<Real>,
    /// TAU1, rise time constant (omitted or zero: `CKTstep`).
    pub rise_tau: Option<Real>,
    /// TD2, fall delay (omitted or zero: `TD1 + CKTstep`; negative rejected).
    pub fall_delay: Option<Real>,
    /// TAU2, fall time constant (omitted or zero: `CKTstep`).
    pub fall_tau: Option<Real>,
}

/// `SFFM(VO VA [FC [MDI [FM [TD [PHASEM [PHASEC]]]]]])` as written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SffmSpec {
    /// VO, the offset.
    pub offset: Real,
    /// VA, the amplitude.
    pub amplitude: Real,
    /// FC, carrier frequency (omitted: `5 / CKTfinalTime`).
    pub carrier: Option<Real>,
    /// MDI, modulation index (omitted: 90), limited to `[0, FC/FM]`.
    pub index: Option<Real>,
    /// FM, signal frequency (omitted or zero: `500 / CKTfinalTime`).
    pub signal: Option<Real>,
    /// TD in seconds (default 0; negative rejected).
    pub delay: Option<Real>,
    /// PHASEM, signal phase in degrees (default 0).
    pub signal_phase: Option<Real>,
    /// PHASEC, carrier phase in degrees (default 0).
    pub carrier_phase: Option<Real>,
}

/// `AM(VO VMO [VMA [FM [FC [TD [PHASEM [PHASEC]]]]]])` as written, in the
/// coefficient order of `vsrcload.c` (`case AM`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmSpec {
    /// VO, the offset.
    pub offset: Real,
    /// VMO, the carrier amplitude offset.
    pub carrier_amplitude: Real,
    /// VMA, the modulation amplitude (omitted: 1).
    pub modulation_amplitude: Option<Real>,
    /// FM, modulation frequency (omitted: `5 / CKTfinalTime`).
    pub modulation_frequency: Option<Real>,
    /// FC, carrier frequency (omitted: `500 / CKTfinalTime`).
    pub carrier_frequency: Option<Real>,
    /// TD in seconds (default 0; negative rejected).
    pub delay: Option<Real>,
    /// PHASEM, modulation phase in degrees (default 0).
    pub modulation_phase: Option<Real>,
    /// PHASEC, carrier phase in degrees (default 0).
    pub carrier_phase: Option<Real>,
}

/// A SIN/EXP/SFFM/AM setter with C's analysis-dependent defaults pending.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FunctionSpec {
    /// SIN.
    Sine(SineSpec),
    /// EXP.
    Exp(ExpSpec),
    /// SFFM.
    Sffm(SffmSpec),
    /// AM.
    Am(AmSpec),
}

impl FunctionSpec {
    fn kind(&self) -> &'static str {
        match self {
            Self::Sine(_) => "SIN",
            Self::Exp(_) => "EXP",
            Self::Sffm(_) => "SFFM",
            Self::Am(_) => "AM",
        }
    }

    /// Checks everything that does not depend on the analysis: a contiguous,
    /// finite optional prefix and nonnegative delays.
    ///
    /// # Errors
    /// Invalid fields, as described.
    pub fn validate(&self) -> SpiceResult<()> {
        let kind = self.kind();
        match self {
            Self::Sine(s) => {
                prefix(
                    kind,
                    &[s.offset, s.amplitude],
                    &[s.frequency, s.delay, s.damping, s.phase],
                )?;
                nonnegative_delay(kind, "TD", s.delay)
            }
            Self::Exp(e) => {
                prefix(
                    kind,
                    &[e.initial, e.pulsed],
                    &[e.rise_delay, e.rise_tau, e.fall_delay, e.fall_tau],
                )?;
                nonnegative_delay(kind, "TD1", e.rise_delay)?;
                nonnegative_delay(kind, "TD2", e.fall_delay)
            }
            Self::Sffm(f) => {
                prefix(
                    kind,
                    &[f.offset, f.amplitude],
                    &[
                        f.carrier,
                        f.index,
                        f.signal,
                        f.delay,
                        f.signal_phase,
                        f.carrier_phase,
                    ],
                )?;
                nonnegative_delay(kind, "TD", f.delay)
            }
            Self::Am(a) => {
                prefix(
                    kind,
                    &[a.offset, a.carrier_amplitude],
                    &[
                        a.modulation_amplitude,
                        a.modulation_frequency,
                        a.carrier_frequency,
                        a.delay,
                        a.modulation_phase,
                        a.carrier_phase,
                    ],
                )?;
                nonnegative_delay(kind, "TD", a.delay)
            }
        }
    }

    /// The value C loads in OP/DC analyses when no DC value is given: the
    /// function at `time = 0` (`vsrcload.c` sets `time = 0` under `MODEDC`).
    /// With nonnegative delays this never needs the analysis defaults.
    ///
    /// # Errors
    /// The spec is invalid ([`Self::validate`]).
    pub fn time_zero(&self) -> SpiceResult<Real> {
        self.validate()?;
        let value = match self {
            Self::Sine(s) => s.offset + s.amplitude * (s.phase.unwrap_or(0.) * PI / 180.).sin(),
            Self::Exp(e) => e.initial,
            Self::Sffm(_) | Self::Am(_) => 0.,
        };
        finite(self.kind(), value)
    }

    /// Resolves C's defaults for one transient run.
    ///
    /// # Errors
    /// The spec is invalid, or a resolved default is not finite.
    pub fn resolve(&self, timing: &TransientTiming) -> SpiceResult<SourceFunction> {
        self.validate()?;
        let kind = self.kind();
        let step = timing.step();
        let stop = timing.final_time();
        let resolved = match *self {
            Self::Sine(s) => SourceFunction::Sine(Sine {
                offset: s.offset,
                amplitude: s.amplitude,
                frequency: or_default(s.frequency, true, 1. / stop),
                delay: s.delay.unwrap_or(0.),
                damping: s.damping.unwrap_or(0.),
                phase: s.phase.unwrap_or(0.),
            }),
            Self::Exp(e) => {
                let rise_delay = or_default(e.rise_delay, true, step);
                SourceFunction::Exp(Exponential {
                    initial: e.initial,
                    pulsed: e.pulsed,
                    rise_delay,
                    rise_tau: or_default(e.rise_tau, true, step),
                    fall_delay: or_default(e.fall_delay, true, rise_delay + step),
                    fall_tau: or_default(e.fall_tau, true, step),
                })
            }
            Self::Sffm(f) => {
                let carrier = or_default(f.carrier, false, 5. / stop);
                let signal = or_default(f.signal, true, 500. / stop);
                let mut index = or_default(f.index, false, 90.);
                // vsrcload.c limits the modulation index on every load.
                if index > carrier / signal {
                    index = carrier / signal;
                } else if index < 0. {
                    index = 0.;
                }
                SourceFunction::Sffm(Sffm {
                    offset: f.offset,
                    amplitude: f.amplitude,
                    carrier,
                    index,
                    signal,
                    delay: f.delay.unwrap_or(0.),
                    signal_phase: f.signal_phase.unwrap_or(0.),
                    carrier_phase: f.carrier_phase.unwrap_or(0.),
                })
            }
            Self::Am(a) => SourceFunction::Am(Am {
                offset: a.offset,
                carrier_amplitude: a.carrier_amplitude,
                modulation_amplitude: or_default(a.modulation_amplitude, false, 1.),
                modulation_frequency: or_default(a.modulation_frequency, false, 5. / stop),
                carrier_frequency: or_default(a.carrier_frequency, false, 500. / stop),
                delay: a.delay.unwrap_or(0.),
                modulation_phase: a.modulation_phase.unwrap_or(0.),
                carrier_phase: a.carrier_phase.unwrap_or(0.),
            }),
        };
        if !resolved.parameters().iter().all(|v| v.is_finite()) {
            return Err(invalid(kind, "a resolved field is not finite"));
        }
        Ok(resolved)
    }
}

/// A resolved SIN: `VO + VA sin(2 pi FREQ s + PHASE) exp(-THETA s)` with
/// `s = t - TD > 0`, and `VO + VA sin(PHASE)` before the delay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sine {
    offset: Real,
    amplitude: Real,
    frequency: Real,
    delay: Real,
    damping: Real,
    phase: Real,
}

/// A resolved EXP: V1 until TD1, an exponential approach to V2 with TAU1, and
/// from TD2 an exponential return towards V1 with TAU2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exponential {
    initial: Real,
    pulsed: Real,
    rise_delay: Real,
    rise_tau: Real,
    fall_delay: Real,
    fall_tau: Real,
}

/// A resolved SFFM:
/// `VO + VA sin(2 pi FC s + PHASEC + MDI sin(2 pi FM s + PHASEM))` for
/// `s = t - TD > 0`, and zero before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sffm {
    offset: Real,
    amplitude: Real,
    carrier: Real,
    index: Real,
    signal: Real,
    delay: Real,
    signal_phase: Real,
    carrier_phase: Real,
}

/// A resolved AM:
/// `VO + (VMO + VMA sin(2 pi FM s + PHASEM)) sin(2 pi FC s + PHASEC)` for
/// `s = t - TD > 0`, and zero before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Am {
    offset: Real,
    carrier_amplitude: Real,
    modulation_amplitude: Real,
    modulation_frequency: Real,
    carrier_frequency: Real,
    delay: Real,
    modulation_phase: Real,
    carrier_phase: Real,
}

/// A fully specified SIN/EXP/SFFM/AM.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceFunction {
    /// SIN.
    Sine(Sine),
    /// EXP.
    Exp(Exponential),
    /// SFFM.
    Sffm(Sffm),
    /// AM.
    Am(Am),
}

impl SourceFunction {
    fn kind(&self) -> &'static str {
        match self {
            Self::Sine(_) => "SIN",
            Self::Exp(_) => "EXP",
            Self::Sffm(_) => "SFFM",
            Self::Am(_) => "AM",
        }
    }

    /// The resolved coefficients in C order (after defaults and, for SFFM,
    /// the modulation-index limit). Phases are in degrees.
    #[must_use]
    pub fn parameters(&self) -> Vec<Real> {
        match *self {
            Self::Sine(s) => vec![
                s.offset,
                s.amplitude,
                s.frequency,
                s.delay,
                s.damping,
                s.phase,
            ],
            Self::Exp(e) => vec![
                e.initial,
                e.pulsed,
                e.rise_delay,
                e.rise_tau,
                e.fall_delay,
                e.fall_tau,
            ],
            Self::Sffm(f) => vec![
                f.offset,
                f.amplitude,
                f.carrier,
                f.index,
                f.signal,
                f.delay,
                f.signal_phase,
                f.carrier_phase,
            ],
            Self::Am(a) => vec![
                a.offset,
                a.carrier_amplitude,
                a.modulation_amplitude,
                a.modulation_frequency,
                a.carrier_frequency,
                a.delay,
                a.modulation_phase,
                a.carrier_phase,
            ],
        }
    }

    /// Forcing at `t` with the requested one-sided limit. Only SFFM/AM jump
    /// (at their delay, from zero); elsewhere both limits agree.
    ///
    /// # Errors
    /// `t` or the result is not finite.
    pub fn value_at(&self, t: Real, limit: Limit) -> SpiceResult<Real> {
        let kind = self.kind();
        if !t.is_finite() {
            return Err(invalid(kind, "nonfinite evaluation time"));
        }
        let value = match *self {
            Self::Sine(s) => {
                let time = t - s.delay;
                let phase = s.phase * PI / 180.;
                if time <= 0. {
                    s.offset + s.amplitude * phase.sin()
                } else {
                    s.offset
                        + s.amplitude
                            * (s.frequency * time * 2. * PI + phase).sin()
                            * (-time * s.damping).exp()
                }
            }
            Self::Exp(e) => {
                let rise = |time: Real| {
                    (e.pulsed - e.initial) * (1. - (-(time - e.rise_delay) / e.rise_tau).exp())
                };
                if t <= e.rise_delay {
                    e.initial
                } else if t <= e.fall_delay {
                    e.initial + rise(t)
                } else {
                    e.initial
                        + rise(t)
                        + (e.initial - e.pulsed) * (1. - (-(t - e.fall_delay) / e.fall_tau).exp())
                }
            }
            Self::Sffm(f) => {
                let time = t - f.delay;
                if time < 0. || (time == 0. && limit == Limit::Left) {
                    0.
                } else {
                    let phasec = f.carrier_phase * PI / 180.;
                    let phasem = f.signal_phase * PI / 180.;
                    f.offset
                        + f.amplitude
                            * ((2. * PI * f.carrier * time + phasec)
                                + f.index * (2. * PI * f.signal * time + phasem).sin())
                            .sin()
                }
            }
            Self::Am(a) => {
                let time = t - a.delay;
                if time < 0. || (time == 0. && limit == Limit::Left) {
                    0.
                } else {
                    let phasec = a.carrier_phase * PI / 180.;
                    let phasem = a.modulation_phase * PI / 180.;
                    a.offset
                        + (a.carrier_amplitude
                            + a.modulation_amplitude
                                * (2. * PI * a.modulation_frequency * time + phasem).sin())
                            * (2. * PI * a.carrier_frequency * time + phasec).sin()
                }
            }
        };
        finite(kind, value)
    }

    /// The corners the port lands on, ascending and unique: SIN/SFFM/AM
    /// delay, EXP `TD1` and `TD2`. See the module notes on C parity.
    #[must_use]
    pub fn corners(&self) -> Vec<Real> {
        let mut corners = match *self {
            Self::Sine(s) => vec![s.delay],
            Self::Exp(e) => vec![e.rise_delay, e.fall_delay],
            Self::Sffm(f) => vec![f.delay],
            Self::Am(a) => vec![a.delay],
        };
        corners.sort_by(Real::total_cmp);
        corners.dedup();
        corners
    }
}

/// A PWL with C's `td=` delay and optional `r=` repetition (`vsrcpar.c`
/// `VSRC_TD`/`VSRC_R`, `vsrcload.c` `case PWL`).
///
/// With delay `TD`, the knots apply at `TD + t_i`; before the first knot the
/// first value holds. Without repetition the last value holds after the last
/// knot. With `r = t_k` the segment `[t_k, t_last]` repeats forever with period
/// `t_last - t_k`; where `v_k != v_last` each repetition boundary is a jump
/// (left limit `v_last`, right limit `v_k`; C evaluates exactly the first
/// boundary to `v_last` and later ones to `v_k`).
#[derive(Debug, Clone, PartialEq)]
pub struct PwlSource {
    knots: Vec<(Real, Real)>,
    delay: Real,
    repeat: Option<usize>,
}

impl PwlSource {
    /// Creates a delayed, optionally repeating PWL.
    ///
    /// `repeat_from` is the `r=` value: `None` (or C's `r < -0.5` "no
    /// repetition" spelling) disables repetition; otherwise it must equal one
    /// knot time exactly and be less than the last knot time, as `vsrcpar.c`
    /// requires.
    ///
    /// # Errors
    /// No knots, nonfinite or negative knot times, times not strictly
    /// increasing, nonfinite values or delay, or an invalid `r=`.
    pub fn new(
        knots: Vec<(Real, Real)>,
        delay: Real,
        repeat_from: Option<Real>,
    ) -> SpiceResult<Self> {
        let kind = "PWL";
        if knots.is_empty()
            || !knots
                .iter()
                .all(|(t, v)| t.is_finite() && *t >= 0. && v.is_finite())
            || !knots.windows(2).all(|w| w[0].0 < w[1].0)
        {
            return Err(invalid(
                kind,
                "times must be finite, nonnegative and strictly increasing with finite values",
            ));
        }
        if !delay.is_finite() {
            return Err(invalid(kind, "nonfinite td"));
        }
        let repeat = match repeat_from {
            Some(r) if !r.is_finite() => return Err(invalid(kind, "nonfinite r")),
            Some(r) if r >= -0.5 => {
                let end = knots[knots.len() - 1].0;
                if r >= end {
                    return Err(invalid(
                        kind,
                        format!("repeat start r={r} must be smaller than the last time point"),
                    ));
                }
                let index = knots.iter().position(|(t, _)| *t == r).ok_or_else(|| {
                    invalid(kind, format!("repeat start r={r} matches no time point"))
                })?;
                Some(index)
            }
            _ => None,
        };
        Ok(Self {
            knots,
            delay,
            repeat,
        })
    }

    /// The knots as given (before the delay).
    #[must_use]
    pub fn knots(&self) -> &[(Real, Real)] {
        &self.knots
    }

    /// The `td=` delay.
    #[must_use]
    pub const fn delay(&self) -> Real {
        self.delay
    }

    /// The knot index repetition restarts from, if repeating.
    #[must_use]
    pub const fn repeat(&self) -> Option<usize> {
        self.repeat
    }

    /// Linear interpolation at an in-range local time (C's segment search).
    fn interpolate(&self, time: Real) -> Real {
        for pair in self.knots.windows(2) {
            let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
            if t1 >= time {
                let fraction = (time - t0) / (t1 - t0);
                return v0 + fraction * (v1 - v0);
            }
        }
        self.knots[self.knots.len() - 1].1
    }

    /// Forcing at `t` with the requested one-sided limit.
    ///
    /// # Errors
    /// `t` nonfinite, or so many repetitions from the origin that the
    /// in-cycle position is unresolvable.
    pub fn value_at(&self, t: Real, limit: Limit) -> SpiceResult<Real> {
        let kind = "PWL";
        if !t.is_finite() {
            return Err(invalid(kind, "nonfinite evaluation time"));
        }
        let time = t - self.delay;
        if !time.is_finite() {
            return Err(invalid(kind, "evaluation time overflows"));
        }
        let (first, last) = (self.knots[0], self.knots[self.knots.len() - 1]);
        if time <= first.0 {
            return Ok(first.1);
        }
        let Some(index) = self
            .repeat
            .filter(|_| time > last.0 || (time == last.0 && limit == Limit::Right))
        else {
            return Ok(if time >= last.0 {
                last.1
            } else {
                self.interpolate(time)
            });
        };
        let start = self.knots[index].0;
        let period = last.0 - start;
        let elapsed = time - start;
        let cycle = (elapsed / period).floor();
        if !cycle.is_finite() || cycle >= MAX_CYCLE {
            return Err(invalid(
                kind,
                "evaluation time is too many repetitions from the origin",
            ));
        }
        // C's arithmetic, including its clamp to the last knot ("prevent
        // glitches"), so a rounded boundary lands where C puts it.
        let position = (start + (elapsed - period * cycle)).min(last.0);
        if position <= start || position >= last.0 {
            // A repetition boundary: the end of one repetition on the left,
            // the restart value on the right.
            return Ok(match limit {
                Limit::Left => last.1,
                Limit::Right => self.knots[index].1,
            });
        }
        Ok(self.interpolate(position))
    }

    /// Lazily enumerates the delayed knot times in `[t0, t1]`, continuing
    /// through the repetitions, ascending and strictly increasing.
    ///
    /// # Errors
    /// Nonfinite or reversed window, or a window spanning more repetitions
    /// than can be resolved.
    pub fn breakpoints_in(&self, t0: Real, t1: Real) -> SpiceResult<PwlBreakpoints> {
        if !(t0.is_finite() && t1.is_finite() && t0 <= t1) {
            return Err(invalid(
                "PWL",
                "breakpoint window must be finite with t0 <= t1",
            ));
        }
        let mut first_cycle = 1.;
        if let Some(index) = self.repeat {
            let last = self.knots[self.knots.len() - 1].0;
            let period = last - self.knots[index].0;
            let cycles = ((t1 - self.delay - last) / period).ceil();
            if !cycles.is_finite() || cycles >= MAX_CYCLE {
                return Err(invalid(
                    "PWL",
                    "breakpoint window spans too many repetitions",
                ));
            }
            // Skip whole repetitions before the window (one early for rounding).
            let skip = ((t0 - self.delay - last) / period).floor() - 1.;
            if skip > 1. {
                first_cycle = skip;
            }
        }
        Ok(PwlBreakpoints {
            source: self.clone(),
            t0,
            t1,
            cycle: 0.,
            first_cycle,
            index: 0,
            last: None,
            done: false,
        })
    }
}

/// Lazy knot iterator for a [`PwlSource`] over a finite window.
#[derive(Debug, Clone)]
pub struct PwlBreakpoints {
    source: PwlSource,
    t0: Real,
    t1: Real,
    /// 0 for the first pass over every knot, then the repetition number.
    cycle: Real,
    first_cycle: Real,
    index: usize,
    last: Option<Real>,
    done: bool,
}

impl Iterator for PwlBreakpoints {
    type Item = Real;
    fn next(&mut self) -> Option<Real> {
        let knots = &self.source.knots;
        while !self.done {
            if self.index >= knots.len() {
                let Some(repeat) = self.source.repeat else {
                    self.done = true;
                    break;
                };
                self.cycle = if self.cycle == 0. {
                    self.first_cycle
                } else {
                    self.cycle + 1.
                };
                // Knots after the restart point; the restart itself coincides
                // with the previous repetition's last knot.
                self.index = repeat + 1;
            }
            let period = self
                .source
                .repeat
                .map_or(0., |repeat| knots[knots.len() - 1].0 - knots[repeat].0);
            let t = self.source.delay + knots[self.index].0 + period * self.cycle;
            self.index += 1;
            if !t.is_finite() || t > self.t1 {
                self.done = true;
            } else if t >= self.t0 && self.last.is_none_or(|last| t > last) {
                self.last = Some(t);
                return Some(t);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const L: Limit = Limit::Left;
    const R: Limit = Limit::Right;

    fn timing() -> TransientTiming {
        TransientTiming::new(1e-3, 1.).unwrap()
    }
    fn close(a: Real, b: Real) -> bool {
        (a - b).abs() <= 1e-12 * (1. + a.abs().max(b.abs()))
    }

    fn sine(fields: [Option<Real>; 4]) -> FunctionSpec {
        FunctionSpec::Sine(SineSpec {
            offset: 0.5,
            amplitude: 2.,
            frequency: fields[0],
            delay: fields[1],
            damping: fields[2],
            phase: fields[3],
        })
    }

    #[test]
    fn sine_defaults_values_and_corners() {
        // Two fields: FREQ = 1/stop, no delay/damping/phase.
        let s = sine([None; 4]).resolve(&timing()).unwrap();
        assert_eq!(s.parameters(), [0.5, 2., 1., 0., 0., 0.]);
        assert!(close(s.value_at(0.25, R).unwrap(), 2.5));
        // An explicit zero frequency is also replaced.
        let s = sine([Some(0.), None, None, None])
            .resolve(&timing())
            .unwrap();
        assert_eq!(s.parameters()[2], 1.);
        // Delay, damping and phase.
        let spec = sine([Some(10.), Some(0.1), Some(2.), Some(30.)]);
        let s = spec.resolve(&timing()).unwrap();
        let before = 0.5 + 2. * (30f64.to_radians()).sin();
        assert!(close(spec.time_zero().unwrap(), before));
        for t in [0., 0.05, 0.1] {
            assert!(close(s.value_at(t, L).unwrap(), before));
            assert!(close(s.value_at(t, R).unwrap(), before));
        }
        let t = 0.1 + 0.0125;
        let want = 0.5
            + 2. * (2. * PI * 10. * 0.0125 + 30f64.to_radians()).sin() * (-2.0_f64 * 0.0125).exp();
        assert!(close(s.value_at(t, R).unwrap(), want));
        assert_eq!(s.corners(), [0.1]);
    }

    #[test]
    fn exp_defaults_values_and_corners() {
        let spec = |f: [Option<Real>; 4]| {
            FunctionSpec::Exp(ExpSpec {
                initial: 1.,
                pulsed: 3.,
                rise_delay: f[0],
                rise_tau: f[1],
                fall_delay: f[2],
                fall_tau: f[3],
            })
        };
        // Two fields: TD1 = TAU1 = TAU2 = step, TD2 = TD1 + step.
        let e = spec([None; 4]).resolve(&timing()).unwrap();
        assert_eq!(e.parameters(), [1., 3., 1e-3, 1e-3, 2e-3, 1e-3]);
        // Explicit zeros are replaced; TD2 follows the resolved TD1.
        let e = spec([Some(0.5), Some(0.), Some(0.), Some(0.)])
            .resolve(&timing())
            .unwrap();
        assert_eq!(e.parameters(), [1., 3., 0.5, 1e-3, 0.501, 1e-3]);
        let spec = spec([Some(0.1), Some(0.2), Some(0.5), Some(0.3)]);
        assert_eq!(spec.time_zero().unwrap(), 1.);
        let e = spec.resolve(&timing()).unwrap();
        assert_eq!(e.value_at(0.1, L).unwrap(), 1.);
        let rise = |t: Real| 2. * (1. - (-(t - 0.1) / 0.2).exp());
        assert!(close(e.value_at(0.3, R).unwrap(), 1. + rise(0.3)));
        assert!(close(e.value_at(0.5, L).unwrap(), 1. + rise(0.5)));
        let fall = |t: Real| -2. * (1. - (-(t - 0.5) / 0.3).exp());
        assert!(close(
            e.value_at(0.8, R).unwrap(),
            1. + rise(0.8) + fall(0.8)
        ));
        assert_eq!(e.corners(), [0.1, 0.5]);
    }

    #[test]
    fn sffm_defaults_limits_and_delay_jump() {
        let spec = |f: [Option<Real>; 6]| {
            FunctionSpec::Sffm(SffmSpec {
                offset: 1.,
                amplitude: 2.,
                carrier: f[0],
                index: f[1],
                signal: f[2],
                delay: f[3],
                signal_phase: f[4],
                carrier_phase: f[5],
            })
        };
        // FC = 5/stop, MDI = 90 limited to FC/FM = 5/500, FM = 500/stop.
        let f = spec([None; 6]).resolve(&timing()).unwrap();
        assert_eq!(f.parameters(), [1., 2., 5., 0.01, 500., 0., 0., 0.]);
        // Explicit FC = 0 stays (then MDI is limited to 0); explicit FM = 0 is
        // replaced; a negative MDI becomes 0.
        let f = spec([Some(0.), Some(3.), Some(0.), None, None, None])
            .resolve(&timing())
            .unwrap();
        assert_eq!(f.parameters()[2..5], [0., 0., 500.]);
        let f = spec([Some(100.), Some(-1.), Some(10.), None, None, None])
            .resolve(&timing())
            .unwrap();
        assert_eq!(f.parameters()[3], 0.);
        // Zero before TD (C returns 0, not VO), a jump at TD.
        let spec = spec([
            Some(100.),
            Some(2.),
            Some(10.),
            Some(0.2),
            Some(90.),
            Some(30.),
        ]);
        assert_eq!(spec.time_zero().unwrap(), 0.);
        let f = spec.resolve(&timing()).unwrap();
        assert_eq!(f.value_at(0.1, R).unwrap(), 0.);
        assert_eq!(f.value_at(0.2, L).unwrap(), 0.);
        let right = 1. + 2. * (30f64.to_radians() + 2. * (90f64.to_radians()).sin()).sin();
        assert!(close(f.value_at(0.2, R).unwrap(), right));
        let s: Real = 0.0123;
        let want = 1.
            + 2. * ((2. * PI * 100. * s + 30f64.to_radians())
                + 2. * (2. * PI * 10. * s + 90f64.to_radians()).sin())
            .sin();
        assert!(close(f.value_at(0.2 + s, L).unwrap(), want));
        assert_eq!(f.corners(), [0.2]);
    }

    #[test]
    fn am_defaults_and_values() {
        let spec = |f: [Option<Real>; 6]| {
            FunctionSpec::Am(AmSpec {
                offset: 0.1,
                carrier_amplitude: 1.,
                modulation_amplitude: f[0],
                modulation_frequency: f[1],
                carrier_frequency: f[2],
                delay: f[3],
                modulation_phase: f[4],
                carrier_phase: f[5],
            })
        };
        let a = spec([None; 6]).resolve(&timing()).unwrap();
        assert_eq!(a.parameters(), [0.1, 1., 1., 5., 500., 0., 0., 0.]);
        // AM replaces only omitted fields: explicit zeros stay.
        let a = spec([Some(0.), Some(0.), Some(0.), None, None, None])
            .resolve(&timing())
            .unwrap();
        assert_eq!(a.parameters()[2..5], [0., 0., 0.]);
        let spec = spec([
            Some(0.5),
            Some(10.),
            Some(1000.),
            Some(0.1),
            Some(45.),
            Some(60.),
        ]);
        assert_eq!(spec.time_zero().unwrap(), 0.);
        let a = spec.resolve(&timing()).unwrap();
        assert_eq!(a.value_at(0.1, L).unwrap(), 0.);
        let s: Real = 0.0031;
        let want = 0.1
            + (1. + 0.5 * (2. * PI * 10. * s + 45f64.to_radians()).sin())
                * (2. * PI * 1000. * s + 60f64.to_radians()).sin();
        assert!(close(a.value_at(0.1 + s, R).unwrap(), want));
        assert!(close(
            a.value_at(0.1, R).unwrap(),
            0.1 + (1. + 0.5 * 45f64.to_radians().sin()) * 60f64.to_radians().sin()
        ));
    }

    #[test]
    fn function_rejections() {
        let base = SineSpec {
            offset: 0.,
            amplitude: 1.,
            frequency: None,
            delay: None,
            damping: None,
            phase: None,
        };
        for bad in [
            SineSpec {
                delay: Some(1.),
                ..base
            },
            SineSpec {
                frequency: Some(1.),
                delay: Some(-1.),
                ..base
            },
            SineSpec {
                offset: Real::NAN,
                ..base
            },
        ] {
            assert!(
                FunctionSpec::Sine(bad).resolve(&timing()).is_err(),
                "{bad:?}"
            );
            assert!(FunctionSpec::Sine(bad).time_zero().is_err(), "{bad:?}");
        }
        let exp = ExpSpec {
            initial: 0.,
            pulsed: 1.,
            rise_delay: Some(1.),
            rise_tau: Some(1.),
            fall_delay: Some(-2.),
            fall_tau: None,
        };
        assert!(FunctionSpec::Exp(exp).validate().is_err());
        // A growing exponential that overflows is an evaluation error.
        let grow = FunctionSpec::Exp(ExpSpec {
            fall_delay: None,
            rise_tau: Some(-1e-3),
            ..exp
        })
        .resolve(&timing())
        .unwrap();
        assert!(grow.value_at(10., R).is_err());
        let s = FunctionSpec::Sine(base).resolve(&timing()).unwrap();
        assert!(s.value_at(Real::NAN, R).is_err());
    }

    #[test]
    fn pwl_delay_and_repeat() {
        let knots = vec![(0., 0.), (1., 1.), (2., 0.5)];
        // td only: shifted, holds the ends.
        let p = PwlSource::new(knots.clone(), 0.5, None).unwrap();
        assert_eq!(p.value_at(0.25, R).unwrap(), 0.);
        assert_eq!(p.value_at(1., R).unwrap(), 0.5);
        assert_eq!(p.value_at(2.5, R).unwrap(), 0.5);
        assert_eq!(p.value_at(9., L).unwrap(), 0.5);
        // r = -1 is C's explicit "no repetition".
        let p = PwlSource::new(knots.clone(), 0., Some(-1.)).unwrap();
        assert_eq!(p.repeat(), None);
        // r = 1: [1, 2] repeats with period 1; a jump 0.5 -> 1 at each boundary.
        let p = PwlSource::new(knots.clone(), 0.5, Some(1.)).unwrap();
        assert_eq!(p.repeat(), Some(1));
        assert_eq!(p.value_at(2.5, L).unwrap(), 0.5);
        assert_eq!(p.value_at(2.5, R).unwrap(), 1.);
        assert_eq!(p.value_at(3., R).unwrap(), 0.75);
        assert_eq!(p.value_at(3.5, L).unwrap(), 0.5);
        assert_eq!(p.value_at(3.5, R).unwrap(), 1.);
        assert_eq!(p.value_at(100.75, R).unwrap(), 0.875);
        let corners: Vec<_> = p.breakpoints_in(0., 5.).unwrap().collect();
        assert_eq!(corners, [0.5, 1.5, 2.5, 3.5, 4.5]);
        // r = 0 repeats everything: a continuous triangle when ends match.
        let tri = PwlSource::new(vec![(0., 0.), (1., 1.), (2., 0.)], 0., Some(0.)).unwrap();
        assert_eq!(tri.value_at(2., L).unwrap(), 0.);
        assert_eq!(tri.value_at(2., R).unwrap(), 0.);
        assert_eq!(tri.value_at(7.5, R).unwrap(), 0.5);
        let corners: Vec<_> = tri.breakpoints_in(3., 6.).unwrap().collect();
        assert_eq!(corners, [3., 4., 5., 6.]);
        // A huge window is enumerated lazily.
        let mut lazy = tri.breakpoints_in(0., 1e9).unwrap();
        assert_eq!(lazy.by_ref().take(4).collect::<Vec<_>>(), [0., 1., 2., 3.]);
        // Mid-run windows skip earlier repetitions without missing corners.
        let window: Vec<_> = tri.breakpoints_in(1000., 1002.5).unwrap().collect();
        assert_eq!(window, [1000., 1001., 1002.]);
    }

    #[test]
    fn pwl_rejections() {
        let knots = vec![(0., 0.), (1., 1.)];
        assert!(PwlSource::new(vec![], 0., None).is_err());
        assert!(PwlSource::new(vec![(1., 0.), (1., 1.)], 0., None).is_err());
        assert!(PwlSource::new(knots.clone(), Real::NAN, None).is_err());
        // r must match a time point and precede the last one (vsrcpar.c).
        assert!(PwlSource::new(knots.clone(), 0., Some(0.5)).is_err());
        assert!(PwlSource::new(knots.clone(), 0., Some(1.)).is_err());
        assert!(PwlSource::new(vec![(0., 1.)], 0., Some(0.)).is_err());
        let p = PwlSource::new(knots, 0., Some(0.)).unwrap();
        assert!(p.value_at(Real::INFINITY, R).is_err());
        assert!(p.breakpoints_in(1., 0.).is_err());
        let tiny = PwlSource::new(vec![(0., 0.), (1e-300, 1.)], 0., Some(0.)).unwrap();
        assert!(tiny.value_at(1e10, R).is_err());
        assert!(tiny.breakpoints_in(0., 1.).is_err());
    }
}
