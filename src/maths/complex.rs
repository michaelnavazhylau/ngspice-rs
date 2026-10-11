//! Complex sparse LU for AC through the SPARSE 1.3 port. Scalar conversion
//! happens only at this boundary.
//!
//! C: `SMPcLUfac`/`SMPcReorder`/`SMPcSolve`/`SMPcaSolve` (`spsmp.c`). As in
//! [`crate::maths::linear`], there is no condition estimate or residual check;
//! singular pivots and non-finite solutions are errors.
use std::sync::Arc;

use crate::maths::{
    SparseMatrix,
    linear::{PivotTolerances, check_status, numerical, order_fresh, square},
    sparse13::{Sparse, Status},
};
use crate::primitives::{Complex, SpiceResult};

/// Complex assembled sparse matrix, independent of real stamping storage.
#[derive(Debug, Clone)]
pub struct ComplexMatrix {
    n: usize,
    entries: Vec<(usize, usize, Complex)>,
    pivot: PivotTolerances,
}
impl ComplexMatrix {
    /// Assembles `A + j omega E`, preserving cancellations and checking
    /// overflow. The pivot tolerances are `a`'s.
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
            pivot: a.pivot_tolerances(),
        })
    }

    /// Orders and factors (`SMPpreOrder` + `SMPcReorder`).
    /// # Errors
    /// Structurally or numerically singular matrix.
    pub fn factorize(&self) -> SpiceResult<ComplexLu> {
        let mut sparse = Sparse::new(self.n);
        let handles: Vec<_> = self
            .entries
            .iter()
            .map(|&(r, c, v)| {
                let handle = sparse.get_element(r + 1, c + 1);
                sparse.set(handle, v);
                handle
            })
            .collect();
        order_fresh("complex sparse LU", &mut sparse, self.pivot)?;
        Ok(ComplexLu {
            n: self.n,
            pattern: Arc::new(self.entries.iter().map(|&(r, c, _)| (r, c)).collect()),
            handles: Arc::new(handles),
            sparse,
        })
    }

    /// Refactors with the pivot order of `previous` when the pattern is
    /// identical (`SMPcLUfac`, `spFactor`), reordering after a zero pivot
    /// (`SMPcReorder`) as `NIacIter` does; otherwise orders afresh.
    /// # Errors
    /// Structurally or numerically singular matrix.
    pub fn factorize_reusing(&self, previous: &ComplexLu) -> SpiceResult<ComplexLu> {
        let same = previous.n == self.n
            && previous.pattern.len() == self.entries.len()
            && previous
                .pattern
                .iter()
                .zip(&self.entries)
                .all(|(p, &(r, c, _))| *p == (r, c));
        if !same {
            return self.factorize();
        }
        let mut sparse = previous.sparse.clone();
        let load = |sparse: &mut Sparse<Complex>| {
            sparse.clear();
            for (handle, &(_, _, v)) in previous.handles.iter().zip(&self.entries) {
                sparse.set(*handle, v);
            }
        };
        load(&mut sparse);
        let mut status = sparse.factor();
        if status == Status::ZeroDiag {
            load(&mut sparse);
            status = sparse.order_and_factor(self.pivot.relative, self.pivot.absolute);
        }
        check_status("complex sparse LU", status, &sparse)?;
        Ok(ComplexLu {
            n: self.n,
            pattern: Arc::clone(&previous.pattern),
            handles: Arc::clone(&previous.handles),
            sparse,
        })
    }
}

/// Owned complex factors; subsequent real operator mutations cannot stale them.
#[derive(Debug)]
pub struct ComplexLu {
    n: usize,
    pattern: Arc<Vec<(usize, usize)>>,
    handles: Arc<Vec<usize>>,
    sparse: Sparse<Complex>,
}
impl ComplexLu {
    /// Solves `A x = b` (`SMPcSolve`).
    /// # Errors
    /// Wrong RHS size, non-finite RHS or solution.
    pub fn solve(&self, rhs: &[Complex]) -> SpiceResult<Vec<Complex>> {
        self.solve_with(rhs, false)
    }

    /// Solves the transposed (not conjugated) system `A^T x = b` with the same
    /// factors. This is the adjoint solve of `.noise` (C `SMPcaSolve` in
    /// `NInzIter`).
    /// # Errors
    /// Wrong RHS size, non-finite RHS or solution.
    pub fn solve_transposed(&self, rhs: &[Complex]) -> SpiceResult<Vec<Complex>> {
        self.solve_with(rhs, true)
    }

    fn solve_with(&self, rhs: &[Complex], transposed: bool) -> SpiceResult<Vec<Complex>> {
        if rhs.len() != self.n || rhs.iter().any(|v| !v.is_finite()) {
            return Err(numerical("complex solve", "invalid RHS"));
        }
        let x = if transposed {
            self.sparse.solve_transposed(rhs)
        } else {
            self.sparse.solve(rhs)
        };
        if x.iter().any(|v| !v.is_finite()) {
            return Err(numerical(
                "complex solve",
                "non-finite solution/singular system",
            ));
        }
        Ok(x)
    }

    /// `spDeterminant`: `(mantissa, decimal exponent)`, with C's
    /// `1 <= max(|re|, |im|) < 10` normalization.
    pub fn determinant(&self) -> (Complex, i32) {
        self.sparse.determinant()
    }
}
