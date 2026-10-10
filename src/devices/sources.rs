//! Independent sources, with current positive from the first terminal to the second.
//!
//! A voltage source can also be an RFSPICE **port** (`portnum`/`z0`, C
//! `VSRCisPort`): `vsrcset.c` then creates an internal node `<name>#res`, puts
//! the ideal source between that node and the negative terminal, and
//! `vsrcload.c`/`vsrcacld.c` stamp the reference conductance `1/z0` between the
//! positive terminal and the internal node in **every** analysis. A port is
//! therefore a Thevenin source with series resistance `z0`, and its branch
//! current is the current through the ideal source from `<name>#res` to the
//! negative terminal. See `docs/port/SPARAM.md`.
use crate::devices::{AnalysisMode, Device, LinearContext, LinearSource, StampContext, Waveform};
use crate::primitives::{Complex, NodeId, Real, SpiceError, SpiceResult};

/// The RFSPICE port data of a voltage source (C `VSRCisPort` and its
/// `VSRCportNum`, `VSRCportZ0`, `VSRCportPower`, `VSRCportFreq`,
/// `VSRCportPhase` fields), with `vsrctemp.c`'s defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RfPort {
    /// The 1-based port index (`portnum`). Ports of a circuit must be numbered
    /// `1..=N` without gaps or duplicates (`vsrctemp.c`).
    pub number: usize,
    /// The positive, finite reference impedance `z0` in ohms (default 50).
    pub z0: Real,
    /// The internal node `<name>#res` between the series `z0` and the ideal
    /// source.
    pub internal: NodeId,
    /// `pwr` in watts (default 1 mW). Only the large-signal `PORT` time function
    /// uses it.
    pub power: Real,
    /// `freq` in hertz (default 1 GHz). Only the `PORT` time function uses it.
    pub frequency: Real,
    /// `phase` in degrees (default 0). Stored like C, which never uses it.
    pub phase: Real,
    /// True when `pwr`/`freq` selected C's large-signal `PORT` time function
    /// (`vsrcpar.c` sets `VSRCfunctionType = PORT`) and no later waveform setter
    /// replaced it. Its transient evaluation is not ported.
    pub power_function: bool,
}

impl RfPort {
    /// The incident/scattered power-wave scale `ki = 1/(2 sqrt(z0))` of
    /// `vsrctemp.c` (`VSRCki`).
    #[must_use]
    pub fn ki(&self) -> Real {
        0.5 / self.z0.sqrt()
    }
}

/// Independent V or I source. AC, DC and transient excitations are distinct.
#[derive(Debug, Clone)]
pub struct IndependentSource {
    name: String,
    /// `[positive, negative]`, plus the internal `#res` node of a port.
    terminals: Vec<NodeId>,
    voltage: bool,
    dc: Real,
    ac: Complex,
    /// Whether an AC value was written (C `VSRCacGiven`), even `ac 0`.
    ac_given: bool,
    waveform: Waveform,
    port: Option<RfPort>,
    /// What the card gave, as `.sens` replays C's records
    /// ([`crate::devices::sensitivity`]).
    sensitivity: crate::devices::sensitivity::SourceInputs,
    /// `distof1`/`distof2` distortion inputs (C `VSRCdF1given`, ...).
    distortion: [Option<crate::devices::distortion::DistortionInput>; 2],
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
            terminals: terminals.to_vec(),
            voltage,
            dc,
            ac,
            ac_given: ac != Complex::ZERO,
            sensitivity: crate::devices::sensitivity::SourceInputs {
                dc_given: true,
                function_given: !matches!(waveform, Waveform::Constant(_)),
                ac: (ac != Complex::ZERO)
                    .then(|| (ac.magnitude(), ac.im.atan2(ac.re).to_degrees())),
            },
            waveform,
            port: None,
            distortion: [None, None],
        })
    }

    /// Records what the card wrote for `.sens`: an explicit DC value, a
    /// transient function, and the AC magnitude and phase (degrees) when any
    /// AC setter was written.
    #[must_use]
    pub fn with_sensitivity_inputs(
        mut self,
        inputs: crate::devices::sensitivity::SourceInputs,
    ) -> Self {
        self.sensitivity = inputs;
        self
    }

    /// A copy driving `dc` in DC analyses and `ac` in AC analyses, for a
    /// `.sens` load.
    pub(crate) fn sensitivity_copy(&self, dc: Real, ac: Complex) -> SpiceResult<Self> {
        if !dc.is_finite() || !ac.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{}: nonfinite perturbed source value",
                self.name
            )));
        }
        Ok(Self {
            dc,
            ac,
            ..self.clone()
        })
    }

    /// Records the `distof1` (`f1`) and `distof2` (`f2`) distortion inputs
    /// (`vsrcpar.c`/`isrcpar.c` `VSRC_D_F1`/`VSRC_D_F2`), used only by
    /// `.disto`.
    /// # Errors
    /// A nonfinite magnitude or phase.
    pub fn with_distortion(
        mut self,
        f1: Option<crate::devices::distortion::DistortionInput>,
        f2: Option<crate::devices::distortion::DistortionInput>,
    ) -> SpiceResult<Self> {
        if [f1, f2]
            .iter()
            .flatten()
            .any(|i| !i.magnitude.is_finite() || !i.phase.is_finite())
        {
            return Err(SpiceError::circuit(format!(
                "{}: non-finite distortion input",
                self.name
            )));
        }
        self.distortion = [f1, f2];
        Ok(self)
    }

    /// Records whether the card wrote an AC value (`ac`, `acmag` or
    /// `acphase`; C `VSRCacGiven`/`ISRCacGiven`, even `ac 0`), which decides
    /// how pole-zero analysis treats a voltage source (`vsrcpzld.c`) and
    /// whether a `.noise` input reference is valid (`noisean.c`).
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

    /// Makes this voltage source an RF port (see the module documentation);
    /// the port's internal node becomes the third terminal.
    /// # Errors
    /// A current source, a port number of zero, a nonpositive or non-finite
    /// `z0`, non-finite `pwr`/`freq`/`phase`, or an internal node that is one
    /// of the external terminals.
    pub fn with_port(mut self, port: RfPort) -> SpiceResult<Self> {
        if !self.voltage {
            return Err(SpiceError::circuit(format!(
                "{}: only voltage sources can be RF ports",
                self.name
            )));
        }
        if port.number == 0
            || !(port.z0.is_finite() && port.z0 > 0.)
            || !(port.power.is_finite() && port.frequency.is_finite() && port.phase.is_finite())
        {
            return Err(SpiceError::circuit(format!(
                "{}: invalid RF port parameters",
                self.name
            )));
        }
        if self.terminals[..2].contains(&port.internal) {
            return Err(SpiceError::circuit(format!(
                "{}: the port's internal node must be distinct from its terminals",
                self.name
            )));
        }
        self.terminals.truncate(2);
        self.terminals.push(port.internal);
        self.port = Some(port);
        Ok(self)
    }

    /// The terminals of the ideal source: `[positive, negative]`, or
    /// `[#res, negative]` for a port.
    fn ideal(&self) -> [NodeId; 2] {
        [
            self.port
                .as_ref()
                .map_or(self.terminals[0], |port| port.internal),
            self.terminals[1],
        ]
    }
}

/// Fails for a port whose large-signal `PORT` time function (`pwr`/`freq`)
/// would drive a transient analysis: `vsrcload.c` adds it to the value left
/// over from the previously loaded source instance, which the port does not
/// reproduce. DC and small-signal analyses use the explicit DC value and the AC
/// phasor and are unaffected. Both transient backends call this first.
///
/// # Errors
/// [`SpiceError::NotYetPorted`] naming the first such port.
pub fn reject_transient_power_ports(circuit: &crate::devices::Circuit) -> SpiceResult<()> {
    for device in circuit.devices() {
        if device.rf_port().is_some_and(|port| port.power_function) {
            return Err(power_function_error(device.name()));
        }
    }
    Ok(())
}

fn power_function_error(name: &str) -> SpiceError {
    SpiceError::not_yet_ported(
        format!("{name}: the transient value of the RF port power function (pwr=/freq=)"),
        "src/spicelib/devices/vsrc/vsrcload.c (case PORT)",
    )
}

impl Device for IndependentSource {
    /// Noiseless: C's VSRC/ISRC have no noise routine (`DEVnoise = NULL`); the
    /// transient noise sources (`trnoise`/`trrandom`) are not ported.
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }

    /// `.sens`: C's VSRC/ISRC records ([`crate::devices::sensitivity`]). An
    /// RF port is refused: `VSRCtemp` re-registers ports on every call and
    /// the port's `z0`/`pwr`/`freq` perturbations are not ported.
    fn sensitivity(
        &self,
        _context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<Box<dyn crate::devices::sensitivity::DeviceSensitivity + '_>> {
        if self.port.is_some() {
            return Err(crate::devices::sensitivity::not_ported(
                &self.name,
                self.designator(),
                " (an RF port source)",
            ));
        }
        Ok(Box::new(
            crate::devices::sensitivity::SourceSensitivity::new(
                self,
                self.voltage,
                self.dc,
                self.sensitivity,
            ),
        ))
    }

    /// Linear, with its `distof1`/`distof2` inputs (`cktdisto.c` reads them
    /// for the first-order `D_RHSF1`/`D_RHSF2` solves).
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(match self.distortion {
            [None, None] => crate::devices::distortion::DeviceDistortion::Linear,
            [f1, f2] => crate::devices::distortion::DeviceDistortion::Input { f1, f2 },
        })
    }

    fn input_source(&self) -> Option<crate::devices::noise::InputSource> {
        Some(crate::devices::noise::InputSource {
            kind: if self.voltage {
                crate::devices::linear::SourceKind::Voltage
            } else {
                crate::devices::linear::SourceKind::Current
            },
            ac_given: self.ac_given,
        })
    }

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
    fn rf_port(&self) -> Option<&RfPort> {
        self.port.as_ref()
    }
    /// DC loads stamp the DC value. Companion transient loads stamp the time-`t`
    /// forcing, evaluated with the one-sided limit of `context.forcing`
    /// (`vsrcload.c`/`isrcload.c` evaluate the waveform at `CKTtime`). A port
    /// also stamps `1/z0` between its positive terminal and `#res`.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let value = match context.mode {
            AnalysisMode::OperatingPoint | AnalysisMode::DcSweep => self.dc,
            AnalysisMode::Transient { time, .. } => {
                if self.port.as_ref().is_some_and(|port| port.power_function) {
                    return Err(power_function_error(&self.name));
                }
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
        if let Some(port) = &self.port {
            let g0 = 1. / port.z0;
            let [positive, internal] = [self.terminals[0], port.internal];
            context.stamp(positive, positive, g0)?;
            context.stamp(internal, internal, g0)?;
            context.stamp(positive, internal, -g0)?;
            context.stamp(internal, positive, -g0)?;
        }
        if self.voltage {
            let branch = context.branch(0)?;
            crate::devices::linear::branch_stamp(
                context.matrix,
                context.unknowns,
                self.ideal(),
                branch,
            )?;
            context.rhs.add_to(branch, value)
        } else {
            context.stamp_rhs(self.terminals[0], -value)?;
            context.stamp_rhs(self.terminals[1], value)
        }
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        if let Some(port) = &self.port {
            context.nodal([self.terminals[0], port.internal], 1. / port.z0, false)?;
        }
        let rows = if self.voltage {
            vec![(context.branch(self.ideal())?, 1.)]
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
    /// in the AC matrix. An RF port is refused: `vsrcpzld.c` stamps only the
    /// ideal source between `<name>#res` and the negative terminal and omits
    /// the port's `1/z0` (which `vsrcload.c`/`vsrcacld.c` stamp), leaving the
    /// positive terminal disconnected in C's pole-zero matrix.
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        _bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        if !self.voltage {
            return Ok(());
        }
        if self.port.is_some() {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "pole-zero analysis of RF port {}: C's vsrcpzld.c omits the port's z0",
                    self.name
                ),
                location: None,
            });
        }
        if !self.ac_given {
            context.branch(self.ideal())?;
            return Ok(());
        }
        let branch = context
            .branch
            .ok_or_else(|| SpiceError::circuit("missing branch-row binding"))?;
        let [positive, negative] = self.ideal();
        for (node, sign) in [(positive, 1.), (negative, -1.)] {
            if let Some(row) = context.unknowns.node_row(node) {
                context.system.a.add(row, branch, sign)?;
            }
        }
        context.system.a.add(branch, branch, 1.)
    }
}
