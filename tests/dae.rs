//! Numeric adapter guardrails and acceptance semantics.
use ngspice_rs::maths::{
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
            Err(ngspice_rs::primitives::SpiceError::circuit("accept failed"))
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
    // A coupled, rank-deficient mass is index one with W^T A N = 1/sqrt(2).
    assert_eq!(LinearDae::new(&a, &e).unwrap().differential_dimension(), 1);
    e.clear();
    e.add(0, 0, 1.).unwrap();
    a.add(1, 1, -1.).unwrap();
    assert!(LinearDae::new(&a, &e).is_err());
    a.add(1, 1, f64::NAN).unwrap();
    assert!(LinearDae::new(&a, &e).is_err());
}

/// Floating capacitor between unknowns 0 and 1 (`E = C [[1,-1],[-1,1]]`),
/// each tied to ground through a conductance.
fn floating(g0: f64, g1: f64) -> (SparseMatrix, SparseMatrix) {
    let mut a = SparseMatrix::new(2, 2);
    a.add(0, 0, g0).unwrap();
    a.add(1, 1, g1).unwrap();
    let mut e = SparseMatrix::new(2, 2);
    for (r, c, v) in [(0, 0, 1.), (0, 1, -1.), (1, 0, -1.), (1, 1, 1.)] {
        e.add(r, c, v).unwrap();
    }
    (a, e)
}

#[test]
fn floating_mass_projection_preserves_charge_and_integrates_its_mode() {
    let (a, e) = floating(1., 3.);
    let dae = LinearDae::new(&a, &e).unwrap();
    assert_eq!(dae.differential_dimension(), 1);
    // Any state projects onto g0 x0 + g1 x1 = b0 + b1 with x0 - x1 kept.
    let b = Vector::from_slice(&[2., 0.]);
    let x = dae.project(&Vector::from_slice(&[5., 1.]), &b).unwrap();
    let x = x.as_slice();
    assert!((x[0] - x[1] - 4.).abs() < 1e-12);
    assert!((x[0] + 3. * x[1] - 2.).abs() < 1e-12);
    // Capacitor voltage u = x0 - x1 obeys u' = b0 - x0 - ... : with b = 0,
    // u' = -(g0 g1 / (g0 + g1)) u.
    let segment = DaeSegment {
        start: 0.,
        end: 1.,
        initial: dae
            .project(&Vector::from_slice(&[1., 0.]), &Vector::zeros(2))
            .unwrap(),
        b_start: Vector::zeros(2),
        b_end: Vector::zeros(2),
        samples: vec![0.5, 1.],
    };
    let result = dae
        .integrate_segment(&segment, &options(), &mut |_, _| Ok(()))
        .unwrap();
    for (t, x) in result.samples {
        let u = x.as_slice()[0] - x.as_slice()[1];
        assert!((u - (-0.75 * t).exp()).abs() < 1e-6, "t={t} u={u}");
        // KCL through the capacitor: g0 x0 = -g1 x1.
        assert!((x.as_slice()[0] + 3. * x.as_slice()[1]).abs() < 1e-8);
    }
}

#[test]
fn higher_index_and_nonunique_nullspaces_are_rejected() {
    // x0' - x1 = b0, x0 = b1: index two (W^T A N = 0).
    let mut a = SparseMatrix::new(2, 2);
    a.add(0, 1, -1.).unwrap();
    a.add(1, 0, 1.).unwrap();
    let mut e = SparseMatrix::new(2, 2);
    e.add(0, 0, 1.).unwrap();
    let error = LinearDae::new(&a, &e).err().unwrap().to_string();
    assert!(error.contains("higher-index"), "{error}");
    // A floating capacitor with no conductive return: its common mode is
    // undetermined, so the solution is not unique despite zero residuals.
    let (a, e) = floating(0., 0.);
    let mut a = a;
    a.add(0, 0, 0.).unwrap();
    let error = LinearDae::new(&a, &e).err().unwrap().to_string();
    assert!(error.contains("higher-index/singular"), "{error}");
    // Oversized coupled blocks are refused rather than analysed densely.
    let n = ngspice_rs::maths::diffsol::MAX_MASS_BLOCK + 1;
    let mut a = SparseMatrix::new(n, n);
    let mut e = SparseMatrix::new(n, n);
    for i in 0..n {
        a.add(i, i, 1.).unwrap();
        e.add(i, i, 2.).unwrap();
        if i + 1 < n {
            e.add(i, i + 1, -1.).unwrap();
            e.add(i + 1, i, -1.).unwrap();
        }
    }
    let error = LinearDae::new(&a, &e).err().unwrap().to_string();
    assert!(error.contains("dense analysis limit"), "{error}");
}
