//! Adaptive trapezoidal / Gear companion-model transient driver.
//!
//! This is the SPICE-compatible `.tran` backend: capacitors and inductors are
//! and M4's bounded nonlinear device charges are stamped as companion models
//! from accepted charge/flux history
//! ([`crate::devices::Circuit::load`]), each timestep is a trial that is either
//! rejected (nothing advances) or accepted atomically
//! ([`crate::devices::Circuit::accept_point`]). It is a separate implementation
//! from the explicitly selected diffsol BDF backend and never hands companion
//! stamps to diffsol.
//!
//! The control flow follows the transient loop of `dctran.c` and its helpers
//! (`ckttrunc.c`, `cktterr.c`, `niinteg.c`, `nicomcof.c`). In short:
//!
//! * **Initial point**: the DC bias (shared M4 Newton/continuation for nonlinear
//!   circuits) with the sources at their `t = 0` left
//!   limit and the `.ic` node voltages imposed (forced in every Newton load of
//!   a nonlinear bias, with `.nodeset` forced in its `MODEINITJCT`/`MODEINITFIX`
//!   loads), or, with `uic`, the charges and fluxes of the capacitor/inductor
//!   initial conditions and of the nonlinear devices' instance initial
//!   conditions without any solve (see [`crate::analysis::initial`] and
//!   `docs/port/TRANSIENT.md`). The charge/flux
//!   state fills the whole accepted history (C copies `CKTstate0` into
//!   `CKTstate1..3`) with zero derivative.
//! * **Step size**: first step `min(stop/100, tstep)/10`, cut at the `t = 0`
//!   breakpoint to `0.1 min(stop/50, gap to the next breakpoint)/10`; maximum
//!   step `tmax`, or `min(tstep, (stop - start)/50)` (`traninit.c`); minimum
//!   step `1e-11 maxstep`. `tstep` is therefore a step ceiling, never an output
//!   spacing.
//! * **Order policy**: the first step and the first step after every source
//!   breakpoint use order 1 (backward Euler). After an accepted order-1 step the
//!   order is raised to 2 when `maxord > 1` and the order-2 truncation estimate
//!   allows a step more than 5 % larger. Like `dctran.c`
//!   (`CKTdeltaOld` is filled with `CKTmaxStep`) the estimate may already be
//!   probed on the second step, using the maximum step as the placeholder for
//!   the not-yet-existing older step ([`crate::maths::StepHistory::with_fill`]).
//!   `maxord=1` keeps backward Euler throughout. `dctran.c` has no other order
//!   change: it never raises the order above 2, so `maxord` 3 to 6 (accepted
//!   for both methods, as in C) run exactly like `maxord=2`; the Gear order
//!   3–6 coefficients of [`crate::maths::integrator`] are not reached from
//!   here (#98, verified bit-identical in the C reference binary).
//! * **Truncation error**: per capacitor/inductor divided differences of the
//!   charge/flux ([`crate::maths::Coefficients::truncation_timestep`], `CKTterr`)
//!   bound the next step to `min(2 dt, limit)`. A trial whose bound is not
//!   above `0.9 dt` is rejected and retried with the bound. The first step is
//!   never checked, as in C.
//! * **Breakpoints**: sources' corners and jumps are consumed lazily. A step is
//!   cut to land exactly on the next breakpoint (or equalised in two halves
//!   when the following step would be tiny); the step *ending* at a breakpoint
//!   evaluates the forcing with the left limit, later steps with the right.
//! * **Failures**: a step at or below the minimum is retried once at the
//!   minimum, then the run fails ("timestep too small"); a work limit bounds
//!   accepted + rejected steps; accept-hook and solver failures abort the run.
//!
//! # Output
//!
//! Like C's rawfile, the plot holds **every accepted time point** with
//! `time >= tstart` (no interpolation and no resampling onto the `.tran` step),
//! so no sample ever interpolates across a breakpoint and every breakpoint inside
//! the run has a sample. C emits no second sample for a jump; neither does this
//! driver: the sample at a breakpoint is the left limit and the next sample is
//! the first step of the right-hand segment.
//!
//! # Newton hook (M4)
//!
//! A trial is an iteration `load -> solve -> converged?`. Linear circuits take
//! exactly one solve per trial (followed by a state-only reload at the
//! solution); circuits with nonlinear devices iterate with C's voltage/current
//! tolerance test up to `itl4` times (request `tranmaxiter=`, default 100:
//! C's `NIiter()` raises its nominal default of 10, and any limit below 100,
//! to 100) and report non-convergence by shrinking the step by eight with order
//! 1, as `dctran.c` does.
//!
//! The nonlinear initial bias is the shared DC solve (`dctran.c` calls `CKTop`
//! with `CKTdcMaxIter`): request `maxiter=` (deck `itl1`), `srcsteps=`,
//! `gminsteps=` and `gminfactor=` configure it exactly as they configure `.op`
//! (see `docs/port/DC_CONTINUATION.md`). `xmu=` is the trapezoidal weighting of
//! `nicomcof.c` (default 0.5).

use std::collections::VecDeque;
use std::iter::Peekable;

use crate::devices::{
    AnalysisMode, Circuit, Forcing, LinearSystem, LoadRequest, SystemBreakpoints, TransientTiming,
    TrialState,
};
use crate::maths::integrator::{DEFAULT_XMU, TruncationTolerances};
use crate::maths::{Coefficients, IntegrationMethod, SparseMatrix, StepHistory, Vector};
use crate::primitives::{Complex, Real, SpiceError, SpiceResult};

use crate::analysis::initial::{self, Hints, VoltageTolerance};
use crate::analysis::linear::{number, plot, unsupported};
use crate::analysis::{AnalysisContext, AnalysisRequest, Plot};

/// C's *effective* default `CKTtranMaxIter` (`.option itl4`): Newton
/// iterations per timepoint. `cktntask.c` sets 10, but `NIiter()`
/// (`niiter.c`) raises every limit below 100 to 100, so C iterates up to 100.
const TRAN_MAX_ITER: usize = 100;
/// Default whole-run limit on accepted + rejected steps.
const DEFAULT_MAX_STEPS: usize = 1_000_000;
/// Largest accepted `maxsteps=`.
const MAX_STEPS_LIMIT: usize = 10_000_000;
/// ngspice defaults (`cktntask.c`, `cktsopt.c`).
const DEFAULT_RELTOL: Real = 1e-3;
const DEFAULT_VNTOL: Real = 1e-6;
const DEFAULT_ABSTOL: Real = 1e-12;
const DEFAULT_CHGTOL: Real = 1e-14;
const DEFAULT_TRTOL: Real = 7.0;
/// Request keys the companion backend understands.
const KEYS: [&str; 20] = [
    "backend",
    "method",
    "maxord",
    "rtol",
    "vntol",
    "abstol",
    "chgtol",
    "trtol",
    "maxsteps",
    "tranmaxiter",
    "xmu",
    "maxiter",
    "srcsteps",
    "gminsteps",
    "gminfactor",
    "stagemaxiter",
    "adaptiter",
    "noopiter",
    "continuation",
    "limiting",
];
/// Request keys forwarded to the initial-bias DC solve (`limiting` also
/// selects the step control of every timepoint's Newton solve).
const BIAS_KEYS: [&str; 9] = [
    "maxiter",
    "srcsteps",
    "gminsteps",
    "gminfactor",
    "stagemaxiter",
    "adaptiter",
    "noopiter",
    "continuation",
    "limiting",
];

/// Counters describing one companion transient run.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransientStats {
    /// Accepted time points after the initial operating point.
    pub accepted: usize,
    /// Trials discarded for truncation error or non-convergence.
    pub rejected: usize,
    /// Accepted steps that ended exactly on a source breakpoint or the stop time.
    pub breakpoints: usize,
    /// Smallest accepted step (0 before any step).
    pub min_step: Real,
    /// Largest accepted step.
    pub max_step: Real,
}

/// Convergence and truncation tolerances. `vntol` applies to node voltages and
/// `abstol` to branch currents (`NIconvTest`); truncation uses `reltol`,
/// `abstol` (on the charge derivative, a current), `chgtol` and `trtol`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tolerances {
    reltol: Real,
    vntol: Real,
    abstol: Real,
    chgtol: Real,
    trtol: Real,
}

/// A validated `.tran` request for the companion backend.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Settings {
    step: Real,
    stop: Real,
    start: Real,
    max_step: Real,
    method: IntegrationMethod,
    max_order: u8,
    tolerances: Tolerances,
    max_steps: usize,
    /// Newton iterations per timepoint (`itl4`, C `CKTtranMaxIter`).
    tran_max_iter: usize,
    /// Trapezoidal weighting (`.option xmu`, C `CKTxmu`), in `[0, 0.5]`.
    xmu: Real,
}

impl Settings {
    fn from_request(request: &AnalysisRequest) -> SpiceResult<Self> {
        let mut positional = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for argument in &request.arguments {
            if let Some((key, _)) = argument.split_once('=') {
                let key = key.trim().to_ascii_lowercase();
                if !KEYS.contains(&key.as_str()) {
                    return Err(unsupported(format!(
                        "companion transient option '{key}' (supported: {})",
                        KEYS.join(", ")
                    )));
                }
                if !seen.insert(key.clone()) {
                    return Err(unsupported(format!("duplicate transient option {key}")));
                }
            } else {
                positional.push(argument.as_str());
            }
        }
        if !(2..=4).contains(&positional.len()) {
            return Err(unsupported(".tran requires tstep tstop [tstart [tmax]]"));
        }
        let step = number(positional.first().copied(), "step time")?;
        let stop = number(positional.get(1).copied(), "stop time")?;
        let start = number(
            Some(positional.get(2).copied().unwrap_or("0")),
            "start time",
        )?;
        let tmax = number(Some(positional.get(3).copied().unwrap_or("0")), "tmax")?;
        if step <= 0. || stop <= 0. || start < 0. || start >= stop || tmax < 0. {
            return Err(unsupported(
                "invalid transient time bounds (need tstep > 0, tstop > 0, 0 <= tstart < tstop, tmax >= 0)",
            ));
        }
        // traninit.c: tmax = 0 selects min(tstep, (tstop - tstart)/50).
        let max_step = if tmax > 0. {
            tmax
        } else {
            step.min((stop - start) / 50.)
        };
        // cktsopt.c clamps maxord to 1..=6 with a warning; out-of-range or
        // non-integer values are explicit errors here.
        let max_order = match request.named("maxord") {
            None => 2,
            Some(text) => text
                .parse::<u8>()
                .ok()
                .filter(|order| (1..=IntegrationMethod::MAX_GEAR_ORDER).contains(order))
                .ok_or_else(|| {
                    unsupported(format!(
                        "maxord must be an integer in 1..={}, not '{text}'",
                        IntegrationMethod::MAX_GEAR_ORDER
                    ))
                })?,
        };
        let method = match request.named("method") {
            None => IntegrationMethod::Trapezoidal,
            Some(name) if name.eq_ignore_ascii_case("bdf") => {
                return Err(unsupported(
                    "method=bdf is the diffsol backend; select it with backend=diffsol \
                     (companion methods are trap and gear)",
                ));
            }
            Some(name) => IntegrationMethod::parse(name, max_order).ok_or_else(|| {
                unsupported(format!(
                    "unknown transient method '{name}'; expected trap, trapezoidal or gear"
                ))
            })?,
        };
        method.validate_runtime()?;
        let positive = |key: &str, default: Real| -> SpiceResult<Real> {
            let value = request
                .named(key)
                .map_or(Ok(default), |text| number(Some(text), key))?;
            if value > 0. {
                Ok(value)
            } else {
                Err(unsupported(format!("transient {key} must be positive")))
            }
        };
        let tolerances = Tolerances {
            reltol: positive("rtol", DEFAULT_RELTOL)?,
            vntol: positive("vntol", DEFAULT_VNTOL)?,
            abstol: positive("abstol", DEFAULT_ABSTOL)?,
            chgtol: positive("chgtol", DEFAULT_CHGTOL)?,
            trtol: positive("trtol", DEFAULT_TRTOL)?,
        };
        let max_steps = match request.named("maxsteps") {
            None => DEFAULT_MAX_STEPS,
            Some(text) => text
                .parse()
                .ok()
                .filter(|n| (1..=MAX_STEPS_LIMIT).contains(n))
                .ok_or_else(|| {
                    unsupported(format!(
                        "maxsteps must be an integer in 1..={MAX_STEPS_LIMIT}"
                    ))
                })?,
        };
        let tran_max_iter = match request.named("tranmaxiter") {
            None => TRAN_MAX_ITER,
            Some(text) => crate::primitives::parse_spice_number(text)
                .filter(|v| {
                    v.fract() == 0.
                        && (1. ..=crate::analysis::newton::MAX_ITERATIONS as Real).contains(v)
                })
                .map(|v| v as usize)
                .ok_or_else(|| {
                    unsupported(format!(
                        "tranmaxiter must be an integer in 1..={}, not '{text}'",
                        crate::analysis::newton::MAX_ITERATIONS
                    ))
                })?,
        };
        let xmu = match request.named("xmu") {
            None => DEFAULT_XMU,
            Some(text) => crate::primitives::parse_spice_number(text)
                .filter(|v| (0. ..=0.5).contains(v))
                .ok_or_else(|| unsupported(format!("xmu must be in [0, 0.5], not '{text}'")))?,
        };
        Ok(Self {
            step,
            stop,
            start,
            max_step,
            method,
            max_order,
            tolerances,
            max_steps,
            tran_max_iter,
            xmu,
        })
    }

    /// The initial-bias DC settings: `maxiter`/`srcsteps`/`gminsteps`/
    /// `gminfactor` from the request (validated by
    /// [`crate::analysis::bias::DcSettings::from_request`]) with this run's Newton
    /// tolerances.
    fn bias(&self, request: &AnalysisRequest) -> SpiceResult<crate::analysis::bias::DcSettings> {
        let forwarded = request.arguments.iter().filter(|argument| {
            argument.split_once('=').is_some_and(|(key, _)| {
                BIAS_KEYS.contains(&key.trim().to_ascii_lowercase().as_str())
            })
        });
        let mut settings =
            crate::analysis::bias::DcSettings::from_request(&AnalysisRequest::with_arguments(
                crate::primitives::AnalysisKind::OperatingPoint,
                forwarded,
            ))?;
        settings.newton.reltol = self.tolerances.reltol;
        settings.newton.vntol = self.tolerances.vntol;
        settings.newton.abstol = self.tolerances.abstol;
        Ok(settings)
    }

    /// C `CKTdelmin`.
    fn min_step(&self) -> Real {
        1e-11 * self.max_step
    }
}

fn failure(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "transient".to_owned(),
        message: message.into(),
    }
}

/// `AlmostEqualUlps(a, b, 100)` for finite values of the same scale.
fn nearly_equal(a: Real, b: Real) -> bool {
    (a - b).abs() <= 100.0 * Real::EPSILON * a.abs().max(b.abs())
}

/// Lazily merged source breakpoints, bracketed by `0` and the stop time.
///
/// C keeps a sorted list `CKTbreaks`, always starting with the time-zero and
/// ending with the final-time breakpoint, and merges entries closer than
/// `CKTminBreak` (`cktsetbk.c`). Here only the next two entries are ever
/// materialized.
struct Breaks {
    source: Peekable<SystemBreakpoints>,
    queue: VecDeque<Real>,
    stop: Real,
    min_break: Real,
    pulled: usize,
    limit: usize,
    finished: bool,
    /// An extra breakpoint (`uic`: `CKTsetBreak(step)` in `dctran.c`), merged
    /// into the source breakpoints in time order.
    extra: Option<Real>,
}

impl Breaks {
    fn new(system: &LinearSystem, settings: &Settings, extra: Option<Real>) -> SpiceResult<Self> {
        Ok(Self {
            source: system.breakpoints_in(0., settings.stop)?.peekable(),
            queue: VecDeque::from([0.]),
            stop: settings.stop,
            min_break: settings.max_step * 5e-5,
            pulled: 0,
            limit: settings.max_steps,
            finished: false,
            extra,
        })
    }

    fn fill(&mut self, wanted: usize) -> SpiceResult<()> {
        while self.queue.len() < wanted && !self.finished {
            let take_extra = match (self.extra, self.source.peek()) {
                (Some(extra), Some(source)) => extra <= *source,
                (Some(_), None) => true,
                (None, _) => false,
            };
            let time = if take_extra {
                self.extra.take()
            } else {
                self.source.next()
            };
            let Some(time) = time else {
                self.queue.push_back(self.stop);
                self.finished = true;
                break;
            };
            if !take_extra {
                self.pulled += 1;
                if self.pulled > self.limit {
                    return Err(failure(format!(
                        "source breakpoint limit ({}) exceeded before the stop time",
                        self.limit
                    )));
                }
            }
            let last = self.queue.back().copied().unwrap_or(0.);
            if time > last + self.min_break && time < self.stop - self.min_break {
                self.queue.push_back(time);
            }
        }
        Ok(())
    }

    /// The next breakpoint at or after the accepted time and the one after it.
    fn front_two(&mut self) -> SpiceResult<(Real, Real)> {
        self.fill(2)?;
        let first = self.queue.front().copied().unwrap_or(self.stop);
        Ok((first, self.queue.get(1).copied().unwrap_or(first)))
    }

    /// Drops breakpoints the run has moved past (`t > breaks[0]` in C).
    fn discard_before(&mut self, time: Real) {
        while self.queue.len() > 1 && self.queue.front().is_some_and(|b| *b < time) {
            self.queue.pop_front();
        }
    }
}

enum Trial {
    Converged { x: Vector, state: TrialState },
    NotConverged,
}

struct Driver<'a> {
    circuit: &'a Circuit,
    system: &'a LinearSystem,
    settings: Settings,
    timing: TransientTiming,
    model_context: crate::devices::ModelContext,
    /// Newton/continuation settings of the nonlinear initial bias.
    bias: crate::analysis::bias::DcSettings,
    history: crate::devices::StateHistory,
    steps: StepHistory,
    hints: Hints,
    uic: bool,
    nonlinear: bool,
    /// `true` for rows that carry branch currents (`abstol`), else `vntol`.
    branch_row: Vec<bool>,
    stats: TransientStats,
}

/// Runs an ordinary `.tran` with the companion backend, returning the plot and
/// run statistics.
///
/// `request` uses the same positional `tstep tstop [tstart [tmax]]` arguments
/// as C plus named `method` (`trap` default, `trapezoidal`, `gear`), `maxord`
/// (1 to 6; only 1 versus more than 1 matters, as in `dctran.c`), `rtol`, `vntol`, `abstol`, `chgtol`, `trtol`, `maxsteps`,
/// `tranmaxiter` (Newton iterations per timepoint, `itl4`), `xmu` (trapezoidal
/// weighting) and the initial-bias `maxiter`, `srcsteps`, `gminsteps`,
/// `gminfactor`.
///
/// # Errors
///
/// Invalid or unsupported requests (`maxord` outside 1..=6, unknown options), `.ic` or
/// `.nodeset` entries naming unknown nodes, an `.ic` contradicting ideal
/// sources, `uic` initial conditions that would need an impulse at `t = 0+`,
/// structural or singular systems, a failing accept hook, an exceeded
/// breakpoint/work limit, or "timestep too small". A failed initialization
/// returns before any device accept hook ran or any plot row exists.
pub fn companion_transient(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<(Plot, TransientStats)> {
    let settings = Settings::from_request(request)?;
    let bias = settings.bias(request)?;
    // Unknown/ground nodes in .ic/.nodeset fail before anything is assembled.
    let hints = initial::resolve(circuit, request)?;
    let model_context = context.model_context();
    circuit.finalize()?;
    crate::devices::sources::reject_transient_power_ports(circuit)?;
    let nonlinear = circuit.devices().iter().any(|d| d.is_nonlinear());
    let mut system = if nonlinear {
        circuit.small_signal_system(&model_context, &Vector::zeros(circuit.unknown_count()))?
    } else {
        circuit.linear_system_with_context(&model_context)?
    };
    let timing = TransientTiming::new(settings.step, settings.stop)?;
    system.bind_transient_timing(&timing)?;
    let circuit: &Circuit = circuit;
    let mut branch_row = vec![false; circuit.unknown_count()];
    for index in 0..circuit.device_count() {
        for row in circuit.branch_rows(index).unwrap_or(0..0) {
            branch_row[row] = true;
        }
    }
    let driver = Driver {
        circuit,
        system: &system,
        settings,
        timing,
        model_context,
        bias,
        history: circuit.state_history(),
        steps: StepHistory::with_fill(settings.max_step),
        hints,
        uic: request.uic,
        nonlinear: circuit.devices().iter().any(|d| d.is_nonlinear()),
        branch_row,
        stats: TransientStats::default(),
    };
    driver.run()
}

impl Driver<'_> {
    fn run(mut self) -> SpiceResult<(Plot, TransientStats)> {
        let Settings {
            step,
            stop,
            start,
            max_step,
            method,
            max_order,
            max_steps,
            ..
        } = self.settings;
        let min_step = self.settings.min_step();
        let mut plot = plot(
            self.circuit,
            "tran1",
            "Transient Analysis",
            Some(("time", "time")),
            false,
        )?;
        let mut x = self.initialize()?;
        // C (dctran.c) writes no t = 0 row under uic: the first dump is the
        // first accepted timepoint (`CKTtime > 0`).
        if start <= 0. && !self.uic {
            push(&mut plot, 0., &x)?;
        }
        // dctran.c: under uic a breakpoint at the print step limits ringing of
        // the first steps ("CKTsetBreak(ckt, ckt->CKTstep)").
        let mut breaks = Breaks::new(self.system, &self.settings, self.uic.then_some(step))?;
        let mut t = 0.;
        let mut delta = (stop / 100.).min(step) / 10.;
        let mut save_delta = stop / 50.;
        let mut order = 1_u8;
        let mut first = true;
        while t < stop {
            delta = delta.min(max_step);
            let (bp0, bp1) = breaks.front_two()?;
            let mut forced = false;
            if bp0 < stop && (nearly_equal(t, bp0) || bp0 - t <= min_step) {
                // First point after a breakpoint: backward Euler and a short
                // step bounded by the gap to the next breakpoint.
                order = 1;
                delta = delta.min(0.1 * save_delta.min(bp1 - bp0));
                if first {
                    delta /= 10.;
                }
                delta = delta.max(2. * min_step);
            } else if t + delta >= bp0 {
                save_delta = delta;
                delta = bp0 - t;
                forced = true;
            } else if t + 1.9 * delta > bp0 {
                // Equalise the last two steps before the breakpoint.
                save_delta = delta;
                delta = (bp0 - t) / 2.;
            }
            loop {
                if self.stats.accepted + self.stats.rejected >= max_steps {
                    return Err(failure(format!(
                        "work limit of {max_steps} accepted + rejected steps reached at t = {t:e}"
                    )));
                }
                let coefficients = self.steps.trial(method, order, delta, self.settings.xmu)?;
                let t_new = if forced { bp0 } else { t + delta };
                if t_new <= t {
                    return Err(failure(format!(
                        "no progress: step {delta:e} does not advance t = {t:e}"
                    )));
                }
                // The step ending at a breakpoint takes the left limit.
                let limit = if forced {
                    crate::devices::Limit::Left
                } else {
                    crate::devices::Limit::Right
                };
                let attempted = delta;
                match self.solve_trial(t_new, &coefficients, limit, &x)? {
                    Trial::NotConverged => {
                        self.stats.rejected += 1;
                        delta /= 8.;
                        order = 1;
                        forced = false;
                    }
                    Trial::Converged { x: x_new, state } => {
                        let mut next = delta;
                        let mut reject = false;
                        if !first {
                            let bound =
                                (2. * delta).min(self.truncation_limit(&coefficients, &state)?);
                            if bound > 0.9 * delta {
                                next = bound;
                                if coefficients.order() == 1 && max_order > 1 {
                                    // Probe order 2; C overwrites the step with
                                    // this estimate either way. dctran.c never
                                    // probes or raises beyond order 2, whatever
                                    // maxord (3..=6) allows.
                                    let probe =
                                        self.steps.trial(method, 2, delta, self.settings.xmu)?;
                                    next = (2. * delta).min(self.truncation_limit(&probe, &state)?);
                                    order = if next <= 1.05 * delta { 1 } else { 2 };
                                }
                            } else {
                                reject = true;
                                delta = bound;
                            }
                        }
                        if !reject {
                            self.circuit.accept_point(
                                &x_new,
                                Some(t_new),
                                &mut self.history,
                                state,
                            )?;
                            self.steps.accept(&coefficients);
                            self.record_step(attempted, forced || t_new >= stop);
                            t = t_new;
                            x = x_new;
                            if t >= start {
                                push(&mut plot, t, &x)?;
                            }
                            breaks.discard_before(t);
                            first = false;
                            delta = next;
                            break;
                        }
                        self.stats.rejected += 1;
                        forced = false;
                    }
                }
                // dctran.c: at or below delmin, retry once at delmin, then fail.
                if delta <= min_step {
                    if attempted > min_step {
                        delta = min_step;
                    } else {
                        return Err(failure(format!(
                            "timestep too small at t = {t:e} (minimum {min_step:e})"
                        )));
                    }
                }
            }
        }
        Ok((plot, self.stats))
    }

    fn record_step(&mut self, step: Real, on_breakpoint: bool) {
        let stats = &mut self.stats;
        stats.min_step = if stats.accepted == 0 {
            step
        } else {
            stats.min_step.min(step)
        };
        stats.max_step = stats.max_step.max(step);
        stats.accepted += 1;
        stats.breakpoints += usize::from(on_breakpoint);
    }

    /// The initial point of the run: the solution `x` at `t = 0` and the
    /// accepted charge/flux state filling the whole history.
    ///
    /// * Ordinary run (C `CKTop` with `MODETRANOP`): `A x = b(0-)` with the
    ///   sources at their left limit; `.ic` node voltages are enforced as hard
    ///   row constraints during this solve only ([`initial::constrained_bias`],
    ///   or forced rows of the nonlinear bias, [`crate::analysis::bias::NodeForcing`]),
    ///   instance `ic=` of C/L/D/Q is ignored (MOS1 `ic=` only moves its
    ///   `MODEINITJCT` start) and `.nodeset` cannot change a linear point.
    /// * `uic` (C `NIiter` returns after one `CKTload`): no solve; charges and
    ///   fluxes come from the capacitor/inductor initial values (see
    ///   [`initial::uic_start`]), nonlinear devices load once at their
    ///   instance initial conditions, and [`initial::check_impulse_free`]
    ///   rejects capacitor/inductor initial conditions that would need an
    ///   impulse (junction charges are not part of that check, as in C).
    ///
    /// Nothing is committed (no accept hook, no history) until every check has
    /// passed, so a failed initialization leaves no partial state.
    fn initialize(&mut self) -> SpiceResult<Vector> {
        let rhs = self.system.transient_rhs(0., crate::devices::Limit::Left)?;
        let (x, trial) = if self.uic {
            let start = initial::uic_start(self.circuit, &self.hints, &self.model_context)?;
            // C's single MODETRANOP|MODEUIC|MODEINITJCT load: nonlinear
            // devices evaluate at their instance initial conditions.
            let trial = self.initial_state(&start.x, &start.charges, true)?;
            let tolerances = &self.settings.tolerances;
            initial::check_impulse_free(
                self.circuit,
                &self.system.a,
                &self
                    .system
                    .transient_rhs(0., crate::devices::Limit::Right)?,
                &start.x,
                initial::Tolerances {
                    reltol: tolerances.reltol,
                    vntol: tolerances.vntol,
                    abstol: tolerances.abstol,
                },
                &self.model_context,
            )?;
            (start.x, trial)
        } else {
            let tolerances = &self.settings.tolerances;
            let constraints = initial::irredundant_constraints(
                self.circuit,
                &rhs,
                &self.hints.initial,
                VoltageTolerance {
                    reltol: tolerances.reltol,
                    vntol: tolerances.vntol,
                },
            )?;
            let (x, solved) = if self.nonlinear {
                // CKTic: the Newton guess holds the .nodeset, then the .ic
                // values; cktload.c forces the .ic rows in every load of the
                // MODETRANOP solve and the .nodeset rows in its MODEINITJCT/
                // MODEINITFIX loads.
                let mut seed = Vector::zeros(self.circuit.unknown_count());
                for hint in self.hints.nodesets.iter().chain(&self.hints.initial) {
                    seed.as_mut_slice()[hint.row] = hint.value;
                }
                let nodes = crate::analysis::bias::NodeForcing {
                    initial: constraints
                        .imposed
                        .iter()
                        .map(|hint| (hint.row, hint.value))
                        .collect(),
                    nodesets: initial::forced_nodesets(
                        self.circuit,
                        &self.hints.nodesets,
                        &self.hints.initial,
                    ),
                };
                let solution = crate::analysis::bias::solve_dc_forced(
                    self.circuit,
                    &self.model_context,
                    &self.bias,
                    &[],
                    Some(&seed),
                    Some(&rhs),
                    &nodes,
                )?
                .solution;
                (solution.values, Some(solution.trial))
            } else if constraints.is_unconstrained() {
                (self.system.a.solve(&rhs)?, None)
            } else {
                (
                    initial::constrained_bias(&self.system.a, &rhs, &constraints.imposed)?,
                    None,
                )
            };
            initial::check_implied(
                &x,
                &constraints,
                VoltageTolerance {
                    reltol: tolerances.reltol,
                    vntol: tolerances.vntol,
                },
            )?;
            // A nonlinear bias's converged trial is the DC-mode load at `x`
            // (charges, zero derivatives) and also carries the operating
            // point's discrete switch states, which a fresh reload in the
            // initial phase would re-derive from the instance flags.
            let trial = match solved {
                Some(trial) => trial,
                None => self.initial_state(&x, &[], false)?,
            };
            (x, trial)
        };
        // C copies CKTstate0 into CKTstate1..3; the derivative is zero. The
        // port fills every retained vector (ACCEPTED_DEPTH, sized for Gear
        // order 6); dctran.c's orders 1-2 never read beyond CKTstate3.
        self.circuit
            .accept_point(&x, Some(0.), &mut self.history, trial.clone())?;
        for _ in 1..crate::devices::ACCEPTED_DEPTH {
            self.history.commit(trial.clone())?;
        }
        Ok(x)
    }

    /// The charge/flux state of the initial point: a DC-mode load at `x`
    /// (`q = C v`, `flux = L i`, zero derivative) with the listed absolute
    /// state slots overwritten. `uic` marks C's `uic` initial load, in which
    /// nonlinear devices evaluate (and store their junction voltages and
    /// charges) at their instance initial conditions
    /// ([`crate::devices::TrialState::with_initial_conditions`]).
    fn initial_state(
        &self,
        x: &Vector,
        overrides: &[(usize, Real)],
        uic: bool,
    ) -> SpiceResult<TrialState> {
        let n = self.circuit.unknown_count();
        let mut trial = self.history.trial().with_initial_conditions(uic);
        self.circuit.load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: x,
                model_context: &self.model_context,
                integration: None,
                history: &self.history,
                forcing: None,
            },
            &mut SparseMatrix::new(n, n),
            &mut Vector::zeros(n),
            &mut trial,
        )?;
        for (slot, value) in overrides {
            self.history
                .device(&mut trial, *slot..*slot + 1)?
                .set(0, *value)?;
        }
        Ok(trial)
    }

    /// One trial point: `load -> solve -> converged?` (C `NIiter`).
    ///
    /// Linear circuits are exact after one solve. The state of the trial is
    /// evaluated at the converged solution with a matrix-free reload so that the
    /// recorded charge/flux and derivative belong to the solved point (C keeps
    /// the state of the last Newton load, one iterate behind).
    fn solve_trial(
        &self,
        time: Real,
        coefficients: &Coefficients,
        limit: crate::devices::Limit,
        previous: &Vector,
    ) -> SpiceResult<Trial> {
        let n = self.circuit.unknown_count();
        let tolerance = &self.settings.tolerances;
        let options = crate::analysis::newton::NewtonOptions {
            max_iterations: self.settings.tran_max_iter,
            reltol: tolerance.reltol,
            vntol: tolerance.vntol,
            abstol: tolerance.abstol,
            limiting: self.bias.newton.limiting,
            ..crate::analysis::newton::NewtonOptions::default()
        };
        let load = |guess: &Vector,
                    matrix: &mut SparseMatrix,
                    rhs: &mut Vector,
                    phase: crate::devices::IterationPhase,
                    previous: Option<&TrialState>| {
            let mut state = self
                .history
                .trial_in(phase, previous)?
                .with_device_limiting(options.limiting.is_device());
            self.circuit.load(
                &LoadRequest {
                    mode: AnalysisMode::Transient {
                        time,
                        dt: coefficients.dt(),
                    },
                    solution: guess,
                    model_context: &self.model_context,
                    integration: Some(coefficients),
                    history: &self.history,
                    forcing: Some(Forcing {
                        limit,
                        timing: self.timing,
                    }),
                },
                matrix,
                rhs,
                &mut state,
            )?;
            Ok::<_, SpiceError>(state)
        };
        if self.nonlinear {
            let limited = crate::analysis::bias::limited_rows(self.circuit);
            // dctran.c: MODEINITTRAN/MODEINITPRED for the first load of a
            // timepoint, MODEINITFLOAT afterwards.
            return match crate::analysis::newton::solve_phased(
                previous,
                &self.branch_row,
                limited.as_deref(),
                &options,
                crate::analysis::newton::PhasePolicy::Predicted,
                None,
                |x, phase, last| {
                    let mut matrix = SparseMatrix::new(n, n);
                    let mut rhs = Vector::zeros(n);
                    let state = load(x, &mut matrix, &mut rhs, phase, last)?;
                    Ok((matrix, rhs, state))
                },
            )
            .map_err(|failure| failure.error)
            {
                Ok(solved) => Ok(Trial::Converged {
                    x: solved.values,
                    state: solved.trial,
                }),
                Err(SpiceError::Numerical { message, .. })
                    if message.contains("Newton iteration limit") =>
                {
                    Ok(Trial::NotConverged)
                }
                Err(error) => Err(error),
            };
        }
        // Linear circuits are exact after one solve; reload only to record
        // charge/flux at the solved point, with no Newton iteration loop.
        let mut matrix = SparseMatrix::new(n, n);
        let mut rhs = Vector::zeros(n);
        let phase = crate::devices::IterationPhase::Predict;
        load(previous, &mut matrix, &mut rhs, phase, None)?;
        matrix.fold_duplicates();
        let (matrix, rhs) = equilibrated(&matrix, &rhs)?;
        let x = matrix.solve(&rhs)?;
        let state = load(
            &x,
            &mut SparseMatrix::new(n, n),
            &mut Vector::zeros(n),
            phase,
            None,
        )?;
        Ok(Trial::Converged { x, state })
    }

    /// C `CKTtrunc`: the smallest step bound over every charge-storage element
    /// and every discrete-state device ([`crate::devices::Device::timestep_limit`],
    /// `swtrunc.c`), from the trial point and the accepted history.
    fn truncation_limit(
        &self,
        coefficients: &Coefficients,
        trial: &TrialState,
    ) -> SpiceResult<Real> {
        let tolerances = &self.settings.tolerances;
        let tolerances = TruncationTolerances {
            reltol: tolerances.reltol,
            abstol: tolerances.abstol,
            chgtol: tolerances.chgtol,
            trtol: tolerances.trtol,
        };
        let order = usize::from(coefficients.order());
        let mut limit = Real::INFINITY;
        for (index, device) in self.circuit.devices().iter().enumerate() {
            for slot in device.truncation_slots() {
                let base = self
                    .circuit
                    .state_rows(index)
                    .ok_or_else(|| failure("missing state range"))?
                    .start
                    + slot;
                let accepted = |age: usize, offset: usize| {
                    self.history
                        .accepted(age)
                        .and_then(|vector| vector.get(base + offset))
                        .copied()
                        .ok_or_else(|| failure("accepted state history is too short"))
                };
                let mut charge = vec![trial.values()[base]];
                for age in 1..=order + 1 {
                    charge.push(accepted(age, 0)?);
                }
                let derivative = [trial.values()[base + 1], accepted(1, 1)?];
                limit = limit.min(coefficients.truncation_timestep(
                    &charge,
                    derivative,
                    &tolerances,
                )?);
            }
            // Discrete-state bounds (`swtrunc.c`), from the same trial.
            let rows = self
                .circuit
                .state_rows(index)
                .ok_or_else(|| failure("missing state range"))?;
            if !rows.is_empty() {
                let bound = device.timestep_limit(&crate::devices::TruncationContext {
                    trial: trial
                        .slice(rows.clone())
                        .ok_or_else(|| failure("trial state is too short"))?,
                    accepted: self.history.accepted(1).and_then(|vector| vector.get(rows)),
                    dt: coefficients.dt(),
                })?;
                if let Some(bound) = bound {
                    if !bound.is_finite() {
                        return Err(failure(format!(
                            "{}: nonfinite timestep limit",
                            device.name()
                        )));
                    }
                    limit = limit.min(bound);
                }
            }
        }
        Ok(limit)
    }
}

/// Row-equilibrated copy of `A x = b`: every row is divided by its largest
/// entry, which leaves the solution unchanged.
///
/// A companion inductor row `v+ - v- - (L/h) i = veq` has a coefficient `L/h`
/// (1e5 and more for small first steps) next to O(1) entries; elimination
/// through it costs the digits the solver's backward-residual check demands
/// as soon as the initial current is not zero. Dividing each row by its
/// largest entry removes the disparity without touching any device stamp.
fn equilibrated(matrix: &SparseMatrix, rhs: &Vector) -> SpiceResult<(SparseMatrix, Vector)> {
    let n = matrix.rows();
    let mut largest = vec![0.; n];
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
    let mut scaled = SparseMatrix::new(n, matrix.cols());
    for t in matrix.triplets() {
        scaled.add(t.row, t.col, t.value * scale(t.row))?;
    }
    let mut b = rhs.clone();
    for (row, value) in b.as_mut_slice().iter_mut().enumerate() {
        *value *= scale(row);
    }
    Ok((scaled, b))
}

fn push(plot: &mut Plot, time: Real, x: &Vector) -> SpiceResult<()> {
    let mut point = vec![Complex::real(time)];
    point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
    plot.push_point(point)
}
