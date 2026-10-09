//! Experimental, numeric-only index-two reduction; never selected by `LinearDae`.
//!
//! See `docs/port/HIGHER_INDEX_DAE_ADR.md` for the fully voltage-constrained
//! block class, physical ordering, impulse exclusions and production gate.
//! Continuous charge/flux and current signs follow upstream `CAPload`
//! (`src/spicelib/devices/cap/capload.c`) and `INDload`
//! (`src/spicelib/devices/ind/indload.c`); this reduction deliberately does not
//! reproduce their companion discretization or `DCtran` event scheduling.
use crate::maths::{DenseLu, Matrix, SparseMatrix, Vector};
use crate::primitives::{SpiceError, SpiceResult};

/// Hard dense-prototype budget, including voltage and branch-current unknowns.
pub const MAX_UNKNOWNS: usize = 32;

fn error(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "higher-index prototype".into(),
        message: message.into(),
    }
}

/// Physical-coordinate and original-equation residual tolerances.
/// No time integration or accepted-state policy is implied.
#[derive(Debug, Clone)]
pub struct ProjectionOptions {
    /// Relative bound, strictly positive and at most 1e-6.
    pub relative: f64,
    /// Absolute state/IC bounds, one per physical unknown (V or A).
    pub state_absolute: Vec<f64>,
    /// Absolute equation bounds, one per original row (A or V).
    /// Also used for differentiated rows, in A/s or V/s.
    pub equation_absolute: Vec<f64>,
    /// Allocation/work bound, in 1..=MAX_UNKNOWNS.
    pub max_unknowns: usize,
}

/// Explicit smooth forcing data at one time. The caller must supply actual
/// derivatives on a C2 interval, not finite differences across a source jump.
#[derive(Debug, Clone)]
pub struct ForcingJet {
    /// Original forcing `b`, in original row order.
    pub value: Vector,
    /// First derivative `b'`, in original row order.
    pub first: Vector,
    /// Second derivative of the voltage constraints only (last n rows).
    pub constraint_second: Vector,
}

/// Reconstructed physical state and its unique consistent derivative.
#[derive(Debug, Clone)]
pub struct ConsistentPoint {
    /// `[q, z, lambda]`: voltages, inductor currents, voltage-source currents.
    pub state: Vector,
    /// Derivatives of every physical unknown, including source currents.
    pub derivative: Vector,
    /// Second derivatives of `[q,z]` only. No fictitious lambda'' is supplied.
    pub dynamic_acceleration: Vector,
}

/// Residual evidence in original units, never a transformed-equation norm.
#[derive(Debug, Clone)]
pub struct ResidualReport {
    /// `E x' + A x - b`.
    pub original: Vector,
    /// `E x'' + A x' - b'` (E annihilates source-current coordinates).
    pub differentiated: Vector,
    /// `H^T q'' - s''`.
    pub constraint_second: Vector,
    /// Largest residual / (absolute + relative * sum of absolute terms).
    pub max_bound_ratio: f64,
}

/// Owned, fallible reduction of the bounded block pencil
///
/// ```text
/// C q' + G q + B z + H lambda = f
/// -L z' + B^T q - R z         = g
/// H^T q                      = s
/// ```
///
/// C and L are positive diagonal; H is square and nonsingular. All q are
/// constrained, while z are the free ODE coordinates. n,k must both be positive.
/// The matrix input is numeric, not a device-stamping or topology API.
/// Positive lambda and z follow their incidence column (first to second terminal).
pub struct ConstrainedPencil {
    a: Matrix,
    e: Matrix,
    n: usize,
    k: usize,
    h: DenseLu,
    ht: DenseLu,
    l: DenseLu,
    options: ProjectionOptions,
}

impl ConstrainedPencil {
    /// Validate and snapshot operators; check rank with existing dense pivot guards.
    ///
    /// # Errors
    /// Empty/dimension/budget/tolerance/finite/duplicate-overflow errors,
    /// unsupported block structure or redundant/numerically unresolved H.
    /// Singular H is refused even for homogeneous forcing: residual zero does
    /// not establish unique source currents. Partial constraints are unsupported.
    pub fn new(
        a: &SparseMatrix,
        e: &SparseMatrix,
        voltages: usize,
        inductors: usize,
        options: ProjectionOptions,
    ) -> SpiceResult<Self> {
        let total = voltages
            .checked_mul(2)
            .and_then(|v| v.checked_add(inductors));
        let Some(total) = total else {
            return Err(error("dimension overflow"));
        };
        if voltages == 0
            || inductors == 0
            || a.rows() != total
            || a.cols() != total
            || e.rows() != total
            || e.cols() != total
        {
            return Err(error(
                "empty or unsupported operator dimensions; require 2*n+k",
            ));
        }
        if options.max_unknowns == 0
            || options.max_unknowns > MAX_UNKNOWNS
            || total > options.max_unknowns
        {
            return Err(error("dense prototype budget exceeded/invalid"));
        }
        if !options.relative.is_finite()
            || options.relative <= 0.
            || options.relative > 1e-6
            || options.state_absolute.len() != total
            || options.equation_absolute.len() != total
            || options
                .state_absolute
                .iter()
                .chain(&options.equation_absolute)
                .any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err(error("invalid tolerance dimensions/values"));
        }
        let a = assembled(a)?;
        let e = assembled(e)?;
        let n = voltages;
        let k = inductors;
        for r in 0..total {
            for c in 0..total {
                let mass = e.data()[r * total + c];
                let allowed = r == c && r < n + k;
                if (!allowed && mass != 0.)
                    || (r == c && r < n && mass <= 0.)
                    || (r == c && (n..n + k).contains(&r) && mass >= 0.)
                {
                    return Err(error("unsupported mass; require positive diagonal C and L"));
                }
                if r >= n && c >= n && (r >= n + k || c >= n + k) && a.data()[r * total + c] != 0. {
                    return Err(error("unsupported branch/constraint coupling"));
                }
                if r < n && c >= n && a.data()[r * total + c] != a.data()[c * total + r] {
                    return Err(error("incidence blocks must be transposes"));
                }
            }
        }
        let mut h = Matrix::zeros(n, n);
        let mut l = Matrix::zeros(k, k);
        for r in 0..n {
            for c in 0..n {
                h.set(r, c, a.data()[r * total + n + k + c])?;
            }
        }
        for r in 0..k {
            l.set(r, r, -e.data()[(n + r) * total + n + r])?;
        }
        let hf = h.lu_decompose().map_err(|e| error(format!(
            "redundant/rank-deficient or numerically unresolved constraints; inconsistent forcing also unsupported: {e}"
        )))?;
        Ok(Self {
            a,
            e,
            n,
            k,
            h: hf,
            ht: h.transpose().lu_decompose()?,
            l: l.lu_decompose()?,
            options,
        })
    }

    fn check(&self, v: &Vector, len: usize) -> SpiceResult<()> {
        if v.len() != len || !v.is_finite() {
            return Err(error(
                "vector dimension/nonfinite input or arithmetic overflow",
            ));
        }
        Ok(())
    }

    fn check_jet(&self, jet: &ForcingJet) -> SpiceResult<()> {
        self.check(&jet.value, self.a.rows())?;
        self.check(&jet.first, self.a.rows())?;
        self.check(&jet.constraint_second, self.n)
    }

    /// Reconstruct a consistent point from free inductor currents and a smooth
    /// forcing jet. Solve the reduced ODE RHS and differentiated reconstruction;
    /// certify original and hidden equations before returning any point.
    ///
    /// # Errors
    /// Invalid inputs, solve/rank/overflow failures or residual violations.
    pub fn reconstruct(&self, currents: &Vector, jet: &ForcingJet) -> SpiceResult<ConsistentPoint> {
        self.check(currents, self.k)?;
        self.check_jet(jet)?;
        let d = self.a.rows();
        let b = jet.value.as_slice();
        let db = jet.first.as_slice();
        let q = self.ht.solve(&Vector::from_slice(&b[self.n + self.k..]))?;
        let dq = self.ht.solve(&Vector::from_slice(&db[self.n + self.k..]))?;
        let ddq = self.ht.solve(&jet.constraint_second)?;
        let mut x = Vector::zeros(d);
        let mut dx = Vector::zeros(d);
        x.as_mut_slice()[..self.n].copy_from_slice(q.as_slice());
        x.as_mut_slice()[self.n..self.n + self.k].copy_from_slice(currents.as_slice());
        dx.as_mut_slice()[..self.n].copy_from_slice(dq.as_slice());
        let ax = self.a.mul_vector(&x)?;
        let rhs: Vec<_> = (self.n..self.n + self.k)
            .map(|r| ax.as_slice()[r] - b[r])
            .collect();
        let dz = self.l.solve(&Vector::from_slice(&rhs))?;
        dx.as_mut_slice()[self.n..self.n + self.k].copy_from_slice(dz.as_slice());
        let edx = self.e.mul_vector(&dx)?;
        let rhs: Vec<_> = (0..self.n)
            .map(|r| b[r] - ax.as_slice()[r] - edx.as_slice()[r])
            .collect();
        let lambda = self.h.solve(&Vector::from_slice(&rhs))?;
        x.as_mut_slice()[self.n + self.k..].copy_from_slice(lambda.as_slice());
        let adx = self.a.mul_vector(&dx)?;
        let rhs: Vec<_> = (self.n..self.n + self.k)
            .map(|r| adx.as_slice()[r] - db[r])
            .collect();
        let ddz = self.l.solve(&Vector::from_slice(&rhs))?;
        let mut ddx = Vector::zeros(d);
        ddx.as_mut_slice()[..self.n].copy_from_slice(ddq.as_slice());
        ddx.as_mut_slice()[self.n..self.n + self.k].copy_from_slice(ddz.as_slice());
        let eddx = self.e.mul_vector(&ddx)?;
        let rhs: Vec<_> = (0..self.n)
            .map(|r| db[r] - adx.as_slice()[r] - eddx.as_slice()[r])
            .collect();
        let dlambda = self.h.solve(&Vector::from_slice(&rhs))?;
        dx.as_mut_slice()[self.n + self.k..].copy_from_slice(dlambda.as_slice());
        let point = ConsistentPoint {
            state: x,
            derivative: dx,
            dynamic_acceleration: Vector::from_slice(&ddx.as_slice()[..self.n + self.k]),
        };
        self.residuals(&point, jet)?;
        Ok(point)
    }

    /// Validate a supplied physical IC without silently changing capacitor
    /// voltage, flux or source current. Free currents are preserved exactly.
    ///
    /// # Errors
    /// Invalid or incompatible ICs, or reconstruction/residual failure.
    pub fn consistent_initial(
        &self,
        initial: &Vector,
        jet: &ForcingJet,
    ) -> SpiceResult<ConsistentPoint> {
        self.check(initial, self.a.rows())?;
        let point = self.reconstruct(
            &Vector::from_slice(&initial.as_slice()[self.n..self.n + self.k]),
            jet,
        )?;
        for (r, (&old, &new)) in initial
            .as_slice()
            .iter()
            .zip(point.state.as_slice())
            .enumerate()
        {
            let bound = self.options.state_absolute[r] + self.options.relative * old.abs();
            if !bound.is_finite() || (old - new).abs() > bound {
                return Err(error(format!(
                    "incompatible initial condition at physical unknown {r}"
                )));
            }
        }
        Ok(point)
    }

    /// Require a C2 join. Voltage jumps require capacitor-charge impulses;
    /// slope corners require source-current jumps and are also outside this
    /// smooth-only prototype. No event state is fabricated or committed.
    ///
    /// # Errors
    /// Invalid jets or any differing value/derivative/constraint curvature.
    pub fn check_smooth_join(&self, left: &ForcingJet, right: &ForcingJet) -> SpiceResult<()> {
        self.check_jet(left)?;
        self.check_jet(right)?;
        if left.value.as_slice()[self.n + self.k..] != right.value.as_slice()[self.n + self.k..] {
            return Err(error("unsupported impulsive voltage-constraint jump"));
        }
        if left.value != right.value
            || left.first != right.first
            || left.constraint_second != right.constraint_second
        {
            return Err(error("unsupported nonsmooth forcing join"));
        }
        Ok(())
    }

    /// Certify original, first-differentiated and second-constraint equations.
    /// This is also a fallible independent check of caller-supplied candidates.
    ///
    /// # Errors
    /// Dimension/finite errors, residual arithmetic overflow, or exceeded bounds.
    pub fn residuals(
        &self,
        point: &ConsistentPoint,
        jet: &ForcingJet,
    ) -> SpiceResult<ResidualReport> {
        self.check_jet(jet)?;
        let d = self.a.rows();
        self.check(&point.state, d)?;
        self.check(&point.derivative, d)?;
        self.check(&point.dynamic_acceleration, self.n + self.k)?;
        let mut ddx = Vector::zeros(d);
        ddx.as_mut_slice()[..self.n + self.k]
            .copy_from_slice(point.dynamic_acceleration.as_slice());
        let (original, r0) = self.equation_residual(&point.state, &point.derivative, &jet.value)?;
        let (differentiated, r1) = self.equation_residual(&point.derivative, &ddx, &jet.first)?;
        let mut second_rhs = Vector::zeros(d);
        second_rhs.as_mut_slice()[self.n + self.k..]
            .copy_from_slice(jet.constraint_second.as_slice());
        let addx = self.a.mul_vector(&ddx)?;
        let mut max_bound_ratio = r0.max(r1);
        let mut constraint_second = Vector::zeros(self.n);
        let zero = Vector::zeros(d);
        for r in self.n + self.k..d {
            let residual = addx.as_slice()[r] - second_rhs.as_slice()[r];
            constraint_second.as_mut_slice()[r - self.n - self.k] = residual;
            max_bound_ratio =
                max_bound_ratio.max(self.row_ratio(r, &ddx, &zero, &second_rhs, residual)?);
        }
        Ok(ResidualReport {
            original,
            differentiated,
            constraint_second,
            max_bound_ratio,
        })
    }

    fn equation_residual(&self, x: &Vector, dx: &Vector, b: &Vector) -> SpiceResult<(Vector, f64)> {
        let ax = self.a.mul_vector(x)?;
        let edx = self.e.mul_vector(dx)?;
        let mut result = Vector::zeros(self.a.rows());
        let mut ratio: f64 = 0.;
        for r in 0..self.a.rows() {
            let residual = ax.as_slice()[r] + edx.as_slice()[r] - b.as_slice()[r];
            result.as_mut_slice()[r] = residual;
            ratio = ratio.max(self.row_ratio(r, x, dx, b, residual)?);
        }
        Ok((result, ratio))
    }

    fn row_ratio(
        &self,
        r: usize,
        x: &Vector,
        dx: &Vector,
        b: &Vector,
        residual: f64,
    ) -> SpiceResult<f64> {
        let d = self.a.rows();
        let mut scale = b.as_slice()[r].abs();
        for c in 0..d {
            scale += (self.a.data()[r * d + c] * x.as_slice()[c]).abs()
                + (self.e.data()[r * d + c] * dx.as_slice()[c]).abs();
        }
        let bound = self.options.equation_absolute[r] + self.options.relative * scale;
        if !residual.is_finite() || !scale.is_finite() || !bound.is_finite() {
            return Err(error(format!(
                "original-unit residual arithmetic overflow at row {r}"
            )));
        }
        if residual.abs() > bound {
            return Err(error(format!(
                "original-unit residual bound exceeded at row {r}: {residual} > {bound}"
            )));
        }
        Ok(residual.abs() / bound)
    }
}

fn assembled(input: &SparseMatrix) -> SpiceResult<Matrix> {
    if input.triplets().iter().any(|t| !t.value.is_finite()) {
        return Err(error("nonfinite operator coefficient"));
    }
    let mut output = Matrix::zeros(input.rows(), input.cols());
    for t in input.triplets() {
        output.add_to(t.row, t.col, t.value)?;
    }
    if output.data().iter().any(|v| !v.is_finite()) {
        return Err(error("duplicate assembly overflow"));
    }
    Ok(output)
}
