//! Lossless transmission line `T` (C `src/spicelib/devices/tra/`).
//!
//! ```text
//! Tname p1 n1 p2 n2 z0=<ohms> [td=<s> | f=<Hz> [nl=<length>]] [ic=v1,i1,v2,i2]
//!       [v1= i1= v2= i2=] [rel=<r>] [abs=<a>]
//! ```
//!
//! # Model (Branin, method of characteristics)
//!
//! Each port is the characteristic impedance `Z0` in series with a voltage
//! source driven by the wave that left the *other* port `TD` earlier:
//!
//! ```text
//! v(int1) - v(n1) = [v(p2) - v(n2) + Z0 i2](t - TD)      (input1)
//! v(int2) - v(n2) = [v(p1) - v(n1) + Z0 i1](t - TD)      (input2)
//! ```
//!
//! `i1`/`i2` are the currents **into** the positive terminal of port 1/2
//! (through `Z0` from `p1` to the internal node `int1`, through the source to
//! `n1`). As in C (`trasetup.c` creates them with `CKTmkVolt`), the two
//! currents and the two internal nodes are ordinary node unknowns named
//! `<name>#i1`, `<name>#i2`, `<name>#int1`, `<name>#int2`, so plots and
//! rawfiles carry `v(t1#i1)` (a current in amperes, typed `voltage` exactly as
//! ngspice writes it), `v(t1#i2)`, `v(t1#int1)`, `v(t1#int2)`.
//!
//! | Analysis | Port | C |
//! | --- | --- | --- |
//! | DC (`.op`, `.dc`, transient bias) | the delay is zero: `v(int1) - v(n1) = v(p2) - v(n2) + (1 - gmin) Z0 i2` (and symmetrically) | `traload.c`, `MODEDC` |
//! | AC, `.sp` | the exact factor `exp(-j omega TD)` on the cross terms, reassembled at every frequency ([`Device::small_signal_depends_on_frequency`]) | `traacld.c` |
//! | transient (companion driver) | `input1/2` interpolated quadratically from the accepted history at `t - TD` ([`DelayLine`]); history appended and pruned only on accepted points; device breakpoints and step bound | `traload.c`, `traacct.c`, `tratrunc.c` |
//! | diffsol BDF, pole-zero, noise, distortion, sensitivity, `uic` | explicit errors | C has no `DEVpzLoad`/`DEVnoise`/`DEVdisto`/`DEVsen*` for TRA |
//!
//! AC: the companion pencil is `A + j omega E`, which cannot hold
//! `exp(-j omega TD)`; at each frequency the real part of every complex
//! coefficient goes into `A` and its imaginary part divided by `omega` into
//! `E`, which reproduces the coefficient exactly at that frequency. At
//! frequency zero (the operating-point assembly) the line stamps its DC form.
//!
//! # Parameters
//!
//! Setters apply in written order (the last one wins, `TRAparam`); `zo` is
//! an alias of `z0`; `ic=` is C's `IF_REALVEC` fallthrough (one to four
//! values set `v1`, `i1`, `v2`, `i2` in that order). `z0` is required
//! (`TRAsetup`: "transmission line z0 must be given"). `td` given wins; else
//! `td = nl / f` with `nl = 0.25` and `f = 1 GHz` defaults (`trasetup.c`,
//! `tratemp.c`). `rel`/`abs` (defaults 1 and 1) are the slope-change
//! tolerances of the breakpoint and step-bound tests. Deliberate divergences,
//! all explicit errors: `z0 <= 0` (C divides by it), `td <= 0` (C's history
//! then has coincident times and skips the source load), nonfinite values,
//! and a leading positional value (C ignores it silently).

use crate::devices::delay::{DelayContext, DelayHistory, DelayLine, DelayUpdate};
use crate::devices::linear::LinearContext;
use crate::devices::traits::{AnalysisMode, Device, StampContext};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ParameterKind};
use crate::primitives::{
    NodeId, NodeKind, NodeTable, Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number,
};

/// C reference for errors.
const C_REFERENCE: &str = "src/spicelib/devices/tra/";

/// Internal-node suffixes in C's `trasetup.c` creation order.
pub const INTERNAL_SUFFIXES: [&str; 4] = ["i1", "i2", "int1", "int2"];

/// A lossless transmission line instance.
#[derive(Debug, Clone, PartialEq)]
pub struct TransmissionLine {
    name: String,
    /// `p1 n1 p2 n2`, then the internal `i1 i2 int1 int2`.
    terminals: [NodeId; 8],
    z0: Real,
    td: Real,
    nl: Real,
    f: Real,
    /// `v1 i1 v2 i2` initial conditions (used only by `uic`).
    initial: [Real; 4],
    reltol: Real,
    abstol: Real,
    location: SourceLoc,
}

fn invalid(location: &SourceLoc, message: impl Into<String>) -> SpiceError {
    SpiceError::Parse {
        location: location.clone(),
        message: message.into(),
    }
}

impl TransmissionLine {
    /// Builds the line from its instance card, interning the four terminals
    /// and the four internal nodes (nothing is interned on error).
    ///
    /// # Errors
    /// Missing `z0`, invalid values, unknown setters or node-name collisions.
    pub fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
    ) -> SpiceResult<Box<dyn Device>> {
        if instance.nodes.len() != 4 || instance.model.is_some() {
            return Err(SpiceError::circuit(format!(
                "{}: a transmission line has four terminals and no model",
                instance.name
            )));
        }
        let mut z0 = None;
        let mut td = None;
        let mut nl = 0.25;
        let mut f = 1e9;
        let mut initial = [0.; 4];
        let mut reltol = 1.;
        let mut abstol = 1.;
        let number = |text: &str, at: &SourceLoc, name: &str| {
            parse_spice_number(text)
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid(at, format!("{name} must be a finite number, not '{text}'")))
        };
        for p in &instance.parameters {
            match (&p.kind, p.name.as_str()) {
                (ParameterKind::InitialConditions(values), "ic") => {
                    for value in values {
                        let slot = match value.name.as_str() {
                            "v1" => 0,
                            "i1" => 1,
                            "v2" => 2,
                            "i2" => 3,
                            other => {
                                return Err(invalid(
                                    &value.value.location,
                                    format!("unexpected ic component {other}"),
                                ));
                            }
                        };
                        initial[slot] = number(&value.value.text, &value.value.location, "ic")?;
                    }
                }
                (ParameterKind::Scalar, name) => {
                    let value = number(&p.value, &p.location, name)?;
                    match name {
                        "z0" | "zo" => z0 = Some(value),
                        "td" => td = Some(value),
                        "nl" => nl = value,
                        "f" => f = value,
                        "v1" => initial[0] = value,
                        "i1" => initial[1] = value,
                        "v2" => initial[2] = value,
                        "i2" => initial[3] = value,
                        "rel" => reltol = value,
                        "abs" => abstol = value,
                        other => {
                            return Err(invalid(
                                &p.location,
                                format!("unknown transmission-line parameter {other}"),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(SpiceError::not_yet_ported(
                        format!(
                            "{}: transmission-line setter '{}' of this form",
                            p.location, p.name
                        ),
                        "src/spicelib/devices/tra/traparam.c",
                    ));
                }
            }
        }
        let Some(z0) = z0 else {
            return Err(invalid(
                &instance.location,
                format!(
                    "{}: transmission line z0 must be given (trasetup.c)",
                    instance.name
                ),
            ));
        };
        if z0 <= 0. || !(1. / z0).is_finite() {
            return Err(invalid(
                &instance.location,
                format!("{}: z0 must be positive, got {z0}", instance.name),
            ));
        }
        let td = match td {
            Some(td) => td,
            None => {
                if f <= 0. || nl <= 0. {
                    return Err(invalid(
                        &instance.location,
                        format!(
                            "{}: f and nl must be positive when td is not given (td = nl/f)",
                            instance.name
                        ),
                    ));
                }
                nl / f
            }
        };
        if !(td.is_finite() && td > 0.) {
            return Err(invalid(
                &instance.location,
                format!(
                    "{}: td must be positive and finite, got {td}",
                    instance.name
                ),
            ));
        }
        if reltol < 0. || abstol < 0. {
            return Err(invalid(
                &instance.location,
                format!("{}: rel and abs must not be negative", instance.name),
            ));
        }
        let mut staged = nodes.clone();
        let mut terminals = [NodeId::GROUND; 8];
        for (slot, node) in instance.nodes.iter().enumerate() {
            terminals[slot] = staged.intern(node);
        }
        for (slot, suffix) in INTERNAL_SUFFIXES.iter().enumerate() {
            let name = format!("{}#{suffix}", instance.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "transmission-line internal-node name collision: {name}"
                )));
            }
            let node = staged.intern(&name);
            staged.set_kind(node, NodeKind::Internal);
            terminals[4 + slot] = node;
        }
        *nodes = staged;
        Ok(Box::new(Self {
            name: instance.name.clone(),
            terminals,
            z0,
            td,
            nl,
            f,
            initial,
            reltol,
            abstol,
            location: instance.location.clone(),
        }))
    }

    /// Characteristic impedance in ohms.
    #[must_use]
    pub const fn z0(&self) -> Real {
        self.z0
    }

    /// Propagation delay in seconds (`td`, or `nl / f`).
    #[must_use]
    pub const fn td(&self) -> Real {
        self.td
    }

    const fn pos1(&self) -> NodeId {
        self.terminals[0]
    }
    const fn neg1(&self) -> NodeId {
        self.terminals[1]
    }
    const fn pos2(&self) -> NodeId {
        self.terminals[2]
    }
    const fn neg2(&self) -> NodeId {
        self.terminals[3]
    }
    const fn ibr1(&self) -> NodeId {
        self.terminals[4]
    }
    const fn ibr2(&self) -> NodeId {
        self.terminals[5]
    }
    const fn int1(&self) -> NodeId {
        self.terminals[6]
    }
    const fn int2(&self) -> NodeId {
        self.terminals[7]
    }

    /// The `(row, column, value)` entries every analysis stamps (the two
    /// `Z0` resistors and the source branch incidences, `traload.c`).
    fn static_entries(&self) -> [(NodeId, NodeId, Real); 16] {
        let g = 1. / self.z0;
        [
            (self.pos1(), self.pos1(), g),
            (self.pos1(), self.int1(), -g),
            (self.neg1(), self.ibr1(), -1.),
            (self.pos2(), self.pos2(), g),
            (self.neg2(), self.ibr2(), -1.),
            (self.int1(), self.pos1(), -g),
            (self.int1(), self.int1(), g),
            (self.int1(), self.ibr1(), 1.),
            (self.int2(), self.int2(), g),
            (self.int2(), self.ibr2(), 1.),
            (self.ibr1(), self.neg1(), -1.),
            (self.ibr1(), self.int1(), 1.),
            (self.ibr2(), self.neg2(), -1.),
            (self.ibr2(), self.int2(), 1.),
            (self.pos2(), self.int2(), -g),
            (self.int2(), self.pos2(), -g),
        ]
    }

    /// The cross-port entries with coefficient `c` on the voltages and
    /// `c z` on the currents: the DC load (`c = 1`, `z = (1 - gmin) Z0`) or
    /// the AC load (`c = exp(-j omega TD)`, `z = Z0`) of `traload.c` and
    /// `traacld.c`, as `(row, column, factor of c, scale)` pairs.
    fn cross_entries(&self, z: Real) -> [(NodeId, NodeId, Real); 6] {
        [
            (self.ibr1(), self.pos2(), -1.),
            (self.ibr1(), self.neg2(), 1.),
            (self.ibr1(), self.ibr2(), -z),
            (self.ibr2(), self.pos1(), -1.),
            (self.ibr2(), self.neg1(), 1.),
            (self.ibr2(), self.ibr1(), -z),
        ]
    }

    /// `[v(p2) - v(n2) + Z0 i2, v(p1) - v(n1) + Z0 i1]` of a solution: the
    /// waves launched towards port 1 and port 2 (C `TRAinput1/2` sources).
    fn waves(&self, context: &DelayContext<'_>) -> [Real; 2] {
        let v = |node| context.node_voltage(node);
        [
            (v(self.pos2()) - v(self.neg2())) + v(self.ibr2()) * self.z0,
            (v(self.pos1()) - v(self.neg1())) + v(self.ibr1()) * self.z0,
        ]
    }

    /// `traload.c` (`MODEINITPRED`): the waves at `time - TD`, interpolated
    /// quadratically through the history samples `i-2, i-1, i` where `i` is
    /// the first sample (from the third on) later than `time - TD`, or the
    /// last one (extrapolation).
    fn delayed(&self, history: &DelayHistory, time: Real) -> SpiceResult<[Real; 2]> {
        let size = history
            .len()
            .checked_sub(1)
            .filter(|size| *size >= 2)
            .ok_or_else(|| {
                SpiceError::circuit(format!(
                    "{}: delay history used before the transient start",
                    self.name
                ))
            })?;
        let target = time - self.td;
        let at = |index: usize| history.time(index).unwrap_or(Real::NAN);
        let mut i = 2;
        while i < size && at(i) <= target {
            i += 1;
        }
        let (t1, t2, t3) = (at(i - 2), at(i - 1), at(i));
        if t2 - t1 == 0. || t3 - t2 == 0. || t3 - t1 == 0. {
            // C skips the source load (`continue`); history times are
            // strictly increasing here, so this is an internal error.
            return Err(SpiceError::circuit(format!(
                "{}: coincident delay-history times",
                self.name
            )));
        }
        // Same operation order as traload.c.
        let mut f1 = (target - t2) * (target - t3);
        let mut f2 = (target - t1) * (target - t3);
        let mut f3 = (target - t1) * (target - t2);
        f1 /= t1 - t2;
        f2 /= t2 - t1;
        f2 /= t2 - t3;
        f3 /= t2 - t3;
        f1 /= t1 - t3;
        f3 /= t1 - t3;
        let value =
            |index: usize, component: usize| history.value(index, component).unwrap_or(Real::NAN);
        let result = [0, 1].map(|component| {
            f1 * value(i - 2, component) + f2 * value(i - 1, component) + f3 * value(i, component)
        });
        if result.iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: self.name.clone(),
                message: "nonfinite delayed wave".into(),
            });
        }
        Ok(result)
    }

    /// `traacct.c`/`tratrunc.c`'s test: the slope of either wave changed by
    /// at least `rel * max(|d1|, |d2|) + abs`.
    fn slope_changed(&self, slopes: [(Real, Real); 2]) -> bool {
        slopes
            .iter()
            .any(|(d1, d2)| (d1 - d2).abs() >= self.reltol * d1.abs().max(d2.abs()) + self.abstol)
    }

    /// Stamps a coefficient that may be complex into a small-signal system:
    /// `A += Re(c)`, `E += Im(c) / omega`.
    fn stamp_small_signal(&self, context: &mut LinearContext<'_>, dc: bool) -> SpiceResult<()> {
        let frequency = context.model_context.frequency;
        let (z, real, imag, omega) = if dc || frequency == 0. {
            ((1. - context.model_context.gmin) * self.z0, 1., 0., 0.)
        } else {
            let omega = 2. * std::f64::consts::PI * frequency;
            let phase = -omega * self.td;
            (self.z0, phase.cos(), phase.sin(), omega)
        };
        let unknowns = context.unknowns;
        let row = |node: NodeId| unknowns.node_row(node);
        for (r, c, value) in self.static_entries() {
            if let (Some(r), Some(c)) = (row(r), row(c)) {
                context.system.a.add(r, c, value)?;
            }
        }
        for (r, c, value) in self.cross_entries(z) {
            if let (Some(r), Some(c)) = (row(r), row(c)) {
                context.system.a.add(r, c, value * real)?;
                if imag != 0. {
                    context.system.e.add(r, c, value * imag / omega)?;
                }
            }
        }
        Ok(())
    }

    fn unsupported(&self, analysis: &str) -> SpiceError {
        SpiceError::not_yet_ported(
            format!(
                "{analysis} with transmission line {} (C has no TRA routine for it)",
                self.name
            ),
            C_REFERENCE,
        )
    }
}

impl Device for TransmissionLine {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        't'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn small_signal_depends_on_frequency(&self) -> bool {
        true
    }

    fn delay_line(&self) -> Option<&dyn DelayLine> {
        Some(self)
    }

    fn observation_parameter(
        &self,
        keyword: &str,
        _context: &crate::devices::ModelContext,
    ) -> SpiceResult<Option<Real>> {
        // TRAask (traask.c), the scalar real parameters.
        Ok(match keyword {
            "z0" | "zo" => Some(self.z0),
            "td" => Some(self.td),
            "nl" => Some(self.nl),
            "f" => Some(self.f),
            "v1" => Some(self.initial[0]),
            "i1" => Some(self.initial[1]),
            "v2" => Some(self.initial[2]),
            "i2" => Some(self.initial[3]),
            "rel" => Some(self.reltol),
            "abs" => Some(self.abstol),
            _ => None,
        })
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        for (r, c, value) in self.static_entries() {
            context.stamp(r, c, value)?;
        }
        match context.mode {
            AnalysisMode::OperatingPoint | AnalysisMode::DcSweep => {
                let z = (1. - context.gmin) * self.z0;
                for (r, c, value) in self.cross_entries(z) {
                    context.stamp(r, c, value)?;
                }
            }
            AnalysisMode::Transient { time, .. } => {
                let history = context.states.delay_history().ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: transient load without a delay history (companion driver only)",
                        self.name
                    ))
                })?;
                let [input1, input2] = self.delayed(history, time)?;
                context.stamp_rhs(self.ibr1(), input1)?;
                context.stamp_rhs(self.ibr2(), input2)?;
            }
            AnalysisMode::Ac { .. } => {
                return Err(SpiceError::circuit(format!(
                    "{}: the AC load is assembled by assemble_small_signal",
                    self.name
                )));
            }
        }
        Ok(())
    }

    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        self.stamp_small_signal(context, true)
    }

    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        self.stamp_small_signal(context, false)
    }

    fn assemble_pole_zero(
        &self,
        _context: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        Err(SpiceError::Unsupported {
            feature: format!(
                "pole-zero analysis of transmission line {}: exp(-s TD) is not a polynomial \
                 pencil, and C has no TRA pole-zero load (DEVpzLoad is NULL in trainit.c, so \
                 ngspice silently leaves the line out of the matrix)",
                self.name
            ),
            location: Some(self.location.clone()),
        })
    }

    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(self.unsupported(".noise"))
    }

    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(self.unsupported(".disto"))
    }
}

impl DelayLine for TransmissionLine {
    fn width(&self) -> usize {
        2
    }

    fn start(&self, context: &DelayContext<'_>) -> SpiceResult<DelayUpdate> {
        let [v1, i1, v2, i2] = self.initial;
        let wave = if context.initial_conditions {
            // traload.c MODEINITTRAN | MODEUIC.
            [v2 + i2 * self.z0, v1 + i1 * self.z0]
        } else {
            self.waves(context)
        };
        // traload.c MODEINITTRAN: three equal samples at -2TD, -TD and 0.
        Ok(DelayUpdate {
            reset: Some(vec![
                (-2. * self.td, wave.to_vec()),
                (-self.td, wave.to_vec()),
                (0., wave.to_vec()),
            ]),
            ..DelayUpdate::default()
        })
    }

    /// `traacct.c` (the `#else` branch C compiles).
    fn accept(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<DelayUpdate> {
        let time = context.time;
        let size = history
            .len()
            .checked_sub(1)
            .filter(|s| *s >= 2)
            .ok_or_else(|| {
                SpiceError::circuit(format!("{}: delay history too short", self.name))
            })?;
        let at = |index: usize| history.time(index).unwrap_or(Real::NAN);
        let value =
            |index: usize, component: usize| history.value(index, component).unwrap_or(Real::NAN);
        let mut update = DelayUpdate::default();
        // Shift out samples no later interpolation can reach, keeping two
        // before the first one at or after `time - TD`.
        if time - self.td > at(2) {
            let mut i = 2;
            while i < size && time - self.td > at(i) {
                i += 1;
            }
            update.drop_front = i - 2;
        }
        if time - at(size) > context.min_break {
            let wave = self.waves(context);
            update.append = Some((time, wave.to_vec()));
            let [h0, h1, ..] = context.steps;
            let slopes = [0, 1].map(|k| {
                (
                    (wave[k] - value(size, k)) / h0,
                    (value(size, k) - value(size - 1, k)) / h1,
                )
            });
            if self.slope_changed(slopes) {
                // The slope changed at the previous sample: its wave reaches
                // the other port TD later.
                let when = at(size) + self.td;
                if when > time {
                    update.breakpoints.push(when);
                }
            }
        }
        Ok(update)
    }

    /// `tratrunc.c`, with C's step indices (`CKTdeltaOld[1]` and `[2]`, the
    /// two latest accepted steps, although the trial spans `CKTdeltaOld[0]`).
    fn timestep_limit(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<Option<Real>> {
        let size = history
            .len()
            .checked_sub(1)
            .filter(|s| *s >= 1)
            .ok_or_else(|| {
                SpiceError::circuit(format!("{}: delay history too short", self.name))
            })?;
        let at = |index: usize| history.time(index).unwrap_or(Real::NAN);
        let value =
            |index: usize, component: usize| history.value(index, component).unwrap_or(Real::NAN);
        let wave = self.waves(context);
        let [_, h1, h2] = context.steps;
        let slopes = [0, 1].map(|k| {
            (
                (wave[k] - value(size, k)) / h1,
                (value(size, k) - value(size - 1, k)) / h2,
            )
        });
        Ok(self
            .slope_changed(slopes)
            .then(|| at(size) + self.td - context.time))
    }
}
