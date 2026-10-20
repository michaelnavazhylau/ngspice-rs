//! BSIM3v3.3 input plumbing (B3-0 foundation, #90).
//!
//! This is deliberately **not a simulation backend**. The generated IFparm
//! interface facts cover the complete model/instance tables; the validated
//! projection applies ordered explicit setters and preserves provenance, and
//! [`defaults`], [`temperature`] and [`setup`] reproduce `b3set.c`/`b3temp.c`
//! defaults and `b3check.c` diagnostics, including effective geometry. Every
//! factory call still returns `NotYetPorted` after validation, before interning
//! nodes, allocating states or loading DC/AC/charge terms.

use crate::devices::ifparam::{Device, descriptor, version_text};
use crate::devices::models::{ModelFamily, ResolvedModel};
use crate::devices::schema::temperature_kelvin;
use crate::netlist::ast::{DeviceInstance, ModelCard, ParameterAssignment};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

pub use crate::devices::ifparam::{
    Access, AppliedParameter, Diagnostic, InputType, Inputs, Parameter, Value,
};

/// The BSIM3v3.3 interface descriptor the shared input plumbing uses.
pub(crate) const DEVICE: Device = Device {
    reference: "bsim3/b3.c",
    model: parameters::MODEL,
    instance: parameters::INSTANCE,
    uninteresting: parameters::UNINTERESTING,
    ic_names: ["icvds", "icvgs", "icvbs"],
    model_real: bsim3_model_real,
};

/// C's `IF_UNINTERESTING` rows, as `TABLE:keyword` pairs.
#[must_use]
pub fn uninteresting() -> &'static [&'static str] {
    DEVICE.uninteresting
}

/// `b3mpar.c`: TNOM adds CONSTCtoK; NPEAK/NGATE and their L/W/P
/// coefficients use distinct thresholds before a cm^-3 conversion.
fn bsim3_model_real(p: &Parameter, value: Real, location: &SourceLoc) -> SpiceResult<Real> {
    let threshold = match p.c_field {
        Some("BSIM3npeak" | "BSIM3lnpeak" | "BSIM3wnpeak" | "BSIM3pnpeak") => 1e20,
        Some("BSIM3ngate") => 1.000_001e24,
        Some("BSIM3lngate" | "BSIM3wngate" | "BSIM3pngate") => 1e23,
        _ => Real::INFINITY,
    };
    if p.canonical == "tnom" {
        temperature_kelvin(value, location)
    } else {
        Ok(if value > threshold {
            value * 1e-6
        } else {
            value
        })
    }
}

/// DC channel and junction evaluation from `b3ld.c`.
pub mod charges;
pub mod dc;
/// Model-only setup defaults and diagnostics, before geometry/temperature setup.
pub mod defaults;
mod defaults_data;
/// The device: `b3ld.c`'s DC load wired into the solver.
pub mod device;
/// BSIM3v3.3 noise generators from `b3noi.c`.
pub mod noise;
/// Generated IFparm order, identifiers and setter/given-field mappings.
pub mod parameters;
/// Effective geometry, size-dependent parameters and `b3check.c` diagnostics.
pub mod setup;
mod size_data;
/// Model-level temperature processing from `b3temp.c`.
pub mod temperature;

/// Select the BSIM3 directory from the **first** VERSION token, as
/// `inpdomod.c`/`INPfindVer` do. The eventual string setter still has last-set
/// precedence. Only the 3.3 family is being ported; no older model is substituted.
///
/// # Errors
/// Unsupported versions, or nonliteral version input.
pub fn select_version(card: &ModelCard) -> SpiceResult<&str> {
    let version = card
        .parameters
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("version"));
    let Some(p) = version else {
        return Ok("3.3.0");
    };
    let text = version_text(p)?;
    if text.starts_with("3.3") {
        return Ok(text);
    }
    let reference = if text.starts_with("3.0") {
        "src/spicelib/devices/bsim3v0/"
    } else if text.starts_with("3.1") {
        "src/spicelib/devices/bsim3v1/"
    } else if text.starts_with("3.2") {
        "src/spicelib/devices/bsim3v32/"
    } else {
        "src/spicelib/parser/inpdomod.c, inpfindv.c (no supported BSIM3 VERSION)"
    };
    Err(SpiceError::not_yet_ported(
        format!("{}: model '{}' BSIM3 version {text}", p.location, card.name),
        reference,
    ))
}

/// Validated explicit model inputs, distinct from a setup/defaulted model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelParameters {
    /// Applied model setters; unset/defaulted values are deliberately absent.
    pub inputs: Inputs,
    /// Last effective polarity flag, initially the model family (+1 NMOS/-1 PMOS).
    pub polarity: i8,
    instance_defaults: Vec<ParameterAssignment>,
    location: SourceLoc,
}

impl ModelParameters {
    /// Apply the model-only defaults from `b3set.c`, preserving given bits.
    /// This does not allocate instances or perform effective-size/temperature
    /// processing; only model-local TOX/TOXM/IJTH checks are included.
    ///
    /// # Errors
    /// Invalid context, nonpositive oxide thickness, negative IJTH, or a
    /// nonfinite dependent default. Geometry-dependent checks remain pending.
    pub fn model_defaults(
        &self,
        context: &crate::devices::ModelContext,
    ) -> SpiceResult<defaults::ModelDefaults> {
        defaults::ModelDefaults::from_parameters(self, context)
    }

    /// Full setup for one instance: `b3set.c` model defaults, `b3temp.c`
    /// model temperature pass, instance defaults/geometry and `b3check.c`
    /// diagnostics. No nodes, states or load terms are created.
    ///
    /// # Errors
    /// Invalid scale/instance shape, ACM geometry, fatal `b3check.c`
    /// diagnostics or nonfinite derived parameters.
    pub fn setup_instance(
        &self,
        instance: &DeviceInstance,
        scale: Real,
        context: &crate::devices::ModelContext,
    ) -> SpiceResult<setup::InstanceSetup> {
        let inputs = self.instance_parameters(instance, scale)?;
        let defaults = self.model_defaults(context)?;
        let temperature = defaults.at_temperature(context.temperature, &instance.location)?;
        let mut result =
            setup::InstanceSetup::new(&defaults, &temperature, &inputs, &instance.location)?;
        let mut warnings = temperature.warnings;
        warnings.append(&mut result.warnings);
        result.warnings = warnings;
        Ok(result)
    }

    /// Shared, mutable model state for setting up several instances of one
    /// model. `b3check.c` resets (CALCACM, CGDO/CGSO/CGBO) persist across
    /// instances in C's single model struct, so reuse one value per model.
    ///
    /// # Errors
    /// Invalid defaults/temperature processing.
    pub fn model_setup(
        &self,
        context: &crate::devices::ModelContext,
        location: &SourceLoc,
    ) -> SpiceResult<setup::ModelSetup> {
        let defaults = self.model_defaults(context)?;
        let temperature = defaults.at_temperature(context.temperature, location)?;
        setup::ModelSetup::new(defaults, temperature)
    }

    /// Apply reversed model-level instance defaults, then instance setters
    /// (`inpgmod.c`/`inpdpar.c`). C discards model-level M, not instance M.
    /// `scale` is the caller's explicit geometry scale; deck options which the
    /// port does not support remain refused before factory dispatch.
    ///
    /// # Errors
    /// Unknown/ask-only setters, invalid shapes, nonfinite input, integer or
    /// scaled-geometry overflow. Physical setup checks are not performed here.
    pub fn instance_parameters(
        &self,
        instance: &DeviceInstance,
        scale: Real,
    ) -> SpiceResult<Inputs> {
        if instance.designator != 'm'
            || instance.nodes.len() != 4
            || !scale.is_finite()
            || scale <= 0.
        {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "BSIM3 needs four-terminal M input and a finite positive scale",
            ));
        }
        let mut inputs = Inputs::new(&DEVICE, false);
        for p in self.instance_defaults.iter().chain(&instance.parameters) {
            inputs.apply(p, scale)?;
        }
        Ok(inputs)
    }
}

/// Validate explicit BSIM3 model setters, including their instance-default
/// fallback, in C order. This does **not** apply `b3set.c` defaults or claim
/// that `b3check.c`'s effective-geometry diagnostics have run.
///
/// # Errors
/// Wrong family/level/version, unknown/read-only setters or invalid typed input.
pub fn model_parameters(model: &ResolvedModel<'_>) -> SpiceResult<ModelParameters> {
    if !matches!(model.family(), ModelFamily::Nmos | ModelFamily::Pmos)
        || !matches!(model.levels().selector, 8 | 49)
    {
        return Err(SpiceError::parse(
            model.card().location.clone(),
            "BSIM3 input schema requires NMOS/PMOS level 8 or 49",
        ));
    }
    select_version(model.card())?;
    let mut result = ModelParameters {
        inputs: Inputs::new(&DEVICE, true),
        polarity: if model.family() == ModelFamily::Nmos {
            1
        } else {
            -1
        },
        instance_defaults: Vec::new(),
        location: model.card().location.clone(),
    };
    for p in &model.card().parameters {
        if p.name.eq_ignore_ascii_case("level") {
            continue;
        }
        if descriptor(parameters::MODEL, &p.name).is_some() {
            result.inputs.apply(p, 1.)?;
            if p.name.eq_ignore_ascii_case("nmos") {
                result.polarity = 1;
            }
            if p.name.eq_ignore_ascii_case("pmos") {
                result.polarity = -1;
            }
        } else {
            let mut check = Inputs::new(&DEVICE, false);
            check.apply(p, 1.)?;
            if !p.name.eq_ignore_ascii_case("m") {
                result.instance_defaults.push(p.clone());
            }
        }
    }
    result.instance_defaults.reverse();
    Ok(result)
}

/// Schema-only dispatch target. Never creates a dummy Device or a partial
/// circuit: the missing setup/load is an explicit factory error.
pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut crate::primitives::NodeTable,
    model: &ResolvedModel<'_>,
    context: &crate::devices::ModelContext,
) -> SpiceResult<Box<dyn crate::devices::Device>> {
    device::Bsim3::instantiate(instance, nodes, model, context)
}

#[cfg(test)]
mod tests {
    use super::{bsim3_model_real, descriptor, parameters};
    use crate::primitives::SourceLoc;
    use std::path::PathBuf;

    #[test]
    fn doping_setters_use_strict_binary64_thresholds_not_textual_approximations() {
        let location = SourceLoc::new(PathBuf::from("threshold.cir"), 1, 1);
        for (name, threshold) in [
            ("nch", 1e20_f64),
            ("lnch", 1e20),
            ("wnch", 1e20),
            ("pnch", 1e20),
            ("ngate", 1.000_001e24),
            ("lngate", 1e23),
            ("wngate", 1e23),
            ("pngate", 1e23),
        ] {
            let parameter = descriptor(parameters::MODEL, name).unwrap();
            let below = f64::from_bits(threshold.to_bits() - 1);
            let above = f64::from_bits(threshold.to_bits() + 1);
            assert_eq!(
                bsim3_model_real(parameter, below, &location).unwrap(),
                below,
                "{name}"
            );
            assert_eq!(
                bsim3_model_real(parameter, threshold, &location).unwrap(),
                threshold,
                "{name}"
            );
            assert_eq!(
                bsim3_model_real(parameter, above, &location).unwrap(),
                above * 1e-6,
                "{name}"
            );
        }
    }
}
