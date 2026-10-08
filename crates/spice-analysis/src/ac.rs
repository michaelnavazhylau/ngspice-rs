//! Linear complex small-signal analysis, using faer (not diffsol).
use crate::linear::{number, plot, unsupported};
use crate::{AnalysisRequest, Plot};
use spice_core::{Complex, SpiceResult};
use spice_devices::Circuit;
use spice_maths::complex::ComplexMatrix;

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::AnalysisContext,
) -> SpiceResult<Plot> {
    let settings = crate::bias::DcSettings::from_request(request)?;
    let positional = AnalysisRequest::with_arguments(
        request.kind,
        request
            .arguments
            .iter()
            .filter(|a| !a.contains('='))
            .cloned(),
    );
    if positional.arguments.len() != 4 {
        return Err(unsupported(".ac lin|dec|oct points start stop"));
    }
    let mode = positional.argument(0).unwrap().to_ascii_lowercase();
    let points: usize = positional
        .argument(1)
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0 && *n <= 100_000)
        .ok_or_else(|| unsupported("AC points must be in 1..=100000"))?;
    let start = number(positional.argument(2), "start frequency")?;
    let end = number(positional.argument(3), "stop frequency")?;
    if start <= 0. || end < start {
        return Err(unsupported("invalid AC frequency bounds"));
    }
    let grid = match mode.as_str() {
        "lin" => (0..points)
            .map(|i| {
                if points == 1 {
                    start
                } else {
                    (1. - (i as f64) / ((points - 1) as f64)) * start
                        + (i as f64) / ((points - 1) as f64) * end
                }
            })
            .collect::<Vec<_>>(),
        "dec" | "oct" => {
            let base: f64 = if mode == "dec" { 10. } else { 2. };
            let span = (end.ln() - start.ln()) / base.ln();
            let steps = span * (points as f64);
            // Logarithms can put an exact integer span just below that integer
            // (3 points/decade, 1..1000 gives 8.999999999999998). Snap only
            // within a small floating-arithmetic bound, preserving the endpoint.
            // This does not relax any golden/value comparison tolerance.
            let rounded = steps.round();
            let steps = if (steps - rounded).abs() <= 32. * f64::EPSILON * steps.abs().max(1.) {
                rounded
            } else {
                steps
            };
            let count = steps.floor() + 1.;
            if !count.is_finite() || count > 100_000. {
                return Err(unsupported("AC sample limit exceeded"));
            }
            (0..count as usize)
                .map(|i| {
                    (start.ln() + base.ln() * (i as f64) / (points as f64))
                        .exp()
                        .min(end)
                })
                .collect()
        }
        _ => return Err(unsupported("AC sweep mode must be lin, dec or oct")),
    };
    if grid.windows(2).any(|w| w[0] >= w[1]) {
        return Err(unsupported("AC frequency grid makes no progress"));
    }
    circuit.finalize()?;
    let hints = crate::initial::resolve(circuit, request)?;
    let mut seed = spice_maths::Vector::zeros(circuit.unknown_count());
    for hint in hints.nodesets {
        seed.as_mut_slice()[hint.row] = hint.value;
    }
    // AC is linearized only after a valid physical DC solution, never at zero.
    let solved = crate::bias::solve_dc_with(
        circuit,
        &context.model_context(),
        &settings,
        &[],
        Some(&seed),
        None,
    )?
    .solution;
    // Devices with discrete state (switches) linearize at the operating
    // point's converged state, not a reload from their initial flags.
    let system = circuit.small_signal_system_at(
        &context.model_context(),
        &solved.values,
        Some(&solved.trial),
    )?;
    let mut plot = plot(
        circuit,
        "ac1",
        "AC Analysis",
        Some(("frequency", "frequency")),
        true,
    )?;
    for f in grid {
        let matrix =
            ComplexMatrix::from_operators(&system.a, &system.e, 2. * std::f64::consts::PI * f)?;
        let x = matrix.factorize()?.solve(&system.ac_rhs())?;
        let mut point = vec![Complex::real(f)];
        point.extend(x);
        plot.push_point(point)?;
    }
    Ok(plot)
}
