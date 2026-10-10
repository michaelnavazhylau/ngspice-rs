//! Contextual input wrapper delegates effective values to existing R/C/L stamps.
use super::PassiveParameters;
use crate::devices::models::{ModelContext, ModelFamily, ResolvedModel};
use crate::devices::noise::{DeviceNoise, NoiseContext};
use crate::devices::{
    Capacitor, Device, InductanceValue, Inductor, LinearContext, Resistor, ResistorMetadata,
    ResistorOrigin, StampContext, StorageElement, StorageKind,
};
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeTable, Real, SpiceError, SpiceResult};

#[derive(Debug, Clone)]
struct ModelPassive {
    name: String,
    /// The model card's name, for C's `.noise` visiting order.
    model: String,
    terminals: [NodeId; 2],
    parameters: PassiveParameters,
}
/// The charge/flux slot of the delegated scalar [`Capacitor`]/[`Inductor`].
const STORAGE_QUANTITY: usize = 0;

impl ModelPassive {
    fn storage_kind(&self) -> Option<StorageKind> {
        match self.parameters.family() {
            ModelFamily::Capacitor => Some(StorageKind::Capacitor),
            ModelFamily::Inductor => Some(StorageKind::Inductor),
            _ => None,
        }
    }
    fn scalar(&self, context: &ModelContext) -> SpiceResult<Box<dyn Device>> {
        let value = self.parameters.effective_value(context)?;
        let ic = self.parameters.initial_condition();
        Ok(match self.parameters.family() {
            ModelFamily::Resistor => Box::new(
                Resistor::new(&self.name, self.terminals, value)?
                    .with_noise(self.parameters.resistor_noise(&self.model)),
            ),
            ModelFamily::Capacitor => {
                Box::new(Capacitor::new(&self.name, self.terminals, value, ic)?)
            }
            _ => Box::new(Inductor::new(&self.name, self.terminals, value, ic)?),
        })
    }
}
impl Device for ModelPassive {
    fn observation_parameter(
        &self,
        keyword: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        let p = &self.parameters;
        Ok(match keyword {
            "r" | "resistance" if p.family == ModelFamily::Resistor => {
                Some(p.scaled_value(context)?)
            }
            "g" | "conductance" if p.family == ModelFamily::Resistor => {
                Some(1. / p.effective_value(context)?)
            }
            "c" | "cap" | "capacitance" if p.family == ModelFamily::Capacitor => {
                Some(p.effective_value(context)?)
            }
            "l" | "inductance" if p.family == ModelFamily::Inductor => {
                Some(p.coupling_value(context)?)
            }
            "m" => Some(p.multiplicity),
            "scale" => Some(p.scale),
            "temp" => Some(p.temperature.unwrap_or(context.temperature)),
            "tc1" => Some(p.tc1),
            "tc2" => Some(p.tc2),
            "ic" => Some(p.initial_condition.unwrap_or(0.)),
            _ => p.instance_values.get(keyword).map(|v| v.value),
        })
    }
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        let primary = match self.parameters.family() {
            ModelFamily::Resistor => "resistance",
            ModelFamily::Capacitor => "capacitance",
            _ => "inductance",
        };
        let keyword = match keyword.to_ascii_lowercase().as_str() {
            "r" => "resistance".to_owned(),
            "c" | "cap" => "capacitance".to_owned(),
            other => other.to_owned(),
        };
        super::instance_schema(self.parameters.family(), primary)
            .iter()
            .find(|d| d.name == keyword)
            .map(|d| d.name)
    }

    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        Ok(Box::new(Self {
            parameters: self.parameters.swept(parameter, value, context)?,
            ..self.clone()
        }))
    }

    /// Linear in `.disto`: C gives this device no distortion routine
    /// (`DEVdisto = NULL`, `res`/`cap`/`ind` `*init.c`), so it enters only through its
    /// small-signal matrix.
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> crate::primitives::SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(crate::devices::distortion::DeviceDistortion::Linear)
    }

    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        self.parameters.family().designator()
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn branch_currents(&self) -> usize {
        usize::from(self.parameters.family() == ModelFamily::Inductor)
    }
    fn state_count(&self) -> usize {
        match self.parameters.family() {
            ModelFamily::Capacitor | ModelFamily::Inductor => 2,
            _ => 0,
        }
    }
    /// The scalar capacitor/inductor's charge/flux slot (`captrunc.c`,
    /// `indtrunc.c`): the state layout is the delegated scalar device's.
    fn truncation_slot(&self) -> Option<usize> {
        self.storage_kind().map(|_| STORAGE_QUANTITY)
    }
    /// The effective (temperature/TC/scale/`m`-adjusted) value and the
    /// instance `ic=`, so `uic` seeds model-backed C/L exactly as literal ones.
    fn storage_element(&self, context: &ModelContext) -> Option<SpiceResult<StorageElement>> {
        let kind = self.storage_kind()?;
        Some(
            self.parameters
                .effective_value(context)
                .map(|value| StorageElement {
                    kind,
                    value,
                    initial: self.parameters.initial_condition(),
                }),
        )
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        self.scalar(&ModelContext::new(
            context.temperature,
            context.nominal_temperature,
        ))?
        .stamp(context)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        self.scalar(context.model_context)?.assemble_linear(context)
    }
    /// The delegated scalar's noise: `resnoise.c` for a resistor (with the
    /// model's KF/AF/EF, the instance noise area, `m`, `temp=` and `noisy`),
    /// noiseless for C and L (`DEVnoise = NULL`).
    fn noise(&self, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        self.scalar(context.model_context)?.noise(context)
    }
    fn inductance(&self, context: &ModelContext) -> Option<SpiceResult<InductanceValue>> {
        (self.parameters.family() == ModelFamily::Inductor).then(|| {
            Ok(InductanceValue {
                effective: self.parameters.effective_value(context)?,
                coupling_base: self.parameters.coupling_value(context)?,
            })
        })
    }
    fn resistor_metadata(&self) -> Option<ResistorMetadata> {
        (self.parameters.family() == ModelFamily::Resistor).then(|| ResistorMetadata {
            origin: ResistorOrigin::ModelBacked,
            supplied: self.parameters.nominal_value(),
            multiplicity: self.parameters.multiplicity(),
        })
    }
    fn resistor_effective(&self, supplied: Real, context: &ModelContext) -> SpiceResult<Real> {
        if self.parameters.family() != ModelFamily::Resistor {
            return Err(SpiceError::circuit(format!(
                "{} is not a resistor",
                self.name
            )));
        }
        self.parameters
            .with_nominal_value(supplied)?
            .effective_value(context)
    }

    /// `.sens`: C's resistor records from the card's model and instance
    /// setters ([`crate::devices::sensitivity`]).
    fn sensitivity(
        &self,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn crate::devices::sensitivity::DeviceSensitivity + '_>> {
        use crate::devices::sensitivity::{ResistorInputs, ResistorSensitivity, written};
        let (m, i) = (
            self.parameters.model_values(),
            self.parameters.instance_values(),
        );
        match self.parameters.family() {
            ModelFamily::Resistor => Ok(Box::new(ResistorSensitivity::new(
                &self.name,
                Some(&self.model),
                self.terminals,
                ResistorInputs {
                    resistance: written(i, "resistance"),
                    temp: written(i, "temp"),
                    length: written(i, "l"),
                    width: written(i, "w"),
                    m: written(i, "m"),
                    scale: written(i, "scale"),
                    tc1: written(i, "tc1"),
                    tc2: written(i, "tc2"),
                    model_r: written(m, "r"),
                    rsh: written(m, "rsh"),
                    defw: written(m, "defw"),
                    defl: written(m, "l"),
                    narrow: written(m, "narrow"),
                    short: written(m, "short"),
                    model_tc1: written(m, "tc1"),
                    model_tc2: written(m, "tc2"),
                    tnom: written(m, "tnom"),
                    kf: written(m, "kf"),
                    af: written(m, "af"),
                    lf: written(m, "lf"),
                    wf: written(m, "wf"),
                    ef: written(m, "ef"),
                },
                context,
            ))),
            family => {
                use crate::devices::sensitivity::{
                    ReactiveInputs, ReactiveKind, ReactiveSensitivity,
                };
                type Keys = &'static [&'static str];
                let (kind, value, instance_keys, model_keys): (_, _, Keys, Keys) = if family
                    == ModelFamily::Capacitor
                {
                    (
                        ReactiveKind::Capacitor,
                        "capacitance",
                        &["ic", "temp", "w", "l", "m", "tc1", "tc2", "scale"],
                        &[
                            "cap", "cj", "cjsw", "defw", "narrow", "short", "tc1", "tc2", "tnom",
                        ],
                    )
                } else {
                    (
                        ReactiveKind::Inductor,
                        "inductance",
                        &["ic", "temp", "m", "tc1", "tc2", "scale"],
                        &["ind", "tc1", "tc2", "tnom"],
                    )
                };
                let mut instance: Vec<(&'static str, Real)> = Vec::new();
                instance.extend(written(i, value).map(|v| (value, v)));
                for key in instance_keys {
                    instance.extend(written(i, key).map(|v| (*key, v)));
                }
                let mut model = Vec::new();
                for key in model_keys {
                    model.extend(written(m, key).map(|v| (*key, v)));
                }
                Ok(Box::new(ReactiveSensitivity::new(
                    kind,
                    &self.name,
                    Some(&self.model),
                    self.terminals,
                    ReactiveInputs { instance, model },
                    context,
                )))
            }
        }
    }

    /// Pole-zero load: C `respzld.c`, `cappzld.c`, `indpzld.c` equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
}

pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
    model: &ResolvedModel<'_>,
    context: &ModelContext,
) -> SpiceResult<Box<dyn Device>> {
    let parameters = model.passive_parameters(instance)?;
    // Evaluate before interning, then stage construction so both schema and
    // existing scalar-constructor failures preserve the caller's table.
    parameters.effective_value(context)?;
    let mut staged = nodes.clone();
    let terminals = [
        staged.intern(&instance.nodes[0]),
        staged.intern(&instance.nodes[1]),
    ];
    let device = ModelPassive {
        name: instance.name.clone(),
        model: model.card().name.clone(),
        terminals,
        parameters,
    };
    device.scalar(context)?;
    *nodes = staged;
    Ok(Box::new(device))
}

/// Per-point literal R/C/L replacement, retaining temperature and multiplier setters.
pub(crate) fn swept_literal(
    name: &str,
    terminals: [NodeId; 2],
    family: ModelFamily,
    value: Real,
    ic: Option<Real>,
    change: (&str, Real),
    context: &ModelContext,
) -> SpiceResult<Box<dyn Device>> {
    Ok(Box::new(ModelPassive {
        name: name.to_owned(),
        model: String::new(),
        terminals,
        parameters: PassiveParameters::literal(family, value, ic)?
            .swept(change.0, change.1, context)?,
    }))
}
