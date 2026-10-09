//! Typed bounded DC sweeps, including one outer sweep (`dctrcurv.c`).
//! Targets are independent V/I sources, resistors (literal or model-backed,
//! identified by physical device metadata), circuit temperature and the
//! settable instance parameters `@inst[param]` that both C and the port
//! support. Model parameters (C rejects them too) and more than two axes (C
//! has two nesting levels) are explicitly unsupported.
//! Nothing is mutated: sources use temporary RHS offsets, temperature,
//! resistor and instance-parameter values travel in an immutable per-point
//! `ModelContext`. See `docs/port/DC_SWEEPS.md`.
use crate::analysis::linear::{number, plot, unsupported};
use crate::analysis::{AnalysisContext, AnalysisRequest, Plot};
use crate::devices::{Circuit, ModelContext};
use crate::maths::Vector;
use crate::primitives::{AnalysisKind, Complex, Real, SpiceError, SpiceResult};

/// Largest number of grid points, per axis and as a Cartesian product.
pub const MAX_SWEEP_POINTS: usize = 100_000;
/// Relative slack, in steps, within which a grid is taken to reach its stop
/// (`0..0.3 by 0.1` is `2.9999999999999996` steps in binary floating point).
const ENDPOINT_TOLERANCE: Real = 32. * Real::EPSILON;
/// C `CONSTCtoK`: `dctrcurv.c` accumulates a temperature sweep in kelvin.
const CELSIUS_TO_KELVIN: Real = 273.15;

/// A resolved sweep target, not an arbitrary parameter-name string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepTarget {
    /// Independent voltage-source DC value (V).
    VoltageSource(String),
    /// Independent current-source DC value (A).
    CurrentSource(String),
    /// Supplied scalar resistance (ohms) of a literal or model-backed resistor.
    /// Temperature, TC, scale and multiplicity still apply to model-backed
    /// resistors (`crate::devices::sweep`).
    Resistor(String),
    /// Circuit temperature (Celsius); nominal temperature is unchanged.
    Temperature,
    /// A settable real instance parameter, `.dc @instance[parameter]` (C
    /// `dctrcurv.c` `PARAM_CODE`, `DCTfindInstParam`/`DCTsetInstParam`).
    InstanceParameter {
        /// The target as C names it, `@instance[parameter]`, with the
        /// instance's own spelling and the canonical parameter keyword.
        name: String,
        /// The instance name, in the device's own spelling.
        instance: String,
        /// The canonical (lowercase, alias-folded) parameter keyword.
        parameter: String,
        /// How the value reaches the equations.
        route: ParameterRoute,
    },
}
/// How an [`SweepTarget::InstanceParameter`] value reaches the equations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterRoute {
    /// `@v1[dc]`: the voltage-source DC value, as [`SweepTarget::VoltageSource`].
    VoltageSource,
    /// `@i1[dc]` / `@i1[c]`: the current-source DC value, as
    /// [`SweepTarget::CurrentSource`].
    CurrentSource,
    /// `@r1[r]` / `@r1[resistance]`: the supplied resistance, as
    /// [`SweepTarget::Resistor`] (C `RESparam` then `REStemp`).
    Resistor,
    /// Any other supported parameter: a per-point device replacement
    /// ([`crate::devices::Device::with_instance_parameter`]).
    Device,
}
impl SweepTarget {
    /// The plot unit of the swept quantity. An instance parameter has no
    /// common physical unit and is reported as `parameter` (C names its scale
    /// `param-sweep`).
    #[must_use]
    pub fn unit(&self) -> &str {
        match self {
            Self::VoltageSource(_) => "voltage",
            Self::CurrentSource(_) => "current",
            Self::Resistor(_) => "resistance",
            Self::Temperature => "temperature",
            Self::InstanceParameter { .. } => "parameter",
        }
    }
    /// The instance name, `temp`, or `@instance[parameter]`.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::VoltageSource(n) | Self::CurrentSource(n) | Self::Resistor(n) => n,
            Self::Temperature => "temp",
            Self::InstanceParameter { name, .. } => name,
        }
    }
    /// True when the swept value is a supplied resistance.
    fn is_resistance(&self) -> bool {
        matches!(
            self,
            Self::Resistor(_)
                | Self::InstanceParameter {
                    route: ParameterRoute::Resistor,
                    ..
                }
        )
    }
    /// The independent source this target sweeps, if any.
    fn source(&self) -> Option<&str> {
        match self {
            Self::VoltageSource(name) | Self::CurrentSource(name) => Some(name),
            Self::InstanceParameter {
                instance,
                route: ParameterRoute::VoltageSource | ParameterRoute::CurrentSource,
                ..
            } => Some(instance),
            _ => None,
        }
    }
}
/// A finite directional sweep with an inclusive reachable stop and bounded work.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepSpec {
    /// Typed source, resistor or circuit-temperature target.
    pub target: SweepTarget,
    /// First value in the target's physical unit.
    pub start: Real,
    /// Last bound (included only if reached on the grid).
    pub stop: Real,
    /// Nonzero step with the correct direction.
    pub step: Real,
}
impl SweepSpec {
    /// Build a bounded grid `start, start+step, ...`, in the sign of `step`.
    ///
    /// The stop is included when the grid reaches it within rounding (the
    /// last value is then exactly `stop`) and is never overshot. A single
    /// point (`start == stop`) is allowed. Rejects nonfinite inputs, a zero or
    /// wrong-direction step, overflow, more than [`MAX_SWEEP_POINTS`], steps too
    /// small to make progress, temperatures at/below absolute zero and
    /// resistances that are zero or have a nonfinite conductance.
    /// # Errors
    /// [`SpiceError::Unsupported`] for any of the above.
    pub fn grid(&self) -> SpiceResult<Vec<Real>> {
        let (start, stop, step) = (self.start, self.stop, self.step);
        if ![start, stop, step].iter().all(|v| v.is_finite()) || step == 0. {
            return Err(unsupported(
                "DC sweep needs finite start/stop and a nonzero step",
            ));
        }
        let ratio = (stop - start) / step;
        if !ratio.is_finite() {
            return Err(unsupported("DC sweep span overflows"));
        }
        if ratio < 0. {
            return Err(unsupported("DC sweep step points away from its stop"));
        }
        let slack = ENDPOINT_TOLERANCE * ratio.max(1.);
        let steps = (ratio + slack).floor();
        if steps + 1. > MAX_SWEEP_POINTS as Real {
            return Err(unsupported("DC sweep point limit exceeded"));
        }
        let steps = steps as usize;
        let mut values: Vec<Real> = (0..=steps).map(|i| start + (i as Real) * step).collect();
        if steps > 0 && (ratio - steps as Real).abs() <= slack {
            values[steps] = stop;
        }
        self.check_values(&values)?;
        Ok(values)
    }

    /// The values C's `dctrcurv.c` visits: the swept quantity starts at
    /// `start` and is advanced by `value += step`, and the sweep continues
    /// while `sign(step) (value - stop) <= 1e3 DBL_EPSILON`. A temperature is
    /// accumulated in kelvin (`CKTtemp`) and reported in Celsius.
    ///
    /// For steps that are not binary-exact (`0.1`) these differ from
    /// [`Self::grid`] by an ulp or so, which is irrelevant for continuous
    /// devices but decides a switch control that lands exactly on a
    /// threshold, so DC sweeps of circuits with discrete-state devices use
    /// these values. The stop test is C's absolute one, so the last value may
    /// lie up to `1e3 DBL_EPSILON` beyond `stop` as it does in C.
    /// # Errors
    /// Everything [`Self::grid`] rejects, and the same point/progress/domain
    /// limits applied to the accumulated values.
    pub fn accumulated_grid(&self) -> SpiceResult<Vec<Real>> {
        // The same input validation (finite, direction, budget) as the grid.
        self.grid()?;
        let offset = match self.target {
            SweepTarget::Temperature => CELSIUS_TO_KELVIN,
            _ => 0.,
        };
        let tolerance = 1e3 * Real::EPSILON;
        let direction = self.step.signum();
        let mut stored = self.start + offset;
        let mut values = Vec::new();
        loop {
            let value = stored - offset;
            if direction * (value - self.stop) > tolerance {
                break;
            }
            if values.len() >= MAX_SWEEP_POINTS {
                return Err(unsupported("DC sweep point limit exceeded"));
            }
            values.push(value);
            stored += self.step;
        }
        self.check_values(&values)?;
        Ok(values)
    }

    fn check_values(&self, values: &[Real]) -> SpiceResult<()> {
        let step = self.step;
        if values.iter().any(|v| !v.is_finite()) {
            return Err(unsupported("DC sweep overflows"));
        }
        if values
            .windows(2)
            .any(|w| (w[1] - w[0]) * step.signum() <= 0.)
        {
            return Err(unsupported("DC sweep makes no progress"));
        }
        if self.target == SweepTarget::Temperature && values.iter().any(|v| *v <= -273.15) {
            return Err(unsupported("DC temperature below absolute zero"));
        }
        if self.target.is_resistance() && values.iter().any(|v| *v == 0. || !(1. / v).is_finite()) {
            return Err(unsupported(
                "DC resistance must be nonzero with finite conductance",
            ));
        }
        Ok(())
    }
}
/// Split a C instance-parameter target `@instance[parameter]` (`dctrcurv.c`
/// `DCTfindInstParam`) into its nonempty instance and parameter. C ignores
/// anything after the closing bracket; the port rejects it.
fn parameter_target(text: &str) -> SpiceResult<Option<(&str, &str)>> {
    let Some(rest) = text.strip_prefix('@') else {
        return Ok(None);
    };
    let malformed = || {
        unsupported(format!(
            "malformed DC sweep target {text}; expected @instance[parameter]"
        ))
    };
    let (instance, rest) = rest.split_once('[').ok_or_else(malformed)?;
    let parameter = rest.strip_suffix(']').ok_or_else(malformed)?;
    if instance.is_empty() || parameter.is_empty() || parameter.contains(['[', ']']) {
        return Err(malformed());
    }
    Ok(Some((instance, parameter)))
}

/// What one axis written as `target` names before the source table exists:
/// the quantity it sweeps (for duplicate detection) and how it is applied.
enum Written<'a> {
    Temperature,
    /// A plain name: an independent source or a resistor.
    Name(&'a str),
    /// `@instance[r|resistance]` of a resistor.
    Resistance(&'a str),
    /// `@instance[parameter]` that the device replaces itself.
    Device(&'a str, &'static str),
    /// `@instance[parameter]` left for the source table (`dc`, current `c`)
    /// or for an explicit rejection.
    Other(&'a str, &'a str),
}
impl<'a> Written<'a> {
    fn classify(circuit: &Circuit, target: &'a str) -> SpiceResult<Self> {
        if target.eq_ignore_ascii_case("temp") {
            return Ok(Self::Temperature);
        }
        let Some((instance, parameter)) = parameter_target(target)? else {
            return Ok(Self::Name(target));
        };
        if matches!(parameter.to_ascii_lowercase().as_str(), "r" | "resistance")
            && circuit.resistor(instance).is_some()
        {
            return Ok(Self::Resistance(instance));
        }
        if let Some(device) = circuit.device(instance)
            && let Some(canonical) = device.instance_parameter(parameter)
        {
            return Ok(Self::Device(instance, canonical));
        }
        Ok(Self::Other(instance, parameter))
    }
    /// The swept quantity: `r1` and `@r1[r]`, `v1` and `@v1[dc]` are one.
    fn key(&self) -> String {
        match self {
            Self::Temperature => "temp".into(),
            Self::Name(name) | Self::Resistance(name) => name.to_ascii_lowercase(),
            Self::Device(name, parameter) => format!("@{}[{parameter}]", name.to_ascii_lowercase()),
            Self::Other(name, parameter) => match parameter.to_ascii_lowercase().as_str() {
                "dc" | "c" => name.to_ascii_lowercase(),
                other => format!("@{}[{other}]", name.to_ascii_lowercase()),
            },
        }
    }
}

/// Parse and resolve one/two axes against actual independent sources,
/// resistors (by physical metadata, never by instance-name prefix), circuit
/// temperature and `@instance[parameter]` targets.
/// Positional form: `target start stop step [outer start stop step]`; C's
/// `dot_dc` (`inp2dot.c`) reads exactly these two levels (`TRCVNESTLEVEL` is 2)
/// and silently ignores anything after them, which the port rejects.
/// # Errors
/// Missing/unsupported/duplicate targets, invalid grid or Cartesian work
/// budget; [`SpiceError::NotYetPorted`] for an instance parameter C sweeps but
/// the port does not.
pub fn resolve(
    circuit: &Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Vec<SweepSpec>> {
    if request.kind != AnalysisKind::DcSweep {
        return Err(unsupported("typed sweep requires a DC request"));
    }
    let args: Vec<_> = request
        .arguments
        .iter()
        .filter(|a| !a.contains('='))
        .collect();
    if !matches!(args.len(), 4 | 8) {
        return Err(unsupported(
            ".dc requires one or two target/start/stop/step axes",
        ));
    }
    // Resolve the first point's resistor/temperature/parameter settings before
    // assembly: the original resistance may overflow at a swept temperature
    // even though every requested replacement is valid. Physical resistor
    // metadata needs no equation assembly, and source kinds come from the
    // resulting system.
    let mut names = std::collections::BTreeSet::new();
    let mut probe = context.model_context();
    let mut written = Vec::new();
    for axis in args.as_chunks::<4>().0 {
        let target = Written::classify(circuit, axis[0])?;
        if !names.insert(target.key()) {
            return Err(unsupported("duplicate nested DC target"));
        }
        let start = number(Some(axis[1]), "sweep start")?;
        match target {
            Written::Temperature => probe.temperature = start,
            Written::Name(name) | Written::Resistance(name) if circuit.resistor(name).is_some() => {
                probe = probe.with_resistor_override(circuit.resistor_override(name, start)?)?;
            }
            _ => {}
        }
        written.push(target);
    }
    // Instance parameters are validated at the probe temperature.
    for (axis, target) in args.as_chunks::<4>().0.iter().zip(&written) {
        if let Written::Device(name, parameter) = target {
            let start = number(Some(axis[1]), "sweep start")?;
            probe = probe.with_instance_override(
                circuit.instance_override(name, parameter, start, &probe)?,
            )?;
        }
    }
    let system = circuit.small_signal_system(&probe, &Vector::zeros(circuit.unknown_count()))?;
    let source = |name: &str| {
        system
            .sources
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    };
    let instance_name = |index: usize| circuit.devices()[index].name().to_owned();
    let parameter = |instance: String, parameter: &str, route| SweepTarget::InstanceParameter {
        name: format!("@{instance}[{parameter}]"),
        instance,
        parameter: parameter.to_owned(),
        route,
    };
    let mut axes = vec![];
    let mut work = 1usize;
    for (axis, written) in args.as_chunks::<4>().0.iter().zip(written) {
        let target = match written {
            Written::Temperature => SweepTarget::Temperature,
            Written::Name(name) => {
                if let Some(source) = source(name) {
                    match source.kind {
                        crate::devices::SourceKind::Voltage => {
                            SweepTarget::VoltageSource(source.name.clone())
                        }
                        crate::devices::SourceKind::Current => {
                            SweepTarget::CurrentSource(source.name.clone())
                        }
                    }
                } else if let Some((index, _)) = circuit.resistor(name) {
                    SweepTarget::Resistor(instance_name(index))
                } else {
                    return Err(unsupported(format!(
                        "DC sweep supports independent V/I sources, resistors, temp and \
                         @instance[parameter] only; unsupported target {name}"
                    )));
                }
            }
            Written::Resistance(name) => {
                let (index, _) = circuit
                    .resistor(name)
                    .ok_or_else(|| SpiceError::circuit("resolved DC resistor disappeared"))?;
                parameter(instance_name(index), "r", ParameterRoute::Resistor)
            }
            Written::Device(name, canonical) => {
                let (index, _) = circuit.instance_parameter(name, canonical)?;
                parameter(instance_name(index), canonical, ParameterRoute::Device)
            }
            Written::Other(name, keyword) => {
                let keyword = keyword.to_ascii_lowercase();
                match source(name) {
                    // vsrc.c/isrc.c: `dc` (isrc alias `c`) is the DC value
                    // that `.dc v1` sweeps.
                    Some(source)
                        if keyword == "dc"
                            || (keyword == "c"
                                && source.kind == crate::devices::SourceKind::Current) =>
                    {
                        let route = match source.kind {
                            crate::devices::SourceKind::Voltage => ParameterRoute::VoltageSource,
                            crate::devices::SourceKind::Current => ParameterRoute::CurrentSource,
                        };
                        parameter(source.name.clone(), "dc", route)
                    }
                    // Explicit NotYetPorted/Unsupported with the C reference.
                    _ => {
                        circuit.instance_parameter(name, &keyword)?;
                        return Err(SpiceError::circuit(format!(
                            "DC sweep target @{name}[{keyword}] could not be resolved"
                        )));
                    }
                }
            }
        };
        let spec = SweepSpec {
            target,
            start: number(Some(axis[1]), "sweep start")?,
            stop: number(Some(axis[2]), "sweep stop")?,
            step: number(Some(axis[3]), "sweep step")?,
        };
        work = work
            .checked_mul(spec.grid()?.len())
            .filter(|n| *n <= MAX_SWEEP_POINTS)
            .ok_or_else(|| unsupported("nested DC sweep point limit exceeded"))?;
        axes.push(spec);
    }
    Ok(axes)
}

/// The per-point model context and source values of one Cartesian point:
/// `values` pairs with `axes` (inner first).
fn point_context<'a>(
    circuit: &Circuit,
    axes: &'a [SweepSpec],
    values: [Real; 2],
    base: ModelContext,
) -> SpiceResult<(ModelContext, Vec<(&'a str, Real)>)> {
    let mut model = base;
    let mut sources = vec![];
    for (axis, value) in axes.iter().zip(values) {
        if let Some(name) = axis.target.source() {
            sources.push((name, value));
            continue;
        }
        match &axis.target {
            SweepTarget::Temperature => model.temperature = value,
            SweepTarget::Resistor(name)
            | SweepTarget::InstanceParameter {
                instance: name,
                route: ParameterRoute::Resistor,
                ..
            } => {
                model = model.with_resistor_override(circuit.resistor_override(name, value)?)?;
            }
            _ => {}
        }
    }
    // Instance parameters are applied (and validated) at the point's
    // temperature, after a temperature axis in either position.
    for (axis, value) in axes.iter().zip(values) {
        if let SweepTarget::InstanceParameter {
            instance,
            parameter,
            route: ParameterRoute::Device,
            ..
        } = &axis.target
        {
            model = model.with_instance_override(
                circuit.instance_override(instance, parameter, value, &model)?,
            )?;
        }
    }
    Ok((model, sources))
}
/// Reject every point the run would later fail on for *input* reasons: a swept
/// resistance whose effective value is invalid at some swept temperature, or a
/// circuit that cannot be assembled at some swept temperature. Convergence
/// failures remain run-time errors.
fn preflight(
    circuit: &Circuit,
    axes: &[SweepSpec],
    grids: &[Vec<Real>],
    context: &AnalysisContext,
) -> SpiceResult<()> {
    let zero = Vector::zeros(circuit.unknown_count());
    // A temperature or device-parameter axis changes the devices themselves:
    // validate actual Cartesian point contexts, not the original recipes at
    // each temperature (those recipes are replaced by the sweep).
    if axes.iter().any(|axis| {
        matches!(
            axis.target,
            SweepTarget::Temperature
                | SweepTarget::InstanceParameter {
                    route: ParameterRoute::Device,
                    ..
                }
        )
    }) {
        let single = [0.];
        let outer = grids.get(1).map_or(&single[..], Vec::as_slice);
        for outer_value in outer {
            for inner_value in &grids[0] {
                let (model, _) = point_context(
                    circuit,
                    axes,
                    [*inner_value, *outer_value],
                    context.model_context(),
                )?;
                circuit.small_signal_system(&model, &zero)?;
            }
        }
        return Ok(());
    }
    let model = context.model_context();
    for (axis, grid) in axes.iter().zip(grids) {
        let name = match &axis.target {
            SweepTarget::Resistor(name)
            | SweepTarget::InstanceParameter {
                instance: name,
                route: ParameterRoute::Resistor,
                ..
            } => name,
            _ => continue,
        };
        for value in grid {
            let target = circuit.resistor_override(name, *value)?;
            circuit.effective_resistance(&target, &model)?;
        }
    }
    Ok(())
}
/// Request key of the per-point warm-start Newton limit (deck `itl2`, C
/// `CKTdcTrcvMaxIter`).
pub(crate) const POINT_ITERATIONS_KEY: &str = "trcvmaxiter";

/// Split the `.dc`-only `trcvmaxiter=` (1..=[`crate::analysis::newton::MAX_ITERATIONS`])
/// from the DC Newton/continuation settings shared with `.op`.
fn sweep_settings(
    request: &AnalysisRequest,
) -> SpiceResult<(Option<usize>, crate::analysis::bias::DcSettings)> {
    let mut limit = None;
    let mut rest = Vec::new();
    for argument in &request.arguments {
        match argument.split_once('=') {
            Some((key, text)) if key.trim().eq_ignore_ascii_case(POINT_ITERATIONS_KEY) => {
                if limit.is_some() {
                    return Err(SpiceError::Unsupported {
                        feature: format!("duplicate .dc option {POINT_ITERATIONS_KEY}"),
                        location: None,
                    });
                }
                let largest = crate::analysis::newton::MAX_ITERATIONS as Real;
                let value = crate::primitives::parse_spice_number(text.trim())
                    .filter(|v| v.fract() == 0. && (1. ..=largest).contains(v))
                    .ok_or_else(|| SpiceError::Unsupported {
                        feature: format!(
                            "{POINT_ITERATIONS_KEY} must be an integer in 1..={}, not '{}'",
                            crate::analysis::newton::MAX_ITERATIONS,
                            text.trim()
                        ),
                        location: None,
                    })?;
                limit = Some(value as usize);
            }
            _ => rest.push(argument.clone()),
        }
    }
    let mut stripped = request.clone();
    stripped.arguments = rest;
    Ok((
        limit,
        crate::analysis::bias::DcSettings::from_request(&stripped)?,
    ))
}

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Plot> {
    circuit.finalize()?;
    let circuit: &Circuit = circuit;
    // Everything that can be rejected is rejected before the first sample.
    let hints = crate::analysis::initial::resolve(circuit, request)?;
    // cktload.c forces .nodeset rows in the MODEINITJCT/MODEINITFIX loads
    // (the first point of each inner sweep and every CKTop restart).
    let nodes = crate::analysis::bias::NodeForcing {
        initial: Vec::new(),
        nodesets: crate::analysis::initial::forced_nodesets(circuit, &hints.nodesets, &[]),
    };
    let axes = resolve(circuit, request, context)?;
    let (point_iterations, settings) = sweep_settings(request)?;
    // dctrcurv.c always warm-starts every point after the first with
    // `NIiter(CKTdcTrcvMaxIter)` (C's effective default 100) before falling
    // back to a fresh `CKTop`; the port's legacy ladder schedule instead runs
    // one full solve per point unless `itl2` is given.
    let point_iterations = point_iterations.or(match settings.continuation.schedule {
        crate::analysis::bias::ContinuationSchedule::Ngspice(_) => {
            Some(crate::analysis::config::C_DEFAULT_ITL2_EFFECTIVE)
        }
        crate::analysis::bias::ContinuationSchedule::Ladder => None,
    });
    // A switch control landing exactly on a threshold is decided by the last
    // bit of the swept value, so with discrete-state devices the sweep visits
    // C's accumulated values (`dctrcurv.c`) rather than `start + i step`.
    let discrete = circuit.devices().iter().any(|d| d.has_discrete_state());
    let grids = axes
        .iter()
        .map(|axis| {
            if discrete {
                axis.accumulated_grid()
            } else {
                axis.grid()
            }
        })
        .collect::<SpiceResult<Vec<_>>>()?;
    // C's absolute stop test can add one point per axis to the checked budget.
    grids
        .iter()
        .try_fold(1usize, |work, grid| work.checked_mul(grid.len()))
        .filter(|n| *n <= MAX_SWEEP_POINTS)
        .ok_or_else(|| unsupported("nested DC sweep point limit exceeded"))?;
    preflight(circuit, &axes, &grids, context)?;
    // C order: the first axis is the inner (fast) loop, the second the outer.
    let inner = &grids[0];
    let single = [0.];
    let outer: &[Real] = grids.get(1).map_or(&single[..], Vec::as_slice);
    let mut result = plot(
        circuit,
        "dc1",
        "DC transfer characteristic",
        Some(("sweep", axes[0].target.unit())),
        false,
    )?;
    if axes.len() == 2 {
        result.push_variable(crate::analysis::results::Variable::new(
            format!("sweep({})", axes[1].target.name()),
            axes[1].target.unit(),
        ));
    }
    let linear = !circuit.devices().iter().any(|d| d.is_nonlinear());
    // The factorization is valid only while the operator is: a resistor or
    // temperature axis changes it at every point, so those recompute below.
    let source_only = axes.iter().all(|a| a.target.source().is_some());
    // Preserve the delivered repeated-RHS LU optimization on source-only linear sweeps.
    let system = if linear && source_only {
        Some(circuit.small_signal_system(
            &context.model_context(),
            &Vector::zeros(circuit.unknown_count()),
        )?)
    } else {
        None
    };
    let lu = system.as_ref().map(|s| s.a.factorize()).transpose()?;
    let mut previous = Vector::zeros(circuit.unknown_count());
    // dctrcurv.c rotates the state vectors before every point, so a point's
    // CKTstate1 is the previous point's state (the first point's own state is
    // copied in after it). Only devices with discrete state (switches) read it.
    let mut history = circuit.state_history();
    let mut first;
    for hint in hints.nodesets {
        previous.as_mut_slice()[hint.row] = hint.value;
    }
    for outer_value in outer {
        // dctrcurv.c: when the inner sweep wraps to its start for the next
        // outer value, `firstTime` is set again and the mode is MODEINITJCT,
        // so the first point of every inner sweep is decided from the
        // instance flags like the very first point, and its converged state
        // then becomes the accepted history (C's `firstTime` memcpy).
        first = true;
        for inner_value in inner {
            let (model, overrides) = point_context(
                circuit,
                &axes,
                [*inner_value, *outer_value],
                context.model_context(),
            )?;
            let mut solved_state = None;
            let x = if let (Some(system), Some(lu)) = (&system, &lu) {
                let mut rhs = system.dc_rhs(None)?;
                for (name, value) in &overrides {
                    let source = system
                        .sources
                        .iter()
                        .find(|s| s.name == *name)
                        .ok_or_else(|| SpiceError::circuit("resolved DC source disappeared"))?;
                    for (row, sign) in &source.rows {
                        rhs.add_to(*row, sign * (value - source.dc))?;
                    }
                }
                lu.solve(&rhs)?
            } else {
                // dctrcurv.c: every point after the first first tries a plain
                // warm-started Newton bounded by `itl2` (CKTdcTrcvMaxIter) and
                // only on failure falls back to the full operating-point solve.
                let warm = match point_iterations.filter(|_| !first) {
                    None => None,
                    Some(limit) => {
                        let direct = crate::analysis::bias::DcSettings {
                            newton: crate::analysis::newton::NewtonOptions {
                                max_iterations: limit,
                                ..settings.newton
                            },
                            continuation: crate::analysis::bias::ContinuationPolicy::disabled(),
                        };
                        match crate::analysis::bias::solve_dc_from(
                            circuit,
                            &model,
                            &direct,
                            &overrides,
                            Some(&previous),
                            &history,
                            crate::analysis::newton::PhasePolicy::Predicted,
                            &nodes,
                        ) {
                            Ok(solved) => Some(solved.solution),
                            Err(failure)
                                if matches!(
                                    failure.report.outcome,
                                    crate::analysis::bias::DcOutcome::Exhausted
                                        | crate::analysis::bias::DcOutcome::BudgetExhausted
                                ) =>
                            {
                                None
                            }
                            Err(failure) => return Err(failure.error),
                        }
                    }
                };
                let solution = match warm {
                    Some(solution) => solution,
                    None => {
                        crate::analysis::bias::solve_dc_from(
                            circuit,
                            &model,
                            &settings,
                            &overrides,
                            Some(&previous),
                            &history,
                            // Without a warm-start limit the full solve's
                            // direct attempt is the predicted point; after a
                            // failed warm start it restarts like CKTop.
                            if first || point_iterations.is_some() {
                                crate::analysis::newton::PhasePolicy::OperatingPoint
                            } else {
                                crate::analysis::newton::PhasePolicy::Predicted
                            },
                            &nodes,
                        )?
                        .solution
                    }
                };
                solved_state = Some(solution.trial);
                solution.values
            };
            first = false;
            // Only a solved point is accepted; a failure above returns before
            // any accept hook runs for it. A nonlinear point's state becomes the
            // accepted history of the next one.
            match solved_state {
                Some(trial) => circuit.accept_point(&x, None, &mut history, trial)?,
                None => circuit.accept_solution(&x, None)?,
            }
            let mut point = vec![Complex::real(*inner_value)];
            point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
            if axes.len() == 2 {
                point.push(Complex::real(*outer_value));
            }
            result.push_point(point)?;
            previous = x;
        }
    }
    Ok(result)
}
