//! `solve_ni` reuses the factors of a bound matrix whose values, `diag_gmin`
//! and pivot thresholds equal those it last refactored (SPARSE's `spFactor`
//! of identical inputs with the same order recomputes them bit for bit).
//! These tests pin the reuse to C's refactorization and every invalidation:
//! a changed value, a changed `diag_gmin`, a requested reorder and a complex
//! (AC) factorization in the shared frame.
use ngspice_rs::maths::complex::ComplexMatrix;
use ngspice_rs::maths::{NiStep, Pattern, SparseMatrix, Vector, solve_ni};
use ngspice_rs::primitives::Complex;
use std::sync::Arc;

/// A diagonally dominant, nonsymmetric 3x3 pattern (the pivot order stays
/// on the diagonal for every value set below).
const POSITIONS: [(usize, usize); 7] = [(0, 0), (0, 1), (1, 0), (1, 1), (1, 2), (2, 1), (2, 2)];
const A: [f64; 7] = [4.0, -1.25, -0.5, 3.0, -0.75, -1.5, 5.0];
const B: [f64; 7] = [4.5, -1.25, -0.5, 3.0, -0.75, -1.5, 5.0];

fn pattern() -> Arc<Pattern> {
    Arc::new(Pattern::new(3, POSITIONS))
}

fn bound(pattern: &Arc<Pattern>, values: &[f64; 7]) -> SparseMatrix {
    let mut matrix = SparseMatrix::bound(pattern.clone());
    for (&(row, col), &value) in POSITIONS.iter().zip(values) {
        matrix.add(row, col, value).unwrap();
    }
    matrix
}

fn rhs() -> Vector {
    Vector::from_slice(&[1.0, -2.0, 0.5])
}

fn solve(pattern: &Arc<Pattern>, values: &[f64; 7], reorder: bool, gmin: f64) -> Vec<f64> {
    match solve_ni(&bound(pattern, values), &rhs(), &mut None, reorder, gmin).unwrap() {
        NiStep::Solved(x) => x.as_slice().to_vec(),
        NiStep::ZeroPivot => panic!("zero pivot"),
    }
}

/// `(A + gmin I) x - b`, which must vanish for the right factors.
fn residual(values: &[f64; 7], gmin: f64, x: &[f64]) -> f64 {
    let mut r = rhs().as_slice().to_vec();
    for v in &mut r {
        *v = -*v;
    }
    for (&(row, col), &value) in POSITIONS.iter().zip(values) {
        r[row] += value * x[col];
    }
    for (row, value) in r.iter_mut().enumerate() {
        *value += gmin * x[row];
    }
    r.iter().fold(0.0_f64, |m, v| m.max(v.abs()))
}

fn bits(x: &[f64]) -> Vec<u64> {
    x.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn identical_inputs_reuse_factors_equal_to_a_refactorization() {
    // Reused: the second solve of A with the factors of the first
    // refactorization of A.
    let reused = pattern();
    solve(&reused, &B, false, 0.0);
    let first = solve(&reused, &A, false, 0.0);
    let second = solve(&reused, &A, false, 0.0);
    // Refactored: the same order, then A refactored from B's frame.
    let refactored = pattern();
    solve(&refactored, &B, false, 0.0);
    let reference = solve(&refactored, &A, false, 0.0);
    assert_eq!(bits(&first), bits(&reference));
    assert_eq!(bits(&second), bits(&reference));
    assert!(residual(&A, 0.0, &second) < 1e-15);
}

#[test]
fn a_changed_value_refactors() {
    let p = pattern();
    solve(&p, &A, false, 0.0);
    solve(&p, &A, false, 0.0);
    let x = solve(&p, &B, false, 0.0);
    assert!(residual(&B, 0.0, &x) < 1e-15, "{x:?}");
    assert!(residual(&A, 0.0, &x) > 1e-3);
}

#[test]
fn a_changed_diagonal_gmin_refactors() {
    let p = pattern();
    solve(&p, &A, false, 0.0);
    solve(&p, &A, false, 0.0);
    let x = solve(&p, &A, false, 0.25);
    assert!(residual(&A, 0.25, &x) < 1e-15, "{x:?}");
    assert!(residual(&A, 0.0, &x) > 1e-3);
}

#[test]
fn a_requested_reorder_factors_again() {
    let p = pattern();
    solve(&p, &A, false, 0.0);
    let before = solve(&p, &A, false, 0.0);
    let reordered = solve(&p, &A, true, 0.0);
    assert!(residual(&A, 0.0, &reordered) < 1e-15);
    // The order is unchanged here, so so is the solution.
    assert_eq!(bits(&before), bits(&reordered));
}

#[test]
fn a_complex_factorization_in_the_shared_frame_invalidates_the_factors() {
    let p = pattern();
    solve(&p, &A, false, 0.0);
    solve(&p, &A, false, 0.0);
    // An AC factorization of A + j omega E on the same pattern (the
    // circuit's one SPARSE frame) overwrites the frame's factors.
    let a = bound(&p, &A);
    let mut e = SparseMatrix::new(3, 3);
    for row in 0..3 {
        e.add(row, row, 1e-3).unwrap();
    }
    let ac = ComplexMatrix::from_operators_on(&p, &a, &e, 2.0e3)
        .unwrap()
        .expect("stamps inside the pattern");
    let lu = ac.factorize().unwrap();
    let z = lu
        .solve(&[Complex::real(1.0), Complex::real(-2.0), Complex::real(0.5)])
        .unwrap();
    assert!(z.iter().any(|v| v.im != 0.0));
    let x = solve(&p, &A, false, 0.0);
    assert!(residual(&A, 0.0, &x) < 1e-15, "{x:?}");
}
