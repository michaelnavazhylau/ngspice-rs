//! Adaptive trapezoidal / Gear companion-model transient driver.
//!
//! This is the SPICE-compatible `.tran` backend: capacitors and inductors are
//! and M4's bounded nonlinear device charges are stamped as companion models
//! from accepted charge/flux history
//! ([`crate::devices::Circuit::load`]), each timestep is a trial that is either
//! rejected (nothing advances) or accepted atomically
//! ([`crate::devices::Circuit::accept_point`]). It is the only transient
//! backend.
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
//!   spacing. The loop is a literal port of `dctran.c` as the reference binary
//!   is built (XSPICE defined, no event-driven instances), so the accepted
//!   time points are bit-identical whenever the device loads are: the time
//!   advances by `CKTtime += CKTdelta` and a rejection subtracts the same
//!   delta again, `CKTsaveDelta`, `CKTbreak` and `firsttime` follow C, and
//!   every trial's Newton solve starts from the previous trial's last iterate
//!   (C's `CKTrhsOld`), rejected or not.
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
//! * **Breakpoints**: C's table ([`BreakTable`], `CKTsetBreak`/`CKTclrBreak`
//!   with `CKTminBreak = 10 CKTdelmin`) is filled at each accepted point by
//!   the sources' accept hooks ([`SourceBreaks`], `VSRCaccept`/`ISRCaccept`).
//!   A step that would pass `CKTbreaks[0]` is cut to end there (at
//!   `CKTtime + (CKTbreaks[0] - CKTtime)`), or the distance is halved when
//!   `t + 1.9 dt` passes it; the step ending at a breakpoint evaluates the
//!   forcing with the left limit, other steps with the right (only port-only
//!   jumps tell them apart). Devices with a delay history (transmission lines,
//!   [`crate::devices::delay`]) add entries through the same `CKTsetBreak`
//!   when they accept a point (`TRAaccept`), in C's `CKTaccept` type order
//!   between the current and the voltage sources.
//! * **Failures**: a step at or below the minimum is retried once at the
//!   minimum, then the run fails ("timestep too small"); a work limit bounds
//!   accepted + rejected steps (C has none); accept-hook and solver failures
//!   abort the run.
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

use crate::devices::{
    AnalysisMode, BreakTable, Circuit, Forcing, LinearSystem, LoadRequest, SourceBreaks,
    SourceKind, TransientTiming, TrialState,
};
use crate::maths::integrator::{DEFAULT_XMU, TruncationTolerances};
use crate::maths::{Coefficients, IntegrationMethod, SparseMatrix, StepHistory, Vector};
use crate::primitives::{Complex, Real, SpiceError, SpiceResult, almost_equal_ulps};

use crate::analysis::initial::{self, Hints, VoltageTolerance};
use crate::analysis::linear::{number, plot, unsupported};
use crate::analysis::{AnalysisContext, AnalysisRequest, Plot, PointSink};

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
                    "method=bdf: ngspice has no BDF integrator (the diffsol backend was \
                     removed); the companion methods are trap and gear",
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

enum Trial {
    Converged {
        x: Vector,
        state: TrialState,
    },
    /// The iteration limit was reached; `last` is the iterate of the final
    /// load (C's `CKTrhsOld`, the next trial's starting point).
    NotConverged {
        last: Vector,
    },
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
    companion(circuit, request, context, None)
}

/// [`companion_transient`], handing every accepted point to `sink` as it is
/// accepted instead of storing it, as C's `dctran.c` hands each one to
/// `CKTdump`/`OUTpData`.
///
/// `sink` receives the plot header before the initial operating point is
/// solved and then exactly the rows [`companion_transient`] would store. The
/// returned plot has the variables but no points.
///
/// # Errors
///
/// As [`companion_transient`], and whatever `sink` reports.
pub fn companion_transient_into(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
    sink: &mut dyn PointSink,
) -> SpiceResult<(Plot, TransientStats)> {
    companion(circuit, request, context, Some(sink))
}

fn companion(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
    sink: Option<&mut dyn PointSink>,
) -> SpiceResult<(Plot, TransientStats)> {
    let settings = Settings::from_request(request)?;
    let bias = settings.bias(request)?;
    if request.uic
        && let Some(line) = circuit.devices().iter().find(|d| d.delay_line().is_some())
    {
        // traload.c starts the delayed waves from the instance
        // `ic=`/`v1 i1 v2 i2` under MODEUIC, but the port's impulse check
        // (initial::check_impulse_free) has no transmission-line
        // formulation yet.
        return Err(SpiceError::not_yet_ported(
            format!(
                ".tran uic with transmission line {} (initial waves from its ic=)",
                line.name()
            ),
            "src/spicelib/devices/tra/traload.c (MODEINITTRAN | MODEUIC)",
        ));
    }
    // Unknown/ground nodes in .ic/.nodeset fail before anything is assembled.
    let hints = initial::resolve(circuit, request)?;
    let model_context = context.model_context();
    circuit.finalize()?;
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
        branch_row,
        stats: TransientStats::default(),
    };
    driver.run(sink)
}

impl Driver<'_> {
    fn run(mut self, mut sink: Option<&mut dyn PointSink>) -> SpiceResult<(Plot, TransientStats)> {
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
        // traninit.c: CKTdelmin; dctran.c (XSPICE build): CKTminBreak.
        let min_step = self.settings.min_step();
        let mut breaks = BreakTable::new(stop, 10.0 * min_step);
        let mut sources = SourceBreaks::new(self.system);
        let mut plot = plot(
            self.circuit,
            "tran1",
            "Transient Analysis",
            Some(("time", "time")),
            false,
        )?;
        if let Some(sink) = sink.as_deref_mut() {
            sink.begin(&plot)?;
        }
        // dctran.c: delta = MIN(CKTfinalTime/100, CKTstep)/10.
        let mut delta = (stop / 100.).min(step) / 10.;
        let mut x = self.initialize()?;
        // CKTtime, CKTbreak, firsttime, CKTorder and CKTsaveDelta.
        let mut t = 0.;
        let mut at_break = true;
        let mut first = true;
        let mut order = 1_u8;
        let mut save_delta = stop / 50.;
        // Whether the next trial is the step cut to end on CKTbreaks[0]; only
        // selects the forcing's one-sided limit at port-only jumps.
        let mut forced;
        // The accepted point's forcing limit and observations.
        let mut observed = Vec::new();
        // The delay lines' CKTsetBreak requests of the last accepted point.
        let mut device_breaks: Vec<Real> = Vec::new();
        loop {
            // nextTime: CKTaccept (device hooks ran when the point was
            // committed), the sources' breakpoint requests, then output.
            // CKTaccept runs DEVaccept in dev.c type order: isrc, then tra
            // (the delay lines' requests), then vsrc.
            sources.accept(
                self.system,
                SourceKind::Current,
                t,
                at_break,
                &self.timing,
                &mut breaks,
            )?;
            for time in std::mem::take(&mut device_breaks) {
                breaks.set(t, time)?;
            }
            sources.accept(
                self.system,
                SourceKind::Voltage,
                t,
                at_break,
                &self.timing,
                &mut breaks,
            )?;
            if t > breaks.first() {
                breaks.clear();
            }
            at_break = false;
            if t == 0. {
                // C (dctran.c) writes no t = 0 row under uic.
                if start <= 0. && !self.uic {
                    let observed = self.circuit.observe_real(
                        &LoadRequest {
                            mode: AnalysisMode::OperatingPoint,
                            solution: &x,
                            model_context: &self.model_context,
                            integration: None,
                            history: &self.history,
                            forcing: None,
                        },
                        &[],
                        Some((
                            0.,
                            Forcing {
                                limit: crate::devices::Limit::Left,
                                timing: self.timing,
                            },
                        )),
                        None,
                    )?;
                    push(&mut plot, sink.as_deref_mut(), 0., &x, observed)?;
                }
            } else if t >= start {
                push(
                    &mut plot,
                    sink.as_deref_mut(),
                    t,
                    &x,
                    std::mem::take(&mut observed),
                )?;
            }
            if almost_equal_ulps(t, stop, 100) {
                break;
            }
            // resume:
            delta = delta.min(max_step);
            // Are we at a breakpoint, or indistinguishably close?
            if almost_equal_ulps(t, breaks.first(), 100) || breaks.first() - t <= min_step {
                // First timepoint after a breakpoint: cut the integration
                // order and limit the step to .1 times the minimum of the
                // time to the next breakpoint and the previous step.
                order = 1;
                delta = delta.min(0.1 * save_delta.min(breaks.second() - breaks.first()));
                if first {
                    // A breakpoint to reduce ringing of currents in devices.
                    if self.uic {
                        breaks.set(t, step)?;
                    }
                    delta /= 10.;
                }
                // XSPICE builds do not raise delta to 2 * CKTdelmin here.
            }
            // XSPICE: throw out permanent breakpoints at or very near the
            // current time, unless it is the final stop break.
            while (breaks.first() <= t + breaks.min_break()
                || almost_equal_ulps(breaks.first(), t, 100))
                && breaks.first() < stop
            {
                breaks.clear();
            }
            forced = false;
            if t + delta > breaks.first() {
                // Force the breakpoint.
                at_break = true;
                forced = true;
                save_delta = delta;
                delta = breaks.first() - t;
            } else if !almost_equal_ulps(t + delta, stop, 100) && t + 1.9 * delta > breaks.first() {
                // Equalise the last two steps before the breakpoint.
                save_delta = delta;
                delta = (breaks.first() - t) / 2.0;
            }
            loop {
                if self.stats.accepted + self.stats.rejected >= max_steps {
                    return Err(failure(format!(
                        "work limit of {max_steps} accepted + rejected steps reached at t = {t:e}"
                    )));
                }
                let old_delta = delta;
                t += delta;
                let coefficients = self.steps.trial(method, order, delta, self.settings.xmu)?;
                // The step ending at a breakpoint takes the left limit.
                let limit = if forced {
                    crate::devices::Limit::Left
                } else {
                    crate::devices::Limit::Right
                };
                match self.solve_trial(t, &coefficients, limit, &x, first)? {
                    Trial::NotConverged { last } => {
                        x = last;
                        t -= delta;
                        self.stats.rejected += 1;
                        delta /= 8.;
                        order = 1;
                        forced = false;
                    }
                    Trial::Converged { x: x_new, state } => {
                        let mut accept = first;
                        let mut next = delta;
                        if !first {
                            // CKTtrunc: newdelta = MIN(2 * delta, device bounds).
                            next = (2. * delta).min(self.truncation_limit(
                                &coefficients,
                                &state,
                                &x_new,
                                t,
                            )?);
                            if next > 0.9 * delta {
                                accept = true;
                                // Don't raise the order for backward Euler.
                                // dctran.c never probes or raises beyond
                                // order 2, whatever maxord (3..=6) allows.
                                if order == 1 && max_order > 1 {
                                    let probe =
                                        self.steps.trial(method, 2, delta, self.settings.xmu)?;
                                    order = 2;
                                    next = (2. * delta)
                                        .min(self.truncation_limit(&probe, &state, &x_new, t)?);
                                    if next <= 1.05 * delta {
                                        order = 1;
                                    }
                                }
                            }
                        }
                        if accept {
                            observed = if t >= start {
                                self.circuit.observe_real(
                                    &LoadRequest {
                                        mode: AnalysisMode::Transient {
                                            time: t,
                                            dt: coefficients.dt(),
                                        },
                                        solution: &x_new,
                                        model_context: &self.model_context,
                                        integration: Some(&coefficients),
                                        history: &self.history,
                                        forcing: Some(Forcing {
                                            limit,
                                            timing: self.timing,
                                        }),
                                    },
                                    &[],
                                    Some((
                                        t,
                                        Forcing {
                                            limit,
                                            timing: self.timing,
                                        },
                                    )),
                                    Some(&state),
                                )?
                            } else {
                                Vec::new()
                            };
                            // CKTaccept: TRAaccept's history update and
                            // breakpoint requests (set at nextTime below).
                            let acceptance = self.delay_acceptance(coefficients.dt(), false);
                            device_breaks = self.circuit.accept_transient_point(
                                &x_new,
                                t,
                                &mut self.history,
                                state,
                                &acceptance,
                            )?;
                            self.steps.accept(&coefficients);
                            self.record_step(delta, at_break);
                            x = x_new;
                            first = false;
                            delta = next;
                            break;
                        }
                        // Truncation error too large: redo with the bound,
                        // starting from this trial's solution (CKTrhsOld).
                        x = x_new;
                        t -= delta;
                        self.stats.rejected += 1;
                        delta = next;
                        forced = false;
                    }
                }
                // At or below delmin: retry once at delmin, then fail.
                if delta <= min_step {
                    if old_delta > min_step {
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
    ///   sources at their left limit; `.ic` node voltages are enforced as
    ///   `cktload.c`'s forced rows during this solve only
    ///   ([`crate::analysis::bias::NodeForcing`]),
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
            // dctran.c calls CKTop (NIiter) for every circuit, linear or not.
            let (x, solved) = {
                // CKTic: the Newton guess holds the .nodeset, then the .ic
                // values; cktload.c forces the .ic rows in every load of the
                // MODETRANOP solve and the .nodeset rows in its MODEINITJCT/
                // MODEINITFIX loads.
                let mut seed = Vector::zeros(self.circuit.unknown_count());
                for hint in self.hints.nodesets.iter().chain(&self.hints.initial) {
                    seed.as_mut_slice()[hint.row] = hint.value;
                }
                // Every entry is forced, as cktload.c does;
                // `irredundant_constraints` only rejects contradictions.
                let nodes = crate::analysis::bias::NodeForcing {
                    initial: self
                        .hints
                        .initial
                        .iter()
                        .map(|hint| (hint.row, hint.value))
                        .collect(),
                    nodesets: initial::forced_nodesets(&self.hints.nodesets),
                };
                let solution = crate::analysis::bias::solve_dc_forced(
                    self.circuit,
                    &self.model_context,
                    &self.bias,
                    &[],
                    Some(&seed),
                    // MODETRANOP: the sources' time functions at 0-.
                    Some(Forcing {
                        limit: crate::devices::Limit::Left,
                        timing: self.timing,
                    }),
                    &nodes,
                )?
                .solution;
                (solution.values, Some(solution.trial))
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
        // The t = 0 point also starts every transmission line's delay
        // history (traload.c, MODEINITTRAN); it requests no breakpoints.
        let acceptance = self.delay_acceptance(self.settings.max_step, true);
        self.circuit.accept_transient_point(
            &x,
            0.,
            &mut self.history,
            trial.clone(),
            &acceptance,
        )?;
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
            &mut self.circuit.new_matrix(),
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

    /// One trial point: C `NIiter` from `previous` (`CKTrhsOld`: the last
    /// iterate of the previous trial, accepted or not; ngspice is built
    /// without `PREDICTOR`), `MODEINITTRAN` before the first accepted step
    /// (`firsttime`) and `MODEINITPRED` after, for linear and nonlinear
    /// circuits alike. The trial is that of the last Newton load, one iterate
    /// behind the returned solution, as C's `CKTstate0`.
    fn solve_trial(
        &self,
        time: Real,
        coefficients: &Coefficients,
        limit: crate::devices::Limit,
        previous: &Vector,
        initial: bool,
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
                .with_device_limiting(options.limiting.is_device())
                .with_convergence_tolerances(options.reltol, options.abstol)
                .with_initial_transient(initial && phase == crate::devices::IterationPhase::Predict)
                .with_uic(self.uic);
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
        let limited = crate::analysis::bias::limited_rows(self.circuit);
        // dctran.c: MODEINITTRAN (first step) or MODEINITPRED for the first
        // load of a timepoint, MODEINITFLOAT afterwards.
        let policy = if initial {
            crate::analysis::newton::PhasePolicy::InitialTransient
        } else {
            crate::analysis::newton::PhasePolicy::Predicted
        };
        match crate::analysis::newton::solve_phased(
            previous,
            &self.branch_row,
            limited.as_deref(),
            &options,
            policy,
            None,
            |x, phase, last| {
                let mut matrix = self.circuit.new_matrix();
                let mut rhs = Vector::zeros(n);
                let state = load(x, &mut matrix, &mut rhs, phase, last)?;
                Ok((matrix, rhs, state))
            },
            |new, old, trial| {
                self.circuit
                    .convergence_test(&crate::devices::ConvergenceRequest {
                        solution: new,
                        previous: old,
                        trial,
                        model_context: &self.model_context,
                        time,
                        reltol: options.reltol,
                        abstol: options.abstol,
                        vntol: options.vntol,
                    })
            },
        ) {
            Ok(solved) => Ok(Trial::Converged {
                x: solved.values,
                state: solved.trial,
            }),
            Err(crate::analysis::newton::NewtonFailure {
                last_iterate: Some(last),
                ..
            }) => Ok(Trial::NotConverged { last }),
            Err(failure) => Err(failure.error),
        }
    }

    /// The delay-line quantities of an accepted point or a converged trial
    /// whose step is `dt`: C `CKTdeltaOld[0..3]` (`dt`, then the two latest
    /// accepted steps; steps not taken yet read as the maximum step, as
    /// `dctran.c` initializes `CKTdeltaOld`) and the `CKTminBreak` that
    /// `TRAaccept` compares sample spacing with: the XSPICE build's
    /// `10 CKTdelmin`, the same merge distance as the [`BreakTable`].
    fn delay_acceptance(&self, dt: Real, start: bool) -> crate::devices::DelayAcceptance {
        let accepted = self.steps.accepted();
        let older = |age: usize| accepted.get(age).copied().unwrap_or(self.settings.max_step);
        crate::devices::DelayAcceptance {
            steps: [dt, older(0), older(1)],
            min_break: 10. * self.settings.min_step(),
            start,
        }
    }

    /// C `CKTtrunc`: the smallest step bound over every charge-storage element,
    /// every discrete-state device ([`crate::devices::Device::timestep_limit`],
    /// `swtrunc.c`) and every delay line (`tratrunc.c`), from the converged
    /// trial `solution` at `time` and the accepted history.
    fn truncation_limit(
        &self,
        coefficients: &Coefficients,
        trial: &TrialState,
        solution: &Vector,
        time: Real,
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
        if let Some(bound) = self.circuit.delay_timestep_limit(
            &self.history,
            solution,
            time,
            &self.delay_acceptance(coefficients.dt(), false),
        )? {
            limit = limit.min(bound);
        }
        Ok(limit)
    }
}

/// Stores an accepted point in `plot`, or hands it to `sink` when there is one.
fn push<'s>(
    plot: &mut Plot,
    sink: Option<&mut (dyn PointSink + 's)>,
    time: Real,
    x: &Vector,
    observed: Vec<Complex>,
) -> SpiceResult<()> {
    let mut point = vec![Complex::real(time)];
    point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
    point.extend(observed);
    match sink {
        Some(sink) => {
            plot.check_point(&point)?;
            sink.point(&point)
        }
        None => plot.push_point(point),
    }
}
