//! Adaptive trapezoidal / Gear-2 companion-model transient driver.
//!
//! This is the SPICE-compatible `.tran` backend: capacitors and inductors are
//! stamped as companion models from accepted charge/flux history
//! ([`spice_devices::Circuit::load`]), each timestep is a trial that is either
//! rejected (nothing advances) or accepted atomically
//! ([`spice_devices::Circuit::accept_point`]). It is a separate implementation
//! from the explicitly selected diffsol BDF backend and never hands companion
//! stamps to diffsol.
//!
//! The control flow follows the transient loop of `dctran.c` and its helpers
//! (`ckttrunc.c`, `cktterr.c`, `niinteg.c`, `nicomcof.c`). In short:
//!
//! * **Initial point**: the DC bias with the sources at their `t = 0` left
//!   limit; its charge/flux state fills the whole accepted history (C copies
//!   `CKTstate0` into `CKTstate1..3`) with zero derivative.
//! * **Step size**: first step `min(stop/100, tstep)/10`, cut at the `t = 0`
//!   breakpoint to `0.1 min(stop/50, gap to the next breakpoint)/10`; maximum
//!   step `tmax`, or `min(tstep, (stop - start)/50)` (`traninit.c`); minimum
//!   step `1e-11 maxstep`. `tstep` is therefore a step ceiling, never an output
//!   spacing.
//! * **Order policy**: the first step and the first step after every source
//!   breakpoint use order 1 (backward Euler). After an accepted order-1 step the
//!   order is raised to the method's `maxord` (at most 2) when the order-2
//!   truncation estimate allows a step more than 5 % larger. Like `dctran.c`
//!   (`CKTdeltaOld` is filled with `CKTmaxStep`) the estimate may already be
//!   probed on the second step, using the maximum step as the placeholder for
//!   the not-yet-existing older step ([`spice_maths::StepHistory::with_fill`]).
//!   `maxord=1` keeps backward Euler throughout.
//! * **Truncation error**: per capacitor/inductor divided differences of the
//!   charge/flux ([`spice_maths::Coefficients::truncation_timestep`], `CKTterr`)
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
//! tolerance test up to `itl4` times and report non-convergence by shrinking the
//! step by eight with order 1, as `dctran.c` does.

use std::collections::VecDeque;
use std::iter::Peekable;

use spice_core::{Complex, Real, SpiceError, SpiceResult};
use spice_devices::{
    AnalysisMode, Circuit, Forcing, LinearSystem, LoadRequest, SystemBreakpoints, TransientTiming,
    TrialState,
};
use spice_maths::integrator::{DEFAULT_XMU, TruncationTolerances};
use spice_maths::{Coefficients, IntegrationMethod, SparseMatrix, StepHistory, Vector};

use crate::linear::{number, plot, unsupported};
use crate::{AnalysisContext, AnalysisRequest, Plot};

/// C's `CKTtranMaxIter` (`.option itl4`): Newton iterations per timepoint.
const TRAN_MAX_ITER: usize = 10;
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
const KEYS: [&str; 9] = [
    "backend", "method", "maxord", "rtol", "vntol", "abstol", "chgtol", "trtol", "maxsteps",
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
        let max_order = match request.named("maxord") {
            None => 2,
            Some(text) => match text.parse::<u8>() {
                Ok(order @ 1..=2) => order,
                Ok(order @ 3..=6) => {
                    return Err(unsupported(format!(
                        "maxord={order}: only integration orders 1 and 2 are implemented"
                    )));
                }
                _ => {
                    return Err(unsupported(format!(
                        "maxord must be an integer in 1..=2, not '{text}'"
                    )));
                }
            },
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
        Ok(Self {
            step,
            stop,
            start,
            max_step,
            method,
            max_order,
            tolerances,
            max_steps,
        })
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
}

impl Breaks {
    fn new(system: &LinearSystem, settings: &Settings) -> SpiceResult<Self> {
        Ok(Self {
            source: system.breakpoints_in(0., settings.stop)?.peekable(),
            queue: VecDeque::from([0.]),
            stop: settings.stop,
            min_break: settings.max_step * 5e-5,
            pulled: 0,
            limit: settings.max_steps,
            finished: false,
        })
    }

    fn fill(&mut self, wanted: usize) -> SpiceResult<()> {
        while self.queue.len() < wanted && !self.finished {
            let Some(time) = self.source.next() else {
                self.queue.push_back(self.stop);
                self.finished = true;
                break;
            };
            self.pulled += 1;
            if self.pulled > self.limit {
                return Err(failure(format!(
                    "source breakpoint limit ({}) exceeded before the stop time",
                    self.limit
                )));
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
    model_context: spice_devices::ModelContext,
    history: spice_devices::StateHistory,
    steps: StepHistory,
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
/// (1 or 2), `rtol`, `vntol`, `abstol`, `chgtol`, `trtol` and `maxsteps`.
///
/// # Errors
///
/// Invalid or unsupported requests (`uic`, device/`.ic` initial conditions,
/// `maxord > 2`, unknown options), structural or singular systems, a failing
/// accept hook, an exceeded breakpoint/work limit, or "timestep too small".
pub fn companion_transient(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<(Plot, TransientStats)> {
    let settings = Settings::from_request(request)?;
    if request.uic {
        return Err(unsupported(
            ".tran uic requires .ic/instance-IC initialization semantics (GitHub #27 analysis half); \
             the flag is parsed but not applied",
        ));
    }
    let model_context = context.model_context();
    let mut system = circuit.linear_system_with_context(&model_context)?;
    let timing = TransientTiming::new(settings.step, settings.stop)?;
    system.bind_transient_timing(&timing)?;
    if system.has_initial_conditions {
        return Err(unsupported(
            "device ic= requires .ic/uic semantics; this backend starts from the DC operating point",
        ));
    }
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
        history: circuit.state_history(),
        steps: StepHistory::with_fill(settings.max_step),
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
        // The bias point sees the sources just before t = 0 (left limit), as
        // C's MODETRANOP evaluates the waveforms at time zero.
        let mut x = self
            .system
            .a
            .solve(&self.system.transient_rhs(0., spice_devices::Limit::Left)?)?;
        self.accept_initial(&x)?;
        if start <= 0. {
            push(&mut plot, 0., &x)?;
        }
        let mut breaks = Breaks::new(self.system, &self.settings)?;
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
                let coefficients = self.steps.trial(method, order, delta, DEFAULT_XMU)?;
                let t_new = if forced { bp0 } else { t + delta };
                if t_new <= t {
                    return Err(failure(format!(
                        "no progress: step {delta:e} does not advance t = {t:e}"
                    )));
                }
                // The step ending at a breakpoint takes the left limit.
                let limit = if forced {
                    spice_devices::Limit::Left
                } else {
                    spice_devices::Limit::Right
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
                                    // this estimate either way.
                                    let probe = self.steps.trial(method, 2, delta, DEFAULT_XMU)?;
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

    /// Records the DC bias as accepted state at `t = 0`. C copies it into
    /// `CKTstate1..3`; the derivative of each charge/flux is zero.
    fn accept_initial(&mut self, x: &Vector) -> SpiceResult<()> {
        let n = self.circuit.unknown_count();
        let mut trial = self.history.trial();
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
        self.circuit
            .accept_point(x, Some(0.), &mut self.history, trial.clone())?;
        for _ in 1..spice_devices::ACCEPTED_DEPTH {
            self.history.commit(trial.clone())?;
        }
        Ok(())
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
        limit: spice_devices::Limit,
        previous: &Vector,
    ) -> SpiceResult<Trial> {
        let n = self.circuit.unknown_count();
        let load = |guess: &Vector, matrix: &mut SparseMatrix, rhs: &mut Vector| {
            let mut state = self.history.trial();
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
        let mut guess = previous.clone();
        for iteration in 1..=TRAN_MAX_ITER {
            let mut matrix = SparseMatrix::new(n, n);
            let mut rhs = Vector::zeros(n);
            load(&guess, &mut matrix, &mut rhs)?;
            matrix.fold_duplicates();
            let x = matrix.solve(&rhs)?;
            if !x.is_finite() {
                return Err(failure(format!("nonfinite solution at t = {time:e}")));
            }
            // C needs a second iteration before it can declare convergence;
            // a linear circuit is exact after the first solve.
            if !self.nonlinear || (iteration > 1 && self.converged(&x, &guess)) {
                let state = load(&x, &mut SparseMatrix::new(n, n), &mut Vector::zeros(n))?;
                return Ok(Trial::Converged { x, state });
            }
            guess = x;
        }
        Ok(Trial::NotConverged)
    }

    /// C `NIconvTest`: `vntol` on node voltages, `abstol` on branch currents.
    fn converged(&self, new: &Vector, old: &Vector) -> bool {
        let tolerances = &self.settings.tolerances;
        new.as_slice()
            .iter()
            .zip(old.as_slice())
            .zip(&self.branch_row)
            .all(|((new, old), branch)| {
                let absolute = if *branch {
                    tolerances.abstol
                } else {
                    tolerances.vntol
                };
                (new - old).abs() <= tolerances.reltol * new.abs().max(old.abs()) + absolute
            })
    }

    /// C `CKTtrunc`: the smallest step bound over every charge-storage element,
    /// from the trial point and the accepted history.
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
            let Some(slot) = device.truncation_slot() else {
                continue;
            };
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
            limit =
                limit.min(coefficients.truncation_timestep(&charge, derivative, &tolerances)?);
        }
        Ok(limit)
    }
}

fn push(plot: &mut Plot, time: Real, x: &Vector) -> SpiceResult<()> {
    let mut point = vec![Complex::real(time)];
    point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
    plot.push_point(point)
}
