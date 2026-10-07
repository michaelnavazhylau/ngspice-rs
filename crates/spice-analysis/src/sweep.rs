//! Typed bounded DC sweeps, including one outer sweep (`dctran.c`).
//! Targets are independent V/I sources and circuit temperature. Resistance,
//! model parameters and more than two axes are explicitly unsupported.
use crate::linear::{number, plot, unsupported};
use crate::{AnalysisContext, AnalysisRequest, Plot};
use spice_core::{AnalysisKind, Complex, Real, SpiceError, SpiceResult};
use spice_devices::Circuit;
use spice_maths::Vector;

/// A resolved sweep target, not an arbitrary parameter-name string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepTarget {
    /// Independent voltage-source DC value (V).
    VoltageSource(String),
    /// Independent current-source DC value (A).
    CurrentSource(String),
    /// Circuit temperature (Celsius); nominal temperature is unchanged.
    Temperature,
}
impl SweepTarget {
    fn unit(&self) -> &str {
        match self {
            Self::VoltageSource(_) => "voltage",
            Self::CurrentSource(_) => "current",
            Self::Temperature => "temperature",
        }
    }
    fn name(&self) -> &str {
        match self {
            Self::VoltageSource(n) | Self::CurrentSource(n) => n,
            Self::Temperature => "temp",
        }
    }
}
/// A finite directional sweep with an inclusive reachable stop and bounded work.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepSpec {
    /// Typed source or circuit-temperature target.
    pub target: SweepTarget,
    /// First value in the target's physical unit.
    pub start: Real,
    /// Last bound (included only if reached on the grid).
    pub stop: Real,
    /// Nonzero step with the correct direction.
    pub step: Real,
}
impl SweepSpec {
    /// Build a bounded grid; reject overflow, wrong directions and no progress.
    pub fn grid(&self) -> SpiceResult<Vec<Real>> {
        if [self.start, self.stop, self.step]
            .iter()
            .any(|v| !v.is_finite())
            || self.step == 0.
            || (self.stop - self.start) * self.step < 0.
        {
            return Err(unsupported("invalid sweep direction/step"));
        }
        let count = ((self.stop - self.start) / self.step).floor() + 1.;
        if !count.is_finite() || !(1. ..=100_000.).contains(&count) {
            return Err(unsupported("DC sweep point limit exceeded"));
        }
        let values: Vec<_> = (0..count as usize)
            .map(|i| self.start + (i as Real) * self.step)
            .collect();
        if values.iter().any(|v| !v.is_finite()) || values.windows(2).any(|w| w[0] == w[1]) {
            return Err(unsupported("DC sweep makes no progress"));
        }
        if self.target == SweepTarget::Temperature && values.iter().any(|v| *v <= -273.15) {
            return Err(unsupported("DC temperature below absolute zero"));
        }
        Ok(values)
    }
}
/// Parse and resolve one/two axes against actual independent sources.
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
    let system = circuit.small_signal_system(
        &context.model_context(),
        &Vector::zeros(circuit.unknown_count()),
    )?;
    let mut axes = vec![];
    let mut names = std::collections::BTreeSet::new();
    let mut work = 1usize;
    for axis in args.as_chunks::<4>().0 {
        let name = axis[0].to_ascii_lowercase();
        if !names.insert(name.clone()) {
            return Err(unsupported("duplicate nested DC target"));
        }
        let target = if name == "temp" {
            SweepTarget::Temperature
        } else {
            let source = system
                .sources
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&name))
                .ok_or_else(|| {
                    unsupported("DC sweep supports independent V/I sources and temp only")
                })?;
            match source.kind {
                spice_devices::SourceKind::Voltage => {
                    SweepTarget::VoltageSource(source.name.clone())
                }
                spice_devices::SourceKind::Current => {
                    SweepTarget::CurrentSource(source.name.clone())
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
            .filter(|n| *n <= 100_000)
            .ok_or_else(|| unsupported("nested DC sweep point limit exceeded"))?;
        axes.push(spec);
    }
    Ok(axes)
}
pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Plot> {
    circuit.finalize()?;
    let hints = crate::initial::resolve(circuit, request)?;
    let axes = resolve(circuit, request, context)?;
    let options = crate::newton::NewtonOptions::from_request(request)?;
    let inner = axes[0].grid()?;
    let outer = if axes.len() == 2 {
        axes[1].grid()?
    } else {
        vec![0.]
    };
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
    let temperature_axis = axes.iter().any(|a| a.target == SweepTarget::Temperature);
    // Preserve the delivered repeated-RHS LU optimization on source-only linear sweeps.
    let system = if linear && !temperature_axis {
        Some(circuit.small_signal_system(
            &context.model_context(),
            &Vector::zeros(circuit.unknown_count()),
        )?)
    } else {
        None
    };
    let lu = system.as_ref().map(|s| s.a.factorize()).transpose()?;
    let mut previous = Vector::zeros(circuit.unknown_count());
    for hint in hints.nodesets {
        previous.as_mut_slice()[hint.row] = hint.value;
    }
    for outer_value in outer {
        for inner_value in &inner {
            let mut model = context.model_context();
            let mut overrides = vec![];
            for (axis, value) in axes.iter().zip([*inner_value, outer_value]) {
                match &axis.target {
                    SweepTarget::VoltageSource(name) | SweepTarget::CurrentSource(name) => {
                        overrides.push((name.as_str(), value))
                    }
                    SweepTarget::Temperature => model.temperature = value,
                }
            }
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
                crate::bias::solve_dc(circuit, &model, &options, &overrides, Some(&previous), None)?
                    .values
            };
            circuit.accept_solution(&x, None)?;
            let mut point = vec![Complex::real(*inner_value)];
            point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
            if axes.len() == 2 {
                point.push(Complex::real(outer_value));
            }
            result.push_point(point)?;
            previous = x;
        }
    }
    Ok(result)
}
