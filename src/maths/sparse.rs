//! Triplet storage for the sparse MNA matrix.
//!
//! ngspice's MNA matrix is built by every device adding small contributions at
//! scattered `(row, column)` positions — see `cktload.c` and the `*Load`
//! functions throughout `src/spicelib/devices/`. SPARSE 1.3
//! (`src/maths/sparse/spbuild.c`) and KLU each have their own way of accepting
//! those contributions; a coordinate list is the format both can be fed from,
//! and the format the port uses to accumulate a stamp before assembling.
//!
//! Factorization returns owned faer factors, independent of stamping storage.

use std::borrow::Cow;

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
    triplets: Vec<Triplet>,
    folded: bool,
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
        }
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
        self.triplets.len()
    }

    /// True when nothing has been stamped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.triplets.is_empty()
    }

    /// True when duplicate positions have been folded away.
    #[must_use]
    pub const fn is_folded(&self) -> bool {
        self.folded
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
        if value != 0.0 {
            self.triplets.push(Triplet { row, col, value });
        }
        self.folded = false;
        Ok(())
    }

    /// The contribution at a position, summed over duplicates.
    ///
    /// Linear in the number of stored contributions; call
    /// [`SparseMatrix::fold_duplicates`] first if this is on a hot path.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Real {
        self.triplets
            .iter()
            .filter(|triplet| triplet.row == row && triplet.col == col)
            .map(|triplet| triplet.value)
            .sum()
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
    }

    /// The stored contributions.
    #[must_use]
    pub fn triplets(&self) -> &[Triplet] {
        &self.triplets
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
        for triplet in &self.triplets {
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
