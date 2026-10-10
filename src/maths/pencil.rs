//! Finite roots of `det(A + s E) = 0` for a real, square, regular matrix
//! pencil: the numerical core of pole-zero analysis.
//!
//! ngspice finds the roots of the circuit determinant with a deflated Muller
//! search (`src/spicelib/analysis/cktpzstr.c`, `src/maths/ni/nipzmeth.c`).
//! This module instead computes them as the finite generalized eigenvalues of
//! the pencil (`docs/port/POLE_ZERO_ADR.md`). It is an independent algorithm,
//! not a translation of the C search.
//!
//! MNA pencils are singular in `E` (resistive nodes, voltage-source and
//! controlled-source rows), so most of their eigenvalues are infinite, and
//! capacitor/voltage-source loops or inductor/current-source cutsets give
//! infinite eigenvalues of index two and more. A QZ decomposition of the whole
//! pencil perturbs such Jordan blocks at infinity into spurious *finite*
//! eigenvalues of size `~1/sqrt(eps)`. [`pencil_roots`] therefore removes the
//! infinite part first with orthogonal rank decisions:
//!
//! 1. Row-compress `E` with its SVD: `U^T E = [E1; 0]` where `E1` has the
//!    `r` numerically nonzero singular directions. The other `k = n - r` rows
//!    of `U^T A` are free of `s`; unless they have full row rank the pencil
//!    is singular (`det == 0` for every `s`), which is an error.
//! 2. Column-compress those rows with their SVD, `[A21 A22] Z = [0 A22']`.
//!    Then `U^T (A + s E) Z = [[A11 + s E11, *], [0, A22']]` is block upper
//!    triangular with a constant nonsingular `A22'`: `k` infinite eigenvalues
//!    are deflated and the `r x r` pencil `(A11, E11)` keeps every finite
//!    root. Repeat from step 1 (a singular `E11` is a higher-index block).
//! 3. Once `E` is nonsingular the remaining roots are the eigenvalues of
//!    `(A, -E)`, computed by faer's QZ (`faer::linalg::gevd::gevd_cplx`, see
//!    [`finite_qz`] for why the complex variant).
//!
//! This is the infinite-eigenvalue part of the classical orthogonal
//! staircase reduction of a pencil (Van Dooren 1979); no block is ever
//! inverted, which matters for MNA pencils whose algebraic blocks routinely
//! have condition numbers of `1e11` and more (a Schur-complement elimination
//! lost several digits on the poles of a BJT amplifier).
//!
//! Every transformation is orthogonal, so the determinant changes only by a
//! nonzero constant factor. Rank decisions use one tolerance
//! [`RANK_TOLERANCE_FACTOR`]` * n * eps * max(||A||_F, ||E||_F)` after exact
//! power-of-two row/column (Ruiz) and frequency scaling; the frequency scale
//! is undone on the returned roots.

use faer::Mat;

use crate::maths::Matrix;
use crate::primitives::{Complex, Real, SpiceError, SpiceResult};

/// Largest pencil dimension accepted. The reduction is dense (`O(n^3)` SVDs
/// and QZ); larger circuits are rejected explicitly rather than run for an
/// unbounded time.
pub const MAX_PENCIL_DIMENSION: usize = 1000;

/// Multiplier of `n * eps * max(||A||_F, ||E||_F)` in every rank decision.
pub const RANK_TOLERANCE_FACTOR: Real = 16.0;

/// Ruiz scaling sweeps (each also rebalances the frequency scale).
const SCALING_SWEEPS: usize = 8;

/// The finite roots of a pencil, see [`pencil_roots`].
#[derive(Debug, Clone, PartialEq)]
pub struct PencilRoots {
    /// Finite roots in ascending order of real part, then of `|imag|`. A
    /// complex root is followed immediately by its conjugate, the positive
    /// imaginary part first (the order `PZpost` in C's `pzan.c` writes).
    pub roots: Vec<Complex>,
    /// How many of the `n` generalized eigenvalues are infinite (`n` minus
    /// the number of roots).
    pub infinite: usize,
    /// The power-of-two frequency scale `omega` applied before the reduction
    /// (roots were computed for `s / omega` and multiplied back).
    pub frequency_scale: Real,
}

fn error(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "pole-zero pencil".to_owned(),
        message: message.into(),
    }
}

/// The nearest power of two to a positive finite `x`.
fn power_of_two(x: Real) -> Real {
    let exponent = x.log2().round().clamp(-1000.0, 1000.0);
    // Exact: integer exponents in this range are representable.
    2_f64.powi(exponent as i32)
}

fn frobenius(m: &Mat<Real>) -> Real {
    let mut sum = 0.0;
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            sum += m[(i, j)] * m[(i, j)];
        }
    }
    sum.sqrt()
}

/// Exact power-of-two scaling: Ruiz row/column equilibration of `[A E]`
/// interleaved with a frequency scale `omega` that balances `||A||` and
/// `||omega E||`. Returns `omega`; the roots of the scaled pencil are
/// `s / omega`.
fn balance(a: &mut Mat<Real>, e: &mut Mat<Real>) -> Real {
    let n = a.nrows();
    let mut omega = 1.0;
    for _ in 0..SCALING_SWEEPS {
        let (na, ne) = (frobenius(a), frobenius(e));
        if na > 0.0 && ne > 0.0 {
            let w = power_of_two(na / ne);
            if w != 1.0 {
                for j in 0..n {
                    for i in 0..n {
                        e[(i, j)] *= w;
                    }
                }
                omega *= w;
            }
        }
        for i in 0..n {
            let largest = (0..n).fold(0.0_f64, |m, j| m.max(a[(i, j)].abs()).max(e[(i, j)].abs()));
            if largest > 0.0 {
                let f = power_of_two(1.0 / largest.sqrt());
                for j in 0..n {
                    a[(i, j)] *= f;
                    e[(i, j)] *= f;
                }
            }
        }
        for j in 0..n {
            let largest = (0..n).fold(0.0_f64, |m, i| m.max(a[(i, j)].abs()).max(e[(i, j)].abs()));
            if largest > 0.0 {
                let f = power_of_two(1.0 / largest.sqrt());
                for i in 0..n {
                    a[(i, j)] *= f;
                    e[(i, j)] *= f;
                }
            }
        }
    }
    omega
}

/// A full SVD with singular values in nonincreasing order: `(U, sigma, V)`
/// with `M = U diag(sigma) V^T` (`U` is `rows x rows`, `V` is `cols x cols`).
fn ordered_svd(m: &Mat<Real>) -> SpiceResult<(Mat<Real>, Vec<Real>, Mat<Real>)> {
    let (rows, cols) = (m.nrows(), m.ncols());
    if rows == 0 || cols == 0 {
        return Ok((
            Mat::<Real>::identity(rows, rows),
            Vec::new(),
            Mat::<Real>::identity(cols, cols),
        ));
    }
    let svd = m
        .svd()
        .map_err(|e| error(format!("SVD did not converge: {e:?}")))?;
    let k = rows.min(cols);
    let s = svd.S().column_vector();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&x, &y| s[y].total_cmp(&s[x]));
    let sigma: Vec<Real> = order.iter().map(|&i| s[i]).collect();
    // Permute the leading `k` singular vectors; the complementary columns
    // (null-space bases) keep their positions.
    let permute = |q: faer::MatRef<'_, Real>| {
        Mat::<Real>::from_fn(q.nrows(), q.ncols(), |i, j| {
            if j < k { q[(i, order[j])] } else { q[(i, j)] }
        })
    };
    let (u, v) = (permute(svd.U()), permute(svd.V()));
    if sigma.iter().any(|x| !x.is_finite()) || u.has_nan() || v.has_nan() {
        return Err(error("SVD produced non-finite values"));
    }
    Ok((u, sigma, v))
}

fn rank(sigma: &[Real], tolerance: Real) -> usize {
    sigma.iter().take_while(|&&x| x > tolerance).count()
}

fn block(m: &Mat<Real>, rows: std::ops::Range<usize>, cols: std::ops::Range<usize>) -> Mat<Real> {
    m.as_ref()
        .subrows(rows.start, rows.len())
        .subcols(cols.start, cols.len())
        .to_owned()
}

/// The finite roots of `det(A + s E) = 0`.
///
/// `A` and `E` are real, square and of equal dimension. An empty pencil has
/// no roots. See the [module documentation](self) for the method.
///
/// # Errors
///
/// [`SpiceError::Numerical`] for mismatched or non-square dimensions,
/// non-finite entries, a dimension above [`MAX_PENCIL_DIMENSION`], a singular
/// pencil (`det(A + s E)` vanishes identically, so its roots are undefined) or
/// a failed SVD/QZ decomposition.
pub fn pencil_roots(a: &Matrix, e: &Matrix) -> SpiceResult<PencilRoots> {
    let n = a.rows();
    if a.cols() != n || e.rows() != n || e.cols() != n {
        return Err(error(format!(
            "A is {}x{} and E is {}x{}; both must be square and equal",
            a.rows(),
            a.cols(),
            e.rows(),
            e.cols()
        )));
    }
    if n > MAX_PENCIL_DIMENSION {
        return Err(error(format!(
            "dimension {n} exceeds the dense pole-zero limit {MAX_PENCIL_DIMENSION}"
        )));
    }
    if a.data().iter().chain(e.data()).any(|x| !x.is_finite()) {
        return Err(error("non-finite matrix entry"));
    }
    let mut am = Mat::<Real>::from_fn(n, n, |i, j| a.data()[i * n + j]);
    let mut em = Mat::<Real>::from_fn(n, n, |i, j| e.data()[i * n + j]);
    let omega = balance(&mut am, &mut em);
    let norm = frobenius(&am).max(frobenius(&em));
    if n > 0 && norm == 0.0 {
        return Err(error("singular pencil: A and E are both zero"));
    }
    let tolerance = RANK_TOLERANCE_FACTOR * (n.max(1) as Real) * Real::EPSILON * norm;
    let scaled = reduce(am, em, tolerance)?;
    let mut roots: Vec<Complex> = scaled
        .into_iter()
        .map(|z| Complex::new(z.re * omega, z.im * omega))
        .collect();
    if roots.iter().any(|z| !z.is_finite()) {
        return Err(error("non-finite root"));
    }
    roots = order_roots(&roots)?;
    Ok(PencilRoots {
        infinite: n - roots.len(),
        roots,
        frequency_scale: omega,
    })
}

/// Deflates the infinite eigenvalues and returns the finite roots of the
/// scaled pencil, unordered. Orthogonal transformations only: no block of
/// the pencil is ever inverted.
fn reduce(mut am: Mat<Real>, mut em: Mat<Real>, tolerance: Real) -> SpiceResult<Vec<Complex>> {
    loop {
        let n = am.nrows();
        if n == 0 {
            return Ok(Vec::new());
        }
        let (u, sigma, _) = ordered_svd(&em)?;
        let r = rank(&sigma, tolerance);
        if r == n {
            return finite_qz(&am, &em);
        }
        // Row compression: U^T E = [E1; ~0], whose last n - r rows are
        // below the tolerance and are dropped as exact zeros.
        let am_rows = u.transpose() * &am;
        let em_rows = u.transpose() * &em;
        let k = n - r;
        // Those rows of U^T A must have full row rank: a combination of
        // equations free of `s` that also vanishes in `A` makes the
        // determinant identically zero.
        let algebraic = block(&am_rows, r..n, 0..n);
        let (_, s2, w) = ordered_svd(&algebraic)?;
        if rank(&s2, tolerance) < k {
            return Err(singular(k - rank(&s2, tolerance), n));
        }
        // Column compression of the algebraic rows: Z = [null, range], so
        // U^T (A + s E) Z = [[A11 + s E11, *], [0, A22]] with A22 constant
        // and nonsingular: k infinite eigenvalues are deflated.
        let z = Mat::<Real>::from_fn(
            n,
            n,
            |i, j| {
                if j < r { w[(i, k + j)] } else { w[(i, j - r)] }
            },
        );
        am = block(&(&am_rows * &z), 0..r, 0..r);
        em = block(&(&em_rows * &z), 0..r, 0..r);
    }
}

fn singular(deficiency: usize, n: usize) -> SpiceError {
    error(format!(
        "singular pencil: det(A + sE) vanishes for every s ({deficiency} algebraic row \
         combination(s) of the {n}-dimensional pencil are rank deficient)"
    ))
}

/// The eigenvalues of `(A, -E)` for a numerically nonsingular `E`, from
/// faer's complex QZ without eigenvectors.
///
/// Two faer 0.24.4 paths are avoided deliberately: the high-level
/// `Mat::generalized_eigen` also computes eigenvectors and under-sizes that
/// workspace for small pencils (it panics), and the real QZ
/// (`gevd_real`) returns visibly wrong eigenvalues for complex-conjugate
/// 2x2 blocks (a series RLC pole pair off by 4 %, not conjugate). The real
/// pencil is therefore decomposed in complex arithmetic and the conjugate
/// pairs are restored by [`order_roots`].
fn finite_qz(am: &Mat<Real>, em: &Mat<Real>) -> SpiceResult<Vec<Complex>> {
    use faer::c64;
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::evd::ComputeEigenvectors;
    let n = am.nrows();
    let mut a = Mat::<c64>::from_fn(n, n, |i, j| c64::new(am[(i, j)], 0.0));
    let mut b = Mat::<c64>::from_fn(n, n, |i, j| c64::new(-em[(i, j)], 0.0));
    let mut alpha = faer::diag::Diag::<c64>::zeros(n);
    let mut beta = faer::diag::Diag::<c64>::zeros(n);
    let par = faer::Par::Seq;
    let mut buffer = MemBuffer::new(faer::linalg::gevd::gevd_scratch::<c64>(
        n,
        ComputeEigenvectors::No,
        ComputeEigenvectors::No,
        par,
        Default::default(),
    ));
    faer::linalg::gevd::gevd_cplx(
        a.as_mut(),
        b.as_mut(),
        alpha.as_mut(),
        beta.as_mut(),
        None,
        None,
        par,
        MemStack::new(&mut buffer),
        Default::default(),
    )
    .map_err(|e| error(format!("QZ did not converge: {e:?}")))?;
    let (alpha, beta) = (alpha.column_vector(), beta.column_vector());
    let mut roots = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (alpha[i], beta[i]);
        let denominator = b.re * b.re + b.im * b.im;
        if denominator == 0.0 || !denominator.is_finite() {
            return Err(error(
                "QZ returned an infinite eigenvalue after the infinite part was deflated",
            ));
        }
        roots.push(Complex::new(
            (a.re * b.re + a.im * b.im) / denominator,
            (a.im * b.re - a.re * b.im) / denominator,
        ));
    }
    Ok(roots)
}

/// Largest pairing distance `|z_i - conj(z_j)|` accepted when the computed
/// roots are made conjugate-symmetric, relative to the largest root
/// magnitude. Backward-stable QZ keeps well-conditioned roots of a real
/// pencil within rounding of their conjugates; a multiple root splits by
/// about `sqrt(eps)` of the scale. A larger asymmetry is reported instead of
/// being symmetrised away.
pub const CONJUGATE_PAIR_TOLERANCE: Real = 1e-6;

/// Restores the conjugate symmetry of a real pencil's spectrum and sorts the
/// roots as C's `PZpost` lists them: ascending real part, then ascending
/// imaginary part of the upper-half-plane root, each complex root followed
/// by its conjugate (positive imaginary part first).
///
/// The complex QZ treats conjugate roots independently, so the computed set
/// is symmetric only to rounding. Roots are paired greedily by the smallest
/// `|z_i - conj(z_j)|` (pairing a root with itself, at cost `2 |imag|`, makes
/// it real); a pair becomes `m, conj(m)` with `m = (z_i + conj(z_j)) / 2`.
/// No tolerance decides what is real: a perturbed double real root may come
/// back as two close real roots or as a narrow complex pair, as close to the
/// exact pair as the computation can tell.
fn order_roots(roots: &[Complex]) -> SpiceResult<Vec<Complex>> {
    let n = roots.len();
    let mut candidates: Vec<(Real, usize, usize)> = Vec::with_capacity(n * (n + 1) / 2);
    for i in 0..n {
        for j in i..n {
            let (z, w) = (roots[i], roots[j]);
            candidates.push(((z.re - w.re).hypot(z.im + w.im), i, j));
        }
    }
    candidates.sort_by(|x, y| x.0.total_cmp(&y.0));
    let scale = roots.iter().fold(0.0_f64, |m, z| m.max(z.re.hypot(z.im)));
    let mut used = vec![false; n];
    let mut representatives = Vec::with_capacity(n);
    for (cost, i, j) in candidates {
        if used[i] || used[j] {
            continue;
        }
        if cost > CONJUGATE_PAIR_TOLERANCE * scale {
            return Err(error(format!(
                "computed roots are not conjugate-symmetric (pairing distance {cost:e} at \
                 root magnitude {scale:e})"
            )));
        }
        used[i] = true;
        used[j] = true;
        let (z, w) = (roots[i], roots[j]);
        if i == j {
            representatives.push(Complex::new(z.re, 0.0));
        } else {
            let m = Complex::new(0.5 * (z.re + w.re), 0.5 * (z.im - w.im).abs());
            if m.im == 0.0 {
                representatives.push(m);
                representatives.push(m);
            } else {
                representatives.push(m);
            }
        }
    }
    representatives.sort_by(|x, y| x.re.total_cmp(&y.re).then(x.im.total_cmp(&y.im)));
    let mut ordered = Vec::with_capacity(n);
    for z in representatives {
        ordered.push(z);
        if z.im > 0.0 {
            ordered.push(Complex::new(z.re, -z.im));
        }
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(n: usize, values: &[Real]) -> Matrix {
        let mut m = Matrix::zeros(n, n);
        m.data_mut().copy_from_slice(values);
        m
    }

    fn close(z: Complex, re: Real, im: Real) -> bool {
        let scale = re.abs().max(im.abs()).max(1.0);
        (z.re - re).abs() <= 1e-10 * scale && (z.im - im).abs() <= 1e-10 * scale
    }

    #[test]
    fn diagonal_pencils_give_their_ratios() {
        // (1 + s) (4 + 2 s): roots -1 and -2.
        let roots =
            pencil_roots(&matrix(2, &[1., 0., 0., 4.]), &matrix(2, &[1., 0., 0., 2.])).unwrap();
        assert_eq!(roots.roots.len(), 2);
        assert!(close(roots.roots[0], -2., 0.), "{roots:?}");
        assert!(close(roots.roots[1], -1., 0.), "{roots:?}");
        assert_eq!(roots.infinite, 0);
    }

    #[test]
    fn complex_pairs_are_conjugate_and_positive_first() {
        // s^2 + 2 s + 5 as a companion pencil: roots -1 +- 2j.
        let a = matrix(2, &[0., -1., 5., 2.]);
        let e = matrix(2, &[1., 0., 0., 1.]);
        let roots = pencil_roots(&a, &e).unwrap().roots;
        assert_eq!(roots.len(), 2);
        assert!(close(roots[0], -1., 2.), "{roots:?}");
        assert_eq!(roots[1], Complex::new(roots[0].re, -roots[0].im));
    }

    #[test]
    fn index_one_infinite_eigenvalues_are_deflated() {
        // RC node with a resistive neighbour: E has rank 1.
        let a = matrix(2, &[2e-3, -1e-3, -1e-3, 1e-3]);
        let e = matrix(2, &[0., 0., 0., 1e-6]);
        let roots = pencil_roots(&a, &e).unwrap();
        // det = 2e-3 (1e-3 + 1e-6 s) - 1e-6 = 1e-6 + 2e-9 s => s = -500.
        assert_eq!(roots.roots.len(), 1);
        assert!(close(roots.roots[0], -500., 0.), "{roots:?}");
        assert_eq!(roots.infinite, 1);
    }

    #[test]
    fn index_two_blocks_do_not_leak_spurious_roots() {
        // A capacitor directly across an ideal voltage source (a CV loop),
        // plus an RC section: x = [v1, v2, i].
        //   KCL v1: C1 s v1 + g (v1 - v2) + i = 0
        //   KCL v2: C2 s v2 + g (v2 - v1) + g v2 = 0
        //   source: v1 = 0
        let (g, c1, c2) = (1e-3, 1e-6, 2e-6);
        let a = matrix(3, &[g, -g, 1., -g, 2. * g, 0., 1., 0., 0.]);
        let e = matrix(3, &[c1, 0., 0., 0., c2, 0., 0., 0., 0.]);
        let roots = pencil_roots(&a, &e).unwrap();
        assert_eq!(roots.roots.len(), 1, "{roots:?}");
        assert!(close(roots.roots[0], -2. * g / c2, 0.), "{roots:?}");
        assert_eq!(roots.infinite, 2);
    }

    #[test]
    fn singular_pencils_and_bad_input_are_errors() {
        // A floating node: a zero row and column in both A and E.
        let a = matrix(2, &[1., 0., 0., 0.]);
        let e = matrix(2, &[1., 0., 0., 0.]);
        let error = pencil_roots(&a, &e).unwrap_err();
        assert!(error.to_string().contains("singular pencil"), "{error}");
        let error = pencil_roots(&Matrix::zeros(2, 2), &Matrix::zeros(3, 3)).unwrap_err();
        assert!(error.to_string().contains("square"), "{error}");
        let error = pencil_roots(&matrix(1, &[Real::NAN]), &matrix(1, &[1.])).unwrap_err();
        assert!(error.to_string().contains("non-finite"), "{error}");
        assert!(
            pencil_roots(&Matrix::zeros(0, 0), &Matrix::zeros(0, 0))
                .unwrap()
                .roots
                .is_empty()
        );
    }

    #[test]
    fn purely_algebraic_regular_pencils_have_no_roots() {
        let roots = pencil_roots(&matrix(2, &[1., 2., 3., 4.]), &Matrix::zeros(2, 2)).unwrap();
        assert!(roots.roots.is_empty());
        assert_eq!(roots.infinite, 2);
    }

    #[test]
    fn widely_scaled_pencils_keep_their_roots() {
        // Poles at -1e3 and -1e9 rad/s from 1 nF/1 uF and kohm/ohm values.
        let a = matrix(2, &[1e-3, 0., 0., 1.]);
        let e = matrix(2, &[1e-6, 0., 0., 1e-9]);
        let roots = pencil_roots(&a, &e).unwrap().roots;
        assert!(close(roots[0], -1e9, 0.), "{roots:?}");
        assert!(close(roots[1], -1e3, 0.), "{roots:?}");
    }
}
