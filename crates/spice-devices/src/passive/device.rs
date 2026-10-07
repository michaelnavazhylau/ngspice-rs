//! Contextual input wrapper delegates effective values to existing R/C/L stamps.
use super::PassiveParameters;
use crate::models::{ModelContext, ModelFamily, ResolvedModel};
use crate::{Capacitor, Device, Inductor, LinearContext, Resistor, StampContext};
use spice_core::{NodeId, NodeTable, SpiceResult};
use spice_netlist::ast::DeviceInstance;

#[derive(Debug)]
struct ModelPassive {
    name: String,
    terminals: [NodeId; 2],
    parameters: PassiveParameters,
}
impl ModelPassive {
    fn scalar(&self, context: &ModelContext) -> SpiceResult<Box<dyn Device>> {
        let value = self.parameters.effective_value(context)?;
        let ic = self.parameters.initial_condition();
        Ok(match self.parameters.family() {
            ModelFamily::Resistor => Box::new(Resistor::new(&self.name, self.terminals, value)?),
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
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        self.scalar(&ModelContext {
            temperature: context.temperature,
            nominal_temperature: context.nominal_temperature,
        })?
        .stamp(context)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        self.scalar(context.model_context)?.assemble_linear(context)
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
        terminals,
        parameters,
    };
    device.scalar(context)?;
    *nodes = staged;
    Ok(Box::new(device))
}
