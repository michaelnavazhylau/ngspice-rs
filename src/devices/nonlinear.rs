//! Bounded junction equations and charge-aware trial stamping.
//!
//! C references: `dio/dioload.c`, `diosetup.c`, `diotemp.c`, `NIintegrate`.
//! No breakdown, sidewall, tunneling, recombination or self-heating physics is
//! inferred from syntax: unsupported setters fail before node interning.
use crate::devices::schema::{
    ScalarDomain as Domain, ScalarParameter as Parameter, ScalarSchema, ScalarUnit as Unit,
    ScalarValues,
};
use crate::devices::{Device, LinearContext, ModelContext, ResolvedModel, StampContext};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

pub(crate) const K_OVER_Q: Real = 1.38064852e-23 / 1.6021766208e-19; // ngspice CONSTboltz/CHARGE
const MODEL: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        Parameter {
            name: "is",
            unit: Unit::Ampere,
            domain: Domain::Positive,
            default: Some(1e-14),
        },
        Parameter {
            name: "n",
            unit: Unit::Dimensionless,
            domain: Domain::Positive,
            default: Some(1.),
        },
        Parameter {
            name: "rs",
            unit: Unit::Ohm,
            domain: Domain::NonNegative,
            default: Some(0.),
        },
        Parameter {
            name: "tnom",
            unit: Unit::Celsius,
            domain: Domain::Temperature,
            default: None,
        },
        Parameter {
            name: "cjo",
            unit: Unit::Farad,
            domain: Domain::NonNegative,
            default: Some(0.),
        },
        Parameter {
            name: "vj",
            unit: Unit::Volt,
            domain: Domain::Positive,
            default: Some(1.),
        },
        Parameter {
            name: "m",
            unit: Unit::Dimensionless,
            domain: Domain::NonNegative,
            default: Some(0.5),
        },
        Parameter {
            name: "fc",
            unit: Unit::Dimensionless,
            domain: Domain::NonNegative,
            default: Some(0.5),
        },
        Parameter {
            name: "tt",
            unit: Unit::Second,
            domain: Domain::NonNegative,
            default: Some(0.),
        },
    ],
};
const INSTANCE: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        Parameter {
            name: "area",
            unit: Unit::Dimensionless,
            domain: Domain::Positive,
            default: Some(1.),
        },
        Parameter {
            name: "m",
            unit: Unit::Dimensionless,
            domain: Domain::Positive,
            default: Some(1.),
        },
        Parameter {
            name: "temp",
            unit: Unit::Celsius,
            domain: Domain::Temperature,
            default: None,
        },
    ],
};
pub(crate) fn value(values: &ScalarValues, name: &str) -> SpiceResult<Real> {
    values
        .get(name)
        .map(|v| v.value)
        .ok_or_else(|| SpiceError::circuit(format!("missing nonlinear schema default {name}")))
}

/// A level-1 diode with an optional internal anode for series resistance.
/// Only the explicitly enumerated diode model/instance schema is supported.
#[derive(Debug)]
pub struct Diode {
    name: String,
    terminals: Vec<NodeId>,
    junction: [NodeId; 2],
    parameters: DiodeParameters,
}
/// Typed validated nonlinear diode parameters. Quantities are at nominal
/// temperature and are reevaluated per run without cumulative adjustments.
#[derive(Debug, Clone, Copy)]
struct DiodeParameters {
    is: Real,
    n: Real,
    rs: Real,
    cjo: Real,
    vj: Real,
    grading: Real,
    fc: Real,
    tt: Real,
    scale: Real,
    temperature: Option<Real>,
    nominal: Option<Real>,
}
impl Diode {
    pub(crate) fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if instance.nodes.len() != 2 {
            return Err(SpiceError::circuit("diode needs two terminals"));
        }
        let m = model.parameters(&MODEL)?;
        let i = INSTANCE.validate(&instance.parameters, &instance.location)?;
        let p = DiodeParameters {
            is: value(&m, "is")?,
            n: value(&m, "n")?,
            rs: value(&m, "rs")?,
            cjo: value(&m, "cjo")?,
            vj: value(&m, "vj")?,
            grading: value(&m, "m")?,
            fc: value(&m, "fc")?,
            tt: value(&m, "tt")?,
            scale: value(&i, "area")? * value(&i, "m")?,
            temperature: i.get("temp").map(|v| v.value),
            nominal: m.get("tnom").map(|v| v.value),
        };
        if p.grading >= 1. || p.fc >= 1. || !p.scale.is_finite() {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "diode requires 0 <= M,FC < 1 and finite area*m",
            ));
        }
        p.evaluate(0., context)?; // validate derivations before node interning
        let mut staged = nodes.clone();
        let external = [
            staged.intern(&instance.nodes[0]),
            staged.intern(&instance.nodes[1]),
        ];
        let mut terminals = external.to_vec();
        let positive = if p.rs > 0. {
            let name = format!("{}#anode", instance.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit("diode internal-node name collision"));
            }
            let prime = staged.intern(&name);
            staged.set_kind(prime, NodeKind::Internal);
            terminals.push(prime);
            prime
        } else {
            external[0]
        };
        *nodes = staged;
        Ok(Box::new(Self {
            name: instance.name.clone(),
            terminals,
            junction: [positive, external[1]],
            parameters: p,
        }))
    }
}
impl DiodeParameters {
    fn evaluate(self, voltage: Real, context: &ModelContext) -> SpiceResult<JunctionPoint> {
        let temperature = self.temperature.unwrap_or(context.temperature) + 273.15;
        let nominal = self.nominal.unwrap_or(context.nominal_temperature) + 273.15;
        if !temperature.is_finite() || temperature <= 0. || !nominal.is_finite() || nominal <= 0. {
            return Err(SpiceError::circuit("invalid diode temperature"));
        }
        // Non-nominal depletion-capacitance temperature laws are not implemented.
        if self.cjo > 0. && temperature != nominal {
            return Err(SpiceError::Unsupported {
                feature: "diode depletion-charge temperature adjustment".into(),
                location: None,
            });
        }
        let vt = self.n * K_OVER_Q * temperature;
        let is = self.is
            * self.scale
            * (((temperature / nominal - 1.) * 1.11 / vt)
                + 3. / self.n * (temperature / nominal).ln())
            .exp();
        let (current, conductance) = junction_current(voltage, vt, is)?;
        let (charge, capacitance) = depletion_charge(
            voltage,
            self.cjo * self.scale,
            self.vj,
            self.grading,
            self.fc,
        );
        let point = JunctionPoint {
            current: current + context.gmin * voltage,
            conductance: conductance + context.gmin,
            charge: charge + self.tt * current,
            capacitance: capacitance + self.tt * conductance,
        };
        point.validate()?;
        Ok(point)
    }
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct JunctionPoint {
    pub current: Real,
    pub conductance: Real,
    pub charge: Real,
    pub capacitance: Real,
}
impl JunctionPoint {
    pub(crate) fn validate(self) -> SpiceResult<()> {
        if [
            self.current,
            self.conductance,
            self.charge,
            self.capacitance,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || self.capacitance < 0.
        {
            return Err(SpiceError::Numerical {
                context: "junction".into(),
                message: "nonfinite/out-of-domain junction equations".into(),
            });
        }
        Ok(())
    }
}
/// The C reverse-bias continuation joins the exponential at -3*Vt.
pub(crate) fn junction_current(v: Real, vt: Real, is: Real) -> SpiceResult<(Real, Real)> {
    let result = if v >= -3. * vt {
        let exp = (v / vt).exp();
        (is * (exp - 1.), is * exp / vt)
    } else {
        let arg = (3. * vt / (v * std::f64::consts::E)).powi(3);
        (-is * (1. + arg), is * 3. * arg / v)
    };
    if [is, vt, result.0, result.1].iter().any(|v| !v.is_finite()) || vt <= 0. || is <= 0. {
        return Err(SpiceError::Numerical {
            context: "junction".into(),
            message: "junction exponential/temperature overflow".into(),
        });
    }
    Ok(result)
}
pub(crate) fn depletion_charge(v: Real, c: Real, p: Real, m: Real, fc: Real) -> (Real, Real) {
    let boundary = fc * p;
    let at = |v: Real| {
        let arg = 1. - v / p;
        (c * p * (1. - arg.powf(1. - m)) / (1. - m), c * arg.powf(-m))
    };
    if v < boundary {
        at(v)
    } else {
        let (q0, c0) = at(boundary);
        let dc = c0 * m / (p - boundary);
        let delta = v - boundary;
        (q0 + c0 * delta + 0.5 * dc * delta * delta, c0 + dc * delta)
    }
}
pub(crate) fn stamp_junction(
    context: &mut StampContext<'_>,
    nodes: [NodeId; 2],
    v: Real,
    p: JunctionPoint,
    slot: usize,
) -> SpiceResult<()> {
    let mut conductance = p.conductance;
    let mut equivalent = p.current - p.conductance * v;
    if let Some(coefficients) = context.integration {
        let crate::devices::AnalysisMode::Transient { dt, .. } = context.mode else {
            return Err(SpiceError::circuit("junction companion outside transient"));
        };
        if dt != coefficients.dt() {
            return Err(SpiceError::circuit("junction timestep mismatch"));
        }
        let mut history = vec![p.charge];
        for age in 1..=coefficients.charge_history_len() {
            history.push(
                context
                    .states
                    .accepted(age, slot)
                    .ok_or_else(|| SpiceError::circuit("missing accepted junction charge"))?,
            );
        }
        let previous = if coefficients.needs_previous_derivative() {
            Some(
                context
                    .states
                    .accepted(1, slot + 1)
                    .ok_or_else(|| SpiceError::circuit("missing accepted junction derivative"))?,
            )
        } else {
            None
        };
        let companion = coefficients.integrate(&history, previous, p.capacitance)?;
        conductance += companion.conductance;
        // For nonlinear Q, integrate()'s current is dQ/dt-a0*Q.
        // Linearize Q around actual v: dq/dt - a0*(dQ/dv)*v.
        equivalent += companion.derivative - companion.conductance * v;
        context.states.set(slot + 1, companion.derivative)?;
    } else {
        if context.mode.is_transient() {
            return Err(SpiceError::circuit(
                "junction transient needs companion integration",
            ));
        }
        context.states.set(slot + 1, 0.)?;
    }
    context.states.set(slot, p.charge)?;
    crate::devices::linear::nodal_stamp(context.matrix, context.unknowns, nodes, conductance)?;
    context.stamp_rhs(nodes[0], -equivalent)?;
    context.stamp_rhs(nodes[1], equivalent)
}
impl Device for Diode {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'd'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn state_count(&self) -> usize {
        2
    }
    fn truncation_slot(&self) -> Option<usize> {
        Some(0)
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("diode AC needs small-signal assembly"));
        }
        let v = context.node_voltage(self.junction[0]) - context.node_voltage(self.junction[1]);
        let p = self.parameters.evaluate(v, &context.model_context())?;
        if self.parameters.rs > 0. {
            crate::devices::linear::nodal_stamp(
                context.matrix,
                context.unknowns,
                [self.terminals[0], self.junction[0]],
                self.parameters.scale / self.parameters.rs,
            )?;
        }
        stamp_junction(context, self.junction, v, p, 0)
    }
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let v = |node| {
            context
                .unknowns
                .node_row(node)
                .and_then(|r| bias.get(r))
                .unwrap_or(0.)
        };
        let p = self.parameters.evaluate(
            v(self.junction[0]) - v(self.junction[1]),
            context.model_context,
        )?;
        if self.parameters.rs > 0. {
            context.nodal(
                [self.terminals[0], self.junction[0]],
                self.parameters.scale / self.parameters.rs,
                false,
            )?;
        }
        context.nodal(self.junction, p.conductance, false)?;
        context.nodal(self.junction, p.capacitance, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn charge_derivative_and_current_jacobian_are_consistent() {
        for v in [-1., -0.08, -0.02, 0., 0.49, 0.5, 0.51, 0.7] {
            let h = 1e-7;
            let (q, c) = depletion_charge(v, 2e-12, 1., 0.5, 0.5);
            let derivative = (depletion_charge(v + h, 2e-12, 1., 0.5, 0.5).0
                - depletion_charge(v - h, 2e-12, 1., 0.5, 0.5).0)
                / (2. * h);
            assert!(
                (derivative - c).abs() < 1e-8 * c.abs(),
                "{v}: {q} {derivative} {c}"
            );
            let (i, g) = junction_current(v, 0.026, 1e-14).unwrap();
            let derivative = (junction_current(v + h, 0.026, 1e-14).unwrap().0
                - junction_current(v - h, 0.026, 1e-14).unwrap().0)
                / (2. * h);
            assert!(
                (derivative - g).abs() < 1e-7 * g.abs() + 4. * f64::EPSILON * 1e-14 / h,
                "{v}: {i} {derivative} {g}"
            );
        }
    }
}
