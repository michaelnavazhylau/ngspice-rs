//! Numeric adapter guardrails and acceptance semantics.
use spice_maths::{
    SparseMatrix, Vector,
    diffsol::{BdfOptions, DaeSegment, LinearDae},
};
fn model() -> LinearDae {
    let mut a = SparseMatrix::new(2, 2);
    a.add(0, 0, 1.).unwrap();
    a.add(1, 1, 1.).unwrap();
    let mut e = SparseMatrix::new(2, 2);
    e.add(0, 0, 1.).unwrap();
    LinearDae::new(&a, &e).unwrap()
}
fn segment() -> DaeSegment {
    DaeSegment {
        start: 0.,
        end: 1.,
        initial: Vector::from_slice(&[1., 2.]),
        b_start: Vector::from_slice(&[0., 2.]),
        b_end: Vector::from_slice(&[0., 2.]),
        samples: vec![0.1, 0.5, 1.],
    }
}
fn options() -> BdfOptions {
    BdfOptions {
        rtol: 1e-8,
        atol: vec![1e-10, 1e-12],
        max_step: 0.037,
        max_steps: 10000,
    }
}
#[test]
fn consistent_initial_conditions_samples_and_accepted_steps_are_distinct() {
    let mut accepted = vec![];
    let result = model()
        .integrate_segment(&segment(), &options(), &mut |t, x| {
            accepted.push((t, x.clone()));
            Ok(())
        })
        .unwrap();
    assert!(accepted.len() > result.samples.len());
    assert_eq!(accepted.len(), result.steps);
    assert!(
        accepted
            .windows(2)
            .all(|w| w[1].0 > w[0].0 && w[1].0 - w[0].0 <= 0.037 + 1e-14)
    );
    for (t, x) in result.samples {
        assert!((x.as_slice()[0] - (-t).exp()).abs() < 1e-6);
        assert_eq!(x.as_slice()[1], 2.);
    }
    assert!((result.final_state.as_slice()[0] - (-1_f64).exp()).abs() < 1e-6);
}
#[test]
fn inconsistent_initial_conditions_are_not_silently_changed() {
    let mut s = segment();
    s.initial.as_mut_slice()[1] = 3.;
    let mut calls = 0;
    let err = model()
        .integrate_segment(&s, &options(), &mut |_, _| {
            calls += 1;
            Ok(())
        })
        .err()
        .unwrap();
    assert!(err.to_string().contains("inconsistent"));
    assert_eq!(calls, 0);
}
#[test]
fn dimension_nonfinite_grid_and_work_limits_propagate() {
    let dae = model();
    let mut s = segment();
    s.b_end = Vector::zeros(1);
    assert!(
        dae.integrate_segment(&s, &options(), &mut |_, _| Ok(()))
            .is_err()
    );
    let mut s = segment();
    s.initial.as_mut_slice()[0] = f64::NAN;
    assert!(
        dae.integrate_segment(&s, &options(), &mut |_, _| Ok(()))
            .is_err()
    );
    let mut s = segment();
    s.samples = vec![1., 0.5];
    assert!(
        dae.integrate_segment(&s, &options(), &mut |_, _| Ok(()))
            .is_err()
    );
    let mut o = options();
    o.atol = vec![1e-9];
    assert!(
        dae.integrate_segment(&segment(), &o, &mut |_, _| Ok(()))
            .is_err()
    );
    let mut o = options();
    o.max_steps = 1;
    let mut calls = 0;
    assert!(
        dae.integrate_segment(&segment(), &o, &mut |_, _| {
            calls += 1;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    let err = dae
        .integrate_segment(&segment(), &options(), &mut |_, _| {
            Err(spice_core::SpiceError::circuit("accept failed"))
        })
        .err()
        .unwrap();
    assert!(err.to_string().contains("accept failed"));
}
#[test]
fn unsupported_mass_structure_and_numeric_assembly_are_diagnosed() {
    let mut a = SparseMatrix::new(2, 2);
    a.add(0, 0, 1.).unwrap();
    a.add(1, 1, 1.).unwrap();
    let mut e = SparseMatrix::new(2, 2);
    assert!(LinearDae::new(&a, &e).is_err());
    e.add(0, 0, 1.).unwrap();
    e.add(0, 1, -1.).unwrap();
    assert!(LinearDae::new(&a, &e).is_err());
    e.clear();
    e.add(0, 0, 1.).unwrap();
    a.add(1, 1, -1.).unwrap();
    assert!(LinearDae::new(&a, &e).is_err());
    a.add(1, 1, f64::NAN).unwrap();
    assert!(LinearDae::new(&a, &e).is_err());
}
