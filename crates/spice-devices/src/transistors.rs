//! Bounded Ebers-Moll BJT and Shichman-Hodges MOS1 equations.
//! C: `bjt/bjtload.c`, `bjtsetup.c`, `mos1/mos1load.c`, `mos1temp.c`.
//! Advanced charge, temperature, series-node and high-injection parameters
//! are explicit errors, not ignored setters. MOS intrinsic Meyer capacitance
//! (`tox > 0`) is not implemented; absent/zero TOX matches C's zero oxide cap.
use crate::limiting::{self, Limiter, Linearization};
use crate::nonlinear::{
    JunctionPoint, K_OVER_Q, depletion_charge, junction_current, stamp_junction, value,
};
use crate::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::{Device, LinearContext, ModelContext, ModelFamily, ResolvedModel, StampContext};
use spice_core::{NodeId, NodeTable, Real, SpiceError, SpiceResult};
use spice_maths::Vector;
use spice_netlist::ast::DeviceInstance;

/// Effective drain/source, channel current, its terminal partials and the
/// linearization offset `sum(partial * voltage)` at the evaluated voltages.
/// First BJT state slot of the limited `vbe`, `vbc`, `vsub` (C `BJTvbe`,
/// `BJTvbc`, `BJTvsub`).
const BJT_LIMITED_SLOTS: usize = 4;
/// `bjtdefs.h` `VCRIT_DISABLED`: the critical voltage of a substrate junction
/// without a saturation current.
const BJT_SUBSTRATE_VCRIT: Real = 50.;
/// First MOS1 state slot of the limited `vbs`, `vgs`, `vds` and the previous
/// load's threshold `von` (C `MOS1vbs`, `MOS1vgs`, `MOS1vds`, `MOS1von`).
const MOS1_LIMITED_SLOTS: usize = 10;

type ChannelPoint = ([NodeId; 2], Real, [(NodeId, Real); 4], Real);

fn scalar(name: &'static str, unit: U, domain: D, default: Option<Real>) -> P {
    P {
        name,
        unit,
        domain,
        default,
    }
}
fn instance_values(instance: &DeviceInstance, mos: bool) -> SpiceResult<ScalarValues> {
    let mut schema = vec![
        scalar("m", U::Dimensionless, D::Positive, Some(1.)),
        scalar("temp", U::Celsius, D::Temperature, None),
    ];
    if mos {
        schema.extend([
            scalar("l", U::Metre, D::Positive, Some(1e-4)),
            scalar("w", U::Metre, D::Positive, Some(1e-4)),
        ]);
    } else {
        schema.push(scalar("area", U::Dimensionless, D::Positive, Some(1.)));
    }
    ScalarSchema {
        parameters: &schema,
    }
    .validate(&instance.parameters, &instance.location)
}
fn nominal(context: &ModelContext, temp: Option<Real>, tnom: Option<Real>) -> SpiceResult<Real> {
    let temperature = temp.unwrap_or(context.temperature);
    let reference = tnom.unwrap_or(context.nominal_temperature);
    if temperature != reference {
        return Err(SpiceError::Unsupported {
            feature: "non-nominal BJT/MOS1 temperature physics".into(),
            location: None,
        });
    }
    let kelvin = temperature + 273.15;
    if !kelvin.is_finite() || kelvin <= 0. {
        return Err(SpiceError::circuit("invalid transistor temperature"));
    }
    Ok(K_OVER_Q * kelvin)
}
fn polarity(family: ModelFamily) -> Real {
    if matches!(family, ModelFamily::Pnp | ModelFamily::Pmos) {
        -1.
    } else {
        1.
    }
}
fn bias_voltage(context: &LinearContext<'_>, bias: &Vector, node: NodeId) -> Real {
    context
        .unknowns
        .node_row(node)
        .and_then(|r| bias.get(r))
        .unwrap_or(0.)
}
/// Arbitrary terminal-current Jacobian: current leaving `output[0]` and
/// entering `output[1]`. `partials` are actual physical voltage derivatives;
/// `linearized` is `sum(partial * voltage)` at the (possibly limited) voltages
/// the current was evaluated at ([`node_linearization`] for the solution).
fn stamp_current(
    context: &mut StampContext<'_>,
    output: [NodeId; 2],
    current: Real,
    partials: &[(NodeId, Real)],
    linearized: Real,
) -> SpiceResult<()> {
    let equivalent = current - linearized;
    for (row, sign) in [(output[0], 1.), (output[1], -1.)] {
        for (col, derivative) in partials {
            context.stamp(row, *col, sign * derivative)?;
        }
        context.stamp_rhs(row, -sign * equivalent)?;
    }
    Ok(())
}
/// `sum(partial * node voltage)` at the present solution.
fn node_linearization(context: &StampContext<'_>, partials: &[(NodeId, Real)]) -> Real {
    partials
        .iter()
        .map(|(node, derivative)| derivative * context.node_voltage(*node))
        .sum()
}
fn linear_current(
    context: &mut LinearContext<'_>,
    output: [NodeId; 2],
    partials: &[(NodeId, Real)],
) -> SpiceResult<()> {
    for (row, sign) in [(output[0], 1.), (output[1], -1.)] {
        for (col, derivative) in partials {
            if let (Some(r), Some(c)) = (
                context.unknowns.node_row(row),
                context.unknowns.node_row(*col),
            ) {
                context.system.a.add(r, c, sign * derivative)?;
            }
        }
    }
    Ok(())
}
fn charge_point(
    v: Real,
    depletion: (Real, Real, Real, Real),
    diffusion: (Real, Real, Real),
    pol: Real,
) -> JunctionPoint {
    let (c, p, m, fc) = depletion;
    let (tt, i, g) = diffusion;
    let (q, cap) = depletion_charge(pol * v, c, p, m, fc);
    JunctionPoint {
        current: 0.,
        conductance: 0.,
        charge: pol * (q + tt * i),
        capacitance: cap + tt * g,
    }
}

/// Classic BJT bounded to Ebers-Moll transport and two junction charges.
#[derive(Debug)]
pub struct Bjt {
    name: String,
    nodes: Vec<NodeId>,
    pol: Real,
    is: Real,
    bf: Real,
    br: Real,
    nf: Real,
    nr: Real,
    cje: Real,
    cjc: Real,
    vje: Real,
    vjc: Real,
    mje: Real,
    mjc: Real,
    fc: Real,
    tf: Real,
    tr: Real,
    temp: Option<Real>,
    tnom: Option<Real>,
    /// Instance multiplier `m`. Area and `m` both scale `is`/`cje`/`cjc`, but
    /// only `m` scales the `CKTgmin` junction terms (`bjtload.c` stamps every
    /// conductance as `m * g`; area never touches `gmin`).
    multiplier: Real,
}
impl Bjt {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if model.levels().selector != 1 || !(3..=4).contains(&i.nodes.len()) {
            return Err(SpiceError::Unsupported {
                feature: "BJT backend requires level 1 and 3/4 terminals".into(),
                location: Some(i.location.clone()),
            });
        }
        let schema = [
            scalar("is", U::Ampere, D::Positive, Some(1e-16)),
            scalar("bf", U::Dimensionless, D::Positive, Some(100.)),
            scalar("br", U::Dimensionless, D::Positive, Some(1.)),
            scalar("nf", U::Dimensionless, D::Positive, Some(1.)),
            scalar("nr", U::Dimensionless, D::Positive, Some(1.)),
            scalar("tnom", U::Celsius, D::Temperature, None),
            scalar("cje", U::Farad, D::NonNegative, Some(0.)),
            scalar("cjc", U::Farad, D::NonNegative, Some(0.)),
            scalar("vje", U::Volt, D::Positive, Some(0.75)),
            scalar("vjc", U::Volt, D::Positive, Some(0.75)),
            scalar("mje", U::Dimensionless, D::NonNegative, Some(0.33)),
            scalar("mjc", U::Dimensionless, D::NonNegative, Some(0.33)),
            scalar("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
            scalar("tf", U::Second, D::NonNegative, Some(0.)),
            scalar("tr", U::Second, D::NonNegative, Some(0.)),
        ];
        let m = model.parameters(&ScalarSchema {
            parameters: &schema,
        })?;
        let instance = instance_values(i, false)?;
        let scale = value(&instance, "area")? * value(&instance, "m")?;
        let mut device = Self {
            name: i.name.clone(),
            nodes: vec![],
            pol: polarity(model.family()),
            is: value(&m, "is")? * scale,
            bf: value(&m, "bf")?,
            br: value(&m, "br")?,
            nf: value(&m, "nf")?,
            nr: value(&m, "nr")?,
            cje: value(&m, "cje")? * scale,
            cjc: value(&m, "cjc")? * scale,
            vje: value(&m, "vje")?,
            vjc: value(&m, "vjc")?,
            mje: value(&m, "mje")?,
            mjc: value(&m, "mjc")?,
            fc: value(&m, "fc")?,
            tf: value(&m, "tf")?,
            tr: value(&m, "tr")?,
            temp: instance.get("temp").map(|v| v.value),
            tnom: m.get("tnom").map(|v| v.value),
            multiplier: value(&instance, "m")?,
        };
        if device.mje >= 1. || device.mjc >= 1. || device.fc >= 1. || !scale.is_finite() {
            return Err(SpiceError::circuit(
                "BJT MJE/MJC/FC must be below 1; finite area*m required",
            ));
        }
        nominal(context, device.temp, device.tnom)?;
        junction_current(0., device.nf * K_OVER_Q * 300.15, device.is)?;
        if [device.cje, device.cjc].iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::circuit("BJT geometry overflow"));
        }
        let mut staged = nodes.clone();
        device.nodes = i.nodes.iter().map(|n| staged.intern(n)).collect();
        *nodes = staged;
        Ok(Box::new(device))
    }
    fn points(
        &self,
        vbe: Real,
        vbc: Real,
        context: &ModelContext,
    ) -> SpiceResult<[(Real, Real, JunctionPoint); 2]> {
        let vt = nominal(context, self.temp, self.tnom)?;
        let (ibe, gbe) = junction_current(self.pol * vbe, self.nf * vt, self.is)?;
        let (ibc, gbc) = junction_current(self.pol * vbc, self.nr * vt, self.is)?;
        let be = charge_point(
            vbe,
            (self.cje, self.vje, self.mje, self.fc),
            (self.tf, ibe, gbe),
            self.pol,
        );
        let bc = charge_point(
            vbc,
            (self.cjc, self.vjc, self.mjc, self.fc),
            (self.tr, ibc, gbc),
            self.pol,
        );
        be.validate()?;
        bc.validate()?;
        Ok([(self.pol * ibe, gbe, be), (self.pol * ibc, gbc, bc)])
    }
}
impl Bjt {
    /// `bjtload.c`'s junction voltages for this load in the device frame
    /// (`type * v`): `[vbe, vbc, vsub]`, `vsub` measured from the substrate's
    /// connection node to the substrate. `MODEINITJCT` starts at
    /// `vbe = tVcrit`, `vbc = vsub = 0`; later loads apply `DEVpnjlim` with
    /// `vt = kT/q` (no emission coefficient) and `tVcrit` from the area-scaled
    /// saturation current (`bjttemp.c`; `m` scales the stamps, not `tSatCur`);
    /// the gmin-only substrate junction uses `VCRIT_DISABLED` (50 V).
    fn limit_junctions(
        &self,
        limiter: &mut Limiter,
        states: &crate::DeviceState<'_>,
        raw: [Real; 3],
        vt: Real,
    ) -> [Real; 3] {
        let vcrit = limiting::critical_voltage(vt, self.is / self.multiplier);
        if limiter.mode() == Linearization::Initial {
            return [vcrit, 0., 0.];
        }
        let mut limited = raw;
        for (index, critical) in [vcrit, vcrit, BJT_SUBSTRATE_VCRIT].into_iter().enumerate() {
            let previous = limiter.previous(states, BJT_LIMITED_SLOTS + index);
            limited[index] = limiter.pn_junction(raw[index], previous, vt, critical);
        }
        limited
    }

    /// The substrate node (the optional fourth terminal, else ground) and the
    /// node its junction connects to: the collector for NPN (default vertical
    /// geometry), the base for PNP (default lateral), as `bjtsetup.c` chooses
    /// when the `subs` model parameter is not given.
    fn substrate(&self) -> [NodeId; 2] {
        let substrate = self.nodes.get(3).copied().unwrap_or(NodeId::GROUND);
        let connection = if self.pol > 0. {
            self.nodes[0]
        } else {
            self.nodes[1]
        };
        [substrate, connection]
    }
}
impl Device for Bjt {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'q'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.nodes
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn state_count(&self) -> usize {
        // BE/BC charge-derivative pairs, then the limited vbe, vbc and vsub.
        7
    }
    fn truncation_slots(&self) -> Vec<usize> {
        vec![0, 2]
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("BJT AC requires small-signal assembly"));
        }
        let (c, b, e) = (self.nodes[0], self.nodes[1], self.nodes[2]);
        let [substrate, connection] = self.substrate();
        let raw = [
            self.pol * (context.node_voltage(b) - context.node_voltage(e)),
            self.pol * (context.node_voltage(b) - context.node_voltage(c)),
            context.node_voltage(substrate) - context.node_voltage(connection),
        ];
        let vt = nominal(&context.model_context(), self.temp, self.tnom)?;
        let mut limiter = Limiter::new(&context.states);
        let limited = self.limit_junctions(&mut limiter, &context.states, raw, vt);
        let (vbe, vbc) = (self.pol * limited[0], self.pol * limited[1]);
        let [(ibe, gbe, qbe), (ibc, gbc, qbc)] = self.points(vbe, vbc, &context.model_context())?;
        let gmin = self.multiplier * context.gmin;
        stamp_current(
            context,
            [c, e],
            ibe - ibc,
            &[(b, gbe - gbc), (e, -gbe), (c, gbc)],
            gbe * vbe - gbc * vbc,
        )?;
        stamp_current(
            context,
            [b, e],
            ibe / self.bf,
            &[(b, gbe / self.bf), (e, -gbe / self.bf)],
            gbe / self.bf * vbe,
        )?;
        stamp_current(
            context,
            [b, c],
            ibc / self.br,
            &[(b, gbc / self.br), (c, -gbc / self.br)],
            gbc / self.br * vbc,
        )?;
        for (slot, ports, v, q) in [(0, [b, e], vbe, qbe), (2, [b, c], vbc, qbc)] {
            stamp_junction(
                context,
                ports,
                v,
                JunctionPoint {
                    current: gmin * v,
                    conductance: gmin,
                    ..q
                },
                slot,
            )?;
        }
        // bjtload.c: without a substrate saturation current the substrate
        // junction is just CKTgmin between the substrate node and its
        // connection node (see `substrate`), scaled by `m` like every BJT term.
        // Its (limited) voltage only decides convergence: a linear current's
        // stamp does not depend on the linearization point.
        let v = context.node_voltage(connection) - context.node_voltage(substrate);
        let partials = [(connection, gmin), (substrate, -gmin)];
        let linearized = node_linearization(context, &partials);
        stamp_current(
            context,
            [connection, substrate],
            gmin * v,
            &partials,
            linearized,
        )?;
        limiter.finish(
            &mut context.states,
            &[
                (BJT_LIMITED_SLOTS, limited[0]),
                (BJT_LIMITED_SLOTS + 1, limited[1]),
                (BJT_LIMITED_SLOTS + 2, limited[2]),
            ],
        )
    }
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let (c, b, e) = (self.nodes[0], self.nodes[1], self.nodes[2]);
        let vbe = bias_voltage(context, bias, b) - bias_voltage(context, bias, e);
        let vbc = bias_voltage(context, bias, b) - bias_voltage(context, bias, c);
        let [(_, gbe, qbe), (_, gbc, qbc)] = self.points(vbe, vbc, context.model_context)?;
        linear_current(context, [c, e], &[(b, gbe - gbc), (e, -gbe), (c, gbc)])?;
        linear_current(context, [b, e], &[(b, gbe / self.bf), (e, -gbe / self.bf)])?;
        linear_current(context, [b, c], &[(b, gbc / self.br), (c, -gbc / self.br)])?;
        let gmin = self.multiplier * context.model_context.gmin;
        for (ports, q) in [([b, e], qbe), ([b, c], qbc)] {
            context.nodal(ports, gmin, false)?;
            context.nodal(ports, q.capacitance, true)?;
        }
        context.nodal(self.substrate(), gmin, false)?;
        Ok(())
    }
}

/// MOS level 1, including drain/source reversal, body effect and overlap charge.
#[derive(Debug)]
pub struct Mos1 {
    name: String,
    nodes: [NodeId; 4],
    pol: Real,
    vto: Real,
    lambda: Real,
    gamma: Real,
    phi: Real,
    is: Real,
    beta: Real,
    cbd: Real,
    cbs: Real,
    pb: Real,
    mj: Real,
    fc: Real,
    overlap: [Real; 3],
    temp: Option<Real>,
    tnom: Option<Real>,
}
impl Mos1 {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if model.levels().selector != 1 || i.nodes.len() != 4 {
            return Err(SpiceError::circuit("MOS1 needs level 1 and four terminals"));
        }
        let schema = [
            scalar("kp", U::AmperePerVoltSquared, D::Positive, Some(2e-5)),
            scalar("vto", U::Volt, D::Finite, Some(0.)),
            scalar("lambda", U::InverseVolt, D::NonNegative, Some(0.)),
            scalar("gamma", U::SquareRootVolt, D::NonNegative, Some(0.)),
            scalar("phi", U::Volt, D::Positive, Some(0.6)),
            scalar("is", U::Ampere, D::Positive, Some(1e-14)),
            scalar("cbd", U::Farad, D::NonNegative, Some(0.)),
            scalar("cbs", U::Farad, D::NonNegative, Some(0.)),
            scalar("pb", U::Volt, D::Positive, Some(0.8)),
            scalar("mj", U::Dimensionless, D::NonNegative, Some(0.5)),
            scalar("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
            scalar("cgso", U::FaradPerMetre, D::NonNegative, Some(0.)),
            scalar("cgdo", U::FaradPerMetre, D::NonNegative, Some(0.)),
            scalar("cgbo", U::FaradPerMetre, D::NonNegative, Some(0.)),
            scalar("tox", U::Metre, D::NonNegative, Some(0.)),
            scalar("tnom", U::Celsius, D::Temperature, None),
        ];
        let m = model.parameters(&ScalarSchema {
            parameters: &schema,
        })?;
        let instance = instance_values(i, true)?;
        if value(&m, "tox")? > 0. {
            return Err(SpiceError::Unsupported {
                feature: "MOS1 intrinsic Meyer/channel charge (TOX > 0)".into(),
                location: Some(i.location.clone()),
            });
        }
        let mult = value(&instance, "m")?;
        let w = value(&instance, "w")?;
        let l = value(&instance, "l")?;
        let mut device = Self {
            name: i.name.clone(),
            nodes: [NodeId::GROUND; 4],
            pol: polarity(model.family()),
            vto: value(&m, "vto")?,
            lambda: value(&m, "lambda")?,
            gamma: value(&m, "gamma")?,
            phi: value(&m, "phi")?,
            is: value(&m, "is")? * mult,
            beta: value(&m, "kp")? * mult * w / l,
            cbd: value(&m, "cbd")? * mult,
            cbs: value(&m, "cbs")? * mult,
            pb: value(&m, "pb")?,
            mj: value(&m, "mj")?,
            fc: value(&m, "fc")?,
            overlap: [
                value(&m, "cgso")? * w * mult,
                value(&m, "cgdo")? * w * mult,
                value(&m, "cgbo")? * l * mult,
            ],
            temp: instance.get("temp").map(|v| v.value),
            tnom: m.get("tnom").map(|v| v.value),
        };
        nominal(context, device.temp, device.tnom)?;
        if device.mj >= 1.
            || device.fc >= 1.
            || [
                device.beta,
                device.is,
                device.cbd,
                device.cbs,
                device.overlap[0],
                device.overlap[1],
                device.overlap[2],
            ]
            .iter()
            .any(|v| !v.is_finite())
            || device.beta <= 0.
            || device.is <= 0.
        {
            return Err(SpiceError::circuit("invalid MOS1 geometry or MJ/FC >= 1"));
        }
        let mut staged = nodes.clone();
        for (port, name) in device.nodes.iter_mut().zip(&i.nodes) {
            *port = staged.intern(name);
        }
        *nodes = staged;
        Ok(Box::new(device))
    }
    fn channel(&self, v: [Real; 4]) -> SpiceResult<ChannelPoint> {
        let [d, g, s, b] = self.nodes;
        let (drain, source, vd, vs) = if self.pol * (v[0] - v[2]) >= 0. {
            (d, s, v[0], v[2])
        } else {
            (s, d, v[2], v[0])
        };
        let vds = self.pol * (vd - vs);
        let vgs = self.pol * (v[1] - vs);
        let vbs = self.pol * (v[3] - vs);
        if self.gamma > 0. && vbs > 0. {
            return Err(SpiceError::Unsupported {
                feature: "MOS1 forward-body-bias threshold/limiting with GAMMA > 0".into(),
                location: None,
            });
        }
        let root = self.phi.sqrt();
        let (body, body_derivative) = if vbs <= 0. {
            let a = (self.phi - vbs).sqrt();
            (a, 0.5 / a)
        } else {
            (root - vbs / (2. * root), 0.5 / root)
        };
        let over = vgs - (self.pol * self.vto + self.gamma * (body - root));
        let (current, gm, gds) = if over <= 0. {
            (0., 0., 0.)
        } else if vds >= over {
            (
                0.5 * self.beta * over * over * (1. + self.lambda * vds),
                self.beta * over * (1. + self.lambda * vds),
                0.5 * self.beta * over * over * self.lambda,
            )
        } else {
            let base = self.beta * vds * (over - 0.5 * vds);
            (
                base * (1. + self.lambda * vds),
                self.beta * vds * (1. + self.lambda * vds),
                self.beta * (over - vds) * (1. + self.lambda * vds) + base * self.lambda,
            )
        };
        let gmb = gm * self.gamma * body_derivative;
        if [current, gm, gds, gmb].iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: "MOS1 channel".into(),
                message: "nonfinite channel current/Jacobian".into(),
            });
        }
        Ok((
            [drain, source],
            self.pol * current,
            [(drain, gds), (g, gm), (source, -gds - gm - gmb), (b, gmb)],
            gds * vd + gm * v[1] - (gds + gm + gmb) * vs + gmb * v[3],
        ))
    }
    /// The threshold `von` (device frame) at bulk voltage `vb` (`vbs`, or
    /// `vbd` in reverse mode), as `mos1load.c` stores it for the next load's
    /// `DEVfetlim`: `type * vto + gamma * (sarg - sqrt(phi))`.
    fn threshold(&self, vb: Real) -> Real {
        let root = self.phi.sqrt();
        let sarg = if vb <= 0. {
            (self.phi - vb).sqrt()
        } else {
            (root - vb / (2. * root)).max(0.)
        };
        self.pol * self.vto + self.gamma * (sarg - root)
    }
    /// `mos1load.c`'s `[vbs, vgs, vds]` (device frame) for this load:
    /// `MODEINITJCT` starts at `vbs = -1`, `vgs = type * vto`, `vds = 0`;
    /// later loads limit the gate voltage with `DEVfetlim` (against the
    /// previous `von`, through `vgs` or `vgd` as the previous `vds` was
    /// forward or reverse), `vds` with `DEVlimvds` and the forward-biased
    /// bulk junction with `DEVpnjlim` (`vt = kT/q`, `vcrit` from `m * is`).
    fn limit_voltages(
        &self,
        limiter: &mut Limiter,
        states: &crate::DeviceState<'_>,
        raw: [Real; 3],
        vt: Real,
    ) -> [Real; 3] {
        if limiter.mode() == Linearization::Initial {
            return [-1., self.pol * self.vto, 0.];
        }
        let previous: Vec<_> = (0..4)
            .map(|index| limiter.previous(states, MOS1_LIMITED_SLOTS + index))
            .collect();
        let [Some(vbs_old), Some(vgs_old), Some(vds_old), Some(von)] = previous[..] else {
            return raw;
        };
        let [mut vbs, mut vgs, mut vds] = raw;
        let vgd = vgs - vds;
        if vds_old >= 0. {
            vgs = limiter.fet_gate(vgs, vgs_old, von);
            vds = limiter.drain_source(vgs - vgd, vds_old);
        } else {
            let vgd = limiter.fet_gate(vgd, vgs_old - vds_old, von);
            vds = -limiter.drain_source(-(vgs - vgd), -vds_old);
            vgs = vgd + vds;
        }
        let vcrit = limiting::critical_voltage(vt, self.is);
        if vds >= 0. {
            vbs = limiter.pn_junction(vbs, Some(vbs_old), vt, vcrit);
        } else {
            let vbd = limiter.pn_junction(vbs - vds, Some(vbs_old - vds_old), vt, vcrit);
            vbs = vbd + vds;
        }
        [vbs, vgs, vds]
    }
    fn junction(&self, v: Real, c: Real, vt: Real, gmin: Real) -> SpiceResult<JunctionPoint> {
        // MOS1 uses a constant reverse saturation current below -3*Vt,
        // unlike the diode/BJT cubic reverse continuation.
        let (i, g) = if self.pol * v <= -3. * vt {
            (-self.is, 0.)
        } else {
            junction_current(self.pol * v, vt, self.is)?
        };
        let mut q = charge_point(v, (c, self.pb, self.mj, self.fc), (0., i, g), self.pol);
        q.current = self.pol * i + gmin * v;
        q.conductance = g + gmin;
        q.validate()?;
        Ok(q)
    }
}
impl Device for Mos1 {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'm'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.nodes
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn state_count(&self) -> usize {
        // Five charge-derivative pairs, then the limited vbs, vgs, vds and von.
        14
    }
    fn truncation_slots(&self) -> Vec<usize> {
        vec![0, 2, 4, 6, 8]
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("MOS1 AC needs small-signal assembly"));
        }
        let v = self.nodes.map(|n| context.node_voltage(n));
        let vt = nominal(&context.model_context(), self.temp, self.tnom)?;
        let raw = [v[3] - v[2], v[1] - v[2], v[0] - v[2]].map(|x| self.pol * x);
        let mut limiter = Limiter::new(&context.states);
        let [vbs, vgs, vds] = self.limit_voltages(&mut limiter, &context.states, raw, vt);
        // The channel at the limited voltages, relative to the source terminal.
        let local = [vds, vgs, 0., vbs].map(|x| self.pol * x);
        let (ports, i, partials, linearized) = self.channel(local)?;
        stamp_current(context, ports, i, &partials, linearized)?;
        let von = self.threshold(if vds >= 0. { vbs } else { vbs - vds });
        let [d, g, s, b] = self.nodes;
        for (slot, node, c, voltage) in [
            (0, d, self.cbd, self.pol * (vbs - vds)),
            (2, s, self.cbs, self.pol * vbs),
        ] {
            stamp_junction(
                context,
                [b, node],
                voltage,
                self.junction(voltage, c, vt, context.gmin)?,
                slot,
            )?;
        }
        for (slot, node, c) in [
            (4, s, self.overlap[0]),
            (6, d, self.overlap[1]),
            (8, b, self.overlap[2]),
        ] {
            let voltage = context.node_voltage(g) - context.node_voltage(node);
            stamp_junction(
                context,
                [g, node],
                voltage,
                JunctionPoint {
                    current: 0.,
                    conductance: 0.,
                    charge: c * voltage,
                    capacitance: c,
                },
                slot,
            )?;
        }
        limiter.finish(
            &mut context.states,
            &[
                (MOS1_LIMITED_SLOTS, vbs),
                (MOS1_LIMITED_SLOTS + 1, vgs),
                (MOS1_LIMITED_SLOTS + 2, vds),
                (MOS1_LIMITED_SLOTS + 3, von),
            ],
        )
    }
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let v = self.nodes.map(|n| bias_voltage(context, bias, n));
        let (ports, _, partials, _) = self.channel(v)?;
        linear_current(context, ports, &partials)?;
        let vt = nominal(context.model_context, self.temp, self.tnom)?;
        let [d, g, s, b] = self.nodes;
        for (node, c) in [(d, self.cbd), (s, self.cbs)] {
            let voltage = bias_voltage(context, bias, b) - bias_voltage(context, bias, node);
            let q = self.junction(voltage, c, vt, context.model_context.gmin)?;
            context.nodal([b, node], q.conductance, false)?;
            context.nodal([b, node], q.capacitance, true)?;
        }
        for (node, c) in [
            (s, self.overlap[0]),
            (d, self.overlap[1]),
            (b, self.overlap[2]),
        ] {
            context.nodal([g, node], c, true)?;
        }
        Ok(())
    }
}
