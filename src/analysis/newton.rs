//! Reusable, disposable Newton loads for DC and companion transient solves.
//!
//! C references: `niiter.c`, `niconv.c`, `cktop.c`. Step control follows
//! [`StepLimiting`]: by default ([`StepLimiting::Device`]) the phased solve
//! leaves it to the devices, which limit their own junction/FET voltages as
//! ngspice does (`DEVpnjlim`/`DEVfetlim`/`DEVlimvds`, see
//! [`crate::devices::limiting`]) and mark limited loads nonconvergent; the
//! port's earlier bounded global voltage-step damping remains available as
//! the [`StepLimiting::Global`] fallback policy and is what the unphased
//! [`solve`]/[`solve_counted`] apply (their loads cannot limit).
//! No trial or continuation stage invokes device acceptance hooks.
//!
//! [`solve_phased`] additionally tags each load with C's `MODEINITF` phase
//! ([`IterationPhase`], `niiter.c`), hands it the previous load's trial (C's
//! `CKTstate0` survives between iterations) and refuses to converge on a load
//! a device marked nonconvergent (C `CKTnoncon`), as the S/W switches need.
use crate::devices::{IterationPhase, TrialState};
use crate::maths::{SparseMatrix, Vector};
use crate::primitives::{Real, SpiceError, SpiceResult};

/// Largest accepted per-solve Newton iteration limit (`maxiter`, deck `itl1`).
pub const MAX_ITERATIONS: usize = 10_000;

/// Request names owned by [`crate::analysis::bias::ContinuationPolicy`], not by Newton.
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
/// (the default, [`crate::analysis::bias::ContinuationSchedule::Ngspice`]) or
/// `continuation=ladder` (the port's fixed ladders). Port-only.
pub const SCHEDULE_KEY: &str = "continuation";

/// Request key of C's `noopiter` (`noopiter=1` skips the direct Newton
/// attempt, `0` keeps it; [`crate::analysis::bias::ContinuationPolicy::skip_direct`]).
pub const SKIP_DIRECT_KEY: &str = "noopiter";

/// Request key of the ngspice schedule's adaptation base, C's raw
/// `CKTdcTrcvMaxIter` (deck `itl2` as written, default 50;
/// [`crate::analysis::bias::NgspiceStepping::adapt_iterations`]).
pub const ADAPT_ITERATIONS_KEY: &str = "adaptiter";

/// Request key selecting [`NewtonOptions::limiting`]: `limiting=device`
/// (ngspice's per-device junction limiting, the default) or `limiting=global`
/// (the port's bounded global voltage-step damping). A port-only key: C has
/// no such option.
pub const LIMITING_KEY: &str = "limiting";

/// Request key of the Newton limit per gmin/source-stepping stage (deck
/// `itl2`, C `CKTdcTrcvMaxIter` in `cktop.c`); see
/// [`crate::analysis::bias::ContinuationPolicy::stage_max_iterations`].
pub(crate) const STAGE_ITERATIONS_KEY: &str = "stagemaxiter";

/// How Newton bounds the step between iterates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepLimiting {
    /// ngspice's policy (`niiter.c` applies no global damping by default):
    /// in [`solve_phased`] the devices limit their own controlling voltages
    /// in every load ([`crate::devices::limiting`]) and the iterate is never
    /// damped. Nonlinear devices without limiting in C (behavioural sources,
    /// switches) take full steps, as in C.
    #[default]
    Device,
    /// The port's original bounded policy, kept as a fallback: devices load
    /// exactly at the iterate and the largest nodal step is scaled down to
    /// [`NewtonOptions::voltage_step`] (on the rows of
    /// `bias::limited_rows`). Loads for this policy must forbid
    /// device limiting ([`crate::devices::TrialState::with_device_limiting`]).
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
    /// Relative iterate tolerance (C `CKTreltol`).
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
    /// C `CKTdiagGmin`: the conductance gmin stepping adds to every
    /// internal diagonal element of SPARSE's frame before each factorization
    /// (`LoadGmin` in `SMPreorder`/`SMPluFac`); zero otherwise.
    pub diag_gmin: Real,
    /// C `MODEDC && CKThadNodeset`: the solve has nodesets, so `NIiter`'s
    /// `ipass` keeps the first `MODEINITFLOAT` pass after `MODEINITFIX`
    /// from converging.
    pub had_nodeset: bool,
}
impl Default for NewtonOptions {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            // ngspice's defaults (cktntask.c): CKTreltol, CKTvoltTol, CKTabstol.
            reltol: 1e-3,
            vntol: 1e-6,
            abstol: 1e-12,
            voltage_step: 0.2,
            limiting: StepLimiting::Device,
            diag_gmin: 0.,
            had_nodeset: false,
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
    /// [`crate::analysis::bias::DcSettings::from_request`], which resolves all of them.
    /// # Errors
    /// Invalid or unimplemented convergence arguments.
    pub fn from_request(request: &crate::analysis::AnalysisRequest) -> SpiceResult<Self> {
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
            let value = crate::primitives::parse_spice_number(text.trim())
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
                    return Err(crate::primitives::SpiceError::Unsupported {
                        feature: format!(
                            "continuation option {name} needs DcSettings::from_request, \
                             not NewtonOptions::from_request"
                        ),
                        location: None,
                    });
                }
                _ => {
                    return Err(crate::primitives::SpiceError::Unsupported {
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
            || !self.diag_gmin.is_finite()
            || self.diag_gmin < 0.
        {
            return Err(failure(
                "invalid Newton tolerances, voltage limit or iteration budget",
            ));
        }
        Ok(())
    }
}

/// A converged Newton solution and the trial of the load that produced it.
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
/// tolerances, as `NIiter` does (see [`solve_phased`]): no device convergence
/// tests, and the global voltage-step damping of [`StepLimiting::Global`].
/// The callback must not commit state.
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
    /// At the iteration limit: the iterate the last load used. `NIiter`
    /// returns `E_ITERLIM` before swapping `CKTrhs` and `CKTrhsOld`, so this is
    /// the `CKTrhsOld` the next solve (`dctran.c`'s retry) starts from.
    pub last_iterate: Option<Vector>,
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
/// through [`crate::devices::Device::limits_voltage_steps`]; see
/// [`crate::analysis::bias::limited_rows`].
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
    let mut last_iterate = None;
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
            initial_transient: false,
            nodesets: options.had_nodeset,
        },
        |x: &Vector, _, _: Option<&T>| load(x),
        |_: &Vector, _: &Vector, _: &T| Ok(false),
        &mut iterations,
        &mut last_iterate,
    )
    .map_err(|error| NewtonFailure {
        error,
        iterations,
        last_iterate,
    })
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
    /// The first transient timepoint (`MODEINITTRAN`): loaded as
    /// [`Self::Predicted`], but `NIiter` reorders on its first two passes.
    InitialTransient,
}

/// [`solve_counted`] for device trials with C's Newton phases.
///
/// `load(x, phase, previous)` must build its trial with
/// [`crate::devices::StateHistory::trial_in`]`(phase, previous)`. A load whose
/// trial [`TrialState::is_nonconvergent`] never ends the solve (C: `CKTnoncon`
/// set by a device). Otherwise `NIconvTest` compares successive iterates and
/// then calls `device_test(new, old, trial)`, the devices' convergence tests
/// (C `CKTconvTest`, see [`crate::devices::Circuit::convergence_test`]),
/// which returns `true` when a device has not converged. The mode sequence
/// and reordering are `NIiter`'s; see `iterate`. A `continued` trial (the converged trial
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
#[allow(clippy::too_many_arguments)]
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
    device_test: impl FnMut(&Vector, &Vector, &TrialState) -> SpiceResult<bool>,
) -> Result<NewtonSolution<TrialState>, NewtonFailure> {
    let mut iterations = 0;
    let mut last_iterate = None;
    let first = match (policy, &continued) {
        (_, Some(_)) => IterationPhase::Float,
        (PhasePolicy::OperatingPoint, None) => IterationPhase::Junction,
        (PhasePolicy::Predicted | PhasePolicy::InitialTransient, None) => IterationPhase::Predict,
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
            initial_transient: policy == PhasePolicy::InitialTransient,
            nodesets: options.had_nodeset,
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
        device_test,
        &mut iterations,
        &mut last_iterate,
    )
    .map_err(|error| NewtonFailure {
        error,
        iterations,
        last_iterate,
    })
}

struct Phases<T, F> {
    first: IterationPhase,
    previous: Option<T>,
    nonconvergent: F,
    /// Apply the global voltage-step damping ([`StepLimiting::Global`]).
    damped: bool,
    /// `MODEINITTRAN`: `NIiter` reorders on the first iteration and again
    /// after it.
    initial_transient: bool,
    /// `MODEDC` with nodesets (`CKThadNodeset`): the first `MODEINITFLOAT`
    /// iterate after `MODEINITFIX` never converges (`ipass`).
    nodesets: bool,
}

/// C `NIiter` (`niiter.c`, the SPARSE path) over disposable loads.
///
/// Each pass loads at the present iterate, factors the matrix of the
/// circuit (reordering under `NISHOULDREORDER`: every `MODEINITJCT` load,
/// the load after it, and the first two loads of `MODEINITTRAN`; otherwise
/// refactoring, which after a zero pivot reloads at the same iterate and
/// reorders) with `diag_gmin` on the diagonal, and solves. Past
/// `max_iterations` it fails. A load that set `CKTnoncon` (a device trial
/// marked nonconvergent) and the first pass never converge; otherwise
/// `NIconvTest` compares the new iterate with the previous one (`reltol`
/// with `vntol` for node rows and `abstol` for branch rows) and then runs the
/// devices' convergence tests (`device_test(new, old, trial)`, C
/// `CKTconvTest`). Mode changes follow `NIiter`: `MODEINITJCT` to
/// `MODEINITFIX`, which ends at the first converged pass, `MODEINITPRED`
/// (and `MODEINITTRAN`) to `MODEINITFLOAT`, where a converged pass returns
/// the iterate that pass loaded (C's `CKTrhsOld`, within the tolerances of
/// the new one) with that load's trial (C's `CKTstate0`); no further load
/// is made.
#[allow(clippy::too_many_arguments)]
fn iterate<T, F: Fn(&T) -> bool>(
    initial: &Vector,
    branch_rows: &[bool],
    limited_rows: Option<&[bool]>,
    options: &NewtonOptions,
    phases: Phases<T, F>,
    mut load: impl FnMut(&Vector, IterationPhase, Option<&T>) -> SpiceResult<(SparseMatrix, Vector, T)>,
    mut device_test: impl FnMut(&Vector, &Vector, &T) -> SpiceResult<bool>,
    started: &mut usize,
    last_iterate: &mut Option<Vector>,
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
    let mut ordering = None;
    let mut reorder = phases.initial_transient;
    let mut ipass = false;
    let mut iterno = 0;
    loop {
        let (mut matrix, rhs, trial) = load(&guess, phase, previous.as_ref())?;
        iterno += 1;
        *started = iterno;
        let flagged = (phases.nonconvergent)(&trial);
        check(&matrix, &rhs, n)?;
        matrix.fold_duplicates();
        if phase == IterationPhase::Junction {
            reorder = true;
        }
        let mut next = match linearised_solve(&matrix, &rhs, &mut ordering, reorder, options)? {
            crate::maths::NiStep::Solved(next) => {
                reorder = false;
                next
            }
            crate::maths::NiStep::ZeroPivot => {
                // E_SINGULAR from SMPluFac: NISHOULDREORDER and `continue`,
                // reloading at the same iterate.
                reorder = true;
                previous = Some(trial);
                continue;
            }
        };
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
        if iterno > options.max_iterations {
            *last_iterate = Some(guess);
            return Err(failure(format!(
                "Newton iteration limit ({}) reached",
                options.max_iterations
            )));
        }
        let mut noncon = if flagged || iterno == 1 {
            true
        } else {
            !converged(&next, &guess, branch_rows, options) || device_test(&next, &guess, &trial)?
        };
        match phase {
            IterationPhase::Float => {
                if phases.nodesets {
                    noncon |= ipass;
                    ipass = false;
                }
                if !noncon {
                    // NIiter returns before swapping CKTrhs and CKTrhsOld:
                    // the solution ngspice keeps (CKTdump writes CKTrhsOld,
                    // the next solve starts from it) is the iterate the last
                    // load used, the one CKTstate0 belongs to.
                    return Ok(NewtonSolution {
                        values: guess,
                        trial,
                        iterations: iterno,
                    });
                }
            }
            IterationPhase::Junction => {
                phase = IterationPhase::Fix;
                reorder = true;
            }
            IterationPhase::Fix => {
                if !noncon {
                    phase = IterationPhase::Float;
                }
                ipass = true;
            }
            IterationPhase::Predict => {
                if phases.initial_transient && iterno <= 1 {
                    reorder = true;
                }
                phase = IterationPhase::Float;
            }
        }
        previous = Some(trial);
        guess = next;
    }
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
/// One `NIiter` factor-and-solve on the raw matrix ([`crate::maths::solve_ni`]):
/// there is no row equilibration or conditioning guard; ngspice has neither.
fn linearised_solve(
    matrix: &SparseMatrix,
    rhs: &Vector,
    ordering: &mut Option<crate::maths::SparseSymbolic>,
    reorder: bool,
    options: &NewtonOptions,
) -> SpiceResult<crate::maths::NiStep> {
    crate::maths::solve_ni(matrix, rhs, ordering, reorder, options.diag_gmin)
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
    fn counted_solve_reports_the_iterations_used_by_success_and_failure() {
        let identity = |_: &Vector| -> SpiceResult<(SparseMatrix, Vector, ())> {
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, 1.)?;
            Ok((a, Vector::zeros(1), ()))
        };
        // x_{n+1} = x_n + 1 never converges.
        let drifting = |x: &Vector| -> SpiceResult<(SparseMatrix, Vector, ())> {
            let mut a = SparseMatrix::new(1, 1);
            a.add(0, 0, 1.)?;
            let mut b = Vector::zeros(1);
            b.add_to(0, x.as_slice()[0] + 1.)?;
            Ok((a, b, ()))
        };
        let far = Vector::zeros(1);
        let options = NewtonOptions {
            max_iterations: 3,
            ..NewtonOptions::default()
        };
        // NIiter fails after the solve that exceeds the limit: 3 + 1 passes.
        let failure = solve_counted(&far, &[false], &options, drifting).unwrap_err();
        assert_eq!(failure.iterations, 4);
        assert!(failure.error.to_string().contains("iteration limit"));
        assert_eq!(
            solve(&far, &[false], &options, drifting).unwrap_err(),
            failure.error
        );
        // MODEINITJCT, MODEINITFIX, then a converged MODEINITFLOAT pass.
        let solved = solve_counted(&Vector::zeros(1), &[false], &options, identity).unwrap();
        assert_eq!(solved.iterations, 3);
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
