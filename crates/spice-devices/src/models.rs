//! Top-level model resolution, backend selectors and initial diode schemas.
//!
//! C: `inpmkmod.c::INPmakeMod` keeps the first declaration; `inpdomod.c` selects
//! a backend using `inpfindl.c::INPfindLev`'s first level. `inpgmod.c` applies
//! ordered setters. Resolution/validation are not proof of a simulation backend.
//! No scoped expansion, binning, global state or nonlinear arithmetic lives here.
//!
//! ```
//! use std::path::Path;
//! use spice_devices::{ModelContext, ModelResolver};
//! use spice_netlist::{Parser, source::parse_deck_text};
//! let deck = parse_deck_text(Path::new("diode.cir"),
//!     "diode inputs\nd1 a 0 mdl temp=40\n.model mdl d(is=2e-14 tnom=30)\n.end\n");
//! let netlist = Parser::new().parse_deck(&deck)?;
//! let resolver = ModelResolver::new(&netlist.models)?;
//! let model = resolver.resolve(&netlist.devices[0])?.expect("diode reference");
//! let context = ModelContext::default();
//! assert_eq!(model.diode_parameters(&context)?.saturation_current, 2e-14);
//! assert_eq!(model.diode_instance_parameters(&netlist.devices[0], &context)?
//!     .temperature_kelvin, 313.15);
//! // Validated inputs do not imply an available diode simulation factory.
//! # Ok::<(), spice_core::SpiceError>(())
//! ```

use std::collections::BTreeMap;

use spice_core::{Real, SourceLoc, SpiceError, SpiceResult};
use spice_netlist::ast::{DeviceInstance, ModelCard};

use crate::schema::{
    ScalarDomain, ScalarParameter, ScalarSchema, ScalarUnit, ScalarValues, finite_literal,
    temperature_kelvin,
};
use crate::sweep::{MAX_RESISTOR_OVERRIDES, ResistorOverride};

/// Parsed model family, including polarity without rewriting the raw AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFamily {
    /// Scalar resistor (`r` or `res`).
    Resistor,
    /// Scalar capacitor.
    Capacitor,
    /// Scalar inductor.
    Inductor,
    /// Junction diode.
    Diode,
    /// NPN classic BJT.
    Npn,
    /// PNP classic BJT.
    Pnp,
    /// NMOS (bounded selector for level 1).
    Nmos,
    /// PMOS (bounded selector for level 1).
    Pmos,
}

impl ModelFamily {
    /// Family associated with an ordinary parsed model type.
    #[must_use]
    pub fn from_base(base: &str) -> Option<Self> {
        match base.to_ascii_lowercase().as_str() {
            "r" | "res" => Some(Self::Resistor),
            "c" => Some(Self::Capacitor),
            "l" => Some(Self::Inductor),
            "d" => Some(Self::Diode),
            "npn" => Some(Self::Npn),
            "pnp" => Some(Self::Pnp),
            "nmos" => Some(Self::Nmos),
            "pmos" => Some(Self::Pmos),
            _ => None,
        }
    }

    /// Compatible instance designator. Node and model names have separate
    /// namespaces; model lookup never applies the `gnd` alias.
    #[must_use]
    pub const fn designator(self) -> char {
        match self {
            Self::Resistor => 'r',
            Self::Capacitor => 'c',
            Self::Inductor => 'l',
            Self::Diode => 'd',
            Self::Npn | Self::Pnp => 'q',
            Self::Nmos | Self::Pmos => 'm',
        }
    }
}

/// Backend selection is distinct from ordered model setters. Only diode has a
/// `level` integer setter in the initial family tables; BJT/MOS/passive `level`
/// assignments are selector-only (inpgmod.c consumes them without setting data).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelSelection {
    /// First raw scalar level, or `None` if omitted. No AST mutation occurs.
    pub first_raw: Option<Real>,
    /// Backend selector: first-level rounding for BJT/MOS/`r`; fixed 1 for
    /// diode, capacitor, inductor and `res`.
    pub selector: u8,
    /// Diode's last integer setter (`floor(raw + 0.5)`), defaulting to 1.
    /// `None` for families with no level setter.
    pub applied: Option<u8>,
}

/// Immutable lookup over one deck's top-level declarations. Duplicate names
/// retain the *first* card as INPmakeMod does; all cards stay visible in the AST.
/// No validation of a shadowed card is implied, nor any cross-deck state.
#[derive(Debug)]
pub struct ModelResolver<'a> {
    definitions: BTreeMap<String, &'a ModelCard>,
}

impl<'a> ModelResolver<'a> {
    /// Index model names independently of node symbols or backend availability.
    ///
    /// # Errors
    /// Empty model names in a programmatically constructed AST.
    pub fn new(cards: &'a [ModelCard]) -> SpiceResult<Self> {
        let mut definitions = BTreeMap::new();
        for card in cards {
            if card.name.is_empty() {
                return Err(SpiceError::parse(card.location.clone(), "empty model name"));
            }
            definitions
                .entry(card.name.to_ascii_lowercase())
                .or_insert(card);
        }
        Ok(Self { definitions })
    }

    /// Raw first declaration by case-insensitive name; no ground aliasing.
    #[must_use]
    pub fn model(&self, name: &str) -> Option<&'a ModelCard> {
        self.definitions.get(&name.to_ascii_lowercase()).copied()
    }

    /// Resolve an instance reference and validate its family and selector.
    /// Literal R/C/L/V/I may have no reference; D/Q/M require one. This does not
    /// validate arbitrary device keywords or make an unavailable factory work.
    ///
    /// # Errors
    /// Missing/wrong-family models, unsupported levels, invalid selector data.
    /// Diagnostics retain the instance/card/setter location as appropriate.
    pub fn resolve(&self, instance: &DeviceInstance) -> SpiceResult<Option<ResolvedModel<'a>>> {
        let Some(name) = &instance.model else {
            if matches!(instance.designator.to_ascii_lowercase(), 'd' | 'q' | 'm') {
                return Err(SpiceError::parse(
                    instance.location.clone(),
                    format!("device {} requires a model", instance.name),
                ));
            }
            return Ok(None);
        };
        let card = self.model(name).ok_or_else(|| {
            SpiceError::parse(
                instance.location.clone(),
                format!(
                    "model '{name}' referenced by {} is not defined",
                    instance.name
                ),
            )
        })?;
        let family = family(card)?;
        if family.designator() != instance.designator.to_ascii_lowercase() {
            return Err(SpiceError::parse(
                instance.location.clone(),
                format!(
                    "wrong model family {:?} for {}; model '{name}' declared at {}",
                    family, instance.name, card.location
                ),
            ));
        }
        Ok(Some(ResolvedModel {
            card,
            family,
            levels: levels(card, family)?,
        }))
    }
}

/// A resolved borrowed declaration. Parameter validation is an explicit,
/// device-owned schema operation, separate from lookup and selector choice.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedModel<'a> {
    card: &'a ModelCard,
    family: ModelFamily,
    levels: LevelSelection,
}

impl<'a> ResolvedModel<'a> {
    /// Original first model card, with raw ordered setters and source locations.
    #[must_use]
    pub const fn card(&self) -> &'a ModelCard {
        self.card
    }

    /// Validated family.
    #[must_use]
    pub const fn family(&self) -> ModelFamily {
        self.family
    }

    /// Backend selection versus applied integer data.
    #[must_use]
    pub const fn levels(&self) -> LevelSelection {
        self.levels
    }

    /// Apply a device-owned scalar schema, excluding `level` already consumed
    /// by selector/setter validation. Never ignores unknown physics keywords.
    ///
    /// # Errors
    /// The supplied schema's validation failures, with source provenance.
    pub fn parameters(&self, schema: &ScalarSchema<'_>) -> SpiceResult<ScalarValues> {
        schema.validate(
            self.card
                .parameters
                .iter()
                .filter(|p| !p.name.eq_ignore_ascii_case("level")),
            &self.card.location,
        )
    }

    /// Validate the initial diode core: IS/N/RS/TNOM only. Defaults follow
    /// `diosetup.c`; temperature conversion follows `diompar.c::DIOmParam`.
    /// IS epsmin clamping and compatibility-mode RS substitutions are not
    /// applied by this input schema; numerical device setup remains separate.
    ///
    /// # Errors
    /// Wrong family, unknown setters, invalid ranges or context temperatures.
    pub fn diode_parameters(&self, context: &ModelContext) -> SpiceResult<DiodeModelParameters> {
        self.require_diode()?;
        context.validate(&self.card.location)?;
        let values = self.parameters(&DIODE_MODEL_SCHEMA)?;
        let nominal = values
            .get("tnom")
            .map_or(context.nominal_temperature, |v| v.value);
        Ok(DiodeModelParameters {
            saturation_current: required(&values, "is", &self.card.location)?,
            emission_coefficient: required(&values, "n", &self.card.location)?,
            series_resistance: required(&values, "rs", &self.card.location)?,
            nominal_temperature_kelvin: temperature_kelvin(
                nominal,
                values
                    .get("tnom")
                    .and_then(|v| v.location.as_ref())
                    .unwrap_or(&self.card.location),
            )?,
        })
    }

    /// Validate bounded diode AREA/TEMP setters. `diotemp.c::DIOtemp` uses
    /// circuit temperature when TEMP is omitted; TNOM does not set TEMP.
    /// Dtemp/multiplicity/geometry/IC/flags remain explicit gaps, not ignored.
    ///
    /// # Errors
    /// Mismatched instance/model, unknown setters, invalid ranges/context.
    pub fn diode_instance_parameters(
        &self,
        instance: &DeviceInstance,
        context: &ModelContext,
    ) -> SpiceResult<DiodeInstanceParameters> {
        self.require_diode()?;
        if !instance.designator.eq_ignore_ascii_case(&'d')
            || !instance
                .model
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&self.card.name))
        {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "instance does not reference this diode model",
            ));
        }
        context.validate(&instance.location)?;
        let values = DIODE_INSTANCE_SCHEMA.validate(&instance.parameters, &instance.location)?;
        let temperature = values.get("temp").map_or(context.temperature, |v| v.value);
        Ok(DiodeInstanceParameters {
            area: required(&values, "area", &instance.location)?,
            temperature_kelvin: temperature_kelvin(
                temperature,
                values
                    .get("temp")
                    .and_then(|v| v.location.as_ref())
                    .unwrap_or(&instance.location),
            )?,
        })
    }

    fn require_diode(&self) -> SpiceResult<()> {
        if self.family == ModelFamily::Diode {
            Ok(())
        } else {
            Err(SpiceError::parse(
                self.card.location.clone(),
                "diode schema requires a diode model",
            ))
        }
    }
}

/// Device-owned context, intentionally independent of spice-analysis. Analysis
/// consumers can pass their temperature settings explicitly when factories land.
///
/// The context is an immutable per-point value: a typed DC sweep carries its
/// resistor overrides here (see [`crate::sweep`]) instead of mutating devices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelContext {
    /// Circuit temperature, degrees Celsius (instance TEMP default).
    pub temperature: Real,
    /// Nominal temperature, degrees Celsius (model TNOM default).
    pub nominal_temperature: Real,
    /// Per-point replacements of resistors' supplied scalars, applied by
    /// [`crate::Circuit`] when it assembles or loads. Empty slots are `None`.
    pub resistor_overrides: [Option<ResistorOverride>; MAX_RESISTOR_OVERRIDES],
    /// Junction minimum conductance in siemens (C `CKTgmin`, `.option gmin`),
    /// added in parallel with every diode, BJT and MOS1 junction. It is not the
    /// artificial nodal continuation conductance of DC gmin stepping. Default
    /// [`DEFAULT_GMIN`]; must be finite and nonnegative.
    pub gmin: Real,
    /// The analysis frequency in Hz that a behavioural source's `hertz` reads
    /// (C `CKTomega / 2 pi`): the AC frequency while an AC analysis re-solves
    /// the operating point for a frequency-dependent circuit, 0 otherwise.
    pub frequency: Real,
}

/// ngspice's default junction `gmin` (`cktntask.c`: `TSKgmin = 1e-12`).
pub const DEFAULT_GMIN: Real = 1e-12;

impl Default for ModelContext {
    fn default() -> Self {
        Self::new(27.0, 27.0)
    }
}
impl ModelContext {
    /// A context at explicit temperatures with no resistor overrides.
    #[must_use]
    pub const fn new(temperature: Real, nominal_temperature: Real) -> Self {
        Self {
            temperature,
            nominal_temperature,
            resistor_overrides: [None; MAX_RESISTOR_OVERRIDES],
            gmin: DEFAULT_GMIN,
            frequency: 0.,
        }
    }

    /// This context with the `hertz` frequency replaced (validated on use).
    #[must_use]
    pub const fn with_frequency(mut self, frequency: Real) -> Self {
        self.frequency = frequency;
        self
    }

    /// This context with junction `gmin` replaced (validated on use, see
    /// [`Self::gmin`]).
    #[must_use]
    pub const fn with_gmin(mut self, gmin: Real) -> Self {
        self.gmin = gmin;
        self
    }

    /// This context plus one resistor override, in the first free slot.
    ///
    /// # Errors
    /// [`SpiceError::Circuit`] when the same resistor is already overridden or
    /// every slot is used. The context itself is not changed on failure.
    pub fn with_resistor_override(mut self, target: ResistorOverride) -> SpiceResult<Self> {
        if self
            .resistor_overrides
            .iter()
            .flatten()
            .any(|existing| existing.device() == target.device())
        {
            return Err(SpiceError::circuit("duplicate resistor override"));
        }
        let slot = self
            .resistor_overrides
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or_else(|| SpiceError::circuit("too many resistor overrides"))?;
        *slot = Some(target);
        Ok(self)
    }

    pub(crate) fn validate(&self, location: &SourceLoc) -> SpiceResult<()> {
        temperature_kelvin(self.temperature, location)?;
        temperature_kelvin(self.nominal_temperature, location)?;
        if !(self.frequency.is_finite() && self.frequency >= 0.) {
            return Err(SpiceError::parse(
                location.clone(),
                format!(
                    "analysis frequency must be finite and nonnegative, got {}",
                    self.frequency
                ),
            ));
        }
        if !(self.gmin.is_finite() && self.gmin >= 0.) {
            return Err(SpiceError::parse(
                location.clone(),
                format!(
                    "junction gmin must be finite and nonnegative, got {}",
                    self.gmin
                ),
            ));
        }
        Ok(())
    }
}

/// Validated diode model inputs, not diode simulation equations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodeModelParameters {
    /// IS in amperes, strictly positive (default 1e-14 A).
    pub saturation_current: Real,
    /// N, strictly positive (default 1).
    pub emission_coefficient: Real,
    /// RS in ohms, nonnegative (default 0).
    pub series_resistance: Real,
    /// TNOM in Kelvin, strictly positive (context default 300.15 K).
    pub nominal_temperature_kelvin: Real,
}
/// Validated diode instance inputs, not topology/stamping or initialization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodeInstanceParameters {
    /// Area scale, strictly positive (default 1).
    pub area: Real,
    /// TEMP in Kelvin, strictly positive (context default 300.15 K).
    pub temperature_kelvin: Real,
}

const DIODE_MODEL_SCHEMA: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        ScalarParameter {
            name: "is",
            unit: ScalarUnit::Ampere,
            domain: ScalarDomain::Positive,
            default: Some(1e-14),
        },
        ScalarParameter {
            name: "n",
            unit: ScalarUnit::Dimensionless,
            domain: ScalarDomain::Positive,
            default: Some(1.0),
        },
        ScalarParameter {
            name: "rs",
            unit: ScalarUnit::Ohm,
            domain: ScalarDomain::NonNegative,
            default: Some(0.0),
        },
        ScalarParameter {
            name: "tnom",
            unit: ScalarUnit::Celsius,
            domain: ScalarDomain::Temperature,
            default: None,
        },
    ],
};
const DIODE_INSTANCE_SCHEMA: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        ScalarParameter {
            name: "area",
            unit: ScalarUnit::Dimensionless,
            domain: ScalarDomain::Positive,
            default: Some(1.0),
        },
        ScalarParameter {
            name: "temp",
            unit: ScalarUnit::Celsius,
            domain: ScalarDomain::Temperature,
            default: None,
        },
    ],
};

fn required(values: &ScalarValues, name: &str, location: &SourceLoc) -> SpiceResult<Real> {
    values.get(name).map(|v| v.value).ok_or_else(|| {
        SpiceError::parse(
            location.clone(),
            format!("missing schema default for {name}"),
        )
    })
}
fn family(card: &ModelCard) -> SpiceResult<ModelFamily> {
    ModelFamily::from_base(&card.base).ok_or_else(|| SpiceError::Unsupported {
        feature: format!("model family '{}'", card.base),
        location: Some(card.location.clone()),
    })
}

fn rounded_level(raw: Real, location: &SourceLoc) -> SpiceResult<u8> {
    let rounded = (raw + 0.5).floor();
    // Stricter than INPfindLev's warning/fallback and unsafe C casts. Reject
    // negative raw/nonfinite/out-of-range values, including invalid duplicates.
    if !raw.is_finite() || raw < 0.0 || !(0.0..=99.0).contains(&rounded) {
        return Err(SpiceError::parse(
            location.clone(),
            "model level must be finite, nonnegative and round into 0..=99",
        ));
    }
    Ok(rounded as u8)
}

fn levels(card: &ModelCard, family: ModelFamily) -> SpiceResult<LevelSelection> {
    let mut first = None;
    let mut last = None;
    for p in &card.parameters {
        if p.name.eq_ignore_ascii_case("level") {
            let raw = finite_literal(p)?;
            let integer = rounded_level(raw, &p.location)?;
            first.get_or_insert((raw, integer, &p.location));
            last = Some(integer);
        }
    }
    let raw = first.map(|(raw, _, _)| raw);
    if card.level != raw {
        return Err(SpiceError::parse(
            card.location.clone(),
            "raw level cache must equal the first ordered level setter",
        ));
    }
    let first_integer = first.map_or(1, |(_, integer, _)| integer);
    let location = first.map_or(&card.location, |(_, _, location)| location);
    let selector = match family {
        ModelFamily::Npn | ModelFamily::Pnp | ModelFamily::Nmos | ModelFamily::Pmos => {
            first_integer
        }
        ModelFamily::Resistor if card.base.eq_ignore_ascii_case("r") => first_integer,
        _ => 1,
    };
    let applied = (family == ModelFamily::Diode).then_some(last.unwrap_or(1));
    let supported = match family {
        ModelFamily::Npn | ModelFamily::Pnp => selector <= 2,
        ModelFamily::Diode => applied == Some(1),
        ModelFamily::Resistor if card.base.eq_ignore_ascii_case("r") => selector <= 1,
        _ => selector == 1 && first_integer == 1,
    };
    if !supported {
        let setter_location = if family == ModelFamily::Diode {
            card.parameters
                .iter()
                .rev()
                .find(|p| p.name.eq_ignore_ascii_case("level"))
                .map_or(location, |p| &p.location)
        } else {
            location
        };
        return Err(SpiceError::not_yet_ported(
            format!(
                "{setter_location}: {:?} model '{}' selector {selector}, applied level {applied:?}",
                family, card.name
            ),
            "src/spicelib/parser/inpdomod.c, inpfindl.c, inpgval.c; src/spicelib/devices/dio/diosetup.c",
        ));
    }
    Ok(LevelSelection {
        first_raw: raw,
        selector,
        applied,
    })
}
