//! Typed bounded DC sweeps, including one outer sweep (`dctrcurv.c`).
//! Targets are independent V/I sources, resistors (literal or model-backed,
//! identified by physical device metadata) and circuit temperature. Other
//! model parameters and more than two axes are explicitly unsupported.
//! Nothing is mutated: sources use temporary RHS offsets, temperature and
//! resistor values travel in an immutable per-point `ModelContext`.
//! See `docs/port/DC_SWEEPS.md`.
use crate::linear::{number, plot, unsupported};
use crate::{AnalysisContext, AnalysisRequest, Plot};
use spice_core::{AnalysisKind, Complex, Real, SpiceError, SpiceResult};
use spice_devices::{Circuit, ModelContext};
use spice_maths::Vector;

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
    /// resistors (`spice_devices::sweep`).
    Resistor(String),
    /// Circuit temperature (Celsius); nominal temperature is unchanged.
    Temperature,
}
impl SweepTarget {
    /// The plot unit of the swept quantity.
    #[must_use]
    pub fn unit(&self) -> &str {
        match self {
            Self::VoltageSource(_) => "voltage",
            Self::CurrentSource(_) => "current",
            Self::Resistor(_) => "resistance",
            Self::Temperature => "temperature",
        }
    }
    /// The instance name, or `temp`.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::VoltageSource(n) | Self::CurrentSource(n) | Self::Resistor(n) => n,
            Self::Temperature => "temp",
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
        match self.target {
            SweepTarget::Temperature if values.iter().any(|v| *v <= -273.15) => {
                return Err(unsupported("DC temperature below absolute zero"));
            }
            SweepTarget::Resistor(_)
                if values.iter().any(|v| *v == 0. || !(1. / v).is_finite()) =>
            {
                return Err(unsupported(
                    "DC resistance must be nonzero with finite conductance",
                ));
            }
            _ => {}
        }
        Ok(())
    }
}
/// Parse and resolve one/two axes against actual independent sources and
/// resistors (by physical metadata, never by instance-name prefix).
/// Positional form: `target start stop step [outer start stop step]`.
/// # Errors
/// Missing/unsupported/duplicate targets, invalid grid or Cartesian work budget.
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
    // Resolve the first point's resistor/temperature settings before assembly:
    // the original resistance may overflow at a swept temperature even though
    // every requested replacement is valid. Physical resistor metadata needs
    // no equation assembly, and source kinds come from the resulting system.
    let mut names = std::collections::BTreeSet::new();
    let mut probe = context.model_context();
    for axis in args.as_chunks::<4>().0 {
        let name = axis[0].to_ascii_lowercase();
        if !names.insert(name.clone()) {
            return Err(unsupported("duplicate nested DC target"));
        }
        let start = number(Some(axis[1]), "sweep start")?;
        if name == "temp" {
            probe.temperature = start;
        } else if circuit.resistor(&name).is_some() {
            probe = probe.with_resistor_override(circuit.resistor_override(&name, start)?)?;
        }
    }
    let system = circuit.small_signal_system(&probe, &Vector::zeros(circuit.unknown_count()))?;
    let mut axes = vec![];
    let mut work = 1usize;
    for axis in args.as_chunks::<4>().0 {
        let name = axis[0].to_ascii_lowercase();
        let target = if name == "temp" {
            SweepTarget::Temperature
        } else if let Some(source) = system
            .sources
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&name))
        {
            match source.kind {
                spice_devices::SourceKind::Voltage => {
                    SweepTarget::VoltageSource(source.name.clone())
                }
                spice_devices::SourceKind::Current => {
                    SweepTarget::CurrentSource(source.name.clone())
                }
            }
        } else if let Some((index, _)) = circuit.resistor(&name) {
            SweepTarget::Resistor(circuit.devices()[index].name().to_owned())
        } else {
            return Err(unsupported(format!(
                "DC sweep supports independent V/I sources, resistors and temp only; unsupported target {name}"
            )));
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
    let temperatures = axes
        .iter()
        .zip(grids)
        .find(|(axis, _)| axis.target == SweepTarget::Temperature)
        .map(|(_, grid)| grid.clone());
    let zero = Vector::zeros(circuit.unknown_count());
    if temperatures.is_some() {
        // Validate actual Cartesian point contexts, not the original resistor
        // recipes at each temperature: those recipes are replaced by the sweep.
        let single = [0.];
        let outer = grids.get(1).map_or(&single[..], Vec::as_slice);
        for outer_value in outer {
            for inner_value in &grids[0] {
                let mut model = context.model_context();
                for (axis, value) in axes.iter().zip([*inner_value, *outer_value]) {
                    match &axis.target {
                        SweepTarget::Temperature => model.temperature = value,
                        SweepTarget::Resistor(name) => {
                            model = model
                                .with_resistor_override(circuit.resistor_override(name, value)?)?;
                        }
                        _ => {}
                    }
                }
                circuit.small_signal_system(&model, &zero)?;
            }
        }
        return Ok(());
    }
    let temperatures = vec![context.temperature];
    for (axis, grid) in axes.iter().zip(grids) {
        let SweepTarget::Resistor(name) = &axis.target else {
            continue;
        };
        for temperature in &temperatures {
            let model = ModelContext {
                temperature: *temperature,
                ..context.model_context()
            };
            for value in grid {
                let target = circuit.resistor_override(name, *value)?;
                circuit.effective_resistance(&target, &model)?;
            }
        }
    }
    Ok(())
}
/// Request key of the per-point warm-start Newton limit (deck `itl2`, C
/// `CKTdcTrcvMaxIter`).
pub(crate) const POINT_ITERATIONS_KEY: &str = "trcvmaxiter";

/// Split the `.dc`-only `trcvmaxiter=` (1..=[`crate::newton::MAX_ITERATIONS`])
/// from the DC Newton/continuation settings shared with `.op`.
fn sweep_settings(
    request: &AnalysisRequest,
) -> SpiceResult<(Option<usize>, crate::bias::DcSettings)> {
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
                let largest = crate::newton::MAX_ITERATIONS as Real;
                let value = spice_core::parse_spice_number(text.trim())
                    .filter(|v| v.fract() == 0. && (1. ..=largest).contains(v))
                    .ok_or_else(|| SpiceError::Unsupported {
                        feature: format!(
                            "{POINT_ITERATIONS_KEY} must be an integer in 1..={}, not '{}'",
                            crate::newton::MAX_ITERATIONS,
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
    Ok((limit, crate::bias::DcSettings::from_request(&stripped)?))
}

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Plot> {
    circuit.finalize()?;
    let circuit: &Circuit = circuit;
    // Everything that can be rejected is rejected before the first sample.
    let hints = crate::initial::resolve(circuit, request)?;
    let axes = resolve(circuit, request, context)?;
    let (point_iterations, settings) = sweep_settings(request)?;
    // dctrcurv.c always warm-starts every point after the first with
    // `NIiter(CKTdcTrcvMaxIter)` (C's effective default 100) before falling
    // back to a fresh `CKTop`; the port's legacy ladder schedule instead runs
    // one full solve per point unless `itl2` is given.
    let point_iterations = point_iterations.or(match settings.continuation.schedule {
        crate::bias::ContinuationSchedule::Ngspice(_) => {
            Some(crate::config::C_DEFAULT_ITL2_EFFECTIVE)
        }
        crate::bias::ContinuationSchedule::Ladder => None,
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
        result.push_variable(crate::results::Variable::new(
            format!("sweep({})", axes[1].target.name()),
            axes[1].target.unit(),
        ));
    }
    let linear = !circuit.devices().iter().any(|d| d.is_nonlinear());
    // The factorization is valid only while the operator is: a resistor or
    // temperature axis changes it at every point, so those recompute below.
    let source_only = axes.iter().all(|a| {
        matches!(
            a.target,
            SweepTarget::VoltageSource(_) | SweepTarget::CurrentSource(_)
        )
    });
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
            let mut model = context.model_context();
            let mut overrides = vec![];
            for (axis, value) in axes.iter().zip([*inner_value, *outer_value]) {
                match &axis.target {
                    SweepTarget::VoltageSource(name) | SweepTarget::CurrentSource(name) => {
                        overrides.push((name.as_str(), value))
                    }
                    SweepTarget::Temperature => model.temperature = value,
                    SweepTarget::Resistor(name) => {
                        model = model
                            .with_resistor_override(circuit.resistor_override(name, value)?)?;
                    }
                }
            }
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
                        let direct = crate::bias::DcSettings {
                            newton: crate::newton::NewtonOptions {
                                max_iterations: limit,
                                ..settings.newton
                            },
                            continuation: crate::bias::ContinuationPolicy::disabled(),
                        };
                        match crate::bias::solve_dc_from(
                            circuit,
                            &model,
                            &direct,
                            &overrides,
                            Some(&previous),
                            &history,
                            crate::newton::PhasePolicy::Predicted,
                        ) {
                            Ok(solved) => Some(solved.solution),
                            Err(failure)
                                if matches!(
                                    failure.report.outcome,
                                    crate::bias::DcOutcome::Exhausted
                                        | crate::bias::DcOutcome::BudgetExhausted
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
                        crate::bias::solve_dc_from(
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
                                crate::newton::PhasePolicy::OperatingPoint
                            } else {
                                crate::newton::PhasePolicy::Predicted
                            },
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
