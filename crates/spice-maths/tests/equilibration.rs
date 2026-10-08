//! Opt-in equilibration: physical units, certified rank and failure policies.
use spice_core::Complex;
use spice_maths::{
    EquilibratedComplexLu, EquilibratedDenseLu, EquilibratedSparseLu, Matrix, SparseMatrix, Vector,
    complex::ComplexMatrix, equilibration::MAX_SCALING_EXPONENT,
};

fn matrices(values: &[&[f64]]) -> (SparseMatrix, Matrix) {
    let mut sparse = SparseMatrix::new(values.len(), values[0].len());
    let mut dense = Matrix::zeros(sparse.rows(), sparse.cols());
    for (r, row) in values.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            sparse.add(r, c, v).unwrap();
            dense.set(r, c, v).unwrap();
        }
    }
    (sparse, dense)
}

fn close(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() <= 1e-13 * expected.abs() + 1e-28,
        "{got} != {expected}"
    );
}

#[test]
fn mixed_voltage_current_column_scales_pivot_repeated_rhs_and_owned_snapshots() {
    // x = [node voltage, branch current]; both rows mix very different units.
    let (mut sparse, mut dense) = matrices(&[&[0.0, 1e20], &[1.0, 2e20]]);
    assert!(sparse.factorize().is_err());
    assert!(dense.lu_decompose().is_err());
    let sf = EquilibratedSparseLu::new(&sparse, None).unwrap();
    let df = EquilibratedDenseLu::new(&dense).unwrap();
    assert_eq!(sf.scaling(), df.scaling());
    assert_eq!(
        sf.scaling(),
        EquilibratedSparseLu::new(&sparse, None).unwrap().scaling()
    );
    assert!(sf.scaling().column_factors()[0] > 1.0);
    for &v in sf
        .scaling()
        .row_factors()
        .iter()
        .chain(sf.scaling().column_factors())
    {
        assert!(
            v.is_finite()
                && v >= 2.0_f64.powi(-MAX_SCALING_EXPONENT)
                && v <= 2.0_f64.powi(MAX_SCALING_EXPONENT)
        );
    }
    sparse.clear();
    dense.clear();
    for (b, expected) in [([1.0, 4.0], [2.0, 1e-20]), ([-3.0, -5.5], [0.5, -3e-20])] {
        let rhs = Vector::from_slice(&b);
        for x in [sf.solve(&rhs).unwrap(), df.solve(&rhs).unwrap()] {
            close(x.as_slice()[0], expected[0]);
            close(x.as_slice()[1], expected[1]);
            close(1e20 * x.as_slice()[1], b[0]);
            close(x.as_slice()[0] + 2e20 * x.as_slice()[1], b[1]);
            let error = sf.check_residual(&rhs, &x).unwrap();
            // Metadata is in original equation units, not R*M*C coordinates.
            let tol = 128.0 * f64::EPSILON * 2.0;
            close(error.row_bounds[0], tol * (1e20 * x.max_abs() + b[0].abs()));
            close(
                error.row_bounds[1],
                tol * ((1.0 + 2e20) * x.max_abs() + b[1].abs()),
            );
            assert!(
                error
                    .row_residuals
                    .iter()
                    .zip(error.row_bounds)
                    .all(|(&e, bound)| e <= bound)
            );
            df.check_residual(&rhs, &x).unwrap();
        }
    }
    assert!(
        sf.check_residual(&Vector::zeros(2), &Vector::from_slice(&[0.0, 1e-20]))
            .is_err()
    );
    assert!(
        df.check_residual(&Vector::zeros(2), &Vector::zeros(1))
            .is_err()
    );
}

#[test]
fn mixed_row_scales_and_diagonal_scale_rejection_are_opt_in_only() {
    for values in [
        &[&[0.0, 2e-20][..], &[3e20, 4e20][..]][..],
        &[&[1.0, 0.0][..], &[0.0, 1e-30][..]][..],
    ] {
        let (s, d) = matrices(values);
        assert!(s.factorize().is_err());
        assert!(d.lu_decompose().is_err());
        let expected = Vector::from_slice(&[1.0, 2.0]);
        let rhs = s.mul_vector(&expected).unwrap();
        for x in [
            EquilibratedSparseLu::new(&s, None)
                .unwrap()
                .solve(&rhs)
                .unwrap(),
            EquilibratedDenseLu::new(&d).unwrap().solve(&rhs).unwrap(),
        ] {
            for (&got, &expected) in x.as_slice().iter().zip(expected.as_slice()) {
                close(got, expected);
            }
        }
    }
}

#[test]
fn complex_frequency_assembly_pivot_scales_phases_and_snapshot() {
    let (mut a, _) = matrices(&[&[0.0, 1e20], &[1.0, 0.0]]);
    let (mut e, _) = matrices(&[&[0.0, 0.0], &[0.0, 1e20]]);
    assert!(
        ComplexMatrix::from_operators(&a, &e, 2.0)
            .unwrap()
            .factorize()
            .is_err()
    );
    let factor = EquilibratedComplexLu::from_operators(&a, &e, 2.0).unwrap();
    a.clear();
    e.clear();
    for x in [
        [Complex::new(1.0, -2.0), Complex::new(2e-20, 1e-20)],
        [Complex::ZERO, Complex::new(-1e-20, 3e-20)],
    ] {
        let rhs = [
            Complex::real(1e20) * x[1],
            x[0] + Complex::imaginary(2e20) * x[1],
        ];
        let got = factor.solve(&rhs).unwrap();
        for (v, expected) in got.iter().zip(x) {
            close(v.re, expected.re);
            close(v.im, expected.im);
        }
        let error = factor.check_residual(&rhs, &got).unwrap();
        let max_x = got.iter().fold(0.0_f64, |m, v| m.max(v.magnitude()));
        let tol = 128.0 * f64::EPSILON * 2.0;
        close(
            error.row_bounds[0],
            tol * (1e20 * max_x + rhs[0].magnitude()),
        );
        close(
            error.row_bounds[1],
            tol * ((1.0 + 2e20) * max_x + rhs[1].magnitude()),
        );
        assert!(
            error
                .row_residuals
                .iter()
                .zip(error.row_bounds)
                .all(|(&e, b)| e <= b)
        );
    }
    assert!(
        factor
            .check_residual(&[Complex::ZERO; 2], &[Complex::real(1.0); 2])
            .is_err()
    );
    assert!(factor.solve(&[Complex::ZERO]).is_err());
    assert!(factor.solve(&[Complex::real(f64::NAN); 2]).is_err());
}

#[test]
fn duplicate_cancellation_and_exact_pattern_symbolic_reuse() {
    let (mut s, _) = matrices(&[&[2e-20, 1e-20], &[0.0, 3e20]]);
    s.add(1, 0, 1e300).unwrap();
    s.add(1, 0, -1e300).unwrap();
    let saved = s.clone();
    let f = EquilibratedSparseLu::new(&s, None).unwrap();
    assert_eq!(s, saved);
    s.add(0, 0, 2e-20).unwrap();
    let reused = EquilibratedSparseLu::new(&s, Some(f.symbolic())).unwrap();
    let b = Vector::from_slice(&[6e-20, 6e20]);
    let x = reused.solve(&b).unwrap();
    close(x.as_slice()[0], 1.0);
    close(x.as_slice()[1], 2.0);
    s.add(0, 1, -1e-20).unwrap();
    assert!(EquilibratedSparseLu::new(&s, Some(f.symbolic())).is_err());
    EquilibratedSparseLu::new(&s, None).unwrap();
    // Unscaled symbolic metadata is compatible when the assembled pattern matches.
    let (s, _) = matrices(&[&[2.0, 1.0], &[0.0, 3.0]]);
    EquilibratedSparseLu::new(&s, Some(s.factorize().unwrap().symbolic())).unwrap();
    // Fold E before frequency multiplication: cancelling original duplicates
    // must not overflow separately or invent complex structural entries.
    let (a, _) = matrices(&[&[1.0]]);
    let (mut e, _) = matrices(&[&[f64::MAX]]);
    e.add(0, 0, -f64::MAX).unwrap();
    let f = EquilibratedComplexLu::from_operators(&a, &e, 2.0).unwrap();
    assert_eq!(
        f.solve(&[Complex::real(3.0)]).unwrap(),
        vec![Complex::real(3.0)]
    );
}

#[test]
fn singular_homogeneous_nonunique_and_ideal_source_loop_remain_failures() {
    for values in [
        &[&[1.0, 0.0][..], &[0.0, 0.0][..]][..], // zero row and column
        &[&[1.0, 0.0][..], &[2.0, 0.0][..]][..], // zero column only
        &[&[1e-20, 2e-20][..], &[2e20, 4e20][..]][..],
        &[
            &[1.0, 1.0, 1.0][..],
            &[1.0, 0.0, 0.0][..],
            &[1.0, 0.0, 0.0][..],
        ][..], // parallel ideal voltage sources
    ] {
        let (s, d) = matrices(values);
        assert!(EquilibratedSparseLu::new(&s, None).is_err());
        assert!(EquilibratedDenseLu::new(&d).is_err());
        assert!(
            EquilibratedComplexLu::from_operators(&s, &SparseMatrix::new(s.rows(), s.cols()), 0.0)
                .is_err()
        );
    }
    // Dense retains its pivot policy; sparse/complex retain their stronger
    // conditioning certification rather than accepting this near dependence.
    let (s, _) = matrices(&[&[1.0, 1.0], &[1.0, 1.0 + 1e-15]]);
    assert!(EquilibratedSparseLu::new(&s, None).is_err());
    assert!(EquilibratedComplexLu::from_operators(&s, &SparseMatrix::new(2, 2), 0.0).is_err());
}

#[test]
fn dimensions_nonfinite_duplicate_and_frequency_overflow_are_errors() {
    for (s, d) in [
        (SparseMatrix::new(0, 0), Matrix::zeros(0, 0)),
        (SparseMatrix::new(2, 3), Matrix::zeros(2, 3)),
    ] {
        assert!(EquilibratedSparseLu::new(&s, None).is_err());
        assert!(EquilibratedDenseLu::new(&d).is_err());
        assert!(EquilibratedComplexLu::from_operators(&s, &s, 0.0).is_err());
    }
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (s, d) = matrices(&[&[v]]);
        assert!(EquilibratedSparseLu::new(&s, None).is_err());
        assert!(EquilibratedDenseLu::new(&d).is_err());
        assert!(EquilibratedComplexLu::from_operators(&s, &s, 0.0).is_err());
    }
    let (mut s, _) = matrices(&[&[f64::MAX]]);
    s.add(0, 0, f64::MAX).unwrap();
    assert!(EquilibratedSparseLu::new(&s, None).is_err());
    let zero = SparseMatrix::new(1, 1);
    assert!(EquilibratedComplexLu::from_operators(&s, &zero, 0.0).is_err());
    assert!(EquilibratedComplexLu::from_operators(&zero, &s, 0.0).is_err());
    let (one, _) = matrices(&[&[1.0]]);
    for w in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(EquilibratedComplexLu::from_operators(&one, &zero, w).is_err());
    }
    assert!(EquilibratedComplexLu::from_operators(&one, &SparseMatrix::new(2, 2), 0.0).is_err());
    let (e, _) = matrices(&[&[f64::MAX]]);
    assert!(EquilibratedComplexLu::from_operators(&one, &e, 2.0).is_err());
    let (e, _) = matrices(&[&[1e-300]]);
    assert!(EquilibratedComplexLu::from_operators(&one, &e, 1e-300).is_err());
    let (s, d) = matrices(&[&[1.0]]);
    let sf = EquilibratedSparseLu::new(&s, None).unwrap();
    let df = EquilibratedDenseLu::new(&d).unwrap();
    for b in [Vector::zeros(2), Vector::from_slice(&[f64::NAN])] {
        assert!(sf.solve(&b).is_err());
        assert!(df.solve(&b).is_err());
    }
}

#[test]
fn coefficient_rhs_solution_and_original_residual_range_failures_are_explicit() {
    // Row normalization would destroy the tiny off-diagonal; never drop it.
    let (s, d) = matrices(&[&[1e300, 1e-300], &[0.0, 1.0]]);
    assert!(EquilibratedSparseLu::new(&s, None).is_err());
    assert!(EquilibratedDenseLu::new(&d).is_err());
    assert!(EquilibratedComplexLu::from_operators(&s, &SparseMatrix::new(2, 2), 0.0).is_err());
    for (coefficient, rhs) in [(1e-200, f64::MAX), (1e200, 1e-200), (1e200, 1e-100)] {
        let (s, d) = matrices(&[&[coefficient]]);
        let sf = EquilibratedSparseLu::new(&s, None).unwrap();
        let df = EquilibratedDenseLu::new(&d).unwrap();
        // Third case overflows the original-unit normwise residual scale only
        // when checking a deliberately enormous candidate, not during solve.
        if rhs == 1e-100 {
            assert!(
                sf.check_residual(&Vector::zeros(1), &Vector::from_slice(&[f64::MAX]))
                    .is_err()
            );
            assert!(
                df.check_residual(&Vector::zeros(1), &Vector::from_slice(&[f64::MAX]))
                    .is_err()
            );
        } else {
            assert!(sf.solve(&Vector::from_slice(&[rhs])).is_err());
            assert!(df.solve(&Vector::from_slice(&[rhs])).is_err());
            let cf =
                EquilibratedComplexLu::from_operators(&s, &SparseMatrix::new(1, 1), 0.0).unwrap();
            assert!(cf.solve(&[Complex::new(rhs, rhs)]).is_err());
        }
    }
    // Column recovery itself can overflow, even with finite scaled y.
    let (s, _) = matrices(&[&[0.0, 1e70], &[1e-70, 1e70]]);
    let sf = EquilibratedSparseLu::new(&s, None).unwrap();
    assert!(sf.solve(&Vector::from_slice(&[0.0, f64::MAX])).is_err());
    // RHS scaling stays normal; the physical solution would underflow.
    let (s, _) = matrices(&[&[1e200]]);
    let sf = EquilibratedSparseLu::new(&s, None).unwrap();
    assert!(sf.solve(&Vector::from_slice(&[1e-140])).is_err());
}

#[test]
fn exponent_bounds_extremes_and_subnormal_input_are_deterministic() {
    for v in [f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1)] {
        let (s, _) = matrices(&[&[v]]);
        let f = EquilibratedSparseLu::new(&s, None).unwrap();
        let scaling = f.scaling();
        for &s in scaling.row_factors().iter().chain(scaling.column_factors()) {
            assert!(s.is_finite() && s > 0.0 && s >= 2.0_f64.powi(-512) && s <= 2.0_f64.powi(512));
        }
        // Zero RHS remains valid even when physical residual arithmetic at
        // nonzero RHS would overflow; no unscaled inverse rank check is added.
        assert_eq!(f.solve(&Vector::zeros(1)).unwrap(), Vector::zeros(1));
    }
}
