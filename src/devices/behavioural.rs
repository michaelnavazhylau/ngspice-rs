//! Behavioural (arbitrary) sources: `Bname n+ n- v=expr | i=expr`, plus the
//! XSPICE `pwl`/`spice2poly` instances the front end generates for E/G/F/H
//! `TABLE` and `POLY` forms ([`crate::netlist::behavioural`]).
//!
//! C references (behaviour only): `src/spicelib/devices/asrc/` (`asrcset.c`,
//! `asrcload.c`, `asrcacld.c`, `asrcpar.c`, `asrctemp.c`, `asrcfbr.c`) and the
//! expression machinery of `src/spicelib/parser/` ([`program`]).
//!
//! # Equations
//!
//! With `f(x)` the expression over its quantities `x` (node voltages and
//! branch currents), `g = df/dx` at the present iterate `x0` and the
//! instance factor `k` (below), a load linearises `f` like `ASRCload`:
//!
//! | Output | Stamps (`A x = b`, ground rows/columns dropped) |
//! | --- | --- |
//! | `v=` (branch row `r`) | `A[n+][r] += 1`, `A[n-][r] -= 1`, `A[r][n+] += 1`, `A[r][n-] -= 1`, `A[r][x_i] -= k g_i`, `b[r] += k (f - g.x0)` |
//! | `i=` | `A[n+][x_i] += k g_i`, `A[n-][x_i] -= k g_i`, `b[n+] -= k (f - g.x0)`, `b[n-] += k (f - g.x0)` |
//!
//! so a voltage source enforces `v(n+) - v(n-) = k f(x)` and a current source
//! drives `k f(x)` from `n+` through itself to `n-` (the signs of V and I
//! sources). AC uses the same Jacobian at the bias point and no RHS
//! (`ASRCacLoad`). The factor is `k = m (1 + tc1 d + tc2 d^2)` with
//! `d = T + dtemp - 300.15 K` (`T` the instance `temp=` or the circuit
//! temperature); `reciproctc=1` inverts the temperature factor and
//! `reciprocm=1` divides by `m`.
//!
//! `time` is the transient time (0 in OP/DC and in the AC linearisation),
//! `temper` the circuit temperature and `hertz` the AC frequency (0 outside
//! AC; an AC analysis of a circuit using it re-solves the operating point at
//! every frequency, as `acan.c` does for `CKTvarHertz`). ngspice sets no breakpoints for B
//! sources, and neither does the port: a time-dependent expression is
//! followed by the ordinary truncation-error step control.
//!
//! A voltage B source's branch current may be sensed by `i(bname)` (C
//! `ASRCfindBr`). C would also invent an unstamped branch for a current B
//! source; the port reports that reference as not findable instead.

pub mod program;
pub mod xspice;

use crate::maths::{SparseMatrix, Vector};
use crate::netlist::ast::{DeviceInstance, ParameterKind};
use crate::netlist::bexpr::BehaviouralExpression;
use crate::primitives::{
    NodeId, NodeTable, Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number,
};

use crate::devices::linear::LinearContext;
use crate::devices::traits::{AnalysisMode, ControlReference, Device, MnaUnknowns, StampContext};
use program::{Environment, Evaluation, Program, Quantity};

/// C reference used in diagnostics.
pub const C_REFERENCE: &str = "src/spicelib/devices/asrc/ (asrcload.c, asrcacld.c, asrcpar.c), \
     src/spicelib/parser/inpptree.c, ptfuncs.c";

/// Whether the source imposes a voltage or drives a current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviouralOutput {
    /// `v=`: a voltage source with a branch-current unknown.
    Voltage,
    /// `i=`: a current source.
    Current,
}

/// The instance setters of `ASRCpTable` that scale the output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BehaviouralScale {
    /// Multiplier `m` (default 1).
    pub m: Real,
    /// First-order temperature coefficient (1/K).
    pub tc1: Real,
    /// Second-order temperature coefficient (1/K^2).
    pub tc2: Real,
    /// Instance temperature in degrees Celsius, if given.
    pub temperature: Option<Real>,
    /// Temperature offset from the circuit (K), used without `temp=`.
    pub dtemp: Real,
    /// `reciproctc=1`: divide by the temperature factor instead.
    pub reciprocal_tc: bool,
    /// `reciprocm=1`: divide by `m` instead.
    pub reciprocal_m: bool,
}

impl Default for BehaviouralScale {
    fn default() -> Self {
        Self {
            m: 1.,
            tc1: 0.,
            tc2: 0.,
            temperature: None,
            dtemp: 0.,
            reciprocal_tc: false,
            reciprocal_m: false,
        }
    }
}

impl BehaviouralScale {
    /// The output factor at circuit temperature `circuit` (degrees Celsius),
    /// as `ASRCload` computes it.
    #[must_use]
    pub fn factor(&self, circuit: Real) -> Real {
        let kelvin = self.temperature.unwrap_or(circuit) + 273.15;
        let difference = kelvin + self.dtemp - 300.15;
        let mut factor = 1. + self.tc1 * difference + self.tc2 * difference * difference;
        if self.reciprocal_tc {
            factor = 1. / factor;
        }
        if self.reciprocal_m {
            factor / self.m
        } else {
            factor * self.m
        }
    }
}

/// One variable of the compiled expression, bound to the circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variable {
    Node(NodeId),
    /// Index into [`Device::controlling_sources`].
    Branch(usize),
}

/// A behavioural source. See the [module documentation](self).
#[derive(Debug, Clone)]
pub struct Behavioural {
    name: String,
    designator: char,
    output: BehaviouralOutput,
    /// `[n+, n-]` followed by every other node the expression reads.
    terminals: Vec<NodeId>,
    variables: Vec<Variable>,
    controls: Vec<ControlReference>,
    program: Program,
    scale: BehaviouralScale,
}

impl Behavioural {
    /// Which output this source has.
    #[must_use]
    pub const fn output(&self) -> BehaviouralOutput {
        self.output
    }

    /// The output scaling setters.
    #[must_use]
    pub const fn scale(&self) -> &BehaviouralScale {
        &self.scale
    }

    /// The compiled expression.
    #[must_use]
    pub const fn program(&self) -> &Program {
        &self.program
    }

    /// Evaluates the expression with the variables read from `solution`.
    fn evaluate(
        &self,
        solution: &Vector,
        unknowns: &MnaUnknowns,
        controls: &[usize],
        time: Real,
        context: &crate::devices::ModelContext,
    ) -> SpiceResult<(Evaluation, Vec<Real>, Vec<Option<usize>>)> {
        let columns = self.columns(unknowns, controls)?;
        let values: Vec<Real> = columns
            .iter()
            .map(|column| column.and_then(|row| solution.get(row)).unwrap_or(0.))
            .collect();
        let evaluation = self
            .program
            .evaluate(&Environment {
                values: &values,
                time,
                temperature: context.temperature,
                gmin: context.gmin,
                frequency: context.frequency,
            })
            .map_err(|error| match error {
                SpiceError::Numerical { message, .. } => SpiceError::Numerical {
                    context: format!("behavioural source {}", self.name),
                    message,
                },
                other => other,
            })?;
        Ok((evaluation, values, columns))
    }

    /// The matrix column of every variable (`None` for ground).
    fn columns(
        &self,
        unknowns: &MnaUnknowns,
        controls: &[usize],
    ) -> SpiceResult<Vec<Option<usize>>> {
        self.variables
            .iter()
            .map(|variable| match variable {
                Variable::Node(node) => Ok(unknowns.node_row(*node)),
                Variable::Branch(index) => {
                    controls.get(*index).copied().map(Some).ok_or_else(|| {
                        SpiceError::circuit(format!(
                            "{}: controlling branch {} is not bound",
                            self.name, self.controls[*index].name
                        ))
                    })
                }
            })
            .collect()
    }

    /// Stamps the linearised output. `rhs` is `None` for the AC Jacobian.
    #[allow(clippy::too_many_arguments)]
    fn stamp_into(
        &self,
        matrix: &mut SparseMatrix,
        rhs: Option<&mut Vector>,
        unknowns: &MnaUnknowns,
        branch: Option<usize>,
        evaluation: &Evaluation,
        values: &[Real],
        columns: &[Option<usize>],
        factor: Real,
    ) -> SpiceResult<()> {
        let row = |node: NodeId| unknowns.node_row(node);
        let (p, n) = (row(self.terminals[0]), row(self.terminals[1]));
        let mut add = |r: Option<usize>, c: Option<usize>, value: Real| match (r, c) {
            (Some(r), Some(c)) => matrix.add(r, c, value),
            _ => Ok(()),
        };
        let mut equivalent = evaluation.value;
        for (gradient, value) in evaluation.gradient.iter().zip(values) {
            equivalent -= value * gradient;
        }
        let equivalent = factor * equivalent;
        match self.output {
            BehaviouralOutput::Voltage => {
                let k = Some(branch.ok_or_else(|| {
                    SpiceError::circuit(format!("{}: missing branch row", self.name))
                })?);
                add(p, k, 1.)?;
                add(n, k, -1.)?;
                add(k, n, -1.)?;
                add(k, p, 1.)?;
                for (gradient, column) in evaluation.gradient.iter().zip(columns) {
                    add(k, *column, -gradient * factor)?;
                }
                if let (Some(rhs), Some(k)) = (rhs, k) {
                    rhs.add_to(k, equivalent)?;
                }
            }
            BehaviouralOutput::Current => {
                for (gradient, column) in evaluation.gradient.iter().zip(columns) {
                    add(p, *column, gradient * factor)?;
                    add(n, *column, -gradient * factor)?;
                }
                if let Some(rhs) = rhs {
                    if let Some(p) = p {
                        rhs.add_to(p, -equivalent)?;
                    }
                    if let Some(n) = n {
                        rhs.add_to(n, equivalent)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn checked_factor(&self, temperature: Real) -> SpiceResult<Real> {
        let factor = self.scale.factor(temperature);
        if factor.is_finite() {
            Ok(factor)
        } else {
            Err(SpiceError::Numerical {
                context: format!("behavioural source {}", self.name),
                message: format!("non-finite output factor (m/tc1/tc2) {factor}"),
            })
        }
    }
}

impl Device for Behavioural {
    /// Noiseless: C gives B sources no noise routine (`DEVnoise = NULL`,
    /// `src/spicelib/devices/asrc/asrcinit.c`), and the XSPICE `spice2poly`/
    /// `pwl` code models that TABLE/POLY lower to declare none of the noise
    /// parameters `MIFnoise` looks for (`src/xspice/mif/mifnoise.c`).
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> crate::primitives::SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        self.designator
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn branch_currents(&self) -> usize {
        usize::from(self.output == BehaviouralOutput::Voltage)
    }

    fn controlling_sources(&self) -> &[ControlReference] {
        &self.controls
    }

    /// `ASRCfindBr`: a voltage B source's current can be sensed.
    fn findable_branch(&self) -> Option<usize> {
        (self.output == BehaviouralOutput::Voltage).then_some(0)
    }

    fn is_nonlinear(&self) -> bool {
        true
    }

    /// `ASRCload` applies no limiting.
    fn limits_voltage_steps(&self) -> bool {
        false
    }

    /// `hertz` makes the DC equations frequency dependent (C `CKTvarHertz`).
    fn depends_on_frequency(&self) -> bool {
        self.program.uses_frequency()
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let time = match context.mode {
            AnalysisMode::Transient { time, .. } => time,
            AnalysisMode::OperatingPoint | AnalysisMode::DcSweep => 0.,
            AnalysisMode::Ac { .. } => {
                return Err(SpiceError::circuit(format!(
                    "{}: behavioural AC needs small-signal assembly",
                    self.name
                )));
            }
        };
        let (evaluation, values, columns) = self.evaluate(
            context.solution,
            context.unknowns,
            context.controls,
            time,
            &context.model_context(),
        )?;
        let factor = self.checked_factor(context.temperature)?;
        let branch = (!context.branches.is_empty()).then_some(context.branches.start);
        self.stamp_into(
            context.matrix,
            Some(context.rhs),
            context.unknowns,
            branch,
            &evaluation,
            &values,
            &columns,
            factor,
        )
    }

    /// The bias-point Jacobian (`ASRCacLoad`): real, frequency independent.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let temperature = context.model_context.temperature;
        let (evaluation, values, columns) = self.evaluate(
            bias,
            context.unknowns,
            context.controls,
            0.,
            context.model_context,
        )?;
        let factor = self.checked_factor(temperature)?;
        self.stamp_into(
            &mut context.system.a,
            None,
            context.unknowns,
            context.branch,
            &evaluation,
            &values,
            &columns,
            factor,
        )
    }
}

fn unsupported(location: &SourceLoc, feature: impl Into<String>) -> SpiceError {
    SpiceError::Unsupported {
        feature: feature.into(),
        location: Some(location.clone()),
    }
}

fn literal(text: &str, location: &SourceLoc, what: &str) -> SpiceResult<Real> {
    parse_spice_number(text)
        .filter(|value| value.is_finite())
        .ok_or_else(|| unsupported(location, format!("non-finite or nonliteral {what}={text}")))
}

/// Builds a B source (or a front-end generated XSPICE `a` instance) from its
/// literalized AST instance, binding nodes in `nodes` only on success.
pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
) -> SpiceResult<Box<dyn Device>> {
    let location = &instance.location;
    if instance.model.is_some() || instance.nodes.len() != 2 {
        return Err(unsupported(
            location,
            format!("{} needs two nodes and no model", instance.name),
        ));
    }
    let mut expression: Option<(BehaviouralOutput, &BehaviouralExpression)> = None;
    let mut scale = BehaviouralScale::default();
    let mut temperature_given = false;
    let mut dtemp_given = false;
    for parameter in &instance.parameters {
        let at = &parameter.location;
        match (&parameter.kind, parameter.name.as_str()) {
            (ParameterKind::Behavioural(tree), "v" | "i") => {
                if expression.is_some() {
                    return Err(unsupported(
                        at,
                        format!("{}: more than one v=/i= expression", instance.name),
                    ));
                }
                let output = if parameter.name == "v" {
                    BehaviouralOutput::Voltage
                } else {
                    BehaviouralOutput::Current
                };
                expression = Some((output, tree));
            }
            (ParameterKind::Scalar, name) => {
                let value = literal(&parameter.value, at, name)?;
                match name {
                    "m" => scale.m = value,
                    "tc1" => scale.tc1 = value,
                    "tc2" => scale.tc2 = value,
                    "temp" => {
                        scale.temperature = Some(value);
                        temperature_given = true;
                    }
                    "dtemp" => {
                        scale.dtemp = value;
                        dtemp_given = true;
                    }
                    // IF_INTEGER flags: only the value 1 selects the reciprocal.
                    "reciproctc" => scale.reciprocal_tc = value.round() == 1.,
                    "reciprocm" => scale.reciprocal_m = value.round() == 1.,
                    other => {
                        return Err(unsupported(
                            at,
                            format!("{} parameter {other}", instance.name),
                        ));
                    }
                }
            }
            (ParameterKind::Expression(_), name) => {
                return Err(unsupported(
                    at,
                    format!(
                        "non-literal {} parameter {name}={} (expressions must be literalized first)",
                        instance.name, parameter.value
                    ),
                ));
            }
            (_, name) => {
                return Err(unsupported(
                    at,
                    format!("{} parameter {name}", instance.name),
                ));
            }
        }
    }
    if temperature_given && dtemp_given {
        // ASRCtemp ignores dtemp (with a message) when temp is given.
        return Err(unsupported(
            location,
            format!(
                "{}: both temp= and dtemp= (C ignores dtemp with a message); give one",
                instance.name
            ),
        ));
    }
    let Some((output, tree)) = expression else {
        return Err(SpiceError::parse(
            location.clone(),
            format!(
                "{}: a B source needs v=expression or i=expression",
                instance.name
            ),
        ));
    };
    let program = Program::compile(&tree.root)?;
    let mut staged = nodes.clone();
    let positive = staged.intern(&instance.nodes[0]);
    let negative = staged.intern(&instance.nodes[1]);
    if output == BehaviouralOutput::Voltage && positive == negative {
        // ASRCsetup: "instance %s is a shorted ASRC".
        return Err(unsupported(
            location,
            format!("instance {} is a shorted ASRC", instance.name),
        ));
    }
    let mut terminals = vec![positive, negative];
    let mut variables = Vec::new();
    let mut controls = Vec::new();
    for quantity in program.quantities() {
        match quantity {
            Quantity::Node(name) => {
                let node = staged.intern(name);
                if !terminals.contains(&node) {
                    terminals.push(node);
                }
                variables.push(Variable::Node(node));
            }
            Quantity::Branch(name) => {
                variables.push(Variable::Branch(controls.len()));
                controls.push(ControlReference {
                    name: name.clone(),
                    location: Some(tree.span.start.clone()),
                });
            }
        }
    }
    *nodes = staged;
    Ok(Box::new(Behavioural {
        name: instance.name.clone(),
        designator: instance.designator,
        output,
        terminals,
        variables,
        controls,
        program,
        scale,
    }))
}
