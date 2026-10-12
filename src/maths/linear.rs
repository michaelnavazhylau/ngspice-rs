//! Owned faer factors. Matrices remain plain, cloneable stamping storage.
//!
//! faer-control experiment (`proto/faer-control`): faer stays the LU backend,
//! but the wrapper overhead of `work/m9` is removed so the cost of faer itself
//! can be measured against the SPARSE 1.3 prototype:
//!
//! - no n-solve rank/conditioning probe and no backward-residual check. Like
//!   ngspice (and the prototype), any factorization faer completes is
//!   accepted; only structural singularity (`LuError::SymbolicSingular`) and
//!   non-finite solutions are errors. A numerically zero pivot shows up as a
//!   non-finite solution, since faer's partial pivoting takes its reciprocal.
//! - faer's symbolic analysis (COLAMD column order, elimination tree and
//!   supernodal structure) is reused for every factorization of an identical
//!   pattern; only `SymbolicLu::factorize_numeric_lu` runs again. faer's
//!   partial pivoting chooses its **row** pivots during that numeric phase, so
//!   row pivots are not (and cannot be) reused; the numeric storage and the
//!   scratch buffer are reused instead of reallocated.
//! - the CSC structure and the source-entry -> CSC-slot map are built once per
//!   pattern; values are scattered straight into faer's CSC value array, from
//!   folded triplets or from a circuit-bound [`Pattern`] (no faer triplet sort).
//! - faer runs sequentially (`Par::Seq`) rather than on its default global
//!   rayon pool.

use std::sync::{Arc, Weak};

use crate::maths::{Matrix, Pattern, SparseMatrix, Vector};
use crate::primitives::{SpiceError, SpiceResult};
use faer::Par;
use faer::dyn_stack::{MemBuffer, MemStack};
use faer::prelude::*;
use faer::sparse::linalg::LuError;
use faer::sparse::linalg::lu::{NumericLu, SymbolicLu, factorize_symbolic_lu};
use faer::sparse::{SparseColMatRef, SymbolicSparseColMat};
use faer::traits::ComplexField;

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

// Row-scaled normwise backward error, kept only for the dense LU (not on
// any simulation hot path; unchanged from `work/m9`). A componentwise |A| |x| scale is
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

/// What a [`SparseSymbolic`] was built from: a folded row-major triplet
/// pattern, or a circuit [`Pattern`] (identified by its `Arc`).
#[derive(Debug, Clone)]
pub(crate) enum Key {
    Triplets(Arc<Vec<(usize, usize)>>),
    /// Weak, because the pattern caches the factors (no reference cycle).
    Pattern(Weak<Pattern>),
}

/// Reusable faer symbolic LU for one exact assembled pattern, with the CSC
/// structure it was computed on and the slot of every source entry.
#[derive(Debug, Clone)]
pub struct SparseSymbolic {
    n: usize,
    key: Key,
    csc: Arc<SymbolicSparseColMat<usize>>,
    /// CSC value index of each source entry (folded triplet index, or
    /// [`crate::maths::ElementId`] index for a bound pattern).
    slot: Arc<Vec<usize>>,
    lu: Arc<SymbolicLu<usize>>,
}

impl SparseSymbolic {
    /// Builds the CSC structure and faer's symbolic LU for unique positions
    /// given in source order.
    pub(crate) fn analyse(n: usize, key: Key, positions: &[(usize, usize)]) -> SpiceResult<Self> {
        let mut order: Vec<usize> = (0..positions.len()).collect();
        order.sort_unstable_by_key(|&k| (positions[k].1, positions[k].0));
        if order.windows(2).any(|w| positions[w[0]] == positions[w[1]]) {
            return Err(numerical("faer CSC construction", "duplicate position"));
        }
        let mut col_ptr = vec![0_usize; n + 1];
        let mut row_idx = Vec::with_capacity(positions.len());
        let mut slot = vec![0_usize; positions.len()];
        for (csc, &k) in order.iter().enumerate() {
            let (row, col) = positions[k];
            if row >= n || col >= n {
                return Err(numerical("faer CSC construction", "position out of range"));
            }
            col_ptr[col + 1] += 1;
            row_idx.push(row);
            slot[k] = csc;
        }
        for col in 0..n {
            col_ptr[col + 1] += col_ptr[col];
        }
        let csc = SymbolicSparseColMat::new_checked(n, n, col_ptr, None, row_idx);
        let lu = factorize_symbolic_lu(csc.as_ref(), Default::default())
            .map_err(|e| numerical("faer symbolic LU", format!("{e:?}")))?;
        Ok(Self {
            n,
            key,
            csc: Arc::new(csc),
            slot: Arc::new(slot),
            lu: Arc::new(lu),
        })
    }

    /// Whether a matrix has exactly this assembled pattern.
    pub fn matches(&self, matrix: &SparseMatrix) -> bool {
        if matrix.rows() != self.n || matrix.cols() != self.n {
            return false;
        }
        match (&self.key, matrix.bound_values()) {
            (Key::Pattern(pattern), Some((bound, _))) => {
                std::ptr::eq(pattern.as_ptr(), Arc::as_ptr(bound))
            }
            (Key::Pattern(_), None) | (Key::Triplets(_), Some(_)) => false,
            (Key::Triplets(_), None) => {
                let mut folded = std::borrow::Cow::Borrowed(matrix);
                if !matrix.is_folded() {
                    folded.to_mut().fold_duplicates();
                }
                self.matches_positions(folded.triplets().iter().map(|t| (t.row, t.col)))
            }
        }
    }

    /// Whether row-major unique positions equal this triplet pattern.
    pub(crate) fn matches_positions(
        &self,
        mut positions: impl ExactSizeIterator<Item = (usize, usize)>,
    ) -> bool {
        match &self.key {
            Key::Triplets(pattern) => {
                positions.len() == pattern.len()
                    && pattern.iter().all(|p| positions.next() == Some(*p))
            }
            Key::Pattern(_) => false,
        }
    }
}

/// Numeric faer LU for one symbolic analysis, with reusable storage.
pub(crate) struct Factor<T: ComplexField> {
    pub(crate) symbolic: SparseSymbolic,
    numeric: NumericLu<usize, T>,
    values: Vec<T>,
    scratch: MemBuffer,
}

impl<T: ComplexField> std::fmt::Debug for Factor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Factor")
            .field("symbolic", &self.symbolic)
            .finish_non_exhaustive()
    }
}

fn lu_error(context: &str, error: LuError) -> SpiceError {
    match error {
        LuError::SymbolicSingular { index } => numerical(
            context,
            format!("singular matrix: no pivot at elimination step {index}"),
        ),
        LuError::Generic(e) => numerical(context, format!("{e:?}")),
    }
}

impl<T: ComplexField + Copy> Factor<T> {
    pub(crate) fn new(symbolic: SparseSymbolic, zero: T) -> SpiceResult<Self> {
        let req = symbolic
            .lu
            .factorize_numeric_lu_scratch::<T>(Par::Seq, Default::default())
            .or(symbolic.lu.solve_in_place_scratch::<T>(1, Par::Seq));
        let scratch = MemBuffer::try_new(req)
            .map_err(|_| numerical("faer sparse LU", "out of memory for scratch"))?;
        let nnz = symbolic.slot.len();
        Ok(Self {
            symbolic,
            numeric: NumericLu::new(),
            values: vec![zero; nnz],
            scratch,
        })
    }

    /// Numeric factorization of `values` (source order, one per slot).
    pub(crate) fn factor(
        &mut self,
        context: &str,
        values: impl Iterator<Item = T>,
    ) -> SpiceResult<()> {
        let slot = &self.symbolic.slot;
        let mut count = 0;
        for (k, value) in values.enumerate() {
            let Some(&index) = slot.get(k) else {
                return Err(numerical(context, "value count exceeds the pattern"));
            };
            self.values[index] = value;
            count += 1;
        }
        if count != slot.len() {
            return Err(numerical(context, "value count does not match the pattern"));
        }
        let matrix = SparseColMatRef::new(self.symbolic.csc.as_ref().as_ref(), &self.values);
        self.symbolic
            .lu
            .factorize_numeric_lu(
                &mut self.numeric,
                matrix,
                Par::Seq,
                MemStack::new(&mut self.scratch),
                Default::default(),
            )
            .map(|_| ())
            .map_err(|e| lu_error(context, e))
    }

    /// Solves in place (`A x = b`, or `A^T x = b` unconjugated).
    pub(crate) fn solve_in_place(&self, x: &mut [T], transposed: bool) {
        let n = x.len();
        let lu = faer::sparse::linalg::lu::LuRef::new_unchecked(&self.symbolic.lu, &self.numeric);
        let mut buffer = MemBuffer::new(self.symbolic.lu.solve_in_place_scratch::<T>(1, Par::Seq));
        let stack = MemStack::new(&mut buffer);
        let rhs = MatMut::from_column_major_slice_mut(x, n, 1);
        if transposed {
            lu.solve_transpose_in_place_with_conj(faer::Conj::No, rhs, Par::Seq, stack);
        } else {
            lu.solve_in_place_with_conj(faer::Conj::No, rhs, Par::Seq, stack);
        }
    }
}

/// Solves `matrix x = rhs`, refactoring numerically with the symbolic
/// analysis (and storage) of `ordering` when the pattern is unchanged and
/// analysing afresh otherwise, then keeps the factors for the next call. A
/// matrix bound to a circuit [`Pattern`] uses the factors cached on the
/// pattern instead.
///
/// # Errors
/// As [`SparseLu::new`] and [`SparseLu::solve`].
pub fn solve_with_ordering(
    matrix: &SparseMatrix,
    rhs: &Vector,
    ordering: &mut Option<SparseLu>,
) -> SpiceResult<Vector> {
    if let Some((pattern, _)) = matrix.bound_values() {
        let mut cached = pattern.take_ordering();
        let x = solve_reusing(matrix, rhs, &mut cached);
        pattern.store_ordering(cached);
        return x;
    }
    solve_reusing(matrix, rhs, ordering)
}

fn solve_reusing(
    matrix: &SparseMatrix,
    rhs: &Vector,
    ordering: &mut Option<SparseLu>,
) -> SpiceResult<Vector> {
    let factor = match ordering.take() {
        Some(mut previous) if previous.factor.symbolic.matches(matrix) => {
            previous.refactor(matrix)?;
            previous
        }
        _ => SparseLu::fresh(matrix, SparseLu::symbolic_for(matrix, None)?)?,
    };
    let x = factor.solve(rhs)?;
    *ordering = Some(factor);
    Ok(x)
}

/// Owned numeric sparse LU.
#[derive(Debug)]
pub struct SparseLu {
    factor: Factor<f64>,
}

fn assembled(matrix: &SparseMatrix) -> SpiceResult<std::borrow::Cow<'_, SparseMatrix>> {
    square(matrix.rows(), matrix.cols())?;
    if matrix.triplets().iter().any(|t| !t.value.is_finite()) {
        return Err(numerical("sparse LU assembly", "non-finite coefficient"));
    }
    let mut matrix = std::borrow::Cow::Borrowed(matrix);
    if !matrix.is_folded() {
        matrix.to_mut().fold_duplicates();
        if matrix.triplets().iter().any(|t| !t.value.is_finite()) {
            return Err(numerical(
                "sparse LU assembly",
                "duplicate summation overflow",
            ));
        }
    }
    Ok(matrix)
}

fn finite_bound(values: &[f64]) -> SpiceResult<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(numerical("sparse LU assembly", "non-finite coefficient"));
    }
    Ok(())
}

impl SparseLu {
    /// The symbolic analysis for `matrix`: `reuse` when its pattern matches,
    /// otherwise a fresh one.
    fn symbolic_for(
        matrix: &SparseMatrix,
        reuse: Option<&SparseSymbolic>,
    ) -> SpiceResult<SparseSymbolic> {
        Ok(match reuse {
            Some(previous) if previous.matches(matrix) => previous.clone(),
            Some(_) => {
                return Err(numerical(
                    "sparse symbolic LU",
                    "assembled CSC pattern changed",
                ));
            }
            None => {
                if let Some((pattern, values)) = matrix.bound_values() {
                    square(matrix.rows(), matrix.cols())?;
                    finite_bound(values)?;
                    let positions: Vec<_> = (0..pattern.len())
                        .map(|index| pattern.position(crate::maths::sparse::element_id(index)))
                        .collect();
                    SparseSymbolic::analyse(
                        pattern.order(),
                        Key::Pattern(Arc::downgrade(pattern)),
                        &positions,
                    )?
                } else {
                    let folded = assembled(matrix)?;
                    let positions: Vec<_> =
                        folded.triplets().iter().map(|t| (t.row, t.col)).collect();
                    SparseSymbolic::analyse(
                        folded.rows(),
                        Key::Triplets(Arc::new(positions.clone())),
                        &positions,
                    )?
                }
            }
        })
    }

    /// Factors a snapshot, optionally reusing symbolic work for an identical pattern.
    ///
    /// # Errors
    /// Invalid dimensions, coefficients, changed pattern, structural
    /// singularity or an exactly zero pivot.
    pub fn new(matrix: &SparseMatrix, reuse: Option<&SparseSymbolic>) -> SpiceResult<Self> {
        let symbolic = Self::symbolic_for(matrix, reuse)?;
        let result = Self::fresh(matrix, symbolic)?;
        // faer reports only structural singularity from the factorization; an
        // exactly zero pivot becomes an infinite reciprocal. One solve with a
        // fixed RHS turns that into an error here, as SPARSE reports a zero
        // pivot from `spFactor` (a single solve, not the old n-solve probe).
        // The internal factor-and-solve path skips it: its own solve's finite
        // check catches the same condition.
        result.solve(&Vector::from_slice(&vec![1.0; result.factor.symbolic.n]))?;
        Ok(result)
    }

    fn fresh(matrix: &SparseMatrix, symbolic: SparseSymbolic) -> SpiceResult<Self> {
        let mut result = Self {
            factor: Factor::new(symbolic, 0.0)?,
        };
        result.refactor(matrix)?;
        Ok(result)
    }

    /// [`SparseLu::new`] plus the `work/m9` n-solve conditioning check
    /// (inverse row-sum estimate). Kept only for one-time structural
    /// decisions that rely on it, namely the diffsol DAE higher-index guard
    /// (`maths::diffsol`), whose constraint block is assembled from SVD
    /// null-space bases and is singular only up to rounding. Not used on any
    /// per-iteration path.
    ///
    /// # Errors
    /// As [`SparseLu::new`], or conditioning beyond the resolvable range.
    pub fn new_conditioned(matrix: &SparseMatrix) -> SpiceResult<Self> {
        let result = Self::new(matrix, None)?;
        let n = result.factor.symbolic.n;
        let mut inverse_rows = vec![0.0; n];
        for i in 0..n {
            let mut rhs = Vector::zeros(n);
            rhs.set(i, 1.0)?;
            let column = result.solve(&rhs)?;
            for (sum, value) in inverse_rows.iter_mut().zip(column.as_slice()) {
                *sum += value.abs();
            }
        }
        let mut rows = vec![0.0; n];
        for t in assembled(matrix)?.triplets() {
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

    /// Numeric refactorization of a matrix with this pattern, in place.
    fn refactor(&mut self, matrix: &SparseMatrix) -> SpiceResult<()> {
        if let Some((_, values)) = matrix.bound_values() {
            finite_bound(values)?;
            return self
                .factor
                .factor("faer sparse numeric LU", values.iter().copied());
        }
        let folded = assembled(matrix)?;
        self.factor.factor(
            "faer sparse numeric LU",
            folded.triplets().iter().map(|t| t.value),
        )
    }

    /// Symbolic factors for subsequent identical-pattern assemblies.
    pub fn symbolic(&self) -> &SparseSymbolic {
        &self.factor.symbolic
    }

    /// Solves with this snapshot; later mutations to storage cannot stale it.
    ///
    /// # Errors
    /// Invalid RHS or non-finite solution.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        rhs_valid(rhs, self.factor.symbolic.n)?;
        let mut x = rhs.as_slice().to_vec();
        self.factor.solve_in_place(&mut x, false);
        let x = Vector::from_slice(&x);
        if !x.is_finite() {
            return Err(numerical(
                "LU solve",
                "non-finite solution (singular system)",
            ));
        }
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
