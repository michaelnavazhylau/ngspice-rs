//! Independent sources, with current positive from the first terminal to the second.
use crate::{Device, LinearContext, LinearSource, StampContext, Waveform};
use spice_core::{Complex, NodeId, Real, SpiceError, SpiceResult};

/// Independent V or I source. AC, DC and transient excitations are distinct.
#[derive(Debug, Clone)]
pub struct IndependentSource {
    name: String,
    terminals: [NodeId; 2],
    voltage: bool,
    dc: Real,
    ac: Complex,
    waveform: Waveform,
}
impl IndependentSource {
    /// Creates an independent source. `voltage=false` selects a current source.
    /// # Errors
    /// Non-finite excitation or invalid waveform.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        voltage: bool,
        dc: Real,
        ac: Complex,
        waveform: Waveform,
    ) -> SpiceResult<Self> {
        waveform.validate()?;
        if !dc.is_finite() || !ac.is_finite() {
            return Err(SpiceError::circuit("non-finite source excitation"));
        }
        Ok(Self {
            name: name.into(),
            terminals,
            voltage,
            dc,
            ac,
            waveform,
        })
    }
}
impl Device for IndependentSource {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        if self.voltage { 'v' } else { 'i' }
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn branch_currents(&self) -> usize {
        usize::from(self.voltage)
    }
    fn stamp(&mut self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if !context.mode.is_dc() {
            return Err(SpiceError::circuit(
                "use linear equation assembly for dynamic sources",
            ));
        }
        if self.voltage {
            let branch = context
                .branch
                .ok_or_else(|| SpiceError::circuit("missing source branch row"))?;
            crate::linear::branch_stamp(context.matrix, context.unknowns, self.terminals, branch)?;
            context.rhs.add_to(branch, self.dc)
        } else {
            context.stamp_rhs(self.terminals[0], -self.dc)?;
            context.stamp_rhs(self.terminals[1], self.dc)
        }
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        let rows = if self.voltage {
            vec![(context.branch(self.terminals)?, 1.)]
        } else {
            [(self.terminals[0], -1.), (self.terminals[1], 1.)]
                .into_iter()
                .filter_map(|(node, sign)| context.unknowns.node_row(node).map(|r| (r, sign)))
                .collect()
        };
        context.system.sources.push(LinearSource {
            name: self.name.clone(),
            rows,
            dc: self.dc,
            ac: self.ac,
            waveform: self.waveform.clone(),
        });
        Ok(())
    }
}
