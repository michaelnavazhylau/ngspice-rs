//! Newton junction/FET voltage limiting shared by the nonlinear devices.
//!
//! C references: `src/spicelib/devices/devsup.c` (`DEVpnjlim`, `DEVfetlim`,
//! `DEVlimvds`), the "limit nonlinear branch voltages" blocks and the
//! `MODEINITJCT` start voltages of `dio/dioload.c`, `bjt/bjtload.c` and
//! `mos1/mos1load.c`, and the critical voltages of `diotemp.c`, `bjttemp.c`
//! and `mos1temp.c` (`vt * log(vt / (sqrt(2) * Is))`).
//!
//! # The device hook
//!
//! A device that limits its controlling voltages does so in its ordinary
//! [`crate::Device::stamp`] through a [`Limiter`] built from its
//! [`DeviceState`]:
//!
//! 1. [`Limiter::new`] reads the load's mode ([`Linearization`]): exact
//!    outside a device-limited Newton solve, C's `MODEINITJCT` start voltages
//!    in the first DC operating-point load ([`crate::IterationPhase::Junction`]),
//!    otherwise limiting relative to the previous load's values.
//! 2. [`Limiter::previous`] returns the value the device stored in a state slot
//!    during the previous load of the same solve (C `CKTstate0`), or, in the
//!    predictor phase of a transient timepoint or warm-started `.dc` point, the
//!    last *accepted* value (C copies `CKTstate1` into `CKTstate0` there).
//! 3. [`Limiter::pn_junction`], [`Limiter::fet_gate`] and
//!    [`Limiter::drain_source`] apply `DEVpnjlim`, `DEVfetlim` and
//!    `DEVlimvds`; the device then evaluates (and linearises) its equations at
//!    the limited voltages, exactly as C does.
//! 4. [`Limiter::finish`] stores the voltages for the next load and, when any
//!    voltage was limited (or this is the `MODEINITJCT` load), marks the load
//!    nonconvergent (C `CKTnoncon++`), so the Newton driver never declares
//!    convergence on a load that was not evaluated at the iterate itself.
//!
//! The last rule is how limiting stays compatible with the port's
//! physical-residual convergence test: a converged iterate is always reloaded
//! with limiting inactive, i.e. exactly at the returned solution.
use crate::{DeviceState, IterationPhase};
use spice_core::{Real, SpiceResult};

/// `sqrt(2)` (C `CONSTroot2`).
const ROOT2: Real = std::f64::consts::SQRT_2;

/// The critical junction voltage `vt * ln(vt / (sqrt(2) * is))` above which
/// [`pnjlim`] limits forward steps (C `DIOtVcrit`, `BJTtVcrit`,
/// `MOS1sourceVcrit`/`MOS1drainVcrit`).
#[must_use]
pub fn critical_voltage(vt: Real, saturation_current: Real) -> Real {
    vt * (vt / (ROOT2 * saturation_current)).ln()
}

/// A [`pnjlim`] result: the voltage to evaluate and C's `icheck` flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limited {
    /// The (possibly limited) junction voltage.
    pub value: Real,
    /// C's `icheck`: the step was limited.
    pub limited: bool,
}

/// `DEVpnjlim`: logarithmic limiting of a forward PN-junction step beyond
/// `vcrit`, and Gillespie's limit on large negative steps.
#[must_use]
pub fn pnjlim(vnew: Real, vold: Real, vt: Real, vcrit: Real) -> Limited {
    if vnew > vcrit && (vnew - vold).abs() > vt + vt {
        let value = if vold > 0. {
            let arg = (vnew - vold) / vt;
            if arg > 0. {
                vold + vt * (2. + (arg - 2.).ln())
            } else {
                vold - vt * (2. + (2. - arg).ln())
            }
        } else {
            vt * (vnew / vt).ln()
        };
        return Limited {
            value,
            limited: true,
        };
    }
    if vnew < 0. {
        let floor = if vold > 0. {
            -vold - 1.
        } else {
            2. * vold - 1.
        };
        if vnew < floor {
            return Limited {
                value: floor,
                limited: true,
            };
        }
    }
    Limited {
        value: vnew,
        limited: false,
    }
}

/// `DEVfetlim`: limits the per-iteration change of a FET gate voltage
/// relative to the threshold `vto` (Gillespie's `vtstlo`).
#[must_use]
pub fn fetlim(vnew: Real, vold: Real, vto: Real) -> Real {
    let vtsthi = (2. * (vold - vto)).abs() + 2.;
    let vtstlo = (vold - vto).abs() + 1.;
    let vtox = vto + 3.5;
    let delv = vnew - vold;
    if vold >= vto {
        if vold >= vtox {
            if delv <= 0. {
                // Going off.
                if vnew >= vtox {
                    if -delv > vtstlo {
                        return vold - vtstlo;
                    }
                    vnew
                } else {
                    vnew.max(vto + 2.)
                }
            } else if delv >= vtsthi {
                // Staying on.
                vold + vtsthi
            } else {
                vnew
            }
        } else if delv <= 0. {
            // Middle region, decreasing.
            vnew.max(vto - 0.5)
        } else {
            // Middle region, increasing.
            vnew.min(vto + 4.)
        }
    } else if delv <= 0. {
        // Off.
        if -delv > vtsthi { vold - vtsthi } else { vnew }
    } else {
        let vtemp = vto + 0.5;
        if vnew <= vtemp {
            if delv > vtstlo { vold + vtstlo } else { vnew }
        } else {
            vtemp
        }
    }
}

/// `DEVlimvds`: limits the per-iteration change of a FET drain-source voltage.
#[must_use]
pub fn limvds(vnew: Real, vold: Real) -> Real {
    if vold >= 3.5 {
        if vnew > vold {
            vnew.min(3. * vold + 2.)
        } else if vnew < 3.5 {
            vnew.max(2.)
        } else {
            vnew
        }
    } else if vnew > vold {
        vnew.min(4.)
    } else {
        vnew.max(-0.5)
    }
}

/// Which voltages a limiting device linearises around in one load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linearization {
    /// Exactly the supplied solution: loads outside a device-limited Newton
    /// solve (output reloads, AC bias, the legacy global-damping policy).
    Exact,
    /// The first load of a DC operating point (C `MODEINITJCT`): the device's
    /// start voltages (`tVcrit` for forward junctions), independent of the
    /// seed. The load is always nonconvergent.
    Initial,
    /// Every other Newton load: the solution's voltages, limited relative to
    /// [`Limiter::previous`].
    Limited,
}

/// One device's limiting bookkeeping for one load; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limiter {
    mode: Linearization,
    nonconvergent: bool,
}

impl Limiter {
    /// The limiting mode of the load `states` belongs to.
    #[must_use]
    pub fn new(states: &DeviceState<'_>) -> Self {
        let mode = if !states.device_limiting() {
            Linearization::Exact
        } else if states.phase() == IterationPhase::Junction {
            Linearization::Initial
        } else {
            Linearization::Limited
        };
        Self {
            mode,
            nonconvergent: mode == Linearization::Initial,
        }
    }

    /// The mode chosen by [`Self::new`].
    #[must_use]
    pub const fn mode(&self) -> Linearization {
        self.mode
    }

    /// The reference value of a limited voltage stored in `slot` (device
    /// relative): the previous load's value in this solve, or in
    /// [`IterationPhase::Predict`] the latest accepted value. `None` (no
    /// limiting) without such a value or outside [`Linearization::Limited`].
    #[must_use]
    pub fn previous(&self, states: &DeviceState<'_>, slot: usize) -> Option<Real> {
        if self.mode != Linearization::Limited {
            return None;
        }
        match states.phase() {
            IterationPhase::Predict => states.accepted(1, slot),
            _ => states.iterate(slot),
        }
        .filter(|value| value.is_finite())
    }

    /// `DEVpnjlim` of `raw` against `previous` (no limiting without one);
    /// a limited step makes the load nonconvergent.
    pub fn pn_junction(
        &mut self,
        raw: Real,
        previous: Option<Real>,
        vt: Real,
        vcrit: Real,
    ) -> Real {
        let Some(old) = previous else {
            return raw;
        };
        let limited = pnjlim(raw, old, vt, vcrit);
        self.nonconvergent |= limited.limited;
        limited.value
    }

    /// `DEVfetlim` of a gate voltage `raw` against `previous` and threshold
    /// `von`; any change makes the load nonconvergent.
    pub fn fet_gate(&mut self, raw: Real, previous: Real, von: Real) -> Real {
        self.changed(raw, fetlim(raw, previous, von))
    }

    /// `DEVlimvds` of a drain-source voltage `raw` against `previous`; any
    /// change makes the load nonconvergent.
    pub fn drain_source(&mut self, raw: Real, previous: Real) -> Real {
        self.changed(raw, limvds(raw, previous))
    }

    fn changed(&mut self, raw: Real, value: Real) -> Real {
        // C flags only `DEVpnjlim` steps (`Check`) and leaves FET limiting to
        // the device convergence tests (`mos1conv.c`); the port refuses
        // convergence on any limited load so the returned point is always an
        // exact, physically checked evaluation.
        self.nonconvergent |= value != raw;
        value
    }

    /// Whether the load is nonconvergent so far.
    #[must_use]
    pub const fn is_nonconvergent(&self) -> bool {
        self.nonconvergent
    }

    /// Store each `(slot, voltage)` for the next load and report a
    /// nonconvergent load (C `CKTnoncon++`).
    ///
    /// # Errors
    /// An out-of-range slot or nonfinite voltage.
    pub fn finish(self, states: &mut DeviceState<'_>, record: &[(usize, Real)]) -> SpiceResult<()> {
        for (slot, value) in record {
            states.set(*slot, *value)?;
        }
        if self.nonconvergent {
            states.report_nonconvergence()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateHistory;

    const VT: Real = crate::nonlinear::K_OVER_Q * 300.15;

    #[test]
    fn pnjlim_follows_devsup() {
        let vcrit = critical_voltage(VT, 1e-14);
        assert!((vcrit - VT * (VT / (ROOT2 * 1e-14)).ln()).abs() < 1e-15);
        // Small steps and steps below vcrit pass unchanged.
        assert_eq!(
            pnjlim(0.6, 0.59, VT, vcrit),
            Limited {
                value: 0.6,
                limited: false
            }
        );
        assert_eq!(
            pnjlim(0.5, 0.0, VT, vcrit),
            Limited {
                value: 0.5,
                limited: false
            }
        );
        // Forward step from a positive voltage: vold + vt (2 + ln(arg - 2)).
        let step = pnjlim(5., 0.6, VT, vcrit);
        let arg: Real = (5. - 0.6) / VT;
        assert!(step.limited);
        assert!((step.value - (0.6 + VT * (2. + (arg - 2.).ln()))).abs() < 1e-15);
        // From a non-positive voltage: vt ln(vnew / vt).
        let step = pnjlim(5., -1., VT, vcrit);
        assert!((step.value - VT * (5. / VT).ln()).abs() < 1e-15);
        // Downward step above vcrit (arg <= 0).
        let step = pnjlim(0.9, 2., VT, vcrit);
        let arg: Real = (0.9 - 2.) / VT;
        assert!((step.value - (2. - VT * (2. + (2. - arg).ln()))).abs() < 1e-15);
        // Gillespie's negative limits.
        assert_eq!(
            pnjlim(-10., 0.5, VT, vcrit),
            Limited {
                value: -1.5,
                limited: true
            }
        );
        assert_eq!(
            pnjlim(-10., -1., VT, vcrit),
            Limited {
                value: -3.,
                limited: true
            }
        );
        assert_eq!(
            pnjlim(-2., -1., VT, vcrit),
            Limited {
                value: -2.,
                limited: false
            }
        );
    }

    #[test]
    fn fetlim_and_limvds_follow_devsup() {
        let vto = 1.;
        // Off, rising past vto + 0.5: clamped there.
        assert_eq!(fetlim(5., 0., vto), 1.5);
        // Off, rising within vto + 0.5: unchanged (a rise beyond vtstlo
        // would end above vto + 1, so Gillespie's vtstlo never binds here).
        assert_eq!(fetlim(1.4, -3., vto), 1.4);
        // Off, falling by more than vtsthi = 2|vold - vto| + 2.
        assert_eq!(fetlim(-10., 0., vto), -4.);
        // Middle region: bounded by vto - 0.5 and vto + 4.
        assert_eq!(fetlim(-5., 2., vto), 0.5);
        assert_eq!(fetlim(10., 2., vto), 5.);
        // Strongly on, going off below vtox: at least vto + 2.
        assert_eq!(fetlim(0., 5., vto), 3.);
        // Strongly on, going off but staying above vtox: unchanged.
        assert_eq!(fetlim(4.6, 10., vto), 4.6);
        assert_eq!(fetlim(4.6, 6., vto), 4.6);
        // Strongly on, rising by more than vtsthi = 2|10 - 1| + 2 = 20.
        assert_eq!(fetlim(40., 10., vto), 30.);
        assert_eq!(limvds(10., 0.), 4.);
        assert_eq!(limvds(-10., 0.), -0.5);
        assert_eq!(limvds(20., 4.), 14.);
        assert_eq!(limvds(0., 4.), 2.);
        assert_eq!(limvds(3.6, 4.), 3.6);
    }

    #[test]
    fn limiter_modes_follow_the_trial() {
        let history = StateHistory::new(1);
        let mut exact = history.trial();
        let states = history.device(&mut exact, 0..1).unwrap();
        let limiter = Limiter::new(&states);
        assert_eq!(limiter.mode(), Linearization::Exact);
        assert_eq!(limiter.previous(&states, 0), None);

        let mut first = history.trial_in(IterationPhase::Junction, None).unwrap();
        let mut states = history.device(&mut first, 0..1).unwrap();
        let limiter = Limiter::new(&states);
        assert_eq!(limiter.mode(), Linearization::Initial);
        limiter.finish(&mut states, &[(0, 0.7)]).unwrap();
        assert!(first.is_nonconvergent());

        let mut second = history.trial_in(IterationPhase::Fix, Some(&first)).unwrap();
        let mut states = history.device(&mut second, 0..1).unwrap();
        let mut limiter = Limiter::new(&states);
        assert_eq!(limiter.previous(&states, 0), Some(0.7));
        let vcrit = critical_voltage(VT, 1e-14);
        assert_eq!(limiter.pn_junction(0.71, Some(0.7), VT, vcrit), 0.71);
        assert!(!limiter.is_nonconvergent());
        limiter.finish(&mut states, &[(0, 0.71)]).unwrap();
        assert!(!second.is_nonconvergent());

        let legacy = history
            .trial_in(IterationPhase::Float, Some(&second))
            .unwrap()
            .with_device_limiting(false);
        assert!(!legacy.device_limiting());
    }
}
