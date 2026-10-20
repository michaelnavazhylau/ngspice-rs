//! Model-level temperature processing from `b3temp.c::BSIM3temp`, before
//! per-instance effective geometry. Junction-capacitance clamping, saturation
//! currents and potentials are C order and C constants; no load equations here.
use std::collections::BTreeMap;

use super::Diagnostic;
use super::defaults::ModelDefaults;
use crate::devices::schema::temperature_kelvin;
use crate::primitives::{Real, SourceLoc, SpiceResult};

// `b3temp.c` local constants, deliberately not the NIST values of `const.h`.
const KB_OVER_Q: Real = 8.617_087e-5;
const EPS_SI: Real = 1.035_94e-10;
const EPS_OX: Real = 3.453_133e-11;
// `main.c`: CONSTvt0 = CONSTboltz * REFTEMP / CHARGE (300.15 K).
const CONST_BOLTZ: Real = 1.380_648_52e-23;
const CONST_CHARGE: Real = 1.602_176_620_8e-19;
const REF_TEMP: Real = 27.0 + 273.15;
const CONST_VT0: Real = CONST_BOLTZ * REF_TEMP / CONST_CHARGE;
#[allow(clippy::approx_constant)] // `const.h`'s CONSTsqrt2 spelling
const CONST_ROOT2: Real = 1.414_213_562_373_095;
/// `b3temp.c`'s `Meter2Micron` equivalent used by the size-dependent setup.
pub(super) const METER2MICRON: Real = 1.0e6;

/// Model fields after `b3temp.c`'s temperature pass, plus the internal
/// constants the size-dependent setup needs.
#[derive(Debug, Clone, PartialEq)]
pub struct TemperatureModel {
    values: BTreeMap<&'static str, Real>,
    /// Nonfatal `b3temp.c` messages in C order.
    pub warnings: Vec<Diagnostic>,
}

impl TemperatureModel {
    /// Apply C's model-level temperature pass. `temperature` is the circuit
    /// temperature in degrees Celsius (`CKTtemp` is Kelvin in C).
    ///
    /// # Errors
    /// Nonfinite or nonphysical temperature, or nonfinite derived quantities.
    pub fn new(
        defaults: &ModelDefaults,
        temperature: Real,
        location: &SourceLoc,
    ) -> SpiceResult<Self> {
        let temp = temperature_kelvin(temperature, location)?;
        let tnom = defaults.get("tnom").ok_or_else(|| {
            crate::primitives::SpiceError::circuit("BSIM3 model defaults lost tnom")
        })?;
        if tnom <= 0. || !temp.is_finite() {
            return Err(crate::primitives::SpiceError::parse(
                location.clone(),
                "BSIM3 temperature processing needs a positive TNOM",
            ));
        }
        let number = |name: &str| {
            defaults.get(name).ok_or_else(|| {
                crate::primitives::SpiceError::circuit(format!("BSIM3 model defaults lost {name}"))
            })
        };
        let mut values = BTreeMap::new();
        let mut warnings = Vec::new();
        let mut clamped = BTreeMap::new();

        // Junction potentials are clamped before anything uses them. C prints
        // to stderr and keeps going.
        for (name, message) in [
            ("pb", "Given pb is less than 0.1. Pb is set to 0.1."),
            ("pbsw", "Given pbsw is less than 0.1. Pbsw is set to 0.1."),
            (
                "pbswg",
                "Given pbswg is less than 0.1. Pbswg is set to 0.1.",
            ),
        ] {
            let given = number(name)?;
            let value = if given < 0.1 {
                warnings.push(Diagnostic::warning(
                    message,
                    "src/spicelib/devices/bsim3/b3temp.c",
                ));
                0.1
            } else {
                given
            };
            values.insert(name, value);
            clamped.insert(name, value);
        }

        let vcrit = CONST_VT0 * (CONST_VT0 / (CONST_ROOT2 * 1.0e-14)).ln();
        let factor1 = (EPS_SI / EPS_OX * number("tox")?).sqrt();

        let vtm0 = KB_OVER_Q * tnom;
        let eg0 = 1.16 - 7.02e-4 * tnom * tnom / (tnom + 1108.0);
        let ni = 1.45e10
            * (tnom / 300.15)
            * (tnom / 300.15).sqrt()
            * (21.556_598_1 - eg0 / (2.0 * vtm0)).exp();
        let vtm = KB_OVER_Q * temp;
        let eg = 1.16 - 7.02e-4 * temp * temp / (temp + 1108.0);

        let (jct_temp_sat_cur, jct_sidewall_temp_sat_cur) = if temp != tnom {
            let exponent = eg0 / vtm0 - eg / vtm + number("xti")? * (temp / tnom).ln();
            let scale = (exponent / number("nj")?).exp();
            (number("js")? * scale, number("jsw")? * scale)
        } else {
            (number("js")?, number("jsw")?)
        };
        values.insert("jct_temp_sat_cur_density", jct_temp_sat_cur.max(0.));
        values.insert(
            "jct_sidewall_temp_sat_cur_density",
            jct_sidewall_temp_sat_cur.max(0.),
        );

        let delta_temp = temp - tnom;
        for (base, coefficient, temp_name, message) in [
            (
                "cj",
                "tcj",
                "unit_area_temp_jct_cap",
                "Temperature effect has caused cj to be negative. Cj is clamped to zero.",
            ),
            (
                "cjsw",
                "tcjsw",
                "unit_length_sidewall_temp_jct_cap",
                "Temperature effect has caused cjsw to be negative. Cjsw is clamped to zero.",
            ),
            (
                "cjswg",
                "tcjswg",
                "unit_length_gate_sidewall_temp_jct_cap",
                "Temperature effect has caused cjswg to be negative. Cjswg is clamped to zero.",
            ),
        ] {
            let scaled = number(coefficient)? * delta_temp;
            let cap = number(base)?;
            let value = if scaled >= -1.0 {
                cap * (1.0 + scaled)
            } else {
                // C only overwrites the field when the base cap is positive;
                // otherwise a zero-initialized model field is left behind.
                if cap > 0.0 {
                    warnings.push(Diagnostic::warning(
                        message,
                        "src/spicelib/devices/bsim3/b3temp.c",
                    ));
                }
                0.0
            };
            values.insert(temp_name, value);
        }

        let phi_b = (clamped["pb"] - number("tpb")? * delta_temp).max(0.01);
        let phi_bsw = (clamped["pbsw"] - number("tpbsw")? * delta_temp).max(0.01);
        let phi_bswg = (clamped["pbswg"] - number("tpbswg")? * delta_temp).max(0.01);
        if clamped["pb"] - number("tpb")? * delta_temp < 0.01 {
            warnings.push(Diagnostic::warning(
                "Temperature effect has caused pb to be less than 0.01. Pb is clamped to 0.01.",
                "src/spicelib/devices/bsim3/b3temp.c",
            ));
        }
        if clamped["pbsw"] - number("tpbsw")? * delta_temp <= 0.01 {
            warnings.push(Diagnostic::warning(
                "Temperature effect has caused pbsw to be less than 0.01. Pbsw is clamped to 0.01.",
                "src/spicelib/devices/bsim3/b3temp.c",
            ));
        }
        if clamped["pbswg"] - number("tpbswg")? * delta_temp <= 0.01 {
            warnings.push(Diagnostic::warning(
                "Temperature effect has caused pbswg to be less than 0.01. Pbswg is clamped to 0.01.",
                "src/spicelib/devices/bsim3/b3temp.c",
            ));
        }

        for (name, value) in [
            ("vcrit", vcrit),
            ("factor1", factor1),
            ("vtm", vtm),
            ("vtm0", vtm0),
            ("ni", ni),
            ("eg0", eg0),
            ("eg", eg),
            ("phi_b", phi_b),
            ("phi_bsw", phi_bsw),
            ("phi_bswg", phi_bswg),
            ("temp", temp),
            ("tnom", tnom),
        ] {
            if !value.is_finite() {
                return Err(crate::primitives::SpiceError::parse(
                    location.clone(),
                    format!("BSIM3 temperature processing produced nonfinite {name}"),
                ));
            }
            values.insert(name, value);
        }
        Ok(Self { values, warnings })
    }

    /// A processed value by its stable name (a model keyword or an internal
    /// name such as `vcrit`, `phi_b`).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Real> {
        self.values.get(name).copied()
    }
}
