//! Proves the two solver crates chosen for M2/M3 are wired up and usable.
//!
//! These dependency smoke tests complement the production-interface tests in
//! `linear_solvers.rs` and `dae.rs`:
//!
//! - `faer` factorises a matrix that was stamped through
//!   [`spice_maths::SparseMatrix`], the way MNA assembly will feed it.
//! - `diffsol` integrates an initial-value problem with the `faer` backend,
//!   which is the shape the `.tran` driver needs.
//!
//! If either crate is removed from the workspace manifest, or its backend
//! features stop resolving, this test target stops compiling.

use diffsol::{FaerSparseLU, FaerSparseMat, OdeBuilder, OdeSolverMethod};
use faer::prelude::*;
use faer::sparse::{SparseColMat, Triplet as FaerTriplet};
use spice_maths::SparseMatrix;

/// Conductances of a 1 kΩ / 2 kΩ series pair driven by 1 mA.
const G1: f64 = 1.0 / 1_000.0;
const G2: f64 = 1.0 / 2_000.0;

/// Stamps a resistor divider the way `*Load` routines do — `+g` on the two
/// diagonals, `-g` on the two off-diagonals, plus a ground connection — folds
/// the duplicates, hands the triplets to `faer` and solves `G v = i`.
///
/// `v1 = 3 V` and `v2 = 2 V`: 1 mA through 1 kΩ and 2 kΩ in series.
#[test]
fn faer_sparse_lu_solves_a_stamped_mna_matrix() {
    let mut stamps = SparseMatrix::new(2, 2);
    // R1 between node 1 and node 2.
    stamps.add(0, 0, G1).unwrap();
    stamps.add(0, 1, -G1).unwrap();
    stamps.add(1, 0, -G1).unwrap();
    stamps.add(1, 1, G1).unwrap();
    // R2 from node 2 to ground: a second contribution to (1, 1).
    stamps.add(1, 1, G2).unwrap();
    stamps.fold_duplicates();

    let entries: Vec<FaerTriplet<usize, usize, f64>> = stamps
        .triplets()
        .iter()
        .map(|stamp| FaerTriplet::new(stamp.row, stamp.col, stamp.value))
        .collect();
    let conductance = SparseColMat::<usize, f64>::try_new_from_triplets(2, 2, &entries).unwrap();

    let mut current = Mat::<f64>::zeros(2, 1);
    // I1 = 1 mA injected into node 1.
    current[(0, 0)] = 1e-3;

    let lu = conductance.sp_lu().unwrap();
    lu.solve_in_place(current.as_mut());

    let v1 = current[(0, 0)];
    let v2 = current[(1, 0)];
    assert!((v1 - 3.0).abs() < 1e-9, "node 1 = {v1} V, expected 3 V");
    assert!((v2 - 2.0).abs() < 1e-9, "node 2 = {v2} V, expected 2 V");
}

/// Integrates `dy/dt = -0.1 y`, `y(0) = 1` to `t = 2` with diffsol's BDF solver
/// over `faer`'s sparse representation, and checks it against `exp(-0.2)`.
///
/// This is the implicit path a stiff `.tran` run takes: a Jacobian, a Newton
/// solve per step and adaptive step control.
#[test]
fn diffsol_bdf_solves_exponential_decay_with_the_faer_backend() {
    type M = FaerSparseMat<f64>;

    let problem = OdeBuilder::<M>::new()
        .h0(1.0)
        .rtol(1e-9)
        .atol([1e-12])
        .p([0.1])
        // f(x) = -p[0] * x, and its Jacobian-vector product J v = -p[0] * v.
        .rhs_implicit(
            |x, p, _t, y| {
                for (y, x) in y.iter_mut().zip(x.iter()) {
                    *y = -p[0] * *x;
                }
            },
            |_x, p, _t, v, y| {
                for (y, v) in y.iter_mut().zip(v.iter()) {
                    *y = -p[0] * *v;
                }
            },
        )
        // y(0) = 1.
        .init(|_p, _t, y| y.fill(1.0), 1)
        .build()
        .unwrap();

    let mut solver = problem.bdf::<FaerSparseLU<f64>>().unwrap();

    let t_final = 2.0;
    while solver.state().t <= t_final {
        solver.step().unwrap();
    }
    let y = solver.interpolate(t_final).unwrap();

    let expected = (-0.1 * t_final).exp();
    assert!(
        (y[0] - expected).abs() < 1e-6,
        "y({t_final}) = {}, expected {expected}",
        y[0]
    );
}
