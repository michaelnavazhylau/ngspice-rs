//! Explicitly selected diffsol BDF transient; companion-model trap/Gear stay separate.
use crate::linear::{accept, number, plot, unsupported};
use crate::{AnalysisRequest, Plot};
use spice_core::{Complex, SpiceResult};
use spice_devices::Circuit;
use spice_maths::diffsol::{BdfOptions, DaeSegment, LinearDae};

pub(crate) fn run(circuit: &mut Circuit, request: &AnalysisRequest) -> SpiceResult<Plot> {
    if request.named("backend") != Some("diffsol") || request.named("method") != Some("bdf") {
        return Err(unsupported(
            "transient requires explicit backend=diffsol method=bdf; ngspice trap/Gear companion methods are not implemented",
        ));
    }
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
            ".tran step stop [start [maxstep]] backend=diffsol method=bdf; .ic/uic are unsupported",
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
    let mut grid: Vec<_> = (0..=count as usize)
        .map(|i| start + (i as f64) * dt)
        .collect();
    if grid.last().copied() != Some(end) {
        grid.push(end);
    }
    if grid.windows(2).any(|w| w[0] >= w[1]) {
        return Err(unsupported("transient sample grid makes no progress"));
    }
    let system = circuit.linear_system()?;
    if system.has_initial_conditions {
        return Err(unsupported(
            "device ic= requires .ic/uic semantics; this backend starts from a linear operating point",
        ));
    }
    let dae = LinearDae::new(&system.a, &system.e)?;
    let op = system.a.solve(&system.dc_rhs(None)?)?;
    let mut x = dae.project(&op, &system.transient_rhs(0., false))?;
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
    accept(circuit, &x)?;
    if grid.first() == Some(&0.) {
        push(&mut plot, 0., &x)?;
    }
    let mut boundaries = system.breakpoints(end);
    boundaries.push(end);
    let mut segment_start = 0.;
    for segment_end in boundaries {
        let segment = DaeSegment {
            start: segment_start,
            end: segment_end,
            initial: x,
            b_start: system.transient_rhs(segment_start, false),
            b_end: system.transient_rhs(segment_end, true),
            samples: grid
                .iter()
                .copied()
                .filter(|t| *t > segment_start && *t < segment_end)
                .collect(),
        };
        let result = dae.integrate_segment(&segment, &options, &mut |_t, x| accept(circuit, x))?;
        options.max_steps -= result.steps;
        for (t, sample) in result.samples {
            push(&mut plot, t, &sample)?;
        }
        // Restart all BDF history and project algebraic states from the RIGHT.
        // Capacitor voltages and inductor currents never jump in this subset.
        x = dae.project(
            &result.final_state,
            &system.transient_rhs(segment_end, false),
        )?;
        if x != result.final_state {
            accept(circuit, &x)?;
        }
        if grid.contains(&segment_end) {
            push(&mut plot, segment_end, &x)?;
        }
        segment_start = segment_end;
    }
    Ok(plot)
}
fn push(plot: &mut Plot, time: f64, x: &spice_maths::Vector) -> SpiceResult<()> {
    let mut point = vec![Complex::real(time)];
    point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
    plot.push_point(point)
}
