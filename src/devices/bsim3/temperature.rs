//! Model-level temperature processing: C's model struct after `b3temp.c`'s
//! model pass ([`super::temp::temperature_model`]), before per-instance
//! effective geometry.
use super::Diagnostic;
use super::defaults::{ModelDefaults, model_member};
use super::fields::Model;
use super::temp::{self, ModelTemp};
use crate::devices::schema::temperature_kelvin;
use crate::primitives::{Real, SourceLoc, SpiceResult};

/// C's model struct after `b3temp.c`'s temperature pass, plus the function
/// locals its instance loop reads.
#[derive(Clone)]
pub struct TemperatureModel {
    pub(super) model: Model,
    pub(super) locals: ModelTemp,
    /// `CKTtemp` in kelvin.
    pub(super) kelvin: Real,
    /// Nonfatal `b3temp.c` messages in C order.
    pub warnings: Vec<Diagnostic>,
}

impl std::fmt::Debug for TemperatureModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemperatureModel")
            .field("kelvin", &self.kelvin)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

impl TemperatureModel {
    /// Apply C's model-level temperature pass. `temperature` is the circuit
    /// temperature in degrees Celsius (`CKTtemp` is Kelvin in C).
    ///
    /// # Errors
    /// A nonphysical temperature.
    pub fn new(
        defaults: &ModelDefaults,
        temperature: Real,
        location: &SourceLoc,
    ) -> SpiceResult<Self> {
        let kelvin = temperature_kelvin(temperature, location)?;
        let mut model = defaults.model.clone();
        let mut messages = Vec::new();
        let locals = temp::temperature_model(&mut model, kelvin, &mut messages);
        Ok(Self {
            model,
            locals,
            kelvin,
            warnings: messages
                .into_iter()
                .map(|message| Diagnostic::warning(message, "src/spicelib/devices/bsim3/b3temp.c"))
                .collect(),
        })
    }

    /// A processed value: `temp` (`CKTtemp`, K), a model keyword, or one of
    /// the model members `b3temp.c` derives (`vcrit`, `vtm`, `factor1`,
    /// `phib`, `phibsw`, `phibswg`).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Real> {
        let member = match name {
            "temp" => return Some(self.kelvin),
            "vcrit" => "BSIM3vcrit",
            "vtm" => "BSIM3vtm",
            "factor1" => "BSIM3factor1",
            "phib" => "BSIM3PhiB",
            "phibsw" => "BSIM3PhiBSW",
            "phibswg" => "BSIM3PhiBSWG",
            keyword => super::descriptor(super::parameters::MODEL, keyword)?.c_field?,
        };
        model_member(&self.model, member)
    }
}
