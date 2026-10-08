//! Reusable, disposable Newton loads for DC and companion transient solves.
//!
//! C references: `niiter.c`, `niconv.c`, `cktop.c`. This bounded policy uses
//! global voltage-step damping, not ngspice's per-junction PN/FET limiting.
//! No trial or continuation stage invokes device acceptance hooks.
use spice_core::{Real, SpiceError, SpiceResult};
use spice_maths::{SparseMatrix, Vector};

/// Largest accepted per-solve Newton iteration limit (`maxiter`, deck `itl1`).
pub const MAX_ITERATIONS: usize = 10_000;

/// Request names owned by [`crate::bias::ContinuationPolicy`], not by Newton.
pub(crate) const CONTINUATION_KEYS: [&str; 4] =
    ["srcsteps", "gminsteps", "gminfactor", STAGE_ITERATIONS_KEY];

/// Request key of the Newton limit per gmin/source-stepping stage (deck
/// `itl2`, C `CKTdcTrcvMaxIter` in `cktop.c`); see
/// [`crate::bias::ContinuationPolicy::stage_max_iterations`].
pub(crate) const STAGE_ITERATIONS_KEY: &str = "stagemaxiter";

/// Finite work and physical voltage/current tolerances for Newton iteration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewtonOptions {
    /// Maximum loads/solves per continuation stage.
    pub max_iterations: usize,
    /// Relative iterate and equation-residual tolerance.
    pub reltol: Real,
    /// Absolute voltage tolerance (V).
    pub vntol: Real,
    /// Absolute current tolerance (A).
    pub abstol: Real,
    /// Maximum nodal voltage change per iteration (V); limits trial exponentials.
    pub voltage_step: Real,
}
impl Default for NewtonOptions {
    fn default() -> Self {
        Self {
            max_iterations: 200,
            reltol: 1e-8,
            vntol: 1e-10,
            abstol: 1e-12,
            voltage_step: 0.2,
        }
    }
}
impl NewtonOptions {
    /// Resolve named `rtol`, `vntol`, `abstol`, `maxiter` arguments for DC/AC.
    /// Unknown and duplicate names fail instead of silently selecting defaults.
    /// Continuation names (`srcsteps`, `gminsteps`, `gminfactor`) also fail here:
    /// a Newton-only caller would silently drop them, so use
    /// [`crate::bias::DcSettings::from_request`], which resolves all of them.
    /// # Errors
    /// Invalid or unimplemented convergence arguments.
    pub fn from_request(request: &crate::AnalysisRequest) -> SpiceResult<Self> {
        let mut options = Self::default();
        let largest = MAX_ITERATIONS as f64;
        let mut seen = std::collections::BTreeSet::new();
        for argument in &request.arguments {
            let Some((key, text)) = argument.split_once('=') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            if !seen.insert(key.clone()) {
                return Err(failure("duplicate convergence option"));
            }
            let value = spice_core::parse_spice_number(text.trim())
                .filter(|v| v.is_finite())
                .ok_or_else(|| failure("nonfinite/nonliteral convergence option"))?;
            match key.as_str() {
                "rtol" => options.reltol = value,
                "vntol" => options.vntol = value,
                "abstol" => options.abstol = value,
                "maxiter" if value.fract() == 0. && (1. ..=largest).contains(&value) => {
                    options.max_iterations = value as usize
                }
                name if CONTINUATION_KEYS.contains(&name) => {
                    return Err(spice_core::SpiceError::Unsupported {
                        feature: format!(
                            "continuation option {name} needs DcSettings::from_request, \
                             not NewtonOptions::from_request"
                        ),
                        location: None,
                    });
                }
                _ => {
                    return Err(spice_core::SpiceError::Unsupported {
                        feature: format!("convergence option {key}"),
                        location: None,
                    });
                }
            }
        }
        options.validate()?;
        Ok(options)
    }

    /// Validate tolerances and bounded work before the first load.
    pub fn validate(&self) -> SpiceResult<()> {
        if !(1..=MAX_ITERATIONS).contains(&self.max_iterations)
            || [self.reltol, self.vntol, self.abstol, self.voltage_step]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err(failure(
                "invalid Newton tolerances, voltage limit or iteration budget",
            ));
        }
        Ok(())
    }
}

/// One solution whose disposable load was reevaluated at the solved point.
#[derive(Debug)]
pub struct NewtonSolution<T> {
    /// Converged physical MNA values.
    pub values: Vector,
    /// Trial data belonging to `values`, not to the preceding iterate.
    pub trial: T,
    /// Number of solves, bounded by [`NewtonOptions::max_iterations`].
    pub iterations: usize,
}

/// Solve `J(x) x_new = b_equivalent(x)` using fresh, immutable trial loads.
///
/// `branch_rows[r]` selects current tolerance for unknown `r` and voltage
/// tolerance for its KVL equation. Other rows use voltage iterate/current KCL
/// tolerances. Both iterate **and reloaded equation residual** must converge.
/// The callback must not commit state or limit the evaluated voltage: residuals
/// must represent the actual physical equations at the supplied solution.
///
/// # Errors
/// Invalid dimensions/settings, load failures, singular/nonfinite systems, or
/// exhaustion of the iteration budget. No acceptance hooks are called.
pub fn solve<T>(
    initial: &Vector,
    branch_rows: &[bool],
    options: &NewtonOptions,
    load: impl FnMut(&Vector) -> SpiceResult<(SparseMatrix, Vector, T)>,
) -> SpiceResult<NewtonSolution<T>> {
    solve_counted(initial, branch_rows, options, load).map_err(|failure| failure.error)
}

/// A failed Newton solve and the number of iterations it consumed.
#[derive(Debug, Clone, PartialEq)]
pub struct NewtonFailure {
    /// Why the solve failed (unchanged from [`solve`]).
    pub error: SpiceError,
    /// Iterations started before the failure, at most
    /// [`NewtonOptions::max_iterations`]; zero when settings were rejected.
    pub iterations: usize,
}

/// [`solve`] that also reports the work a failed solve used, so a caller owning a
/// total work budget (DC continuation) charges exactly what was spent.
///
/// # Errors
/// As [`solve`], with the iteration count in [`NewtonFailure`].
pub fn solve_counted<T>(
    initial: &Vector,
    branch_rows: &[bool],
    options: &NewtonOptions,
    load: impl FnMut(&Vector) -> SpiceResult<(SparseMatrix, Vector, T)>,
) -> Result<NewtonSolution<T>, NewtonFailure> {
    let mut iterations = 0;
    iterate(initial, branch_rows, options, load, &mut iterations)
        .map_err(|error| NewtonFailure { error, iterations })
}

fn iterate<T>(
    initial: &Vector,
    branch_rows: &[bool],
    options: &NewtonOptions,
    mut load: impl FnMut(&Vector) -> SpiceResult<(SparseMatrix, Vector, T)>,
    started: &mut usize,
) -> SpiceResult<NewtonSolution<T>> {
    options.validate()?;
    let n = initial.len();
    if n == 0 || n != branch_rows.len() || !initial.is_finite() {
        return Err(failure(
            "Newton requires a nonempty square system, finite solution and matching row kinds",
        ));
    }
    let mut guess = initial.clone();
    for iteration in 1..=options.max_iterations {
        *started = iteration;
        let (mut matrix, rhs, _) = load(&guess)?;
        check(&matrix, &rhs, n)?;
        matrix.fold_duplicates();
        let (matrix, rhs) = equilibrated(&matrix, &rhs)?;
        let mut next = matrix.solve(&rhs)?;
        let largest_voltage_step = next
            .as_slice()
            .iter()
            .zip(guess.as_slice())
            .zip(branch_rows)
            .filter(|(_, branch)| !**branch)
            .map(|((new, old), _)| (new - old).abs())
            .fold(0., Real::max);
        let damping = (options.voltage_step / largest_voltage_step).min(1.);
        if damping < 1. {
            for (new, old) in next.as_mut_slice().iter_mut().zip(guess.as_slice()) {
                *new = old + damping * (*new - old);
            }
        }
        if !next.is_finite() {
            return Err(failure("nonfinite Newton iterate"));
        }
        if converged(&next, &guess, branch_rows, options) {
            let (mut physical, rhs, trial) = load(&next)?;
            check(&physical, &rhs, n)?;
            physical.fold_duplicates();
            if residual_ok(&physical, &rhs, &next, branch_rows, options)? {
                return Ok(NewtonSolution {
                    values: next,
                    trial,
                    iterations: iteration,
                });
            }
        }
        guess = next;
    }
    Err(failure(format!(
        "Newton iteration limit ({}) reached",
        options.max_iterations
    )))
}
fn check(matrix: &SparseMatrix, rhs: &Vector, n: usize) -> SpiceResult<()> {
    if matrix.rows() != n || matrix.cols() != n || rhs.len() != n || !rhs.is_finite() {
        return Err(failure("invalid Newton load dimensions or nonfinite RHS"));
    }
    Ok(())
}
fn converged(new: &Vector, old: &Vector, branch: &[bool], options: &NewtonOptions) -> bool {
    new.as_slice()
        .iter()
        .zip(old.as_slice())
        .zip(branch)
        .all(|((new, old), branch)| {
            (new - old).abs()
                <= options.reltol * new.abs().max(old.abs())
                    + if *branch {
                        options.abstol
                    } else {
                        options.vntol
                    }
        })
}
fn residual_ok(
    matrix: &SparseMatrix,
    rhs: &Vector,
    x: &Vector,
    branch: &[bool],
    options: &NewtonOptions,
) -> SpiceResult<bool> {
    let mut residual: Vec<_> = rhs.as_slice().iter().map(|v| -*v).collect();
    let mut scale: Vec<_> = rhs.as_slice().iter().map(|v| v.abs()).collect();
    for t in matrix.triplets() {
        let term = t.value * x.as_slice()[t.col];
        residual[t.row] += term;
        scale[t.row] += term.abs();
    }
    if residual.iter().chain(&scale).any(|v| !v.is_finite()) {
        return Err(failure("nonfinite Newton physical residual"));
    }
    Ok(residual
        .iter()
        .zip(scale)
        .zip(branch)
        .all(|((residual, scale), branch)| {
            residual.abs()
                <= options.reltol * scale
                    + if *branch {
                        options.vntol
                    } else {
                        options.abstol
                    }
        }))
}
pub(crate) fn equilibrated(
    matrix: &SparseMatrix,
    rhs: &Vector,
) -> SpiceResult<(SparseMatrix, Vector)> {
    let mut largest = vec![0.; matrix.rows()];
    for t in matrix.triplets() {
        largest[t.row] = Real::max(largest[t.row], t.value.abs());
    }
    let scale = |row: usize| {
        if largest[row] > 0. && largest[row].is_finite() {
            1. / largest[row]
        } else {
            1.
        }
    };
    let mut scaled = SparseMatrix::new(matrix.rows(), matrix.cols());
    for t in matrix.triplets() {
        scaled.add(t.row, t.col, t.value * scale(t.row))?;
    }
    let mut b = rhs.clone();
    for (row, value) in b.as_mut_slice().iter_mut().enumerate() {
        *value *= scale(row);
    }
    Ok((scaled, b))
}
fn failure(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "Newton".into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn solves_nonlinear_equation_and_returns_state_at_solution() {
        let initial = Vector::zeros(1);
        let solution = solve(&initial, &[false], &NewtonOptions::default(), |x| {
            let v = x.as_slice()[0];
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, v.exp())?;
            let mut b = Vector::zeros(1);
            b.add_to(0, 2. - v.exp() + v.exp() * v)?;
            Ok((a, b, v))
        })
        .unwrap();
        assert!((solution.values.as_slice()[0] - 2_f64.ln()).abs() < 1e-9);
        assert_eq!(solution.trial, solution.values.as_slice()[0]);
    }
    #[test]
    fn residual_prevents_false_small_step_success() {
        let mut initial = Vector::zeros(1);
        initial.add_to(0, 1e12).unwrap();
        let options = NewtonOptions {
            max_iterations: 2,
            ..NewtonOptions::default()
        };
        let error = solve(&initial, &[false], &options, |_| {
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, 1.)?;
            Ok((a, Vector::zeros(1), ()))
        })
        .unwrap_err();
        assert!(error.to_string().contains("iteration limit"));
    }
    #[test]
    fn counted_solve_reports_the_iterations_used_by_success_and_failure() {
        let identity = |_: &Vector| -> SpiceResult<(SparseMatrix, Vector, ())> {
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, 1.)?;
            Ok((a, Vector::zeros(1), ()))
        };
        let mut far = Vector::zeros(1);
        far.add_to(0, 1e12).unwrap();
        let options = NewtonOptions {
            max_iterations: 3,
            ..NewtonOptions::default()
        };
        let failure = solve_counted(&far, &[false], &options, identity).unwrap_err();
        assert_eq!(failure.iterations, 3);
        assert!(failure.error.to_string().contains("iteration limit"));
        assert_eq!(
            solve(&far, &[false], &options, identity).unwrap_err(),
            failure.error
        );
        let solved = solve_counted(&Vector::zeros(1), &[false], &options, identity).unwrap();
        assert_eq!(solved.iterations, 1);
        let rejected = NewtonOptions {
            max_iterations: 0,
            ..options
        };
        let failure = solve_counted(&far, &[false], &rejected, identity).unwrap_err();
        assert_eq!(failure.iterations, 0);
    }
    #[test]
    fn rejects_singular_homogeneous_system_and_invalid_settings() {
        assert!(
            solve(
                &Vector::zeros(1),
                &[false],
                &NewtonOptions::default(),
                |_| Ok((SparseMatrix::new(1, 1), Vector::zeros(1), ()))
            )
            .is_err()
        );
        assert!(
            NewtonOptions {
                abstol: Real::NAN,
                ..NewtonOptions::default()
            }
            .validate()
            .is_err()
        );
    }
}
