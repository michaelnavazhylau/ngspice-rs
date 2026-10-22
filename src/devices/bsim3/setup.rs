//! Instance setup on C's structs: `b3set.c`'s instance pass
//! ([`super::set::setup_instance`]), then `b3temp.c`'s size-dependent and
//! temperature-dependent instance values with `b3check.c`
//! ([`super::temp::temperature_instance`]). This is geometry and parameter
//! setup only: no nodes are interned, no states are allocated and no load
//! equations run.
use super::defaults::ModelDefaults;
use super::fields::{Instance, Model, SizeParam};
use super::host::node;
use super::temp::{self, ModelTemp, SizeCache};
use super::temperature::TemperatureModel;
use super::{Diagnostic, Inputs, apply, descriptor, parameters};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

/// Effective geometry, size-dependent parameters and instance-level setup
/// results, read from C's instance and size-dependent structs.
#[derive(Clone)]
pub struct InstanceSetup {
    pub(super) model: Model,
    pub(super) instance: Instance,
    pub(super) size: SizeParam,
    /// Scaled deck L (C `here->BSIM3l`), after the 5 µm default.
    pub length: Real,
    /// Scaled deck W (C `here->BSIM3w`), after the 5 µm default.
    pub width: Real,
    /// Effective length (C `pParam->BSIM3leff`).
    pub leff: Real,
    /// Effective width (C `pParam->BSIM3weff`).
    pub weff: Real,
    /// Effective length for C-V.
    pub leff_cv: Real,
    /// Effective width for C-V.
    pub weff_cv: Real,
    /// Model polarity (+1 NMOS, -1 PMOS).
    pub polarity: i8,
    /// Instance multiplicity (C `here->BSIM3m`).
    pub multiplier: Real,
    /// NQS selector after instance/model resolution.
    pub nqs_mod: i32,
    /// AC NQS selector after instance/model resolution.
    pub acnqs_mod: i32,
    /// Geometry selector (C `here->BSIM3geo`).
    pub geo: i32,
    /// Drain junction area (m²).
    pub drain_area: Real,
    /// Source junction area (m²).
    pub source_area: Real,
    /// Drain junction perimeter (m).
    pub drain_perimeter: Real,
    /// Source junction perimeter (m).
    pub source_perimeter: Real,
    /// Drain squares (ACM off: C defaults to 1).
    pub drain_squares: Real,
    /// Source squares (ACM off: C defaults to 1).
    pub source_squares: Real,
    /// Instance threshold shift (V).
    pub delvto: Real,
    /// Instance mobility multiplier.
    pub mulu0: Real,
    /// Initial VBS.
    pub ic_vbs: Real,
    /// Initial VDS.
    pub ic_vds: Real,
    /// Initial VGS.
    pub ic_vgs: Real,
    /// Instance threshold after `delvto` (C `here->BSIM3vth0`).
    pub vth0: Real,
    /// Instance flat-band voltage after `delvto`.
    pub vfb: Real,
    /// Instance zero-bias flat-band voltage after `delvto`.
    pub vfbzb: Real,
    /// Temperature-adjusted mobility times `mulu0`.
    pub u0temp: Real,
    /// C's recomputed `tconst`, which reads the `T0` the size-dependent block
    /// left (`sqrt(1 + nlx/leff)`), not `leffCV²`.
    pub tconst: Real,
    /// Gate-source overlap capacitance (F).
    pub cgso: Real,
    /// Gate-drain overlap capacitance (F).
    pub cgdo: Real,
    /// Drain series conductance (S).
    pub drain_conductance: Real,
    /// Source series conductance (S).
    pub source_conductance: Real,
    /// Source junction saturation current (A), `b3temp.c`'s local.
    pub source_sat_current: Real,
    /// Drain junction saturation current (A), `b3temp.c`'s local.
    pub drain_sat_current: Real,
    /// Source junction breakdown voltage (0 when C leaves it unset).
    pub vjsm: Real,
    /// Drain junction breakdown voltage (0 when C leaves it unset).
    pub vjdm: Real,
    /// Source junction breakdown current (0 when C leaves it unset).
    pub isevjsm: Real,
    /// Drain junction breakdown current (0 when C leaves it unset).
    pub isevjdm: Real,
    /// Nonfatal `b3set.c`/`b3temp.c`/`b3check.c` messages in C order.
    pub warnings: Vec<Diagnostic>,
}

impl std::fmt::Debug for InstanceSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceSetup")
            .field("length", &self.length)
            .field("width", &self.width)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

/// C's model struct as one `BSIM3temp` pass works on it, shared by every
/// instance of the model: `b3check.c`'s model resets (CALCACM,
/// CGDO/CGSO/CGBO) and the size-dependent knots persist across instances,
/// as they do in C.
#[derive(Clone)]
pub struct ModelSetup {
    defaults: ModelDefaults,
    temperature: TemperatureModel,
    model: Model,
    locals: ModelTemp,
    cache: SizeCache,
}

impl std::fmt::Debug for ModelSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelSetup").finish_non_exhaustive()
    }
}

impl ModelSetup {
    /// Capture the defaulted model and its temperature pass.
    ///
    /// # Errors
    /// None today; the signature keeps the stage uniform.
    #[allow(clippy::unnecessary_wraps)] // Uniform stage signature.
    pub fn new(defaults: ModelDefaults, temperature: TemperatureModel) -> SpiceResult<Self> {
        Ok(Self {
            model: temperature.model.clone(),
            locals: temperature.locals,
            cache: SizeCache::new(),
            defaults,
            temperature,
        })
    }

    /// The model fields this state was built from.
    #[must_use]
    pub const fn defaults(&self) -> &ModelDefaults {
        &self.defaults
    }

    /// The model-level temperature pass.
    #[must_use]
    pub const fn temperature(&self) -> &TemperatureModel {
        &self.temperature
    }

    /// Set up one instance against the shared model state, applying any
    /// `b3check.c` model resets in C order.
    ///
    /// # Errors
    /// See [`InstanceSetup::new`].
    pub fn setup_instance(
        &mut self,
        instance: &Inputs,
        location: &SourceLoc,
    ) -> SpiceResult<InstanceSetup> {
        InstanceSetup::with_model(self, instance, location)
    }
}

/// Every `b3temp.c` size-dependent parameter keyword, in C order.
#[must_use]
pub fn size_dependent_names() -> Vec<&'static str> {
    super::size_data::SIZE_DEPENDENT
        .iter()
        .map(|row| row.0)
        .collect()
}

/// The `SizeParam` member `name` names: a model keyword's own member (for
/// example `nch` is `BSIM3npeak`), else a member spelled `name` ignoring case
/// (`leffcv` is `leffCV`).
fn size_member(size: &SizeParam, name: &str) -> Option<Real> {
    let keyword = descriptor(parameters::MODEL, name)
        .and_then(|p| p.c_field)
        .and_then(|field| field.strip_prefix("BSIM3"))
        .and_then(|member| {
            SizeParam::MEMBERS
                .iter()
                .find(|c| c.strip_prefix("BSIM3") == Some(member))
        });
    let member = keyword.or_else(|| {
        SizeParam::MEMBERS.iter().find(|c| {
            c.strip_prefix("BSIM3")
                .unwrap_or(c)
                .eq_ignore_ascii_case(name)
        })
    })?;
    let mut copy = size.clone();
    copy.real_mut(member).map(|value| *value)
}

impl InstanceSetup {
    /// A size-dependent/derived parameter by keyword, e.g. `phi` or `rdsw`.
    #[must_use]
    pub fn pparam(&self, name: &str) -> Option<Real> {
        size_member(&self.size, name)
    }

    /// Apply `b3set.c` instance defaults, `b3temp.c` size/temperature
    /// processing and `b3check.c` parameter checks for one instance, against
    /// a model of its own.
    ///
    /// # Errors
    /// ACM geometry (unported `ACM_SourceDrainResistances`), fatal
    /// `b3check.c` diagnostics and effective geometry <= 0.
    pub fn new(
        model: &ModelDefaults,
        temperature: &TemperatureModel,
        instance: &Inputs,
        location: &SourceLoc,
    ) -> SpiceResult<Self> {
        let mut shared = ModelSetup::new(model.clone(), temperature.clone())?;
        Self::with_model(&mut shared, instance, location)
    }

    fn with_model(
        shared: &mut ModelSetup,
        instance: &Inputs,
        location: &SourceLoc,
    ) -> SpiceResult<Self> {
        let mut messages = Vec::new();
        let mut here = apply::instance(instance)?;
        here.dNode = node::D;
        here.gNode = node::G;
        here.sNode = node::S;
        here.bNode = node::B;
        super::set::setup_instance(&shared.model, &mut here, &mut messages).map_err(
            |e| match e {
                SpiceError::NotYetPorted { what, c_reference } => SpiceError::NotYetPorted {
                    what: format!("{location}: {what}"),
                    c_reference,
                },
                other => other,
            },
        )?;
        let size = temp::temperature_instance(
            &mut shared.model,
            &mut here,
            &shared.locals,
            &mut shared.cache,
            false,
            "",
            "",
            &mut messages,
        )
        .map_err(|e| match e {
            SpiceError::Circuit { message } => SpiceError::parse(location.clone(), message),
            other => other,
        })?;
        let model = &shared.model;
        // `b3temp.c`'s local junction saturation currents, as C computes them.
        let sat = |area: Real, perimeter: Real| {
            if (area <= 0.0) && (perimeter <= 0.0) {
                1.0e-14
            } else {
                area * model.jctTempSatCurDensity + perimeter * model.jctSidewallTempSatCurDensity
            }
        };
        Ok(Self {
            length: here.l,
            width: here.w,
            leff: size.leff,
            weff: size.weff,
            leff_cv: size.leffCV,
            weff_cv: size.weffCV,
            polarity: if model.r#type > 0 { 1 } else { -1 },
            multiplier: here.m,
            nqs_mod: here.nqsMod,
            acnqs_mod: here.acnqsMod,
            geo: here.geo,
            drain_area: here.drainArea,
            source_area: here.sourceArea,
            drain_perimeter: here.drainPerimeter,
            source_perimeter: here.sourcePerimeter,
            drain_squares: here.drainSquares,
            source_squares: here.sourceSquares,
            delvto: here.delvto,
            mulu0: here.mulu0,
            ic_vbs: here.icVBS,
            ic_vds: here.icVDS,
            ic_vgs: here.icVGS,
            vth0: here.vth0,
            vfb: here.vfb,
            vfbzb: here.vfbzb,
            u0temp: here.u0temp,
            tconst: here.tconst,
            cgso: here.cgso,
            cgdo: here.cgdo,
            drain_conductance: here.drainConductance,
            source_conductance: here.sourceConductance,
            source_sat_current: sat(here.sourceArea, here.sourcePerimeter),
            drain_sat_current: sat(here.drainArea, here.drainPerimeter),
            vjsm: here.vjsm,
            vjdm: here.vjdm,
            isevjsm: here.IsEvjsm,
            isevjdm: here.IsEvjdm,
            warnings: messages
                .into_iter()
                .map(|message| {
                    let reference = "src/spicelib/devices/bsim3/b3check.c";
                    if message.starts_with("Fatal:") {
                        Diagnostic::fatal(message, reference)
                    } else {
                        Diagnostic::warning(message, reference)
                    }
                })
                .collect(),
            model: model.clone(),
            instance: here,
            size,
        })
    }
}
