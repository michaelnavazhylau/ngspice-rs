//! C-compatible moving polynomial interpolation (`maths/poly/interpolate.c`).
//! The absolute power basis and fit acceptance thresholds are intentional:
//! changing coordinates changes which degrees C falls back from.
use crate::maths::polynomial::{evaluate, fit_power_basis};
use crate::primitives::{Real, SpiceError, SpiceResult};
fn failure(message: &str) -> SpiceError {
    SpiceError::Numerical {
        context: "Fourier interpolation".into(),
        message: message.into(),
    }
}

fn fit(
    x: &[Real],
    y: &[Real],
    degree: usize,
    initial: bool,
    coefficients: &mut [Real],
) -> SpiceResult<usize> {
    let mut first = 0;
    for d in (1..=degree).rev() {
        if d < degree
            && (if initial {
                d % 2 == 1
            } else {
                (degree - d).is_multiple_of(2)
            })
        {
            first += 1;
        }
        // C clears only the coefficient prefix for a degree > 1 fit.
        // Retain the unused tail for its final interval's original-degree evaluation.
        if d > 1 {
            coefficients[..=d].fill(0.);
        }
        if let Some(c) = fit_power_basis(&x[first..first + d + 1], &y[first..first + d + 1]) {
            coefficients[..=d].copy_from_slice(&c);
            let acceptable = d == 1
                || x[first..first + d + 1]
                    .iter()
                    .zip(&y[first..first + d + 1])
                    .all(|(x, y)| {
                        let p = evaluate(&c, *x);
                        let error = (p - y).abs();
                        error <= 0.001 && error / p.abs().max(0.001) <= 0.001
                    });
            if acceptable {
                return Ok(d);
            }
        }
    }
    Err(failure("no usable polynomial degree"))
}

/// Resample onto an ascending physical grid with C's degree fallback and edge policy.
pub(super) fn resample(
    times: &[Real],
    values: &[Real],
    grid: &[Real],
    degree: usize,
) -> SpiceResult<Vec<Real>> {
    if times.len() != values.len()
        || times.len() <= degree
        || degree == 0
        || degree > 16
        || grid.len() < 2
        || times
            .iter()
            .chain(values)
            .chain(grid)
            .any(|x| !x.is_finite())
        || times.windows(2).any(|w| w[0] > w[1])
        || grid.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(failure("invalid interpolation samples or degree"));
    }
    let middle = degree.div_ceil(2);
    let mut start = 0;
    while start + degree + 1 < times.len() && times[start + middle] < grid[0] {
        start += 1;
    }
    let mut x = vec![times[start]];
    let mut y = vec![values[start]];
    let mut end = start;
    while x.len() <= degree && end + 1 < times.len() {
        if times[end + 1] == times[end] {
            if x.len() == 1 {
                end += 1;
                y[0] = values[end];
                continue;
            }
            let k = x.len() - 1;
            x[k] -= (x[k] - x[k - 1]) * 0.001;
        }
        end += 1;
        x.push(times[end]);
        y.push(values[end]);
    }
    if x.len() <= degree {
        return Err(failure("too few distinct samples"));
    }
    let mut c = vec![0.; degree + 1];
    let mut used = fit(&x, &y, degree, true, &mut c)?;
    let mut out = Vec::with_capacity(grid.len());
    while out.len() < grid.len() && grid[out.len()] <= x[middle] {
        out.push(evaluate(&c[..=used], grid[out.len()]));
    }
    for next in end + 1..times.len() {
        if out.len() == grid.len() {
            break;
        }
        let discarded = x[0];
        x.rotate_left(1);
        y.rotate_left(1);
        x[degree] = times[next];
        y[degree] = values[next];
        if x[degree] == x[degree - 1] {
            let width = if degree == 1 {
                x[0] - discarded
            } else {
                x[degree - 1] - x[degree - 2]
            };
            x[degree - 1] -= width * 0.001;
        }
        if next < times.len() - degree && x[middle] < grid[out.len()] {
            continue;
        }
        used = fit(&x, &y, degree, false, &mut c)?;
        while out.len() < grid.len() && grid[out.len()] <= x[middle] {
            out.push(evaluate(&c[..=used], grid[out.len()]));
        }
    }
    while out.len() < grid.len() {
        out.push(evaluate(&c, grid[out.len()]));
    }
    if out.iter().any(|v| !v.is_finite()) {
        return Err(failure("nonfinite interpolated value"));
    }
    Ok(out)
}
