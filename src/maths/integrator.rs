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
//! (`NIpred`) and `src/spicelib/analysis/cktterr.c` (`CKTterr`): trapezoidal
//! orders 1–2 and variable-step Gear orders 1–6 (#98). Which order a run
//! actually uses is the transient driver's policy; `dctran.c` (and therefore
//! [`crate::analysis::companion`]) only ever toggles between orders 1 and 2,
//! whatever `maxord` says, so Gear orders 3–6 are reachable through this API
//! but not from an ngspice-compatible `.tran`.
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
//! The separately selected [`crate::maths::diffsol`] BDF adapter does not implement
//! this companion-model contract and never consumes these coefficients.

use crate::primitives::{Real, SpiceError, SpiceResult};

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

    /// The highest order with implemented coefficient/history operations:
    /// every representable Gear order since #98 (trapezoidal stops at 2).
    pub const MAX_RUNTIME_ORDER: u8 = Self::MAX_GEAR_ORDER;

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
    /// [`SpiceError::Unsupported`] for Gear orders 0 and above 6 (ngspice's
    /// `cktsopt.c` clamps `maxord` to `1..=6` with a warning; the port refuses
    /// instead of silently changing the request).
    pub fn validate_runtime(self) -> SpiceResult<()> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(SpiceError::Unsupported {
                feature: format!(
                    "{} integration order {}; orders 1 to {} are implemented",
                    self.as_str(),
                    self.max_order(),
                    Self::MAX_GEAR_ORDER
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

/// Number of accepted step sizes retained (C `CKTdeltaOld[1..=6]`): enough
/// for order-6 Gear prediction and truncation-error estimation, which need
/// `order` accepted steps.
const HISTORY_LEN: usize = IntegrationMethod::MAX_GEAR_ORDER as usize;

/// `cktterr.c` `gearCoeff`: the error constants of Gear orders 1–6.
const GEAR_ERROR_CONSTANTS: [Real; 6] = [
    0.5,
    0.222_222_222_2,
    0.136_363_636_4,
    0.096,
    0.072_992_700_73,
    0.058_309_037_90,
];
/// `cktterr.c` `trapCoeff`: the error constants of trapezoidal orders 1–2.
const TRAP_ERROR_CONSTANTS: [Real; 2] = [0.5, 0.083_333_333_33];

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
    /// Placeholder for steps older than the first accepted one.
    fill: Option<Real>,
}

impl StepHistory {
    /// A history with no accepted steps.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            accepted: [0.0; HISTORY_LEN],
            len: 0,
            fill: None,
        }
    }

    /// A history whose not-yet-accepted older steps read as `fill`, as C
    /// initializes `CKTdeltaOld[0..7]` to `CKTmaxStep` before the first step.
    ///
    /// The placeholders only feed [`Coefficients::truncation_timestep`] (which
    /// may then run at an order above the number of accepted steps, exactly as
    /// `dctran.c` probes order 2 on its second step); coefficients and
    /// prediction still require real accepted steps.
    #[must_use]
    pub const fn with_fill(fill: Real) -> Self {
        Self {
            accepted: [0.0; HISTORY_LEN],
            len: 0,
            fill: Some(fill),
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
            .min(IntegrationMethod::MAX_GEAR_ORDER)
    }

    /// Builds the coefficients for a trial step of size `dt`, without
    /// changing this history. `xmu` only affects trapezoidal order 2.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a nonpositive/nonfinite step, `xmu`
    /// outside `[0, 0.5]`, nonfinite coefficients or a history too short for
    /// the order; [`SpiceError::Unsupported`] for order 0, an invalid method
    /// (Gear above order 6) or an order above the method's maximum (2 for
    /// trapezoidal).
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
        if !method.is_valid() || order == 0 || order > method.max_order() {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "{} integration at order {order} (maximum {}; implemented: trap 1..=2, \
                     gear 1..={})",
                    method.as_str(),
                    method.max_order(),
                    IntegrationMethod::MAX_GEAR_ORDER
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
        if let Some(fill) = self.fill {
            deltas[self.len + 1..].fill(fill);
        }
        // nicomcof.c
        let mut ag = [0.0; HISTORY_LEN + 1];
        match (method, order) {
            (_, 1) => ag[..2].copy_from_slice(&[1.0 / dt, -1.0 / dt]),
            (IntegrationMethod::Trapezoidal, _) => {
                ag[..2].copy_from_slice(&[1.0 / dt / (1.0 - xmu), xmu / (1.0 - xmu)]);
            }
            (IntegrationMethod::Gear { .. }, 2) => {
                // Closed form for t_{n+1}, t_n, t_{n-1}, kept from the
                // original Gear-2 port so its results stay bit-identical;
                // gear_corrector agrees to rounding (tested).
                let h1 = deltas[1];
                let r = 1.0 + h1 / dt;
                let a2 = 1.0 / (r * h1);
                let a1 = -a2 * r * r;
                ag[..3].copy_from_slice(&[-(a1 + a2), a1, a2]);
            }
            (IntegrationMethod::Gear { .. }, _) => {
                gear_corrector(&deltas, usize::from(order), &mut ag);
            }
        }
        for value in ag {
            finite(value, "integration coefficient")?;
        }
        Ok(Coefficients {
            method,
            order,
            deltas,
            history: self.len,
            filled: self.fill.is_some(),
            ag,
        })
    }

    /// Commits the step of accepted `coefficients`, shifting older steps
    /// (C keeps `CKTdeltaOld[0..=6]`; the oldest step drops out).
    /// This is the only operation that advances the history.
    pub fn accept(&mut self, coefficients: &Coefficients) {
        self.accepted.copy_within(0..HISTORY_LEN - 1, 1);
        self.accepted[0] = coefficients.dt();
        self.len = (self.len + 1).min(HISTORY_LEN);
    }

    /// Forgets every accepted step, e.g. when a run restarts.
    pub fn clear(&mut self) {
        *self = Self {
            fill: self.fill,
            ..Self::new()
        };
    }
}

/// The variable-step Gear (BDF) corrector of `order` on the points
/// `t_{n+1} - s_i`, `s_0 = 0`, `s_i = deltas[0] + ... + deltas[i - 1]`.
///
/// `nicomcof.c` solves the Vandermonde system `sum_i ag_i (s_i/h)^j =
/// -delta_{j1}/h`, `j = 0..=order`, in step-normalized form (its "SPICE2
/// difference warning"). The solution is the derivative at `t_{n+1}` of the
/// Lagrange basis polynomials, which this evaluates in closed form on the same
/// normalized abscissae `sigma_i = s_i / h`, so no matrix (and no pivot-free
/// elimination) is involved:
///
/// * `ag_0 = (1/h) sum_{j >= 1} 1/sigma_j`,
/// * `ag_i = (1/h) prod_{j != 0, i} sigma_j / (-sigma_i prod_{j != 0, i} (sigma_j - sigma_i))`.
///
/// The two agree to rounding; the result is exact for polynomials of degree
/// `order` (tested), which is what defines the C coefficients.
fn gear_corrector(
    deltas: &[Real; HISTORY_LEN + 1],
    order: usize,
    ag: &mut [Real; HISTORY_LEN + 1],
) {
    let h = deltas[0];
    let mut sigma = [0.0; HISTORY_LEN + 1];
    let mut sum = 0.0;
    for i in 1..=order {
        sum += deltas[i - 1];
        sigma[i] = sum / h;
    }
    ag[0] = (1..=order).map(|j| 1.0 / sigma[j]).sum::<Real>() / h;
    for i in 1..=order {
        let (mut numerator, mut denominator) = (1.0, -sigma[i]);
        for j in (1..=order).filter(|&j| j != i) {
            numerator *= sigma[j];
            denominator *= sigma[j] - sigma[i];
        }
        ag[i] = numerator / denominator / h;
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
    /// Older steps are placeholders (see [`StepHistory::with_fill`]).
    filled: bool,
    ag: [Real; HISTORY_LEN + 1],
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
                let mut nodes = [0.0; HISTORY_LEN + 1];
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
    /// accepted steps. The bound is `(trtol tol / max(abstol, c_k |dd|))^(1/k)`
    /// with the order-`k` error constant `c_k` of `cktterr.c` (`gearCoeff`,
    /// `trapCoeff`) and the `(k + 1)`-th divided difference `dd` of the charge.
    /// The caller takes the minimum over all elements.
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
        if (self.history < order && !self.filled) || charge.len() < order + 2 {
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
        // cktterr.c gearCoeff/trapCoeff (trial() guarantees the index).
        let factor = match self.method {
            IntegrationMethod::Trapezoidal => TRAP_ERROR_CONSTANTS[order - 1],
            IntegrationMethod::Gear { .. } => GEAR_ERROR_CONSTANTS[order - 1],
        };
        let del = tol.trtol * tolerance / tol.abstol.max(factor * diff[0].abs());
        let del = match order {
            1 => del,
            2 => del.sqrt(),
            3 => del.cbrt(),
            _ => (del.ln() / Real::from(self.order)).exp(),
        };
        finite(del, "truncation timestep")
    }
}

#[cfg(test)]
mod tests {
    use super::{Coefficients, DEFAULT_XMU, IntegrationMethod, StepHistory, TruncationTolerances};

    const TRAP: IntegrationMethod = IntegrationMethod::Trapezoidal;
    const GEAR2: IntegrationMethod = IntegrationMethod::Gear { order: 2 };
    const GEAR6: IntegrationMethod = IntegrationMethod::Gear { order: 6 };

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
    fn gear_orders_one_to_six_run_and_others_are_rejected() {
        assert!(IntegrationMethod::Gear { order: 6 }.is_valid());
        assert!(!IntegrationMethod::Gear { order: 0 }.is_valid());
        assert!(!IntegrationMethod::Gear { order: 7 }.is_valid());
        for order in 1..=6 {
            let method = IntegrationMethod::Gear { order };
            assert!(method.validate_runtime().is_ok(), "{order}");
            // Order k needs k - 1 accepted steps.
            let warm = history(&vec![1.0; usize::from(order) - 1]);
            assert_eq!(warm.available_order(method), order);
            assert!(warm.trial(method, order, 1.0, DEFAULT_XMU).is_ok());
            if order > 1 {
                let short = history(&vec![1.0; usize::from(order) - 2]);
                assert!(short.trial(method, order, 1.0, DEFAULT_XMU).is_err());
            }
        }
        for order in [0, 7, u8::MAX] {
            let method = IntegrationMethod::Gear { order };
            let error = method.validate_runtime().unwrap_err();
            assert!(error.to_string().contains("orders 1 to 6"), "{error}");
            // An invalid method never indexes past the coefficient storage.
            assert!(
                history(&[1.0; 6])
                    .trial(method, 7, 1.0, DEFAULT_XMU)
                    .is_err()
            );
        }
        // A method's own maximum is honored, and trapezoidal stops at 2.
        assert!(
            history(&[1.0])
                .trial(IntegrationMethod::Gear { order: 1 }, 2, 1.0, DEFAULT_XMU)
                .is_err()
        );
        assert!(
            history(&[1.0; 4])
                .trial(IntegrationMethod::Gear { order: 3 }, 4, 1.0, DEFAULT_XMU)
                .is_err()
        );
        let error = history(&[1.0; 4])
            .trial(TRAP, 3, 1.0, DEFAULT_XMU)
            .unwrap_err();
        assert!(error.to_string().contains("trap 1..=2"), "{error}");
        assert_eq!(history(&[1.0; 6]).available_order(TRAP), 2);
        assert_eq!(history(&[1.0; 6]).available_order(GEAR6), 6);
        // The step history keeps exactly six accepted steps (C CKTdeltaOld[1..=6]).
        let long = history(&[7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
        assert_eq!(long.accepted(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
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
        assert_eq!(history.accepted(), &[5e-7, 5e-7, 2e-6, 1e-6]);
        assert_eq!(history.len(), 4);
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

    #[test]
    fn filled_history_supplies_placeholder_steps_for_truncation_only() {
        // dctran.c fills CKTdeltaOld with CKTmaxStep, so the second step can
        // already probe an order-2 truncation estimate.
        let tol = TruncationTolerances::default();
        let q = [3.1e-9, 2.0e-9, 1.6e-9, 1.5e-9];
        let d = [1e-3, 8e-4];
        let mut filled = StepHistory::with_fill(1e-4);
        let mut plain = StepHistory::new();
        for history in [&mut filled, &mut plain] {
            let first = history.trial(TRAP, 1, 1e-6, DEFAULT_XMU).unwrap();
            history.accept(&first);
        }
        let probe = |h: &StepHistory| h.trial(TRAP, 2, 5e-7, DEFAULT_XMU).unwrap();
        assert!(probe(&plain).truncation_timestep(&q, d, &tol).is_err());
        let with_fill = probe(&filled).truncation_timestep(&q, d, &tol).unwrap();
        // The older step is the placeholder (1e-4) after one real step (1e-6).
        let reference = history(&[1e-4, 1e-6])
            .trial(TRAP, 2, 5e-7, DEFAULT_XMU)
            .unwrap();
        assert!(close(
            with_fill,
            reference.truncation_timestep(&q, d, &tol).unwrap()
        ));
        // Prediction still needs real accepted steps, and clear() keeps the fill.
        assert!(probe(&filled).predict(&[1.0, 0.5, 0.25]).is_err());
        filled.clear();
        assert!(filled.is_empty());
        assert!(probe_after_clear(&filled));
    }

    fn probe_after_clear(history: &StepHistory) -> bool {
        // Order 2 is unavailable without a real step, filled or not.
        history.trial(TRAP, 2, 5e-7, DEFAULT_XMU).is_err()
            && history.trial(TRAP, 1, 5e-7, DEFAULT_XMU).is_ok()
    }

    /// Nonuniform accepted steps (oldest first) and the trial step used by the
    /// order-3..6 tests: ratios up to 3.5 between neighbours.
    const OLD_STEPS: [f64; 6] = [0.11, 0.35, 0.1, 0.2, 0.15, 0.3];
    const TRIAL_STEP: f64 = 0.25;

    /// Time points `t_{n+1}, t_n, ..., t_{n-6}` for [`OLD_STEPS`], most recent
    /// first, with `t_n = 1`.
    fn nonuniform_times() -> [f64; 8] {
        let mut times = [0.0; 8];
        times[0] = 1.0 + TRIAL_STEP;
        times[1] = 1.0;
        for (i, dt) in OLD_STEPS.iter().rev().enumerate() {
            times[i + 2] = times[i + 1] - dt;
        }
        times
    }

    /// Textbook fixed-step BDF coefficients (times `h`) for orders 1 to 6.
    #[test]
    fn gear_matches_the_fixed_step_bdf_tables() {
        let tables: [&[f64]; 6] = [
            &[1.0, -1.0],
            &[1.5, -2.0, 0.5],
            &[11.0 / 6.0, -3.0, 1.5, -1.0 / 3.0],
            &[25.0 / 12.0, -4.0, 3.0, -4.0 / 3.0, 0.25],
            &[137.0 / 60.0, -5.0, 5.0, -10.0 / 3.0, 1.25, -0.2],
            &[147.0 / 60.0, -6.0, 7.5, -20.0 / 3.0, 3.75, -1.2, 1.0 / 6.0],
        ];
        let h = 2.5e-7;
        let warm = history(&[h; 6]);
        for (order, table) in (1..=6).zip(tables) {
            let c = warm.trial(GEAR6, order, h, DEFAULT_XMU).unwrap();
            assert_eq!(c.ag().len(), usize::from(order) + 1);
            for (a, b) in c.ag().iter().zip(table) {
                assert!((a * h - b).abs() <= 1e-12, "order {order}: {a} vs {b}/h");
            }
        }
    }

    /// An independent solve of the `nicomcof.c` system: `sum_i ag_i
    /// (s_i/h)^j = -[j == 1]/h` by Gaussian elimination with partial pivoting.
    fn vandermonde_ag(deltas: &[f64], order: usize) -> Vec<f64> {
        let n = order + 1;
        let h = deltas[0];
        let mut s = vec![0.0; n];
        for i in 1..n {
            s[i] = s[i - 1] + deltas[i - 1];
        }
        let mut m: Vec<Vec<f64>> = (0..n)
            .map(|j| {
                let mut row: Vec<f64> = s.iter().map(|si| (si / h).powi(j as i32)).collect();
                row.push(if j == 1 { -1.0 / h } else { 0.0 });
                row
            })
            .collect();
        for k in 0..n {
            let pivot = (k..n)
                .max_by(|&a, &b| m[a][k].abs().total_cmp(&m[b][k].abs()))
                .unwrap();
            m.swap(k, pivot);
            let pivot_row = m[k].clone();
            for row in m.iter_mut().skip(k + 1) {
                let f = row[k] / pivot_row[k];
                for (value, p) in row.iter_mut().zip(&pivot_row).skip(k) {
                    *value -= f * p;
                }
            }
        }
        let mut x = vec![0.0; n];
        for k in (0..n).rev() {
            let tail: f64 = (k + 1..n).map(|c| m[k][c] * x[c]).sum();
            x[k] = (m[k][n] - tail) / m[k][k];
        }
        x
    }

    /// Orders 1..6 on nonuniform steps: the corrector reproduces the derivative
    /// and the predictor the value of every polynomial of degree `order`, and
    /// the coefficients agree with the C Vandermonde formulation.
    #[test]
    fn high_order_gear_is_exact_for_polynomials_on_nonuniform_steps() {
        let warm = history(&OLD_STEPS);
        let times = nonuniform_times();
        for order in 1..=6_u8 {
            let k = i32::from(order);
            // Degree `order` with nonzero lower terms, centred to keep the
            // values O(1).
            let f = |t: f64| {
                (0..=k)
                    .map(|p| (t - 0.6).powi(p) / f64::from(p + 1))
                    .sum::<f64>()
            };
            let df = |t: f64| {
                (1..=k)
                    .map(|p| f64::from(p) * (t - 0.6).powi(p - 1) / f64::from(p + 1))
                    .sum::<f64>()
            };
            let c = warm.trial(GEAR6, order, TRIAL_STEP, DEFAULT_XMU).unwrap();
            let q: Vec<f64> = times.iter().map(|t| f(*t)).collect();
            assert_eq!(c.charge_history_len(), usize::from(order));
            let companion = c.integrate(&q, None, 1.0).unwrap();
            let close6 = |a: f64, b: f64| (a - b).abs() <= 1e-9 * (1.0 + b.abs());
            assert!(close6(companion.derivative, df(times[0])), "order {order}");
            assert!(close6(c.predict(&q[1..]).unwrap(), q[0]), "order {order}");
            let reference = vandermonde_ag(&c.deltas, usize::from(order));
            // The general Lagrange form also reproduces the order-1 and the
            // closed-form order-2 coefficients.
            let mut general = [0.0; 7];
            super::gear_corrector(&c.deltas, usize::from(order), &mut general);
            for ((a, b), g) in c.ag().iter().zip(&reference).zip(general) {
                assert!(
                    (a - b).abs() <= 1e-10 * b.abs().max(1.0 / TRIAL_STEP),
                    "order {order}"
                );
                assert!(
                    (a - g).abs() <= 1e-12 * g.abs().max(1.0 / TRIAL_STEP),
                    "order {order}"
                );
            }
            // One degree more is not reproduced: the rule really has order k.
            let g = |t: f64| (t - 0.6).powi(k + 1);
            let q: Vec<f64> = times.iter().map(|t| g(*t)).collect();
            let derivative = c.integrate(&q, None, 1.0).unwrap().derivative;
            let exact = f64::from(k + 1) * (times[0] - 0.6).powi(k);
            assert!((derivative - exact).abs() > 1e-6, "order {order}");
        }
        // Short charge or solution histories are errors, not panics.
        let c = warm.trial(GEAR6, 6, TRIAL_STEP, DEFAULT_XMU).unwrap();
        assert!(c.integrate(&[0.0; 6], None, 1.0).is_err());
        assert!(c.predict(&[0.0; 6]).is_err());
        assert!(
            c.truncation_timestep(&[0.0; 7], [0.0; 2], &TruncationTolerances::default())
                .is_err()
        );
    }

    /// `CKTterr` for orders 1..6: on a polynomial of degree `order + 1` with
    /// leading coefficient `a` the `(order + 1)`-th divided difference is `a`,
    /// so the bound is `(trtol tol / max(abstol, c_k |a|))^(1/k)` exactly.
    #[test]
    fn high_order_truncation_timestep_follows_cktterr() {
        let constants = [
            0.5,
            0.222_222_222_2,
            0.136_363_636_4,
            0.096,
            0.072_992_700_73,
            0.058_309_037_90,
        ];
        let tol = TruncationTolerances::default();
        let warm = history(&OLD_STEPS);
        let times = nonuniform_times();
        for order in 1..=6_u8 {
            let k = i32::from(order);
            let a = 3.0e-3;
            let q: Vec<f64> = times.iter().map(|t| a * t.powi(k + 1) + 1e-9 * t).collect();
            let d = [2e-3, 1e-3];
            let c = warm.trial(GEAR6, order, TRIAL_STEP, DEFAULT_XMU).unwrap();
            let del = c.truncation_timestep(&q, d, &tol).unwrap();
            let volttol = tol.abstol + tol.reltol * 2e-3;
            let chargetol = tol.reltol * q[0].abs().max(q[1].abs()).max(tol.chgtol) / TRIAL_STEP;
            let base = tol.trtol * volttol.max(chargetol)
                / tol.abstol.max(constants[usize::from(order) - 1] * a);
            let expected = base.powf(1.0 / f64::from(k));
            assert!(
                (del - expected).abs() <= 1e-9 * expected,
                "order {order}: {del} vs {expected}"
            );
        }
    }

    /// Fixed-step convergence: Gear order `k` integrating `q' = -q` over
    /// `[0, 1]` from exact starting values loses a factor of about `2^k` in
    /// global error per halving of every step, on a nonuniform repeating step
    /// pattern (so the variable-step coefficients are exercised, not just the
    /// fixed-step table).
    #[test]
    fn gear_orders_converge_at_their_order() {
        let pattern = [1.0, 0.7, 1.3];
        let solve = |order: u8, base: f64| -> f64 {
            // Exact history at t = 0 and the order - 1 earlier points (steps
            // from the pattern), most recent first; older steps are accepted
            // first.
            let dts: Vec<f64> = (0..usize::from(order) - 1)
                .map(|i| base * pattern[(i + 1) % 3])
                .collect();
            let mut steps = StepHistory::new();
            for &dt in dts.iter().rev() {
                let c = steps.trial(TRAP, 1, dt, DEFAULT_XMU).unwrap();
                steps.accept(&c);
            }
            let mut t = 0.0;
            let mut q = vec![1.0];
            for dt in &dts {
                t -= dt;
                q.push((-t).exp());
            }
            let mut now = 0.0;
            let mut i = 0;
            while now < 1.0 - 1e-12 {
                let dt = (base * pattern[i % 3]).min(1.0 - now);
                i += 1;
                let c = steps.trial(GEAR6, order, dt, DEFAULT_XMU).unwrap();
                // derivative = ag0 q0 + history; with q0 = 0 the companion
                // current is the history part, so ag0 q0 + current = -q0.
                let mut trial = vec![0.0];
                trial.extend_from_slice(&q);
                let companion = c.integrate(&trial, None, 1.0).unwrap();
                let q0 = -companion.current / (companion.conductance + 1.0);
                q.insert(0, q0);
                q.truncate(8);
                steps.accept(&c);
                now += dt;
            }
            (q[0] - (-1.0_f64).exp()).abs()
        };
        for order in 1..=6_u8 {
            // Coarser for high orders so the finest error stays far above
            // rounding (about 1e-13 at order 6).
            let base = [0.04, 0.04, 0.04, 0.06, 0.08, 0.12][usize::from(order) - 1];
            let errors: Vec<f64> = (0..3)
                .map(|n| solve(order, base / f64::from(1 << n)))
                .collect();
            let expected = f64::from(1_u32 << order);
            for pair in errors.windows(2) {
                let ratio = pair[0] / pair[1];
                assert!(
                    ratio > 0.75 * expected && ratio < 1.35 * expected,
                    "order {order}: errors {errors:?}, ratio {ratio} (expected about {expected})"
                );
            }
        }
    }
}
