//! Dense row-major real matrix and vector storage.
//!
//! Mirrors `src/maths/dense/` (`smat.c`), which ngspice uses for the small
//! systems in `.sens`, `.tf` and `.pz`. The MNA matrix itself is sparse — see
//! [`crate::maths::sparse`]. This is a general storage/LU API, not an implementation
//! of `.sens` or `.tf`; `.pz` uses [`crate::maths::pencil`] on dense copies.
//!
//! Factorization returns an owned pivoted LU; public storage stays row-major.

use crate::primitives::{Real, SpiceError, SpiceResult};

use crate::maths::linear::DenseLu;

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

    /// Factors a snapshot without changing row-major storage.
    ///
    /// # Errors
    ///
    /// Invalid dimensions/coefficients or singular pivots. Empty systems are rejected.
    pub fn lu_decompose(&self) -> SpiceResult<DenseLu> {
        DenseLu::new(self)
    }

    /// Convenience solve, factoring the current storage. No prior LU is required.
    ///
    /// # Errors
    ///
    /// Invalid matrix/RHS or failed backward residual.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        self.lu_decompose()?.solve(rhs)
    }

    /// Eigenvalues of a real symmetric matrix, in nondecreasing order, by
    /// cyclic Jacobi rotations (accurate to a few ulps of the largest
    /// eigenvalue). Used for definiteness checks of small coupled-inductance
    /// matrices; it is not a general eigensolver API.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for an empty or non-square matrix, nonfinite
    /// entries, an asymmetric matrix (entries must agree exactly), or a
    /// failed decomposition.
    pub fn symmetric_eigenvalues(&self) -> SpiceResult<Vec<Real>> {
        let error = |message: String| SpiceError::Numerical {
            context: "dense symmetric eigenvalues".to_owned(),
            message,
        };
        if self.rows != self.cols || self.rows == 0 {
            return Err(error(format!(
                "needs a nonempty square matrix, got {}x{}",
                self.rows, self.cols
            )));
        }
        if self.data.iter().any(|value| !value.is_finite()) {
            return Err(error("nonfinite entry".to_owned()));
        }
        let n = self.rows;
        for row in 0..n {
            for col in 0..row {
                if self.data[row * n + col] != self.data[col * n + row] {
                    return Err(error(format!(
                        "entries ({row}, {col}) and ({col}, {row}) differ"
                    )));
                }
            }
        }
        let values = jacobi_eigenvalues(self.data.clone(), n)
            .ok_or_else(|| error("Jacobi rotations did not converge".to_owned()))?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(error("nonfinite eigenvalue".to_owned()));
        }
        Ok(values)
    }
}

/// Cyclic Jacobi eigenvalues of the symmetric row-major `a` (n x n), sorted.
/// `None` if the off-diagonal mass fails to vanish within the sweep budget.
fn jacobi_eigenvalues(mut a: Vec<Real>, n: usize) -> Option<Vec<Real>> {
    let off = |a: &[Real]| -> Real {
        let mut sum = 0.0;
        for row in 0..n {
            for col in 0..n {
                if row != col {
                    sum += a[row * n + col] * a[row * n + col];
                }
            }
        }
        sum
    };
    let total: Real = a.iter().map(|v| v * v).sum();
    for _sweep in 0..100 {
        if off(&a) <= Real::EPSILON * Real::EPSILON * total {
            let mut values: Vec<Real> = (0..n).map(|i| a[i * n + i]).collect();
            values.sort_by(Real::total_cmp);
            return Some(values);
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq == 0.0 {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let akp = a[k * n + p];
                    let akq = a[k * n + q];
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let apk = a[p * n + k];
                    let aqk = a[q * n + k];
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
            }
        }
    }
    None
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
    #[inline(always)]
    pub fn add_to(&mut self, index: usize, value: Real) -> SpiceResult<()> {
        match self.data.get_mut(index) {
            Some(slot) => {
                *slot += value;
                Ok(())
            }
            None => Err(out_of_range(index, self.data.len())),
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

/// [`Vector::add_to`]'s error, out of its hot path.
#[cold]
#[inline(never)]
fn out_of_range(index: usize, len: usize) -> SpiceError {
    SpiceError::Numerical {
        context: "dense vector".to_owned(),
        message: format!("index {index} is out of range for length {len}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Matrix, Vector};

    #[test]
    fn symmetric_eigenvalues_are_sorted_and_validated() {
        let mut matrix = Matrix::zeros(2, 2);
        for (row, col, value) in [(0, 0, 2.0), (0, 1, 1.0), (1, 0, 1.0), (1, 1, 2.0)] {
            matrix.set(row, col, value).unwrap();
        }
        let values = matrix.symmetric_eigenvalues().unwrap();
        assert!((values[0] - 1.0).abs() < 1e-14 && (values[1] - 3.0).abs() < 1e-14);
        // An indefinite matrix has a negative eigenvalue.
        matrix.set(0, 1, 3.0).unwrap();
        matrix.set(1, 0, 3.0).unwrap();
        assert!(matrix.symmetric_eigenvalues().unwrap()[0] < 0.0);
        matrix.set(1, 0, 2.0).unwrap();
        assert!(matrix.symmetric_eigenvalues().is_err());
        assert!(Matrix::zeros(2, 3).symmetric_eigenvalues().is_err());
        assert!(Matrix::zeros(0, 0).symmetric_eigenvalues().is_err());
        let mut nonfinite = Matrix::zeros(1, 1);
        nonfinite.data_mut()[0] = f64::NAN;
        assert!(nonfinite.symmetric_eigenvalues().is_err());
    }

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
    fn solve_without_prior_factorization() {
        let mut matrix = Matrix::zeros(1, 1);
        matrix.set(0, 0, 2.0).unwrap();
        assert_eq!(
            matrix
                .solve(&Vector::from_slice(&[4.0]))
                .unwrap()
                .as_slice(),
            &[2.0]
        );
    }
}
