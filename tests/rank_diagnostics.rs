//! Guard regressions for #47: full-range diagnostics must not depend on user RHS.
use ngspice_rs::maths::{SparseLu, SparseMatrix, Vector, complex::ComplexMatrix};
use ngspice_rs::primitives::{Complex, SpiceError};

fn matrix(values: &[&[f64]]) -> SparseMatrix {
    let mut a = SparseMatrix::new(values.len(), values.len());
    for (r, row) in values.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0.0 {
                a.add(r, c, v).unwrap();
            }
        }
    }
    a
}

fn reject_both(a: &SparseMatrix) {
    assert!(matches!(
        a.solve(&Vector::zeros(a.rows())),
        Err(SpiceError::Numerical { .. })
    ));
    let e = SparseMatrix::new(a.rows(), a.cols());
    let ac = ComplexMatrix::from_operators(a, &e, 1.0).unwrap();
    assert!(matches!(ac.factorize(), Err(SpiceError::Numerical { .. })));
}

#[test]
fn rank_diagnostics_homogeneous_ideal_source_loop() {
    // Two parallel ideal voltage sources: branch currents have a free difference.
    reject_both(&matrix(&[&[1., 1., 1.], &[1., 0., 0.], &[1., 0., 0.]]));
    // Structurally full but linearly dependent rows, not an empty-row check.
    reject_both(&matrix(&[&[1., 2., 3.], &[2., 4., 6.], &[3., 6., 9.]]));
}

#[test]
fn rank_diagnostics_near_dependent_constraints_and_complex_phase() {
    for delta in [0., 1e-14, 1e-13] {
        let a = matrix(&[&[1., 1.], &[1., 1. + delta]]);
        reject_both(&a);
        // A genuinely complex near-dependency, not merely real values in c64.
        let ac = ComplexMatrix::from_operators(&a, &a, 2.).unwrap();
        assert!(ac.factorize().is_err());
    }
    let a = matrix(&[&[1., 1.], &[1., 1. + 1e-8]]);
    assert!(a.factorize().is_ok());
    assert!(
        ComplexMatrix::from_operators(&a, &a, 2.)
            .unwrap()
            .factorize()
            .is_ok()
    );
}

#[test]
fn rank_diagnostics_pivoting_disconnected_blocks_repeated_rhs_snapshots() {
    // Two independent indefinite MNA blocks; the last block forces row pivoting.
    let mut a = matrix(&[
        &[2., 1., 0., 0.],
        &[1., 0., 0., 0.],
        &[0., 0., 0., 3.],
        &[0., 0., 4., 1.],
    ]);
    let e = a.clone();
    let lu = a.factorize().unwrap();
    let ac = ComplexMatrix::from_operators(&a, &e, 2.)
        .unwrap()
        .factorize()
        .unwrap();
    for expected in [Vector::zeros(4), Vector::from_slice(&[1., -2., 3., -4.])] {
        let rhs = a.mul_vector(&expected).unwrap();
        assert_eq!(lu.solve(&rhs).unwrap(), expected);
        let rhs_ac: Vec<_> = rhs
            .as_slice()
            .iter()
            .map(|&v| Complex::new(v, 2. * v))
            .collect();
        for (got, &want) in ac.solve(&rhs_ac).unwrap().iter().zip(expected.as_slice()) {
            assert!((*got - Complex::real(want)).magnitude() < 1e-13);
        }
    }
    let rhs = a
        .mul_vector(&Vector::from_slice(&[1., 2., 3., 4.]))
        .unwrap();
    a.clear();
    assert_eq!(lu.solve(&rhs).unwrap().as_slice(), &[1., 2., 3., 4.]);
    assert!(ac.solve(&[Complex::ZERO; 4]).is_ok());
    reject_both(&a);
}

#[test]
fn rank_diagnostics_duplicate_cancellation_and_overflow() {
    let mut a = matrix(&[&[2., 1.], &[1., 0.]]);
    let lu = a.factorize().unwrap();
    a.add(0, 1, -1.).unwrap();
    assert!(SparseLu::new(&a, Some(lu.symbolic())).is_err());
    reject_both(&a); // The cancelled pattern now has a zero column.
    assert_eq!(
        lu.solve(&Vector::from_slice(&[3., 1.])).unwrap().as_slice(),
        &[1., 1.]
    );
    let mut overflow = SparseMatrix::new(1, 1);
    overflow.add(0, 0, f64::MAX).unwrap();
    overflow.add(0, 0, f64::MAX).unwrap();
    assert!(overflow.factorize().is_err());
    assert!(ComplexMatrix::from_operators(&overflow, &SparseMatrix::new(1, 1), 1.).is_err());
}

#[test]
fn rank_diagnostics_unit_pivots_do_not_establish_resolvable_conditioning() {
    // The unpermuted triangular LU has unit diagonal, but its inverse grows
    // geometrically. A pivot-only strategy would miss the conditioning.
    let n = 16;
    let mut a = SparseMatrix::new(n, n);
    for r in 0..n {
        a.add(r, r, 1.).unwrap();
        if r + 1 < n {
            a.add(r, r + 1, 8.).unwrap();
        }
    }
    reject_both(&a);
}

#[test]
fn rank_diagnostics_balanced_nullspace_size_and_permutation_sweep() {
    // A cycle Laplacian has a distributed null vector, including at larger n.
    // Permute rows without permuting columns to vary pivoting, preserving rank.
    for n in [3, 8, 17, 65, 129, 257] {
        for shift in [0, 1, n / 2] {
            let mut a = SparseMatrix::new(n, n);
            for r in 0..n {
                let pr = (r + shift) % n;
                a.add(pr, r, 2.).unwrap();
                a.add(pr, (r + 1) % n, -1.).unwrap();
                a.add(pr, (r + n - 1) % n, -1.).unwrap();
            }
            reject_both(&a);
            assert!(
                ComplexMatrix::from_operators(&a, &a, 0.75)
                    .unwrap()
                    .factorize()
                    .is_err()
            );
        }
    }
}
