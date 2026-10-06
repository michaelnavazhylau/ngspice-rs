//! Numerical integration of charge-storage elements.
//!
//! When a capacitor or inductor is stamped into the MNA matrix, its current
//! depends on the history of its voltage (or current). ngspice turns that
//! dependence into an equivalent conductance plus a known current source — the
//! "companion model" — using a numerical integration rule:
//!
//! - **Trapezoidal** (order 2, the default), `src/maths/ni/niinteg.c`
//! - **Gear** (orders 1–6), selected by `.option method=gear`
//!
//! The C implementation stores the coefficients per integration order in
//! `src/maths/ni/nicomcof.c`, predicts the next value with
//! `src/maths/ni/nipred.c`, and handles the timestep changes that force a
//! coefficient recomputation.
//!
//! Only the types are ported here; [`Integrator::set_timestep`], integration
//! and prediction remain pending. The separately selected [`crate::diffsol`]
//! BDF adapter does not implement this SPICE companion-model contract.

use spice_core::{Real, SpiceError, SpiceResult};

use crate::C_REFERENCE_INTEGRATION;

/// Which integration rule to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntegrationMethod {
    /// Second-order trapezoidal rule. ngspice's default.
    #[default]
    Trapezoidal,
    /// Gear's backward differentiation formula of the given order, 1 to 6.
    Gear {
        /// Integration order.
        order: u8,
    },
}

impl IntegrationMethod {
    /// The highest Gear order ngspice supports.
    pub const MAX_GEAR_ORDER: u8 = 6;

    /// True when this method/order is representable, not proof that its
    /// coefficient/history operations are implemented.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self {
            Self::Trapezoidal => true,
            Self::Gear { order } => order >= 1 && order <= Self::MAX_GEAR_ORDER,
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

/// The current timestep and the time it lands on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timestep {
    /// Step size in seconds.
    pub dt: Real,
    /// Absolute time reached.
    pub time: Real,
}

impl Timestep {
    /// Builds a timestep.
    #[must_use]
    pub const fn new(dt: Real, time: Real) -> Self {
        Self { dt, time }
    }
}

/// Integration coefficients for one order, plus the history they apply to.
///
/// `coefficients[0]` multiplies the present value; the rest multiply past
/// values, most recent first.
#[derive(Debug, Clone, PartialEq)]
pub struct Integrator {
    method: IntegrationMethod,
    timestep: Timestep,
    coefficients: Vec<Real>,
}

impl Integrator {
    /// An integrator with no history.
    #[must_use]
    pub fn new(method: IntegrationMethod) -> Self {
        Self {
            method,
            timestep: Timestep::new(0.0, 0.0),
            coefficients: Vec::new(),
        }
    }

    /// The integration rule.
    #[must_use]
    pub const fn method(&self) -> IntegrationMethod {
        self.method
    }

    /// The current timestep.
    #[must_use]
    pub const fn timestep(&self) -> Timestep {
        self.timestep
    }

    /// The coefficients, once computed.
    #[must_use]
    pub fn coefficients(&self) -> &[Real] {
        &self.coefficients
    }

    /// The order currently in use: 2 for trapezoidal once started, the Gear
    /// order otherwise.
    #[must_use]
    pub const fn order(&self) -> u8 {
        match self.method {
            IntegrationMethod::Trapezoidal => 2,
            IntegrationMethod::Gear { order } => order,
        }
    }

    /// Recomputes the coefficients for a new timestep.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a nonpositive/nonfinite step, otherwise
    /// [`SpiceError::NotYetPorted`]. The C code is `niinteg.c` together with
    /// the coefficient tables in `nicomcof.c`.
    pub fn set_timestep(&mut self, timestep: Timestep) -> SpiceResult<()> {
        if !(timestep.dt.is_finite() && timestep.dt > 0.0) {
            return Err(SpiceError::Numerical {
                context: "integrator".to_owned(),
                message: format!("timestep must be positive and finite, got {}", timestep.dt),
            });
        }
        Err(SpiceError::not_yet_ported(
            "integration coefficients",
            C_REFERENCE_INTEGRATION,
        ))
    }

    /// Integrates `derivative` over the stored history.
    ///
    /// # Errors
    ///
    /// Always [`SpiceError::NotYetPorted`].
    pub fn integrate(&self, _derivative: &[Real]) -> SpiceResult<Real> {
        Err(SpiceError::not_yet_ported(
            "numerical integration",
            C_REFERENCE_INTEGRATION,
        ))
    }

    /// Predicts the next value, as `nipred.c` does before each Newton iteration.
    ///
    /// # Errors
    ///
    /// Always [`SpiceError::NotYetPorted`].
    pub fn predict(&self, _history: &[Real]) -> SpiceResult<Real> {
        Err(SpiceError::not_yet_ported(
            "predictor",
            "src/maths/ni/nipred.c",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{IntegrationMethod, Integrator, Timestep};

    #[test]
    fn trapezoidal_is_the_default() {
        assert_eq!(IntegrationMethod::default(), IntegrationMethod::Trapezoidal);
        assert!(IntegrationMethod::default().is_valid());
        assert_eq!(Integrator::new(IntegrationMethod::default()).order(), 2);
    }

    #[test]
    fn gear_orders_are_bounded() {
        assert!(IntegrationMethod::Gear { order: 1 }.is_valid());
        assert!(IntegrationMethod::Gear { order: 6 }.is_valid());
        assert!(!IntegrationMethod::Gear { order: 0 }.is_valid());
        assert!(!IntegrationMethod::Gear { order: 7 }.is_valid());
    }

    #[test]
    fn method_parsing_uses_the_option_spelling() {
        assert_eq!(
            IntegrationMethod::parse("TRAP", 2),
            Some(IntegrationMethod::Trapezoidal)
        );
        assert_eq!(
            IntegrationMethod::parse("gear", 4),
            Some(IntegrationMethod::Gear { order: 4 })
        );
        assert_eq!(IntegrationMethod::parse("euler", 1), None);
        assert_eq!(IntegrationMethod::Gear { order: 4 }.as_str(), "gear");
        assert_eq!(IntegrationMethod::Trapezoidal.as_str(), "trap");
    }

    #[test]
    fn a_non_positive_timestep_is_rejected_before_anything_else() {
        let mut integrator = Integrator::new(IntegrationMethod::Trapezoidal);
        let error = integrator
            .set_timestep(Timestep::new(0.0, 0.0))
            .unwrap_err();
        assert!(!error.is_not_yet_ported());
        assert!(error.to_string().contains("must be positive and finite"));
    }

    #[test]
    fn integration_reports_that_it_is_missing() {
        let mut integrator = Integrator::new(IntegrationMethod::Trapezoidal);
        let error = integrator
            .set_timestep(Timestep::new(1e-6, 1e-6))
            .unwrap_err();
        assert!(error.is_not_yet_ported());
        assert!(error.to_string().contains("src/maths/ni/niinteg.c"));
        assert!(
            integrator
                .integrate(&[1.0])
                .unwrap_err()
                .is_not_yet_ported()
        );
        assert!(integrator.predict(&[1.0]).unwrap_err().is_not_yet_ported());
    }
}
