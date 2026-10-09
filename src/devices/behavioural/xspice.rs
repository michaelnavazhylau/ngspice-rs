//! The static transfer map of the XSPICE `pwl` code model, as `inpcom.c`
//! instantiates it for an E/G `TABLE` (`fraction=TRUE limit=TRUE`).
//!
//! C reference (behaviour only): `src/xspice/icm/analog/pwl/cfunc.mod`
//! (`cm_pwl`) and `src/xspice/cm/cmutil.c` (`cm_smooth_corner`). The table is
//! extended by one mirrored abscissa at each end whose ordinate repeats the
//! end value (`limit=TRUE`), every interior corner is replaced by a parabola
//! over `±fraction` of its shorter adjacent segment, and outside the first and
//! last midpoints the extended end segments (flat) apply.
//!
//! The model's iteration-to-iteration input limiting (`limit_x_value`, a
//! Newton aid that flags non-convergence while the input moves by more than
//! 30 % of a segment) is not reproduced: it changes the iteration path, not
//! the converged operating point.

use crate::primitives::{Real, SpiceError, SpiceResult};

/// A validated smoothed piecewise-linear transfer map.
#[derive(Debug, Clone, PartialEq)]
pub struct XspicePwl {
    x: Vec<Real>,
    y: Vec<Real>,
    fraction: Real,
}

impl XspicePwl {
    /// Builds the map from `(x, y)` points and the smoothing `fraction`.
    ///
    /// # Errors
    /// Fewer than two points, non-finite values, abscissas that do not
    /// strictly increase (the C model silently misbehaves on those) or a
    /// fraction outside the model's `input_domain` limits `[1e-12, 0.5]`.
    pub fn new(points: &[(Real, Real)], fraction: Real) -> SpiceResult<Self> {
        if points.len() < 2 {
            return Err(invalid("a TABLE needs at least two (x, y) points"));
        }
        if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return Err(invalid("TABLE points must be finite"));
        }
        if points.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(invalid(
                "TABLE abscissas must strictly increase (the XSPICE pwl model has no \
                 ordering check and misbehaves otherwise)",
            ));
        }
        if !(1e-12..=0.5).contains(&fraction) {
            return Err(invalid("TABLE smoothing domain outside [1e-12, 0.5]"));
        }
        let n = points.len();
        let mut x = Vec::with_capacity(n + 2);
        let mut y = Vec::with_capacity(n + 2);
        x.push(2. * points[0].0 - points[1].0);
        y.push(points[0].1);
        for (px, py) in points {
            x.push(*px);
            y.push(*py);
        }
        x.push(2. * points[n - 1].0 - points[n - 2].0);
        y.push(points[n - 1].1);
        Ok(Self { x, y, fraction })
    }

    /// The output and its derivative with respect to the input.
    #[must_use]
    pub fn evaluate(&self, input: Real) -> (Real, Real) {
        let (x, y) = (&self.x, &self.y);
        let size = x.len();
        let segment = |lower: usize, at: usize| {
            let slope = (y[lower + 1] - y[lower]) / (x[lower + 1] - x[lower]);
            (y[at] + (input - x[at]) * slope, slope)
        };
        if input <= (x[0] + x[1]) / 2. {
            return segment(0, 0);
        }
        if input >= (x[size - 2] + x[size - 1]) / 2. {
            return segment(size - 2, size - 1);
        }
        for i in 1..size - 1 {
            if input >= (x[i] + x[i + 1]) / 2. {
                continue;
            }
            let lower = x[i] - x[i - 1];
            let upper = x[i + 1] - x[i];
            let domain = self.fraction * lower.min(upper);
            if input < x[i] - domain {
                return segment(i - 1, i);
            }
            if input < x[i] + domain {
                let lower_slope = (y[i] - y[i - 1]) / lower;
                let upper_slope = (y[i + 1] - y[i]) / upper;
                return smooth_corner(input, x[i], y[i], domain, lower_slope, upper_slope);
            }
            return segment(i, i);
        }
        // Unreachable for finite input: the last midpoint test above caught it.
        segment(size - 2, size - 1)
    }
}

/// The parabola joining two slopes over `centre ± domain`, matching value and
/// slope at both ends (`cm_smooth_corner`).
fn smooth_corner(
    input: Real,
    centre: Real,
    value: Real,
    domain: Real,
    lower_slope: Real,
    upper_slope: Real,
) -> (Real, Real) {
    let x_upper = centre + domain;
    let y_upper = value + upper_slope * domain;
    let a = ((upper_slope - lower_slope) / 4.) * (1. / domain);
    let b = upper_slope - 2. * a * x_upper;
    let c = y_upper - a * x_upper * x_upper - b * x_upper;
    (a * input * input + b * input + c, 2. * a * input + b)
}

fn invalid(message: &str) -> SpiceError {
    SpiceError::Unsupported {
        feature: message.to_owned(),
        location: None,
    }
}

#[cfg(test)]
mod tests {
    use super::XspicePwl;

    #[test]
    fn table_is_flat_outside_and_smooth_at_corners() {
        let map = XspicePwl::new(&[(0., 0.), (1., 2.), (2., 3.)], 0.1).unwrap();
        assert_eq!(map.evaluate(-5.), (0., 0.));
        assert_eq!(map.evaluate(10.), (3., 0.));
        // Linear away from corners.
        let (y, slope) = map.evaluate(0.5);
        assert!((y - 1.).abs() < 1e-15 && (slope - 2.).abs() < 1e-15);
        // C's operating point of `table {v(in)} = (0,0) (1,2) (2,3)` at 1 V.
        assert!((map.evaluate(1.).0 - 1.975).abs() < 1e-12);
        // Continuity and slope continuity at the parabola ends.
        for (edge, h) in [(0.9, 1e-9), (1.1, 1e-9), (-0.1, 1e-9), (0.1, 1e-9)] {
            let below = map.evaluate(edge - h);
            let above = map.evaluate(edge + h);
            assert!((below.0 - above.0).abs() < 1e-8, "{edge}");
            assert!((below.1 - above.1).abs() < 1e-6, "{edge}");
        }
    }

    #[test]
    fn derivative_matches_finite_differences() {
        let map = XspicePwl::new(&[(-1., -0.5), (0., 0.), (1., 2.), (3., 2.5)], 0.1).unwrap();
        let mut x = -2.;
        while x < 4. {
            let h = 1e-7;
            let numeric = (map.evaluate(x + h).0 - map.evaluate(x - h).0) / (2. * h);
            let (_, analytic) = map.evaluate(x);
            assert!(
                (numeric - analytic).abs() < 1e-5,
                "{x}: {numeric} {analytic}"
            );
            x += 0.0137;
        }
    }

    #[test]
    fn invalid_tables_are_rejected() {
        assert!(XspicePwl::new(&[(0., 0.)], 0.1).is_err());
        assert!(XspicePwl::new(&[(1., 0.), (0., 1.)], 0.1).is_err());
        assert!(XspicePwl::new(&[(0., 0.), (0., 1.)], 0.1).is_err());
        assert!(XspicePwl::new(&[(0., 0.), (1., f64::NAN)], 0.1).is_err());
    }
}
