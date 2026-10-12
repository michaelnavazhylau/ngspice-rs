//! Device-owned delay history and device-driven transient breakpoints.
//!
//! Some devices read their own accepted terminal waveforms a fixed time back:
//! the lossless transmission line (`T`, C `tra/`, `TRAdelays`) interpolates
//! the wave launched at the far port `TD` earlier, and the lossy line (`O`,
//! C `ltra/`) convolves its whole accepted history. That history is device
//! state in C (an instance array written by `TRAaccept`), but it is not a
//! fixed set of `CKTstate` slots: its length grows with the accepted points
//! and is pruned once older than the delay. The port therefore keeps it next
//! to the fixed slots, in [`crate::devices::StateHistory`], under the same
//! ownership rules (see `docs/port/TRANSIENT.md`, "Device-driven
//! breakpoints"):
//!
//! * a load (trial) only **reads** the history
//!   ([`crate::devices::DeviceState::delay_history`]);
//! * only an accepted transient point changes it, through a validated
//!   [`DelayUpdate`] that is applied after the fixed state is committed
//!   ([`crate::devices::Circuit::accept_transient_point`]). Rejected trials,
//!   Newton iterations and interpolated output samples never touch it;
//! * an accepted point may also request future breakpoints (C
//!   `CKTsetBreak` from `DEVaccept`), which the companion driver merges with
//!   the source breakpoints and the run's stop time;
//! * a converged trial may bound the next step (C `DEVtrunc`) through
//!   [`DelayLine::timestep_limit`].
//!
//! A device opts in through [`crate::devices::Device::delay_line`].

use std::collections::VecDeque;
use std::fmt;

use crate::maths::Vector;
use crate::primitives::{NodeId, Real, SpiceError, SpiceResult};

use crate::devices::traits::MnaUnknowns;

fn delay_error(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "delay history".to_owned(),
        message: message.into(),
    }
}

/// Accepted samples of one delay device, oldest first (C `TRAdelays`: rows of
/// `time, values...`). Times are finite and strictly increasing; every sample
/// holds [`Self::width`] finite values.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DelayHistory {
    width: usize,
    times: VecDeque<Real>,
    values: VecDeque<Real>,
}

impl DelayHistory {
    /// An empty history with `width` values per sample.
    #[must_use]
    pub const fn new(width: usize) -> Self {
        Self {
            width,
            times: VecDeque::new(),
            values: VecDeque::new(),
        }
    }

    /// Values per sample.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Number of samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.times.len()
    }

    /// True before the transient start initialized the history.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// The time of sample `index` (0 is the oldest).
    #[must_use]
    pub fn time(&self, index: usize) -> Option<Real> {
        self.times.get(index).copied()
    }

    /// Value `component` of sample `index`.
    #[must_use]
    pub fn value(&self, index: usize, component: usize) -> Option<Real> {
        if component >= self.width {
            return None;
        }
        self.values.get(index * self.width + component).copied()
    }

    /// The newest sample's time.
    #[must_use]
    pub fn last_time(&self) -> Option<Real> {
        self.times.back().copied()
    }

    /// Checks that `update` can be applied: matching widths, finite values,
    /// strictly increasing times and a drop count that leaves the appended
    /// sample after every kept one.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] describing the first violation.
    pub fn check(&self, update: &DelayUpdate) -> SpiceResult<()> {
        let check_sample = |time: Real, values: &[Real]| {
            if values.len() != self.width {
                return Err(delay_error(format!(
                    "sample has {} values, history has {}",
                    values.len(),
                    self.width
                )));
            }
            if !time.is_finite() || values.iter().any(|v| !v.is_finite()) {
                return Err(delay_error("nonfinite delay sample"));
            }
            Ok(())
        };
        let mut last = None;
        if let Some(reset) = &update.reset {
            for (time, values) in reset {
                check_sample(*time, values)?;
                if last.is_some_and(|previous| *time <= previous) {
                    return Err(delay_error("delay samples must have increasing times"));
                }
                last = Some(*time);
            }
        } else {
            if update.drop_front > self.len() {
                return Err(delay_error(format!(
                    "cannot drop {} of {} delay samples",
                    update.drop_front,
                    self.len()
                )));
            }
            last = self.last_time();
            if update.drop_front == self.len() {
                last = None;
            }
        }
        if let Some((time, values)) = &update.append {
            check_sample(*time, values)?;
            if last.is_some_and(|previous| *time <= previous) {
                return Err(delay_error(format!(
                    "appended delay sample at {time:e} is not after the last one"
                )));
            }
        }
        if update.breakpoints.iter().any(|t| !t.is_finite()) {
            return Err(delay_error("nonfinite breakpoint request"));
        }
        Ok(())
    }

    /// Applies an update [`Self::check`] accepted.
    ///
    /// # Errors
    ///
    /// As [`Self::check`]; nothing changes on error.
    pub(crate) fn apply(&mut self, update: &DelayUpdate) -> SpiceResult<()> {
        self.check(update)?;
        if let Some(reset) = &update.reset {
            self.times.clear();
            self.values.clear();
            for (time, values) in reset {
                self.times.push_back(*time);
                self.values.extend(values.iter().copied());
            }
        } else {
            self.times.drain(..update.drop_front);
            self.values.drain(..update.drop_front * self.width);
        }
        if let Some((time, values)) = &update.append {
            self.times.push_back(*time);
            self.values.extend(values.iter().copied());
        }
        Ok(())
    }

    /// Forgets every sample (a restarted run).
    pub(crate) fn clear(&mut self) {
        self.times.clear();
        self.values.clear();
    }
}

/// What one accepted transient point does to one device's [`DelayHistory`].
///
/// Applied atomically after the fixed device state is committed: first
/// `reset` (the transient start) or `drop_front`, then `append`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DelayUpdate {
    /// Replace the whole history with these samples (C `MODEINITTRAN`
    /// initialization of `TRAdelays`).
    pub reset: Option<Vec<(Real, Vec<Real>)>>,
    /// Drop this many oldest samples (C `TRAaccept`'s shift).
    pub drop_front: usize,
    /// Append this sample.
    pub append: Option<(Real, Vec<Real>)>,
    /// Breakpoints requested for later times (C `CKTsetBreak`).
    pub breakpoints: Vec<Real>,
}

/// The step sizes a delay device sees, C `CKTdeltaOld[0..3]`: during a trial
/// `[trial step, previous accepted step, the one before]`, at acceptance
/// `[accepted step, previous accepted step, the one before]`. Steps that do
/// not exist yet read as the run's maximum step, as `dctran.c` initializes
/// `CKTdeltaOld` with `CKTmaxStep`.
pub type DeltaOld = [Real; 3];

/// Everything a [`DelayLine`] sees at a transient start, an accepted point or
/// a converged trial.
#[derive(Debug, Clone, Copy)]
pub struct DelayContext<'a> {
    /// The accepted (or converged trial) solution.
    pub solution: &'a Vector,
    /// Node numbering, ground eliminated.
    pub unknowns: &'a MnaUnknowns,
    /// The accepted (or trial) time.
    pub time: Real,
    /// C `CKTdeltaOld[0..3]` ([`DeltaOld`]).
    pub steps: DeltaOld,
    /// C `CKTminBreak` as `TRAaccept` uses it: a sample closer than this to
    /// the previous one is not recorded. (The driver merges breakpoints with
    /// its own threshold.)
    pub min_break: Real,
    /// The run starts from `uic` initial conditions (C `MODEUIC`).
    pub initial_conditions: bool,
}

impl DelayContext<'_> {
    /// The solution's voltage at `node`, zero at ground.
    #[must_use]
    pub fn node_voltage(&self, node: NodeId) -> Real {
        self.unknowns
            .node_row(node)
            .and_then(|row| self.solution.get(row))
            .unwrap_or(0.)
    }
}

/// A device with an accepted-waveform delay history (see the module
/// documentation). Every method is a pure function of its arguments: the
/// device never mutates itself or the history.
pub trait DelayLine: fmt::Debug {
    /// Values recorded per accepted point.
    fn width(&self) -> usize;

    /// The history at the transient start (`context.time` is 0), from the
    /// initial operating point or, under `uic`, the instance initial
    /// conditions. Must set [`DelayUpdate::reset`].
    ///
    /// # Errors
    /// Device-specific failures.
    fn start(&self, context: &DelayContext<'_>) -> SpiceResult<DelayUpdate>;

    /// The update for an accepted point after the start.
    ///
    /// # Errors
    /// Device-specific failures (for instance a history too short).
    fn accept(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<DelayUpdate>;

    /// An upper bound on the next step from a converged trial (C
    /// `DEVtrunc`), or `None`. It may be at or below zero, which rejects the
    /// trial like any truncation bound below `0.9 dt`.
    ///
    /// # Errors
    /// Device-specific failures.
    fn timestep_limit(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<Option<Real>>;
}

#[cfg(test)]
mod tests {
    use super::{DelayHistory, DelayUpdate};

    fn sample(time: f64, value: f64) -> (f64, Vec<f64>) {
        (time, vec![value, -value])
    }

    #[test]
    fn updates_reset_drop_and_append_atomically() {
        let mut history = DelayHistory::new(2);
        assert!(history.is_empty());
        history
            .apply(&DelayUpdate {
                reset: Some(vec![sample(-2., 1.), sample(-1., 1.), sample(0., 1.)]),
                ..DelayUpdate::default()
            })
            .unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history.time(0), Some(-2.));
        assert_eq!(history.value(2, 1), Some(-1.));
        assert_eq!(history.value(2, 2), None);
        history
            .apply(&DelayUpdate {
                drop_front: 1,
                append: Some(sample(0.5, 3.)),
                ..DelayUpdate::default()
            })
            .unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history.time(0), Some(-1.));
        assert_eq!(history.last_time(), Some(0.5));
        assert_eq!(history.value(2, 0), Some(3.));
        let before = history.clone();
        for bad in [
            DelayUpdate {
                append: Some(sample(0.5, 1.)),
                ..DelayUpdate::default()
            },
            DelayUpdate {
                drop_front: 4,
                ..DelayUpdate::default()
            },
            DelayUpdate {
                append: Some((1., vec![1.])),
                ..DelayUpdate::default()
            },
            DelayUpdate {
                append: Some(sample(1., f64::NAN)),
                ..DelayUpdate::default()
            },
            DelayUpdate {
                reset: Some(vec![sample(0., 1.), sample(0., 1.)]),
                ..DelayUpdate::default()
            },
            DelayUpdate {
                breakpoints: vec![f64::INFINITY],
                ..DelayUpdate::default()
            },
        ] {
            assert!(history.apply(&bad).is_err(), "{bad:?}");
            assert_eq!(history, before);
        }
        history.clear();
        assert!(history.is_empty());
    }
}
