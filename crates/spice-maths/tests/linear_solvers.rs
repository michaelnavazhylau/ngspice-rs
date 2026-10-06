//! Production-interface LU regressions, including singular homogeneous systems.
use spice_maths::{Matrix, SparseLu, SparseMatrix, Vector};

fn matrices(values: &[&[f64]]) -> (SparseMatrix, Matrix) {
    let mut s = SparseMatrix::new(values.len(), values[0].len());
    let mut d = Matrix::zeros(s.rows(), s.cols());
    for (r, row) in values.iter().enumerate() {
        for (c, a) in row.iter().enumerate() {
            s.add(r, c, *a).unwrap();
            d.set(r, c, *a).unwrap();
        }
    }
    (s, d)
}

#[test]
fn pivoting_nonsymmetric_repeated_rhs_and_snapshot_ownership() {
    let (mut s, mut d) = matrices(&[&[0., 2.], &[3., 4.]]);
    let sf = s.factorize().unwrap();
    let df = d.lu_decompose().unwrap();
    for x in [&[1., 2.][..], &[-3., 0.5][..]] {
        let x = Vector::from_slice(x);
        let b = s.mul_vector(&x).unwrap();
        for got in [sf.solve(&b).unwrap(), df.solve(&b).unwrap()] {
            for (a, b) in got.as_slice().iter().zip(x.as_slice()) {
                assert!((a - b).abs() < 1e-14);
            }
        }
    }
    let b = Vector::from_slice(&[4., 11.]);
    s.clear();
    s.add(0, 0, 1.).unwrap();
    s.add(1, 1, 1.).unwrap();
    d.data_mut().copy_from_slice(&[1., 0., 0., 1.]);
    assert_eq!(sf.solve(&b).unwrap().as_slice(), &[1., 2.]);
    assert_eq!(df.solve(&b).unwrap().as_slice(), &[1., 2.]);
    assert_eq!(s.solve(&b).unwrap(), b);
    assert_eq!(d.solve(&b).unwrap(), b);
    d.fill(0.);
    assert!(d.solve(&b).is_err());
    d.set(0, 0, 1.).unwrap();
    d.add_to(1, 1, 1.).unwrap();
    assert_eq!(d.solve(&b).unwrap(), b);
    d.clear();
    assert!(d.lu_decompose().is_err());
}

#[test]
fn duplicates_and_exact_pattern_checked_symbolic_reuse() {
    let (mut s, _) = matrices(&[&[2., 1.], &[0., 3.]]);
    let factor = s.factorize().unwrap();
    s.add(0, 0, 2.).unwrap();
    let reused = SparseLu::new(&s, Some(factor.symbolic())).unwrap();
    assert_eq!(
        reused
            .solve(&Vector::from_slice(&[6., 6.]))
            .unwrap()
            .as_slice(),
        &[1., 2.]
    );
    s.add(0, 1, -1.).unwrap();
    s.fold_duplicates();
    assert!(SparseLu::new(&s, Some(factor.symbolic())).is_err());
    assert_eq!(
        s.solve(&Vector::from_slice(&[4., 6.])).unwrap().as_slice(),
        &[1., 2.]
    );
}

#[test]
fn invalid_dimensions_nonfinite_and_overflow() {
    for (s, d) in [
        (SparseMatrix::new(0, 0), Matrix::zeros(0, 0)),
        (SparseMatrix::new(2, 3), Matrix::zeros(2, 3)),
    ] {
        assert!(s.factorize().is_err());
        assert!(d.lu_decompose().is_err());
    }
    for a in [f64::NAN, f64::INFINITY] {
        let (s, d) = matrices(&[&[a]]);
        assert!(s.factorize().is_err());
        assert!(d.lu_decompose().is_err());
    }
    let (mut s, d) = matrices(&[&[f64::MAX]]);
    s.add(0, 0, f64::MAX).unwrap();
    assert!(s.factorize().is_err());
    assert!(d.solve(&Vector::zeros(2)).is_err());
    let (s, d) = matrices(&[&[1.]]);
    for rhs in [Vector::zeros(2), Vector::from_slice(&[f64::NAN])] {
        assert!(s.solve(&rhs).is_err());
        assert!(d.solve(&rhs).is_err());
    }
}

#[test]
fn structural_and_numeric_singularity_even_with_zero_rhs() {
    let (s, d) = matrices(&[&[1., 0.], &[0., 1e-30]]);
    assert!(s.factorize().is_err());
    assert!(d.lu_decompose().is_err());
    for values in [
        &[&[1., 0.][..], &[0., 0.][..]][..],
        &[&[1., 2.][..], &[2., 4.][..]][..],
    ] {
        let (s, d) = matrices(values);
        assert!(s.factorize().is_err());
        assert!(d.lu_decompose().is_err());
        assert!(s.solve(&Vector::zeros(2)).is_err());
        assert!(d.solve(&Vector::zeros(2)).is_err());
    }
}
