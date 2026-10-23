//! BSIM4.8 (`bsim4/`, levels 14 and 54): the generated input schema and a
//! statement-by-statement port of C's setup, temperature, check, load,
//! small-signal, pole-zero, noise and convergence routines.
//!
//! The C-shaped modules keep C's struct members (`fields.rs`, generated from
//! `bsim4def.h`) and see the circuit through `host.rs`; `device.rs` wires
//! them into the solver. Every path is compared bit for bit with an FMA-free
//! build of the reference ngspice (`tests/bsim4_device.rs`, the opt-in
//! `c_bsim4_*` tests and `docs/port/BSIM.md`).
//!
//! BSIM4 was developed by the UC Berkeley BSIM Research Group. The translated
//! files follow the upstream notice: Copyright (c) 2025 University of
//! California, licensed under the Educational Community License, Version 2.0
//! (see `NOTICE` and `LICENSE-ECL-2.0`).
use crate::devices::ifparam::Device;
use crate::devices::models::{ModelFamily, ResolvedModel};
use crate::devices::schema::temperature_kelvin;
use crate::netlist::ast::{DeviceInstance, ModelCard, ParameterAssignment};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

use crate::devices::ifparam::{descriptor, version_text};

pub use crate::devices::ifparam::{
    Access, AppliedParameter, Diagnostic, InputType, Inputs, Parameter, Value,
};

mod defaults_data;

// The C-shaped port. These modules follow their C files statement by
// statement over C's own struct members (`fields.rs`), so C's mixed-case
// member and local names are kept.
mod acld;
mod apply;
mod asks;
mod check;
mod cvtest;
mod device;
mod elements;
mod fields;
mod geo;
mod host;
mod load;
mod model_state;
pub(crate) use model_state::{Binding, bind_batch};
mod noise;
/// Generated IFparm order, identifiers and setter/given-field mappings.
pub mod parameters;
mod pzld;
mod sens;
mod set;
mod temp;

// The C host interface (`CKTcircuit` view, `CKTmode` bits), the noise
// generator shape and C's `%g` spelling are shared with the BSIM3 port, which follows the same shape.
pub(crate) use host::{LoadCkt, SmallSignalCkt, mode};
pub(crate) use noise::Source as NoiseSource;
pub(crate) use temp::c_g;

/// The generated literal model defaults in C setup order (`b4set.c`).
///
/// Generated from `b4set.c`'s given-guarded literal assignments, which
/// `set.rs` reproduces in C order (a unit test there checks they agree).
#[must_use]
pub fn literal_defaults() -> &'static [(&'static str, f64)] {
    defaults_data::LITERALS
}

/// The BSIM4 interface descriptor the shared input plumbing uses.
pub(crate) const DEVICE: Device = Device {
    reference: "bsim4/b4.c",
    model: parameters::MODEL,
    instance: parameters::INSTANCE,
    uninteresting: parameters::UNINTERESTING,
    ic_names: ["icvds", "icvgs", "icvbs"],
    model_real: bsim4_model_real,
};

/// `b4mpar.c`: TNOM adds CONSTCtoK; NDEP converts above 1e20 and NSD/NGATE
/// above 1e23 (and their L/W/P coefficients likewise) from cm^-3 to m^-3.
fn bsim4_model_real(p: &Parameter, value: Real, location: &SourceLoc) -> SpiceResult<Real> {
    if p.canonical == "tnom" {
        return temperature_kelvin(value, location);
    }
    let field = p.c_field.unwrap_or_default();
    let threshold = if field.ends_with("ndep") {
        1e20
    } else if field.ends_with("nsd") || field.ends_with("ngate") {
        1e23
    } else {
        Real::INFINITY
    };
    Ok(if value > threshold {
        value * 1e-6
    } else {
        value
    })
}

/// C's `IF_UNINTERESTING` rows, as `TABLE:keyword` pairs.
#[must_use]
pub fn uninteresting() -> &'static [&'static str] {
    DEVICE.uninteresting
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
    /// Apply reversed model-level instance defaults, then instance setters
    /// (`inpgmod.c`/`inpdpar.c`). C discards model-level `m`, not instance `m`.
    ///
    /// # Errors
    /// Unknown/ask-only setters, invalid shapes, nonfinite input, integer or
    /// scaled-geometry overflow. No physical setup check is performed: the
    /// `b4set.c`/`b4temp.c` stage is not ported.
    pub fn instance_parameters(
        &self,
        instance: &DeviceInstance,
        scale: Real,
    ) -> SpiceResult<Inputs> {
        if instance.designator != 'm' || !scale.is_finite() || scale <= 0. {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "BSIM4 needs an M instance and a finite positive scale",
            ));
        }
        // BSIM4 has three- and four-terminal forms; the bulk node is optional.
        if !matches!(instance.nodes.len(), 3 | 4) {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "BSIM4 needs three or four terminals: drain, gate, source[, bulk]",
            ));
        }
        let mut inputs = Inputs::new(&DEVICE, false);
        for p in self.instance_defaults.iter().chain(&instance.parameters) {
            inputs.apply(p, scale)?;
        }
        Ok(inputs)
    }
}

/// Validate explicit BSIM4 model setters, including their instance-default
/// fallback, in C order. This does **not** apply `b4set.c` defaults or claim
/// that `b4check.c`/`b4soachk.c` diagnostics have run.
///
/// # Errors
/// Wrong family/level/version, unknown/read-only setters or invalid typed input.
pub fn model_parameters(model: &ResolvedModel<'_>) -> SpiceResult<ModelParameters> {
    if !matches!(model.family(), ModelFamily::Nmos | ModelFamily::Pmos)
        || !matches!(model.levels().selector, 14 | 54)
    {
        return Err(SpiceError::parse(
            model.card().location.clone(),
            "BSIM4 input schema requires NMOS/PMOS level 14 or 54",
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

/// Select the BSIM4 directory from the **first** VERSION token, as
/// `inpdomod.c`/`INPfindVer` do. Only the current `bsim4/` family is being
/// ported; the `bsim4v5`/`bsim4v6`/`bsim4v7` variants are refused by name.
///
/// # Errors
/// Unsupported versions, or nonliteral version input.
pub fn select_version(card: &ModelCard) -> SpiceResult<&str> {
    let version = card
        .parameters
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("version"));
    let Some(p) = version else {
        return Ok("");
    };
    let text = version_text(p)?;
    let reference = if text.starts_with("4.5") {
        "src/spicelib/devices/bsim4v5/"
    } else if text.starts_with("4.6") {
        "src/spicelib/devices/bsim4v6/"
    } else if text.starts_with("4.7") {
        "src/spicelib/devices/bsim4v7/"
    } else if text.starts_with("4.8") || text.is_empty() {
        return Ok(text);
    } else {
        "src/spicelib/parser/inpdomod.c, inpfindv.c (no supported BSIM4 VERSION)"
    };
    Err(SpiceError::not_yet_ported(
        format!("{}: model '{}' BSIM4 version {text}", p.location, card.name),
        reference,
    ))
}

/// The factory entry: [`device::Bsim4`] runs `b4set.c`/`b4temp.c` before
/// interning any node, so every unported path fails before a partial
/// circuit exists.
pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut crate::primitives::NodeTable,
    model: &ResolvedModel<'_>,
    context: &crate::devices::ModelContext,
    binding: Binding,
) -> SpiceResult<Box<dyn crate::devices::Device>> {
    device::Bsim4::instantiate(instance, nodes, model, context, binding)
}
