//! BSIM3v3.3 input plumbing (B3-0 foundation, #90).
//!
//! This is deliberately **not a simulation backend**. The generated IFparm
//! interface facts cover the complete model/instance tables; the validated
//! projection applies ordered explicit setters and preserves provenance, and
//! [`defaults`], [`temperature`] and [`setup`] reproduce `b3set.c`/`b3temp.c`
//! defaults and `b3check.c` diagnostics, including effective geometry. Every
//! factory call still returns `NotYetPorted` after validation, before interning
//! nodes, allocating states or loading DC/AC/charge terms.

use crate::devices::models::{ModelFamily, ResolvedModel};
use crate::devices::schema::{ScalarUnit, finite_literal, temperature_kelvin};
use crate::netlist::ast::{DeviceInstance, ModelCard, ParameterAssignment, ParameterKind};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

/// DC channel and junction evaluation from `b3ld.c`.
pub mod charges;
pub mod dc;
/// Model-only setup defaults and diagnostics, before geometry/temperature setup.
pub mod defaults;
mod defaults_data;
/// The device: `b3ld.c`'s DC load wired into the solver.
pub mod device;
/// Generated IFparm order, identifiers and setter/given-field mappings.
pub mod parameters;
/// Effective geometry, size-dependent parameters and `b3check.c` diagnostics.
pub mod setup;
mod size_data;
/// Model-level temperature processing from `b3temp.c`.
pub mod temperature;

/// A setup message C prints (to stderr or `b3v33check.log`) instead of
/// returning it. Not a substitute for the eventual load diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// True for C's `Fatal_Flag` checks, which stop setup.
    pub fatal: bool,
    /// C's message text, without a trailing newline.
    pub message: String,
    /// Upstream file/function.
    pub c_reference: &'static str,
}

impl Diagnostic {
    /// A warning: C reports it and continues.
    #[must_use]
    pub fn warning(message: impl Into<String>, c_reference: &'static str) -> Self {
        Self {
            fatal: false,
            message: message.into(),
            c_reference,
        }
    }
    /// A fatal check: C reports it and fails setup.
    #[must_use]
    pub fn fatal(message: impl Into<String>, c_reference: &'static str) -> Self {
        Self {
            fatal: true,
            message: message.into(),
            c_reference,
        }
    }
}

/// The input value type declared by the C IFparm table, not a guessed unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    /// IF_REAL, finite in the Rust projection.
    Real,
    /// IF_INTEGER, `floor(value + 0.5)` with checked i32 conversion.
    Integer,
    /// IF_FLAG, a bare keyword (C supplies integer 1).
    Flag,
    /// IF_STRING, preserving the version spelling instead of parsing a number.
    String,
    /// IF_REALVEC, the IC vector expanded into its scalar component setters.
    RealVector,
}

/// C's public interface access, including ask-only rows that are not setters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// IP: settable, not askable.
    Input,
    /// OP: askable, not settable.
    Output,
    /// IOP/IOPR: settable and askable.
    ReadWrite,
}

/// One generated `b3.c` table entry, in C declaration order. Destination names
/// are factual identifiers, not copied C source or executable field access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    /// Public keyword, including redundant aliases.
    pub name: &'static str,
    /// First keyword with this ID; aliases share last-set precedence.
    pub canonical: &'static str,
    /// Numeric IFparm identifier from `bsim3def.h`.
    pub id: u16,
    /// C's symbolic identifier, for audits and future setter integration.
    pub c_id: &'static str,
    /// Setter destination; absent for ask-only rows and the aggregate IC vector.
    pub c_field: Option<&'static str>,
    /// C's given-bit destination, absent when the setter writes none.
    pub c_given: Option<&'static str>,
    /// C's input data type.
    pub input_type: InputType,
    /// Read/write capability.
    pub access: Access,
    /// Whether the table macro marks this a redundant alias (IOPR).
    pub redundant: bool,
}

impl Parameter {
    #[allow(clippy::too_many_arguments)] // Generated, reviewable IFparm records.
    const fn new(
        name: &'static str,
        canonical: &'static str,
        id: u16,
        c_id: &'static str,
        c_field: Option<&'static str>,
        c_given: Option<&'static str>,
        input_type: InputType,
        access: Access,
        redundant: bool,
    ) -> Self {
        Self {
            name,
            canonical,
            id,
            c_id,
            c_field,
            c_given,
            input_type,
            access,
            redundant,
        }
    }

    /// Reviewed input units for the unambiguous initial subset. `None` means
    /// not audited yet, **not dimensionless**. In particular mobility and
    /// geometry-binning coefficients need the setup phase's unit conventions.
    #[must_use]
    pub fn input_unit(&self) -> Option<ScalarUnit> {
        use ScalarUnit as U;
        match self.canonical {
            "l" | "w" | "pd" | "ps" | "tox" | "toxm" | "xj" | "xt" | "w0" | "nlx" | "lmin"
            | "lmax" | "wmin" | "wmax" | "lint" | "wint" | "xl" | "xw" => Some(U::Metre),
            "ad" | "as" => Some(U::SquareMetre),
            "icvds" | "icvgs" | "icvbs" | "vth0" | "vbx" | "vbm" | "voff" | "vfb" | "vfbcv" => {
                Some(U::Volt)
            }
            "tnom" => Some(U::Celsius),
            "nsub" | "nch" | "ngate" => Some(U::PerCubicCentimetre),
            "cgso" | "cgdo" | "cgbo" | "cjsw" | "cjswg" => Some(U::FaradPerMetre),
            "cj" => Some(U::FaradPerSquareMetre),
            "js" => Some(U::AmperePerSquareMetre),
            "rsh" => Some(U::Ohm),
            "m" | "nrd" | "nrs" | "mulu0" => Some(U::Dimensionless),
            _ if matches!(self.input_type, InputType::Flag | InputType::Integer) => {
                Some(U::Dimensionless)
            }
            _ => None,
        }
    }
}

/// An explicit input after `b3mpar.c`/`b3par.c` setter conversion, not after
/// setup or temperature processing. The AST retains the original spelling.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Finite scalar; TNOM is stored in Kelvin, as in `BSIM3mParam`.
    Real(Real),
    /// Checked C integer setter value.
    Integer(i32),
    /// Bare IF_FLAG input.
    Flag(bool),
    /// Unchanged bare string spelling.
    String(String),
}

/// One applied setter and its source, including repeated and alias setters.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedParameter {
    /// Descriptor of the keyword actually used, not just its canonical alias.
    pub parameter: &'static Parameter,
    /// The value stored by the setter.
    pub value: Value,
    /// Source of the original assignment (or IC component).
    pub location: SourceLoc,
}

/// Ordered explicit inputs. No zero/default values are fabricated for unset
/// fields. Aliases compare by C ID; `applied()` retains every duplicate.
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    table: &'static [Parameter],
    is_model: bool,
    applied: Vec<AppliedParameter>,
}

impl Inputs {
    fn new(is_model: bool) -> Self {
        Self {
            table: if is_model {
                parameters::MODEL
            } else {
                parameters::INSTANCE
            },
            is_model,
            applied: Vec::new(),
        }
    }
    /// Every applied setter, in application order; IC is expanded in C's
    /// VBS/VGS/VDS fallthrough order. This does not rewrite the source AST.
    #[must_use]
    pub fn applied(&self) -> &[AppliedParameter] {
        &self.applied
    }
    /// Last setter for a keyword or any alias of its C parameter ID.
    /// `None` means ungiven, not a default or a simulated ask value.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AppliedParameter> {
        let id = descriptor(self.table, name)?.id;
        self.applied
            .iter()
            .rev()
            .find(|value| value.parameter.id == id)
    }

    fn apply(&mut self, p: &ParameterAssignment, scale: Real) -> SpiceResult<()> {
        let descriptor = descriptor(self.table, &p.name)
            .ok_or_else(|| unsupported_setter(p, "unknown BSIM3 setter"))?;
        if descriptor.access == Access::Output {
            return Err(unsupported_setter(
                p,
                "ask-only BSIM3 parameter is not a setter",
            ));
        }
        if descriptor.input_type == InputType::RealVector {
            let ParameterKind::InitialConditions(components) = &p.kind else {
                return Err(unsupported_setter(p, "BSIM3 IC requires a numeric vector"));
            };
            let names = ["icvds", "icvgs", "icvbs"];
            if components.is_empty()
                || components.len() > names.len()
                || components.iter().zip(names).any(|(c, name)| c.name != name)
            {
                return Err(SpiceError::parse(
                    p.location.clone(),
                    "BSIM3 IC expects VDS[,VGS[,VBS]] in order",
                ));
            }
            for component in components.iter().rev() {
                self.apply(
                    &ParameterAssignment {
                        name: component.name.clone(),
                        value: component.value.text.clone(),
                        kind: ParameterKind::Scalar,
                        location: component.value.location.clone(),
                    },
                    scale,
                )?;
            }
            return Ok(());
        }
        let value = match descriptor.input_type {
            InputType::Real => {
                let mut value = finite_literal(p)?;
                if self.is_model {
                    value = model_real(descriptor, value, &p.location)?;
                } else {
                    // b3par.c multiplies left-to-right, not by scale^2:
                    // a small area can remain finite even when scale^2 overflows.
                    value = match descriptor.canonical {
                        "ad" | "as" => value * scale * scale,
                        "l" | "w" | "pd" | "ps" => value * scale,
                        _ => value,
                    };
                }
                if !value.is_finite() {
                    return Err(SpiceError::parse(
                        p.location.clone(),
                        "BSIM3 setter conversion overflow",
                    ));
                }
                Value::Real(value)
            }
            InputType::Integer => {
                let value = (finite_literal(p)? + 0.5).floor();
                if !(Real::from(i32::MIN)..=Real::from(i32::MAX)).contains(&value) {
                    return Err(SpiceError::parse(
                        p.location.clone(),
                        "BSIM3 integer setter overflows i32",
                    ));
                }
                Value::Integer(value as i32)
            }
            InputType::Flag if p.kind == ParameterKind::Flag && p.value.is_empty() => {
                Value::Flag(true)
            }
            InputType::String => Value::String(version_text(p)?.to_owned()),
            _ => {
                return Err(unsupported_setter(
                    p,
                    "BSIM3 setter has the wrong input shape",
                ));
            }
        };
        self.applied.push(AppliedParameter {
            parameter: descriptor,
            value,
            location: p.location.clone(),
        });
        Ok(())
    }
}

fn descriptor(table: &'static [Parameter], name: &str) -> Option<&'static Parameter> {
    table
        .iter()
        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
}
fn unsupported_setter(p: &ParameterAssignment, what: &str) -> SpiceError {
    SpiceError::Unsupported {
        feature: format!("{what}: '{}' (bsim3/b3.c)", p.name),
        location: Some(p.location.clone()),
    }
}

/// `b3mpar.c`: TNOM adds CONSTCtoK; NPEAK/NGATE and their L/W/P
/// coefficients use distinct thresholds before a cm^-3 conversion.
fn model_real(p: &Parameter, value: Real, location: &SourceLoc) -> SpiceResult<Real> {
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

fn version_text(p: &ParameterAssignment) -> SpiceResult<&str> {
    if !matches!(p.kind, ParameterKind::Scalar | ParameterKind::Textual)
        || p.value.is_empty()
        || !p
            .value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
    {
        return Err(unsupported_setter(
            p,
            "BSIM version must be one bare literal token, not an expression or quoted string",
        ));
    }
    if p.kind == ParameterKind::Scalar {
        finite_literal(p)?;
    }
    Ok(&p.value)
}

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
        let mut inputs = Inputs::new(false);
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
        inputs: Inputs::new(true),
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
            let mut check = Inputs::new(false);
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
    use super::{descriptor, model_real, parameters};
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
                model_real(parameter, below, &location).unwrap(),
                below,
                "{name}"
            );
            assert_eq!(
                model_real(parameter, threshold, &location).unwrap(),
                threshold,
                "{name}"
            );
            assert_eq!(
                model_real(parameter, above, &location).unwrap(),
                above * 1e-6,
                "{name}"
            );
        }
    }
}
