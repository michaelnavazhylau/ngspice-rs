//! Nonlinear DC operating points with bounded continuation (`cktop.c`).
//!
//! The policy and its diagnostics are typed: [`DcSettings`] resolves Newton
//! limits and a [`ContinuationPolicy`] (gmin schedule, source schedule, total
//! work budget); [`solve_dc_with`] returns a [`DcReport`] of every attempt and
//! stage on success *and* failure. [`solve_dc`] is the compatible default
//! wrapper. See `docs/port/DC_CONTINUATION.md` for the contract and for the
//! deliberate differences from ngspice's dynamic gmin/source stepping.
use crate::newton::{self, NewtonFailure, NewtonOptions, NewtonSolution};
use spice_core::{Real, SpiceError, SpiceResult};
use spice_devices::{AnalysisMode, Circuit, LoadRequest, ModelContext, StateHistory, TrialState};
use spice_maths::{SparseMatrix, Vector};

/// Default nodal-gmin schedule (S): one decade per stage, 1e-3 down to 1e-12.
pub const DEFAULT_GMIN_SCHEDULE: [Real; 10] = [
    1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12,
];
/// First value (S) of a schedule built from `gminsteps`/`gminfactor`.
pub const GMIN_START: Real = 1e-3;
/// Default ratio between consecutive scheduled gmin values.
pub const DEFAULT_GMIN_FACTOR: Real = 10.;
/// Largest accepted `gminfactor`.
pub const MAX_GMIN_FACTOR: Real = 1e6;
/// Largest scheduled artificial nodal gmin (S).
pub const MAX_GMIN: Real = 1.;
/// Default number of equal source increments.
pub const DEFAULT_SOURCE_STEPS: usize = 20;
/// Default temporary nodal gmin (S) held during source stepping.
pub const DEFAULT_SOURCE_GMIN: Real = 1e-8;
/// Most gmin stages accepted in one schedule.
pub const MAX_GMIN_STAGES: usize = 100;
/// Most equal source increments accepted (a schedule has one more scale).
pub const MAX_SOURCE_STEPS: usize = 1_000;
/// Largest explicit total Newton-iteration budget.
pub const MAX_TOTAL_ITERATIONS: usize = 10_000_000;

fn invalid(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "DC settings".into(),
        message: message.into(),
    }
}

/// Source stepping: sources scaled through `scales`, held at a temporary gmin.
///
/// The scales only *approach* the solution; they never replace it. A strategy
/// always ends with an extra full-source, zero-gmin solve.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceStepping {
    /// Strictly increasing source multipliers within `[0, 1]`. Every
    /// independent source (including overrides and transient forcing) is
    /// multiplied by the scale; the stored source values never change.
    pub scales: Vec<Real>,
    /// Temporary uniform nodal gmin (S) in `[0, MAX_GMIN]` for these stages.
    pub gmin: Real,
}

impl SourceStepping {
    /// `steps` equal increments: scales `0, 1/steps, ..., 1`.
    ///
    /// # Errors
    /// `steps` outside `1..=MAX_SOURCE_STEPS` or an invalid `gmin`.
    pub fn uniform(steps: usize, gmin: Real) -> SpiceResult<Self> {
        if !(1..=MAX_SOURCE_STEPS).contains(&steps) {
            return Err(invalid(format!(
                "source steps must be in 1..={MAX_SOURCE_STEPS}, not {steps}"
            )));
        }
        let stepping = Self {
            scales: (0..=steps).map(|k| k as Real / steps as Real).collect(),
            gmin,
        };
        stepping.validate()?;
        Ok(stepping)
    }

    /// Check finite, increasing, bounded scales and a finite bounded gmin.
    ///
    /// # Errors
    /// Empty/oversized, nonfinite, out-of-range or non-increasing scales, or an
    /// invalid temporary gmin.
    pub fn validate(&self) -> SpiceResult<()> {
        if self.scales.is_empty() || self.scales.len() > MAX_SOURCE_STEPS + 1 {
            return Err(invalid(format!(
                "source schedule needs 1..={} scales, not {}",
                MAX_SOURCE_STEPS + 1,
                self.scales.len()
            )));
        }
        let mut previous = Real::NEG_INFINITY;
        for scale in &self.scales {
            if !scale.is_finite() || !(0. ..=1.).contains(scale) {
                return Err(invalid(format!(
                    "source scale {scale} must be finite and in [0, 1]"
                )));
            }
            if *scale <= previous {
                return Err(invalid(format!(
                    "source scales must strictly increase (no progress at {scale})"
                )));
            }
            previous = *scale;
        }
        if !self.gmin.is_finite() || !(0. ..=MAX_GMIN).contains(&self.gmin) {
            return Err(invalid(format!(
                "source-stepping gmin {} must be finite and in [0, {MAX_GMIN}] S",
                self.gmin
            )));
        }
        Ok(())
    }
}

/// Deterministic, bounded DC continuation used after direct Newton fails.
///
/// Order: direct Newton (not part of this policy), then gmin stepping, then
/// source stepping. Each strategy ends in a solve with **full sources and zero
/// artificial nodal gmin**; only that solve may be returned. Temporary stages
/// never touch device history, accept hooks or stored source values.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuationPolicy {
    /// Strictly decreasing artificial nodal gmin values (S) solved at full
    /// source before the final zero-gmin solve. Empty disables gmin stepping.
    pub gmin_schedule: Vec<Real>,
    /// Source schedule, or `None` to disable source stepping.
    pub source_stepping: Option<SourceStepping>,
    /// Total Newton iterations over *all* stages of one solve. `None` selects
    /// `stages * NewtonOptions::max_iterations`, i.e. no tighter than the
    /// per-stage limit already implies; `Some(n)` caps the work at `n`.
    pub max_total_iterations: Option<usize>,
}

impl Default for ContinuationPolicy {
    fn default() -> Self {
        Self {
            gmin_schedule: DEFAULT_GMIN_SCHEDULE.to_vec(),
            source_stepping: Some(SourceStepping {
                scales: (0..=DEFAULT_SOURCE_STEPS)
                    .map(|k| k as Real / DEFAULT_SOURCE_STEPS as Real)
                    .collect(),
                gmin: DEFAULT_SOURCE_GMIN,
            }),
            max_total_iterations: None,
        }
    }
}

impl ContinuationPolicy {
    /// No continuation: a failed direct Newton solve is final.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            gmin_schedule: Vec::new(),
            source_stepping: None,
            max_total_iterations: None,
        }
    }

    /// Geometric gmin schedule `start / factor^k`, `k = 0..steps`. `steps == 0`
    /// is an empty (disabled) schedule.
    ///
    /// # Errors
    /// Too many steps, an invalid `start`/`factor`, or a value that underflows.
    pub fn geometric_gmin(start: Real, factor: Real, steps: usize) -> SpiceResult<Vec<Real>> {
        check_factor(factor)?;
        if !(start.is_finite() && 0. < start && start <= MAX_GMIN) {
            return Err(invalid(format!(
                "gmin start must be finite and in (0, {MAX_GMIN}] S, not {start}"
            )));
        }
        if steps > MAX_GMIN_STAGES {
            return Err(invalid(format!(
                "gmin steps must be in 0..={MAX_GMIN_STAGES}, not {steps}"
            )));
        }
        // The default decade ladder is returned exactly, not recomputed.
        let ladder = start == GMIN_START
            && factor == DEFAULT_GMIN_FACTOR
            && steps <= DEFAULT_GMIN_SCHEDULE.len();
        if ladder {
            return Ok(DEFAULT_GMIN_SCHEDULE[..steps].to_vec());
        }
        let schedule: Vec<Real> = (0..steps)
            .map(|k| start / factor.powi(i32::try_from(k).unwrap_or(i32::MAX)))
            .collect();
        validate_gmin_schedule(&schedule)?;
        Ok(schedule)
    }

    /// Resolve `srcsteps`/`gminsteps`/`gminfactor`-style counts over the
    /// defaults. `None` keeps the default; `Some(0)` disables a strategy. A
    /// `gminfactor` without `gminsteps` rebuilds the default number of stages.
    ///
    /// # Errors
    /// Out-of-range counts, an invalid factor or an underflowing schedule.
    pub fn from_steps(
        source_steps: Option<usize>,
        gmin_steps: Option<usize>,
        gmin_factor: Option<Real>,
    ) -> SpiceResult<Self> {
        let mut policy = Self::default();
        if let Some(steps) = source_steps {
            policy.source_stepping = (steps > 0)
                .then(|| SourceStepping::uniform(steps, DEFAULT_SOURCE_GMIN))
                .transpose()?;
        }
        if gmin_steps.is_some() || gmin_factor.is_some() {
            policy.gmin_schedule = Self::geometric_gmin(
                GMIN_START,
                gmin_factor.unwrap_or(DEFAULT_GMIN_FACTOR),
                gmin_steps.unwrap_or(DEFAULT_GMIN_SCHEDULE.len()),
            )?;
        }
        policy.validate()?;
        Ok(policy)
    }

    /// Validate every schedule and budget before the first load.
    ///
    /// # Errors
    /// Nonfinite/non-positive/non-decreasing gmin values, invalid source
    /// scales, or an out-of-range total iteration budget.
    pub fn validate(&self) -> SpiceResult<()> {
        validate_gmin_schedule(&self.gmin_schedule)?;
        if let Some(source) = &self.source_stepping {
            source.validate()?;
        }
        if let Some(total) = self.max_total_iterations
            && !(1..=MAX_TOTAL_ITERATIONS).contains(&total)
        {
            return Err(invalid(format!(
                "total iteration budget must be in 1..={MAX_TOTAL_ITERATIONS}, not {total}"
            )));
        }
        Ok(())
    }
}

fn check_factor(factor: Real) -> SpiceResult<()> {
    if factor.is_finite() && factor > 1. && factor <= MAX_GMIN_FACTOR {
        Ok(())
    } else {
        Err(invalid(format!(
            "gminfactor must be finite and in (1, {MAX_GMIN_FACTOR}], not {factor}"
        )))
    }
}

fn validate_gmin_schedule(schedule: &[Real]) -> SpiceResult<()> {
    if schedule.len() > MAX_GMIN_STAGES {
        return Err(invalid(format!(
            "gmin schedule has {} stages; at most {MAX_GMIN_STAGES}",
            schedule.len()
        )));
    }
    let mut previous = Real::INFINITY;
    for gmin in schedule {
        if !gmin.is_finite() || *gmin <= 0. || *gmin > MAX_GMIN {
            return Err(invalid(format!(
                "scheduled gmin {gmin:e} must be finite and in (0, {MAX_GMIN}] S"
            )));
        }
        if *gmin >= previous {
            return Err(invalid(format!(
                "gmin schedule must strictly decrease (no progress at {gmin:e})"
            )));
        }
        previous = *gmin;
    }
    Ok(())
}

/// Newton limits plus continuation policy for one DC solve.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DcSettings {
    /// Per-stage Newton tolerances and iteration limit.
    pub newton: NewtonOptions,
    /// Continuation used after direct Newton fails.
    pub continuation: ContinuationPolicy,
}

impl DcSettings {
    /// Validate both halves.
    ///
    /// # Errors
    /// Invalid Newton options or continuation policy.
    pub fn validate(&self) -> SpiceResult<()> {
        self.newton.validate()?;
        self.continuation.validate()
    }

    /// Resolve named request arguments over the defaults: `rtol`, `vntol`,
    /// `abstol`, `maxiter` (see [`NewtonOptions::from_request`]) plus
    /// `srcsteps` (`0` disables, else equal increments), `gminsteps` (`0`
    /// disables, else decade-ratio stages from 1e-3 S) and `gminfactor`.
    /// Unknown, duplicate, nonfinite and out-of-range values fail.
    ///
    /// # Errors
    /// Invalid, duplicate or unimplemented arguments.
    pub fn from_request(request: &crate::AnalysisRequest) -> SpiceResult<Self> {
        let mut forwarded = Vec::new();
        let (mut source, mut gmin_steps, mut gmin_factor) = (None, None, None);
        let mut seen = std::collections::BTreeSet::new();
        for argument in &request.arguments {
            let Some((key, text)) = argument.split_once('=') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            if !newton::CONTINUATION_KEYS.contains(&key.as_str()) {
                forwarded.push(argument.clone());
                continue;
            }
            if !seen.insert(key.clone()) {
                return Err(invalid(format!("duplicate continuation option {key}")));
            }
            let value = spice_core::parse_spice_number(text.trim())
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid(format!("nonfinite/nonliteral option {key}")))?;
            if key == "gminfactor" {
                gmin_factor = Some(value);
                continue;
            }
            let limit = if key == "srcsteps" {
                MAX_SOURCE_STEPS
            } else {
                MAX_GMIN_STAGES
            };
            if value.fract() != 0. || !(0. ..=limit as Real).contains(&value) {
                return Err(invalid(format!(
                    "option {key} must be an integer in 0..={limit}, not {text}"
                )));
            }
            if key == "srcsteps" {
                source = Some(value as usize);
            } else {
                gmin_steps = Some(value as usize);
            }
        }
        Ok(Self {
            newton: NewtonOptions::from_request(&crate::AnalysisRequest::with_arguments(
                request.kind,
                forwarded,
            ))?,
            continuation: ContinuationPolicy::from_steps(source, gmin_steps, gmin_factor)?,
        })
    }
}

/// How a DC solve was attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DcStrategy {
    /// Plain Newton from the seed at full sources and zero artificial gmin.
    Direct,
    /// Decreasing artificial nodal gmin at full sources.
    GminStepping,
    /// Increasing source scale with a temporary nodal gmin.
    SourceStepping,
}

impl DcStrategy {
    /// Name used in diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct Newton",
            Self::GminStepping => "gmin stepping",
            Self::SourceStepping => "source stepping",
        }
    }
}

/// One Newton solve (continuation stage).
#[derive(Debug, Clone, PartialEq)]
pub struct DcStage {
    /// The strategy this stage belongs to.
    pub strategy: DcStrategy,
    /// Source multiplier used (`1` is full sources).
    pub source_scale: Real,
    /// Artificial uniform nodal gmin (S) added to non-branch rows. This is
    /// *not* the device's fixed junction gmin, which is always present.
    pub gmin: Real,
    /// Newton iterations spent, charged against the total budget.
    pub iterations: usize,
    /// `None` if the stage converged, else the failure message.
    pub error: Option<String>,
}

impl DcStage {
    /// Whether the stage converged.
    #[must_use]
    pub const fn converged(&self) -> bool {
        self.error.is_none()
    }

    /// Whether this stage solved the *original* equations: full sources, zero
    /// artificial gmin. Only such a stage's solution is ever returned.
    #[must_use]
    pub fn is_unregularized(&self) -> bool {
        self.source_scale == 1. && self.gmin == 0.
    }
}

/// Result of one strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// The strategy's final unregularized solve converged.
    Converged,
    /// A stage (or the final solve) failed.
    Failed,
    /// Reached but configured off (empty gmin schedule / no source stepping).
    Disabled,
}

/// One strategy attempt, in the order tried.
#[derive(Debug, Clone, PartialEq)]
pub struct DcAttempt {
    /// Which strategy.
    pub strategy: DcStrategy,
    /// How it ended.
    pub outcome: AttemptOutcome,
    /// For failures, the stage and error; empty otherwise.
    pub detail: String,
}

/// Final classification of a solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DcOutcome {
    /// Rejected before any Newton solve (invalid settings, seed or overrides).
    #[default]
    Rejected,
    /// Converged through the given strategy.
    Converged(DcStrategy),
    /// A structural, device or other non-numerical failure; never retried.
    NonRetryableFailure,
    /// Direct Newton and every enabled strategy failed numerically.
    Exhausted,
    /// The total iteration budget ran out first.
    BudgetExhausted,
}

/// Diagnostics for one DC solve, returned on success and failure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DcReport {
    /// Strategies in the order they were reached.
    pub attempts: Vec<DcAttempt>,
    /// Every Newton solve performed, in order.
    pub stages: Vec<DcStage>,
    /// Newton iterations spent over all stages (`<= budget`).
    pub total_iterations: usize,
    /// Effective total iteration budget.
    pub budget: usize,
    /// How the solve ended.
    pub outcome: DcOutcome,
}

impl DcReport {
    /// One-line, human-readable account of the attempts and the work spent.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self
            .attempts
            .iter()
            .map(|attempt| {
                let name = attempt.strategy.name();
                match attempt.outcome {
                    AttemptOutcome::Converged => format!("{name}: converged"),
                    AttemptOutcome::Failed => format!("{name}: failed ({})", attempt.detail),
                    AttemptOutcome::Disabled => format!("{name}: disabled"),
                }
            })
            .collect();
        parts.push(format!(
            "{} of {} budgeted Newton iterations used",
            self.total_iterations, self.budget
        ));
        parts.join("; ")
    }
}

/// A successful DC solve and how it was reached.
#[derive(Debug)]
pub struct DcSolution {
    /// The solution of the original (full-source, zero-artificial-gmin) system.
    pub solution: NewtonSolution<TrialState>,
    /// Attempts and stages that produced it.
    pub report: DcReport,
}

/// A failed DC solve with the diagnostics collected before it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct DcFailure {
    /// The error. Device/structural failures are reported unchanged; an
    /// exhausted continuation is a [`SpiceError::Numerical`] naming every attempt.
    pub error: SpiceError,
    /// Attempts and stages before the failure (boxed to keep `Result` small).
    pub report: Box<DcReport>,
}

impl std::fmt::Display for DcFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for DcFailure {}

impl From<DcFailure> for SpiceError {
    fn from(failure: DcFailure) -> Self {
        failure.error
    }
}

/// Row kinds shared by DC and companion Newton iteration.
pub(crate) fn branch_rows(circuit: &Circuit) -> Vec<bool> {
    let mut kinds = vec![false; circuit.unknown_count()];
    for i in 0..circuit.device_count() {
        for row in circuit.branch_rows(i).unwrap_or(0..0) {
            kinds[row] = true;
        }
    }
    kinds
}

/// Solve a finalized circuit without committing device history or accept hooks,
/// using the default [`ContinuationPolicy`]. Compatible wrapper over
/// [`solve_dc_with`] that drops the report.
///
/// # Errors
/// Invalid inputs, device/structural failures, singular final system, or bounded
/// convergence failure. Temporary continuation solutions are never accepted.
pub fn solve_dc(
    circuit: &Circuit,
    context: &ModelContext,
    options: &NewtonOptions,
    overrides: &[(&str, f64)],
    initial: Option<&Vector>,
    forcing: Option<&Vector>,
) -> SpiceResult<NewtonSolution<TrialState>> {
    let settings = DcSettings {
        newton: *options,
        continuation: ContinuationPolicy::default(),
    };
    solve_dc_with(circuit, context, &settings, overrides, initial, forcing)
        .map(|solved| solved.solution)
        .map_err(SpiceError::from)
}

/// Solve a finalized circuit under explicit [`DcSettings`], reporting attempts.
///
/// Independent source overrides are typed, checked, and applied to a temporary
/// RHS only. `forcing` replaces source DC values for the transient bias at 0-.
/// Direct Newton runs first. Only a [`SpiceError::Numerical`] failure starts
/// continuation (gmin stepping, then source stepping, as enabled and within the
/// total iteration budget); any other error is returned at once, unretried.
/// Every success is a solve with **zero artificial nodal gmin and full source
/// values**, so continuation never changes the requested physical solution. The
/// device's own junction gmin is separate and always present.
///
/// # Errors
/// [`DcFailure`] carrying the error and report: invalid settings/inputs,
/// device/structural failures, or bounded convergence failure.
pub fn solve_dc_with(
    circuit: &Circuit,
    context: &ModelContext,
    settings: &DcSettings,
    overrides: &[(&str, f64)],
    initial: Option<&Vector>,
    forcing: Option<&Vector>,
) -> Result<DcSolution, DcFailure> {
    let mut report = DcReport::default();
    match run(
        circuit,
        context,
        settings,
        overrides,
        initial,
        forcing,
        &mut report,
    ) {
        Ok(solution) => Ok(DcSolution { solution, report }),
        Err(error) => Err(DcFailure {
            error,
            report: Box::new(report),
        }),
    }
}

/// A stage failure, classified for the strategy driver.
enum Halt {
    /// Numerical failure: the next strategy may still succeed.
    Retry(String),
    /// Anything else (structural, device, unsupported): never masked by retries.
    Fatal(SpiceError),
    /// The total iteration budget ended the attempt.
    Budget(String),
}

struct Engine<'a> {
    circuit: &'a Circuit,
    context: &'a ModelContext,
    history: StateHistory,
    branches: Vec<bool>,
    target: Vector,
    original: Vector,
    newton: NewtonOptions,
    remaining: usize,
}

type Stages<'a> = &'a [(Real, Real)];

impl Engine<'_> {
    /// One disposable Newton solve at `(scale, gmin)`; charges the budget.
    fn stage(
        &mut self,
        report: &mut DcReport,
        strategy: DcStrategy,
        guess: &Vector,
        (scale, gmin): (Real, Real),
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        if self.remaining == 0 {
            return Err(Halt::Budget(format!(
                "total iteration budget ({}) exhausted before scale {scale}, gmin {gmin:e} S",
                report.budget
            )));
        }
        let n = self.circuit.unknown_count();
        let options = NewtonOptions {
            max_iterations: self.newton.max_iterations.min(self.remaining),
            ..self.newton
        };
        let reduced = options.max_iterations < self.newton.max_iterations;
        let result = newton::solve_counted(guess, &self.branches, &options, |x| {
            let mut a = SparseMatrix::new(n, n);
            let mut b = Vector::zeros(n);
            let mut trial = self.history.trial();
            self.circuit.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: x,
                    model_context: self.context,
                    integration: None,
                    history: &self.history,
                    forcing: None,
                },
                &mut a,
                &mut b,
                &mut trial,
            )?;
            for (row, branch) in self.branches.iter().enumerate() {
                b.add_to(
                    row,
                    scale * self.target.as_slice()[row] - self.original.as_slice()[row],
                )?;
                if !branch && gmin > 0. {
                    a.add(row, row, gmin)?;
                }
            }
            Ok((a, b, trial))
        });
        let (iterations, error) = match &result {
            Ok(solved) => (solved.iterations, None),
            Err(NewtonFailure { error, iterations }) => (*iterations, Some(error)),
        };
        self.remaining = self.remaining.saturating_sub(iterations);
        report.total_iterations += iterations;
        report.stages.push(DcStage {
            strategy,
            source_scale: scale,
            gmin,
            iterations,
            error: error.map(ToString::to_string),
        });
        // A failure that used the whole budget-reduced allowance may only have
        // been cut short by the budget, so it ends the attempt.
        let cut_short = reduced && iterations >= options.max_iterations;
        match result {
            Ok(solved) => Ok(solved),
            Err(NewtonFailure {
                error: error @ SpiceError::Numerical { .. },
                ..
            }) => {
                let detail = format!("scale {scale}, gmin {gmin:e} S: {error}");
                Err(if cut_short {
                    Halt::Budget(format!("{detail} (iteration limit reduced by the budget)"))
                } else {
                    Halt::Retry(detail)
                })
            }
            Err(NewtonFailure { error, .. }) => Err(Halt::Fatal(error)),
        }
    }

    /// Walk `stages` from `start`, then the final full-source zero-gmin solve.
    fn walk(
        &mut self,
        report: &mut DcReport,
        strategy: DcStrategy,
        start: &Vector,
        stages: Stages<'_>,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let mut guess = start.clone();
        for &point in stages {
            guess = self.stage(report, strategy, &guess, point)?.values;
        }
        self.stage(report, strategy, &guess, (1., 0.))
    }

    /// [`Self::walk`] plus its entry in the report's attempt list.
    fn attempt(
        &mut self,
        report: &mut DcReport,
        strategy: DcStrategy,
        start: &Vector,
        stages: Stages<'_>,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let result = self.walk(report, strategy, start, stages);
        let (outcome, detail) = match &result {
            Ok(_) => (AttemptOutcome::Converged, String::new()),
            Err(Halt::Retry(detail) | Halt::Budget(detail)) => {
                (AttemptOutcome::Failed, detail.clone())
            }
            Err(Halt::Fatal(error)) => (AttemptOutcome::Failed, error.to_string()),
        };
        report.attempts.push(DcAttempt {
            strategy,
            outcome,
            detail,
        });
        result
    }
}

fn run(
    circuit: &Circuit,
    context: &ModelContext,
    settings: &DcSettings,
    overrides: &[(&str, f64)],
    initial: Option<&Vector>,
    forcing: Option<&Vector>,
    report: &mut DcReport,
) -> SpiceResult<NewtonSolution<TrialState>> {
    settings.validate()?;
    let options = &settings.newton;
    let policy = &settings.continuation;
    let n = circuit.unknown_count();
    if initial.is_some_and(|x| x.len() != n || !x.is_finite()) {
        return Err(SpiceError::circuit("invalid DC initial solution"));
    }
    if forcing.is_some() && !overrides.is_empty() {
        return Err(SpiceError::circuit(
            "combine neither source overrides nor transient-bias forcing",
        ));
    }
    let zero = Vector::zeros(n);
    let system = circuit.small_signal_system(context, &zero)?;
    let original = system.dc_rhs(None)?;
    let mut target = forcing.cloned().unwrap_or_else(|| original.clone());
    if target.len() != n || !target.is_finite() {
        return Err(SpiceError::circuit("invalid DC forcing"));
    }
    let mut names = std::collections::BTreeSet::new();
    for (name, value) in overrides {
        if !value.is_finite() || !names.insert(name.to_ascii_lowercase()) {
            return Err(SpiceError::circuit(
                "nonfinite or duplicate DC source override",
            ));
        }
        let source = system
            .sources
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                SpiceError::circuit(format!("DC target {name} is not an independent source"))
            })?;
        for (row, sign) in &source.rows {
            target.add_to(*row, sign * (value - source.dc))?;
        }
    }
    let history = circuit.state_history();
    // Preserve exact linear solving (no nonlinear damping or continuation).
    if !circuit.devices().iter().any(|device| device.is_nonlinear()) {
        let result = (|| {
            let values = system.a.solve(&target)?;
            let mut trial = history.trial();
            circuit.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: &values,
                    model_context: context,
                    integration: None,
                    history: &history,
                    forcing: None,
                },
                &mut SparseMatrix::new(n, n),
                &mut Vector::zeros(n),
                &mut trial,
            )?;
            Ok(NewtonSolution {
                values,
                trial,
                iterations: 1,
            })
        })();
        let error = result.as_ref().err().map(ToString::to_string);
        report.budget = 1;
        report.total_iterations = 1;
        report.stages.push(DcStage {
            strategy: DcStrategy::Direct,
            source_scale: 1.,
            gmin: 0.,
            iterations: 1,
            error: error.clone(),
        });
        report.attempts.push(DcAttempt {
            strategy: DcStrategy::Direct,
            outcome: if error.is_none() {
                AttemptOutcome::Converged
            } else {
                AttemptOutcome::Failed
            },
            detail: error.unwrap_or_default(),
        });
        report.outcome = if result.is_ok() {
            DcOutcome::Converged(DcStrategy::Direct)
        } else {
            // An exact linear failure is not rescued with fictitious gmin.
            DcOutcome::NonRetryableFailure
        };
        return result;
    }
    let gmin_stages: Vec<(Real, Real)> = policy.gmin_schedule.iter().map(|g| (1., *g)).collect();
    let source_stages: Vec<(Real, Real)> = policy
        .source_stepping
        .as_ref()
        .map(|s| s.scales.iter().map(|k| (*k, s.gmin)).collect())
        .unwrap_or_default();
    let gmin_enabled = !gmin_stages.is_empty();
    let source_enabled = policy.source_stepping.is_some();
    let stage_count =
        1 + if gmin_enabled {
            gmin_stages.len() + 1
        } else {
            0
        } + if source_enabled {
            source_stages.len() + 1
        } else {
            0
        };
    let budget = policy
        .max_total_iterations
        .unwrap_or_else(|| stage_count.saturating_mul(options.max_iterations));
    report.budget = budget;
    let mut engine = Engine {
        circuit,
        context,
        history,
        branches: branch_rows(circuit),
        target,
        original,
        newton: *options,
        remaining: budget,
    };
    let no_stages: Stages<'_> = &[];
    let seed = initial.unwrap_or(&zero);
    // Source stepping restarts from the all-sources-off state, not the seed.
    let plan = [
        (DcStrategy::Direct, seed, Some(no_stages)),
        (
            DcStrategy::GminStepping,
            seed,
            gmin_enabled.then_some(&gmin_stages[..]),
        ),
        (
            DcStrategy::SourceStepping,
            &zero,
            source_enabled.then_some(&source_stages[..]),
        ),
    ];
    for (strategy, start, stages) in plan {
        let Some(stages) = stages else {
            report.attempts.push(DcAttempt {
                strategy,
                outcome: AttemptOutcome::Disabled,
                detail: String::new(),
            });
            continue;
        };
        match engine.attempt(report, strategy, start, stages) {
            Ok(solved) => {
                report.outcome = DcOutcome::Converged(strategy);
                return Ok(solved);
            }
            Err(Halt::Retry(_)) => {}
            Err(Halt::Fatal(error)) => {
                report.outcome = DcOutcome::NonRetryableFailure;
                return Err(error);
            }
            Err(Halt::Budget(_)) => {
                report.outcome = DcOutcome::BudgetExhausted;
                return Err(SpiceError::Numerical {
                    context: "DC continuation".into(),
                    message: format!(
                        "total iteration budget exhausted: {}; raise maxiter or \
                         max_total_iterations, or simplify the circuit",
                        report.summary()
                    ),
                });
            }
        }
    }
    report.outcome = DcOutcome::Exhausted;
    Err(SpiceError::Numerical {
        context: "DC continuation".into(),
        message: format!(
            "direct Newton and every enabled continuation strategy failed: {}",
            report.summary()
        ),
    })
}
