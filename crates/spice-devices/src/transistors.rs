//! Bounded Ebers-Moll BJT equations (MOS1 lives in [`crate::mos1`]).
//! C: `bjt/bjtload.c`, `bjtsetup.c`.
//! Advanced charge, temperature, series-node and high-injection parameters
//! are explicit errors, not ignored setters.
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
/// entering `output[1]`. `partials` are actual physical voltage derivatives.
fn stamp_current(
    context: &mut StampContext<'_>,
    output: [NodeId; 2],
    current: Real,
    partials: &[(NodeId, Real)],
) -> SpiceResult<()> {
    let mut equivalent = current;
    for (node, derivative) in partials {
        equivalent -= derivative * context.node_voltage(*node);
    }
    for (row, sign) in [(output[0], 1.), (output[1], -1.)] {
        for (col, derivative) in partials {
            context.stamp(row, *col, sign * derivative)?;
        }
        context.stamp_rhs(row, -sign * equivalent)?;
    }
    Ok(())
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
        4
    }
    fn truncation_slots(&self) -> Vec<usize> {
        vec![0, 2]
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("BJT AC requires small-signal assembly"));
        }
        let (c, b, e) = (self.nodes[0], self.nodes[1], self.nodes[2]);
        let vbe = context.node_voltage(b) - context.node_voltage(e);
        let vbc = context.node_voltage(b) - context.node_voltage(c);
        let [(ibe, gbe, qbe), (ibc, gbc, qbc)] = self.points(vbe, vbc, &context.model_context())?;
        let gmin = self.multiplier * context.gmin;
        stamp_current(
            context,
            [c, e],
            ibe - ibc,
            &[(b, gbe - gbc), (e, -gbe), (c, gbc)],
        )?;
        stamp_current(
            context,
            [b, e],
            ibe / self.bf,
            &[(b, gbe / self.bf), (e, -gbe / self.bf)],
        )?;
        stamp_current(
            context,
            [b, c],
            ibc / self.br,
            &[(b, gbc / self.br), (c, -gbc / self.br)],
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
        let [substrate, connection] = self.substrate();
        let v = context.node_voltage(connection) - context.node_voltage(substrate);
        stamp_current(
            context,
            [connection, substrate],
            gmin * v,
            &[(connection, gmin), (substrate, -gmin)],
        )?;
        Ok(())
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
