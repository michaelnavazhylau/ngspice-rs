//! Small, bounded power-basis polynomial fits for front-end resampling.
//! This is independent of the MNA factorization and its rank/residual policy.
use crate::primitives::Real;

/// Fits the unique polynomial through distinct finite samples in absolute coordinates.
/// Uses partial-pivot Gaussian elimination; returns `None` on zero pivots,
/// nonfinite arithmetic, mismatched inputs or more than 17 coefficients.
#[must_use]
pub fn fit_power_basis(x: &[Real], y: &[Real]) -> Option<Vec<Real>> {
    let n = x.len();
    if !(2..=17).contains(&n) || n != y.len() || x.iter().chain(y).any(|v| !v.is_finite()) {
        return None;
    }
    if n == 2 {
        let dx = x[1] - x[0];
        let result = vec![x[1].mul_add(y[0], -x[0] * y[1]) / dx, (y[1] - y[0]) / dx];
        return result.iter().all(|v| v.is_finite()).then_some(result);
    }
    let mut a = vec![vec![0.; n]; n];
    let mut b = y.to_vec();
    for (row, t) in a.iter_mut().zip(x) {
        let mut power = 1.;
        for value in row {
            *value = power;
            power *= t;
        }
    }
    for col in 0..n {
        let mut pivot = col;
        for row in col..n {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        if a[col][col] == 0. {
            return None;
        }
        for row in col + 1..n {
            let factor = a[row][col] / a[col][col];
            let pivot_values = a[col].clone();
            for (value, pivot_value) in a[row].iter_mut().zip(pivot_values) {
                *value = (-factor).mul_add(pivot_value, *value);
            }
            b[row] = (-factor).mul_add(b[col], b[row]);
        }
    }
    for col in (1..n).rev() {
        for row in (0..col).rev() {
            let factor = a[row][col] / a[col][col];
            let pivot_values = a[col].clone();
            for (value, pivot_value) in a[row].iter_mut().zip(pivot_values) {
                *value = (-factor).mul_add(pivot_value, *value);
            }
            b[row] = (-factor).mul_add(b[col], b[row]);
        }
    }
    let result: Vec<_> = (0..n).map(|i| b[i] / a[i][i]).collect();
    result.iter().all(|v| v.is_finite()).then_some(result)
}

/// Evaluates coefficients ordered from constant to highest power using Horner's rule.
#[must_use]
pub fn evaluate(coefficients: &[Real], x: Real) -> Real {
    coefficients.iter().rev().fold(0., |sum, c| sum * x + c)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fits_polynomials_and_rejects_degenerate_samples() {
        let x = [-2., -0.5, 1., 3.];
        let y = x.map(|x| 2. - 3. * x + 4. * x * x - x * x * x);
        let c = fit_power_basis(&x, &y).unwrap();
        for (a, b) in c.iter().zip([2., -3., 4., -1.]) {
            assert!((a - b).abs() < 1e-12);
        }
        assert!(fit_power_basis(&[1., 1.], &[1., 2.]).is_none());
        assert!(fit_power_basis(&[0., 1.], &[1.]).is_none());
    }
}
