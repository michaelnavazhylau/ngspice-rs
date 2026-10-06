//! Bounded diffsol adaptive BDF adapter for diagonal-mass, index-one linear DAEs.
//!
//! This is NOT ngspice trap or fixed Gear-2. Operators and sparsity are assembled
//! explicitly; no NaN probing, mutable devices, or fallible callbacks are hidden
//! inside diffsol equations. Each source segment is affine and integration is
//! restarted at breakpoints by the analysis layer.
use crate::linear::{numerical, square};
use crate::{SparseLu, SparseMatrix, Vector};
use diffsol::matrix::sparsity::MatrixSparsityRef;
use diffsol::{
    ConstantOp, FaerContext, FaerSparseLU, FaerSparseMat, FaerVec, LinearOp, Matrix as DiffMatrix,
    NonLinearOp, NonLinearOpJacobian, OdeBuilder, OdeEquations, OdeEquationsRef, OdeSolverMethod,
    OdeSolverState, OdeSolverStopReason, Op, Vector as DiffVector,
};
use spice_core::SpiceResult;

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

/// Prepared numeric DAE. Only diagonal E with invertible algebraic A block is
/// supported: floating capacitor networks/higher-index source constraints fail
/// explicitly, before automatic diffsol initialization can mispartition them.
pub struct LinearDae {
    a: SparseMatrix,
    jac: M,
    mass: M,
    algebraic: Vec<usize>,
    algebraic_map: Vec<Option<usize>>,
    mass_diagonal: Vec<f64>,
    algebraic_lu: Option<SparseLu>,
}
impl LinearDae {
    /// Validates mass structure, numeric coefficients and index-one solvability.
    /// # Errors
    /// Empty/mismatched/nonfinite operators, off-diagonal E, no dynamic rows,
    /// or a singular algebraic block.
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
        if e.triplets().iter().any(|t| t.row != t.col) {
            return Err(numerical(
                "linear DAE",
                "floating/coupled capacitor mass is not supported by this bounded backend",
            ));
        }
        let algebraic: Vec<_> = (0..a.rows()).filter(|r| e.get(*r, *r) == 0.).collect();
        if algebraic.len() == a.rows() {
            return Err(numerical(
                "linear DAE",
                "at least one dynamic row is required",
            ));
        }
        let mut map = vec![None; a.rows()];
        for (i, r) in algebraic.iter().enumerate() {
            map[*r] = Some(i);
        }
        let algebraic_lu = if algebraic.is_empty() {
            None
        } else {
            let mut aa = SparseMatrix::new(algebraic.len(), algebraic.len());
            for t in a.triplets() {
                if let (Some(r), Some(c)) = (map[t.row], map[t.col]) {
                    aa.add(r, c, t.value)?;
                }
            }
            Some(aa.factorize().map_err(|e| {
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
            mass_diagonal: (0..a.rows()).map(|i| e.get(i, i)).collect(),
            a,
            algebraic,
            algebraic_map: map,
            algebraic_lu,
        })
    }

    /// Projects only algebraic unknowns; dynamic states are preserved at jumps.
    /// # Errors
    /// Invalid vectors or failure of algebraic solve.
    pub fn project(&self, x: &Vector, b: &Vector) -> SpiceResult<Vector> {
        self.check_vector(x)?;
        self.check_vector(b)?;
        let mut x = x.clone();
        if let Some(lu) = &self.algebraic_lu {
            let mut rhs = Vector::from_slice(
                &self
                    .algebraic
                    .iter()
                    .map(|r| b.as_slice()[*r])
                    .collect::<Vec<_>>(),
            );
            for t in self.a.triplets() {
                if let Some(i) = self.algebraic_map[t.row]
                    && self.algebraic_map[t.col].is_none()
                {
                    rhs.as_mut_slice()[i] -= t.value * x.as_slice()[t.col];
                }
            }
            let solved = lu.solve(&rhs)?;
            for (i, r) in self.algebraic.iter().enumerate() {
                x.as_mut_slice()[*r] = solved.as_slice()[i];
            }
        }
        self.check_vector(&x)?;
        Ok(x)
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
        let mut state = problem.bdf_state::<FaerSparseLU<f64>>().map_err(err)?;
        // diffsol's consistent initializer sets algebraic derivatives to zero.
        // In MNA a source branch current can have nonzero derivative at restart
        // (e.g. RL). Compute dynamic derivatives and differentiate the algebraic
        // constraints; otherwise tight current tolerances force h below h_min.
        let ax = self.a.mul_vector(&segment.initial)?;
        let mut dy = Vector::zeros(n);
        let mut db = Vector::zeros(n);
        for i in 0..n {
            if self.mass_diagonal[i] != 0. {
                dy.as_mut_slice()[i] =
                    (segment.b_start.as_slice()[i] - ax.as_slice()[i]) / self.mass_diagonal[i];
            }
            db.as_mut_slice()[i] = (segment.b_end.as_slice()[i] - segment.b_start.as_slice()[i])
                / (segment.end - segment.start);
        }
        let dy = self.project(&dy, &db)?;
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
