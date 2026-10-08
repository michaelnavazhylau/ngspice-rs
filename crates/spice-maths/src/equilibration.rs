//! Explicit, bounded row/column equilibration around the existing checked LU APIs.
//!
//! Sparse/complex factors retain the existing numerical rank guard and its
//! aggregate-certification proof caveat (`docs/port/SPARSE_RANK_DIAGNOSTICS.md`).
//! Scaling does not supply a new uniqueness theorem or forward-error guarantee.
//!
//! For `M x = b`, factors solve `(R M C) y = R b` and return `x = C y`.
//! Original assembled snapshots check residuals in physical units. This is a
//! maths-library opt-in, not a new default or a device/driver scaling policy.
//! `src/maths/sparse/sputils.c::spScale` is the read-only behavioral reference
//! for row/RHS and column/solution transforms; the bounded max-based selection
//! here is an independent policy, not a copied solver or C-option parity claim.

use crate::{
    DenseLu, Matrix, SparseLu, SparseMatrix, SparseSymbolic, Vector,
    complex::{ComplexLu, ComplexMatrix},
    linear::{numerical, square},
};
use spice_core::{Complex, SpiceResult};

/// Largest absolute exponent of any row or column factor (`2^±512`).
pub const MAX_SCALING_EXPONENT: i32 = 512;

type Entry = (usize, usize, Complex);

/// Immutable positive power-of-two factors for `R M C`.
///
/// A single row-max pass followed by a column-max pass uses the largest absolute
/// real/imaginary component, not a condition-number estimate. Factors are clamped
/// independently to `2^-512 ..= 2^512`. No permutation or sparsity change occurs.
#[derive(Debug, Clone, PartialEq)]
pub struct Equilibration {
    rows: Vec<f64>,
    columns: Vec<f64>,
}

impl Equilibration {
    /// Row factors multiplying the matrix and RHS.
    #[must_use]
    pub fn row_factors(&self) -> &[f64] {
        &self.rows
    }

    /// Column factors multiplying the matrix and the scaled solution.
    #[must_use]
    pub fn column_factors(&self) -> &[f64] {
        &self.columns
    }

    fn new(n: usize, entries: &[Entry]) -> SpiceResult<Self> {
        let mut rows = vec![0.0_f64; n];
        for &(r, _, v) in entries {
            rows[r] = rows[r].max(component_max(v));
        }
        let rows = factors(&rows, "row")?;
        let mut columns = vec![0.0_f64; n];
        for &(r, c, v) in entries {
            columns[c] = columns[c].max(component_max(scale_complex(v, rows[r])?));
        }
        Ok(Self {
            rows,
            columns: factors(&columns, "column")?,
        })
    }

    fn scale_entries(&self, entries: &[Entry]) -> SpiceResult<Vec<Entry>> {
        entries
            .iter()
            .map(|&(r, c, v)| {
                Ok((
                    r,
                    c,
                    scale_complex(scale_complex(v, self.rows[r])?, self.columns[c])?,
                ))
            })
            .collect()
    }

    fn transform_real(&self, values: &Vector, factors: &[f64]) -> SpiceResult<Vector> {
        if values.len() != factors.len() {
            return Err(numerical(
                "equilibration transform",
                "vector dimension mismatch",
            ));
        }
        Ok(Vector::from_slice(
            &values
                .as_slice()
                .iter()
                .zip(factors)
                .map(|(&v, &s)| scale(v, s))
                .collect::<SpiceResult<Vec<_>>>()?,
        ))
    }

    fn transform_complex(&self, values: &[Complex], factors: &[f64]) -> SpiceResult<Vec<Complex>> {
        if values.len() != factors.len() {
            return Err(numerical(
                "equilibration transform",
                "vector dimension mismatch",
            ));
        }
        values
            .iter()
            .zip(factors)
            .map(|(&v, &s)| scale_complex(v, s))
            .collect()
    }
}

fn component_max(v: Complex) -> f64 {
    v.re.abs().max(v.im.abs())
}

fn factors(maxima: &[f64], namespace: &str) -> SpiceResult<Vec<f64>> {
    maxima
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            if v == 0.0 || !v.is_finite() {
                return Err(numerical(
                    "equilibration",
                    format!("all-zero/non-finite {namespace} {i}"),
                ));
            }
            // Extract floor(log2(v)) exactly, including subnormal inputs; no libm
            // rounding near powers of two can alter the chosen factors.
            let bits = v.to_bits();
            let biased = ((bits >> 52) & 0x7ff) as i32;
            let exponent = if biased == 0 {
                63 - (bits.leading_zeros() as i32) - 1074
            } else {
                biased - 1023
            };
            Ok(2.0_f64.powi((-exponent).clamp(-MAX_SCALING_EXPONENT, MAX_SCALING_EXPONENT)))
        })
        .collect()
}

fn scale(v: f64, s: f64) -> SpiceResult<f64> {
    let out = v * s;
    if !v.is_finite() || !out.is_finite() || (v != 0.0 && (out == 0.0 || out.is_subnormal())) {
        return Err(numerical(
            "equilibration transform",
            "non-finite value, overflow or destructive underflow",
        ));
    }
    Ok(out)
}

fn scale_complex(v: Complex, s: f64) -> SpiceResult<Complex> {
    Ok(Complex::new(scale(v.re, s)?, scale(v.im, s)?))
}

fn assembled(matrix: &SparseMatrix) -> SpiceResult<SparseMatrix> {
    square(matrix.rows(), matrix.cols())?;
    if matrix.triplets().iter().any(|t| !t.value.is_finite()) {
        return Err(numerical(
            "equilibration assembly",
            "non-finite coefficient",
        ));
    }
    let mut snapshot = matrix.clone();
    snapshot.fold_duplicates();
    if snapshot.triplets().iter().any(|t| !t.value.is_finite()) {
        return Err(numerical(
            "equilibration assembly",
            "duplicate summation overflow",
        ));
    }
    Ok(snapshot)
}

fn real_entries(matrix: &SparseMatrix) -> Vec<Entry> {
    matrix
        .triplets()
        .iter()
        .map(|t| (t.row, t.col, Complex::real(t.value)))
        .collect()
}

fn dense_entries(matrix: &Matrix) -> SpiceResult<Vec<Entry>> {
    square(matrix.rows(), matrix.cols())?;
    if matrix.data().iter().any(|v| !v.is_finite()) {
        return Err(numerical(
            "equilibration assembly",
            "non-finite coefficient",
        ));
    }
    Ok(matrix
        .data()
        .iter()
        .enumerate()
        .filter(|(_, v)| **v != 0.0)
        .map(|(i, &v)| (i / matrix.cols(), i % matrix.cols(), Complex::real(v)))
        .collect())
}

fn operators(n: usize, entries: &[Entry]) -> SpiceResult<(SparseMatrix, SparseMatrix)> {
    let mut re = SparseMatrix::new(n, n);
    let mut im = SparseMatrix::new(n, n);
    for &(r, c, v) in entries {
        re.add(r, c, v.re)?;
        im.add(r, c, v.im)?;
    }
    Ok((re, im))
}

/// Original-unit row residual magnitudes and unchanged LU backward-error bounds.
///
/// A successful check guarantees each residual is at most its bound, not forward
/// accuracy: `bound[r] = 128 ε n (sum_c |M[r,c]| ||x||inf + |b[r]|)`.
#[derive(Debug, Clone, PartialEq)]
pub struct BackwardError {
    /// `|M x - b|` per row (physical equation units).
    pub row_residuals: Vec<f64>,
    /// Permitted normwise backward error per row, in the same units.
    pub row_bounds: Vec<f64>,
}

fn check_residual(
    n: usize,
    entries: &[Entry],
    rhs: &[Complex],
    x: &[Complex],
) -> SpiceResult<BackwardError> {
    if rhs.len() != n || x.len() != n || rhs.iter().chain(x).any(|v| !v.is_finite()) {
        return Err(numerical(
            "equilibration original residual",
            "invalid RHS/solution dimensions or values",
        ));
    }
    let max_x = x.iter().fold(0.0_f64, |m, v| m.max(v.magnitude()));
    let mut ax = vec![Complex::ZERO; n];
    let mut bounds: Vec<_> = rhs.iter().map(|v| v.magnitude()).collect();
    for &(r, c, v) in entries {
        ax[r] = ax[r] + v * x[c];
        bounds[r] += v.magnitude() * max_x;
    }
    let tol = 128.0 * f64::EPSILON * n as f64;
    let mut residuals = Vec::with_capacity(n);
    for r in 0..n {
        let error = (ax[r] - rhs[r]).magnitude();
        if !error.is_finite()
            || !bounds[r].is_finite()
            || !max_x.is_finite()
            || error > tol * bounds[r]
        {
            return Err(numerical(
                "equilibration original residual",
                format!("backward residual failed at row {r}"),
            ));
        }
        residuals.push(error);
        bounds[r] *= tol;
    }
    Ok(BackwardError {
        row_residuals: residuals,
        row_bounds: bounds,
    })
}

fn as_complex(values: &Vector) -> Vec<Complex> {
    values
        .as_slice()
        .iter()
        .map(|&v| Complex::real(v))
        .collect()
}

/// Owned opt-in dense LU of `R M C` plus original-unit diagnostics.
#[derive(Debug)]
pub struct EquilibratedDenseLu {
    original: Vec<Entry>,
    scaling: Equilibration,
    factor: DenseLu,
}

impl EquilibratedDenseLu {
    /// Equilibrates a finite nonempty square snapshot, retaining dense LU guards.
    /// # Errors
    /// Invalid assembly, zero rows/columns, failed transform or unresolved rank.
    pub fn new(matrix: &Matrix) -> SpiceResult<Self> {
        let original = dense_entries(matrix)?;
        let scaling = Equilibration::new(matrix.rows(), &original)?;
        let mut scaled = Matrix::zeros(matrix.rows(), matrix.cols());
        for (r, c, v) in scaling.scale_entries(&original)? {
            scaled.set(r, c, v.re)?;
        }
        Ok(Self {
            original,
            scaling,
            factor: DenseLu::new(&scaled)?,
        })
    }

    /// Immutable scaling metadata belonging to this factor snapshot.
    pub fn scaling(&self) -> &Equilibration {
        &self.scaling
    }

    /// Solves a new physical-unit RHS; no caller storage is mutated.
    /// # Errors
    /// Invalid RHS, failed transforms, checked solve or original-unit residual.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        let scaled_rhs = self.scaling.transform_real(rhs, &self.scaling.rows)?;
        let y = self.factor.solve(&scaled_rhs)?;
        let x = self.scaling.transform_real(&y, &self.scaling.columns)?;
        self.check_residual(rhs, &x)?;
        Ok(x)
    }

    /// Checks a physical-unit candidate against the original assembled snapshot.
    /// # Errors
    /// Invalid dimensions/values, residual arithmetic overflow or excessive error.
    pub fn check_residual(&self, rhs: &Vector, x: &Vector) -> SpiceResult<BackwardError> {
        check_residual(
            self.scaling.rows.len(),
            &self.original,
            &as_complex(rhs),
            &as_complex(x),
        )
    }
}

/// Owned opt-in sparse LU with exact assembled-pattern symbolic reuse.
#[derive(Debug)]
pub struct EquilibratedSparseLu {
    original: Vec<Entry>,
    scaling: Equilibration,
    factor: SparseLu,
}

impl EquilibratedSparseLu {
    /// Folds/validates original duplicates before scaling and factoring a snapshot.
    /// Reuse is allowed only for the identical assembled pattern, independent of
    /// previous factors' scaling values. Nonzero scaling cannot change the pattern.
    /// # Errors
    /// Invalid assembly, transforms, zero rows/columns, changed pattern or rank.
    pub fn new(matrix: &SparseMatrix, reuse: Option<&SparseSymbolic>) -> SpiceResult<Self> {
        let snapshot = assembled(matrix)?;
        let original = real_entries(&snapshot);
        let scaling = Equilibration::new(snapshot.rows(), &original)?;
        let (scaled, _) = operators(snapshot.rows(), &scaling.scale_entries(&original)?)?;
        Ok(Self {
            original,
            scaling,
            factor: SparseLu::new(&scaled, reuse)?,
        })
    }

    /// Immutable scaling metadata belonging to this factor snapshot.
    pub fn scaling(&self) -> &Equilibration {
        &self.scaling
    }

    /// Reusable symbolic factors for an identical assembled sparse pattern.
    pub fn symbolic(&self) -> &SparseSymbolic {
        self.factor.symbolic()
    }

    /// Solves a new physical-unit RHS against the owned snapshot.
    /// # Errors
    /// Invalid RHS, failed transforms, checked solve or original-unit residual.
    pub fn solve(&self, rhs: &Vector) -> SpiceResult<Vector> {
        let scaled_rhs = self.scaling.transform_real(rhs, &self.scaling.rows)?;
        let y = self.factor.solve(&scaled_rhs)?;
        let x = self.scaling.transform_real(&y, &self.scaling.columns)?;
        self.check_residual(rhs, &x)?;
        Ok(x)
    }

    /// Checks a physical-unit candidate against the original assembled snapshot.
    /// # Errors
    /// Invalid dimensions/values, residual arithmetic overflow or excessive error.
    pub fn check_residual(&self, rhs: &Vector, x: &Vector) -> SpiceResult<BackwardError> {
        check_residual(
            self.scaling.rows.len(),
            &self.original,
            &as_complex(rhs),
            &as_complex(x),
        )
    }
}

/// Owned opt-in complex LU of the assembled `A + j omega E` snapshot.
#[derive(Debug)]
pub struct EquilibratedComplexLu {
    original: Vec<Entry>,
    scaling: Equilibration,
    factor: ComplexLu,
}

impl EquilibratedComplexLu {
    /// Folds original A/E duplicates, assembles frequency, then scales and factors.
    /// Frequency is finite and nonnegative. Assembly underflow is an error too.
    /// # Errors
    /// Invalid dimensions/frequency/coefficients, transforms, zero rows/columns or rank.
    pub fn from_operators(a: &SparseMatrix, e: &SparseMatrix, omega: f64) -> SpiceResult<Self> {
        let a = assembled(a)?;
        let e = assembled(e)?;
        if a.rows() != e.rows() || !omega.is_finite() || omega < 0.0 {
            return Err(numerical(
                "equilibration complex assembly",
                "operator dimensions/frequency invalid",
            ));
        }
        let mut entries = std::collections::BTreeMap::new();
        for t in a.triplets() {
            entries.insert((t.row, t.col), Complex::real(t.value));
        }
        for t in e.triplets() {
            // omega=0 intentionally removes the dynamic part, not underflow.
            let im = if omega == 0.0 {
                0.0
            } else {
                scale(t.value, omega)?
            };
            entries.entry((t.row, t.col)).or_insert(Complex::ZERO).im = im;
        }
        let original: Vec<_> = entries
            .into_iter()
            .filter(|(_, v)| *v != Complex::ZERO)
            .map(|((r, c), v)| (r, c, v))
            .collect();
        let scaling = Equilibration::new(a.rows(), &original)?;
        let (re, im) = operators(a.rows(), &scaling.scale_entries(&original)?)?;
        let factor = ComplexMatrix::from_operators(&re, &im, 1.0)?.factorize()?;
        Ok(Self {
            original,
            scaling,
            factor,
        })
    }

    /// Immutable scaling metadata belonging to this factor snapshot.
    pub fn scaling(&self) -> &Equilibration {
        &self.scaling
    }

    /// Solves a complex physical-unit RHS, preserving voltage/current phases.
    /// # Errors
    /// Invalid RHS, failed transforms, checked solve or original-unit residual.
    pub fn solve(&self, rhs: &[Complex]) -> SpiceResult<Vec<Complex>> {
        let scaled_rhs = self.scaling.transform_complex(rhs, &self.scaling.rows)?;
        let y = self.factor.solve(&scaled_rhs)?;
        let x = self.scaling.transform_complex(&y, &self.scaling.columns)?;
        self.check_residual(rhs, &x)?;
        Ok(x)
    }

    /// Checks a physical-unit candidate against the original complex assembly.
    /// # Errors
    /// Invalid dimensions/values, residual arithmetic overflow or excessive error.
    pub fn check_residual(&self, rhs: &[Complex], x: &[Complex]) -> SpiceResult<BackwardError> {
        check_residual(self.scaling.rows.len(), &self.original, rhs, x)
    }
}
