//! Complex assembly and solver guardrails at the faer scalar boundary.
use ngspice_rs::maths::{SparseMatrix, complex::ComplexMatrix};
use ngspice_rs::primitives::Complex;
#[test]
fn complex_pivoting_snapshot_and_repeated_rhs() {
    let mut a = SparseMatrix::new(2, 2);
    a.add(0, 1, 1.).unwrap();
    a.add(1, 0, 2.).unwrap();
    let mut e = SparseMatrix::new(2, 2);
    e.add(1, 1, 1.).unwrap();
    let lu = ComplexMatrix::from_operators(&a, &e, 1.)
        .unwrap()
        .factorize()
        .unwrap();
    a.clear();
    e.clear();
    for rhs in [
        vec![Complex::real(2.), Complex::new(2., 2.)],
        vec![Complex::ZERO, Complex::real(4.)],
    ] {
        let x = lu.solve(&rhs).unwrap();
        assert!((x[1] - rhs[0]).magnitude() < 1e-14);
        assert!(
            (Complex::real(2.) * x[0] + Complex::imaginary(1.) * x[1] - rhs[1]).magnitude() < 1e-14
        );
    }
    assert!(lu.solve(&[Complex::ZERO]).is_err());
    assert!(lu.solve(&[Complex::real(f64::NAN), Complex::ZERO]).is_err());
}
#[test]
fn complex_assembly_nonfinite_overflow_and_singularity() {
    let mut a = SparseMatrix::new(1, 1);
    let mut e = SparseMatrix::new(1, 1);
    assert!(
        ComplexMatrix::from_operators(&a, &e, 0.)
            .unwrap()
            .factorize()
            .is_err()
    );
    a.add(0, 0, 1.).unwrap();
    e.add(0, 0, f64::MAX).unwrap();
    assert!(ComplexMatrix::from_operators(&a, &e, 2.).is_err());
    assert!(ComplexMatrix::from_operators(&a, &e, f64::NAN).is_err());
    e.clear();
    a.add(0, 0, f64::NAN).unwrap();
    assert!(ComplexMatrix::from_operators(&a, &e, 1.).is_err());
    assert!(
        ComplexMatrix::from_operators(&SparseMatrix::new(0, 0), &SparseMatrix::new(0, 0), 1.)
            .is_err()
    );
    assert!(ComplexMatrix::from_operators(&SparseMatrix::new(1, 2), &e, 1.).is_err());
}
#[test]
fn transposed_solves_use_the_same_factors_without_conjugation() {
    // A = [[1, 2], [0, 3]] + j [[0, 0], [1, 0]]: nonsymmetric and complex,
    // so A^T, A^H and A all differ.
    let mut a = SparseMatrix::new(2, 2);
    a.add(0, 0, 1.).unwrap();
    a.add(0, 1, 2.).unwrap();
    a.add(1, 1, 3.).unwrap();
    let mut e = SparseMatrix::new(2, 2);
    e.add(1, 0, 1.).unwrap();
    let lu = ComplexMatrix::from_operators(&a, &e, 1.)
        .unwrap()
        .factorize()
        .unwrap();
    let rhs = [Complex::real(1.), Complex::real(-1.)];
    let x = lu.solve_transposed(&rhs).unwrap();
    // A^T = [[1, j], [2, 3]].
    let row0 = x[0] + Complex::imaginary(1.) * x[1];
    let row1 = Complex::real(2.) * x[0] + Complex::real(3.) * x[1];
    assert!((row0 - rhs[0]).magnitude() < 1e-14, "{row0}");
    assert!((row1 - rhs[1]).magnitude() < 1e-14, "{row1}");
    // The ordinary solve still solves A x = b with the same factors.
    let y = lu.solve(&rhs).unwrap();
    let row0 = y[0] + Complex::real(2.) * y[1];
    let row1 = Complex::imaginary(1.) * y[0] + Complex::real(3.) * y[1];
    assert!((row0 - rhs[0]).magnitude() < 1e-14);
    assert!((row1 - rhs[1]).magnitude() < 1e-14);
    assert!(lu.solve_transposed(&[Complex::ZERO]).is_err());
    assert!(
        lu.solve_transposed(&[Complex::real(f64::INFINITY), Complex::ZERO])
            .is_err()
    );
}

#[test]
fn singular_and_unresolved_near_singular_homogeneous_systems_are_rejected() {
    for delta in [0., 1e-14] {
        let mut a = SparseMatrix::new(2, 2);
        for (r, c, v) in [(0, 0, 1.), (0, 1, 1.), (1, 0, 1.), (1, 1, 1. + delta)] {
            a.add(r, c, v).unwrap();
        }
        let result = ComplexMatrix::from_operators(&a, &SparseMatrix::new(2, 2), 0.)
            .unwrap()
            .factorize();
        assert!(
            result.is_err(),
            "homogeneous rank guard accepted delta={delta}"
        );
    }
}
