//! Complex sparse LU for AC. Scalar conversion happens only at this boundary.
use crate::maths::{
    SparseMatrix,
    linear::{Factor, Key, SparseSymbolic, numerical, square},
};
use crate::primitives::{Complex, SpiceResult};
use faer::c64;

/// Complex assembled sparse matrix, independent of real stamping storage.
#[derive(Debug, Clone)]
pub struct ComplexMatrix {
    n: usize,
    entries: Vec<(usize, usize, Complex)>,
}
impl ComplexMatrix {
    /// Assembles `A + j omega E`, preserving cancellations and checking overflow.
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
        })
    }
    /// Owned faer complex LU (fresh symbolic analysis). As in
    /// [`crate::maths::linear`], there is no rank probe or residual check:
    /// structural singularity and non-finite solutions are the errors.
    /// # Errors
    /// Structural singularity.
    pub fn factorize(&self) -> SpiceResult<ComplexLu> {
        let positions: Vec<_> = self.entries.iter().map(|&(r, c, _)| (r, c)).collect();
        let symbolic = SparseSymbolic::analyse(
            self.n,
            Key::Triplets(std::sync::Arc::new(positions.clone())),
            &positions,
        )?;
        let mut factor = Factor::new(symbolic, c64::new(0., 0.))?;
        self.load(&mut factor)?;
        let result = ComplexLu { n: self.n, factor };
        // One fixed-RHS solve reports an exactly zero pivot here (see
        // `SparseLu::new`); refactorizations in a sweep rely on their own solve.
        result.solve(&vec![Complex::real(1.); self.n])?;
        Ok(result)
    }

    /// Refactors numerically with `previous`'s faer symbolic analysis and
    /// storage when the pattern is identical (frequency sweeps); otherwise
    /// analyses afresh. faer re-chooses row pivots in every numeric phase.
    /// # Errors
    /// Structural singularity.
    pub fn factorize_reusing(&self, mut previous: ComplexLu) -> SpiceResult<ComplexLu> {
        if previous.n != self.n
            || !previous
                .factor
                .symbolic
                .matches_positions(self.entries.iter().map(|&(r, c, _)| (r, c)))
        {
            return self.factorize();
        }
        self.load(&mut previous.factor)?;
        Ok(previous)
    }

    fn load(&self, factor: &mut Factor<c64>) -> SpiceResult<()> {
        factor.factor(
            "complex sparse LU",
            self.entries.iter().map(|(_, _, v)| c64::new(v.re, v.im)),
        )
    }
}
/// Owned complex factors; subsequent real operator mutations cannot stale them.
#[derive(Debug)]
pub struct ComplexLu {
    n: usize,
    factor: Factor<c64>,
}
impl ComplexLu {
    /// Solves `A x = b`.
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
        let mut x: Vec<c64> = rhs.iter().map(|v| c64::new(v.re, v.im)).collect();
        self.factor.solve_in_place(&mut x, transposed);
        let x: Vec<Complex> = x.iter().map(|v| Complex::new(v.re, v.im)).collect();
        if x.iter().any(|v| !v.is_finite()) {
            return Err(numerical(
                "complex solve",
                "non-finite solution/singular system",
            ));
        }
        Ok(x)
    }
}
