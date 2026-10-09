//! Independent sources, with current positive from the first terminal to the second.
use crate::devices::{AnalysisMode, Device, LinearContext, LinearSource, StampContext, Waveform};
use crate::primitives::{Complex, NodeId, Real, SpiceError, SpiceResult};

/// Independent V or I source. AC, DC and transient excitations are distinct.
#[derive(Debug, Clone)]
pub struct IndependentSource {
    name: String,
    terminals: [NodeId; 2],
    voltage: bool,
    dc: Real,
    ac: Complex,
    /// Whether an AC value was written (C `VSRCacGiven`), even `ac 0`.
    ac_given: bool,
    waveform: Waveform,
}
impl IndependentSource {
    /// Creates an independent source. `voltage=false` selects a current source.
    /// The source counts as having an AC value ([`Self::ac_given`]) when `ac`
    /// is nonzero; [`Self::with_ac_given`] records an explicit `ac 0`.
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
            ac_given: ac != Complex::ZERO,
            waveform,
        })
    }

    /// Records whether the card wrote an AC value (`ac`, `acmag` or
    /// `acphase`; C `VSRCacGiven`/`ISRCacGiven`), which decides how
    /// pole-zero analysis treats a voltage source (`vsrcpzld.c`).
    #[must_use]
    pub fn with_ac_given(mut self, given: bool) -> Self {
        self.ac_given = given;
        self
    }

    /// Whether the card wrote an AC value (C `VSRCacGiven`).
    #[must_use]
    pub const fn ac_given(&self) -> bool {
        self.ac_given
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
    /// A voltage source's current can control F/H sources (`VSRCfindBr`).
    fn findable_branch(&self) -> Option<usize> {
        self.voltage.then_some(0)
    }
    /// DC loads stamp the DC value. Companion transient loads stamp the time-`t`
    /// forcing, evaluated with the one-sided limit of `context.forcing`
    /// (`vsrcload.c`/`isrcload.c` evaluate the waveform at `CKTtime`).
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let value = match context.mode {
            AnalysisMode::OperatingPoint | AnalysisMode::DcSweep => self.dc,
            AnalysisMode::Transient { time, .. } => {
                let forcing = context.forcing.ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: transient source load without a forcing context",
                        self.name
                    ))
                })?;
                self.waveform
                    .value_at_timed(time, forcing.limit, &forcing.timing)?
            }
            AnalysisMode::Ac { .. } => {
                return Err(SpiceError::circuit(
                    "use linear equation assembly for AC sources",
                ));
            }
        };
        if self.voltage {
            let branch = context.branch(0)?;
            crate::devices::linear::branch_stamp(
                context.matrix,
                context.unknowns,
                self.terminals,
                branch,
            )?;
            context.rhs.add_to(branch, value)
        } else {
            context.stamp_rhs(self.terminals[0], -value)?;
            context.stamp_rhs(self.terminals[1], value)
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
            kind: if self.voltage {
                crate::devices::linear::SourceKind::Voltage
            } else {
                crate::devices::linear::SourceKind::Current
            },
            rows,
            dc: self.dc,
            ac: self.ac,
            waveform: self.waveform.clone(),
        });
        Ok(())
    }
    /// `VSRCpzLoad` (`vsrcpzld.c`): a voltage source without an AC value
    /// shorts its terminals as in AC; one with an AC value is removed (the
    /// KCL couplings of its current stay, its branch row becomes
    /// `i = 0`), since the pole-zero drive replaces the input. A current
    /// source has no pole-zero load in C and contributes nothing (open), as
    /// in the AC matrix.
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        _bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        if !self.voltage {
            return Ok(());
        }
        if !self.ac_given {
            context.branch(self.terminals)?;
            return Ok(());
        }
        let branch = context
            .branch
            .ok_or_else(|| SpiceError::circuit("missing branch-row binding"))?;
        for (node, sign) in [(self.terminals[0], 1.), (self.terminals[1], -1.)] {
            if let Some(row) = context.unknowns.node_row(node) {
                context.system.a.add(row, branch, sign)?;
            }
        }
        context.system.a.add(branch, branch, 1.)
    }
}
