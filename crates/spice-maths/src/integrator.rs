//! SPICE companion-model integration of charge-storage elements.
//!
//! When a capacitor or inductor is stamped into the MNA matrix, its current
//! depends on the history of its charge (or flux). ngspice turns that
//! dependence into an equivalent conductance plus a known current source — the
//! "companion model" — using a numerical integration rule:
//!
//! - **Trapezoidal** (order 2, the default), with backward-Euler order 1 used
//!   at startup and after breakpoints;
//! - **Gear** (variable-step backward differentiation), selected by
//!   `.option method=gear`.
//!
//! The port follows `src/maths/ni/nicomcof.c` (coefficients `CKTag`/`CKTagp`),
//! `src/maths/ni/niinteg.c` (`NIintegrate`), `src/maths/ni/nipred.c`
//! (`NIpred`) and `src/spicelib/analysis/cktterr.c` (`CKTterr`). Only orders 1
//! and 2 are implemented: [`IntegrationMethod`] still *represents* Gear orders
//! up to 6, but [`IntegrationMethod::validate_runtime`] rejects orders 3–6.
//!
//! # Trial versus accepted state
//!
//! [`StepHistory`] holds the step sizes of *accepted* time points, most recent
//! first (C `CKTdeltaOld[1..]`). [`StepHistory::trial`] builds immutable
//! [`Coefficients`] for a proposed step without touching the history, so any
//! number of rejected or repeated trials leave it intact. Only
//! [`StepHistory::accept`] advances it. The same rule applies to the charge and
//! derivative histories, which are owned by the caller (devices) and passed in
//! as slices.
//!
//! The separately selected [`crate::diffsol`] BDF adapter does not implement
//! this companion-model contract and never consumes these coefficients.

use spice_core::{Real, SpiceError, SpiceResult};

/// Which integration rule to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntegrationMethod {
    /// Second-order trapezoidal rule. ngspice's default.
    #[default]
    Trapezoidal,
    /// Gear's backward differentiation formula, representable up to order 6.
    Gear {
        /// Maximum integration order (`.option maxord`).
        order: u8,
    },
}

impl IntegrationMethod {
    /// The highest Gear order ngspice supports.
    pub const MAX_GEAR_ORDER: u8 = 6;

    /// The highest order with implemented coefficient/history operations.
    pub const MAX_RUNTIME_ORDER: u8 = 2;

    /// True when this method/order is representable, not proof that its
    /// coefficient/history operations are implemented. See
    /// [`Self::validate_runtime`].
    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self {
            Self::Trapezoidal => true,
            Self::Gear { order } => order >= 1 && order <= Self::MAX_GEAR_ORDER,
        }
    }

    /// The highest order this method may run at: 2 for trapezoidal, the
    /// configured order for Gear.
    #[must_use]
    pub const fn max_order(self) -> u8 {
        match self {
            Self::Trapezoidal => 2,
            Self::Gear { order } => order,
        }
    }

    /// Checks that this method's maximum order is implemented.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Unsupported`] for Gear orders 0 and 3–6.
    pub fn validate_runtime(self) -> SpiceResult<()> {
        let order = self.max_order();
        if (1..=Self::MAX_RUNTIME_ORDER).contains(&order) {
            Ok(())
        } else {
            Err(SpiceError::Unsupported {
                feature: format!(
                    "{} integration order {order}; only orders 1 and 2 are implemented",
                    self.as_str()
                ),
                location: None,
            })
        }
    }

    /// The name used by `.option method=`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trapezoidal => "trap",
            Self::Gear { .. } => "gear",
        }
    }

    /// Parses `.option method=trap|gear` together with `maxord`.
    #[must_use]
    pub fn parse(name: &str, gear_order: u8) -> Option<Self> {
        if name.eq_ignore_ascii_case("trap") || name.eq_ignore_ascii_case("trapezoidal") {
            Some(Self::Trapezoidal)
        } else if name.eq_ignore_ascii_case("gear") {
            Some(Self::Gear { order: gear_order })
        } else {
            None
        }
    }
}

/// The ngspice default trapezoidal weighting, `.option xmu=0.5`
/// (`cktntask.c`). `0` degenerates to backward Euler.
pub const DEFAULT_XMU: Real = 0.5;

/// Number of accepted step sizes retained: enough for order-2 prediction and
/// truncation-error estimation.
const HISTORY_LEN: usize = IntegrationMethod::MAX_RUNTIME_ORDER as usize + 1;

fn numerical(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "integrator".to_owned(),
        message: message.into(),
    }
}

fn finite(value: Real, what: &str) -> SpiceResult<Real> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(numerical(format!("{what} is not finite")))
    }
}

/// Accepted step sizes, most recent first — C `CKTdeltaOld[1..]`.
///
/// This is the only integration state that advances between time points, and
/// it advances only through [`Self::accept`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepHistory {
    accepted: [Real; HISTORY_LEN],
    len: usize,
}

impl StepHistory {
    /// A history with no accepted steps.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            accepted: [0.0; HISTORY_LEN],
            len: 0,
        }
    }

    /// Accepted step sizes, most recent first.
    #[must_use]
    pub fn accepted(&self) -> &[Real] {
        &self.accepted[..self.len]
    }

    /// The number of retained accepted steps.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True before any step was accepted.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The highest order `method` can use for a trial right now. Gear order
    /// `k` needs `k - 1` previous steps; trapezoidal order 2 needs a previous
    /// derivative, which exists once one step was accepted.
    #[must_use]
    pub fn available_order(&self, method: IntegrationMethod) -> u8 {
        let by_history = u8::try_from(self.len + 1).unwrap_or(u8::MAX);
        method
            .max_order()
            .min(by_history)
            .min(IntegrationMethod::MAX_RUNTIME_ORDER)
    }

    /// Builds the coefficients for a trial step of size `dt`, without
    /// changing this history. `xmu` only affects trapezoidal order 2.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a nonpositive/nonfinite step, `xmu`
    /// outside `[0, 0.5]`, nonfinite coefficients or a history too short for
    /// the order; [`SpiceError::Unsupported`] for orders other than 1 and 2 or
    /// above the method's maximum.
    pub fn trial(
        &self,
        method: IntegrationMethod,
        order: u8,
        dt: Real,
        xmu: Real,
    ) -> SpiceResult<Coefficients> {
        if !(dt.is_finite() && dt > 0.0) {
            return Err(numerical(format!(
                "timestep must be positive and finite, got {dt}"
            )));
        }
        if !(xmu.is_finite() && (0.0..=0.5).contains(&xmu)) {
            return Err(numerical(format!("xmu must be in [0, 0.5], got {xmu}")));
        }
        if !(1..=IntegrationMethod::MAX_RUNTIME_ORDER).contains(&order)
            || order > method.max_order()
        {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "{} integration at order {order} (maximum {}; implemented 1..=2)",
                    method.as_str(),
                    method.max_order()
                ),
                location: None,
            });
        }
        if order > self.available_order(method) {
            return Err(numerical(format!(
                "order {order} needs {} accepted step(s), have {}",
                order - 1,
                self.len
            )));
        }
        let mut deltas = [0.0; HISTORY_LEN + 1];
        deltas[0] = dt;
        deltas[1..=self.len].copy_from_slice(self.accepted());
        // nicomcof.c
        let ag = match (method, order) {
            (_, 1) => [1.0 / dt, -1.0 / dt, 0.0],
            (IntegrationMethod::Trapezoidal, _) => [1.0 / dt / (1.0 - xmu), xmu / (1.0 - xmu), 0.0],
            (IntegrationMethod::Gear { .. }, _) => {
                // Exact for quadratics on t_{n+1}, t_n, t_{n-1}: the closed
                // form of the Vandermonde solve in nicomcof.c.
                let h1 = deltas[1];
                let r = 1.0 + h1 / dt;
                let a2 = 1.0 / (r * h1);
                let a1 = -a2 * r * r;
                [-(a1 + a2), a1, a2]
            }
        };
        for value in ag {
            finite(value, "integration coefficient")?;
        }
        Ok(Coefficients {
            method,
            order,
            deltas,
            history: self.len,
            ag,
        })
    }

    /// Commits the step of accepted `coefficients`, shifting older steps.
    /// This is the only operation that advances the history.
    pub fn accept(&mut self, coefficients: &Coefficients) {
        self.accepted.copy_within(0..HISTORY_LEN - 1, 1);
        self.accepted[0] = coefficients.dt();
        self.len = (self.len + 1).min(HISTORY_LEN);
    }

    /// Forgets every accepted step, e.g. when a run restarts.
    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

/// Immutable coefficients for one trial step.
///
/// `ag()[0]` multiplies the present charge; the rest follow C `CKTag`. They
/// are only valid together with the charge/derivative histories that were
/// accepted when the [`StepHistory`] produced them.
#[derive(Debug, Clone, PartialEq)]
pub struct Coefficients {
    method: IntegrationMethod,
    order: u8,
    /// C `CKTdeltaOld`: the trial step, then accepted steps.
    deltas: [Real; HISTORY_LEN + 1],
    history: usize,
    ag: [Real; 3],
}

/// One integrated charge-storage element in companion form, as
/// `NIintegrate` produces: `derivative = conductance * v + current` with
/// `conductance = ag0 * C`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Companion {
    /// The derivative of the integrated quantity at the trial point (C
    /// `CKTstate0[ccap]`): capacitor current or inductor voltage.
    pub derivative: Real,
    /// Equivalent conductance (or resistance for flux), `ag0 * capacitance`.
    pub conductance: Real,
    /// History source, `derivative - ag0 * q0`.
    pub current: Real,
}

/// Tolerances for [`Coefficients::truncation_timestep`], named as in C.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TruncationTolerances {
    /// `.option reltol`.
    pub reltol: Real,
    /// `.option abstol`, applied to the derivative (current).
    pub abstol: Real,
    /// `.option chgtol`, the charge floor.
    pub chgtol: Real,
    /// `.option trtol`, the truncation-error overestimation factor.
    pub trtol: Real,
}

impl Default for TruncationTolerances {
    /// ngspice defaults (`cktntask.c`).
    fn default() -> Self {
        Self {
            reltol: 1e-3,
            abstol: 1e-12,
            chgtol: 1e-14,
            trtol: 7.0,
        }
    }
}

impl Coefficients {
    /// The integration rule.
    #[must_use]
    pub const fn method(&self) -> IntegrationMethod {
        self.method
    }

    /// The order of this trial.
    #[must_use]
    pub const fn order(&self) -> u8 {
        self.order
    }

    /// The trial step size.
    #[must_use]
    pub const fn dt(&self) -> Real {
        self.deltas[0]
    }

    /// The corrector coefficients `ag[0..=order]` (C `CKTag`).
    #[must_use]
    pub fn ag(&self) -> &[Real] {
        &self.ag[..=usize::from(self.order)]
    }

    /// Number of accepted charge values [`Self::integrate`] reads after the
    /// trial one: the order for Gear and trapezoidal order 1, one for
    /// trapezoidal order 2.
    #[must_use]
    pub fn charge_history_len(&self) -> usize {
        if self.needs_previous_derivative() {
            1
        } else {
            usize::from(self.order)
        }
    }

    /// True when [`Self::integrate`] needs the previous accepted derivative.
    #[must_use]
    pub fn needs_previous_derivative(&self) -> bool {
        self.method == IntegrationMethod::Trapezoidal && self.order == 2
    }

    /// Integrates one element, as `NIintegrate` does.
    ///
    /// `charge` is the trial charge followed by accepted charges, most recent
    /// first; at least [`Self::charge_history_len`] accepted values are
    /// needed. `previous_derivative` is the accepted derivative at the last
    /// time point, required by trapezoidal order 2.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a short history, a missing derivative,
    /// nonfinite inputs/capacitance or nonfinite results.
    pub fn integrate(
        &self,
        charge: &[Real],
        previous_derivative: Option<Real>,
        capacitance: Real,
    ) -> SpiceResult<Companion> {
        let needed = self.charge_history_len() + 1;
        if charge.len() < needed {
            return Err(numerical(format!(
                "order {} {} integration needs {needed} charge values, got {}",
                self.order,
                self.method.as_str(),
                charge.len()
            )));
        }
        let charge = &charge[..needed];
        if charge.iter().any(|q| !q.is_finite()) || !capacitance.is_finite() {
            return Err(numerical("nonfinite charge history or capacitance"));
        }
        let derivative = if self.needs_previous_derivative() {
            let previous = previous_derivative
                .ok_or_else(|| numerical("trapezoidal order 2 needs the previous derivative"))?;
            finite(previous, "previous derivative")?;
            -previous * self.ag[1] + self.ag[0] * (charge[0] - charge[1])
        } else {
            self.ag().iter().zip(charge).map(|(a, q)| a * q).sum()
        };
        let derivative = finite(derivative, "integrated derivative")?;
        Ok(Companion {
            derivative,
            conductance: finite(self.ag[0] * capacitance, "companion conductance")?,
            current: finite(derivative - self.ag[0] * charge[0], "companion current")?,
        })
    }

    /// Predicts one unknown at the trial time from accepted solutions, most
    /// recent first, as `NIpred` does (C's optional `PREDICTOR` build).
    ///
    /// Trapezoidal uses the `NIpred` divided-difference formulas; Gear uses
    /// polynomial extrapolation of degree `order` (`CKTagp`). Order `k` needs
    /// `k` accepted steps and `k + 1` solutions.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for short/nonfinite history or results.
    pub fn predict(&self, solutions: &[Real]) -> SpiceResult<Real> {
        let order = usize::from(self.order);
        if self.history < order || solutions.len() < order + 1 {
            return Err(numerical(format!(
                "order {order} prediction needs {order} accepted steps and {} solutions",
                order + 1
            )));
        }
        let x = &solutions[..=order];
        if x.iter().any(|v| !v.is_finite()) {
            return Err(numerical("nonfinite solution history"));
        }
        let d = &self.deltas;
        let predicted = match self.method {
            IntegrationMethod::Trapezoidal => {
                let dd0 = (x[0] - x[1]) / d[1];
                if order == 1 {
                    x[0] + d[0] * dd0
                } else {
                    let b = -d[0] / (2.0 * d[1]);
                    let dd1 = (x[1] - x[2]) / d[2];
                    x[0] + (b * dd1 + (1.0 - b) * dd0) * d[0]
                }
            }
            IntegrationMethod::Gear { .. } => {
                // Lagrange extrapolation to the trial time from nodes at
                // distance t_{n+1} - t_{n-i}.
                let mut nodes = [0.0; HISTORY_LEN];
                let mut sum = 0.0;
                for (i, node) in nodes.iter_mut().take(order + 1).enumerate() {
                    sum += d[i];
                    *node = sum;
                }
                (0..=order)
                    .map(|i| {
                        let weight: Real = (0..=order)
                            .filter(|&j| j != i)
                            .map(|j| nodes[j] / (nodes[j] - nodes[i]))
                            .product();
                        weight * x[i]
                    })
                    .sum()
            }
        };
        finite(predicted, "prediction")
    }

    /// Local-truncation-error timestep bound for one element, as `CKTterr`
    /// computes it from divided differences of the charge.
    ///
    /// `charge` is the trial charge followed by at least `order + 1` accepted
    /// charges; `derivative` is `[trial, previous accepted]`. Requires `order`
    /// accepted steps. The caller takes the minimum over all elements.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for short/nonfinite inputs, invalid
    /// tolerances or a nonfinite result.
    pub fn truncation_timestep(
        &self,
        charge: &[Real],
        derivative: [Real; 2],
        tolerances: &TruncationTolerances,
    ) -> SpiceResult<Real> {
        let order = usize::from(self.order);
        let tol = tolerances;
        if [tol.reltol, tol.abstol, tol.chgtol, tol.trtol]
            .iter()
            .any(|v| !(v.is_finite() && *v > 0.0))
        {
            return Err(numerical(
                "truncation tolerances must be positive and finite",
            ));
        }
        if self.history < order || charge.len() < order + 2 {
            return Err(numerical(format!(
                "order {order} truncation estimate needs {order} accepted steps and {} charges",
                order + 2
            )));
        }
        if charge.iter().chain(&derivative).any(|v| !v.is_finite()) {
            return Err(numerical("nonfinite truncation-error history"));
        }
        let volttol = tol.abstol + tol.reltol * derivative[0].abs().max(derivative[1].abs());
        let chargetol =
            tol.reltol * charge[0].abs().max(charge[1].abs()).max(tol.chgtol) / self.dt();
        let tolerance = volttol.max(chargetol);
        // Divided differences over the trial and accepted points.
        let mut diff = [0.0; HISTORY_LEN + 2];
        diff[..order + 2].copy_from_slice(&charge[..order + 2]);
        let mut span = [0.0; HISTORY_LEN + 1];
        span[..=order].copy_from_slice(&self.deltas[..=order]);
        let mut j = order;
        loop {
            for i in 0..=j {
                diff[i] = (diff[i] - diff[i + 1]) / span[i];
            }
            if j == 0 {
                break;
            }
            j -= 1;
            for i in 0..=j {
                span[i] = span[i + 1] + self.deltas[i];
            }
        }
        // cktterr.c gearCoeff/trapCoeff.
        let factor = match (self.method, self.order) {
            (_, 1) => 0.5,
            (IntegrationMethod::Trapezoidal, _) => 0.083_333_333_33,
            (IntegrationMethod::Gear { .. }, _) => 0.222_222_222_2,
        };
        let del = tol.trtol * tolerance / tol.abstol.max(factor * diff[0].abs());
        let del = if order == 2 { del.sqrt() } else { del };
        finite(del, "truncation timestep")
    }
}

#[cfg(test)]
mod tests {
    use super::{Coefficients, DEFAULT_XMU, IntegrationMethod, StepHistory, TruncationTolerances};

    const TRAP: IntegrationMethod = IntegrationMethod::Trapezoidal;
    const GEAR2: IntegrationMethod = IntegrationMethod::Gear { order: 2 };

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
    }

    /// History after accepting `steps` (oldest first).
    fn history(steps: &[f64]) -> StepHistory {
        let mut history = StepHistory::new();
        for &dt in steps {
            let coefficients = history.trial(TRAP, 1, dt, DEFAULT_XMU).unwrap();
            history.accept(&coefficients);
        }
        history
    }

    #[test]
    fn trapezoidal_is_the_default() {
        assert_eq!(IntegrationMethod::default(), TRAP);
        assert!(TRAP.is_valid());
        assert_eq!(TRAP.max_order(), 2);
        assert!(TRAP.validate_runtime().is_ok());
    }

    #[test]
    fn gear_orders_are_representable_but_only_two_run() {
        assert!(IntegrationMethod::Gear { order: 6 }.is_valid());
        assert!(!IntegrationMethod::Gear { order: 0 }.is_valid());
        assert!(!IntegrationMethod::Gear { order: 7 }.is_valid());
        assert!(
            IntegrationMethod::Gear { order: 1 }
                .validate_runtime()
                .is_ok()
        );
        assert!(GEAR2.validate_runtime().is_ok());
        for order in [0, 3, 6] {
            let error = IntegrationMethod::Gear { order }
                .validate_runtime()
                .unwrap_err();
            assert!(error.to_string().contains("only orders 1 and 2"), "{error}");
        }
        let error = history(&[1.0, 1.0])
            .trial(IntegrationMethod::Gear { order: 6 }, 3, 1.0, DEFAULT_XMU)
            .unwrap_err();
        assert!(error.to_string().contains("implemented 1..=2"), "{error}");
        // A method's own maximum is honored.
        assert!(
            history(&[1.0])
                .trial(IntegrationMethod::Gear { order: 1 }, 2, 1.0, DEFAULT_XMU)
                .is_err()
        );
    }

    #[test]
    fn method_parsing_uses_the_option_spelling() {
        assert_eq!(IntegrationMethod::parse("TRAP", 2), Some(TRAP));
        assert_eq!(
            IntegrationMethod::parse("gear", 4),
            Some(IntegrationMethod::Gear { order: 4 })
        );
        assert_eq!(IntegrationMethod::parse("euler", 1), None);
        assert_eq!(GEAR2.as_str(), "gear");
        assert_eq!(TRAP.as_str(), "trap");
    }

    #[test]
    fn known_fixed_step_coefficients() {
        let h = 1e-6;
        let fresh = StepHistory::new();
        // Backward Euler startup for both methods.
        for method in [TRAP, GEAR2] {
            let c = fresh.trial(method, 1, h, DEFAULT_XMU).unwrap();
            assert!(close(c.ag()[0], 1.0 / h) && close(c.ag()[1], -1.0 / h));
        }
        let warm = history(&[h]);
        let trap = warm.trial(TRAP, 2, h, DEFAULT_XMU).unwrap();
        assert!(close(trap.ag()[0], 2.0 / h) && close(trap.ag()[1], 1.0));
        let damped = warm.trial(TRAP, 2, h, 0.25).unwrap();
        assert!(close(damped.ag()[0], 1.0 / h / 0.75) && close(damped.ag()[1], 1.0 / 3.0));
        let gear = warm.trial(GEAR2, 2, h, DEFAULT_XMU).unwrap();
        let expected = [1.5 / h, -2.0 / h, 0.5 / h];
        assert!(gear.ag().iter().zip(expected).all(|(a, b)| close(*a, b)));
    }

    /// Gear corrector and predictor reproduce polynomial derivatives/values
    /// exactly up to their order, on nonuniform steps.
    #[test]
    fn gear_is_exact_for_polynomials_on_nonuniform_steps() {
        type Poly = (fn(f64) -> f64, fn(f64) -> f64, u8);
        let polys: [Poly; 3] = [
            (|_| 3.0, |_| 0.0, 1),
            (|t| 2.0 * t - 1.0, |_| 2.0, 1),
            (|t| t * t - 3.0 * t + 0.5, |t| 2.0 * t - 3.0, 2),
        ];
        let (h1, h) = (0.3, 0.7);
        let warm = history(&[0.2, h1]);
        let times = [1.0 + h, 1.0, 1.0 - h1, 1.0 - h1 - 0.2];
        for (f, df, degree) in polys {
            for order in degree..=2 {
                let c = warm.trial(GEAR2, order, h, DEFAULT_XMU).unwrap();
                let q: Vec<f64> = times.iter().map(|t| f(*t)).collect();
                let companion = c.integrate(&q, None, 1.0).unwrap();
                assert!(close(companion.derivative, df(times[0])), "order {order}");
                assert!(close(c.predict(&q[1..]).unwrap(), q[0]), "order {order}");
            }
        }
    }

    /// Trapezoidal order 2 integrates the mean of the endpoint derivatives,
    /// exact for quadratics; order 1 is backward Euler.
    #[test]
    fn trapezoidal_integration_matches_its_rule() {
        let (h1, h) = (0.4, 0.25);
        let warm = history(&[h1]);
        let f = |t: f64| t * t + t;
        let df = |t: f64| 2.0 * t + 1.0;
        let c = warm.trial(TRAP, 2, h, DEFAULT_XMU).unwrap();
        assert!(c.needs_previous_derivative());
        assert_eq!(c.charge_history_len(), 1);
        let q = [f(1.0 + h), f(1.0)];
        let companion = c.integrate(&q, Some(df(1.0)), 2.0).unwrap();
        assert!(close(companion.derivative, df(1.0 + h)));
        assert!(close(companion.conductance, 2.0 * 2.0 / h));
        assert!(close(
            companion.current,
            companion.derivative - c.ag()[0] * q[0]
        ));
        let be = warm.trial(TRAP, 1, h, DEFAULT_XMU).unwrap();
        let companion = be.integrate(&q, None, 1.0).unwrap();
        assert!(close(companion.derivative, (q[0] - q[1]) / h));
        // NIpred's trapezoidal predictors extrapolate lines exactly on
        // nonuniform steps.
        let warm = history(&[0.3, h1]);
        let times = [1.0 + h, 1.0, 1.0 - h1, 1.0 - h1 - 0.3];
        let line: Vec<f64> = times.iter().map(|t| 4.0 * t - 1.0).collect();
        for order in [1, 2] {
            let c = warm.trial(TRAP, order, h, DEFAULT_XMU).unwrap();
            assert!(close(c.predict(&line[1..]).unwrap(), line[0]));
        }
    }

    #[test]
    fn startup_and_order_changes_follow_the_accepted_history() {
        let mut history = StepHistory::new();
        assert_eq!(history.available_order(GEAR2), 1);
        assert_eq!(history.available_order(TRAP), 1);
        assert!(history.trial(GEAR2, 2, 1e-6, DEFAULT_XMU).is_err());
        let first = history.trial(GEAR2, 1, 1e-6, DEFAULT_XMU).unwrap();
        history.accept(&first);
        assert_eq!(history.available_order(GEAR2), 2);
        assert_eq!(
            history.available_order(IntegrationMethod::Gear { order: 1 }),
            1
        );
        let second = history.trial(GEAR2, 2, 2e-6, DEFAULT_XMU).unwrap();
        // Drop back to order 1 (as after a breakpoint) with history intact.
        let back = history.trial(GEAR2, 1, 5e-7, DEFAULT_XMU).unwrap();
        assert_eq!(back.order(), 1);
        history.accept(&second);
        history.accept(&back);
        history.accept(&back);
        assert_eq!(history.accepted(), &[5e-7, 5e-7, 2e-6]);
        assert_eq!(history.len(), 3);
        history.clear();
        assert!(history.is_empty());
    }

    #[test]
    fn rejected_trials_never_advance_the_history() {
        let accepted = history(&[1e-6, 2e-6]);
        let before = accepted.clone();
        for dt in [1e-6, 1e-9, 5e-6] {
            let c = accepted.trial(GEAR2, 2, dt, DEFAULT_XMU).unwrap();
            let _ = c.integrate(&[1.0, 0.5, 0.2], None, 1e-9).unwrap();
        }
        assert!(accepted.trial(TRAP, 2, -1.0, DEFAULT_XMU).is_err());
        assert_eq!(accepted, before);
    }

    #[test]
    fn invalid_inputs_are_rejected_without_panic() {
        let warm = history(&[1e-6]);
        for dt in [0.0, -1e-6, f64::NAN, f64::INFINITY] {
            let error = warm.trial(TRAP, 1, dt, DEFAULT_XMU).unwrap_err();
            assert!(error.to_string().contains("must be positive and finite"));
        }
        assert!(warm.trial(TRAP, 0, 1e-6, DEFAULT_XMU).is_err());
        for xmu in [-0.1, 0.6, f64::NAN] {
            assert!(warm.trial(TRAP, 2, 1e-6, xmu).is_err());
        }
        // 1/dt overflows for subnormal steps.
        let error = warm.trial(TRAP, 1, 1e-320, DEFAULT_XMU).unwrap_err();
        assert!(error.to_string().contains("not finite"), "{error}");

        let trap = warm.trial(TRAP, 2, 1e-6, DEFAULT_XMU).unwrap();
        assert!(trap.integrate(&[1.0], Some(0.0), 1.0).is_err());
        assert!(trap.integrate(&[1.0, 0.0], None, 1.0).is_err());
        assert!(trap.integrate(&[f64::NAN, 0.0], Some(0.0), 1.0).is_err());
        assert!(
            trap.integrate(&[1.0, 0.0], Some(0.0), f64::INFINITY)
                .is_err()
        );
        assert!(
            trap.integrate(&[f64::MAX, -f64::MAX], Some(0.0), 1.0)
                .is_err()
        );
        let gear = warm.trial(GEAR2, 2, 1e-6, DEFAULT_XMU).unwrap();
        assert!(gear.integrate(&[1.0, 0.0], None, 1.0).is_err());
        assert!(gear.predict(&[1.0, 2.0, 3.0]).is_err(), "needs 2 steps");
        assert!(trap.predict(&[1.0]).is_err());
        assert!(trap.predict(&[1.0, f64::NAN, 0.0]).is_err());
    }

    /// Direct transcription of the order-1/2 `CKTterr` divided differences.
    fn reference_terr(c: &Coefficients, q: &[f64], d: [f64; 2], t: &TruncationTolerances) -> f64 {
        let h = &c.deltas;
        let volttol = t.abstol + t.reltol * d[0].abs().max(d[1].abs());
        let chargetol = t.reltol * q[0].abs().max(q[1].abs()).max(t.chgtol) / c.dt();
        let tol = volttol.max(chargetol);
        let dd = |a: f64, b: f64, h: f64| (a - b) / h;
        let (d0, d1) = (dd(q[0], q[1], h[0]), dd(q[1], q[2], h[1]));
        let e0 = dd(d0, d1, h[0] + h[1]);
        let (diff, factor) = if c.order() == 1 {
            (e0, 0.5)
        } else {
            let d2 = dd(q[2], q[3], h[2]);
            let e1 = dd(d1, d2, h[1] + h[2]);
            let factor = if c.method() == TRAP {
                0.083_333_333_33
            } else {
                0.222_222_222_2
            };
            (dd(e0, e1, h[0] + h[1] + h[2]), factor)
        };
        let del = t.trtol * tol / t.abstol.max(factor * diff.abs());
        if c.order() == 2 { del.sqrt() } else { del }
    }

    #[test]
    fn truncation_timestep_follows_cktterr() {
        let tol = TruncationTolerances::default();
        let warm = history(&[2e-6, 1e-6]);
        let q = [3.1e-9, 2.0e-9, 1.6e-9, 1.5e-9];
        let d = [1e-3, 8e-4];
        for (method, order) in [(TRAP, 1), (TRAP, 2), (GEAR2, 1), (GEAR2, 2)] {
            let c = warm.trial(method, order, 5e-7, DEFAULT_XMU).unwrap();
            let del = c.truncation_timestep(&q, d, &tol).unwrap();
            assert!(
                close(del, reference_terr(&c, &q, d, &tol)),
                "{method:?} {order}"
            );
            assert!(del > 0.0);
        }
        let c = warm.trial(GEAR2, 2, 5e-7, DEFAULT_XMU).unwrap();
        // Short history/charges and invalid tolerances fail.
        assert!(c.truncation_timestep(&q[..3], d, &tol).is_err());
        let fresh = history(&[1e-6]).trial(TRAP, 2, 1e-6, DEFAULT_XMU).unwrap();
        assert!(fresh.truncation_timestep(&q, d, &tol).is_err());
        let bad = TruncationTolerances { trtol: 0.0, ..tol };
        assert!(c.truncation_timestep(&q, d, &bad).is_err());
        assert!(c.truncation_timestep(&q, [f64::NAN, 0.0], &tol).is_err());
    }
}
