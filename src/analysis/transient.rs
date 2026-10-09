//! `.tran` backend dispatch.
//!
//! * no `backend=` (or `backend=companion`): the SPICE-compatible adaptive
//!   trapezoidal / Gear-2 companion driver ([`crate::analysis::companion`]);
//! * `backend=diffsol method=bdf`: the explicitly selected bounded diffsol BDF
//!   backend below, which is not ngspice trap/Gear and shares no state with the
//!   companion driver.
use crate::analysis::linear::{number, plot, unsupported};
use crate::analysis::{AnalysisRequest, Plot};
use crate::devices::{Circuit, Limit, TransientTiming};
use crate::maths::diffsol::{BdfOptions, DaeSegment, LinearDae};
use crate::primitives::{Complex, SpiceError, SpiceResult};

/// Upper bound on source-breakpoint segments in one run.
const MAX_SEGMENTS: usize = 100_000;

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Plot> {
    match request.named("backend") {
        None => crate::analysis::companion::companion_transient(circuit, request, context)
            .map(|(plot, _stats)| plot),
        Some(name) if name.eq_ignore_ascii_case("companion") => {
            crate::analysis::companion::companion_transient(circuit, request, context)
                .map(|(plot, _stats)| plot)
        }
        Some(name) if name.eq_ignore_ascii_case("diffsol") => {
            run_diffsol(circuit, request, context)
        }
        Some(name) => Err(unsupported(format!(
            "unknown transient backend '{name}'; expected companion (default) or diffsol"
        ))),
    }
}

fn run_diffsol(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Plot> {
    if request.named("method") != Some("bdf") {
        return Err(unsupported(
            "backend=diffsol requires method=bdf (diffsol adaptive BDF is not ngspice trap/Gear;              omit backend= for the companion trap/gear driver)",
        ));
    }
    if request.uic {
        return Err(unsupported(
            ".tran uic is implemented only by the companion driver (omit backend=diffsol); \
             the diffsol BDF backend has no initial-condition formulation",
        ));
    }
    if let Some(entry) = request.initial_conditions.first() {
        return Err(SpiceError::Unsupported {
            feature: ".ic is implemented only by the companion driver (omit backend=diffsol); \
                      the diffsol BDF backend would silently start from the DC operating point"
                .into(),
            location: Some(entry.location.clone()),
        });
    }
    // .nodeset only steers DC convergence and cannot change a linear operating
    // point; it is validated (unknown nodes) and otherwise has no effect.
    crate::analysis::initial::resolve(circuit, request)?;
    let mut positional = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for a in &request.arguments {
        if let Some((key, _)) = a.split_once('=') {
            if !seen.insert(key.to_ascii_lowercase()) {
                return Err(unsupported(format!("duplicate transient option {key}")));
            }
            if !["backend", "method", "rtol", "vntol", "abstol", "maxsteps"]
                .iter()
                .any(|k| key.eq_ignore_ascii_case(k))
            {
                return Err(unsupported(format!(
                    "diffsol transient option {key} (including maxord)"
                )));
            }
        } else {
            positional.push(a.as_str());
        }
    }
    if !(2..=4).contains(&positional.len()) {
        return Err(unsupported(
            ".tran step stop [start [maxstep]] backend=diffsol method=bdf; .ic/uic/instance ic= are unsupported",
        ));
    }
    let dt = number(positional.first().copied(), "sample step")?;
    let end = number(positional.get(1).copied(), "stop time")?;
    let start = number(
        Some(positional.get(2).copied().unwrap_or("0")),
        "start time",
    )?;
    let maxstep = if let Some(s) = positional.get(3) {
        number(Some(s), "maximum step")?
    } else {
        dt
    };
    if dt <= 0. || end <= 0. || start < 0. || start >= end || maxstep <= 0. {
        return Err(unsupported("invalid transient time bounds"));
    }
    let count = ((end - start) / dt).floor();
    if !count.is_finite() || !(0. ..=100_000.).contains(&count) {
        return Err(unsupported("transient sample limit exceeded"));
    }
    // `start + i * dt` can land an ulp short of (or past) `end` when `end` is a
    // decimal multiple of `dt`; such a point is the final sample, not a second one.
    let mut grid: Vec<_> = (0..=count as usize)
        .map(|i| start + (i as f64) * dt)
        .filter(|t| end - t > 1e-9 * dt)
        .collect();
    grid.push(end);
    if grid.windows(2).any(|w| w[0] >= w[1]) {
        return Err(unsupported("transient sample grid makes no progress"));
    }
    let mut system = circuit.linear_system_with_context(&context.model_context())?;
    // C resolves PULSE TR/TF/PW/PER defaults from CKTstep and CKTfinalTime.
    system.bind_transient_timing(&TransientTiming::new(dt, end)?)?;
    // DaeSegment interpolates the forcing linearly between breakpoints, which
    // is exact only for piecewise-linear sources; never approximate SIN/EXP/
    // SFFM/AM that way.
    if let Some(source) = system
        .sources
        .iter()
        .find(|s| !s.waveform.is_piecewise_linear())
    {
        return Err(unsupported(format!(
            "{}: SIN/EXP/SFFM/AM forcing is implemented only by the companion driver \
             (omit backend=diffsol); the diffsol BDF backend needs piecewise-linear sources",
            source.name
        )));
    }
    if system.has_initial_conditions {
        return Err(unsupported(
            "device ic= is implemented only by the companion driver (omit backend=diffsol); \
             the diffsol BDF backend starts from a linear operating point",
        ));
    }
    let dae = LinearDae::new(&system.a, &system.e)?;
    // The initial operating point uses the forcing just before t=0 (left limit),
    // as C's MODETRANOP evaluates the waveform at time zero, not the separate DC
    // value. The state is then projected onto the constraints from the right.
    let op = system.a.solve(&system.transient_rhs(0., Limit::Left)?)?;
    let mut x = dae.project(&op, &system.transient_rhs(0., Limit::Right)?)?;
    let rtol = number(
        Some(request.named("rtol").unwrap_or("1e-7")),
        "relative tolerance",
    )?;
    let vntol = number(
        Some(request.named("vntol").unwrap_or("1e-9")),
        "voltage tolerance",
    )?;
    let abstol = number(
        Some(request.named("abstol").unwrap_or("1e-12")),
        "current tolerance",
    )?;
    let mut options = BdfOptions {
        rtol,
        atol: vec![vntol; x.len()],
        max_step: maxstep,
        max_steps: 100_000,
    };
    if let Some(text) = request.named("maxsteps") {
        options.max_steps = text
            .parse()
            .ok()
            .filter(|n| *n > 0 && *n <= 1_000_000)
            .ok_or_else(|| unsupported("maxsteps must be an integer in 1..=1000000"))?;
    }
    if rtol <= 0. || vntol <= 0. || abstol <= 0. {
        return Err(unsupported("transient tolerances must be positive"));
    }
    for i in 0..circuit.device_count() {
        for row in circuit.branch_rows(i).unwrap() {
            options.atol[row] = abstol;
        }
    }
    let mut plot = plot(
        circuit,
        "tran1",
        "Transient Analysis",
        Some(("time", "time")),
        false,
    )?;
    circuit.accept_solution(&x, Some(0.))?;
    if grid.first() == Some(&0.) {
        push(&mut plot, 0., &x)?;
    }
    // Breakpoints are enumerated lazily and consumed under an explicit budget so
    // a fast periodic source cannot expand without bound.
    let mut boundaries = vec![];
    for t in system.breakpoints_in(0., end)? {
        if t <= 0. || t >= end {
            continue;
        }
        if boundaries.len() >= MAX_SEGMENTS {
            return Err(unsupported(format!(
                "source breakpoint limit ({MAX_SEGMENTS}) exceeded before the stop time"
            )));
        }
        boundaries.push(t);
    }
    boundaries.push(end);
    let mut segment_start = 0.;
    for segment_end in boundaries {
        let segment = DaeSegment {
            start: segment_start,
            end: segment_end,
            initial: x,
            b_start: system.transient_rhs(segment_start, Limit::Right)?,
            b_end: system.transient_rhs(segment_end, Limit::Left)?,
            samples: grid
                .iter()
                .copied()
                .filter(|t| *t > segment_start && *t < segment_end)
                .collect(),
        };
        let result = dae.integrate_segment(&segment, &options, &mut |t, x| {
            circuit.accept_solution(x, Some(t))
        })?;
        options.max_steps -= result.steps;
        for (t, sample) in result.samples {
            push(&mut plot, t, &sample)?;
        }
        // Restart all BDF history and project onto the constraints from the
        // RIGHT. The projection moves only along ker E, so capacitor charges
        // (including floating/coupled ones) and inductor fluxes never jump.
        x = dae.project(
            &result.final_state,
            &system.transient_rhs(segment_end, Limit::Right)?,
        )?;
        if x != result.final_state {
            circuit.accept_solution(&x, Some(segment_end))?;
        }
        if grid.contains(&segment_end) {
            push(&mut plot, segment_end, &x)?;
        }
        segment_start = segment_end;
    }
    Ok(plot)
}
fn push(plot: &mut Plot, time: f64, x: &crate::maths::Vector) -> SpiceResult<()> {
    let mut point = vec![Complex::real(time)];
    point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
    plot.push_point(point)
}
