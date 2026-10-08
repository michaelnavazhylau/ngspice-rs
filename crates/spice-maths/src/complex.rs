//! Complex sparse LU for AC. Scalar conversion happens only at this boundary.
use crate::{
    SparseMatrix,
    linear::{numerical, square},
};
use faer::prelude::*;
use faer::sparse::{SparseColMat, Triplet};
use spice_core::{Complex, SpiceResult};

/// Complex assembled sparse matrix, independent of real stamping storage.
#[derive(Debug, Clone)]
pub struct ComplexMatrix {
    n: usize,
    entries: Vec<(usize, usize, Complex)>,
}
impl ComplexMatrix {
    /// Assembles `A + j omega E`, preserving cancellations and checking overflow.
    /// # Errors
    /// Invalid dimensions, frequency or coefficients.
    pub fn from_operators(a: &SparseMatrix, e: &SparseMatrix, omega: f64) -> SpiceResult<Self> {
        square(a.rows(), a.cols())?;
        if e.rows() != a.rows() || e.cols() != a.cols() || !omega.is_finite() || omega < 0. {
            return Err(numerical(
                "complex assembly",
                "operator dimensions/frequency invalid",
            ));
        }
        let mut assembled = std::collections::BTreeMap::new();
        for (m, dynamic) in [(a, false), (e, true)] {
            for t in m.triplets() {
                let v = if dynamic {
                    Complex::imaginary(omega * t.value)
                } else {
                    Complex::real(t.value)
                };
                if !v.is_finite() {
                    return Err(numerical(
                        "complex assembly",
                        "non-finite coefficient/overflow",
                    ));
                }
                let entry = assembled.entry((t.row, t.col)).or_insert(Complex::ZERO);
                *entry = *entry + v;
                if !entry.is_finite() {
                    return Err(numerical("complex assembly", "duplicate sum overflow"));
                }
            }
        }
        Ok(Self {
            n: a.rows(),
            entries: assembled
                .into_iter()
                .filter(|(_, v)| *v != Complex::ZERO)
                .map(|((r, c), v)| (r, c, v))
                .collect(),
        })
    }
    /// Owned faer complex LU with complete-basis numerical rank diagnostics.
    /// The inherited guard is not a formal aggregate uniqueness certificate;
    /// see `docs/port/SPARSE_RANK_DIAGNOSTICS.md`.
    /// # Errors
    /// Structural/numeric singularity or failed rank residual.
    pub fn factorize(&self) -> SpiceResult<ComplexLu> {
        let triplets: Vec<_> = self
            .entries
            .iter()
            .map(|(r, c, v)| Triplet::new(*r, *c, c64::new(v.re, v.im)))
            .collect();
        let csc = SparseColMat::<usize, c64>::try_new_from_triplets(self.n, self.n, &triplets)
            .map_err(|e| numerical("complex CSC", format!("{e:?}")))?;
        let factor = csc
            .sp_lu()
            .map_err(|e| numerical("complex sparse LU", format!("{e:?}")))?;
        let result = ComplexLu {
            matrix: self.clone(),
            factor,
        };
        let mut inverse_rows = vec![0.0; self.n];
        for i in 0..self.n {
            let mut rhs = vec![Complex::ZERO; self.n];
            rhs[i] = Complex::real(1.);
            let column = result.solve(&rhs)?;
            for (sum, value) in inverse_rows.iter_mut().zip(column) {
                *sum += value.magnitude();
            }
        }
        let mut rows = vec![0.0; self.n];
        for (r, _, v) in &self.entries {
            rows[*r] += v.magnitude();
        }
        let condition =
            rows.into_iter().fold(0.0, f64::max) * inverse_rows.into_iter().fold(0.0, f64::max);
        if !condition.is_finite() || 128.0 * f64::EPSILON * (self.n as f64) * condition >= 0.5 {
            return Err(numerical(
                "complex LU rank diagnostic",
                "numeric rank unresolved at this conditioning; rescale the system",
            ));
        }
        Ok(result)
    }
}
/// Owned complex factors; subsequent real operator mutations cannot stale them.
#[derive(Debug)]
pub struct ComplexLu {
    matrix: ComplexMatrix,
    factor: faer::sparse::linalg::solvers::Lu<usize, c64>,
}
impl ComplexLu {
    /// Solves and checks a row-scaled normwise complex backward residual.
    /// # Errors
    /// Wrong RHS size, non-finite RHS/solution or inaccurate solve.
    pub fn solve(&self, rhs: &[Complex]) -> SpiceResult<Vec<Complex>> {
        let n = self.matrix.n;
        if rhs.len() != n || rhs.iter().any(|v| !v.is_finite()) {
            return Err(numerical("complex solve", "invalid RHS"));
        }
        let mut x = Mat::from_fn(n, 1, |r, _| c64::new(rhs[r].re, rhs[r].im));
        self.factor.solve_in_place(x.as_mut());
        let x: Vec<_> = (0..n)
            .map(|r| Complex::new(x[(r, 0)].re, x[(r, 0)].im))
            .collect();
        if x.iter().any(|v| !v.is_finite()) {
            return Err(numerical(
                "complex solve",
                "non-finite solution/singular system",
            ));
        }
        let max_x = x.iter().fold(0_f64, |m, v| m.max(v.magnitude()));
        let mut ax = vec![Complex::ZERO; n];
        let mut scale: Vec<_> = rhs.iter().map(|v| v.magnitude()).collect();
        for (r, c, a) in &self.matrix.entries {
            ax[*r] = ax[*r] + *a * x[*c];
            scale[*r] += a.magnitude() * max_x;
        }
        for r in 0..n {
            let error = (ax[r] - rhs[r]).magnitude();
            if !error.is_finite()
                || !scale[r].is_finite()
                || error > 128. * f64::EPSILON * (n as f64) * scale[r]
            {
                return Err(numerical(
                    "complex solve",
                    format!("backward residual failed at row {r}"),
                ));
            }
        }
        Ok(x)
    }
}
