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

#[derive(Debug)]
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
