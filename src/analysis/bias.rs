//! Nonlinear DC operating points with bounded continuation (`cktop.c`).
//!
//! The policy and its diagnostics are typed: [`DcSettings`] resolves Newton
//! limits and a [`ContinuationPolicy`] (ngspice's adaptive `CKTop`
//! strategies by default, or the port's fixed gmin/source ladders, plus a
//! total work budget); [`solve_dc_with`] returns a [`DcReport`] of every
//! attempt and stage on success *and* failure. [`solve_dc`] is the compatible
//! default wrapper. See `docs/port/DC_CONTINUATION.md` for the contract and
//! for the remaining differences from ngspice.
use crate::analysis::newton::{self, NewtonFailure, NewtonOptions, NewtonSolution, PhasePolicy};
use crate::devices::{
    AnalysisMode, Circuit, IterationPhase, LoadRequest, ModelContext, StateHistory, TrialState,
};
use crate::maths::{SparseMatrix, Vector};
use crate::primitives::{Real, SpiceError, SpiceResult};

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

/// C's default `CKTdcTrcvMaxIter` as written (`cktntask.c`, deck `itl2`):
/// the adaptive ngspice strategies compare each stage's iterations with a
/// quarter and three quarters of it (`iters <= itl2 / 4`).
pub const NGSPICE_ADAPT_ITERATIONS: usize = 50;
/// Most stages one adaptive ngspice strategy may run. A port-only bound: C
/// stops only at its step-size floors, which can take far longer.
pub const MAX_ADAPTIVE_STAGES: usize = 1_000;
/// `dynamic_gmin`/`new_gmin` start from `OldGmin = 1e-2` S divided by the factor.
const DYNAMIC_GMIN_START: Real = 1e-2;
/// `dynamic_gmin` gives up once a failed step's factor is below this, and
/// never shrinks the factor below it after a slow stage.
const DYNAMIC_FACTOR_FLOOR: Real = 1.00005;
/// `new_gmin` never shrinks its factor below 3 after a slow stage.
const TRUE_GMIN_SLOW_FACTOR: Real = 3.;
/// `gillespie_src`: first source increment, smallest increment, increment cap
/// after a failure and smallest progress worth retrying.
const GILLESPIE_FIRST_RAISE: Real = 1e-3;
const GILLESPIE_MIN_RAISE: Real = 1e-7;
const GILLESPIE_MAX_RAISE: Real = 0.01;
const GILLESPIE_MIN_PROGRESS: Real = 1e-8;
/// `gillespie_src`'s zero-source gmin ladder spans ten decades above `gmin`.
const GILLESPIE_GMIN_DECADES: i32 = 10;

/// ngspice's `CKTop` continuation (`cktop.c`), parameterised like C.
///
/// After direct Newton fails (unless [`ContinuationPolicy::skip_direct`]):
///
/// * gmin stepping by [`Self::gmin_steps`]: `1` runs `dynamic_gmin` (an
///   adaptive artificial diagonal gmin from `1e-2 / gminfactor` S down to the
///   junction `gmin`, then a zero-gmin solve) and, if that fails, `new_gmin`
///   (the same adaptive walk applied to the *junction* gmin of every device);
///   `n > 1` runs `spice3_gmin`, `n + 1` stages of artificial gmin from
///   `gmin * gminfactor^n` down to `gmin`, then a zero-gmin solve; `0`
///   disables it.
/// * source stepping by [`Self::source_steps`]: `1` runs `gillespie_src`
///   (adaptive source factor from 0, with a ten-decade gmin ladder if the
///   zero-source solve fails); `n > 1` runs `spice3_src`, `n + 1` equal
///   source scales with no artificial gmin; `0` disables it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NgspiceStepping {
    /// C `CKTnumGminSteps` (deck `gminsteps`, default 1).
    pub gmin_steps: usize,
    /// C `CKTnumSrcSteps` (deck `srcsteps`/`itl6`, default 1).
    pub source_steps: usize,
    /// C `CKTgminFactor` (deck `gminfactor`, default 10).
    pub gmin_factor: Real,
    /// C `CKTdcTrcvMaxIter` as written (deck `itl2`, default
    /// [`NGSPICE_ADAPT_ITERATIONS`]), which steers the adaptive steps. The
    /// stages' Newton *limit* is [`ContinuationPolicy::stage_max_iterations`].
    pub adapt_iterations: usize,
}

impl Default for NgspiceStepping {
    fn default() -> Self {
        Self {
            gmin_steps: 1,
            source_steps: 1,
            gmin_factor: DEFAULT_GMIN_FACTOR,
            adapt_iterations: NGSPICE_ADAPT_ITERATIONS,
        }
    }
}

impl NgspiceStepping {
    /// Check the counts, factor and adaptation base.
    ///
    /// # Errors
    /// Counts above [`MAX_GMIN_STAGES`]/[`MAX_SOURCE_STEPS`], an invalid
    /// factor or an adaptation base outside `1..=MAX_ITERATIONS`.
    pub fn validate(&self) -> SpiceResult<()> {
        check_factor(self.gmin_factor)?;
        if self.gmin_steps > MAX_GMIN_STAGES {
            return Err(invalid(format!(
                "gmin steps must be in 0..={MAX_GMIN_STAGES}, not {}",
                self.gmin_steps
            )));
        }
        if self.source_steps > MAX_SOURCE_STEPS {
            return Err(invalid(format!(
                "source steps must be in 0..={MAX_SOURCE_STEPS}, not {}",
                self.source_steps
            )));
        }
        if !(1..=newton::MAX_ITERATIONS).contains(&self.adapt_iterations) {
            return Err(invalid(format!(
                "adaptation iteration base must be in 1..={}, not {}",
                newton::MAX_ITERATIONS,
                self.adapt_iterations
            )));
        }
        Ok(())
    }

    /// `spice3_gmin`'s artificial gmin ladder for junction gmin `gmin`:
    /// `gmin * factor^steps` down to `gmin`, `steps + 1` values.
    ///
    /// # Errors
    /// A ladder that is not finite, positive, at most [`MAX_GMIN`] and
    /// strictly decreasing (e.g. a zero junction gmin).
    pub fn spice3_ladder(&self, gmin: Real) -> SpiceResult<Vec<Real>> {
        let mut value = gmin;
        for _ in 0..self.gmin_steps {
            value *= self.gmin_factor;
        }
        let mut ladder = Vec::with_capacity(self.gmin_steps + 1);
        for _ in 0..=self.gmin_steps {
            ladder.push(value);
            value /= self.gmin_factor;
        }
        validate_gmin_schedule(&ladder).map_err(|_| {
            invalid(format!(
                "gminsteps={} with gminfactor={} and gmin={gmin:e} S gives the ladder \
                 {:e}..{gmin:e} S; each value must be finite, positive, at most \
                 {MAX_GMIN} S and decreasing",
                self.gmin_steps, self.gmin_factor, ladder[0]
            ))
        })?;
        Ok(ladder)
    }
}

/// Which family of continuation strategies a [`ContinuationPolicy`] runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContinuationSchedule {
    /// ngspice's adaptive `CKTop` strategies ([`NgspiceStepping`]); the
    /// default.
    Ngspice(NgspiceStepping),
    /// The port's original fixed, deterministic ladders
    /// ([`ContinuationPolicy::gmin_schedule`],
    /// [`ContinuationPolicy::source_stepping`]).
    Ladder,
}

/// Bounded DC continuation used after direct Newton fails.
///
/// Order: direct Newton (not part of this policy), then gmin stepping, then
/// source stepping, as [`Self::schedule`] defines them. Each strategy ends in
/// a solve with **full sources, zero artificial nodal gmin and the configured
/// junction gmin**; only that solve may be returned. Temporary stages never
/// touch device history, accept hooks or stored source values.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuationPolicy {
    /// Strategy family. [`ContinuationSchedule::Ladder`] uses the two ladder
    /// fields below; [`ContinuationSchedule::Ngspice`] requires them empty.
    pub schedule: ContinuationSchedule,
    /// Ladder only: strictly decreasing artificial nodal gmin values (S)
    /// solved at full source before the final zero-gmin solve. Empty disables
    /// gmin stepping.
    pub gmin_schedule: Vec<Real>,
    /// Ladder only: source schedule, or `None` to disable source stepping.
    pub source_stepping: Option<SourceStepping>,
    /// Total Newton iterations over *all* stages of one solve. `None` selects
    /// `NewtonOptions::max_iterations` for the direct solve plus the stage
    /// limit for every continuation stage, i.e. no tighter than the per-stage
    /// limits already imply; `Some(n)` caps the work at `n`.
    pub max_total_iterations: Option<usize>,
    /// Newton iteration limit of every gmin/source-stepping stage, including
    /// source stepping's final full-source solve (C: `NIiter(ckt,
    /// CKTdcTrcvMaxIter)`, deck `itl2`). Gmin stepping's closing zero-gmin
    /// solve is not a stage: like the direct solve it uses
    /// `NewtonOptions::max_iterations` (C: `dynamic_gmin`, `spice3_gmin` and
    /// `new_gmin` end with `NIiter(ckt, iterlim)`, i.e. `CKTdcMaxIter`, deck
    /// `itl1`). `None` keeps `NewtonOptions::max_iterations` for the stages too.
    pub stage_max_iterations: Option<usize>,
    /// Skip the direct Newton attempt and start with continuation (C
    /// `CKTnoOpIter`, deck `.options noopiter`).
    pub skip_direct: bool,
}

/// The ngspice schedule with C's defaults (`gminsteps=1`, `srcsteps=1`,
/// `gminfactor=10`, `itl2=50`): `dynamic_gmin`, `new_gmin`, `gillespie_src`.
impl Default for ContinuationPolicy {
    fn default() -> Self {
        Self::ngspice(NgspiceStepping::default())
    }
}

impl ContinuationPolicy {
    /// ngspice's strategies with the given parameters.
    #[must_use]
    pub fn ngspice(stepping: NgspiceStepping) -> Self {
        Self {
            schedule: ContinuationSchedule::Ngspice(stepping),
            ..Self::disabled()
        }
    }

    /// The port's original fixed ladders: nodal gmin 1e-3 down to 1e-12 S one
    /// decade per stage, then twenty equal source increments at 1e-8 S.
    #[must_use]
    pub fn ladder() -> Self {
        Self {
            gmin_schedule: DEFAULT_GMIN_SCHEDULE.to_vec(),
            source_stepping: Some(SourceStepping {
                scales: (0..=DEFAULT_SOURCE_STEPS)
                    .map(|k| k as Real / DEFAULT_SOURCE_STEPS as Real)
                    .collect(),
                gmin: DEFAULT_SOURCE_GMIN,
            }),
            ..Self::disabled()
        }
    }

    /// No continuation: a failed direct Newton solve is final.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            schedule: ContinuationSchedule::Ladder,
            gmin_schedule: Vec::new(),
            source_stepping: None,
            max_total_iterations: None,
            stage_max_iterations: None,
            skip_direct: false,
        }
    }

    /// The ngspice schedule from C's step counts over C's defaults: `None`
    /// keeps a default, `Some(0)` disables a strategy (see
    /// [`NgspiceStepping`]).
    ///
    /// # Errors
    /// Out-of-range counts or an invalid factor/adaptation base.
    pub fn from_ngspice_steps(
        source_steps: Option<usize>,
        gmin_steps: Option<usize>,
        gmin_factor: Option<Real>,
        adapt_iterations: Option<usize>,
    ) -> SpiceResult<Self> {
        let defaults = NgspiceStepping::default();
        let policy = Self::ngspice(NgspiceStepping {
            gmin_steps: gmin_steps.unwrap_or(defaults.gmin_steps),
            source_steps: source_steps.unwrap_or(defaults.source_steps),
            gmin_factor: gmin_factor.unwrap_or(defaults.gmin_factor),
            adapt_iterations: adapt_iterations.unwrap_or(defaults.adapt_iterations),
        });
        policy.validate()?;
        Ok(policy)
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

    /// The ladder schedule from `srcsteps`/`gminsteps`/`gminfactor`-style
    /// counts over the ladder defaults ([`Self::ladder`]). `None` keeps the
    /// default; `Some(0)` disables a strategy. A `gminfactor` without
    /// `gminsteps` rebuilds the default number of stages.
    ///
    /// # Errors
    /// Out-of-range counts, an invalid factor or an underflowing schedule.
    pub fn from_steps(
        source_steps: Option<usize>,
        gmin_steps: Option<usize>,
        gmin_factor: Option<Real>,
    ) -> SpiceResult<Self> {
        let mut policy = Self::ladder();
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
        if let ContinuationSchedule::Ngspice(stepping) = &self.schedule {
            stepping.validate()?;
            if !self.gmin_schedule.is_empty() || self.source_stepping.is_some() {
                return Err(invalid(
                    "gmin_schedule/source_stepping are ladder settings; the ngspice schedule \
                     takes its steps from NgspiceStepping",
                ));
            }
        }
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
        if let Some(limit) = self.stage_max_iterations
            && !(1..=newton::MAX_ITERATIONS).contains(&limit)
        {
            return Err(invalid(format!(
                "continuation stage iteration limit must be in 1..={}, not {limit}",
                newton::MAX_ITERATIONS
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
    /// `abstol`, `maxiter`, `limiting` (see [`NewtonOptions::from_request`])
    /// plus the continuation keys: `continuation=ngspice|ladder` (default
    /// `ngspice`), `srcsteps`, `gminsteps`, `gminfactor` (C's meaning under
    /// `ngspice`, see [`NgspiceStepping`]; the ladder meaning under `ladder`:
    /// `srcsteps` equal increments, `gminsteps` decade-ratio stages from 1e-3
    /// S), `stagemaxiter`, `adaptiter` (ngspice only) and `noopiter` (`0` or
    /// `1`). Unknown, duplicate, nonfinite and out-of-range values fail.
    ///
    /// # Errors
    /// Invalid, duplicate or unimplemented arguments.
    pub fn from_request(request: &crate::analysis::AnalysisRequest) -> SpiceResult<Self> {
        let mut forwarded = Vec::new();
        let (mut source, mut gmin_steps, mut gmin_factor) = (None, None, None);
        let (mut stage_limit, mut adapt, mut skip_direct, mut ladder) = (None, None, false, false);
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
            if key == newton::SCHEDULE_KEY {
                ladder = match text.trim().to_ascii_lowercase().as_str() {
                    "ngspice" => false,
                    "ladder" => true,
                    other => {
                        return Err(invalid(format!(
                            "{key} must be 'ngspice' or 'ladder', not '{other}'"
                        )));
                    }
                };
                continue;
            }
            let value = crate::primitives::parse_spice_number(text.trim())
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid(format!("nonfinite/nonliteral option {key}")))?;
            if key == "gminfactor" {
                gmin_factor = Some(value);
                continue;
            }
            let (lowest, limit) = match key.as_str() {
                "srcsteps" => (0, MAX_SOURCE_STEPS),
                newton::STAGE_ITERATIONS_KEY | newton::ADAPT_ITERATIONS_KEY => {
                    (1, newton::MAX_ITERATIONS)
                }
                newton::SKIP_DIRECT_KEY => (0, 1),
                _ => (0, MAX_GMIN_STAGES),
            };
            if value.fract() != 0. || !(lowest as Real..=limit as Real).contains(&value) {
                return Err(invalid(format!(
                    "option {key} must be an integer in {lowest}..={limit}, not {text}"
                )));
            }
            let count = Some(value as usize);
            match key.as_str() {
                newton::STAGE_ITERATIONS_KEY => stage_limit = count,
                newton::ADAPT_ITERATIONS_KEY => adapt = count,
                newton::SKIP_DIRECT_KEY => skip_direct = value == 1.,
                "srcsteps" => source = count,
                _ => gmin_steps = count,
            }
        }
        let mut continuation = if ladder {
            if adapt.is_some() {
                return Err(invalid(format!(
                    "{} steers the ngspice schedule only, not continuation=ladder",
                    newton::ADAPT_ITERATIONS_KEY
                )));
            }
            ContinuationPolicy::from_steps(source, gmin_steps, gmin_factor)?
        } else {
            ContinuationPolicy::from_ngspice_steps(source, gmin_steps, gmin_factor, adapt)?
        };
        continuation.stage_max_iterations = stage_limit;
        continuation.skip_direct = skip_direct;
        Ok(Self {
            newton: NewtonOptions::from_request(
                &crate::analysis::AnalysisRequest::with_arguments(request.kind, forwarded),
            )?,
            continuation,
        })
    }
}

/// How a DC solve was attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DcStrategy {
    /// Plain Newton from the seed at full sources and zero artificial gmin.
    Direct,
    /// Decreasing artificial nodal gmin at full sources (the ladder,
    /// `dynamic_gmin` or `spice3_gmin`).
    GminStepping,
    /// Decreasing *junction* gmin of every device at full sources (ngspice
    /// `new_gmin`, "true gmin stepping").
    JunctionGminStepping,
    /// Increasing source scale (the ladder with a temporary nodal gmin,
    /// `gillespie_src` or `spice3_src`).
    SourceStepping,
}

impl DcStrategy {
    /// Name used in diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct Newton",
            Self::GminStepping => "gmin stepping",
            Self::JunctionGminStepping => "true gmin stepping",
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
    /// *not* the device's junction gmin, which is always present.
    pub gmin: Real,
    /// The junction gmin (S) of every device in this stage when true gmin
    /// stepping replaced the configured one; `None` for the configured value.
    pub junction_gmin: Option<Real>,
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
    /// artificial gmin and the configured junction gmin. Only such a stage's
    /// solution is ever returned.
    #[must_use]
    pub fn is_unregularized(&self) -> bool {
        self.source_scale == 1. && self.gmin == 0. && self.junction_gmin.is_none()
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
/// Rows Newton's global voltage-step damping watches, or `None` for every
/// non-branch row (the historical policy, kept whenever no device opts out).
///
/// When some nonlinear device does not want limiting
/// ([`crate::devices::Device::limits_voltage_steps`], behavioural sources),
/// only the node rows of the nonlinear devices that do (junctions) are
/// watched, so a large but exact step at a behavioural output is not damped
/// to 0.2 V per iteration.
pub(crate) fn limited_rows(circuit: &Circuit) -> Option<Vec<bool>> {
    let devices = circuit.devices();
    if !devices
        .iter()
        .any(|device| device.is_nonlinear() && !device.limits_voltage_steps())
    {
        return None;
    }
    let mut rows = vec![false; circuit.unknown_count()];
    for device in devices
        .iter()
        .filter(|device| device.is_nonlinear() && device.limits_voltage_steps())
    {
        for terminal in device.terminals() {
            if let Some(row) = circuit.unknowns().node_row(*terminal) {
                rows[row] = true;
            }
        }
    }
    Some(rows)
}

pub(crate) fn branch_rows(circuit: &Circuit) -> Vec<bool> {
    let mut kinds = vec![false; circuit.unknown_count()];
    for i in 0..circuit.device_count() {
        for row in circuit.branch_rows(i).unwrap_or(0..0) {
            kinds[row] = true;
        }
    }
    kinds
}

/// Node rows a DC solve forces in its loads, the exact form of `cktload.c`'s
/// `.nodeset`/`.ic` stamping: each listed row's equation is replaced by
/// `x[row] = value * scale`, where `scale` is the source-stepping factor
/// (C multiplies both by `CKTsrcFact`).
///
/// * [`Self::initial`]: `.ic` rows, forced in **every** load of the solve
///   (C: `MODETRANOP` without `MODEUIC`, the transient operating point).
/// * [`Self::nodesets`]: `.nodeset` rows, forced only in the
///   `MODEINITJCT`/`MODEINITFIX` loads ([`crate::devices::IterationPhase`]
///   `Junction`/`Fix`) and released afterwards, so they select a solution
///   without changing it. A row in both lists is forced to its `.ic` value.
///
/// Rows whose voltage ideal sources already fix must have been removed
/// (C's `1e10` compromise for such rows is not reproduced; see
/// `crate::analysis::initial`). Linear circuits ignore the nodesets (one solution).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeForcing {
    /// `(row, value)` pairs of the `.ic` constraints.
    pub initial: Vec<(usize, Real)>,
    /// `(row, value)` pairs of the `.nodeset` hints.
    pub nodesets: Vec<(usize, Real)>,
}

impl NodeForcing {
    /// No forced rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.initial.is_empty() && self.nodesets.is_empty()
    }

    fn validate(&self, n: usize) -> SpiceResult<()> {
        if self
            .initial
            .iter()
            .chain(&self.nodesets)
            .any(|(row, value)| *row >= n || !value.is_finite())
        {
            return Err(SpiceError::circuit("invalid forced DC node row"));
        }
        Ok(())
    }

    /// Replaces the forced rows of `a x = b` (see the type docs); `nodesets`
    /// selects whether the `.nodeset` rows are forced in this load.
    fn apply(
        &self,
        a: &mut SparseMatrix,
        b: &mut Vector,
        scale: Real,
        nodesets: bool,
    ) -> SpiceResult<()> {
        let mut rows = std::collections::BTreeMap::new();
        if nodesets {
            rows.extend(self.nodesets.iter().copied());
        }
        rows.extend(self.initial.iter().copied());
        if rows.is_empty() {
            return Ok(());
        }
        let mut forced = SparseMatrix::new(a.rows(), a.cols());
        for t in a.triplets() {
            if !rows.contains_key(&t.row) {
                forced.add(t.row, t.col, t.value)?;
            }
        }
        for (row, value) in rows {
            forced.add(row, row, 1.)?;
            b.as_mut_slice()[row] = value * scale;
        }
        *a = forced;
        Ok(())
    }
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
    solve_dc_forced(
        circuit,
        context,
        settings,
        overrides,
        initial,
        forcing,
        &NodeForcing::default(),
    )
}

/// [`solve_dc_with`] with `.ic`/`.nodeset` row forcing ([`NodeForcing`]) in
/// every Newton load of every strategy. The returned point satisfies the
/// forced `.ic` rows and the physical equations of every other row; the
/// `.nodeset` rows are released before convergence.
///
/// # Errors
/// As [`solve_dc_with`], plus a forced row outside the system.
pub fn solve_dc_forced(
    circuit: &Circuit,
    context: &ModelContext,
    settings: &DcSettings,
    overrides: &[(&str, f64)],
    initial: Option<&Vector>,
    forcing: Option<&Vector>,
    nodes: &NodeForcing,
) -> Result<DcSolution, DcFailure> {
    solve(
        circuit,
        context,
        settings,
        overrides,
        Start {
            initial,
            forcing,
            history: &circuit.state_history(),
            policy: PhasePolicy::OperatingPoint,
            nodes,
        },
    )
}

/// [`solve_dc_forced`] continuing an accepted state `history` (C `CKTstate1..`),
/// starting the direct Newton attempt in `policy`'s phase: a DC sweep point
/// after the first runs [`PhasePolicy::Predicted`] (`dctrcurv.c` sets
/// `MODEINITPRED`). Gmin/source-stepping strategies always restart in
/// [`PhasePolicy::OperatingPoint`] against the same history (`CKTop` with
/// `MODEINITJCT`; the rotated states are not cleared).
/// Only devices with discrete state (switches) read the history; nothing is
/// committed to it.
///
/// # Errors
/// As [`solve_dc_with`], or a history that does not match the circuit.
#[allow(clippy::too_many_arguments)]
pub fn solve_dc_from(
    circuit: &Circuit,
    context: &ModelContext,
    settings: &DcSettings,
    overrides: &[(&str, f64)],
    initial: Option<&Vector>,
    history: &StateHistory,
    policy: PhasePolicy,
    nodes: &NodeForcing,
) -> Result<DcSolution, DcFailure> {
    solve(
        circuit,
        context,
        settings,
        overrides,
        Start {
            initial,
            forcing: None,
            history,
            policy,
            nodes,
        },
    )
}

/// Where a DC solve starts: seed, transient-bias forcing, accepted history and
/// the direct attempt's Newton phase policy.
struct Start<'a> {
    initial: Option<&'a Vector>,
    forcing: Option<&'a Vector>,
    history: &'a StateHistory,
    policy: PhasePolicy,
    nodes: &'a NodeForcing,
}

fn solve(
    circuit: &Circuit,
    context: &ModelContext,
    settings: &DcSettings,
    overrides: &[(&str, f64)],
    start: Start<'_>,
) -> Result<DcSolution, DcFailure> {
    let mut report = DcReport::default();
    match run(circuit, context, settings, overrides, start, &mut report) {
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

/// How one continuation stage modifies the original equations.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    /// Source multiplier (C `CKTsrcFact`).
    scale: Real,
    /// Artificial nodal gmin on non-branch rows (C `CKTdiagGmin`).
    gmin: Real,
    /// Replacement junction gmin of every device (C `CKTgmin` in `new_gmin`).
    junction: Option<Real>,
}

impl Point {
    /// Full sources, no artificial gmin, the configured junction gmin.
    const ORIGINAL: Self = Self::nodal(1., 0.);

    const fn nodal(scale: Real, gmin: Real) -> Self {
        Self {
            scale,
            gmin,
            junction: None,
        }
    }
}

struct Engine<'a> {
    circuit: &'a Circuit,
    context: &'a ModelContext,
    history: &'a StateHistory,
    policy: PhasePolicy,
    branches: Vec<bool>,
    /// Rows watched by Newton's voltage-step damping ([`limited_rows`]).
    limited: Option<Vec<bool>>,
    target: Vector,
    original: Vector,
    nodes: &'a NodeForcing,
    newton: NewtonOptions,
    /// Per-stage limit of the gmin/source-stepping strategies.
    stage_limit: usize,
    remaining: usize,
}

/// One planned strategy.
enum Plan<'p> {
    /// Configured off.
    Disabled,
    /// Fixed stages from `start`, then the closing original solve.
    Walk {
        start: &'p Vector,
        stages: Vec<Point>,
    },
    /// `dynamic_gmin` (artificial gmin) or `new_gmin` (`junction`).
    Dynamic {
        stepping: NgspiceStepping,
        junction: bool,
    },
    /// `gillespie_src`.
    Gillespie(NgspiceStepping),
    /// `spice3_src` with this many equal increments.
    Uniform(usize),
}

impl Engine<'_> {
    /// One disposable Newton solve at `point`; charges the budget.
    /// `continued` is the preceding stage's converged trial, if any; `closing`
    /// marks a gmin strategy's final solve of the original equations.
    fn stage(
        &mut self,
        report: &mut DcReport,
        strategy: DcStrategy,
        guess: &Vector,
        continued: Option<TrialState>,
        point: Point,
        closing: bool,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let Point {
            scale,
            gmin,
            junction,
        } = point;
        let describe = || match junction {
            Some(junction) => format!("scale {scale}, junction gmin {junction:e} S"),
            None => format!("scale {scale}, gmin {gmin:e} S"),
        };
        if self.remaining == 0 {
            return Err(Halt::Budget(format!(
                "total iteration budget ({}) exhausted before {}",
                report.budget,
                describe()
            )));
        }
        let n = self.circuit.unknown_count();
        // C bounds the direct solve and the gmin strategies' closing solve by
        // `iterlim` (itl1) and every other continuation solve by itl2
        // (`cktop.c`: `spice3_gmin`/`dynamic_gmin`/`new_gmin` end with
        // `NIiter(ckt, iterlim)`; `gillespie_src`/`spice3_src` ignore it).
        let limit = match strategy {
            DcStrategy::Direct => self.newton.max_iterations,
            DcStrategy::GminStepping | DcStrategy::JunctionGminStepping if closing => {
                self.newton.max_iterations
            }
            _ => self.stage_limit,
        };
        let options = NewtonOptions {
            max_iterations: limit.min(self.remaining),
            ..self.newton
        };
        let reduced = options.max_iterations < limit;
        let history = self.history;
        let device_limiting = options.limiting.is_device();
        let (reltol, abstol) = (options.reltol, options.abstol);
        let context = junction.map_or(*self.context, |junction| self.context.with_gmin(junction));
        let result = newton::solve_phased(
            guess,
            &self.branches,
            self.limited.as_deref(),
            &options,
            // Continuation strategies restart like `CKTop` (MODEINITJCT); only
            // the direct attempt may continue a predicted point.
            if strategy == DcStrategy::Direct {
                self.policy
            } else {
                PhasePolicy::OperatingPoint
            },
            continued,
            |x, phase, previous| {
                let mut a = SparseMatrix::new(n, n);
                let mut b = Vector::zeros(n);
                let mut trial = history
                    .trial_in(phase, previous)?
                    .with_device_limiting(device_limiting)
                    .with_convergence_tolerances(reltol, abstol);
                self.circuit.load(
                    &LoadRequest {
                        mode: AnalysisMode::OperatingPoint,
                        solution: x,
                        model_context: &context,
                        integration: None,
                        history,
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
                // cktload.c: .nodeset rows in MODEINITJCT/MODEINITFIX only,
                // .ic rows in every load, both scaled by CKTsrcFact.
                let hinted = matches!(phase, IterationPhase::Junction | IterationPhase::Fix);
                self.nodes.apply(&mut a, &mut b, scale, hinted)?;
                Ok((a, b, trial))
            },
        );
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
            junction_gmin: junction,
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
                let detail = format!("{}: {error}", describe());
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
        stages: &[Point],
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let mut guess = start.clone();
        let mut continued = None;
        for &point in stages {
            let solved = self.stage(report, strategy, &guess, continued, point, false)?;
            guess = solved.values;
            continued = Some(solved.trial);
        }
        self.stage(report, strategy, &guess, continued, Point::ORIGINAL, true)
    }

    /// A stage continuing `from` (a converged stage), or restarting from the
    /// zero vector in `MODEINITJCT` without one (C zeroes `CKTrhsOld` and
    /// `CKTstate0` before its adaptive strategies).
    fn step(
        &mut self,
        report: &mut DcReport,
        strategy: DcStrategy,
        from: Option<&NewtonSolution<TrialState>>,
        point: Point,
        closing: bool,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        match from {
            Some(from) => self.stage(
                report,
                strategy,
                &from.values,
                Some(from.trial.clone()),
                point,
                closing,
            ),
            None => {
                let zero = Vector::zeros(self.circuit.unknown_count());
                self.stage(report, strategy, &zero, None, point, closing)
            }
        }
    }

    /// `dynamic_gmin` (artificial nodal gmin) or, with `junction`, `new_gmin`
    /// (the devices' junction gmin), both from `1e-2 / gminfactor` S down to
    /// the configured junction gmin with C's adaptive factor, then the
    /// original equations at the direct limit.
    fn dynamic_gmin(
        &mut self,
        report: &mut DcReport,
        stepping: NgspiceStepping,
        junction: bool,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let strategy = if junction {
            DcStrategy::JunctionGminStepping
        } else {
            DcStrategy::GminStepping
        };
        // gtarget = MAX(CKTgmin, CKTgshunt); gshunt is not ported (0).
        let target = self.context.gmin;
        let slow_floor = if junction {
            TRUE_GMIN_SLOW_FACTOR
        } else {
            DYNAMIC_FACTOR_FLOOR
        };
        let (quick, slow) = (
            stepping.adapt_iterations / 4,
            3 * stepping.adapt_iterations / 4,
        );
        let mut factor = stepping.gmin_factor;
        let mut previous = DYNAMIC_GMIN_START;
        let mut gmin = previous / factor;
        let mut saved: Option<NewtonSolution<TrialState>> = None;
        for _ in 0..MAX_ADAPTIVE_STAGES {
            let point = if junction {
                Point {
                    scale: 1.,
                    gmin: 0.,
                    junction: Some(gmin),
                }
            } else {
                Point::nodal(1., gmin)
            };
            match self.step(report, strategy, saved.as_ref(), point, false) {
                Ok(solved) => {
                    let iterations = solved.iterations;
                    saved = Some(solved);
                    if gmin <= target {
                        let last = saved.as_ref();
                        return self.step(report, strategy, last, Point::ORIGINAL, true);
                    }
                    if iterations <= quick {
                        factor = (factor * factor.sqrt()).min(stepping.gmin_factor);
                    }
                    if iterations > slow {
                        factor = factor.sqrt().max(slow_floor);
                    }
                    previous = gmin;
                    if gmin < factor * target {
                        factor = gmin / target;
                        gmin = target;
                    } else {
                        gmin /= factor;
                    }
                }
                Err(Halt::Retry(detail)) => {
                    if factor < DYNAMIC_FACTOR_FLOOR {
                        return Err(Halt::Retry(format!("last step failed ({detail})")));
                    }
                    factor = factor.sqrt().sqrt();
                    gmin = previous / factor;
                }
                Err(halt) => return Err(halt),
            }
        }
        Err(Halt::Retry(format!(
            "stopped after {MAX_ADAPTIVE_STAGES} adaptive stages at {} {gmin:e} S",
            if junction { "junction gmin" } else { "gmin" }
        )))
    }

    /// `gillespie_src`: the circuit with every source off (with a ten-decade
    /// artificial gmin ladder from `gmin * 1e10` if that fails), then C's
    /// adaptive source factor up to full sources.
    fn gillespie(
        &mut self,
        report: &mut DcReport,
        stepping: NgspiceStepping,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let strategy = DcStrategy::SourceStepping;
        let mut saved = match self.step(report, strategy, None, Point::nodal(0., 0.), false) {
            Ok(solved) => solved,
            Err(Halt::Retry(_)) => {
                // diagGmin = (gshunt <= 0 ? gmin : gshunt) * 10^10, then / 10.
                let mut gmin = self.context.gmin;
                for _ in 0..GILLESPIE_GMIN_DECADES {
                    gmin *= 10.;
                }
                let mut last = None;
                for _ in 0..=GILLESPIE_GMIN_DECADES {
                    last = Some(self.step(
                        report,
                        strategy,
                        last.as_ref(),
                        Point::nodal(0., gmin),
                        false,
                    )?);
                    gmin /= 10.;
                }
                last.ok_or_else(|| Halt::Retry("empty zero-source gmin ladder".into()))?
            }
            Err(halt) => return Err(halt),
        };
        let (quick, slow) = (
            stepping.adapt_iterations / 4,
            3 * stepping.adapt_iterations / 4,
        );
        let mut converged: Real = 0.;
        let mut raise = GILLESPIE_FIRST_RAISE;
        let mut scale = converged + raise;
        for _ in 0..MAX_ADAPTIVE_STAGES {
            match self.step(
                report,
                strategy,
                Some(&saved),
                Point::nodal(scale, 0.),
                false,
            ) {
                Ok(solved) => {
                    let iterations = solved.iterations;
                    converged = scale;
                    saved = solved;
                    scale = converged + raise;
                    if iterations <= quick {
                        raise *= 1.5;
                    }
                    if iterations > slow {
                        raise *= 0.5;
                    }
                }
                Err(Halt::Retry(detail)) => {
                    if scale - converged < GILLESPIE_MIN_PROGRESS {
                        return Err(Halt::Retry(format!(
                            "stalled at source scale {converged} ({detail})"
                        )));
                    }
                    raise = (raise / 10.).min(GILLESPIE_MAX_RAISE);
                    // C retries the last converged factor before raising it again.
                    scale = converged;
                }
                Err(halt) => return Err(halt),
            }
            scale = scale.min(1.);
            if converged >= 1. {
                return Ok(saved);
            }
            if raise < GILLESPIE_MIN_RAISE {
                return Err(Halt::Retry(format!(
                    "source increment fell below {GILLESPIE_MIN_RAISE:e} at scale {converged}"
                )));
            }
        }
        Err(Halt::Retry(format!(
            "stopped after {MAX_ADAPTIVE_STAGES} adaptive stages at source scale {converged}"
        )))
    }

    /// `spice3_src`: `steps + 1` equal source scales `i / steps` without
    /// artificial gmin; the last stage solves the original equations.
    fn uniform_sources(
        &mut self,
        report: &mut DcReport,
        steps: usize,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let mut last: Option<NewtonSolution<TrialState>> = None;
        for step in 0..=steps {
            let scale = step as Real / steps as Real;
            last = Some(self.step(
                report,
                DcStrategy::SourceStepping,
                last.as_ref(),
                Point::nodal(scale, 0.),
                false,
            )?);
        }
        last.ok_or_else(|| Halt::Retry("empty source schedule".into()))
    }

    /// Run one planned strategy and record its entry in the attempt list.
    fn attempt(
        &mut self,
        report: &mut DcReport,
        strategy: DcStrategy,
        plan: &Plan<'_>,
    ) -> Result<NewtonSolution<TrialState>, Halt> {
        let result = match plan {
            Plan::Disabled => Err(Halt::Retry(String::new())),
            Plan::Walk { start, stages } => self.walk(report, strategy, start, stages),
            Plan::Dynamic { stepping, junction } => self.dynamic_gmin(report, *stepping, *junction),
            Plan::Gillespie(stepping) => self.gillespie(report, *stepping),
            Plan::Uniform(steps) => self.uniform_sources(report, *steps),
        };
        let (outcome, detail) = match &result {
            Ok(_) => (AttemptOutcome::Converged, String::new()),
            Err(_) if matches!(plan, Plan::Disabled) => (AttemptOutcome::Disabled, String::new()),
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
    start: Start<'_>,
    report: &mut DcReport,
) -> SpiceResult<NewtonSolution<TrialState>> {
    let Start {
        initial,
        forcing,
        history,
        policy: phases,
        nodes,
    } = start;
    settings.validate()?;
    nodes.validate(circuit.unknown_count())?;
    if !settings.newton.limiting.is_device()
        && let Some(device) = circuit.devices().iter().find(|d| d.has_start_settings())
    {
        return Err(SpiceError::Unsupported {
            feature: format!(
                "{}: `off`/MOS1 `ic=` start voltages need ngspice's device limiting \
                 (limiting=device, the default), not limiting=global",
                device.name()
            ),
            location: None,
        });
    }
    if history.len() != circuit.state_len() {
        return Err(SpiceError::circuit(
            "DC state history does not match the circuit numbering",
        ));
    }
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
    // Preserve exact linear solving (no nonlinear damping or continuation).
    if !circuit.devices().iter().any(|device| device.is_nonlinear()) {
        let result = (|| {
            // A linear point is unique: nodesets cannot change it, .ic rows can.
            let values = if nodes.initial.is_empty() {
                system.a.solve(&target)?
            } else {
                let (mut a, mut b) = (system.a.clone(), target.clone());
                let initial = NodeForcing {
                    initial: nodes.initial.clone(),
                    nodesets: Vec::new(),
                };
                initial.apply(&mut a, &mut b, 1., false)?;
                a.fold_duplicates();
                a.solve(&b)?
            };
            let mut trial = history.trial();
            circuit.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: &values,
                    model_context: context,
                    integration: None,
                    history,
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
            junction_gmin: None,
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
    let seed = initial.unwrap_or(&zero);
    let stage_limit = policy
        .stage_max_iterations
        .unwrap_or(options.max_iterations);
    // Source stepping restarts from the all-sources-off state, not the seed.
    let mut plan: Vec<(DcStrategy, Plan<'_>)> = vec![(
        DcStrategy::Direct,
        if policy.skip_direct {
            Plan::Disabled
        } else {
            Plan::Walk {
                start: seed,
                stages: Vec::new(),
            }
        },
    )];
    match policy.schedule {
        ContinuationSchedule::Ladder => {
            let gmin = &policy.gmin_schedule;
            plan.push((
                DcStrategy::GminStepping,
                if gmin.is_empty() {
                    Plan::Disabled
                } else {
                    Plan::Walk {
                        start: seed,
                        stages: gmin.iter().map(|g| Point::nodal(1., *g)).collect(),
                    }
                },
            ));
            plan.push((
                DcStrategy::SourceStepping,
                match &policy.source_stepping {
                    None => Plan::Disabled,
                    Some(source) => Plan::Walk {
                        start: &zero,
                        stages: source
                            .scales
                            .iter()
                            .map(|k| Point::nodal(*k, source.gmin))
                            .collect(),
                    },
                },
            ));
        }
        ContinuationSchedule::Ngspice(stepping) => {
            match stepping.gmin_steps {
                0 => plan.push((DcStrategy::GminStepping, Plan::Disabled)),
                1 => {
                    for junction in [false, true] {
                        let strategy = if junction {
                            DcStrategy::JunctionGminStepping
                        } else {
                            DcStrategy::GminStepping
                        };
                        plan.push((strategy, Plan::Dynamic { stepping, junction }));
                    }
                }
                _ => {
                    let ladder = stepping.spice3_ladder(context.gmin)?;
                    plan.push((
                        DcStrategy::GminStepping,
                        Plan::Walk {
                            start: seed,
                            stages: ladder.iter().map(|g| Point::nodal(1., *g)).collect(),
                        },
                    ));
                }
            }
            plan.push((
                DcStrategy::SourceStepping,
                match stepping.source_steps {
                    0 => Plan::Disabled,
                    1 => Plan::Gillespie(stepping),
                    steps => Plan::Uniform(steps),
                },
            ));
        }
    }
    // Default budget: every solve bounded by itl1 (the direct solve and the
    // gmin strategies' closing solves) at that limit, every other stage at
    // the stage limit, adaptive strategies at their stage cap.
    let (mut direct_count, mut stage_count) = (0_usize, 0_usize);
    for (_, planned) in &plan {
        let (direct, stages) = match planned {
            Plan::Disabled => (0, 0),
            Plan::Walk { stages, .. } => (1, stages.len()),
            Plan::Dynamic { .. } => (1, MAX_ADAPTIVE_STAGES),
            Plan::Gillespie(_) => (
                0,
                1 + GILLESPIE_GMIN_DECADES as usize + 1 + MAX_ADAPTIVE_STAGES,
            ),
            Plan::Uniform(steps) => (0, steps + 1),
        };
        direct_count += direct;
        stage_count = stage_count.saturating_add(stages);
    }
    // The ladder's source stepping ends with a closing solve at the stage limit.
    if policy.schedule == ContinuationSchedule::Ladder && policy.source_stepping.is_some() {
        direct_count -= 1;
        stage_count += 1;
    }
    let budget = policy.max_total_iterations.unwrap_or_else(|| {
        direct_count
            .saturating_mul(options.max_iterations)
            .saturating_add(stage_count.saturating_mul(stage_limit))
    });
    report.budget = budget;
    let mut engine = Engine {
        circuit,
        context,
        history,
        policy: phases,
        branches: branch_rows(circuit),
        limited: limited_rows(circuit),
        target,
        original,
        nodes,
        newton: *options,
        stage_limit,
        remaining: budget,
    };
    for (strategy, planned) in &plan {
        match engine.attempt(report, *strategy, planned) {
            Ok(solved) => {
                report.outcome = DcOutcome::Converged(*strategy);
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
            "direct Newton and every enabled continuation strategy failed: {}; \
             {OPTRAN_NOT_PORTED}",
            report.summary()
        ),
    })
}

/// Appended to an exhausted continuation's error: ngspice's `CKTop` would
/// next run its transient operating-point fallback, which is not ported.
pub const OPTRAN_NOT_PORTED: &str = "ngspice's next fallback, the transient operating point \
     (src/spicelib/analysis/optran.c, OPtran), is not ported";
