//! Single lossy transmission lines (`Y`, `txl`).
//!
//! `inp2y.c` either expands a high-R/L line into three R and four C devices,
//! or binds two signal nodes (the written reference terminals are ignored by
//! C's Padé device). `txlsetup.c` constructs the Padé coefficients and
//! `txlload.c` stamps DC or recursive-convolution transient equations.
//! AC/SP are explicitly refused: C reuses TXLload with an unset delta.
//! See `docs/port/TXL.md` for the compatibility boundaries.
mod pade;
mod transient;
use crate::devices::linear::LinearContext;
use crate::devices::models::{ModelResolver, ResolvedModel};
use crate::devices::rlc::{Capacitor, Resistor};
use crate::devices::schema::{ScalarDomain, ScalarParameter, ScalarSchema, ScalarUnit};
use crate::devices::traits::{AnalysisMode, Device, StampContext};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ModelCard, ParameterKind};
use crate::primitives::{NodeId, NodeTable, Real, SpiceError, SpiceResult};
use std::collections::BTreeSet;

const fn scalar(name: &'static str, unit: ScalarUnit, domain: ScalarDomain) -> ScalarParameter {
    ScalarParameter {
        name,
        domain,
        unit,
        default: None,
    }
}
const MODEL: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        scalar("r", ScalarUnit::OhmPerMetre, ScalarDomain::NonNegative),
        scalar("l", ScalarUnit::HenryPerMetre, ScalarDomain::NonNegative),
        scalar("g", ScalarUnit::SiemensPerMetre, ScalarDomain::NonNegative),
        scalar("c", ScalarUnit::FaradPerMetre, ScalarDomain::Positive),
        scalar("length", ScalarUnit::Metre, ScalarDomain::Positive),
    ],
};
const INSTANCE: ScalarSchema<'static> = ScalarSchema {
    parameters: &[scalar("len", ScalarUnit::Metre, ScalarDomain::Positive)],
};
#[derive(Debug, Clone, Copy)]
struct Model {
    r: Real,
    l: Real,
    g: Real,
    c: Real,
    length: Real,
}
impl Model {
    fn read(card: &ModelCard) -> SpiceResult<Self> {
        let mut setters = Vec::new();
        for p in &card.parameters {
            if p.kind == ParameterKind::Flag && p.name == "txl" {
                continue;
            }
            setters.push(p);
        }
        let values = MODEL.validate(setters, &card.location)?;
        let get = |key: &str| {
            values.get(key).map(|v| v.value).ok_or_else(|| {
                SpiceError::parse(
                    card.location.clone(),
                    format!("TXL model {}: {key} must be given (txlsetup.c)", card.name),
                )
            })
        };
        Ok(Self {
            r: get("r")?,
            l: get("l")?,
            g: get("g")?,
            c: get("c")?,
            length: get("length")?,
        })
    }
}
fn inputs(
    instance: &DeviceInstance,
    model: &ResolvedModel<'_>,
) -> SpiceResult<(Model, Real, Option<Real>)> {
    if instance.nodes.len() != 4 {
        return Err(SpiceError::parse(
            instance.location.clone(),
            "TXL requires four written terminals (inp2y.c)",
        ));
    }
    let m = Model::read(model.card())?;
    let v = INSTANCE.validate(instance.parameters.iter(), &instance.location)?;
    let given = v.get("len").map(|v| v.value);
    Ok((m, given.unwrap_or(m.length), given))
}
/// `inp2y.c`'s high-resistance branch. The peculiar value `3/(R*length)`
/// is passed to the resistor's **resistance** setter in C, not conductance.
/// The Y instance is absent on this path. Generated nodes remain visible.
pub(crate) fn fallback(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
    models: &ModelResolver<'_>,
    deck_nodes: &BTreeSet<&str>,
) -> SpiceResult<Option<Vec<Box<dyn Device>>>> {
    let resolved = models
        .resolve(instance)?
        .ok_or_else(|| SpiceError::circuit("TXL requires a model"))?;
    let (m, length, _) = inputs(instance, &resolved)?;
    if !(m.r != 0. && m.l != 0. && m.r / m.l > 1.6e10) {
        return Ok(None);
    }
    let mut staged = nodes.clone();
    let terminals: Vec<_> = instance.nodes.iter().map(|s| staged.intern(s)).collect();
    let mut line = vec![terminals[0]];
    for k in 1..=2 {
        let name = format!("txlnd{k}{}", instance.name);
        if staged.get(&name).is_some() || deck_nodes.contains(name.as_str()) {
            return Err(SpiceError::circuit(format!(
                "TXL generated-node collision: {name}"
            )));
        }
        line.push(staged.intern(&name));
    }
    line.push(terminals[2]);
    let r = 3. / (m.r * length);
    let c = m.c * length / 6.;
    let mut devices: Vec<Box<dyn Device>> = Vec::new();
    for k in 0..3 {
        devices.push(Box::new(Resistor::new(
            format!("txlres{}{}", k + 1, instance.name),
            [line[k], line[k + 1]],
            r,
        )?));
    }
    for (k, node) in line.iter().enumerate() {
        devices.push(Box::new(Capacitor::new(
            format!("txlcap{}{}", k + 1, instance.name),
            [*node, terminals[1]],
            if k == 0 || k == 3 { c } else { 2. * c },
            None,
        )?));
    }
    *nodes = staged;
    Ok(Some(devices))
}
/// A Padé-path TXL instance. Its two branch currents point into the line
/// at the respective signal terminals, as `txlload.c` stamps them.
#[derive(Debug, Clone)]
pub struct Txl {
    name: String,
    terminals: [NodeId; 2],
    /// The instance `len` setter, if given. C's `@y1[length]` reports this
    /// field alone, so a model-level `length` reads as 0 (measured).
    instance_length: Option<Real>,
    resistance: Real,
    coefficients: pade::Pade,
}
impl Txl {
    pub(crate) fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
    ) -> SpiceResult<Box<dyn Device>> {
        let (m, length, instance_length) = inputs(instance, model)?;
        let coefficients = pade::Pade::new(m.r, m.l, m.g, m.c, length)?;
        let ids: Vec<_> = instance.nodes.iter().map(|n| nodes.intern(n)).collect();
        let resistance = m.r * length;
        if !resistance.is_finite() {
            return Err(SpiceError::circuit("TXL R*length overflow"));
        }
        Ok(Box::new(Self {
            name: instance.name.clone(),
            terminals: [ids[0], ids[2]],
            instance_length,
            resistance,
            coefficients,
        }))
    }
    fn unsupported(&self, analysis: &str) -> SpiceError {
        SpiceError::not_yet_ported(
            format!("{}: TXL {analysis}", self.name),
            "src/spicelib/devices/txl/txlinit.c: DEVacLoad reuses TXLload with an unset CKTdelta rather than a frequency-domain stamp; DEVpzLoad/DEVnoise/DEVdisto are NULL (docs/port/TXL.md, TX0)",
        )
    }
    /// TSTALLOC order shared by setup and each load; row 0 is never ground.
    fn positions(
        p: Option<usize>,
        n: Option<usize>,
        a: usize,
        b: usize,
    ) -> [(Option<usize>, Option<usize>); 14] {
        let a = Some(a);
        let b = Some(b);
        [
            (p, p),
            (p, n),
            (n, p),
            (n, n),
            (a, p),
            (b, n),
            (n, b),
            (p, a),
            (a, a),
            (b, b),
            (a, n),
            (b, p),
            (a, b),
            (b, a),
        ]
    }
    fn dc_values(&self, gmin: Real) -> [Real; 14] {
        let g = 0.1 * gmin;
        [
            g,
            g,
            g,
            g,
            0.,
            -1.,
            1.,
            1.,
            1.,
            0.,
            0.,
            1.,
            1.,
            -self.resistance,
        ]
    }
}
impl Device for Txl {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'y'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn branch_currents(&self) -> usize {
        2
    }
    fn delay_line(&self) -> Option<&dyn crate::devices::delay::DelayLine> {
        Some(self)
    }
    fn setup_matrix(
        &self,
        setup: &mut crate::devices::setup::MatrixSetup<'_>,
    ) -> SpiceResult<bool> {
        let (p, n) = (setup.node(self.terminals[0]), setup.node(self.terminals[1]));
        for (r, c) in Self::positions(p, n, setup.branch(0)?, setup.branch(1)?) {
            setup.element(r, c);
        }
        Ok(true)
    }
    fn stamp(&self, ctx: &mut StampContext<'_>) -> SpiceResult<()> {
        let p = ctx.unknowns.node_row(self.terminals[0]);
        let n = ctx.unknowns.node_row(self.terminals[1]);
        let a = ctx.branch(0)?;
        let b = ctx.branch(1)?;
        let (values, rhs) = match ctx.mode {
            AnalysisMode::OperatingPoint | AnalysisMode::DcSweep => {
                (self.dc_values(ctx.gmin), [0.; 2])
            }
            AnalysisMode::Ac { .. } => return Err(self.unsupported("AC/SP")),
            AnalysisMode::Transient { time, dt } => {
                let history = ctx.states.delay_history().ok_or_else(|| {
                    SpiceError::circuit("TXL transient requires accepted delay history")
                })?;
                self.transient_stamp(history, time, dt, ctx.gmin)?
            }
        };
        for ((r, c), v) in Self::positions(p, n, a, b).into_iter().zip(values) {
            if let (Some(r), Some(c)) = (r, c) {
                ctx.matrix.add(r, c, v)?;
            }
        }
        ctx.rhs.add_to(a, rhs[0])?;
        ctx.rhs.add_to(b, rhs[1])?;
        Ok(())
    }
    fn assemble_linear(&self, ctx: &mut LinearContext<'_>) -> SpiceResult<()> {
        let a = ctx
            .branch
            .ok_or_else(|| SpiceError::circuit("TXL missing branches"))?;
        let p = ctx.unknowns.node_row(self.terminals[0]);
        let n = ctx.unknowns.node_row(self.terminals[1]);
        for ((r, c), v) in Self::positions(p, n, a, a + 1)
            .into_iter()
            .zip(self.dc_values(ctx.model_context.gmin))
        {
            if let (Some(r), Some(c)) = (r, c) {
                ctx.system.a.add(r, c, v)?;
            }
        }
        Ok(())
    }
    fn assemble_small_signal(
        &self,
        ctx: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        // Source-resolution probes in a DC sweep have no bias state.
        if ctx.states.is_none() {
            self.assemble_linear(ctx)
        } else {
            Err(self.unsupported("AC/SP"))
        }
    }
    fn assemble_pole_zero(&self, _ctx: &mut LinearContext<'_>, _bias: &Vector) -> SpiceResult<()> {
        Err(self.unsupported("pole-zero"))
    }
    fn noise(
        &self,
        _ctx: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(self.unsupported("noise"))
    }
    fn distortion(
        &self,
        _ctx: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(self.unsupported("distortion"))
    }
    /// `TXLask`: only `length`, which C reports from the instance field, so a
    /// model-level `length` reads as 0. The `in_node`/`out_node` entries C's
    /// table declares are refused by its own front end ("no such parameter"),
    /// so they are not exposed here either.
    fn observation_parameter(
        &self,
        keyword: &str,
        _ctx: &crate::devices::ModelContext,
    ) -> SpiceResult<Option<Real>> {
        Ok(match keyword {
            "length" => Some(self.instance_length.unwrap_or(0.)),
            _ => None,
        })
    }
}
