//! Dense row-major real matrix and vector storage.
//!
//! Mirrors `src/maths/dense/` (`smat.c`), which ngspice uses for the small
//! systems in `.sens`, `.tf` and `.pz`. The MNA matrix itself is sparse — see
//! [`crate::sparse`] — and lands here only after being densified for those
//! analyses.
//!
//! Storage is ported; [`Matrix::solve`] and [`Matrix::lu_decompose`] are not.

use spice_core::{Real, SpiceError, SpiceResult};

use crate::C_REFERENCE_DENSE;

fn index_error(rows: usize, cols: usize, row: usize, col: usize) -> SpiceError {
    SpiceError::Numerical {
        context: "dense matrix".to_owned(),
        message: format!("index ({row}, {col}) is out of range for a {rows}x{cols} matrix"),
    }
}

fn length_error(expected: usize, actual: usize) -> SpiceError {
    SpiceError::Numerical {
        context: "dense vector".to_owned(),
        message: format!("expected length {expected}, got {actual}"),
    }
}

/// A dense, row-major real matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Real>,
}

impl Matrix {
    /// A zeroed `rows` by `cols` matrix.
    #[must_use]
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
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

    /// Total number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when the matrix has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// True when the matrix is square.
    #[must_use]
    pub const fn is_square(&self) -> bool {
        self.rows == self.cols
    }

    /// Reads an element, or `None` when out of range.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<Real> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.data.get(row * self.cols + col).copied()
    }

    /// Writes an element.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the index is out of range.
    pub fn set(&mut self, row: usize, col: usize, value: Real) -> SpiceResult<()> {
        if row >= self.rows || col >= self.cols {
            return Err(index_error(self.rows, self.cols, row, col));
        }
        self.data[row * self.cols + col] = value;
        Ok(())
    }

    /// Adds to an element, which is what MNA stamping does.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the index is out of range.
    pub fn add_to(&mut self, row: usize, col: usize, value: Real) -> SpiceResult<()> {
        if row >= self.rows || col >= self.cols {
            return Err(index_error(self.rows, self.cols, row, col));
        }
        self.data[row * self.cols + col] += value;
        Ok(())
    }

    /// One row, as a slice.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[Real]> {
        if row >= self.rows {
            return None;
        }
        self.data.get(row * self.cols..(row + 1) * self.cols)
    }

    /// The elements, row-major.
    #[must_use]
    pub fn data(&self) -> &[Real] {
        &self.data
    }

    /// The elements, row-major, mutable.
    pub fn data_mut(&mut self) -> &mut [Real] {
        &mut self.data
    }

    /// Sets every element to `value`.
    pub fn fill(&mut self, value: Real) {
        self.data.fill(value);
    }

    /// Sets every element to zero.
    pub fn clear(&mut self) {
        self.fill(0.0);
    }

    /// The transpose.
    #[must_use]
    pub fn transpose(&self) -> Self {
        let mut result = Self::zeros(self.cols, self.rows);
        for row in 0..self.rows {
            for col in 0..self.cols {
                result.data[col * self.rows + row] = self.data[row * self.cols + col];
            }
        }
        result
    }

    /// Matrix–vector product.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the dimensions do not agree.
    pub fn mul_vector(&self, vector: &Vector) -> SpiceResult<Vector> {
        if vector.len() != self.cols {
            return Err(length_error(self.cols, vector.len()));
        }
        let mut result = Vector::zeros(self.rows);
        for row in 0..self.rows {
            let mut sum = 0.0;
            for (col, value) in vector.as_slice().iter().enumerate() {
                sum += self.data[row * self.cols + col] * value;
            }
            result.as_mut_slice()[row] = sum;
        }
        Ok(result)
    }

    /// Factors the matrix in place, so that [`Matrix::solve`] can be called.
    ///
    /// # Errors
    ///
    /// Always [`SpiceError::NotYetPorted`].
    pub fn lu_decompose(&mut self) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "dense LU decomposition",
            C_REFERENCE_DENSE,
        ))
    }

    /// Solves `A x = rhs` for a pre-factored matrix.
    ///
    /// # Errors
    ///
    /// Always [`SpiceError::NotYetPorted`].
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        if rhs.len() != self.rows {
            return Err(length_error(self.rows, rhs.len()));
        }
        Err(SpiceError::not_yet_ported("dense solve", C_REFERENCE_DENSE))
    }
}

/// A dense real vector, used for the MNA right-hand side and the solution.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    data: Vec<Real>,
}

impl Vector {
    /// A zeroed vector of `len` elements.
    #[must_use]
    pub fn zeros(len: usize) -> Self {
        Self {
            data: vec![0.0; len],
        }
    }

    /// A vector with the given elements.
    #[must_use]
    pub fn from_slice(values: &[Real]) -> Self {
        Self {
            data: values.to_vec(),
        }
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when the vector has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Reads an element, or `None` when out of range.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Real> {
        self.data.get(index).copied()
    }

    /// Writes an element.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the index is out of range.
    pub fn set(&mut self, index: usize, value: Real) -> SpiceResult<()> {
        match self.data.get_mut(index) {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err(SpiceError::Numerical {
                context: "dense vector".to_owned(),
                message: format!(
                    "index {index} is out of range for length {}",
                    self.data.len()
                ),
            }),
        }
    }

    /// Adds to an element.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the index is out of range.
    pub fn add_to(&mut self, index: usize, value: Real) -> SpiceResult<()> {
        match self.data.get_mut(index) {
            Some(slot) => {
                *slot += value;
                Ok(())
            }
            None => Err(SpiceError::Numerical {
                context: "dense vector".to_owned(),
                message: format!(
                    "index {index} is out of range for length {}",
                    self.data.len()
                ),
            }),
        }
    }

    /// The elements.
    #[must_use]
    pub fn as_slice(&self) -> &[Real] {
        &self.data
    }

    /// The elements, mutable.
    pub fn as_mut_slice(&mut self) -> &mut [Real] {
        &mut self.data
    }

    /// Sets every element to zero.
    pub fn clear(&mut self) {
        self.data.fill(0.0);
    }

    /// Dot product.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] when the lengths differ.
    pub fn dot(&self, other: &Self) -> SpiceResult<Real> {
        if self.len() != other.len() {
            return Err(length_error(self.len(), other.len()));
        }
        Ok(self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .sum())
    }

    /// Largest absolute element.
    #[must_use]
    pub fn max_abs(&self) -> Real {
        self.data
            .iter()
            .fold(0.0, |acc, value| acc.max(value.abs()))
    }

    /// True when every element is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.data.iter().all(|value| value.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::{Matrix, Vector};

    #[test]
    fn matrix_indexing_is_row_major() {
        let mut matrix = Matrix::zeros(2, 3);
        matrix.set(1, 2, 7.0).unwrap();
        assert_eq!(matrix.get(1, 2), Some(7.0));
        assert_eq!(matrix.get(2, 2), None);
        assert_eq!(matrix.data(), &[0.0, 0.0, 0.0, 0.0, 0.0, 7.0]);
        assert_eq!(matrix.row(1), Some(&[0.0, 0.0, 7.0][..]));
        assert_eq!(matrix.len(), 6);
        assert!(!matrix.is_square());
    }

    #[test]
    fn stamping_accumulates() {
        let mut matrix = Matrix::zeros(1, 1);
        matrix.add_to(0, 0, 0.5).unwrap();
        matrix.add_to(0, 0, 0.25).unwrap();
        assert_eq!(matrix.get(0, 0), Some(0.75));
    }

    #[test]
    fn out_of_range_writes_are_errors() {
        let mut matrix = Matrix::zeros(1, 1);
        assert!(matrix.set(0, 1, 1.0).is_err());
        assert!(matrix.add_to(1, 0, 1.0).is_err());
        let mut vector = Vector::zeros(1);
        assert!(vector.set(1, 1.0).is_err());
    }

    #[test]
    fn transpose_moves_elements() {
        let mut matrix = Matrix::zeros(2, 3);
        for row in 0..2 {
            for col in 0..3 {
                matrix.set(row, col, (row * 3 + col) as f64).unwrap();
            }
        }
        let transposed = matrix.transpose();
        assert_eq!((transposed.rows(), transposed.cols()), (3, 2));
        assert_eq!(transposed.get(2, 1), Some(5.0));
    }

    #[test]
    fn multiply_and_dot_agree_on_an_identity() {
        let mut identity = Matrix::zeros(3, 3);
        for index in 0..3 {
            identity.set(index, index, 1.0).unwrap();
        }
        let vector = Vector::from_slice(&[1.0, 2.0, 3.0]);
        let product = identity.mul_vector(&vector).unwrap();
        assert_eq!(product, vector);
        assert_eq!(product.dot(&vector).unwrap(), 14.0);
        assert_eq!(product.max_abs(), 3.0);
        assert!(product.is_finite());
    }

    #[test]
    fn dimension_mismatches_are_errors() {
        let matrix = Matrix::zeros(2, 3);
        assert!(matrix.mul_vector(&Vector::zeros(2)).is_err());
        assert!(matrix.solve(&Vector::zeros(3)).is_err());
        assert!(Vector::zeros(2).dot(&Vector::zeros(3)).is_err());
    }

    #[test]
    fn the_solver_reports_that_it_is_missing() {
        let mut matrix = Matrix::zeros(1, 1);
        matrix.set(0, 0, 1.0).unwrap();
        let error = matrix.solve(&Vector::from_slice(&[1.0])).unwrap_err();
        assert!(error.is_not_yet_ported());
        assert!(error.to_string().contains("src/maths/dense"));
        assert!(matrix.lu_decompose().unwrap_err().is_not_yet_ported());
    }
}
