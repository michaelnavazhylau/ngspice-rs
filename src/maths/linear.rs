//! Owned LU factors from the SPARSE 1.3 port (`maths::sparse13`).
//! Matrices remain plain, cloneable stamping storage.
//!
//! This follows ngspice's `spsmp.c` use of SPARSE: a fresh pattern is
//! MNA-preordered (`SMPpreOrder`) and ordered with threshold pivoting
//! (`SMPreorder`, `spOrderAndFactor` with the matrix's [`PivotTolerances`],
//! C's `CKTpivotRelTol`/`CKTpivotAbsTol`); a reused pattern is refactored with the
//! existing pivot order (`SMPluFac`, `spFactor`) and, on a zero pivot,
//! reordered as `NIiter` does after `E_SINGULAR`. Like C there is no
//! condition estimate or residual check: only structural/numeric singularity
//! (`spSINGULAR`/`spZERO_DIAG`) and non-finite solutions are errors, and small
//! pivots are accepted (`spSMALL_PIVOT` is `OK` in ngspice).
//!
//! Elements are created in the folded triplet order (row-major), which sets
//! SPARSE's internal numbering. ngspice instead creates them in device setup
//! order, so pivot choices can differ from C where Markowitz ties occur.

use std::sync::Arc;

use crate::maths::sparse13::{Scalar, Sparse, Status};
use crate::maths::{Matrix, Pattern, SparseMatrix, Vector};
use crate::primitives::{SpiceError, SpiceResult};

/// SPARSE threshold-pivoting limits for ordering a circuit matrix: C's
/// `CKTpivotAbsTol` (`.options pivtol`) and `CKTpivotRelTol` (`.options
/// pivrel`), passed to every `SMPreorder`/`SMPcReorder` (`niiter.c`,
/// `niaciter.c`). A refactor with an existing order (`spFactor`) does not
/// read them.
///
/// `spOrderAndFactor` silently falls back to its stored threshold for a
/// relative value outside `(0, 1]` or a negative absolute value; here such
/// values are rejected by [`Self::validate`] instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PivotTolerances {
    /// Smallest acceptable pivot magnitude (`pivtol`, default `1e-13`).
    pub absolute: f64,
    /// Smallest acceptable ratio of a pivot to the largest magnitude in its
    /// column (`pivrel`, default `1e-3`).
    pub relative: f64,
}

impl PivotTolerances {
    /// ngspice's defaults (`cktntask.c`): `pivtol = 1e-13`, `pivrel = 1e-3`.
    pub const DEFAULT: Self = Self {
        absolute: 1.0e-13,
        relative: 1.0e-3,
    };

    /// Checks `0 <= absolute` and `0 < relative <= 1`, both finite.
    ///
    /// # Errors
    /// A message naming the offending option.
    pub fn validate(&self) -> Result<(), String> {
        if !self.absolute.is_finite() || self.absolute < 0. {
            return Err(format!(
                "pivtol must be finite and >= 0, not {}",
                self.absolute
            ));
        }
        if !self.relative.is_finite() || self.relative <= 0. || self.relative > 1. {
            return Err(format!("pivrel must be in (0, 1], not {}", self.relative));
        }
        Ok(())
    }
}

impl Default for PivotTolerances {
    fn default() -> Self {
        Self::DEFAULT
    }
}

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

/// Maps a SPARSE status to the port's error, naming the singular unknown
/// (0-based matrix row/column) as `NIiter`'s "singular matrix" warning does.
pub(crate) fn check_status<T: Scalar>(
    context: &str,
    status: Status,
    sparse: &Sparse<T>,
) -> SpiceResult<()> {
    match status {
        Status::Okay | Status::SmallPivot => Ok(()),
        Status::ZeroDiag | Status::Singular => {
            let (row, col) = sparse.where_singular();
            let unknowns = if row == col {
                format!("unknown {}", row.saturating_sub(1))
            } else {
                format!(
                    "unknowns {} and {}",
                    row.saturating_sub(1),
                    col.saturating_sub(1)
                )
            };
            Err(numerical(
                context,
                format!("singular matrix: check {unknowns}"),
            ))
        }
    }
}

/// Orders and factors a freshly built matrix (`SMPpreOrder` + `SMPreorder`).
pub(crate) fn order_fresh<T: Scalar>(
    context: &str,
    sparse: &mut Sparse<T>,
    pivot: PivotTolerances,
) -> SpiceResult<()> {
    if !sparse.fully_translated() {
        return Err(numerical(
            context,
            "singular matrix: an unknown has no matrix entries",
        ));
    }
    sparse.mna_preorder();
    let status = sparse.order_and_factor(pivot.relative, pivot.absolute);
    check_status(context, status, sparse)
}

/// What a [`SparseSymbolic`] was built from: a folded triplet pattern, or a
/// circuit [`Pattern`] (identified by its `Arc`).
#[derive(Debug, Clone)]
enum Key {
    Triplets(Arc<Vec<(usize, usize)>>),
    /// Weak, because the pattern caches this symbolic (no reference cycle).
    Pattern(std::sync::Weak<Pattern>),
}

/// Reusable pivot order and fill-in structure for one exact assembled
/// pattern (SPARSE's ordered matrix).
#[derive(Debug, Clone)]
pub struct SparseSymbolic {
    n: usize,
    key: Key,
    /// SPARSE element per triplet (folded order) or per [`crate::maths::ElementId`].
    handles: Arc<Vec<usize>>,
    sparse: Arc<Sparse<f64>>,
}

impl SparseSymbolic {
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
            (Key::Triplets(pattern), None) => {
                let mut folded = std::borrow::Cow::Borrowed(matrix);
                if !matrix.is_folded() {
                    folded.to_mut().fold_duplicates();
                }
                folded.triplets().len() == pattern.len()
                    && folded
                        .triplets()
                        .iter()
                        .zip(pattern.iter())
                        .all(|(t, &(row, col))| t.row == row && t.col == col)
            }
        }
    }
}

/// Solves `matrix x = rhs`, refactoring with `ordering`'s pivots when the
/// pattern is unchanged (`SMPluFac`) and ordering afresh otherwise
/// (`SMPreorder`), then keeps the new factors' order for the next call. The
/// previous factors are refactored in place when nothing else holds them.
///
/// # Errors
/// As [`SparseLu::new`] and [`SparseLu::solve`].
pub fn solve_with_ordering(
    matrix: &SparseMatrix,
    rhs: &Vector,
    ordering: &mut Option<SparseSymbolic>,
) -> SpiceResult<Vector> {
    if let Some((pattern, _)) = matrix.bound_values() {
        // A bound matrix uses its pattern's single persistent order instead.
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
    ordering: &mut Option<SparseSymbolic>,
) -> SpiceResult<Vector> {
    let factor = match ordering.take() {
        Some(previous) if previous.matches(matrix) => SparseLu::refactor(previous, matrix)?,
        _ => SparseLu::new(matrix, None)?,
    };
    let x = factor.solve(rhs)?;
    *ordering = Some(factor.symbolic);
    Ok(x)
}

/// `NIacIter`'s factorization (`niaciter.c`) of a complex system loaded into
/// a circuit pattern's elements (`CKTacLoad`), in the same SPARSE frame as
/// the circuit's real solves: the frame is switched to complex
/// ([`Sparse::retyped`]), `SMPcReorder` runs when the pattern's
/// `NIACSHOULDREORDER` flag is set (once per circuit), `SMPcLUfac` otherwise,
/// and a zero pivot sets the flag and reorders. The frame, with whatever
/// reordering happened, is stored back for the circuit's next solve.
///
/// `values` holds one value per pattern element, in element order.
///
/// # Errors
/// A singular matrix.
pub(crate) fn factor_shared_complex(
    pattern: &Arc<Pattern>,
    values: &[crate::primitives::Complex],
    pivot: PivotTolerances,
) -> SpiceResult<(Sparse<crate::primitives::Complex>, Arc<Vec<usize>>)> {
    use crate::primitives::Complex;
    let n = pattern.order();
    let cached = pattern.take_ordering().filter(|symbolic| {
        matches!(&symbolic.key, Key::Pattern(weak) if std::ptr::eq(weak.as_ptr(), Arc::as_ptr(pattern)))
    });
    let (mut sparse, handles, fresh) = match cached {
        Some(symbolic) => (
            symbolic.sparse.retyped::<Complex>(),
            symbolic.handles,
            false,
        ),
        None => {
            let mut sparse = Sparse::<Complex>::new(n);
            let handles: Vec<_> = (0..pattern.len())
                .map(|index| {
                    let (row, col) = pattern.position(crate::maths::sparse::element_id(index));
                    sparse.get_element(row + 1, col + 1)
                })
                .collect();
            (sparse, Arc::new(handles), true)
        }
    };
    let load = |sparse: &mut Sparse<Complex>| {
        sparse.clear();
        for (handle, value) in handles.iter().zip(values) {
            sparse.set(*handle, *value);
        }
    };
    load(&mut sparse);
    let result = if fresh {
        pattern.set_ac_reorder(false);
        order_fresh("complex sparse LU", &mut sparse, pivot)
    } else {
        let mut status = if pattern.ac_reorder() {
            pattern.set_ac_reorder(false);
            sparse.order_and_factor(pivot.relative, pivot.absolute)
        } else {
            sparse.factor()
        };
        if status == Status::ZeroDiag {
            // NIacIter: E_SINGULAR sets NIACSHOULDREORDER and retries.
            load(&mut sparse);
            status = sparse.order_and_factor(pivot.relative, pivot.absolute);
        }
        check_status("complex sparse LU", status, &sparse)
    };
    pattern.store_ordering(Some(SparseSymbolic {
        n,
        key: Key::Pattern(Arc::downgrade(pattern)),
        handles: Arc::clone(&handles),
        sparse: Arc::new(sparse.retyped::<f64>()),
    }));
    result.map(|()| (sparse, handles))
}

/// The result of one `NIiter` factor-and-solve step ([`solve_ni`]).
#[derive(Debug)]
pub enum NiStep {
    /// The new iterate.
    Solved(Vector),
    /// `SMPluFac` met a zero pivot with the existing order (`E_SINGULAR`):
    /// `NIiter` sets `NISHOULDREORDER` and reloads before trying again.
    ZeroPivot,
}

/// A SPARSE frame for `matrix` with elements created in its order (a bound
/// matrix's pattern order, else folded row-major), values loaded, not yet
/// ordered.
fn fresh_frame(matrix: &SparseMatrix) -> SpiceResult<(Sparse<f64>, SparseSymbolic)> {
    if let Some((pattern, values)) = matrix.bound_values() {
        square(matrix.rows(), matrix.cols())?;
        finite_bound(values)?;
        let mut sparse = Sparse::new(pattern.order());
        let handles: Vec<_> = (0..pattern.len())
            .map(|index| {
                let id = crate::maths::sparse::element_id(index);
                let (row, col) = pattern.position(id);
                let handle = sparse.get_element(row + 1, col + 1);
                sparse.set(handle, values[index]);
                handle
            })
            .collect();
        let symbolic = SparseSymbolic {
            n: pattern.order(),
            key: Key::Pattern(Arc::downgrade(pattern)),
            handles: Arc::new(handles),
            sparse: Arc::new(Sparse::new(0)),
        };
        return Ok((sparse, symbolic));
    }
    let matrix = assembled(matrix)?;
    let n = matrix.rows();
    let pattern: Vec<_> = matrix.triplets().iter().map(|t| (t.row, t.col)).collect();
    let mut sparse = Sparse::new(n);
    let handles: Vec<_> = matrix
        .triplets()
        .iter()
        .map(|t| {
            let handle = sparse.get_element(t.row + 1, t.col + 1);
            sparse.set(handle, t.value);
            handle
        })
        .collect();
    let symbolic = SparseSymbolic {
        n,
        key: Key::Triplets(Arc::new(pattern)),
        handles: Arc::new(handles),
        sparse: Arc::new(Sparse::new(0)),
    };
    Ok((sparse, symbolic))
}

/// One `NIiter` step on `matrix`: the factorization ngspice performs after a
/// load, then `SMPsolve`.
///
/// A matrix never ordered (no matching `ordering`; a bound matrix uses its
/// pattern's persistent frame instead) is MNA-preordered once
/// (`SMPpreOrder`). Then `LoadGmin(diag_gmin)` and, with `reorder`
/// (`NISHOULDREORDER`), `spOrderAndFactor`, which keeps every existing pivot
/// that still passes the thresholds and reorders from the first that does
/// not; without it `spFactor` with the existing order, which reports a zero
/// pivot as [`NiStep::ZeroPivot`] for the caller to reload and reorder.
///
/// # Errors
/// Invalid assembly or RHS, a singular reordered matrix, or a non-finite
/// solution.
pub fn solve_ni(
    matrix: &SparseMatrix,
    rhs: &Vector,
    ordering: &mut Option<SparseSymbolic>,
    reorder: bool,
    diag_gmin: f64,
) -> SpiceResult<NiStep> {
    if let Some((pattern, _)) = matrix.bound_values() {
        let mut cached = pattern.take_ordering();
        let step = ni_step(matrix, rhs, &mut cached, reorder, diag_gmin);
        pattern.store_ordering(cached);
        return step;
    }
    ni_step(matrix, rhs, ordering, reorder, diag_gmin)
}

fn ni_step(
    matrix: &SparseMatrix,
    rhs: &Vector,
    ordering: &mut Option<SparseSymbolic>,
    reorder: bool,
    diag_gmin: f64,
) -> SpiceResult<NiStep> {
    let pivot = matrix.pivot_tolerances();
    let previous = ordering.take().filter(|previous| previous.matches(matrix));
    let (mut sparse, symbolic, fresh) = match previous {
        Some(previous) => {
            let mut sparse =
                Arc::try_unwrap(previous.sparse).unwrap_or_else(|shared| (*shared).clone());
            let owned: Vec<f64>;
            let values: &[f64] = match matrix.bound_values() {
                Some((_, values)) => {
                    finite_bound(values)?;
                    values
                }
                None => {
                    owned = assembled(matrix)?
                        .triplets()
                        .iter()
                        .map(|t| t.value)
                        .collect();
                    &owned
                }
            };
            sparse.clear();
            for (handle, value) in previous.handles.iter().zip(values) {
                sparse.set(*handle, *value);
            }
            let symbolic = SparseSymbolic {
                sparse: Arc::new(Sparse::new(0)),
                ..previous
            };
            (sparse, symbolic, false)
        }
        None => {
            let (mut sparse, symbolic) = fresh_frame(matrix)?;
            if !sparse.fully_translated() {
                return Err(numerical(
                    "sparse LU",
                    "singular matrix: an unknown has no matrix entries",
                ));
            }
            sparse.mna_preorder();
            (sparse, symbolic, true)
        }
    };
    sparse.load_gmin(diag_gmin);
    let status = if fresh || reorder {
        sparse.order_and_factor(pivot.relative, pivot.absolute)
    } else {
        sparse.factor()
    };
    // spZERO_DIAG and spSINGULAR are both E_SINGULAR to NIiter.
    let zero_pivot = !fresh && !reorder && matches!(status, Status::ZeroDiag | Status::Singular);
    let checked = if zero_pivot {
        Ok(())
    } else {
        check_status("sparse LU", status, &sparse)
    };
    let factored = SparseSymbolic {
        sparse: Arc::new(sparse),
        ..symbolic
    };
    let result = checked.and_then(|()| {
        if zero_pivot {
            return Ok(NiStep::ZeroPivot);
        }
        let lu = SparseLu {
            symbolic: factored.clone(),
        };
        lu.solve(rhs).map(NiStep::Solved)
    });
    *ordering = Some(factored);
    result
}

/// Owned numeric sparse LU factors.
#[derive(Debug)]
pub struct SparseLu {
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

fn finite_bound(values: &[f64]) -> SpiceResult<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(numerical("sparse LU assembly", "non-finite coefficient"));
    }
    Ok(())
}

impl SparseLu {
    /// Factors a snapshot, optionally refactoring with the pivot order of an
    /// identical earlier pattern.
    ///
    /// # Errors
    /// Invalid dimensions, coefficients, changed pattern, or a singular matrix.
    pub fn new(matrix: &SparseMatrix, reuse: Option<&SparseSymbolic>) -> SpiceResult<Self> {
        if let Some(previous) = reuse {
            if !previous.matches(matrix) {
                return Err(numerical(
                    "sparse symbolic LU",
                    "assembled CSC pattern changed",
                ));
            }
            return Self::refactor(previous.clone(), matrix);
        }
        if let Some((pattern, values)) = matrix.bound_values() {
            square(matrix.rows(), matrix.cols())?;
            finite_bound(values)?;
            // Elements are created in pattern (device stamp) order, zeros
            // included, as `TSTALLOC` creates them in ngspice.
            let mut sparse = Sparse::new(pattern.order());
            let handles: Vec<_> = (0..pattern.len())
                .map(|index| {
                    let id = crate::maths::sparse::element_id(index);
                    let (row, col) = pattern.position(id);
                    let handle = sparse.get_element(row + 1, col + 1);
                    sparse.set(handle, values[index]);
                    handle
                })
                .collect();
            order_fresh("sparse LU", &mut sparse, matrix.pivot_tolerances())?;
            return Ok(Self {
                symbolic: SparseSymbolic {
                    n: pattern.order(),
                    key: Key::Pattern(Arc::downgrade(pattern)),
                    handles: Arc::new(handles),
                    sparse: Arc::new(sparse),
                },
            });
        }
        let matrix = assembled(matrix)?;
        let n = matrix.rows();
        let pattern: Vec<_> = matrix.triplets().iter().map(|t| (t.row, t.col)).collect();
        let mut sparse = Sparse::new(n);
        let handles: Vec<_> = matrix
            .triplets()
            .iter()
            .map(|t| {
                let handle = sparse.get_element(t.row + 1, t.col + 1);
                sparse.set(handle, t.value);
                handle
            })
            .collect();
        order_fresh("sparse LU", &mut sparse, matrix.pivot_tolerances())?;
        Ok(Self {
            symbolic: SparseSymbolic {
                n,
                key: Key::Triplets(Arc::new(pattern)),
                handles: Arc::new(handles),
                sparse: Arc::new(sparse),
            },
        })
    }

    /// `SMPluFac` with `previous`'s pivots on a matching matrix, reordering
    /// after a zero pivot as `NIiter` does after `E_SINGULAR`.
    fn refactor(previous: SparseSymbolic, matrix: &SparseMatrix) -> SpiceResult<Self> {
        let SparseSymbolic {
            n,
            key,
            handles,
            sparse,
        } = previous;
        let mut sparse = Arc::try_unwrap(sparse).unwrap_or_else(|shared| (*shared).clone());
        let owned: Vec<f64>;
        let values: &[f64] = match matrix.bound_values() {
            Some((_, values)) => {
                finite_bound(values)?;
                values
            }
            None => {
                owned = assembled(matrix)?
                    .triplets()
                    .iter()
                    .map(|t| t.value)
                    .collect();
                &owned
            }
        };
        let load = |sparse: &mut Sparse<f64>| {
            sparse.clear();
            for (handle, value) in handles.iter().zip(values) {
                sparse.set(*handle, *value);
            }
        };
        load(&mut sparse);
        let mut status = sparse.factor();
        if status == Status::ZeroDiag {
            load(&mut sparse);
            let pivot = matrix.pivot_tolerances();
            status = sparse.order_and_factor(pivot.relative, pivot.absolute);
        }
        check_status("sparse LU", status, &sparse)?;
        Ok(Self {
            symbolic: SparseSymbolic {
                n,
                key,
                handles,
                sparse: Arc::new(sparse),
            },
        })
    }

    /// Pivot order for subsequent identical-pattern assemblies.
    pub fn symbolic(&self) -> &SparseSymbolic {
        &self.symbolic
    }

    /// Solves with this snapshot; later mutations to storage cannot stale it.
    ///
    /// # Errors
    /// Invalid RHS or non-finite solution.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        rhs_valid(rhs, self.symbolic.n)?;
        let x = Vector::from_slice(&self.symbolic.sparse.solve(rhs.as_slice()));
        if !x.is_finite() {
            return Err(numerical(
                "LU solve",
                "non-finite solution (singular system)",
            ));
        }
        Ok(x)
    }

    /// `spDeterminant`: `(mantissa, decimal exponent)`.
    pub fn determinant(&self) -> (f64, i32) {
        self.symbolic.sparse.determinant()
    }
}

/// Dense LU through the same SPARSE factorization (ngspice has no separate
/// dense simulator solve); zero entries are not stored, as with SPARSE.
#[derive(Debug)]
pub struct DenseLu {
    factor: SparseLu,
}

impl DenseLu {
    /// Factors a nonempty square finite matrix.
    ///
    /// # Errors
    /// Invalid dimensions/coefficients or a singular matrix.
    pub fn new(matrix: &Matrix) -> SpiceResult<Self> {
        square(matrix.rows(), matrix.cols())?;
        if matrix.data().iter().any(|a| !a.is_finite()) {
            return Err(numerical("dense LU", "non-finite coefficient"));
        }
        let mut sparse = SparseMatrix::new(matrix.rows(), matrix.cols());
        for row in 0..matrix.rows() {
            for col in 0..matrix.cols() {
                let value = matrix.get(row, col).unwrap_or(0.0);
                if value != 0.0 {
                    sparse.add(row, col, value)?;
                }
            }
        }
        let factor = SparseLu::new(&sparse, None).map_err(|error| match error {
            SpiceError::Numerical { message, .. } => numerical("dense LU", message),
            other => other,
        })?;
        Ok(Self { factor })
    }

    /// Solves against the stored factors.
    ///
    /// # Errors
    /// Invalid RHS or non-finite solution.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        self.factor.solve(rhs)
    }
}
