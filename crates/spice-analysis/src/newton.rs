//! Reusable, disposable Newton loads for DC and companion transient solves.
//!
//! C references: `niiter.c`, `niconv.c`, `cktop.c`. Step control follows
//! [`StepLimiting`]: by default ([`StepLimiting::Device`]) the phased solve
//! leaves it to the devices, which limit their own junction/FET voltages as
//! ngspice does (`DEVpnjlim`/`DEVfetlim`/`DEVlimvds`, see
//! [`spice_devices::limiting`]) and mark limited loads nonconvergent; the
//! port's earlier bounded global voltage-step damping remains available as
//! the [`StepLimiting::Global`] fallback policy and is what the unphased
//! [`solve`]/[`solve_counted`] apply (their loads cannot limit).
//! No trial or continuation stage invokes device acceptance hooks.
//!
//! [`solve_phased`] additionally tags each load with C's `MODEINITF` phase
//! ([`IterationPhase`], `niiter.c`), hands it the previous load's trial (C's
//! `CKTstate0` survives between iterations) and refuses to converge on a load
//! a device marked nonconvergent (C `CKTnoncon`), as the S/W switches need.
use spice_core::{Real, SpiceError, SpiceResult};
use spice_devices::{IterationPhase, TrialState};
use spice_maths::{SparseMatrix, Vector};

/// Largest accepted per-solve Newton iteration limit (`maxiter`, deck `itl1`).
pub const MAX_ITERATIONS: usize = 10_000;

/// Request names owned by [`crate::bias::ContinuationPolicy`], not by Newton.
pub(crate) const CONTINUATION_KEYS: [&str; 7] = [
    "srcsteps",
    "gminsteps",
    "gminfactor",
    STAGE_ITERATIONS_KEY,
    SCHEDULE_KEY,
    SKIP_DIRECT_KEY,
    ADAPT_ITERATIONS_KEY,
];

/// Request key selecting the continuation family: `continuation=ngspice`
/// (the default, [`crate::bias::ContinuationSchedule::Ngspice`]) or
/// `continuation=ladder` (the port's fixed ladders). Port-only.
pub const SCHEDULE_KEY: &str = "continuation";

/// Request key of C's `noopiter` (`noopiter=1` skips the direct Newton
/// attempt, `0` keeps it; [`crate::bias::ContinuationPolicy::skip_direct`]).
pub const SKIP_DIRECT_KEY: &str = "noopiter";

/// Request key of the ngspice schedule's adaptation base, C's raw
/// `CKTdcTrcvMaxIter` (deck `itl2` as written, default 50;
/// [`crate::bias::NgspiceStepping::adapt_iterations`]).
pub const ADAPT_ITERATIONS_KEY: &str = "adaptiter";

/// Request key selecting [`NewtonOptions::limiting`]: `limiting=device`
/// (ngspice's per-device junction limiting, the default) or `limiting=global`
/// (the port's bounded global voltage-step damping). A port-only key: C has
/// no such option.
pub const LIMITING_KEY: &str = "limiting";

/// Request key of the Newton limit per gmin/source-stepping stage (deck
/// `itl2`, C `CKTdcTrcvMaxIter` in `cktop.c`); see
/// [`crate::bias::ContinuationPolicy::stage_max_iterations`].
pub(crate) const STAGE_ITERATIONS_KEY: &str = "stagemaxiter";

/// How Newton bounds the step between iterates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepLimiting {
    /// ngspice's policy (`niiter.c` applies no global damping by default):
    /// in [`solve_phased`] the devices limit their own controlling voltages
    /// in every load ([`spice_devices::limiting`]) and the iterate is never
    /// damped. Nonlinear devices without limiting in C (behavioural sources,
    /// switches) take full steps, as in C.
    #[default]
    Device,
    /// The port's original bounded policy, kept as a fallback: devices load
    /// exactly at the iterate and the largest nodal step is scaled down to
    /// [`NewtonOptions::voltage_step`] (on the rows of
    /// [`crate::bias::limited_rows`]). Loads for this policy must forbid
    /// device limiting ([`spice_devices::TrialState::with_device_limiting`]).
    Global,
}

impl StepLimiting {
    /// Whether devices limit their own voltages (trials allow it).
    #[must_use]
    pub const fn is_device(self) -> bool {
        matches!(self, Self::Device)
    }
}

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
    /// Maximum nodal voltage change per iteration (V) under
    /// [`StepLimiting::Global`] (and in the unphased [`solve`]); limits trial
    /// exponentials.
    pub voltage_step: Real,
    /// Step control of [`solve_phased`]; see [`StepLimiting`].
    pub limiting: StepLimiting,
}
impl Default for NewtonOptions {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            reltol: 1e-8,
            vntol: 1e-10,
            abstol: 1e-12,
            voltage_step: 0.2,
            limiting: StepLimiting::Device,
        }
    }
}
impl NewtonOptions {
    /// Resolve named `rtol`, `vntol`, `abstol`, `maxiter` arguments for DC/AC,
    /// plus the port-only [`LIMITING_KEY`] (`limiting=device|global`, see
    /// [`StepLimiting`]).
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
            if key == LIMITING_KEY {
                options.limiting = match text.trim().to_ascii_lowercase().as_str() {
                    "device" => StepLimiting::Device,
                    "global" => StepLimiting::Global,
                    other => {
                        return Err(failure(format!(
                            "{LIMITING_KEY} must be 'device' or 'global', not '{other}'"
                        )));
                    }
                };
                continue;
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
    solve_counted_limited(initial, branch_rows, None, options, load)
}

/// [`solve_counted`] whose voltage-step damping watches only the rows marked
/// in `limited_rows` (all non-branch rows when `None`). Devices that C never
/// limits, such as behavioural sources (`asrcload.c` has no limiting), opt out
/// through [`spice_devices::Device::limits_voltage_steps`]; see
/// [`crate::bias::limited_rows`].
///
/// # Errors
/// As [`solve_counted`], plus a `limited_rows` of the wrong length.
pub fn solve_counted_limited<T>(
    initial: &Vector,
    branch_rows: &[bool],
    limited_rows: Option<&[bool]>,
    options: &NewtonOptions,
    mut load: impl FnMut(&Vector) -> SpiceResult<(SparseMatrix, Vector, T)>,
) -> Result<NewtonSolution<T>, NewtonFailure> {
    let mut iterations = 0;
    iterate(
        initial,
        branch_rows,
        limited_rows,
        options,
        Phases {
            first: IterationPhase::Junction,
            previous: None,
            nonconvergent: |_: &T| false,
            damped: true,
        },
        |x: &Vector, _, _: Option<&T>| load(x),
        &mut iterations,
    )
    .map_err(|error| NewtonFailure { error, iterations })
}

/// How a phased solve starts (C `NIiter`'s initial `MODEINITF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhasePolicy {
    /// A DC operating point from scratch: `MODEINITJCT` for the first load,
    /// `MODEINITFIX` until the iterate first converges, then `MODEINITFLOAT`.
    OperatingPoint,
    /// A point continuing accepted history (a transient timepoint or a
    /// warm-started DC sweep point): `MODEINITPRED`, then `MODEINITFLOAT`.
    Predicted,
}

/// [`solve_counted`] for device trials with C's Newton phases.
///
/// `load(x, phase, previous)` must build its trial with
/// [`spice_devices::StateHistory::trial_in`]`(phase, previous)`. A load whose
/// trial [`TrialState::is_nonconvergent`] never ends the solve (C: `CKTnoncon`
/// set by a device). Convergence is checked, as in [`solve`], by reloading at
/// the new iterate, here in `MODEINITFLOAT` with the producing load as the
/// previous iterate; that reload must also be free of device nonconvergence.
/// Under [`PhasePolicy::OperatingPoint`] the first such check also ends the
/// `MODEINITFIX` phase, so the returned point was always checked under
/// `MODEINITFLOAT`, like `NIiter`. A `continued` trial (the converged trial
/// of a preceding continuation stage, as `cktop.c` continues its stepping
/// stages in `MODEINITFLOAT` with `CKTstate0` intact) starts the solve in
/// `MODEINITFLOAT` with that trial as the previous iterate instead. Devices
/// without discrete state load identically in every phase, so their solves
/// and iteration counts match [`solve_counted`].
///
/// `limited_rows` selects the rows watched by voltage-step damping, as in
/// [`solve_counted_limited`].
///
/// # Errors
/// As [`solve_counted_limited`].
pub fn solve_phased(
    initial: &Vector,
    branch_rows: &[bool],
    limited_rows: Option<&[bool]>,
    options: &NewtonOptions,
    policy: PhasePolicy,
    continued: Option<TrialState>,
    mut load: impl FnMut(
        &Vector,
        IterationPhase,
        Option<&TrialState>,
    ) -> SpiceResult<(SparseMatrix, Vector, TrialState)>,
) -> Result<NewtonSolution<TrialState>, NewtonFailure> {
    let mut iterations = 0;
    let first = match (policy, &continued) {
        (_, Some(_)) => IterationPhase::Float,
        (PhasePolicy::OperatingPoint, None) => IterationPhase::Junction,
        (PhasePolicy::Predicted, None) => IterationPhase::Predict,
    };
    iterate(
        initial,
        branch_rows,
        limited_rows,
        options,
        Phases {
            first,
            previous: continued,
            nonconvergent: TrialState::is_nonconvergent,
            damped: !options.limiting.is_device(),
        },
        |x: &Vector, phase, previous: Option<&TrialState>| {
            let loaded = load(x, phase, previous)?;
            if loaded.2.device_limiting() != options.limiting.is_device() {
                return Err(failure(
                    "Newton load trial does not match the step-limiting policy",
                ));
            }
            Ok(loaded)
        },
        &mut iterations,
    )
    .map_err(|error| NewtonFailure { error, iterations })
}

struct Phases<T, F> {
    first: IterationPhase,
    previous: Option<T>,
    nonconvergent: F,
    /// Apply the global voltage-step damping ([`StepLimiting::Global`]).
    damped: bool,
}

fn iterate<T, F: Fn(&T) -> bool>(
    initial: &Vector,
    branch_rows: &[bool],
    limited_rows: Option<&[bool]>,
    options: &NewtonOptions,
    phases: Phases<T, F>,
    mut load: impl FnMut(&Vector, IterationPhase, Option<&T>) -> SpiceResult<(SparseMatrix, Vector, T)>,
    started: &mut usize,
) -> SpiceResult<NewtonSolution<T>> {
    options.validate()?;
    let n = initial.len();
    if n == 0
        || n != branch_rows.len()
        || limited_rows.is_some_and(|rows| rows.len() != n)
        || !initial.is_finite()
    {
        return Err(failure(
            "Newton requires a nonempty square system, finite solution and matching row kinds",
        ));
    }
    let mut guess = initial.clone();
    let mut phase = phases.first;
    let mut previous = phases.previous;
    for iteration in 1..=options.max_iterations {
        *started = iteration;
        let (mut matrix, rhs, trial) = load(&guess, phase, previous.as_ref())?;
        let flagged = (phases.nonconvergent)(&trial);
        check(&matrix, &rhs, n)?;
        matrix.fold_duplicates();
        let mut next = linearised_solve(&matrix, &rhs)?;
        if phases.damped {
            damp(
                &mut next,
                &guess,
                branch_rows,
                limited_rows,
                options.voltage_step,
            );
        }
        if !next.is_finite() {
            return Err(failure("nonfinite Newton iterate"));
        }
        if !flagged && converged(&next, &guess, branch_rows, options) {
            let (mut physical, rhs, checked) = load(&next, IterationPhase::Float, Some(&trial))?;
            check(&physical, &rhs, n)?;
            physical.fold_duplicates();
            if !(phases.nonconvergent)(&checked)
                && residual_ok(&physical, &rhs, &next, branch_rows, options)?
            {
                return Ok(NewtonSolution {
                    values: next,
                    trial: checked,
                    iterations: iteration,
                });
            }
            // NIiter: an iterate converged under MODEINITFIX ends that phase.
            phase = IterationPhase::Float;
        } else {
            phase = match phase {
                IterationPhase::Junction | IterationPhase::Fix => IterationPhase::Fix,
                IterationPhase::Predict | IterationPhase::Float => IterationPhase::Float,
            };
        }
        previous = Some(trial);
        guess = next;
    }
    Err(failure(format!(
        "Newton iteration limit ({}) reached",
        options.max_iterations
    )))
}
/// [`StepLimiting::Global`]: scale the step so no watched nodal voltage moves
/// by more than `limit` (all non-branch rows when `limited_rows` is `None`).
fn damp(
    next: &mut Vector,
    guess: &Vector,
    branch_rows: &[bool],
    limited_rows: Option<&[bool]>,
    limit: Real,
) {
    let largest_voltage_step = next
        .as_slice()
        .iter()
        .zip(guess.as_slice())
        .zip(branch_rows)
        .enumerate()
        .filter(|(row, (_, branch))| !**branch && limited_rows.is_none_or(|limited| limited[*row]))
        .map(|(_, ((new, old), _))| (new - old).abs())
        .fold(0., Real::max);
    let damping = (limit / largest_voltage_step).min(1.);
    if damping < 1. {
        for (new, old) in next.as_mut_slice().iter_mut().zip(guess.as_slice()) {
            *new = old + damping * (*new - old);
        }
    }
}
/// Iterative-refinement rounds of the balanced fallback solve.
const REFINEMENT_STEPS: usize = 3;

/// Solves one Newton linearisation: row-equilibrated first (the established
/// path, unchanged for every system it accepts), then, only when that solve
/// fails numerically, once more with Curtis-Reid row/column balancing and
/// iterative refinement ([`spice_maths::EquilibratedSparseLu::new_balanced`],
/// [`spice_maths::EquilibratedSparseLu::solve_refined`]).
///
/// The fallback exists for linearisations that are well posed but scaled
/// across tens of decades through a coupling cycle. ngspice's `PTdivide`
/// fudge gives `1/v(x)`, `sqrt(v(x))` or `log(v(x))` a slope of about `1e32`
/// at a 0 V iterate (`src/spicelib/parser/ptfuncs.c`); SPARSE's threshold
/// pivoting factors that matrix, whereas row scaling alone leaves the
/// `v(out)` column `1e32` times weaker than its `v(in)` coupling and trips the
/// conditioning guard. Balancing changes no equation: the factor is checked
/// for rank/conditioning in scaled form and every solution is checked against
/// the original snapshot in physical units; refinement then recovers small
/// unknowns (a 2 V source beside a `-1e99` `log(0)` output) that the normwise
/// residual bound alone would let the solve lose. When both attempts fail, the
/// row-equilibrated error is reported, so diagnostics for genuinely singular
/// systems are unchanged.
fn linearised_solve(matrix: &SparseMatrix, rhs: &Vector) -> SpiceResult<Vector> {
    let (scaled, scaled_rhs) = equilibrated(matrix, rhs)?;
    match scaled.solve(&scaled_rhs) {
        Ok(solution) => Ok(solution),
        Err(error @ SpiceError::Numerical { .. }) => {
            spice_maths::EquilibratedSparseLu::new_balanced(matrix, None)
                .and_then(|factor| factor.solve_refined(rhs, REFINEMENT_STEPS))
                .map_err(|_| error)
        }
        Err(error) => Err(error),
    }
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
