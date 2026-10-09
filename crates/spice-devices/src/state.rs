//! Trial versus accepted device state.
//!
//! ngspice keeps device history in rotating state vectors: `CKTstate0` is
//! written by every `<dev>load()` call during Newton iterations, and
//! `CKTstate1`, `CKTstate2`, … hold the values at previously accepted time
//! points. `dctran.c` rotates them only after a time point is accepted; a
//! rejected step simply reloads `CKTstate0`.
//!
//! The port makes that ownership explicit:
//!
//! - [`StateHistory`] owns the accepted vectors, most recent first. Only
//!   [`StateHistory::commit`] changes it, and it validates before mutating.
//! - [`TrialState`] is a disposable `CKTstate0`. Every load writes into a trial;
//!   dropping it is a rollback.
//! - [`DeviceState`] is the per-device window a [`crate::Device`] sees while
//!   stamping: read-only accepted history, writable trial slots.
//!
//! Devices stamp through `&self`, so a trial cannot mutate a device either.
//! [`crate::Circuit::accept_point`] runs every device's accept hook before the
//! commit, so a failing hook leaves the history untouched.
//!
//! # Newton phases
//!
//! C's `CKTstate0` also survives from one Newton iteration to the next, and
//! `NIiter` (`niiter.c`) tags each load with an initialization phase
//! (`MODEINITJCT`, `MODEINITFIX`, `MODEINITPRED`/`MODEINITTRAN`,
//! `MODEINITFLOAT`). Devices whose discrete state depends on the previous
//! iterate (the S/W switches, `swload.c`/`cswload.c`) need both. A trial
//! therefore carries its [`IterationPhase`], an optional read-only copy of the
//! previous load's trial values ([`DeviceState::iterate`]) and a
//! nonconvergence flag ([`DeviceState::report_nonconvergence`], C
//! `CKTnoncon++`) that the Newton driver reads. None of this is committed:
//! [`StateHistory::commit`] keeps only the values.
//!
//! # Junction limiting
//!
//! A Newton trial ([`StateHistory::trial_in`]) also allows *device limiting*
//! ([`DeviceState::device_limiting`]): diodes, BJTs and MOS1 then evaluate
//! their junction/FET voltages through C's `DEVpnjlim`/`DEVfetlim`/
//! `DEVlimvds` relative to the previous load's values, start a DC operating
//! point at C's `MODEINITJCT` voltages and report every limited load as
//! nonconvergent (see [`crate::limiting`]). A plain [`StateHistory::trial`]
//! (any load outside a device-limited Newton solve) never limits: the device
//! is evaluated exactly at the supplied solution.
//!
//! # `uic` initial load
//!
//! [`TrialState::with_initial_conditions`] marks the single load C performs
//! for a `.tran ... uic` start (`MODETRANOP | MODEUIC | MODEINITJCT`, after
//! which `NIiter` returns without a solve): nonlinear devices then evaluate
//! at their instance initial-condition voltages (`DIOinitCond`,
//! `BJTicVBE`/`BJTicVCE`, `MOS1icVDS`/`VGS`/`VBS`), defaulted from the node
//! values of the supplied solution as `diogetic.c`/`bjtgetic.c`/`mos1ic.c`
//! do, independently of device limiting.

use std::ops::Range;

use spice_core::{Real, SpiceError, SpiceResult};

/// Number of accepted state vectors retained (C `CKTstate1..CKTstate3`):
/// enough for order-2 integration and truncation-error estimates.
pub const ACCEPTED_DEPTH: usize = 3;

fn state_error(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "device state".to_owned(),
        message: message.into(),
    }
}

/// Accepted device state vectors, most recent first.
#[derive(Debug, Clone, PartialEq)]
pub struct StateHistory {
    len: usize,
    accepted: Vec<Vec<Real>>,
}

impl StateHistory {
    /// An empty history for `len` state slots.
    #[must_use]
    pub const fn new(len: usize) -> Self {
        Self {
            len,
            accepted: Vec::new(),
        }
    }

    /// Number of state slots per vector.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when the circuit has no state slots.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many accepted vectors are available (at most [`ACCEPTED_DEPTH`]).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.accepted.len()
    }

    /// The accepted vector `age` points back: 1 is the latest (`CKTstate1`).
    #[must_use]
    pub fn accepted(&self, age: usize) -> Option<&[Real]> {
        age.checked_sub(1)
            .and_then(|index| self.accepted.get(index))
            .map(Vec::as_slice)
    }

    /// A fresh trial in [`IterationPhase::Junction`] with no previous
    /// iterate and no device limiting: every device is evaluated exactly at
    /// the supplied solution. Its slots start as NaN, so a device that forgets
    /// to write one cannot have it committed.
    #[must_use]
    pub fn trial(&self) -> TrialState {
        TrialState {
            values: vec![Real::NAN; self.len],
            phase: IterationPhase::Junction,
            previous: None,
            nonconvergent: false,
            device_limiting: false,
            initial_conditions: false,
            tolerances: None,
        }
    }

    /// A fresh trial for a Newton load in `phase`, seeing `previous` (the
    /// trial of the preceding load of the same solve, C's `CKTstate0` before
    /// this load) read-only. Slots still start as NaN. Device limiting is
    /// allowed ([`TrialState::with_device_limiting`] turns it off for the
    /// port's legacy global-damping policy).
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when `previous` does not match the history.
    pub fn trial_in(
        &self,
        phase: IterationPhase,
        previous: Option<&TrialState>,
    ) -> SpiceResult<TrialState> {
        if previous.is_some_and(|previous| previous.values.len() != self.len) {
            return Err(state_error("previous iterate does not match the history"));
        }
        Ok(TrialState {
            values: vec![Real::NAN; self.len],
            phase,
            previous: previous.map(|previous| previous.values.clone()),
            nonconvergent: false,
            device_limiting: true,
            initial_conditions: false,
            tolerances: None,
        })
    }

    /// Checks that `trial` could be committed.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a dimension mismatch or an unset or
    /// nonfinite slot.
    pub fn check(&self, trial: &TrialState) -> SpiceResult<()> {
        if trial.values.len() != self.len {
            return Err(state_error(format!(
                "trial has {} slots, history has {}",
                trial.values.len(),
                self.len
            )));
        }
        if let Some(slot) = trial.values.iter().position(|v| !v.is_finite()) {
            return Err(state_error(format!(
                "trial state slot {slot} is unset or nonfinite"
            )));
        }
        Ok(())
    }

    /// Commits `trial` as the newest accepted vector, dropping the oldest
    /// beyond [`ACCEPTED_DEPTH`]. Nothing changes on error.
    ///
    /// # Errors
    ///
    /// As [`Self::check`].
    pub fn commit(&mut self, trial: TrialState) -> SpiceResult<()> {
        self.check(&trial)?;
        self.accepted.insert(0, trial.values);
        self.accepted.truncate(ACCEPTED_DEPTH);
        Ok(())
    }

    /// Forgets every accepted vector, e.g. when a run restarts.
    pub fn clear(&mut self) {
        self.accepted.clear();
    }

    /// The per-device window for slots `range` of `trial`.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when `trial` or `range` do not fit.
    pub fn device<'a>(
        &'a self,
        trial: &'a mut TrialState,
        range: Range<usize>,
    ) -> SpiceResult<DeviceState<'a>> {
        if trial.values.len() != self.len || range.end > self.len || range.start > range.end {
            return Err(state_error("device state range out of bounds"));
        }
        let mut accepted: [&[Real]; ACCEPTED_DEPTH] = [&[]; ACCEPTED_DEPTH];
        for (slot, vector) in accepted.iter_mut().zip(&self.accepted) {
            *slot = &vector[range.clone()];
        }
        let TrialState {
            values,
            phase,
            previous,
            nonconvergent,
            device_limiting,
            initial_conditions,
            tolerances,
        } = trial;
        let previous: Option<&'a [Real]> = match previous {
            Some(vector) => {
                let vector: &'a Vec<Real> = vector;
                Some(&vector[range.clone()])
            }
            None => None,
        };
        Ok(DeviceState {
            previous,
            trial: &mut values[range],
            accepted,
            depth: self.accepted.len(),
            phase: *phase,
            nonconvergent: Some(nonconvergent),
            device_limiting: *device_limiting,
            initial_conditions: *initial_conditions,
            tolerances: *tolerances,
        })
    }
}

/// The Newton initialization phase of a load (C `MODEINITF` bits set by
/// `NIiter` in `niiter.c`, `dctran.c` and `dctrcurv.c`).
///
/// Only devices with discrete, iterate-dependent state read it; everything
/// else stamps identically in every phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IterationPhase {
    /// First load of a DC operating-point solve (`MODEINITJCT`); also the
    /// phase of loads outside Newton iteration.
    #[default]
    Junction,
    /// Later DC operating-point loads until the iterate first converges
    /// (`MODEINITFIX`).
    Fix,
    /// First load of a transient timepoint or of a warm-started DC sweep
    /// point (`MODEINITTRAN`/`MODEINITPRED`), where the accepted history holds
    /// the previous point.
    Predict,
    /// Every other load (`MODEINITFLOAT`).
    Float,
}

/// The trial state vector (C `CKTstate0`) for one load.
#[derive(Debug, Clone, PartialEq)]
pub struct TrialState {
    values: Vec<Real>,
    phase: IterationPhase,
    previous: Option<Vec<Real>>,
    nonconvergent: bool,
    device_limiting: bool,
    initial_conditions: bool,
    tolerances: Option<(Real, Real)>,
}

impl TrialState {
    /// The same trial carrying the Newton solve's `reltol` and current
    /// `abstol`, which C's device convergence tests (`DEVconvTest`, run by
    /// `NIconvTest`) use; see [`DeviceState::convergence_tolerances`].
    #[must_use]
    pub const fn with_convergence_tolerances(mut self, reltol: Real, abstol: Real) -> Self {
        self.tolerances = Some((reltol, abstol));
        self
    }

    /// The same trial marked (or unmarked) as C's `uic` initial load; see
    /// the module documentation and [`DeviceState::initial_conditions`].
    #[must_use]
    pub const fn with_initial_conditions(mut self, enabled: bool) -> Self {
        self.initial_conditions = enabled;
        self
    }

    /// Whether this is the `uic` initial load.
    #[must_use]
    pub const fn initial_conditions(&self) -> bool {
        self.initial_conditions
    }

    /// The same trial with device limiting allowed or forbidden (see
    /// [`DeviceState::device_limiting`]). Newton drivers using the legacy
    /// global voltage-step damping forbid it so devices load exactly.
    #[must_use]
    pub const fn with_device_limiting(mut self, enabled: bool) -> Self {
        self.device_limiting = enabled;
        self
    }

    /// Whether devices may limit their junction voltages in this load.
    #[must_use]
    pub const fn device_limiting(&self) -> bool {
        self.device_limiting
    }

    /// The Newton phase this trial was loaded in.
    #[must_use]
    pub const fn phase(&self) -> IterationPhase {
        self.phase
    }

    /// True when a device reported that this load changed a discrete state
    /// relative to the previous iterate (C `CKTnoncon++`), so the solve must
    /// not be declared converged on it.
    #[must_use]
    pub const fn is_nonconvergent(&self) -> bool {
        self.nonconvergent
    }

    /// The trial values; unset slots are NaN.
    #[must_use]
    pub fn values(&self) -> &[Real] {
        &self.values
    }

    /// The slots of one device, e.g. for an accept hook.
    #[must_use]
    pub fn slice(&self, range: Range<usize>) -> Option<&[Real]> {
        self.values.get(range)
    }
}

/// One device's view of trial and accepted state while it stamps.
#[derive(Debug)]
pub struct DeviceState<'a> {
    trial: &'a mut [Real],
    accepted: [&'a [Real]; ACCEPTED_DEPTH],
    depth: usize,
    phase: IterationPhase,
    previous: Option<&'a [Real]>,
    nonconvergent: Option<&'a mut bool>,
    device_limiting: bool,
    initial_conditions: bool,
    tolerances: Option<(Real, Real)>,
}

impl DeviceState<'_> {
    /// A window with no slots, for loads that track no state.
    #[must_use]
    pub fn none() -> DeviceState<'static> {
        DeviceState {
            trial: &mut [],
            accepted: [&[]; ACCEPTED_DEPTH],
            depth: 0,
            phase: IterationPhase::Junction,
            previous: None,
            nonconvergent: None,
            device_limiting: false,
            initial_conditions: false,
            tolerances: None,
        }
    }

    /// The `(reltol, abstol)` of the Newton solve this load belongs to, when
    /// the driver supplied them
    /// ([`TrialState::with_convergence_tolerances`]).
    #[must_use]
    pub const fn convergence_tolerances(&self) -> Option<(Real, Real)> {
        self.tolerances
    }

    /// The Newton phase of this load.
    #[must_use]
    pub const fn phase(&self) -> IterationPhase {
        self.phase
    }

    /// Whether this load belongs to a device-limited Newton solve: a
    /// nonlinear device may then evaluate C's limited junction voltages
    /// instead of the supplied solution (and must report the load as
    /// nonconvergent when it does). `false` for exact loads.
    #[must_use]
    pub const fn device_limiting(&self) -> bool {
        self.device_limiting
    }

    /// Whether this is C's `uic` initial load
    /// ([`TrialState::with_initial_conditions`]): nonlinear devices evaluate
    /// at their instance initial-condition voltages.
    #[must_use]
    pub const fn initial_conditions(&self) -> bool {
        self.initial_conditions
    }

    /// The value of `slot` written by the previous load of the same Newton
    /// solve (C `CKTstate0` before this load), or `None` for the first load.
    #[must_use]
    pub fn iterate(&self, slot: usize) -> Option<Real> {
        self.previous
            .and_then(|previous| previous.get(slot))
            .copied()
    }

    /// Marks this load as not converged (C `CKTnoncon++`): a discrete state
    /// changed since the previous iterate, so one more iteration is needed.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a window that tracks no trial.
    pub fn report_nonconvergence(&mut self) -> SpiceResult<()> {
        let flag = self
            .nonconvergent
            .as_deref_mut()
            .ok_or_else(|| state_error("nonconvergence reported outside a trial"))?;
        *flag = true;
        Ok(())
    }

    /// Number of slots this device owns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.trial.len()
    }

    /// True when the device owns no slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trial.is_empty()
    }

    /// How many accepted points are available (0 before the first accepted
    /// point, or when the load tracks no history).
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// The trial value of `slot`, NaN when not yet written in this load.
    #[must_use]
    pub fn trial(&self, slot: usize) -> Option<Real> {
        self.trial.get(slot).copied()
    }

    /// Writes the trial value of `slot`.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for an out-of-range slot or a nonfinite value.
    pub fn set(&mut self, slot: usize, value: Real) -> SpiceResult<()> {
        if !value.is_finite() {
            return Err(state_error(format!(
                "nonfinite trial value for slot {slot}"
            )));
        }
        let entry = self
            .trial
            .get_mut(slot)
            .ok_or_else(|| state_error(format!("state slot {slot} out of range")))?;
        *entry = value;
        Ok(())
    }

    /// The accepted value of `slot`, `age` points back (1 is the latest).
    #[must_use]
    pub fn accepted(&self, age: usize, slot: usize) -> Option<Real> {
        age.checked_sub(1)
            .and_then(|index| self.accepted.get(index))
            .and_then(|vector| vector.get(slot))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{ACCEPTED_DEPTH, DeviceState, IterationPhase, StateHistory};

    #[test]
    fn commits_rotate_and_are_bounded() {
        let mut history = StateHistory::new(2);
        assert_eq!(history.depth(), 0);
        assert_eq!(history.accepted(1), None);
        for step in 0_u32..5 {
            let mut trial = history.trial();
            let mut device = history.device(&mut trial, 0..2).unwrap();
            assert_eq!(device.depth(), (step as usize).min(ACCEPTED_DEPTH));
            if step > 0 {
                assert_eq!(device.accepted(1, 0), Some(f64::from(step - 1)));
            }
            device.set(0, f64::from(step)).unwrap();
            device.set(1, -f64::from(step)).unwrap();
            history.commit(trial).unwrap();
        }
        assert_eq!(history.depth(), ACCEPTED_DEPTH);
        assert_eq!(history.accepted(1), Some(&[4.0, -4.0][..]));
        assert_eq!(history.accepted(3), Some(&[2.0, -2.0][..]));
        assert_eq!(history.accepted(0), None);
        assert_eq!(history.accepted(4), None);
        history.clear();
        assert_eq!(history.depth(), 0);
    }

    #[test]
    fn invalid_commits_leave_the_history_unchanged() {
        let mut history = StateHistory::new(2);
        let mut trial = history.trial();
        history
            .device(&mut trial, 0..1)
            .unwrap()
            .set(0, 1.0)
            .unwrap();
        // Slot 1 is still unset.
        let error = history.commit(trial.clone()).unwrap_err();
        assert!(error.to_string().contains("slot 1 is unset"), "{error}");
        assert_eq!(history.depth(), 0);
        assert!(history.commit(StateHistory::new(3).trial()).is_err());
        assert_eq!(history.depth(), 0);
    }

    #[test]
    fn device_windows_are_bounded_and_finite() {
        let history = StateHistory::new(3);
        let mut trial = history.trial();
        assert!(history.device(&mut trial, 2..4).is_err());
        let mut short = StateHistory::new(1).trial();
        assert!(history.device(&mut short, 0..1).is_err());
        let mut device = history.device(&mut trial, 1..3).unwrap();
        assert_eq!(device.len(), 2);
        assert!(device.trial(0).unwrap().is_nan());
        assert!(device.set(2, 1.0).is_err());
        assert!(device.set(0, f64::INFINITY).is_err());
        device.set(1, 5.0).unwrap();
        assert_eq!(trial.values()[2], 5.0);
        assert_eq!(trial.slice(2..3), Some(&[5.0][..]));
        let mut none = DeviceState::none();
        assert!(none.is_empty());
        assert_eq!(none.accepted(1, 0), None);
        assert_eq!(none.iterate(0), None);
        assert!(none.report_nonconvergence().is_err());
    }

    #[test]
    fn iterates_are_read_only_and_never_committed() {
        let mut history = StateHistory::new(2);
        let mut first = history.trial();
        assert_eq!(first.phase(), IterationPhase::Junction);
        {
            let mut device = history.device(&mut first, 0..2).unwrap();
            assert_eq!(device.iterate(0), None);
            device.set(0, 1.0).unwrap();
            device.set(1, 2.0).unwrap();
        }
        assert!(!first.is_nonconvergent());
        let mut second = history
            .trial_in(IterationPhase::Float, Some(&first))
            .unwrap();
        {
            let mut device = history.device(&mut second, 1..2).unwrap();
            assert_eq!(device.phase(), IterationPhase::Float);
            assert_eq!(device.iterate(0), Some(2.0));
            assert!(device.trial(0).unwrap().is_nan());
            device.report_nonconvergence().unwrap();
            device.set(0, 3.0).unwrap();
        }
        assert!(second.is_nonconvergent());
        history
            .device(&mut second, 0..1)
            .unwrap()
            .set(0, 4.0)
            .unwrap();
        history.commit(second).unwrap();
        assert_eq!(history.accepted(1), Some(&[4.0, 3.0][..]));
        assert!(
            history
                .trial_in(IterationPhase::Fix, Some(&StateHistory::new(3).trial()))
                .is_err()
        );
    }
}
