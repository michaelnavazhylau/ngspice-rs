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

/// `omega` times each of `e`'s contributions in stamp order, except that
/// the contributions indexed by one of `groups` (ranges into `e`'s unfolded
/// triplets, all at one position) are scaled one by one and summed into a
/// single value first: a device such as `mos1acld.c` adds
/// `xgd + xgs + xgb` (each `cap * CKTomega`) to an element in one step.
fn dynamic_terms(
    e: &SparseMatrix,
    groups: &[std::ops::Range<usize>],
    omega: f64,
) -> Vec<(usize, usize, f64)> {
    let triplets = e.triplets();
    let mut out = Vec::with_capacity(triplets.len());
    let mut groups = groups.iter().peekable();
    let mut i = 0;
    while i < triplets.len() {
        while groups.next_if(|g| g.end <= i || g.is_empty()).is_some() {}
        let t = &triplets[i];
        match groups.peek() {
            Some(g) if g.start == i && g.end <= triplets.len() => {
                let mut sum = 0.;
                for u in &triplets[g.start..g.end] {
                    sum += omega * u.value;
                }
                out.push((t.row, t.col, sum));
                i = g.end;
            }
            _ => {
                out.push((t.row, t.col, omega * t.value));
                i += 1;
            }
        }
    }
    out
}

/// Complex assembled sparse matrix, independent of real stamping storage.
#[derive(Debug, Clone)]
pub struct ComplexMatrix {
    n: usize,
    entries: Vec<(usize, usize, Complex)>,
    pivot: PivotTolerances,
    /// The circuit pattern whose elements `entries` are (one per element,
    /// in element order), or `None` for a standalone matrix.
    bound: Option<Arc<crate::maths::Pattern>>,
}
impl ComplexMatrix {
    /// Assembles `A + j omega E`, preserving cancellations and checking
    /// overflow. The pivot tolerances are `a`'s.
    /// # Errors
    /// Invalid dimensions, frequency or coefficients.
    pub fn from_operators(a: &SparseMatrix, e: &SparseMatrix, omega: f64) -> SpiceResult<Self> {
        Self::from_operators_grouped(a, e, &[], omega)
    }

    /// [`Self::from_operators`] with `groups` of `e`'s stamp-order
    /// contributions (see [`dynamic_terms`]) each added as one sum.
    /// # Errors
    /// As [`Self::from_operators`].
    pub fn from_operators_grouped(
        a: &SparseMatrix,
        e: &SparseMatrix,
        groups: &[std::ops::Range<usize>],
        omega: f64,
    ) -> SpiceResult<Self> {
        square(a.rows(), a.cols())?;
        if e.rows() != a.rows() || e.cols() != a.cols() || !omega.is_finite() || omega < 0. {
            return Err(numerical(
                "complex assembly",
                "operator dimensions/frequency invalid",
            ));
        }
        let mut assembled = std::collections::BTreeMap::new();
        let real = a
            .triplets()
            .iter()
            .map(|t| (t.row, t.col, Complex::real(t.value)));
        let imaginary = dynamic_terms(e, groups, omega)
            .into_iter()
            .map(|(row, col, v)| (row, col, Complex::imaginary(v)));
        {
            for (row, col, v) in real.chain(imaginary) {
                if !v.is_finite() {
                    return Err(numerical(
                        "complex assembly",
                        "non-finite coefficient/overflow",
                    ));
                }
                let entry = assembled.entry((row, col)).or_insert(Complex::ZERO);
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
            bound: None,
        })
    }

    /// `CKTacLoad` into a circuit pattern: every element of `pattern` (zeros
    /// included, as ngspice allocates them at setup) holds the sum of `a`'s
    /// contributions as its real part and of `omega` times `e`'s as its
    /// imaginary part, each summed in stamp order (`omega` may be negative, as
    /// `CKTomega` is for `.disto`'s `f1 - f2`). `None` when a stamp lies
    /// outside the pattern.
    ///
    /// Factoring it ([`Self::factorize`]) shares the circuit's SPARSE frame
    /// and pivot order with its real solves, as `NIacIter` does.
    /// # Errors
    /// Invalid dimensions, frequency or coefficients.
    pub fn from_operators_on(
        pattern: &Arc<crate::maths::Pattern>,
        a: &SparseMatrix,
        e: &SparseMatrix,
        omega: f64,
    ) -> SpiceResult<Option<Self>> {
        Self::from_operators_grouped_on(pattern, a, e, &[], omega)
    }

    /// [`Self::from_operators_on`] with `groups` of `e`'s stamp-order
    /// contributions (see [`dynamic_terms`]) each added as one sum.
    /// # Errors
    /// As [`Self::from_operators_on`].
    pub fn from_operators_grouped_on(
        pattern: &Arc<crate::maths::Pattern>,
        a: &SparseMatrix,
        e: &SparseMatrix,
        groups: &[std::ops::Range<usize>],
        omega: f64,
    ) -> SpiceResult<Option<Self>> {
        let n = pattern.order();
        if a.rows() != n || a.cols() != n || e.rows() != n || e.cols() != n {
            return Err(numerical("complex assembly", "operator dimensions invalid"));
        }
        // `CKTomega` may be negative: `.disto` loads at `f1 - f2`.
        if !omega.is_finite() {
            return Err(numerical("complex assembly", "frequency invalid"));
        }
        let mut values = vec![Complex::ZERO; pattern.len()];
        let real = a.triplets().iter().map(|t| (t.row, t.col, t.value, false));
        let imaginary = dynamic_terms(e, groups, omega)
            .into_iter()
            .map(|(row, col, v)| (row, col, v, true));
        {
            for (row, col, v, dynamic) in real.chain(imaginary) {
                let Some(id) = pattern.find(row, col) else {
                    return Ok(None);
                };
                let value = &mut values[id.index()];
                if dynamic {
                    value.im += v;
                } else {
                    value.re += v;
                }
                if !value.is_finite() {
                    return Err(numerical(
                        "complex assembly",
                        "non-finite coefficient/overflow",
                    ));
                }
            }
        }
        Ok(Some(Self {
            n,
            entries: values
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    let (row, col) = pattern.position(crate::maths::sparse::element_id(index));
                    (row, col, value)
                })
                .collect(),
            pivot: a.pivot_tolerances(),
            bound: Some(Arc::clone(pattern)),
        }))
    }

    /// Orders and factors (`SMPpreOrder` + `SMPcReorder`).
    /// # Errors
    /// Structurally or numerically singular matrix.
    pub fn factorize(&self) -> SpiceResult<ComplexLu> {
        if let Some(pattern) = &self.bound {
            let values: Vec<Complex> = self.entries.iter().map(|&(_, _, v)| v).collect();
            let (sparse, handles) =
                crate::maths::linear::factor_shared_complex(pattern, &values, self.pivot)?;
            return Ok(ComplexLu {
                n: self.n,
                pattern: Arc::new(self.entries.iter().map(|&(r, c, _)| (r, c)).collect()),
                handles,
                sparse,
            });
        }
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
        if self.bound.is_some() {
            // The circuit pattern carries the pivot order itself.
            return self.factorize();
        }
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
