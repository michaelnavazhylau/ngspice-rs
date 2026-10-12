//! Triplet storage for the sparse MNA matrix.
//!
//! ngspice's MNA matrix is built by every device adding small contributions at
//! scattered `(row, column)` positions — see `cktload.c` and the `*Load`
//! functions throughout `src/spicelib/devices/`. SPARSE 1.3
//! (`src/maths/sparse/spbuild.c`) and KLU each have their own way of accepting
//! those contributions; a coordinate list is the format both can be fed from,
//! and the format the port uses to accumulate a stamp before assembling.
//!
//! Factorization returns owned SPARSE 1.3 factors, independent of stamping storage.
//!
//! A matrix can also be *bound* to a [`Pattern`]: the fixed set of element
//! positions a circuit stamps, each with an [`ElementId`] assigned in the order
//! the devices first stamped it (ngspice's `TSTALLOC`/`spGetElement` order).
//! A bound matrix accumulates `values[id] += v` instead of appending triplets,
//! so a Newton iteration neither allocates nor sorts (ngspice: `spClear` then
//! `*ptr += v`). Contributions outside the pattern go to an overflow list, from
//! which the owner extends the pattern (SPARSE: a new element forces a reorder).

use std::borrow::Cow;
use std::cell::OnceCell;
use std::sync::Arc;

use crate::primitives::{Real, SpiceError, SpiceResult};
use petgraph::graphmap::UnGraphMap;

use crate::maths::dense::Vector;
use crate::maths::linear::SparseLu;

/// One `(row, column, value)` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triplet {
    /// Row index.
    pub row: usize,
    /// Column index.
    pub col: usize,
    /// The contribution.
    pub value: Real,
}

/// Handle of one matrix element in a [`Pattern`] (SPARSE's element pointer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(u32);

impl ElementId {
    /// Position of the element in its pattern's creation order.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The element positions of a square matrix, numbered in creation order, with
/// a per-row sorted index for `(row, col) -> ElementId` lookup.
///
/// The pattern also keeps the SPARSE pivot order last used for it, so every
/// solve of a circuit refactors one ordered matrix as ngspice does, across
/// Newton solves and timepoints.
#[derive(Debug)]
pub struct Pattern {
    n: usize,
    positions: Vec<(u32, u32)>,
    row_start: Vec<u32>,
    row_cols: Vec<u32>,
    row_ids: Vec<u32>,
    ordering: std::sync::Mutex<Option<crate::maths::SparseSymbolic>>,
    /// The `(row, col, id)` sequence of the last load, replayed by bound
    /// matrices to skip lookups (ngspice's per-device element pointers).
    trace: std::sync::Mutex<Arc<Trace>>,
    /// ngspice's `NIACSHOULDREORDER`: set when the matrix is created
    /// (`NIreinit`), cleared by the first complex reorder.
    ac_reorder: std::sync::atomic::AtomicBool,
}

/// One recorded stamp: `(row, col, element)`.
type TraceEntry = (u32, u32, u32);

/// A load's stamp sequence, split per device: device `k` stamped
/// `entries[starts[k]..starts[k + 1]]`.
#[derive(Debug, Clone, Default, PartialEq)]
struct Trace {
    entries: Vec<TraceEntry>,
    starts: Vec<u32>,
}

impl Clone for Pattern {
    /// A copy without the cached pivot order.
    fn clone(&self) -> Self {
        Self::index(self.n, self.positions.clone())
    }
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.n == other.n && self.positions == other.positions
    }
}

impl Pattern {
    /// A pattern of order `n` from positions in creation order; repeats keep
    /// their first appearance.
    ///
    /// # Panics
    /// Only if a position is out of range or there are more than `u32::MAX`
    /// elements; callers pass positions already validated by a matrix.
    #[must_use]
    pub fn new(n: usize, positions: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut seen = std::collections::HashSet::new();
        let positions: Vec<(u32, u32)> = positions
            .into_iter()
            .filter(|position| seen.insert(*position))
            .map(|(row, col)| {
                assert!(row < n && col < n, "pattern position out of range");
                (to_u32(row), to_u32(col))
            })
            .collect();
        Self::index(n, positions)
    }

    fn index(n: usize, positions: Vec<(u32, u32)>) -> Self {
        let mut order: Vec<u32> = (0..to_u32(positions.len())).collect();
        order.sort_unstable_by_key(|&id| positions[id as usize]);
        let mut row_start = vec![0_u32; n + 1];
        for &(row, _) in &positions {
            row_start[row as usize + 1] += 1;
        }
        for row in 0..n {
            row_start[row + 1] += row_start[row];
        }
        let row_cols = order.iter().map(|&id| positions[id as usize].1).collect();
        Self {
            n,
            positions,
            row_start,
            row_cols,
            row_ids: order,
            ordering: std::sync::Mutex::new(None),
            trace: std::sync::Mutex::new(Arc::new(Trace::default())),
            ac_reorder: std::sync::atomic::AtomicBool::new(true),
        }
    }

    fn trace(&self) -> Arc<Trace> {
        self.trace
            .lock()
            .map(|trace| Arc::clone(&trace))
            .unwrap_or_default()
    }

    fn store_trace(&self, trace: Arc<Trace>) {
        if let Ok(mut slot) = self.trace.lock() {
            *slot = trace;
        }
    }

    /// Takes the cached pivot order (leaving none while a solve uses it).
    pub(crate) fn take_ordering(&self) -> Option<crate::maths::SparseSymbolic> {
        self.ordering.lock().ok().and_then(|mut slot| slot.take())
    }

    /// `NIACSHOULDREORDER`: whether the next complex factorization reorders.
    pub(crate) fn ac_reorder(&self) -> bool {
        self.ac_reorder.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Sets or clears `NIACSHOULDREORDER`.
    pub(crate) fn set_ac_reorder(&self, value: bool) {
        self.ac_reorder
            .store(value, std::sync::atomic::Ordering::Relaxed);
    }

    /// Caches the pivot order for the next solve.
    pub(crate) fn store_ordering(&self, ordering: Option<crate::maths::SparseSymbolic>) {
        if let Ok(mut slot) = self.ordering.lock() {
            *slot = ordering;
        }
    }

    /// This pattern plus new positions appended in order (existing ids keep
    /// their numbers).
    #[must_use]
    pub fn extended(&self, extra: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut positions = self.positions.clone();
        for (row, col) in extra {
            if self.find(row, col).is_none() {
                let position = (to_u32(row), to_u32(col));
                if !positions[self.positions.len()..].contains(&position) {
                    positions.push(position);
                }
            }
        }
        Self::index(self.n, positions)
    }

    /// Order of the matrix.
    #[must_use]
    pub const fn order(&self) -> usize {
        self.n
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// True when the pattern has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// `(row, col)` of an element.
    #[must_use]
    pub fn position(&self, id: ElementId) -> (usize, usize) {
        let (row, col) = self.positions[id.index()];
        (row as usize, col as usize)
    }

    /// The element at `(row, col)`, if the pattern has one.
    #[must_use]
    pub fn find(&self, row: usize, col: usize) -> Option<ElementId> {
        if row >= self.n {
            return None;
        }
        let (start, end) = (
            self.row_start[row] as usize,
            self.row_start[row + 1] as usize,
        );
        let col = u32::try_from(col).ok()?;
        self.row_cols[start..end]
            .binary_search(&col)
            .ok()
            .map(|offset| ElementId(self.row_ids[start + offset]))
    }

    /// Element ids in row-major position order.
    fn sorted(&self) -> impl Iterator<Item = ElementId> + '_ {
        self.row_ids.iter().map(|&id| ElementId(id))
    }
}

/// The element id with creation index `index` (for solver adapters).
pub(crate) fn element_id(index: usize) -> ElementId {
    ElementId(to_u32(index))
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).expect("matrix index exceeds u32")
}

/// Values of a matrix bound to a [`Pattern`].
#[derive(Debug, Clone, PartialEq)]
struct Bound {
    pattern: Arc<Pattern>,
    values: Vec<Real>,
    /// The folded triplet view, built on demand for triplet consumers.
    view: OnceCell<Vec<Triplet>>,
    /// The pattern's recorded stamp sequence, the device now stamping and the
    /// part of its segment still to match.
    trace: Arc<Trace>,
    device: usize,
    cursor: usize,
    end: usize,
    /// This load's own sequence, once any device diverged from `trace`.
    recording: Option<Trace>,
}

impl Bound {
    /// Starts device `index`'s stamps: replay resumes at its segment.
    fn begin_device(&mut self, index: usize) {
        self.device = index;
        if let Some(recording) = &mut self.recording {
            recording.starts.push(to_u32(recording.entries.len()));
        }
        match (
            self.trace.starts.get(index),
            self.trace.starts.get(index + 1),
        ) {
            (Some(&start), Some(&end)) => {
                self.cursor = start as usize;
                self.end = end as usize;
            }
            _ => {
                self.cursor = 0;
                self.end = 0;
            }
        }
    }

    /// The element at `(row, col)`: the device's next traced stamp when it
    /// matches (two comparisons), otherwise a pattern lookup that switches
    /// this load to recording its own sequence.
    fn element(&mut self, row: usize, col: usize) -> Option<ElementId> {
        let (r, c) = (u32::try_from(row).ok()?, u32::try_from(col).ok()?);
        if self.cursor < self.end {
            let (tr, tc, id) = self.trace.entries[self.cursor];
            if tr == r && tc == c {
                self.cursor += 1;
                if let Some(recording) = &mut self.recording {
                    recording.entries.push((r, c, id));
                }
                return Some(ElementId(id));
            }
        }
        if self.recording.is_none() {
            // Everything before this stamp matched the trace.
            let done = self
                .trace
                .starts
                .get(self.device)
                .map_or(0, |&start| start as usize);
            let matched = if self.end > 0 { self.cursor } else { done };
            let starts = (0..=self.device)
                .map(|d| self.trace.starts.get(d).copied().unwrap_or(0))
                .collect();
            self.recording = Some(Trace {
                entries: self.trace.entries[..matched].to_vec(),
                starts,
            });
        }
        // The rest of this device is looked up.
        self.end = 0;
        let id = self.pattern.find(row, col)?;
        if let Some(recording) = &mut self.recording {
            recording.entries.push((r, c, id.0));
        }
        Some(id)
    }
}

/// A sparse matrix as a list of contributions.
///
/// Duplicate positions are allowed and are summed by [`SparseMatrix::get`] and by
/// [`SparseMatrix::fold_duplicates`]. Devices rely on this: a resistor between
/// two non-ground nodes stamps four positions, and several devices may stamp the
/// same position.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseMatrix {
    rows: usize,
    cols: usize,
    /// Every contribution (unbound), or the overflow outside the pattern.
    triplets: Vec<Triplet>,
    folded: bool,
    bound: Option<Bound>,
    /// Thresholds used when this matrix is (re)ordered.
    pivot: crate::maths::PivotTolerances,
    /// Per row, the circuit unknown's name (C `NODENAME`), used by solver
    /// diagnostics; `None` for a standalone matrix.
    names: Option<crate::maths::UnknownNames>,
}

impl SparseMatrix {
    /// An empty `rows` by `cols` matrix.
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            triplets: Vec::new(),
            folded: false,
            bound: None,
            pivot: crate::maths::PivotTolerances::DEFAULT,
            names: None,
        }
    }

    /// An all-zero matrix bound to `pattern`, accumulating by element.
    #[must_use]
    pub fn bound(pattern: Arc<Pattern>) -> Self {
        let n = pattern.order();
        Self {
            rows: n,
            cols: n,
            triplets: Vec::new(),
            folded: true,
            bound: Some(Bound {
                values: vec![0.0; pattern.len()],
                trace: pattern.trace(),
                device: 0,
                cursor: 0,
                end: 0,
                recording: None,
                pattern,
                view: OnceCell::new(),
            }),
            pivot: crate::maths::PivotTolerances::DEFAULT,
            names: None,
        }
    }

    /// Pivot thresholds used when this matrix is ordered for factorization
    /// (default [`crate::maths::PivotTolerances::DEFAULT`]).
    #[must_use]
    pub const fn pivot_tolerances(&self) -> crate::maths::PivotTolerances {
        self.pivot
    }

    /// Replaces the pivot thresholds (`.options pivtol`/`pivrel`); validated
    /// by the caller (see [`crate::maths::PivotTolerances::validate`]).
    pub const fn set_pivot_tolerances(&mut self, pivot: crate::maths::PivotTolerances) {
        self.pivot = pivot;
    }

    /// The circuit unknowns' names by row, when set: solver warnings and
    /// singular-matrix errors name nodes as ngspice's `NIiter` does
    /// (`singular matrix:  check node ...`) instead of row numbers.
    #[must_use]
    pub fn unknown_names(&self) -> Option<&crate::maths::UnknownNames> {
        self.names.as_ref()
    }

    /// Sets (or clears) the per-row unknown names used in diagnostics.
    pub fn set_unknown_names(&mut self, names: Option<crate::maths::UnknownNames>) {
        self.names = names;
    }

    /// The pattern and per-element values when bound with no overflow: the
    /// fast path for factorization.
    #[must_use]
    pub fn bound_values(&self) -> Option<(&Arc<Pattern>, &[Real])> {
        match &self.bound {
            Some(bound) if self.triplets.is_empty() => Some((&bound.pattern, &bound.values)),
            _ => None,
        }
    }

    /// The bound pattern and, in stamp order, the positions outside it.
    #[must_use]
    pub fn overflow(&self) -> Option<(&Arc<Pattern>, impl Iterator<Item = (usize, usize)> + '_)> {
        self.bound
            .as_ref()
            .map(|bound| (&bound.pattern, self.triplets.iter().map(|t| (t.row, t.col))))
    }

    /// Marks the start of device `index`'s stamps within a load, so a
    /// diverging device only affects its own trace segment.
    pub fn begin_device(&mut self, index: usize) {
        if let Some(bound) = &mut self.bound {
            bound.begin_device(index);
        }
    }

    /// Saves this load's stamp sequence on the pattern when it differed from
    /// the recorded one (call once the load is complete).
    pub fn record_trace(&self) {
        let Some(bound) = &self.bound else {
            return;
        };
        if !self.triplets.is_empty() {
            return;
        }
        if let Some(recording) = &bound.recording {
            let mut trace = recording.clone();
            trace.starts.push(to_u32(trace.entries.len()));
            bound.pattern.store_trace(Arc::new(trace));
        }
    }

    /// Adds to an element by handle (`*ptr += value`).
    pub fn add_element(&mut self, element: ElementId, value: Real) {
        if let Some(bound) = &mut self.bound {
            bound.values[element.index()] += value;
            bound.view.take();
        }
    }

    /// Zeroes every element of `row` whose column `keep` rejects, keeping it
    /// in the structure (C `ZeroNoncurRow` in `cktload.c` stores `0.0`
    /// through `SMPfindElt`). Returns whether a kept column holds an element.
    pub fn zero_row(&mut self, row: usize, keep: impl Fn(usize) -> bool) -> bool {
        let mut kept = false;
        if let Some(bound) = &mut self.bound {
            bound.view.take();
            for (index, value) in bound.values.iter_mut().enumerate() {
                let (r, c) = bound.pattern.position(ElementId(to_u32(index)));
                if r == row {
                    if keep(c) {
                        kept = true;
                    } else {
                        *value = 0.0;
                    }
                }
            }
        }
        for triplet in self.triplets.iter_mut().filter(|t| t.row == row) {
            if keep(triplet.col) {
                kept = true;
            } else {
                triplet.value = 0.0;
            }
        }
        kept
    }

    /// Overwrites the value at a position (C `*ptr = value` on an element),
    /// creating the position if it is not stored.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the position is out of range.
    pub fn set(&mut self, row: usize, col: usize, value: Real) -> SpiceResult<()> {
        self.triplets.retain(|t| t.row != row || t.col != col);
        if let Some(bound) = &mut self.bound
            && let Some(id) = bound.pattern.find(row, col)
        {
            bound.view.take();
            bound.values[id.index()] = value;
            return Ok(());
        }
        self.add(row, col, value)
    }

    /// Number of rows.
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns.
    #[must_use]
    pub const fn cols(&self) -> usize {
        self.cols
    }

    /// Number of stored contributions, including duplicates.
    #[must_use]
    pub fn nnz(&self) -> usize {
        if self.bound.is_some() {
            return self.triplets().len();
        }
        self.triplets.len()
    }

    /// True when nothing has been stamped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nnz() == 0
    }

    /// True when duplicate positions have been folded away.
    #[must_use]
    pub fn is_folded(&self) -> bool {
        self.folded || self.bound.is_some()
    }

    /// Adds a contribution.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the position is out of range.
    pub fn add(&mut self, row: usize, col: usize, value: Real) -> SpiceResult<()> {
        if row >= self.rows || col >= self.cols {
            return Err(SpiceError::Numerical {
                context: "sparse matrix".to_owned(),
                message: format!(
                    "stamp at ({row}, {col}) is out of range for a {}x{} matrix",
                    self.rows, self.cols
                ),
            });
        }
        if let Some(bound) = &mut self.bound {
            bound.view.take();
            if let Some(id) = bound.element(row, col) {
                bound.values[id.index()] += value;
                return Ok(());
            }
        }
        if value != 0.0 {
            self.triplets.push(Triplet { row, col, value });
            self.folded = false;
        }
        Ok(())
    }

    /// The contribution at a position, summed over duplicates.
    ///
    /// Linear in the number of stored contributions; call
    /// [`SparseMatrix::fold_duplicates`] first if this is on a hot path.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Real {
        let bound = self.bound.as_ref().and_then(|bound| {
            bound
                .pattern
                .find(row, col)
                .map(|id| bound.values[id.index()])
        });
        bound.unwrap_or(0.0)
            + self
                .triplets
                .iter()
                .filter(|triplet| triplet.row == row && triplet.col == col)
                .map(|triplet| triplet.value)
                .sum::<Real>()
    }

    /// Sorts and sums duplicate positions.
    pub fn fold_duplicates(&mut self) {
        if self.folded {
            return;
        }
        self.triplets
            .sort_by_key(|triplet| (triplet.row, triplet.col));
        let mut folded: Vec<Triplet> = Vec::with_capacity(self.triplets.len());
        for triplet in self.triplets.drain(..) {
            match folded.last_mut() {
                Some(last) if last.row == triplet.row && last.col == triplet.col => {
                    last.value += triplet.value;
                }
                _ => folded.push(triplet),
            }
        }
        folded.retain(|triplet| triplet.value != 0.0);
        self.triplets = folded;
        self.folded = true;
    }

    /// Drops every contribution, keeping the dimensions.
    pub fn clear(&mut self) {
        self.triplets.clear();
        self.folded = true;
        if let Some(bound) = &mut self.bound {
            bound.values.fill(0.0);
            bound.view.take();
            bound.device = 0;
            bound.cursor = 0;
            bound.end = 0;
            bound.recording = None;
        }
    }

    /// The stored contributions. For a bound matrix: the folded nonzero
    /// elements in row-major order, merged with any overflow.
    #[must_use]
    pub fn triplets(&self) -> &[Triplet] {
        let Some(bound) = &self.bound else {
            return &self.triplets;
        };
        bound.view.get_or_init(|| {
            let mut view: Vec<Triplet> = bound
                .pattern
                .sorted()
                .filter_map(|id| {
                    let value = bound.values[id.index()];
                    let (row, col) = bound.pattern.position(id);
                    (value != 0.0).then_some(Triplet { row, col, value })
                })
                .collect();
            if !self.triplets.is_empty() {
                let mut overflow = SparseMatrix::new(self.rows, self.cols);
                overflow.triplets.clone_from(&self.triplets);
                overflow.fold_duplicates();
                view.extend(overflow.triplets);
                view.sort_by_key(|t| (t.row, t.col));
            }
            view
        })
    }

    /// Builds a petgraph graph of the assembled matrix's structural couplings.
    ///
    /// Vertices are matrix row indices, including diagonal-only/empty rows.
    /// An undirected edge exists when either off-diagonal entry is nonzero
    /// after summing duplicate stamps (as in `cktload.c` assembly). Exact zero
    /// cancellations disappear; asymmetric entries are not summed together.
    /// The input matrix is left unchanged. Use petgraph algorithms for block
    /// decomposition instead of maintaining a separate adjacency structure.
    ///
    /// MNA eliminates ground, so row 0 is an ordinary unknown, **not** ground.
    /// This graph has no physical ground-path information. Disconnected blocks
    /// can all be nonsingular; even a connected matrix can be singular. No
    /// solvability or finite-value checks are performed by this structural API.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] if the matrix is not square: rows and columns
    /// must refer to the same unknown set for this coupling interpretation.
    pub fn coupling_graph(&self) -> SpiceResult<UnGraphMap<usize, ()>> {
        if self.rows != self.cols {
            return Err(SpiceError::Numerical {
                context: "matrix coupling graph".to_owned(),
                message: format!("expected a square matrix, got {}x{}", self.rows, self.cols),
            });
        }
        let mut graph = UnGraphMap::new();
        for row in 0..self.rows {
            graph.add_node(row);
        }
        let mut assembled = Cow::Borrowed(self);
        if !self.is_folded() {
            assembled.to_mut().fold_duplicates();
        }
        for triplet in assembled.triplets() {
            if triplet.row != triplet.col {
                graph.add_edge(triplet.row, triplet.col, ());
            }
        }
        Ok(graph)
    }

    /// Multiplies by a dense vector, for testing the assembled system.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the dimensions do not agree.
    pub fn mul_vector(&self, vector: &Vector) -> SpiceResult<Vector> {
        if vector.len() != self.cols {
            return Err(SpiceError::Numerical {
                context: "sparse matrix".to_owned(),
                message: format!(
                    "vector length {} does not match {} columns",
                    vector.len(),
                    self.cols
                ),
            });
        }
        let mut result = Vector::zeros(self.rows);
        for triplet in self.triplets() {
            let current = result.get(triplet.row).unwrap_or(0.0);
            let contribution = triplet.value * vector.get(triplet.col).unwrap_or(0.0);
            result.set(triplet.row, current + contribution)?;
        }
        Ok(result)
    }

    /// Factors a snapshot. Empty systems are explicitly rejected.
    ///
    /// # Errors
    ///
    /// Invalid assembly or a structurally/numerically singular matrix.
    pub fn factorize(&self) -> SpiceResult<SparseLu> {
        SparseLu::new(self, None)
    }

    /// Convenience solve, factoring the current storage each time.
    ///
    /// For repeated RHS solves, retain the owned result of `factorize`.
    /// No factorization is required beforehand and there is no backend cache.
    ///
    /// # Errors
    ///
    /// Invalid assembly/RHS, singular matrix, or failed residual.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        self.factorize()?.solve(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::SparseMatrix;
    use crate::maths::dense::Vector;

    #[test]
    fn zero_contributions_are_not_stored() {
        let mut matrix = SparseMatrix::new(2, 2);
        matrix.add(0, 0, 0.0).unwrap();
        assert!(matrix.is_empty());
        assert_eq!(matrix.nnz(), 0);
    }

    #[test]
    fn duplicates_sum_on_read() {
        let mut matrix = SparseMatrix::new(2, 2);
        matrix.add(0, 0, 0.001).unwrap();
        matrix.add(0, 0, 0.002).unwrap();
        matrix.add(1, 1, 5.0).unwrap();
        assert_eq!(matrix.nnz(), 3);
        assert!((matrix.get(0, 0) - 0.003).abs() < 1e-18);
        assert_eq!(matrix.get(0, 1), 0.0);
    }

    #[test]
    fn folding_collapses_duplicates_and_cancels_zeros() {
        let mut matrix = SparseMatrix::new(2, 2);
        matrix.add(1, 1, 1.0).unwrap();
        matrix.add(0, 0, 2.0).unwrap();
        matrix.add(0, 0, -2.0).unwrap();
        matrix.fold_duplicates();
        assert!(matrix.is_folded());
        assert_eq!(matrix.nnz(), 1);
        assert_eq!(matrix.triplets()[0].row, 1);
        assert_eq!(matrix.get(0, 0), 0.0);
    }

    #[test]
    fn out_of_range_stamps_are_errors() {
        let mut matrix = SparseMatrix::new(2, 2);
        assert!(matrix.add(0, 2, 1.0).is_err());
        assert!(matrix.add(2, 0, 1.0).is_err());
    }

    #[test]
    fn multiply_walks_the_contributions() {
        let mut matrix = SparseMatrix::new(2, 2);
        matrix.add(0, 0, 2.0).unwrap();
        matrix.add(0, 1, 1.0).unwrap();
        matrix.add(1, 1, 3.0).unwrap();
        let product = matrix.mul_vector(&Vector::from_slice(&[1.0, 1.0])).unwrap();
        assert_eq!(product.as_slice(), &[3.0, 3.0]);
        assert!(matrix.mul_vector(&Vector::zeros(3)).is_err());
    }

    #[test]
    fn singular_system_is_rejected() {
        let matrix = SparseMatrix::new(1, 1);
        assert!(matrix.solve(&Vector::zeros(1)).is_err());
        assert!(!matrix.factorize().unwrap_err().is_not_yet_ported());
    }
}
