//! Model-only default processing: C's model struct as `b3set.c`'s model
//! pass ([`super::set::setup_model`]) leaves it, before instance geometry,
//! state/node allocation, temperature scaling or `b3check.c` diagnostics.
//! Keep this phase distinct from raw explicit inputs.
use super::fields::Model;
use super::{ModelParameters, Value, apply, descriptor, parameters};
use crate::devices::ModelContext;
use crate::devices::schema::temperature_kelvin;
use crate::primitives::{Real, SourceLoc, SpiceResult};

/// A model selector reset performed by `b3set.c`, with the given flag retained.
/// Device-layer code returns diagnostics rather than printing them.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelWarning {
    /// Canonical input keyword.
    pub parameter: &'static str,
    /// The value supplied to setup, after integer setter conversion.
    pub supplied: Real,
    /// The default selected by C's recovery rule.
    pub replacement: Real,
    /// Upstream behavior reference.
    pub c_reference: &'static str,
}

/// C's model struct after the model part of `b3set.c`, not a temperature-
/// or geometry-processed transistor. Zero-initialized but ungiven fields
/// (such as K1/K2) remain distinguishable through [`Self::is_given`].
#[derive(Clone)]
pub struct ModelDefaults {
    pub(super) model: Model,
    /// Effective NMOS (+1) or PMOS (-1), after ordered type flags.
    pub polarity: i8,
    /// VERSION's last string setter, or C's setup default `3.3.0`.
    pub version: String,
    /// C's `BSIM3cox`, calculated from TOX (F/m²).
    pub oxide_capacitance: Real,
    /// C's NQS selector reset warnings, in setup order.
    pub warnings: Vec<ModelWarning>,
}

impl std::fmt::Debug for ModelDefaults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelDefaults")
            .field("polarity", &self.polarity)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// The numeric C member `c_field` of `model` (an `int` member as a number).
pub(super) fn model_member(model: &Model, c_field: &str) -> Option<Real> {
    let mut copy = model.clone();
    if let Some(value) = copy.real_mut(c_field) {
        return Some(*value);
    }
    copy.int_mut(c_field).map(|value| Real::from(*value))
}

impl ModelDefaults {
    /// A numeric model field by keyword, including setup defaults and zero
    /// initialization. Flags and VERSION are exposed separately; aliases use
    /// the same C member.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Real> {
        let parameter = descriptor(parameters::MODEL, name)?;
        model_member(&self.model, parameter.c_field?)
    }

    /// C's given-bit state, not whether the final number differs from a
    /// default. Aliases (and the two type flags) share their actual C bit.
    #[must_use]
    pub fn is_given(&self, name: &str) -> bool {
        let Some(flag) = descriptor(parameters::MODEL, name).and_then(|p| p.c_given) else {
            return false;
        };
        let mut copy = self.model.clone();
        copy.flag_mut(flag).is_some_and(|given| *given)
    }

    /// Apply `b3temp.c`'s model-level temperature pass.
    ///
    /// # Errors
    /// Nonphysical temperature.
    pub fn at_temperature(
        &self,
        temperature: Real,
        location: &SourceLoc,
    ) -> SpiceResult<crate::devices::bsim3::temperature::TemperatureModel> {
        crate::devices::bsim3::temperature::TemperatureModel::new(self, temperature, location)
    }

    pub(super) fn from_parameters(
        input: &ModelParameters,
        context: &ModelContext,
    ) -> SpiceResult<Self> {
        context.validate(&input.location)?;
        let mut model = apply::model(input)?;
        let mut messages = Vec::new();
        super::set::setup_model(
            &mut model,
            temperature_kelvin(context.nominal_temperature, &input.location)?,
            &mut messages,
        );
        // C's `printf` resets of an invalid NQS selector, with the value the
        // deck supplied.
        let mut warnings = Vec::new();
        for (name, value) in [("nqsmod", model.nqsMod), ("acnqsmod", model.acnqsMod)] {
            let supplied = input
                .inputs
                .get(name)
                .and_then(|applied| match applied.value {
                    Value::Integer(v) => Some(Real::from(v)),
                    Value::Real(v) => Some(v),
                    _ => None,
                });
            if let Some(supplied) = supplied
                && supplied != Real::from(value)
            {
                warnings.push(ModelWarning {
                    parameter: name,
                    supplied,
                    replacement: Real::from(value),
                    c_reference: "src/spicelib/devices/bsim3/b3set.c (NQS selector reset)",
                });
            }
        }
        Ok(Self {
            polarity: input.polarity,
            version: model.version.clone(),
            oxide_capacitance: model.cox,
            warnings,
            model,
        })
    }
}
