//! Voltage- and current-controlled switches: S (model `sw`) and W (model `csw`).
//!
//! C references (read-only): `src/spicelib/parser/inp2s.c`, `inp2w.c`;
//! `src/spicelib/devices/sw/` (`sw.c`, `swsetup.c`, `swmparam.c`,
//! `swparam.c`, `swload.c`, `swacload.c`, `swtrunc.c`) and
//! `src/spicelib/devices/csw/` (`csw.c`, `cswsetup.c`, `cswmpar.c`,
//! `cswparam.c`, `cswload.c`, `cswacld.c`, `cswtrunc.c`).
//!
//! # Equations
//!
//! A switch is a conductance `g` between its first and second terminals,
//! `g = 1/RON` when closed and `g = 1/ROFF` when open (defaults: `RON` gives
//! 1 S, an omitted `ROFF` gives the circuit `gmin`, as `SW_OFF_CONDUCTANCE` is
//! `CKTgmin`). The control is `v(nc+) - v(nc-)` for S and the branch current of
//! the named voltage source for W (positive from its first terminal through it
//! to its second, as for F/H). Thresholds are `VT`/`VH` for S and `IT`/`IH` for
//! W, all defaulting to zero.
//!
//! # Discrete state
//!
//! Slot 0 holds the switch state with C's codes: `0` really off, `1` really
//! on, `2` off inside the hysteresis band, `3` on inside it; codes 1 and 3
//! stamp the on conductance. Slot 1 holds the control value. The state is
//! decided per Newton load from the [`IterationPhase`] exactly as
//! `SWload`/`CSWload` decide it from `MODEINITF`:
//!
//! * [`IterationPhase::Junction`]/[`IterationPhase::Fix`] (`MODEINITJCT`/
//!   `MODEINITFIX`): the instance `on`/`off` flag. An `on` switch is really on
//!   above `VT + |VH|` and otherwise on in the band; an `off` switch is really
//!   off below `VT - |VH|` and otherwise off in the band.
//! * [`IterationPhase::Predict`] (`MODEINITTRAN`/`MODEINITPRED`): for
//!   `VH > 0`, on above `VT + VH`, off below `VT - VH`, else the latest
//!   **accepted** state. For `VH <= 0` (including the default 0) the bounds
//!   are `VT - VH` and `VT + VH` and a value inside the band maps the accepted
//!   state as C does (S: band states kept, really-on becomes really-off and
//!   really-off really-on; W: really-on becomes off-in-band, really-off
//!   on-in-band).
//! * [`IterationPhase::Float`] (`MODEINITFLOAT`): the same bounds; inside a
//!   positive band S keeps the **previous iterate's** state (C `CKTstate0`)
//!   while W keeps the accepted state (C reads `CKTstate1` here), and the
//!   `VH <= 0` band maps the accepted state like S's `MODEINITFLOAT` branch
//!   (band states kept, really-on to off-in-band, really-off to on-in-band). A
//!   state different from the previous iterate's reports nonconvergence (C
//!   `CKTnoncon++`, "ensure one more iteration").
//!
//! Where C reads `CKTstate1` before any point was accepted (a plain `.op`),
//! its state vectors are zero-initialised, i.e. "really off"; the port uses
//! the same value. Only an accepted point ([`crate::Circuit::accept_point`])
//! ever changes the accepted state; trial and rejected loads cannot.
//!
//! # Timestep control
//!
//! [`Device::timestep_limit`] follows `swtrunc.c`: while really off with the
//! control rising towards `VT + VH`, or (any other state) falling towards
//! `VT - VH`, the next step is limited to
//! `(0.75 (ref - c) +/- 0.05) / (c - c_accepted) * dt`. C's `cswtrunc.c` uses
//! the same rule with `5e-5`, but `CSWload` never stores the control value (the
//! store is commented out, a `FIXME` in C), so the C limit never applies to W;
//! the port reproduces that and W sets no limit.
//!
//! # AC
//!
//! The small-signal conductance is that of the bias point's state (codes 1
//! and 3 on), the same rule the DC load used. This deliberately differs from
//! C in two ways: `ACan` reloads with `MODEINITSMSIG`, which copies
//! `CKTstate1` (still zero, "off", after a plain operating point) into
//! `CKTstate0`, and `SWacLoad`/`CSWacLoad` then treat every non-zero code,
//! including "off in band", as on. The port uses the converged operating-point
//! state instead.

use spice_core::{NodeId, NodeTable, Real, SpiceError, SpiceResult};
use spice_maths::Vector;
use spice_netlist::ast::{DeviceInstance, ParameterKind};

use crate::schema::{ScalarDomain, ScalarParameter, ScalarSchema, ScalarUnit};
use crate::state::IterationPhase;
use crate::traits::{ControlReference, Device, StampContext, TruncationContext};
use crate::{LinearContext, ModelContext, ResolvedModel};

/// Which switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchKind {
    /// S: voltage-controlled switch, model `sw`.
    Voltage,
    /// W: current-controlled switch, model `csw`.
    Current,
}

impl SwitchKind {
    const fn designator(self) -> char {
        match self {
            Self::Voltage => 's',
            Self::Current => 'w',
        }
    }

    const fn base(self) -> &'static str {
        match self {
            Self::Voltage => "sw",
            Self::Current => "csw",
        }
    }
}

/// C's switch-state codes (`swload.c`: `REALLY_OFF = 0`, `REALLY_ON = 1`,
/// `HYST_OFF = 2`, `HYST_ON = 3`), stored as the value of state slot 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchState {
    /// Open, control outside the hysteresis band.
    ReallyOff,
    /// Closed, control outside the hysteresis band.
    ReallyOn,
    /// Open, control inside the hysteresis band.
    HysteresisOff,
    /// Closed, control inside the hysteresis band.
    HysteresisOn,
}

impl SwitchState {
    /// The C code stored in the state vector.
    #[must_use]
    pub const fn code(self) -> Real {
        match self {
            Self::ReallyOff => 0.,
            Self::ReallyOn => 1.,
            Self::HysteresisOff => 2.,
            Self::HysteresisOn => 3.,
        }
    }

    /// The state for a stored code.
    #[must_use]
    pub fn from_code(code: Real) -> Option<Self> {
        match code {
            0. => Some(Self::ReallyOff),
            1. => Some(Self::ReallyOn),
            2. => Some(Self::HysteresisOff),
            3. => Some(Self::HysteresisOn),
            _ => None,
        }
    }

    /// True when the on conductance is stamped (`REALLY_ON` or `HYST_ON`).
    #[must_use]
    pub const fn is_closed(self) -> bool {
        matches!(self, Self::ReallyOn | Self::HysteresisOn)
    }
}

/// Validated model data at setup: thresholds and the explicit resistances.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SwitchModel {
    threshold: Real,
    hysteresis: Real,
    /// `1/RON`, or C's default 1 S.
    on_conductance: Real,
    /// `1/ROFF`; `None` selects the load's `gmin` (`SW_OFF_CONDUCTANCE`).
    off_conductance: Option<Real>,
}

const fn schema(kind: SwitchKind) -> ScalarSchema<'static> {
    const SW: &[ScalarParameter] = &[
        ScalarParameter {
            name: "vt",
            unit: ScalarUnit::Volt,
            domain: ScalarDomain::Finite,
            default: Some(0.),
        },
        ScalarParameter {
            name: "vh",
            unit: ScalarUnit::Volt,
            domain: ScalarDomain::Finite,
            default: Some(0.),
        },
        ScalarParameter {
            name: "ron",
            unit: ScalarUnit::Ohm,
            domain: ScalarDomain::NonZero,
            default: None,
        },
        ScalarParameter {
            name: "roff",
            unit: ScalarUnit::Ohm,
            domain: ScalarDomain::NonZero,
            default: None,
        },
    ];
    const CSW: &[ScalarParameter] = &[
        ScalarParameter {
            name: "it",
            unit: ScalarUnit::Ampere,
            domain: ScalarDomain::Finite,
            default: Some(0.),
        },
        ScalarParameter {
            name: "ih",
            unit: ScalarUnit::Ampere,
            domain: ScalarDomain::Finite,
            default: Some(0.),
        },
        ScalarParameter {
            name: "ron",
            unit: ScalarUnit::Ohm,
            domain: ScalarDomain::NonZero,
            default: None,
        },
        ScalarParameter {
            name: "roff",
            unit: ScalarUnit::Ohm,
            domain: ScalarDomain::NonZero,
            default: None,
        },
    ];
    ScalarSchema {
        parameters: match kind {
            SwitchKind::Voltage => SW,
            SwitchKind::Current => CSW,
        },
    }
}

/// An S or W switch. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq)]
pub struct Switch {
    name: String,
    kind: SwitchKind,
    /// `[n+, n-]`, then `[nc+, nc-]` for S.
    terminals: Vec<NodeId>,
    /// W's controlling voltage source; empty for S.
    control: Vec<ControlReference>,
    model: SwitchModel,
    /// The last `on`/`off` flag (C `SWzero_stateGiven`): `true` for `on`.
    initially_on: bool,
}

/// State slot of the switch state code.
const STATE: usize = 0;
/// State slot of the control value.
const CONTROL: usize = 1;

impl Switch {
    /// Builds an S/W instance from its AST and resolved `sw`/`csw` model.
    /// Validation happens before any node is interned.
    pub(crate) fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        _context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let kind = match instance.designator {
            's' => SwitchKind::Voltage,
            'w' => SwitchKind::Current,
            other => {
                return Err(SpiceError::circuit(format!(
                    "'{other}' instance {} is not a switch",
                    instance.name
                )));
            }
        };
        let expected = if kind == SwitchKind::Voltage { 4 } else { 2 };
        if instance.nodes.len() != expected {
            return Err(SpiceError::parse(
                instance.location.clone(),
                format!(
                    "{} needs {expected} terminals, got {}",
                    instance.name,
                    instance.nodes.len()
                ),
            ));
        }
        let card = model.card();
        let mut setters = Vec::new();
        for parameter in &card.parameters {
            if parameter.kind == ParameterKind::Flag && parameter.name == kind.base() {
                // SW_MOD_SW / CSW_CSW: "just says that this is a switch".
                continue;
            }
            if parameter.name.eq_ignore_ascii_case("level") {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "'level' on {} model '{}' (C ignores it with a warning)",
                        kind.base(),
                        card.name
                    ),
                    location: Some(parameter.location.clone()),
                });
            }
            setters.push(parameter);
        }
        let values = schema(kind).validate(setters, &card.location)?;
        let (threshold, hysteresis) = match kind {
            SwitchKind::Voltage => ("vt", "vh"),
            SwitchKind::Current => ("it", "ih"),
        };
        let get = |name: &str| values.get(name).map(|value| value.value);
        let conductance = |name: &str| -> SpiceResult<Option<Real>> {
            let Some(value) = values.get(name) else {
                return Ok(None);
            };
            let g = 1. / value.value;
            if !g.is_finite() {
                return Err(SpiceError::parse(
                    value
                        .location
                        .clone()
                        .unwrap_or_else(|| card.location.clone()),
                    format!("{name}={} has no finite conductance", value.value),
                ));
            }
            Ok(Some(g))
        };
        let parameters = SwitchModel {
            threshold: get(threshold).unwrap_or(0.),
            hysteresis: get(hysteresis).unwrap_or(0.),
            on_conductance: conductance("ron")?.unwrap_or(1.),
            off_conductance: conductance("roff")?,
        };
        let mut control = Vec::new();
        let mut initially_on = false;
        for parameter in &instance.parameters {
            match (&parameter.kind, parameter.name.as_str()) {
                (ParameterKind::Instance, "control") if kind == SwitchKind::Current => {
                    if !control.is_empty() {
                        return Err(SpiceError::parse(
                            parameter.location.clone(),
                            format!("{}: more than one controlling source", instance.name),
                        ));
                    }
                    control.push(ControlReference {
                        name: parameter.value.to_ascii_lowercase(),
                        location: Some(parameter.location.clone()),
                    });
                }
                (ParameterKind::Flag, "on") => initially_on = true,
                (ParameterKind::Flag, "off") => initially_on = false,
                _ => {
                    return Err(SpiceError::Unsupported {
                        feature: format!(
                            "{} parameter '{}' on a '{}' switch",
                            instance.name,
                            parameter.name,
                            kind.designator()
                        ),
                        location: Some(parameter.location.clone()),
                    });
                }
            }
        }
        if kind == SwitchKind::Current && control.is_empty() {
            return Err(SpiceError::parse(
                instance.location.clone(),
                format!("{} needs a controlling voltage source", instance.name),
            ));
        }
        let mut staged = nodes.clone();
        let terminals = instance
            .nodes
            .iter()
            .map(|node| staged.intern(node))
            .collect();
        *nodes = staged;
        Ok(Box::new(Self {
            name: instance.name.clone(),
            kind,
            terminals,
            control,
            model: parameters,
            initially_on,
        }))
    }

    /// Which switch this is.
    #[must_use]
    pub const fn kind(&self) -> SwitchKind {
        self.kind
    }

    /// The control value at `solution`: `v(nc+) - v(nc-)` or the controlling
    /// branch current.
    fn control_value(
        &self,
        node: impl Fn(NodeId) -> Real,
        row: impl Fn(usize) -> SpiceResult<Real>,
        controls: &[usize],
    ) -> SpiceResult<Real> {
        match self.kind {
            SwitchKind::Voltage => Ok(node(self.terminals[2]) - node(self.terminals[3])),
            SwitchKind::Current => {
                let branch = controls.first().ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: controlling source is not bound to a branch row",
                        self.name
                    ))
                })?;
                row(*branch)
            }
        }
    }

    /// `SWload`/`CSWload` under `MODEINITJCT`/`MODEINITFIX`.
    fn flag_state(&self, control: Real) -> SwitchState {
        let SwitchModel {
            threshold: t,
            hysteresis: h,
            ..
        } = self.model;
        if self.initially_on {
            if (h >= 0. && control > t + h) || (h < 0. && control > t - h) {
                SwitchState::ReallyOn
            } else {
                SwitchState::HysteresisOn
            }
        } else if (h >= 0. && control < t - h) || (h < 0. && control < t + h) {
            SwitchState::ReallyOff
        } else {
            SwitchState::HysteresisOff
        }
    }

    /// The state for one load. `accepted` is C's `CKTstate1` (zero, "really
    /// off", before any accepted point); `iterate` is `CKTstate0` from the
    /// previous load of the solve.
    fn decide(
        &self,
        phase: IterationPhase,
        control: Real,
        accepted: SwitchState,
        iterate: Option<SwitchState>,
    ) -> SpiceResult<SwitchState> {
        let SwitchModel {
            threshold: t,
            hysteresis: h,
            ..
        } = self.model;
        let band = |inside: SwitchState| {
            if h > 0. {
                if control > t + h {
                    SwitchState::ReallyOn
                } else if control < t - h {
                    SwitchState::ReallyOff
                } else {
                    inside
                }
            } else if control > t - h {
                SwitchState::ReallyOn
            } else if control < t + h {
                SwitchState::ReallyOff
            } else {
                inside
            }
        };
        // The `VH <= 0` band maps the accepted state; S's MODEINITPRED branch
        // swaps really-on/really-off, every other branch enters the band.
        let entered = |swap: bool| match accepted {
            SwitchState::HysteresisOff | SwitchState::HysteresisOn => accepted,
            SwitchState::ReallyOn if swap => SwitchState::ReallyOff,
            SwitchState::ReallyOff if swap => SwitchState::ReallyOn,
            SwitchState::ReallyOn => SwitchState::HysteresisOff,
            SwitchState::ReallyOff => SwitchState::HysteresisOn,
        };
        Ok(match phase {
            IterationPhase::Junction | IterationPhase::Fix => self.flag_state(control),
            IterationPhase::Predict => band(if h > 0. {
                accepted
            } else {
                entered(self.kind == SwitchKind::Voltage)
            }),
            IterationPhase::Float => {
                let iterate = iterate.ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: MODEINITFLOAT load without a previous iterate",
                        self.name
                    ))
                })?;
                band(if h <= 0. {
                    entered(false)
                } else if self.kind == SwitchKind::Voltage {
                    iterate
                } else {
                    accepted
                })
            }
        })
    }

    fn conductance(&self, state: SwitchState, gmin: Real) -> Real {
        if state.is_closed() {
            self.model.on_conductance
        } else {
            self.model.off_conductance.unwrap_or(gmin)
        }
    }

    fn stored(&self, value: Option<Real>, what: &str) -> SpiceResult<Option<SwitchState>> {
        value
            .map(|code| {
                SwitchState::from_code(code).ok_or_else(|| SpiceError::Numerical {
                    context: self.name.clone(),
                    message: format!("invalid {what} switch state {code}"),
                })
            })
            .transpose()
    }
}

impl Device for Switch {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        self.kind.designator()
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn controlling_sources(&self) -> &[ControlReference] {
        &self.control
    }

    fn is_nonlinear(&self) -> bool {
        true
    }

    fn state_count(&self) -> usize {
        2
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit(format!(
                "{}: switch AC needs small-signal assembly",
                self.name
            )));
        }
        let control = self.control_value(
            |node| context.node_voltage(node),
            |row| context.row_value(row),
            context.controls,
        )?;
        let accepted = self
            .stored(context.states.accepted(1, STATE), "accepted")?
            .unwrap_or(SwitchState::ReallyOff);
        let iterate = self.stored(context.states.iterate(STATE), "iterate")?;
        let phase = context.states.phase();
        let state = self.decide(phase, control, accepted, iterate)?;
        if phase == IterationPhase::Float && iterate != Some(state) {
            context.states.report_nonconvergence()?;
        }
        context.states.set(STATE, state.code())?;
        context.states.set(CONTROL, control)?;
        let g = self.conductance(state, context.gmin);
        crate::linear::nodal_stamp(
            context.matrix,
            context.unknowns,
            [self.terminals[0], self.terminals[1]],
            g,
        )
    }

    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let state = match context.states {
            Some(states) => self
                .stored(states.get(STATE).copied(), "bias-point")?
                .ok_or_else(|| {
                    SpiceError::circuit(format!("{}: missing bias-point state", self.name))
                })?,
            // Zero-bias assemblies (forcing/breakpoint extraction) carry no
            // solved state: use the MODEINITJCT state at `bias`.
            None => {
                let unknowns = context.unknowns;
                let control = self.control_value(
                    |node| {
                        unknowns
                            .node_row(node)
                            .and_then(|row| bias.get(row))
                            .unwrap_or(0.)
                    },
                    |row| {
                        bias.get(row).ok_or_else(|| {
                            SpiceError::circuit("bias row out of range for a switch control")
                        })
                    },
                    context.controls,
                )?;
                self.flag_state(control)
            }
        };
        let g = self.conductance(state, context.model_context.gmin);
        context.nodal([self.terminals[0], self.terminals[1]], g, false)
    }

    fn timestep_limit(&self, context: &TruncationContext<'_>) -> SpiceResult<Option<Real>> {
        if self.kind == SwitchKind::Current {
            // cswtrunc.c reads a control slot CSWload never writes.
            return Ok(None);
        }
        let (Some(code), Some(control), Some(previous)) = (
            context.trial.get(STATE).copied(),
            context.trial.get(CONTROL).copied(),
            context.accepted.and_then(|a| a.get(CONTROL)).copied(),
        ) else {
            return Ok(None);
        };
        let change = control - previous;
        let SwitchModel {
            threshold: t,
            hysteresis: h,
            ..
        } = self.model;
        // C tests `CKTstate0[SWswitchstate] == 0` (really off) only.
        let max_change = if code == 0. {
            let reference = t + h;
            if !(control < reference && change > 0.) {
                return Ok(None);
            }
            (reference - control) * 0.75 + 0.05
        } else {
            let reference = t - h;
            if !(control > reference && change < 0.) {
                return Ok(None);
            }
            (reference - control) * 0.75 - 0.05
        };
        Ok(Some(max_change / change * context.dt))
    }
}

#[cfg(test)]
mod tests {
    use super::{Switch, SwitchKind, SwitchModel, SwitchState};
    use crate::state::IterationPhase;
    use spice_core::NodeId;

    fn switch(kind: SwitchKind, threshold: f64, hysteresis: f64, on: bool) -> Switch {
        Switch {
            name: "s1".into(),
            kind,
            terminals: vec![NodeId::GROUND; 4],
            control: Vec::new(),
            model: SwitchModel {
                threshold,
                hysteresis,
                on_conductance: 1.,
                off_conductance: None,
            },
            initially_on: on,
        }
    }

    use SwitchState::{HysteresisOff, HysteresisOn, ReallyOff, ReallyOn};

    #[test]
    fn codes_round_trip() {
        for state in [ReallyOff, ReallyOn, HysteresisOff, HysteresisOn] {
            assert_eq!(SwitchState::from_code(state.code()), Some(state));
        }
        assert_eq!(SwitchState::from_code(-1.), None);
        assert!(ReallyOn.is_closed() && HysteresisOn.is_closed());
        assert!(!ReallyOff.is_closed() && !HysteresisOff.is_closed());
    }

    #[test]
    fn initial_flags_follow_swload_initjct() {
        let off = switch(SwitchKind::Voltage, 1., 0.5, false);
        assert_eq!(off.flag_state(0.4), ReallyOff);
        assert_eq!(off.flag_state(0.5), HysteresisOff);
        assert_eq!(off.flag_state(5.), HysteresisOff);
        let on = switch(SwitchKind::Voltage, 1., 0.5, true);
        assert_eq!(on.flag_state(1.6), ReallyOn);
        assert_eq!(on.flag_state(1.5), HysteresisOn);
        assert_eq!(on.flag_state(-5.), HysteresisOn);
        // Negative hysteresis uses |VH|.
        let negative = switch(SwitchKind::Voltage, 1., -0.5, true);
        assert_eq!(negative.flag_state(1.6), ReallyOn);
        assert_eq!(negative.flag_state(1.4), HysteresisOn);
    }

    #[test]
    fn positive_hysteresis_band_keeps_the_previous_state() {
        let s = switch(SwitchKind::Voltage, 1., 0.5, false);
        let decide = |phase, control, accepted, iterate| {
            s.decide(phase, control, accepted, iterate).unwrap()
        };
        // Rising: off until VT + VH is exceeded.
        assert_eq!(
            decide(IterationPhase::Predict, 1.4, ReallyOff, None),
            ReallyOff
        );
        assert_eq!(
            decide(IterationPhase::Predict, 1.51, ReallyOff, None),
            ReallyOn
        );
        // Falling: on until below VT - VH.
        assert_eq!(
            decide(IterationPhase::Predict, 0.6, ReallyOn, None),
            ReallyOn
        );
        assert_eq!(
            decide(IterationPhase::Predict, 0.49, ReallyOn, None),
            ReallyOff
        );
        // S float keeps the previous iterate inside the band, W the accepted state.
        assert_eq!(
            decide(IterationPhase::Float, 1., ReallyOff, Some(ReallyOn)),
            ReallyOn
        );
        let w = switch(SwitchKind::Current, 1., 0.5, false);
        assert_eq!(
            w.decide(IterationPhase::Float, 1., ReallyOff, Some(ReallyOn))
                .unwrap(),
            ReallyOff
        );
        assert!(
            s.decide(IterationPhase::Float, 1., ReallyOff, None)
                .is_err()
        );
    }

    #[test]
    fn zero_hysteresis_switches_at_the_threshold() {
        let s = switch(SwitchKind::Voltage, 2., 0., false);
        for phase in [IterationPhase::Predict, IterationPhase::Float] {
            assert_eq!(
                s.decide(phase, 2.001, ReallyOff, Some(ReallyOff)).unwrap(),
                ReallyOn
            );
            assert_eq!(
                s.decide(phase, 1.999, ReallyOn, Some(ReallyOn)).unwrap(),
                ReallyOff
            );
        }
        // Exactly at the threshold the C band rules map the accepted state.
        assert_eq!(
            s.decide(IterationPhase::Predict, 2., ReallyOn, None)
                .unwrap(),
            ReallyOff
        );
        assert_eq!(
            s.decide(IterationPhase::Float, 2., ReallyOn, Some(ReallyOn))
                .unwrap(),
            HysteresisOff
        );
        let w = switch(SwitchKind::Current, 2., 0., false);
        assert_eq!(
            w.decide(IterationPhase::Predict, 2., ReallyOn, None)
                .unwrap(),
            HysteresisOff
        );
        assert_eq!(
            w.decide(IterationPhase::Predict, 2., ReallyOff, None)
                .unwrap(),
            HysteresisOn
        );
        assert_eq!(
            w.decide(IterationPhase::Predict, 2., HysteresisOn, None)
                .unwrap(),
            HysteresisOn
        );
    }

    #[test]
    fn negative_hysteresis_band_maps_the_accepted_state() {
        let s = switch(SwitchKind::Voltage, 1., -0.5, false);
        // Band is [0.5, 1.5]; outside it the switch follows the control.
        assert_eq!(
            s.decide(IterationPhase::Predict, 1.6, ReallyOff, None)
                .unwrap(),
            ReallyOn
        );
        assert_eq!(
            s.decide(IterationPhase::Predict, 0.4, ReallyOn, None)
                .unwrap(),
            ReallyOff
        );
        assert_eq!(
            s.decide(IterationPhase::Predict, 1., ReallyOff, None)
                .unwrap(),
            ReallyOn
        );
        assert_eq!(
            s.decide(IterationPhase::Float, 1., ReallyOff, Some(ReallyOn))
                .unwrap(),
            HysteresisOn
        );
        assert_eq!(
            s.decide(IterationPhase::Float, 1., HysteresisOff, Some(ReallyOn))
                .unwrap(),
            HysteresisOff
        );
    }
}
