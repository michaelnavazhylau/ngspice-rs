//! Owned faer factors. Matrices remain plain, cloneable stamping storage.
//!
//! Sparse faer's high-level LU does not expose numeric pivots. We certify its
//! rank by solving every basis RHS and checking `A * inverse(A) ≈ I`, rejecting
//! non-finite results and numerically unresolved rank. This costs n sparse
//! solves at factorization, but only O(n) extra storage; no dense fallback or
//! custom LU is used. A future backend pivot API can replace this diagnostic.

use crate::{Matrix, SparseMatrix, Vector};
use faer::prelude::*;
use faer::sparse::linalg::solvers::{Lu, SymbolicLu};
use faer::sparse::{SparseColMat, Triplet};
use spice_core::{SpiceError, SpiceResult};

pub(crate) fn numerical(context: &str, message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: context.into(),
        message: message.into(),
    }
}

pub(crate) fn square(rows: usize, cols: usize) -> SpiceResult<()> {
    if rows != cols || rows == 0 {
        return Err(numerical(
            "LU",
            format!("expected a nonempty square system, got {rows}x{cols}"),
        ));
    }
    Ok(())
}

fn rhs_valid(rhs: &Vector, n: usize) -> SpiceResult<()> {
    if rhs.len() != n || !rhs.is_finite() {
        return Err(numerical(
            "LU solve",
            "RHS must have the system dimension and finite values",
        ));
    }
    Ok(())
}

// Row-scaled normwise backward error. A componentwise |A| |x| scale is
// too strict for near-zero unknowns in exact constraint rows after pivoting.
// Overflow in either residual or scale is an error.
fn residual(
    entries: impl Iterator<Item = (usize, usize, f64)>,
    rhs: &Vector,
    x: &Vector,
) -> SpiceResult<()> {
    if !x.is_finite() {
        return Err(numerical(
            "LU solve",
            "non-finite solution (singular system)",
        ));
    }
    let mut ax = vec![0.0; rhs.len()];
    let mut scale: Vec<_> = rhs.as_slice().iter().map(|b| b.abs()).collect();
    let max_x = x.max_abs();
    for (r, c, a) in entries {
        ax[r] += a * x.as_slice()[c];
        scale[r] += a.abs() * max_x;
    }
    let tol = 128.0 * f64::EPSILON * (rhs.len().max(1) as f64);
    for r in 0..rhs.len() {
        let error = (ax[r] - rhs.as_slice()[r]).abs();
        if !error.is_finite() || !scale[r].is_finite() || error > tol * scale[r] {
            return Err(numerical(
                "LU solve",
                format!("backward residual failed at row {r}"),
            ));
        }
    }
    Ok(())
}

/// Reusable symbolic analysis, tied to an exact assembled CSC pattern.
#[derive(Debug, Clone)]
pub struct SparseSymbolic {
    n: usize,
    pattern: Vec<(usize, usize)>,
    factor: SymbolicLu<usize>,
}

/// Owned numeric sparse LU and the original assembled matrix for diagnostics.
#[derive(Debug)]
pub struct SparseLu {
    matrix: SparseMatrix,
    factor: Lu<usize, f64>,
    symbolic: SparseSymbolic,
}

fn assembled(matrix: &SparseMatrix) -> SpiceResult<SparseMatrix> {
    square(matrix.rows(), matrix.cols())?;
    if matrix.triplets().iter().any(|t| !t.value.is_finite()) {
        return Err(numerical("sparse LU assembly", "non-finite coefficient"));
    }
    let mut matrix = matrix.clone();
    matrix.fold_duplicates();
    if matrix.triplets().iter().any(|t| !t.value.is_finite()) {
        return Err(numerical(
            "sparse LU assembly",
            "duplicate summation overflow",
        ));
    }
    Ok(matrix)
}

impl SparseLu {
    /// Factors a snapshot, optionally reusing symbolic work for an identical pattern.
    ///
    /// # Errors
    /// Invalid dimensions, coefficients, changed pattern, or unresolved numeric rank.
    pub fn new(matrix: &SparseMatrix, reuse: Option<&SparseSymbolic>) -> SpiceResult<Self> {
        let matrix = assembled(matrix)?;
        let n = matrix.rows();
        let pattern: Vec<_> = matrix.triplets().iter().map(|t| (t.row, t.col)).collect();
        let triplets: Vec<_> = matrix
            .triplets()
            .iter()
            .map(|t| Triplet::new(t.row, t.col, t.value))
            .collect();
        let csc = SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &triplets)
            .map_err(|e| numerical("faer CSC construction", format!("{e:?}")))?;
        let symbolic = match reuse {
            Some(s) if s.n == n && s.pattern == pattern => s.clone(),
            Some(_) => {
                return Err(numerical(
                    "sparse symbolic LU",
                    "assembled CSC pattern changed",
                ));
            }
            None => SparseSymbolic {
                n,
                pattern,
                factor: SymbolicLu::try_new(csc.symbolic())
                    .map_err(|e| numerical("faer symbolic LU", format!("{e:?}")))?,
            },
        };
        let factor = Lu::try_new_with_symbolic(symbolic.factor.clone(), csc.as_ref())
            .map_err(|e| numerical("faer sparse numeric LU", format!("{e:?}")))?;
        let result = Self {
            matrix,
            factor,
            symbolic,
        };
        // All basis vectors must be in the range, even when the user's RHS is zero.
        // An exact singular LU usually returns NaNs here. A finite pseudoinverse
        // cannot pass all basis residuals at resolvable conditioning.
        // Reject rank that floating-point residual arithmetic cannot certify.
        let mut inverse_rows = vec![0.0; n];
        for i in 0..n {
            let mut rhs = Vector::zeros(n);
            rhs.set(i, 1.0)?;
            let inverse_column = result
                .solve(&rhs)
                .map_err(|e| numerical("sparse LU rank diagnostic", e.to_string()))?;
            for (sum, value) in inverse_rows.iter_mut().zip(inverse_column.as_slice()) {
                *sum += value.abs();
            }
        }
        let mut rows = vec![0.0; n];
        for t in result.matrix.triplets() {
            rows[t.row] += t.value.abs();
        }
        let condition =
            rows.into_iter().fold(0.0, f64::max) * inverse_rows.into_iter().fold(0.0, f64::max);
        if !condition.is_finite() || 128.0 * f64::EPSILON * (n as f64) * condition >= 0.5 {
            return Err(numerical(
                "sparse LU rank diagnostic",
                "numeric rank is unresolved at this conditioning; rescale the system",
            ));
        }
        Ok(result)
    }

    /// Symbolic factors for subsequent identical-pattern assemblies.
    pub fn symbolic(&self) -> &SparseSymbolic {
        &self.symbolic
    }

    /// Solves with this snapshot; later mutations to storage cannot stale it.
    ///
    /// # Errors
    /// Invalid RHS, non-finite solution, or failed scaled backward residual.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        rhs_valid(rhs, self.matrix.rows())?;
        let mut x = Mat::from_fn(rhs.len(), 1, |r, _| rhs.as_slice()[r]);
        self.factor.solve_in_place(x.as_mut());
        let x = Vector::from_slice(&(0..rhs.len()).map(|r| x[(r, 0)]).collect::<Vec<_>>());
        residual(
            self.matrix
                .triplets()
                .iter()
                .map(|t| (t.row, t.col, t.value)),
            rhs,
            &x,
        )?;
        Ok(x)
    }
}

/// Owned pivoted dense LU, preserving the public row-major matrix.
#[derive(Debug)]
pub struct DenseLu {
    matrix: Matrix,
    factor: faer::linalg::solvers::PartialPivLu<f64>,
}

impl DenseLu {
    /// Factors a nonempty square finite matrix, with explicit pivot diagnostics.
    ///
    /// # Errors
    /// Invalid dimensions/coefficients or singular/non-finite pivots.
    pub fn new(matrix: &Matrix) -> SpiceResult<Self> {
        square(matrix.rows(), matrix.cols())?;
        if matrix.data().iter().any(|a| !a.is_finite()) {
            return Err(numerical("dense LU", "non-finite coefficient"));
        }
        let factor = Mat::from_fn(matrix.rows(), matrix.cols(), |r, c| {
            matrix.get(r, c).unwrap()
        })
        .partial_piv_lu();
        let scale = matrix.data().iter().fold(0_f64, |m, a| m.max(a.abs()));
        let pivot_tolerance = f64::EPSILON * (matrix.rows() as f64) * scale;
        for i in 0..matrix.rows() {
            let pivot = factor.U()[(i, i)];
            if pivot.abs() <= pivot_tolerance || !pivot.is_finite() {
                return Err(numerical(
                    "dense LU",
                    format!("singular/non-finite pivot {i}"),
                ));
            }
        }
        Ok(Self {
            matrix: matrix.clone(),
            factor,
        })
    }

    /// Solves against the stored snapshot and checks its backward residual.
    ///
    /// # Errors
    /// Invalid RHS or non-finite/inaccurate solution.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        rhs_valid(rhs, self.matrix.rows())?;
        let mut x = Mat::from_fn(rhs.len(), 1, |r, _| rhs.as_slice()[r]);
        self.factor.solve_in_place(x.as_mut());
        let x = Vector::from_slice(&(0..rhs.len()).map(|r| x[(r, 0)]).collect::<Vec<_>>());
        residual(
            (0..self.matrix.rows()).flat_map(|r| {
                (0..self.matrix.cols()).map(move |c| (r, c, self.matrix.get(r, c).unwrap()))
            }),
            rhs,
            &x,
        )?;
        Ok(x)
    }
}
