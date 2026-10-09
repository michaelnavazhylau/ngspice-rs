//! Small dense complex matrices for port-level algebra.
//!
//! Mirrors the `CMat` helpers of `src/maths/dense/dense.c` that the `.sp`
//! analysis uses to turn incident/scattered power waves into S, Y and Z
//! matrices (`cmultiply`, `csum`, `cminus`, `ceye`, `cinverse`). These are
//! `N x N` matrices for `N` RF ports, never the MNA system, so a plain
//! row-major store with Gauss-Jordan inversion (partial pivoting, as C's
//! `cinv_gj`) is adequate; the MNA solves stay with the faer sparse LU of
//! [`crate::maths::complex`].

use crate::primitives::{Complex, SpiceError, SpiceResult};

fn numerical(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "dense complex matrix".to_owned(),
        message: message.into(),
    }
}

/// A dense, row-major, square complex matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct DenseComplex {
    n: usize,
    data: Vec<Complex>,
}

impl DenseComplex {
    /// The `n x n` zero matrix.
    #[must_use]
    pub fn zeros(n: usize) -> Self {
        Self {
            n,
            data: vec![Complex::ZERO; n * n],
        }
    }

    /// The `n x n` identity (`ceye`).
    #[must_use]
    pub fn identity(n: usize) -> Self {
        Self::diagonal(&vec![Complex::real(1.); n])
    }

    /// A diagonal matrix.
    #[must_use]
    pub fn diagonal(values: &[Complex]) -> Self {
        let mut matrix = Self::zeros(values.len());
        for (i, value) in values.iter().enumerate() {
            matrix.data[i * values.len() + i] = *value;
        }
        matrix
    }

    /// The dimension `n`.
    #[must_use]
    pub const fn dim(&self) -> usize {
        self.n
    }

    /// The entry at `(row, col)`, or `None` out of range.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<Complex> {
        (row < self.n && col < self.n).then(|| self.data[row * self.n + col])
    }

    /// Sets the entry at `(row, col)`.
    /// # Errors
    /// An index out of range.
    pub fn set(&mut self, row: usize, col: usize, value: Complex) -> SpiceResult<()> {
        if row >= self.n || col >= self.n {
            return Err(numerical(format!(
                "index ({row}, {col}) is out of range for a {0}x{0} matrix",
                self.n
            )));
        }
        self.data[row * self.n + col] = value;
        Ok(())
    }

    /// The entries in row-major order.
    #[must_use]
    pub fn data(&self) -> &[Complex] {
        &self.data
    }

    fn same_dimension(&self, other: &Self) -> SpiceResult<()> {
        if self.n == other.n {
            Ok(())
        } else {
            Err(numerical(format!(
                "dimension mismatch {0}x{0} against {1}x{1}",
                self.n, other.n
            )))
        }
    }

    fn checked(self) -> SpiceResult<Self> {
        if self.data.iter().all(|value| value.is_finite()) {
            Ok(self)
        } else {
            Err(numerical("non-finite result"))
        }
    }

    /// `self + other` (`csum`).
    /// # Errors
    /// Mismatched dimensions or a non-finite result.
    pub fn add(&self, other: &Self) -> SpiceResult<Self> {
        self.same_dimension(other)?;
        Self {
            n: self.n,
            data: self
                .data
                .iter()
                .zip(&other.data)
                .map(|(a, b)| *a + *b)
                .collect(),
        }
        .checked()
    }

    /// `self - other` (`cminus`).
    /// # Errors
    /// Mismatched dimensions or a non-finite result.
    pub fn sub(&self, other: &Self) -> SpiceResult<Self> {
        self.same_dimension(other)?;
        Self {
            n: self.n,
            data: self
                .data
                .iter()
                .zip(&other.data)
                .map(|(a, b)| *a - *b)
                .collect(),
        }
        .checked()
    }

    /// `self * other` (`cmultiply`).
    /// # Errors
    /// Mismatched dimensions or a non-finite result.
    pub fn mul(&self, other: &Self) -> SpiceResult<Self> {
        self.same_dimension(other)?;
        let n = self.n;
        let mut out = Self::zeros(n);
        for i in 0..n {
            for k in 0..n {
                let a = self.data[i * n + k];
                if a == Complex::ZERO {
                    continue;
                }
                for j in 0..n {
                    out.data[i * n + j] = out.data[i * n + j] + a * other.data[k * n + j];
                }
            }
        }
        out.checked()
    }

    /// The inverse by Gauss-Jordan elimination with partial pivoting, or
    /// `None` when the matrix is singular to working precision: a pivot no
    /// larger than `128 n eps` times the largest entry magnitude (the
    /// threshold of the port's complex LU rank guard). C's `cinv_gj` only
    /// stops at an exactly zero pivot, so rounding-level pivots of a
    /// mathematically singular matrix give it huge, rounding-dependent
    /// entries instead.
    /// # Errors
    /// A non-finite entry or result.
    pub fn inverse(&self) -> SpiceResult<Option<Self>> {
        let n = self.n;
        if self.data.iter().any(|value| !value.is_finite()) {
            return Err(numerical("non-finite entry"));
        }
        let scale = self
            .data
            .iter()
            .fold(0_f64, |m, value| m.max(value.magnitude()));
        if scale == 0. {
            return Ok((n == 0).then(|| self.clone()));
        }
        let threshold = 128. * f64::EPSILON * (n as f64) * scale;
        let mut a = self.data.clone();
        let mut b = Self::identity(n).data;
        for c in 0..n {
            let pivot_row = (c..n)
                .max_by(|&x, &y| {
                    a[x * n + c]
                        .magnitude()
                        .total_cmp(&a[y * n + c].magnitude())
                })
                .unwrap_or(c);
            if a[pivot_row * n + c].magnitude() <= threshold {
                return Ok(None);
            }
            if pivot_row != c {
                for j in 0..n {
                    a.swap(c * n + j, pivot_row * n + j);
                    b.swap(c * n + j, pivot_row * n + j);
                }
            }
            let inverse_pivot = Complex::real(1.) / a[c * n + c];
            for j in 0..n {
                a[c * n + j] = a[c * n + j] * inverse_pivot;
                b[c * n + j] = b[c * n + j] * inverse_pivot;
            }
            for i in 0..n {
                let factor = a[i * n + c];
                if i == c || factor == Complex::ZERO {
                    continue;
                }
                for j in 0..n {
                    a[i * n + j] = a[i * n + j] - factor * a[c * n + j];
                    b[i * n + j] = b[i * n + j] - factor * b[c * n + j];
                }
            }
        }
        Self { n, data: b }.checked().map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::DenseComplex;
    use crate::primitives::Complex;

    fn matrix(n: usize, values: &[(f64, f64)]) -> DenseComplex {
        let mut m = DenseComplex::zeros(n);
        for (index, (re, im)) in values.iter().enumerate() {
            m.set(index / n, index % n, Complex::new(*re, *im)).unwrap();
        }
        m
    }

    #[test]
    fn inverse_times_matrix_is_identity_with_pivoting() {
        // A zero leading entry needs a row exchange.
        let a = matrix(
            3,
            &[
                (0., 0.),
                (2., 1.),
                (1., 0.),
                (1., -1.),
                (0., 0.),
                (3., 0.),
                (4., 0.),
                (1., 1.),
                (0., 2.),
            ],
        );
        let inverse = a.inverse().unwrap().expect("nonsingular");
        let product = a.mul(&inverse).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1. } else { 0. };
                let value = product.get(i, j).unwrap();
                assert!((value.re - expected).abs() < 1e-14, "{i},{j}: {value}");
                assert!(value.im.abs() < 1e-14, "{i},{j}: {value}");
            }
        }
    }

    #[test]
    fn singular_matrices_have_no_inverse() {
        let exact = matrix(2, &[(1., 0.), (-1., 0.), (-1., 0.), (1., 0.)]);
        assert_eq!(exact.inverse().unwrap(), None);
        // Singular up to rounding, as E - S of a series resistor.
        let third = 1. / 3.;
        let rounded = matrix(
            2,
            &[
                (1. - third, 0.),
                (-(1. - third) + 1e-17, 0.),
                (-(1. - third), 0.),
                (1. - third, 0.),
            ],
        );
        assert_eq!(rounded.inverse().unwrap(), None);
        assert_eq!(DenseComplex::zeros(2).inverse().unwrap(), None);
        assert_eq!(
            DenseComplex::zeros(0).inverse().unwrap(),
            Some(DenseComplex::zeros(0))
        );
    }

    #[test]
    fn arithmetic_checks_dimensions_and_finiteness() {
        let two = DenseComplex::identity(2);
        let three = DenseComplex::identity(3);
        assert!(two.add(&three).is_err());
        assert!(two.sub(&three).is_err());
        assert!(two.mul(&three).is_err());
        assert!(two.clone().set(2, 0, Complex::ZERO).is_err());
        assert_eq!(two.get(2, 0), None);
        let huge = DenseComplex::diagonal(&[Complex::real(f64::MAX); 2]);
        assert!(huge.add(&huge).is_err());
        let nan = DenseComplex::diagonal(&[Complex::real(f64::NAN); 2]);
        assert!(nan.inverse().is_err());
        let sum = two.add(&two).unwrap().sub(&two).unwrap();
        assert_eq!(sum, two);
        assert_eq!(sum.dim(), 2);
        assert_eq!(sum.data().len(), 4);
    }
}
