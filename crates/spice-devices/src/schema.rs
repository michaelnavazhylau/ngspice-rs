//! Typed scalar-schema extension points for device-owned model/instance setters.
//!
//! C: `inpgmod.c::create_model` and `inpdpar.c::INPdevParse` apply setters in
//! order. The AST stays raw/ordered; this validated projection stores the last
//! setter and its source, and distinguishes defaults from explicit values.
//! Unsupported setters error instead of C's warning-and-ignore behavior.

use std::collections::{BTreeMap, BTreeSet};

use spice_core::{Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number};
use spice_netlist::ast::ParameterAssignment;

/// Unit of a validated scalar. Add device-specific units as schemas land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarUnit {
    /// Dimensionless ratio or scale.
    Dimensionless,
    /// Current in amperes.
    Ampere,
    /// Resistance in ohms.
    Ohm,
    /// Input temperature in degrees Celsius (conversion is explicit).
    Celsius,
    /// Length in metres.
    Metre,
    /// Capacitance in farads.
    Farad,
    /// Inductance in henries.
    Henry,
    /// Area capacitance density in farads per square metre.
    FaradPerSquareMetre,
    /// Perimeter capacitance density in farads per metre.
    FaradPerMetre,
    /// First-order temperature coefficient, per Kelvin.
    InverseKelvin,
    /// Second-order temperature coefficient, per Kelvin squared.
    InverseKelvinSquared,
    /// Initial capacitor voltage in volts.
    Volt,
    /// Time or transit time in seconds.
    Second,
    /// MOS transconductance parameter, amperes per volt squared.
    AmperePerVoltSquared,
    /// Channel-length modulation coefficient, inverse volts.
    InverseVolt,
    /// Body-effect coefficient, square root of volts.
    SquareRootVolt,
    /// Activation (band-gap) energy in electron-volts.
    ElectronVolt,
    /// First-order band-gap correction, electron-volts per Kelvin.
    ElectronVoltPerKelvin,
    /// Voltage temperature coefficient, volts per Kelvin.
    VoltPerKelvin,
    /// Absolute temperature offset or scale in Kelvin (not converted).
    Kelvin,
}

/// Bounded validation domain, applied to every setter, not only the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarDomain {
    /// Any finite scalar.
    Finite,
    /// Finite and nonzero; negative scalar resistances remain supported.
    NonZero,
    /// Finite and greater than zero.
    Positive,
    /// Finite and at least zero.
    NonNegative,
    /// Celsius whose Kelvin conversion is finite and strictly positive.
    Temperature,
}

impl ScalarDomain {
    fn accepts(self, value: Real) -> bool {
        value.is_finite()
            && match self {
                Self::Finite => true,
                Self::NonZero => value != 0.0,
                Self::Positive => value > 0.0,
                Self::NonNegative => value >= 0.0,
                Self::Temperature => (value + 273.15).is_finite() && value + 273.15 > 0.0,
            }
    }
}

/// One supported canonical setter and its optional default.
/// No default means the value is absent until the owning device applies a
/// context-dependent default or checks that it is required.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalarParameter {
    /// Lowercase canonical setter name.
    pub name: &'static str,
    /// Unit of the scalar.
    pub unit: ScalarUnit,
    /// Permitted range.
    pub domain: ScalarDomain,
    /// Default if no explicit setter is present.
    pub default: Option<Real>,
}

/// A device's explicit scalar parameter surface. It does not resolve model
/// names, select backends, or evaluate parameter expressions.
#[derive(Debug, Clone, Copy)]
pub struct ScalarSchema<'a> {
    /// Supported setters. Names must be nonempty, unique and lowercase.
    pub parameters: &'a [ScalarParameter],
}

/// A scalar after validation, retaining last-set source provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarValue {
    /// Numeric value in `unit`.
    pub value: Real,
    /// Its unit; no implicit temperature conversion occurs.
    pub unit: ScalarUnit,
    /// Last setter location, or `None` for a schema default.
    pub location: Option<SourceLoc>,
}

/// Read-only last-set/default projection. The original AST remains unchanged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScalarValues(BTreeMap<String, ScalarValue>);

impl ScalarValues {
    /// Lookup by canonical name, case-insensitively. Absent optional values
    /// return `None`, rather than inventing a zero.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ScalarValue> {
        self.0.get(&name.to_ascii_lowercase())
    }
}

impl ScalarSchema<'_> {
    /// Validate every ordered scalar setter, apply defaults and retain last-set
    /// precedence. This accepts borrowed iterators so model selector `level`
    /// parameters can be consumed separately, without mutating the AST.
    ///
    /// # Errors
    /// Invalid schema definitions, nonliteral/nonfinite/range errors and unknown
    /// setters. Errors retain the setter location or `owner` for defaults.
    pub fn validate<'a>(
        &self,
        parameters: impl IntoIterator<Item = &'a ParameterAssignment>,
        owner: &SourceLoc,
    ) -> SpiceResult<ScalarValues> {
        let mut seen = BTreeSet::new();
        let mut values = ScalarValues::default();
        for parameter in self.parameters {
            if parameter.name.is_empty()
                || parameter.name != parameter.name.to_ascii_lowercase()
                || !seen.insert(parameter.name)
            {
                return Err(SpiceError::parse(
                    owner.clone(),
                    "invalid or duplicate schema setter name",
                ));
            }
            if let Some(default) = parameter.default {
                validate_value(default, parameter, owner)?;
                values.0.insert(
                    parameter.name.into(),
                    ScalarValue {
                        value: default,
                        unit: parameter.unit,
                        location: None,
                    },
                );
            }
        }
        for assignment in parameters {
            let parameter = self
                .parameters
                .iter()
                .find(|parameter| parameter.name.eq_ignore_ascii_case(&assignment.name))
                .ok_or_else(|| SpiceError::Unsupported {
                    feature: format!("unsupported setter '{}'", assignment.name),
                    location: Some(assignment.location.clone()),
                })?;
            let value = finite_literal(assignment)?;
            validate_value(value, parameter, &assignment.location)?;
            values.0.insert(
                parameter.name.into(),
                ScalarValue {
                    value,
                    unit: parameter.unit,
                    location: Some(assignment.location.clone()),
                },
            );
        }
        Ok(values)
    }
}

pub(crate) fn finite_literal(parameter: &ParameterAssignment) -> SpiceResult<Real> {
    if parameter.kind != spice_netlist::ast::ParameterKind::Scalar {
        return Err(SpiceError::Unsupported {
            feature: format!("non-scalar setter '{}'", parameter.name),
            location: Some(parameter.location.clone()),
        });
    }
    parse_spice_number(&parameter.value)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            SpiceError::parse(
                parameter.location.clone(),
                format!(
                    "expected finite scalar {}={}",
                    parameter.name, parameter.value
                ),
            )
        })
}

fn validate_value(
    value: Real,
    parameter: &ScalarParameter,
    location: &SourceLoc,
) -> SpiceResult<()> {
    if parameter.domain.accepts(value) {
        Ok(())
    } else {
        Err(SpiceError::parse(
            location.clone(),
            format!(
                "{} must satisfy {:?}, got {value}",
                parameter.name, parameter.domain
            ),
        ))
    }
}

/// Convert a validated physical temperature to Kelvin. C: `diompar.c` and
/// `dioparam.c` add `CONSTCtoK`; the port additionally rejects absolute zero,
/// negative absolute temperature and nonfinite input/output.
///
/// # Errors
/// Invalid temperature, preserving `location`.
pub fn temperature_kelvin(celsius: Real, location: &SourceLoc) -> SpiceResult<Real> {
    let parameter = ScalarParameter {
        name: "temperature",
        unit: ScalarUnit::Celsius,
        domain: ScalarDomain::Temperature,
        default: None,
    };
    validate_value(celsius, &parameter, location)?;
    Ok(celsius + 273.15)
}
