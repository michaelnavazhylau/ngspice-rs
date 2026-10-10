//! Bounded model-backed R/C/L input projection and effective values.
//!
//! References: `res/{ressetup,restemp}.c`, `cap/{capsetup,captemp}.c`,
//! `ind/{indsetup,indtemp}.c`, `cap/capacld.c` and `ind/indacld.c`.
//! Geometry/precedence follows those routines, without C's missing-resistance
//! fallback, tiny-resistor clamps, global geometry scale or ignored keywords.
//! Coil/Lundin geometry, exponential TC, DTEMP and AC-only resistance are gaps.
//!
//! ```
//! use std::path::Path;
//! use ngspice_rs::devices::{Circuit, ModelContext};
//! use ngspice_rs::netlist::{Parser, source::parse_deck_text};
//! let deck = parse_deck_text(Path::new("resistor.cir"),
//!     "model resistor\nr1 a 0 rm\n.model rm r(r=1k tc1=0.01)\n.end\n");
//! let netlist = Parser::new().parse_deck(&deck)?;
//! let mut circuit = Circuit::from_netlist(&netlist)?;
//! let hot = ModelContext::new(77.0, 27.0);
//! let system = circuit.linear_system_with_context(&hot)?;
//! assert_eq!(system.a.get(0, 0), 1.0 / 1500.0);
//! assert_eq!(circuit.linear_system()?.a.get(0, 0), 1.0 / 1000.0);
//! # Ok::<(), ngspice_rs::primitives::SpiceError>(())
//! ```

mod device;
pub(crate) use device::{instantiate, swept_literal};

use crate::devices::models::{ModelContext, ModelFamily, ResolvedModel};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter, ScalarSchema, ScalarUnit as U, ScalarValues,
    temperature_kelvin,
};
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

/// Immutable validated input recipe, not a mutable temperature-adjusted cache.
/// Explicit instance values override model defaults; model-specific geometry
/// precedence is preserved. Derived values are recomputed for every context.
#[derive(Debug, Clone, PartialEq)]
pub struct PassiveParameters {
    family: ModelFamily,
    nominal_value: Real,
    temperature: Option<Real>,
    nominal_temperature: Option<Real>,
    tc1: Real,
    tc2: Real,
    scale: Real,
    multiplicity: Real,
    initial_condition: Option<Real>,
    /// Resistor `.noise` inputs (`resnoise.c`); the defaults for C and L.
    noisy: bool,
    kf: Real,
    af: Real,
    ef: Real,
    noise_area: Real,
    location: SourceLoc,
    /// The validated model and instance setters as written (schema defaults
    /// unlocated), for `.sens`, which replays C's own records
    /// ([`crate::devices::sensitivity`]).
    model_values: ScalarValues,
    instance_values: ScalarValues,
}

impl PassiveParameters {
    pub(crate) fn literal(family: ModelFamily, value: Real, ic: Option<Real>) -> SpiceResult<Self> {
        let location = SourceLoc::new(std::path::PathBuf::from("<literal passive>"), 0, 0);
        let primary = match family {
            ModelFamily::Resistor => "resistance",
            ModelFamily::Capacitor => "capacitance",
            _ => "inductance",
        };
        let mut instance_values = ScalarSchema {
            parameters: &instance_schema(family, primary),
        }
        .validate(&[], &location)?;
        instance_values.set(primary, value, U::Dimensionless, location.clone());
        Ok(Self {
            family,
            nominal_value: value,
            temperature: None,
            nominal_temperature: None,
            tc1: 0.,
            tc2: 0.,
            scale: 1.,
            multiplicity: 1.,
            initial_condition: ic,
            noisy: true,
            kf: 0.,
            af: 1.,
            ef: 1.,
            noise_area: 1.,
            model_values: model_schema(family).validate(&[], &location)?,
            instance_values,
            location,
        })
    }

    /// C's DEVparam then DEVtemperature, without rerunning DEVsetup.
    pub(crate) fn swept(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Self> {
        let mut p = self.clone();
        let primary = match self.family {
            ModelFamily::Resistor => "resistance",
            ModelFamily::Capacitor => "capacitance",
            _ => "inductance",
        };
        let definitions = instance_schema(self.family, primary);
        let definition = definitions
            .iter()
            .find(|d| d.name == parameter)
            .ok_or_else(|| {
                SpiceError::circuit(format!("passive parameter {parameter} cannot be swept"))
            })?;
        crate::devices::sweep::check_swept(
            "passive",
            parameter,
            value,
            match parameter {
                "temp" => value > -273.15,
                "w" | "l" | "m" | "scale" => value > 0.,
                _ => true,
            },
            "in its finite physical domain",
        )?;
        p.instance_values
            .set(parameter, value, definition.unit, self.location.clone());
        match parameter {
            name if name == primary => {
                valid_value(self.family, value, &self.location)?;
                p.nominal_value = value;
            }
            "temp" => p.temperature = Some(value),
            "tc1" => p.tc1 = value,
            "tc2" => p.tc2 = value,
            "m" => p.multiplicity = value,
            "scale" => p.scale = value,
            "ic" => p.initial_condition = Some(value),
            "w" | "l" => {
                // restemp.c re-derives geometry only without a given resistance;
                // CAPsetup's geometry is setup-only, not rerun by a DC setter.
                if self.family == ModelFamily::Resistor
                    && optional(&p.instance_values, primary).is_none()
                {
                    p.nominal_value = model_value(
                        self.family,
                        &p.model_values,
                        &p.instance_values,
                        &p.location,
                    )?;
                }
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "passive parameter {parameter} cannot be swept"
                )));
            }
        }
        p.effective_value(context)?;
        Ok(p)
    }

    /// Validated resistor/capacitor/inductor family.
    #[must_use]
    pub const fn family(&self) -> ModelFamily {
        self.family
    }
    /// Unadjusted single-instance R/C/L in ohms/farads/henries, respectively.
    /// Does not include temperature, scale or multiplicity.
    #[must_use]
    pub const fn nominal_value(&self) -> Real {
        self.nominal_value
    }
    /// Positive parallel multiplier (default 1).
    #[must_use]
    pub const fn multiplicity(&self) -> Real {
        self.multiplicity
    }
    /// A copy whose supplied scalar is `value`, with the same TC, TEMP/TNOM,
    /// scale and multiplicity. This is the immutable form of an explicit instance
    /// value, which outranks model/geometry bases; the original is unchanged.
    ///
    /// # Errors
    /// `value` is nonfinite, or violates the family's value domain (resistance
    /// must be nonzero with finite conductance; C/L must be positive).
    pub fn with_nominal_value(&self, value: Real) -> SpiceResult<Self> {
        valid_value(self.family, value, &self.location)?;
        Ok(Self {
            nominal_value: value,
            ..self.clone()
        })
    }
    /// The validated model setters (schema defaults have no location).
    #[must_use]
    pub const fn model_values(&self) -> &ScalarValues {
        &self.model_values
    }
    /// The validated instance setters (schema defaults have no location).
    #[must_use]
    pub const fn instance_values(&self) -> &ScalarValues {
        &self.instance_values
    }
    /// Capacitor initial volts or inductor initial amperes, if given.
    /// Applied by the companion transient under `uic` (see
    /// `Device::storage_element`), ignored otherwise as in C.
    #[must_use]
    pub const fn initial_condition(&self) -> Option<Real> {
        self.initial_condition
    }

    /// The resistor's `.noise` description (`resnoise.c`; `ressetup.c`
    /// defaults `KF = 0`, `AF = EF = LF = WF = 1`, `noisy = 1`), for `model`.
    #[must_use]
    pub fn resistor_noise(&self, model: &str) -> crate::devices::ResistorNoise {
        crate::devices::ResistorNoise {
            noisy: self.noisy,
            kf: self.kf,
            af: self.af,
            ef: self.ef,
            area: self.noise_area,
            multiplicity: self.multiplicity,
            temperature: self.temperature,
            model: Some(model.to_owned()),
        }
    }

    /// Effective value to pass to the existing scalar equation stamps.
    /// `f=1+TC1*dT+TC2*dT^2`; R/L use `nominal*f*scale/m`, C uses
    /// `nominal*f*scale*m`. R's polynomial uses C's Horner evaluation order.
    /// Instance TEMP overrides circuit TEMP; model TNOM overrides context TNOM.
    /// Temperature subtraction uses Kelvin conversions just like the C setters.
    ///
    /// # Errors
    /// Invalid context, nonpositive/nonfinite factor, overflow, underflow to
    /// zero, or nonfinite resistor conductance. No cache/state is mutated.
    pub fn effective_value(&self, context: &ModelContext) -> SpiceResult<Real> {
        let scaled = self.scaled_value(context)?;
        let value = if self.family == ModelFamily::Capacitor {
            scaled * self.multiplicity
        } else {
            scaled / self.multiplicity
        };
        valid_value(self.family, value, &self.location)?;
        Ok(value)
    }

    /// The temperature-adjusted and scaled value before multiplicity: C's
    /// `INDinduct` for an inductor, which `MUTtemp` (`ind/muttemp.c`) uses
    /// for `M = k sqrt(|L1 L2|)` while the inductor itself stamps
    /// `INDinduct / m`.
    ///
    /// # Errors
    /// As [`Self::effective_value`].
    pub fn coupling_value(&self, context: &ModelContext) -> SpiceResult<Real> {
        let scaled = self.scaled_value(context)?;
        valid_value(self.family, scaled, &self.location)?;
        Ok(scaled)
    }

    fn scaled_value(&self, context: &ModelContext) -> SpiceResult<Real> {
        context.validate(&self.location)?;
        let temperature = temperature_kelvin(
            self.temperature.unwrap_or(context.temperature),
            &self.location,
        )?;
        let nominal = temperature_kelvin(
            self.nominal_temperature
                .unwrap_or(context.nominal_temperature),
            &self.location,
        )?;
        let dt = temperature - nominal;
        finite(dt, &self.location, "temperature difference")?;
        let factor = if self.family == ModelFamily::Resistor {
            let quadratic = finite(self.tc2 * dt, &self.location, "quadratic coefficient")?;
            let slope = finite(quadratic + self.tc1, &self.location, "temperature slope")?;
            finite(slope * dt, &self.location, "temperature correction")? + 1.0
        } else {
            let linear = finite(self.tc1 * dt, &self.location, "linear correction")?;
            let quadratic = finite(self.tc2 * dt, &self.location, "quadratic coefficient")?;
            let quadratic = finite(quadratic * dt, &self.location, "quadratic correction")?;
            finite(1.0 + linear, &self.location, "linear temperature factor")? + quadratic
        };
        positive(factor, &self.location, "temperature factor")?;
        let adjusted = finite(
            self.nominal_value * factor,
            &self.location,
            "temperature-adjusted value",
        )?;
        finite(adjusted * self.scale, &self.location, "scaled value")
    }
}

impl ResolvedModel<'_> {
    /// Project bounded passive model/instance setters without changing the AST.
    /// All setters are validated even when a higher-precedence scalar makes a
    /// geometry branch unnecessary. Unsupported names never get ignored.
    ///
    /// # Errors
    /// Wrong family/reference/ports, unknown or invalid setters, missing values
    /// or geometry, and nonfinite/nonpositive effective geometry/base values.
    /// Temperature/scale/multiplicity derivation is checked by `effective_value`.
    pub fn passive_parameters(&self, instance: &DeviceInstance) -> SpiceResult<PassiveParameters> {
        let family = self.family();
        let primary = match family {
            ModelFamily::Resistor => "resistance",
            ModelFamily::Capacitor => "capacitance",
            ModelFamily::Inductor => "inductance",
            _ => {
                return Err(SpiceError::parse(
                    self.card().location.clone(),
                    "passive schema requires an R/C/L model",
                ));
            }
        };
        if instance.nodes.len() != 2
            || !instance
                .designator
                .eq_ignore_ascii_case(&family.designator())
            || !instance
                .model
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&self.card().name))
        {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "instance does not reference this two-terminal passive model",
            ));
        }
        let model = self.parameters(&model_schema(family))?;
        let definitions = instance_schema(family, primary);
        let instance_values = ScalarSchema {
            parameters: &definitions,
        }
        .validate(&instance.parameters, &instance.location)?;
        let location = &instance.location;
        let base = if let Some(value) = optional(&instance_values, primary) {
            value
        } else {
            model_value(family, &model, &instance_values, location)?
        };
        valid_value(family, base, location)?;
        // ressetup.c: RESeffNoiseArea uses the instance geometry (L/W
        // defaulting to the model's) only when the instance gives L or W.
        let noise_area = if family == ModelFamily::Resistor
            && (optional(&instance_values, "l").is_some()
                || optional(&instance_values, "w").is_some())
        {
            let length = optional(&instance_values, "l").unwrap_or(value(&model, "l", 10e-6));
            let width = optional(&instance_values, "w").unwrap_or(value(&model, "defw", 10e-6));
            (length - 2.0 * value(&model, "short", 0.0)).powf(value(&model, "lf", 1.0))
                * (width - 2.0 * value(&model, "narrow", 0.0)).powf(value(&model, "wf", 1.0))
        } else {
            1.0
        };
        Ok(PassiveParameters {
            family,
            nominal_value: base,
            temperature: optional(&instance_values, "temp"),
            nominal_temperature: optional(&model, "tnom"),
            tc1: optional(&instance_values, "tc1").unwrap_or(value(&model, "tc1", 0.0)),
            tc2: optional(&instance_values, "tc2").unwrap_or(value(&model, "tc2", 0.0)),
            scale: value(&instance_values, "scale", 1.0),
            multiplicity: value(&instance_values, "m", 1.0),
            initial_condition: optional(&instance_values, "ic"),
            // `noisy` is an IF_INTEGER setter: floor(value + 0.5).
            noisy: optional(&instance_values, "noisy").is_none_or(|v| (v + 0.5).floor() != 0.0),
            kf: value(&model, "kf", 0.0),
            af: value(&model, "af", 1.0),
            ef: value(&model, "ef", 1.0),
            noise_area,
            location: location.clone(),
            model_values: model,
            instance_values,
        })
    }
}

fn model_value(
    family: ModelFamily,
    model: &ScalarValues,
    instance: &ScalarValues,
    location: &SourceLoc,
) -> SpiceResult<Real> {
    match family {
        ModelFamily::Resistor => {
            let sheet = value(model, "rsh", 0.0);
            // RESsetup supplies positive default L/W (10 um); positive RSH
            // therefore takes precedence over model R, even without L/W given.
            if sheet > 0.0 {
                let length = optional(instance, "l").unwrap_or(value(model, "l", 10e-6));
                let width = optional(instance, "w").unwrap_or(value(model, "defw", 10e-6));
                let length = corrected(length, 2.0 * value(model, "short", 0.0), location, "effective resistor length")?;
                let width = corrected(width, 2.0 * value(model, "narrow", 0.0), location, "effective resistor width")?;
                let squares = finite(length / width, location, "resistor square count")?;
                finite(squares * sheet, location, "sheet resistance")
            } else {
                optional(model, "r").ok_or_else(|| SpiceError::parse(location.clone(), "resistor requires an instance resistance, model r or positive rsh; no implicit 1 mOhm fallback"))
            }
        }
        ModelFamily::Capacitor => {
            if let Some(cap) = optional(model, "cap") { return Ok(cap); }
            // CAPsetup stores DEFL but initializes missing instance length to
            // zero without applying it. Require explicit positive L instead.
            let length = optional(instance, "l").ok_or_else(|| SpiceError::parse(location.clone(), "capacitor geometry requires instance l; C setup does not apply model defl"))?;
            let width = optional(instance, "w").unwrap_or(value(model, "defw", 10e-6));
            let length = corrected(length, value(model, "short", 0.0), location, "effective capacitor length")?;
            let width = corrected(width, value(model, "narrow", 0.0), location, "effective capacitor width")?;
            let area = finite(width * length, location, "capacitor area")?;
            let bottom = finite(value(model, "cj", 0.0) * area, location, "bottom capacitance")?;
            let perimeter = finite(2.0 * finite(length + width, location, "capacitor dimension sum")?, location, "capacitor perimeter")?;
            let sidewall = finite(value(model, "cjsw", 0.0) * perimeter, location, "sidewall capacitance")?;
            finite(bottom + sidewall, location, "geometry capacitance")
        }
        _ => optional(model, "ind").ok_or_else(|| SpiceError::parse(location.clone(), "inductor requires an instance inductance or model ind; coil geometry is unsupported")),
    }
}

const fn p(name: &'static str, unit: U, domain: D, default: Option<Real>) -> ScalarParameter {
    ScalarParameter {
        name,
        unit,
        domain,
        default,
    }
}
fn model_schema(family: ModelFamily) -> ScalarSchema<'static> {
    const R: &[ScalarParameter] = &[
        p("r", U::Ohm, D::Positive, None),
        p("rsh", U::Ohm, D::NonNegative, Some(0.0)),
        p("defw", U::Metre, D::Positive, Some(10e-6)),
        p("l", U::Metre, D::Positive, Some(10e-6)),
        p("narrow", U::Metre, D::NonNegative, Some(0.0)),
        p("short", U::Metre, D::NonNegative, Some(0.0)),
        p("tc1", U::InverseKelvin, D::Finite, Some(0.0)),
        p("tc2", U::InverseKelvinSquared, D::Finite, Some(0.0)),
        p("tnom", U::Celsius, D::Temperature, None),
        p("kf", U::Dimensionless, D::Finite, Some(0.0)),
        p("af", U::Dimensionless, D::Finite, Some(1.0)),
        p("ef", U::Dimensionless, D::Finite, Some(1.0)),
        p("lf", U::Dimensionless, D::Finite, Some(1.0)),
        p("wf", U::Dimensionless, D::Finite, Some(1.0)),
    ];
    const C: &[ScalarParameter] = &[
        p("cap", U::Farad, D::Positive, None),
        p("cj", U::FaradPerSquareMetre, D::NonNegative, Some(0.0)),
        p("cjsw", U::FaradPerMetre, D::NonNegative, Some(0.0)),
        p("defw", U::Metre, D::Positive, Some(10e-6)),
        p("narrow", U::Metre, D::NonNegative, Some(0.0)),
        p("short", U::Metre, D::NonNegative, Some(0.0)),
        p("tc1", U::InverseKelvin, D::Finite, Some(0.0)),
        p("tc2", U::InverseKelvinSquared, D::Finite, Some(0.0)),
        p("tnom", U::Celsius, D::Temperature, None),
    ];
    const L: &[ScalarParameter] = &[
        p("ind", U::Henry, D::Positive, None),
        p("tc1", U::InverseKelvin, D::Finite, Some(0.0)),
        p("tc2", U::InverseKelvinSquared, D::Finite, Some(0.0)),
        p("tnom", U::Celsius, D::Temperature, None),
    ];
    ScalarSchema {
        parameters: match family {
            ModelFamily::Resistor => R,
            ModelFamily::Capacitor => C,
            _ => L,
        },
    }
}
fn instance_schema(family: ModelFamily, primary: &'static str) -> Vec<ScalarParameter> {
    let unit = match family {
        ModelFamily::Resistor => U::Ohm,
        ModelFamily::Capacitor => U::Farad,
        _ => U::Henry,
    };
    let domain = if family == ModelFamily::Resistor {
        D::NonZero
    } else {
        D::Positive
    };
    let mut definitions = vec![
        p(primary, unit, domain, None),
        p("temp", U::Celsius, D::Temperature, None),
        p("tc1", U::InverseKelvin, D::Finite, None),
        p("tc2", U::InverseKelvinSquared, D::Finite, None),
        p("m", U::Dimensionless, D::Positive, Some(1.0)),
        p("scale", U::Dimensionless, D::Positive, Some(1.0)),
    ];
    if family != ModelFamily::Inductor {
        definitions.extend([
            p("l", U::Metre, D::Positive, None),
            p("w", U::Metre, D::Positive, None),
        ]);
    }
    if family == ModelFamily::Resistor {
        definitions.push(p("noisy", U::Dimensionless, D::Finite, None));
    }
    if family != ModelFamily::Resistor {
        definitions.push(p(
            "ic",
            if family == ModelFamily::Capacitor {
                U::Volt
            } else {
                U::Ampere
            },
            D::Finite,
            None,
        ));
    }
    definitions
}
fn optional(values: &ScalarValues, name: &str) -> Option<Real> {
    values.get(name).map(|v| v.value)
}
fn value(values: &ScalarValues, name: &str, default: Real) -> Real {
    optional(values, name).unwrap_or(default)
}
fn finite(value: Real, location: &SourceLoc, name: &str) -> SpiceResult<Real> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(SpiceError::parse(
            location.clone(),
            format!("nonfinite/overflow {name}"),
        ))
    }
}
fn positive(value: Real, location: &SourceLoc, name: &str) -> SpiceResult<Real> {
    finite(value, location, name)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(SpiceError::parse(
            location.clone(),
            format!("{name} must be positive"),
        ))
    }
}
fn corrected(value: Real, correction: Real, location: &SourceLoc, name: &str) -> SpiceResult<Real> {
    finite(correction, location, "geometry correction")?;
    positive(value - correction, location, name)
}
fn valid_value(family: ModelFamily, value: Real, location: &SourceLoc) -> SpiceResult<()> {
    finite(value, location, "effective passive value")?;
    if family == ModelFamily::Resistor {
        if value == 0.0 || !(1.0 / value).is_finite() {
            return Err(SpiceError::parse(
                location.clone(),
                "resistance must be nonzero with finite conductance",
            ));
        }
    } else {
        positive(value, location, "effective passive value")?;
    }
    Ok(())
}
