//! Linear complex small-signal analysis, using faer (not diffsol).
use crate::linear::{number, plot, unsupported};
use crate::{AnalysisRequest, Plot};
use spice_core::{Complex, SpiceResult};
use spice_devices::Circuit;
use spice_maths::complex::ComplexMatrix;

pub(crate) fn run(circuit: &mut Circuit, request: &AnalysisRequest) -> SpiceResult<Plot> {
    if request.arguments.len() != 4 {
        return Err(unsupported(".ac lin|dec|oct points start stop"));
    }
    let mode = request.argument(0).unwrap().to_ascii_lowercase();
    let points: usize = request
        .argument(1)
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0 && *n <= 100_000)
        .ok_or_else(|| unsupported("AC points must be in 1..=100000"))?;
    let start = number(request.argument(2), "start frequency")?;
    let end = number(request.argument(3), "stop frequency")?;
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
            let count = (span * (points as f64)).floor() + 1.;
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
    let system = circuit.linear_system()?;
    // Require a valid bias point even for linear AC; don't accept isolated
    // capacitor networks as an implicit substitute for DC initialization.
    system.a.solve(&system.dc_rhs(None)?)?;
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
