//! Bounded diffsol adaptive BDF adapter for index-one linear DAEs
//! `E x' + A x = b(t)`, including floating and coupled capacitor networks.
//!
//! This is NOT ngspice trap or fixed Gear-2. Operators and sparsity are assembled
//! explicitly; no NaN probing, mutable devices, or fallible callbacks are hidden
//! inside diffsol equations. Each source segment is affine and integration is
//! restarted at breakpoints by the analysis layer.
//!
//! # Index-one formulation
//!
//! Integration runs in the physical MNA coordinates, so per-unknown voltage and
//! current tolerances keep their meaning; diffsol receives the sparse `E`
//! unchanged (BDF is invariant under constant linear coordinate changes). Only
//! the structural analysis and initialization use a transformation:
//!
//! 1. `E` is split into the connected components of its petgraph coupling
//!    graph. Rows without mass are 1×1 zero blocks; a diagonal entry is a 1×1
//!    block; floating/coupled capacitors form larger blocks, each rank-revealed
//!    by a dense SVD with tolerance `64 m ε σ_max` (blocks above
//!    [`MAX_MASS_BLOCK`] rows are rejected).
//! 2. The null vectors give bases `N` of `ker E` and `W` of `ker Eᵀ`. The pencil
//!    is index one exactly when `Wᵀ A N` is nonsingular; this is factored with
//!    the existing numerical sparse rank guard. Tested higher-index loops and
//!    nonunique nullspaces are rejected rather than accepted on a zero residual.
//!    The aggregate-certification proof caveat is documented separately in
//!    `docs/port/SPARSE_RANK_DIAGNOSTICS.md`; this adapter does not resolve it.
//! 3. Consistent states satisfy `Wᵀ (b - A x) = 0`. [`LinearDae::project`] moves
//!    only along `N`, so `E x` — capacitor charges, inductor fluxes — is exactly
//!    preserved across source events. Consistent derivatives use the block
//!    pseudo-inverse of `E` plus the differentiated constraint.
//!
//! For diagonal `E` this reduces to the earlier algebraic-block formulation.
/// Opt-in numeric formulation experiment; not used by the index-one adapter.
/// Production higher-index pencils continue to fail the existing runtime guard.
pub mod higher_index;

use crate::linear::{numerical, square};
use crate::{SparseLu, SparseMatrix, Vector};
use diffsol::matrix::sparsity::MatrixSparsityRef;
use diffsol::{
    BdfState, ConstantOp, FaerContext, FaerSparseLU, FaerSparseMat, FaerVec, LinearOp,
    Matrix as DiffMatrix, NonLinearOp, NonLinearOpJacobian, OdeBuilder, OdeEquations,
    OdeEquationsRef, OdeSolverMethod, OdeSolverState, OdeSolverStopReason, Op,
    Vector as DiffVector,
};
use spice_core::SpiceResult;

/// Largest coupled mass block (rows) analysed with a dense SVD.
pub const MAX_MASS_BLOCK: usize = 512;

type M = FaerSparseMat<f64>;
type V = FaerVec<f64>;

struct Equations {
    jac: M,
    mass: M,
    init: V,
    b0: Vec<f64>,
    b1: Vec<f64>,
    start: f64,
    end: f64,
}
impl Op for Equations {
    type T = f64;
    type M = M;
    type V = V;
    type C = FaerContext;
    fn context(&self) -> &Self::C {
        self.jac.context()
    }
    fn nstates(&self) -> usize {
        self.b0.len()
    }
    fn nout(&self) -> usize {
        self.nstates()
    }
    fn nparams(&self) -> usize {
        0
    }
}
impl NonLinearOp for Equations {
    fn call_inplace(&self, x: &V, t: f64, out: &mut V) {
        let f = ((t - self.start) / (self.end - self.start)).clamp(0., 1.);
        for i in 0..self.b0.len() {
            out.set_index(i, (1. - f) * self.b0[i] + f * self.b1[i]);
        }
        self.jac.gemv(1., x, 1., out);
    }
}
impl NonLinearOpJacobian for Equations {
    fn jac_mul_inplace(&self, _x: &V, _t: f64, v: &V, out: &mut V) {
        self.jac.gemv(1., v, 0., out);
    }
    fn jacobian_inplace(&self, _x: &V, _t: f64, out: &mut M) {
        out.copy_from(&self.jac);
    }
    fn jacobian_sparsity(&self) -> Option<<M as DiffMatrix>::Sparsity> {
        self.jac
            .sparsity()
            .map(|s| MatrixSparsityRef::<M>::to_owned(&s))
    }
}
impl LinearOp for Equations {
    fn gemv_inplace(&self, v: &V, _t: f64, beta: f64, out: &mut V) {
        self.mass.gemv(1., v, beta, out);
    }
    fn matrix_inplace(&self, _t: f64, out: &mut M) {
        out.copy_from(&self.mass);
    }
    fn sparsity(&self) -> Option<<M as DiffMatrix>::Sparsity> {
        self.mass
            .sparsity()
            .map(|s| MatrixSparsityRef::<M>::to_owned(&s))
    }
}
impl ConstantOp for Equations {
    fn call_inplace(&self, _t: f64, out: &mut V) {
        out.copy_from(&self.init);
    }
}
impl<'a> OdeEquationsRef<'a> for Equations {
    type Rhs = &'a Self;
    type Mass = &'a Self;
    type Init = &'a Self;
    type Root = &'a Self;
    type Out = &'a Self;
    type Reset = &'a Self;
}
impl OdeEquations for Equations {
    fn rhs(&self) -> &Self {
        self
    }
    fn mass(&self) -> Option<&Self> {
        Some(self)
    }
    fn init(&self) -> &Self {
        self
    }
    fn set_params(&mut self, _p: &V) {}
    fn get_params(&self, _p: &mut V) {}
}

fn backend(matrix: &SparseMatrix, sign: f64) -> SpiceResult<M> {
    let indices = matrix.triplets().iter().map(|t| (t.row, t.col)).collect();
    let values = matrix.triplets().iter().map(|t| sign * t.value).collect();
    M::try_from_triplets(
        matrix.rows(),
        matrix.cols(),
        indices,
        values,
        FaerContext::default(),
    )
    .map_err(|e| numerical("diffsol matrix construction", e.to_string()))
}
fn vector(v: &V) -> Vector {
    Vector::from_slice(&(0..v.len()).map(|i| v.get_index(i)).collect::<Vec<_>>())
}

/// A connected block of the mass matrix and its pseudo-inverse.
#[derive(Debug, Clone)]
struct MassBlock {
    rows: Vec<usize>,
    /// Row-major `m x m` pseudo-inverse of the block.
    pseudo_inverse: Vec<f64>,
}

/// Rank-revealed structure of `E` (see the module documentation).
#[derive(Debug, Clone, Default)]
struct MassStructure {
    blocks: Vec<MassBlock>,
    /// Sparse rows of `N`: `(null coordinate, weight)` per physical unknown.
    right_null: Vec<Vec<(usize, f64)>>,
    /// Sparse rows of `W`.
    left_null: Vec<Vec<(usize, f64)>>,
    null_dim: usize,
}

fn dae_error(message: impl Into<String>) -> spice_core::SpiceError {
    numerical("linear DAE", message.into())
}

impl MassStructure {
    fn new(e: &SparseMatrix) -> SpiceResult<Self> {
        let n = e.rows();
        let graph = e.coupling_graph()?;
        let mut components = petgraph::algo::kosaraju_scc(&graph);
        for component in &mut components {
            component.sort_unstable();
        }
        components.sort_unstable_by_key(|component| component[0]);
        let mut local = vec![(0, 0); n];
        for (block, rows) in components.iter().enumerate() {
            for (index, row) in rows.iter().enumerate() {
                local[*row] = (block, index);
            }
        }
        let mut dense: Vec<Vec<f64>> = components
            .iter()
            .map(|r| vec![0.; r.len().pow(2)])
            .collect();
        for t in e.triplets() {
            let (block, row) = local[t.row];
            let (other, col) = local[t.col];
            debug_assert_eq!(block, other);
            let m = components[block].len();
            dense[block][row * m + col] += t.value;
        }
        let mut structure = Self {
            right_null: vec![vec![]; n],
            left_null: vec![vec![]; n],
            ..Self::default()
        };
        for (rows, values) in components.into_iter().zip(dense) {
            structure.push_block(rows, &values)?;
        }
        Ok(structure)
    }

    fn push_null(
        &mut self,
        rows: &[usize],
        right: impl Fn(usize) -> f64,
        left: impl Fn(usize) -> f64,
    ) {
        let coordinate = self.null_dim;
        self.null_dim += 1;
        for (i, row) in rows.iter().enumerate() {
            self.right_null[*row].push((coordinate, right(i)));
            self.left_null[*row].push((coordinate, left(i)));
        }
    }

    fn push_block(&mut self, rows: Vec<usize>, values: &[f64]) -> SpiceResult<()> {
        let m = rows.len();
        if m == 1 {
            let value = values[0];
            let pseudo_inverse = if value == 0. {
                self.push_null(&rows, |_| 1., |_| 1.);
                0.
            } else {
                1. / value
            };
            if !pseudo_inverse.is_finite() {
                return Err(dae_error("mass entry has no finite inverse"));
            }
            self.blocks.push(MassBlock {
                rows,
                pseudo_inverse: vec![pseudo_inverse],
            });
            return Ok(());
        }
        if m > MAX_MASS_BLOCK {
            return Err(dae_error(format!(
                "coupled mass block of {m} unknowns exceeds the dense analysis limit {MAX_MASS_BLOCK}"
            )));
        }
        let matrix = faer::Mat::<f64>::from_fn(m, m, |i, j| values[i * m + j]);
        let svd = matrix
            .svd()
            .map_err(|e| dae_error(format!("mass block SVD failed: {e:?}")))?;
        let (u, sigma, v) = (svd.U(), svd.S().column_vector(), svd.V());
        let largest = (0..m).map(|i| sigma[i]).fold(0., f64::max);
        if !largest.is_finite() {
            return Err(dae_error("nonfinite mass block singular values"));
        }
        let tolerance = 64. * (m as f64) * f64::EPSILON * largest;
        let mut pseudo_inverse = vec![0.; m * m];
        for k in 0..m {
            if sigma[k] > tolerance {
                for i in 0..m {
                    for j in 0..m {
                        pseudo_inverse[i * m + j] += v[(i, k)] * u[(j, k)] / sigma[k];
                    }
                }
            } else {
                self.push_null(&rows, |i| v[(i, k)], |i| u[(i, k)]);
            }
        }
        if pseudo_inverse.iter().any(|value| !value.is_finite()) {
            return Err(dae_error("nonfinite mass pseudo-inverse"));
        }
        self.blocks.push(MassBlock {
            rows,
            pseudo_inverse,
        });
        Ok(())
    }

    /// `Wᵀ A N`, the constraint Jacobian on the algebraic subspace.
    fn constraint_matrix(&self, a: &SparseMatrix) -> SpiceResult<SparseMatrix> {
        let mut matrix = SparseMatrix::new(self.null_dim, self.null_dim);
        for t in a.triplets() {
            for (row, w) in &self.left_null[t.row] {
                for (col, v) in &self.right_null[t.col] {
                    matrix.add(*row, *col, w * t.value * v)?;
                }
            }
        }
        matrix.fold_duplicates();
        if matrix.triplets().iter().any(|t| !t.value.is_finite()) {
            return Err(dae_error("constraint assembly overflow"));
        }
        Ok(matrix)
    }

    /// Minimum-norm `p` with `E p = r` on the range of `E`.
    fn mass_solve(&self, r: &[f64]) -> Vector {
        let mut p = Vector::zeros(r.len());
        for block in &self.blocks {
            let m = block.rows.len();
            for (i, row) in block.rows.iter().enumerate() {
                p.as_mut_slice()[*row] = block
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(j, col)| block.pseudo_inverse[i * m + j] * r[*col])
                    .sum();
            }
        }
        p
    }
}

/// Prepared numeric DAE for the index-one subset described in the module
/// documentation. Higher-index constraints, singular pencils and oversized
/// coupled mass blocks fail explicitly, before diffsol integration starts.
pub struct LinearDae {
    a: SparseMatrix,
    jac: M,
    mass: M,
    structure: MassStructure,
    constraint_lu: Option<SparseLu>,
}
impl LinearDae {
    /// Validates operators, the mass structure and index-one solvability.
    /// # Errors
    /// Empty/mismatched/nonfinite operators, no dynamic unknowns, oversized
    /// coupled mass blocks, or a singular constraint block `Wᵀ A N`
    /// (higher-index or nonunique).
    pub fn new(a: &SparseMatrix, e: &SparseMatrix) -> SpiceResult<Self> {
        square(a.rows(), a.cols())?;
        if a.rows() != e.rows() || a.cols() != e.cols() {
            return Err(numerical("linear DAE", "operator dimensions differ"));
        }
        let mut a = a.clone();
        let mut e = e.clone();
        if a.triplets()
            .iter()
            .chain(e.triplets())
            .any(|t| !t.value.is_finite())
        {
            return Err(numerical("linear DAE", "non-finite coefficient"));
        }
        a.fold_duplicates();
        e.fold_duplicates();
        if a.triplets()
            .iter()
            .chain(e.triplets())
            .any(|t| !t.value.is_finite())
        {
            return Err(numerical("linear DAE", "assembly overflow"));
        }
        let structure = MassStructure::new(&e)?;
        if structure.null_dim == a.rows() {
            return Err(numerical(
                "linear DAE",
                "at least one dynamic row is required",
            ));
        }
        let constraint_lu = if structure.null_dim == 0 {
            None
        } else {
            Some(structure.constraint_matrix(&a)?.factorize().map_err(|e| {
                numerical(
                    "linear DAE",
                    "unsupported higher-index/singular algebraic constraints: ".to_owned()
                        + &e.to_string(),
                )
            })?)
        };
        Ok(Self {
            jac: backend(&a, -1.)?,
            mass: backend(&e, 1.)?,
            a,
            structure,
            constraint_lu,
        })
    }

    /// Rank of `E`: the number of differential degrees of freedom.
    #[must_use]
    pub fn differential_dimension(&self) -> usize {
        self.a.rows() - self.structure.null_dim
    }

    /// Projects onto the constraints `Wᵀ (b - A x) = 0` along `ker E`, so `E x`
    /// (charges and fluxes) is unchanged; for diagonal `E` only algebraic
    /// unknowns move.
    /// # Errors
    /// Invalid vectors or failure of the constraint solve.
    pub fn project(&self, x: &Vector, b: &Vector) -> SpiceResult<Vector> {
        self.check_vector(x)?;
        self.check_vector(b)?;
        let mut x = x.clone();
        if let Some(lu) = &self.constraint_lu {
            let ax = self.a.mul_vector(&x)?;
            let mut rhs = Vector::zeros(self.structure.null_dim);
            for (row, weights) in self.structure.left_null.iter().enumerate() {
                let residual = b.as_slice()[row] - ax.as_slice()[row];
                for (coordinate, w) in weights {
                    rhs.as_mut_slice()[*coordinate] += w * residual;
                }
            }
            let delta = lu.solve(&rhs)?;
            for (row, weights) in self.structure.right_null.iter().enumerate() {
                for (coordinate, v) in weights {
                    x.as_mut_slice()[row] += v * delta.as_slice()[*coordinate];
                }
            }
        }
        self.check_vector(&x)?;
        Ok(x)
    }

    /// A consistent derivative at consistent `x` with forcing `b` and forcing
    /// slope `db`: `E x' = b - A x` and `Wᵀ (db - A x') = 0`.
    fn derivative(&self, x: &Vector, b: &Vector, db: &Vector) -> SpiceResult<Vector> {
        let ax = self.a.mul_vector(x)?;
        let residual: Vec<f64> = b
            .as_slice()
            .iter()
            .zip(ax.as_slice())
            .map(|(b, ax)| b - ax)
            .collect();
        self.project(&self.structure.mass_solve(&residual), db)
    }
    fn check_vector(&self, x: &Vector) -> SpiceResult<()> {
        if x.len() != self.a.rows() || !x.is_finite() {
            return Err(numerical("linear DAE", "vector dimension/non-finite input"));
        }
        Ok(())
    }

    /// Integrates one affine source segment. Samples must be strictly increasing
    /// inside (start,end]. The callback runs ONLY after accepted internal steps,
    /// not at interpolation samples or rejected Newton trials.
    /// # Errors
    /// Invalid initial state/options, progress/work limits, or backend failures.
    pub fn integrate_segment(
        &self,
        segment: &DaeSegment,
        options: &BdfOptions,
        accepted: &mut impl FnMut(f64, &Vector) -> SpiceResult<()>,
    ) -> SpiceResult<DaeOutput> {
        self.check_vector(&segment.initial)?;
        self.check_vector(&segment.b_start)?;
        self.check_vector(&segment.b_end)?;
        let n = self.a.rows();
        if !segment.start.is_finite()
            || !segment.end.is_finite()
            || segment.end <= segment.start
            || segment.start < 0.
            || !options.rtol.is_finite()
            || options.rtol <= 0.
            || !options.max_step.is_finite()
            || options.max_step <= 0.
            || options.atol.len() != n
            || options.atol.iter().any(|v| !v.is_finite() || *v <= 0.)
            || options.max_steps == 0
            || segment
                .samples
                .iter()
                .any(|t| !t.is_finite() || *t <= segment.start || *t > segment.end)
            || segment.samples.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(numerical(
                "diffsol BDF",
                "invalid time grid/tolerances/limits",
            ));
        }
        let projected = self.project(&segment.initial, &segment.b_start)?;
        if projected
            .as_slice()
            .iter()
            .zip(segment.initial.as_slice())
            .zip(&options.atol)
            .any(|((a, b), tol)| (a - b).abs() > *tol + options.rtol * b.abs())
        {
            return Err(numerical(
                "diffsol initialization",
                "inconsistent algebraic initial conditions",
            ));
        }
        let eqn = Equations {
            jac: self.jac.clone(),
            mass: self.mass.clone(),
            init: V::from_vec(segment.initial.as_slice().to_vec(), FaerContext::default()),
            b0: segment.b_start.as_slice().to_vec(),
            b1: segment.b_end.as_slice().to_vec(),
            start: segment.start,
            end: segment.end,
        };
        let err = |e: diffsol::DiffsolError| numerical("diffsol BDF", e.to_string());
        let problem = OdeBuilder::<M>::new()
            .t0(segment.start)
            .h0(options.max_step.min(segment.end - segment.start) * 0.01)
            .rtol(options.rtol)
            .atol(options.atol.clone())
            .build_from_eqn(eqn)
            .map_err(err)?;
        // Bypass diffsol's zero-diagonal consistent initializer: it cannot
        // partition coupled mass matrices, and it sets algebraic derivatives
        // to zero, while in MNA a source branch current can have a nonzero
        // derivative at restart (e.g. RL), which forces h below h_min under
        // tight current tolerances. Use the structural derivative instead.
        let mut state = BdfState::new_without_initialise(&problem).map_err(err)?;
        let mut db = Vector::zeros(n);
        for i in 0..n {
            db.as_mut_slice()[i] = (segment.b_end.as_slice()[i] - segment.b_start.as_slice()[i])
                / (segment.end - segment.start);
        }
        let dy = self.derivative(&segment.initial, &segment.b_start, &db)?;
        {
            let s = state.as_mut();
            s.y.copy_from(&V::from_vec(
                segment.initial.as_slice().to_vec(),
                FaerContext::default(),
            ));
            s.dy.copy_from(&V::from_vec(dy.as_slice().to_vec(), FaerContext::default()));
            *s.h = options.max_step.min(segment.end - segment.start) * 0.001;
        }
        state.initialise_diff_to_first_order();
        let mut solver = problem
            .bdf_solver::<FaerSparseLU<f64>>(state)
            .map_err(err)?;
        let mut samples = vec![];
        let mut sample = 0;
        let mut steps = 0;
        while solver.state().t < segment.end {
            if steps >= options.max_steps {
                return Err(numerical(
                    "diffsol BDF",
                    "accepted-step work limit exceeded",
                ));
            }
            let old = solver.state().t;
            let stop = (old + options.max_step).min(segment.end);
            if stop <= old {
                return Err(numerical(
                    "diffsol BDF",
                    "maximum step makes no floating-point progress",
                ));
            }
            solver.set_stop_time(stop).map_err(err)?;
            let reason = solver.step().map_err(err)?;
            steps += 1;
            let now = solver.state().t;
            if !now.is_finite() || now <= old || now > stop + 16. * f64::EPSILON * stop.abs() {
                return Err(numerical(
                    "diffsol BDF",
                    "invalid integration progress/stop time",
                ));
            }
            match reason {
                OdeSolverStopReason::InternalTimestep | OdeSolverStopReason::TstopReached => {}
                _ => return Err(numerical("diffsol BDF", "unexpected root stop")),
            }
            let x = vector(solver.state().y);
            self.check_vector(&x)?;
            accepted(now, &x)?;
            let reached_end = reason == OdeSolverStopReason::TstopReached && stop == segment.end;
            let sample_limit = if reached_end { segment.end } else { now };
            while sample < segment.samples.len() && segment.samples[sample] <= sample_limit {
                let t = segment.samples[sample];
                // diffsol may report a stop within its roundoff tolerance. Never
                // request interpolation past its actual state time.
                let x = if t > now {
                    vector(solver.state().y)
                } else {
                    vector(&solver.interpolate(t).map_err(err)?)
                };
                self.check_vector(&x)?;
                samples.push((t, x));
                sample += 1;
            }
            if reached_end {
                break;
            }
        }
        if sample != segment.samples.len() {
            return Err(numerical("diffsol BDF", "final samples were not reached"));
        }
        Ok(DaeOutput {
            samples,
            final_state: vector(solver.state().y),
            steps,
        })
    }
}

/// Explicit adaptive BDF settings. No trap/Gear/maxord remapping.
#[derive(Debug, Clone)]
pub struct BdfOptions {
    /// Relative error tolerance.
    pub rtol: f64,
    /// Component-wise absolute tolerances (volts and branch amps differ).
    pub atol: Vec<f64>,
    /// Maximum accepted/internal step, enforced by stop times.
    pub max_step: f64,
    /// Maximum accepted steps in this segment.
    pub max_steps: usize,
}
/// One continuous, affine forcing interval (never spans a discontinuity).
pub struct DaeSegment {
    /// Interval start.
    pub start: f64,
    /// Interval end.
    pub end: f64,
    /// Consistent initial state.
    pub initial: Vector,
    /// Forcing at start, from the right.
    pub b_start: Vector,
    /// Forcing at end, from the left.
    pub b_end: Vector,
    /// Requested sample grid, separate from adaptive steps.
    pub samples: Vec<f64>,
}
/// Segment results.
pub struct DaeOutput {
    /// Requested samples only.
    pub samples: Vec<(f64, Vector)>,
    /// Accepted state at the interval end (left limit).
    pub final_state: Vector,
    /// Accepted work used.
    pub steps: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mass_preserves_beta_including_algebraic_rows() {
        let mut e = SparseMatrix::new(2, 2);
        e.add(0, 0, 2.).unwrap();
        let eqn = Equations {
            jac: backend(&e, -1.).unwrap(),
            mass: backend(&e, 1.).unwrap(),
            init: V::zeros(2, FaerContext::default()),
            b0: vec![0.; 2],
            b1: vec![0.; 2],
            start: 0.,
            end: 1.,
        };
        let x = V::from_vec(vec![3., 7.], FaerContext::default());
        let mut y = V::from_vec(vec![5., 11.], FaerContext::default());
        eqn.gemv_inplace(&x, 0., 4., &mut y);
        assert_eq!(vector(&y).as_slice(), &[26., 44.]);
    }
}
