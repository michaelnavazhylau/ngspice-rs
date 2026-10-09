//! #29 numeric-only formulation gate: no deck/runtime higher-index enablement.
use ngspice_rs::maths::{
    SparseMatrix, Vector,
    diffsol::{
        LinearDae,
        higher_index::{ConstrainedPencil, ForcingJet, MAX_UNKNOWNS, ProjectionOptions},
    },
};

fn options(d: usize) -> ProjectionOptions {
    ProjectionOptions {
        relative: 1e-12,
        state_absolute: vec![1e-12; d],
        equation_absolute: vec![1e-12; d],
        max_unknowns: MAX_UNKNOWNS,
    }
}

// Parallel grounded C=2, G=3, L=4, series inductor R=5, source V.
// Unknowns [v, iL, iV], branch currents positive node -> ground.
fn operators() -> (SparseMatrix, SparseMatrix) {
    let mut a = SparseMatrix::new(3, 3);
    for (r, c, v) in [
        (0, 0, 3.),
        (0, 1, 1.),
        (1, 0, 1.),
        (1, 1, -5.),
        (0, 2, 1.),
        (2, 0, 1.),
    ] {
        a.add(r, c, v).unwrap();
    }
    let mut e = SparseMatrix::new(3, 3);
    e.add(0, 0, 2.).unwrap();
    e.add(1, 1, -4.).unwrap();
    (a, e)
}
fn model() -> ConstrainedPencil {
    let (a, e) = operators();
    ConstrainedPencil::new(&a, &e, 1, 1, options(3)).unwrap()
}
fn sine(t: f64) -> ForcingJet {
    ForcingJet {
        value: Vector::from_slice(&[0., 0., t.sin()]),
        first: Vector::from_slice(&[0., 0., t.cos()]),
        constraint_second: Vector::from_slice(&[-t.sin()]),
    }
}
fn ramp(t: f64) -> ForcingJet {
    ForcingJet {
        value: Vector::from_slice(&[0., 0., t]),
        first: Vector::from_slice(&[0., 0., 1.]),
        constraint_second: Vector::from_slice(&[0.]),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-12, "{a} != {b}");
}

#[test]
fn sine_analytic_solution_signed_currents_and_differentiated_residuals() {
    // Steady forced L z' + R z = sin(t): z=(5 sin(t)-4 cos(t))/41.
    let m = model();
    let mut worst: f64 = 0.;
    for i in 0..=100 {
        let t = i as f64 * 0.1;
        let z = (5. * t.sin() - 4. * t.cos()) / 41.;
        let dz = (5. * t.cos() + 4. * t.sin()) / 41.;
        let p = m.reconstruct(&Vector::from_slice(&[z]), &sine(t)).unwrap();
        close(p.state.as_slice()[0], t.sin());
        close(p.state.as_slice()[1], z);
        close(p.state.as_slice()[2], -2. * t.cos() - 3. * t.sin() - z);
        close(p.derivative.as_slice()[0], t.cos());
        close(p.derivative.as_slice()[1], dz);
        close(p.derivative.as_slice()[2], 2. * t.sin() - 3. * t.cos() - dz);
        close(p.dynamic_acceleration.as_slice()[0], -t.sin());
        close(p.dynamic_acceleration.as_slice()[1], -z);
        let r = m.residuals(&p, &sine(t)).unwrap();
        for v in r
            .original
            .as_slice()
            .iter()
            .chain(r.differentiated.as_slice())
            .chain(r.constraint_second.as_slice())
        {
            worst = worst.max(v.abs());
        }
        assert!(r.max_bound_ratio < 0.001);
    }
    assert!(worst < 5e-15);
    println!("sine: 101 points; worst original/differentiated/hidden residual={worst:e}");
}

#[test]
fn ramp_transient_free_current_and_compatible_incompatible_ics() {
    let m = model();
    // z(0)=0: z=t/5 - 4/25 + 4/25 exp(-5t/4).
    for i in 0..=20 {
        let t = i as f64 * 0.1;
        let z = t / 5. - 4. / 25. + 4. / 25. * (-5. * t / 4.).exp();
        let dz = (1. - (-5. * t / 4.).exp()) / 5.;
        let p = m.reconstruct(&Vector::from_slice(&[z]), &ramp(t)).unwrap();
        close(p.derivative.as_slice()[1], dz);
        close(p.state.as_slice()[2], -2. - 3. * t - z);
        m.consistent_initial(&p.state, &ramp(t)).unwrap();
        for row in [0, 2] {
            let mut incompatible = p.state.clone();
            incompatible.as_mut_slice()[row] += 0.1;
            assert!(
                m.consistent_initial(&incompatible, &ramp(t))
                    .unwrap_err()
                    .to_string()
                    .contains("incompatible")
            );
        }
    }
    // Free inductor current may be chosen independently, including nonzero IC.
    let p = m
        .reconstruct(&Vector::from_slice(&[7.]), &ramp(0.))
        .unwrap();
    assert_eq!(p.state.as_slice()[1], 7.);
    m.consistent_initial(&p.state, &ramp(0.)).unwrap();
}

#[test]
fn derivative_is_not_a_hard_coded_solution_and_residual_checker_rejects_corruption() {
    let m = model();
    let mut jet = sine(0.3);
    jet.value.as_mut_slice()[0] = 7.;
    jet.value.as_mut_slice()[1] = -2.;
    jet.first.as_mut_slice()[0] = 3.;
    jet.first.as_mut_slice()[1] = 4.;
    let p = m.reconstruct(&Vector::from_slice(&[0.8]), &jet).unwrap();
    close(
        p.derivative.as_slice()[1],
        (0.3_f64.sin() - 5. * 0.8 + 2.) / 4.,
    );
    close(
        p.state.as_slice()[2],
        7. - 2. * 0.3_f64.cos() - 3. * 0.3_f64.sin() - 0.8,
    );
    for field in 0..3 {
        let mut bad = p.clone();
        match field {
            0 => bad.state.as_mut_slice()[2] += 0.01,
            1 => bad.derivative.as_mut_slice()[2] += 0.01,
            _ => bad.dynamic_acceleration.as_mut_slice()[0] += 0.01,
        }
        assert!(
            m.residuals(&bad, &jet)
                .unwrap_err()
                .to_string()
                .contains("residual bound")
        );
    }
    // Cross-check every returned physical derivative by independent central differences
    // with z(t +/- h) from its local Taylor data; never used by the prototype.
    let h = 1e-5;
    let dz = p.derivative.as_slice()[1];
    let ddz = p.dynamic_acceleration.as_slice()[1];
    let mut shifted = vec![];
    for dt in [-h, h] {
        let mut j = sine(0.3 + dt);
        j.value.as_mut_slice()[0] = 7. + 3. * dt;
        j.value.as_mut_slice()[1] = -2. + 4. * dt;
        j.first.as_mut_slice()[0] = 3.;
        j.first.as_mut_slice()[1] = 4.;
        shifted.push(
            m.reconstruct(
                &Vector::from_slice(&[0.8 + dt * dz + 0.5 * dt * dt * ddz]),
                &j,
            )
            .unwrap(),
        );
    }
    for r in 0..3 {
        let finite_difference =
            (shifted[1].state.as_slice()[r] - shifted[0].state.as_slice()[r]) / (2. * h);
        assert!((finite_difference - p.derivative.as_slice()[r]).abs() < 2e-9);
    }
}

fn two_voltage(h: [[f64; 2]; 2]) -> (SparseMatrix, SparseMatrix) {
    let mut a = SparseMatrix::new(5, 5);
    let mut e = SparseMatrix::new(5, 5);
    for i in 0..2 {
        e.add(i, i, 1. + i as f64).unwrap();
        a.add(i, i, 1.).unwrap();
    }
    e.add(2, 2, -3.).unwrap();
    a.add(0, 2, 1.).unwrap();
    a.add(2, 0, 1.).unwrap();
    a.add(2, 2, -2.).unwrap();
    for (r, row) in h.iter().enumerate() {
        for (c, &value) in row.iter().enumerate() {
            a.add(r, 3 + c, value).unwrap();
            a.add(3 + c, r, value).unwrap();
        }
    }
    (a, e)
}

#[test]
fn coupled_source_coordinates_have_unique_physical_reconstruction() {
    // Source 0 node 0 -> ground, source 1 node 1 -> node 0.
    let (a, e) = two_voltage([[1., -1.], [0., 1.]]);
    let m = ConstrainedPencil::new(&a, &e, 2, 1, options(5)).unwrap();
    let jet = ForcingJet {
        value: Vector::from_slice(&[0., 0., 0., 2., 3.]),
        first: Vector::from_slice(&[0., 0., 0., 1., -2.]),
        constraint_second: Vector::zeros(2),
    };
    let p = m.reconstruct(&Vector::from_slice(&[0.5]), &jet).unwrap();
    assert_eq!(p.state.as_slice(), &[2., 5., 0.5, -6.5, -3.]);
    close(p.derivative.as_slice()[2], 1. / 3.);
    m.residuals(&p, &jet).unwrap();
}

#[test]
fn redundant_inconsistent_and_homogeneous_rank_failures_are_refused_before_projection() {
    // Identical voltage constraints: equal RHS redundant; unequal RHS inconsistent.
    // Both are refused before RHS evaluation: lambda is never unique in either case.
    let (a, e) = two_voltage([[1., 1.], [0., 0.]]);
    // The preparation API intentionally has no forcing argument. This single
    // rank gate rules out every RHS, rather than pretending to classify [2,2]
    // vs [2,3] through a solve that could never have unique source currents.
    let err = ConstrainedPencil::new(&a, &e, 2, 1, options(5))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("redundant/rank-deficient"));
    assert!(err.contains("inconsistent forcing"));
    let (a, e) = two_voltage([[1., 1.], [1., 1. + f64::EPSILON]]);
    assert!(ConstrainedPencil::new(&a, &e, 2, 1, options(5)).is_err());
    let (a, mut e) = operators();
    e.add(1, 1, 4.).unwrap();
    assert!(ConstrainedPencil::new(&a, &e, 1, 1, options(3)).is_err());
}

#[test]
fn voltage_jumps_and_nonsmooth_corners_are_not_projected_as_continuous_currents() {
    let m = model();
    let left = ramp(0.);
    m.check_smooth_join(&left, &left).unwrap();
    let mut right = left.clone();
    right.value.as_mut_slice()[2] = 1e-30; // exact impulse gate; not a tolerance test
    assert!(
        m.check_smooth_join(&left, &right)
            .unwrap_err()
            .to_string()
            .contains("impulsive")
    );
    let mut right = left.clone();
    right.first.as_mut_slice()[2] = 2.;
    assert!(
        m.check_smooth_join(&left, &right)
            .unwrap_err()
            .to_string()
            .contains("nonsmooth")
    );
    let mut right = left.clone();
    right.constraint_second.as_mut_slice()[0] = 1.;
    assert!(m.check_smooth_join(&left, &right).is_err());
}

#[test]
fn dimensions_empty_budget_tolerances_and_unsupported_structure_are_fallible() {
    let (a, e) = operators();
    for (n, k) in [(0, 1), (1, 0), (2, 1), (usize::MAX, 1)] {
        assert!(ConstrainedPencil::new(&a, &e, n, k, options(3)).is_err());
    }
    assert!(
        ConstrainedPencil::new(
            &SparseMatrix::new(0, 0),
            &SparseMatrix::new(0, 0),
            0,
            0,
            options(0)
        )
        .is_err()
    );
    assert!(ConstrainedPencil::new(&SparseMatrix::new(3, 2), &e, 1, 1, options(3)).is_err());
    for budget in [0, 2, MAX_UNKNOWNS + 1] {
        let mut o = options(3);
        o.max_unknowns = budget;
        assert!(ConstrainedPencil::new(&a, &e, 1, 1, o).is_err());
    }
    for invalid in [0., -1., f64::NAN, f64::INFINITY, 1.] {
        let mut o = options(3);
        o.relative = invalid;
        assert!(ConstrainedPencil::new(&a, &e, 1, 1, o).is_err());
    }
    for invalid in [0., -1., f64::NAN, f64::INFINITY] {
        for state in [false, true] {
            let mut o = options(3);
            if state {
                o.state_absolute[0] = invalid;
            } else {
                o.equation_absolute[0] = invalid;
            }
            assert!(ConstrainedPencil::new(&a, &e, 1, 1, o).is_err());
        }
    }
    let mut o = options(3);
    o.state_absolute.pop();
    assert!(ConstrainedPencil::new(&a, &e, 1, 1, o).is_err());
    let mut o = options(3);
    o.equation_absolute.pop();
    assert!(ConstrainedPencil::new(&a, &e, 1, 1, o).is_err());
    let mut bad = a.clone();
    bad.add(2, 2, 1.).unwrap();
    assert!(ConstrainedPencil::new(&bad, &e, 1, 1, options(3)).is_err());
    let mut bad = a.clone();
    bad.add(0, 1, 1.).unwrap();
    assert!(ConstrainedPencil::new(&bad, &e, 1, 1, options(3)).is_err());
    let mut bad = e.clone();
    bad.add(0, 1, 1.).unwrap();
    assert!(ConstrainedPencil::new(&a, &bad, 1, 1, options(3)).is_err());
}

#[test]
fn nonfinite_duplicate_overflow_and_residual_overflow_never_return_a_point() {
    let (a, e) = operators();
    for value in [f64::NAN, f64::INFINITY] {
        let mut bad = a.clone();
        bad.add(0, 0, value).unwrap();
        assert!(ConstrainedPencil::new(&bad, &e, 1, 1, options(3)).is_err());
        let mut bad = e.clone();
        bad.add(0, 0, value).unwrap();
        assert!(ConstrainedPencil::new(&a, &bad, 1, 1, options(3)).is_err());
    }
    let mut bad = a.clone();
    bad.add(0, 0, f64::MAX).unwrap();
    bad.add(0, 0, f64::MAX).unwrap();
    assert!(
        ConstrainedPencil::new(&bad, &e, 1, 1, options(3))
            .err()
            .unwrap()
            .to_string()
            .contains("overflow")
    );
    let m = model();
    assert!(m.reconstruct(&Vector::zeros(0), &ramp(0.)).is_err());
    assert!(
        m.reconstruct(&Vector::from_slice(&[f64::NAN]), &ramp(0.))
            .is_err()
    );
    for field in 0..3 {
        let mut j = ramp(0.);
        match field {
            0 => j.value = Vector::zeros(2),
            1 => j.first = Vector::zeros(2),
            _ => j.constraint_second = Vector::zeros(2),
        }
        assert!(m.reconstruct(&Vector::zeros(1), &j).is_err());
        let mut j = ramp(0.);
        match field {
            0 => j.value.as_mut_slice()[0] = f64::NAN,
            1 => j.first.as_mut_slice()[0] = f64::NAN,
            _ => j.constraint_second.as_mut_slice()[0] = f64::NAN,
        }
        assert!(m.reconstruct(&Vector::zeros(1), &j).is_err());
    }
    assert!(
        m.reconstruct(&Vector::from_slice(&[f64::MAX]), &ramp(0.))
            .is_err()
    );
    let mut p = m.reconstruct(&Vector::zeros(1), &ramp(0.)).unwrap();
    p.state.as_mut_slice()[0] = f64::MAX;
    assert!(m.residuals(&p, &ramp(0.)).is_err());
    p.state = Vector::zeros(2);
    assert!(m.residuals(&p, &ramp(0.)).is_err());
}

#[test]
fn default_index_one_runtime_guard_is_unchanged_for_the_prototype_class() {
    let (a, e) = operators();
    let err = LinearDae::new(&a, &e).err().unwrap().to_string();
    assert!(err.contains("higher-index/singular"), "{err}");
    model(); // Only explicit experimental preparation succeeds.
}

#[test]
fn independently_integrated_reduced_rhs_reproduces_smooth_constrained_solution() {
    let m = model();
    let rhs = |t, z| {
        m.reconstruct(&Vector::from_slice(&[z]), &ramp(t))
            .unwrap()
            .derivative
            .as_slice()[1]
    };
    let mut z = 0.;
    let h = 0.01;
    let mut worst: f64 = 0.;
    for i in 0..100 {
        let t = i as f64 * h;
        let k1 = rhs(t, z);
        let k2 = rhs(t + h / 2., z + h * k1 / 2.);
        let k3 = rhs(t + h / 2., z + h * k2 / 2.);
        let k4 = rhs(t + h, z + h * k3);
        z += h * (k1 + 2. * k2 + 2. * k3 + k4) / 6.;
        let end = (i + 1) as f64 * h;
        let exact = end / 5. - 4. / 25. + 4. / 25. * (-5. * end / 4.).exp();
        worst = worst.max((z - exact).abs());
        let p = m
            .reconstruct(&Vector::from_slice(&[z]), &ramp(end))
            .unwrap();
        assert!((p.state.as_slice()[2] - (-2. - 3. * end - exact)).abs() < 2e-11);
    }
    assert!(worst < 2e-11);
    println!("test-local RK4 ramp: 100 steps; worst current error={worst:e} A");
}

#[test]
fn prepared_operators_are_owned_snapshots() {
    let (mut a, mut e) = operators();
    let before_a = a.clone();
    let before_e = e.clone();
    let m = ConstrainedPencil::new(&a, &e, 1, 1, options(3)).unwrap();
    assert_eq!(a, before_a);
    assert_eq!(e, before_e);
    a.clear();
    e.clear();
    let p = m.reconstruct(&Vector::zeros(1), &ramp(0.)).unwrap();
    assert_eq!(p.state.as_slice(), &[0., 0., -2.]);
    m.residuals(&p, &ramp(0.)).unwrap();
}
