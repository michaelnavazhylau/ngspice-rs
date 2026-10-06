//! Immutable linear MNA assembly, distinct from timestep-dependent companions.
use crate::MnaUnknowns;
use spice_core::{Complex, NodeId, Real, SpiceError, SpiceResult};
use spice_maths::{SparseMatrix, Vector};

/// A bounded source waveform. Breakpoints are explicit and values finite.
#[derive(Debug, Clone, PartialEq)]
pub enum Waveform {
    /// Time-independent forcing.
    Constant(Real),
    /// Right-continuous jump at a nonnegative time.
    Step {
        /// Value before the jump.
        before: Real,
        /// Value after the jump.
        after: Real,
        /// Jump time.
        time: Real,
    },
    /// Continuous piecewise linear points, held constant outside the range.
    Pwl(Vec<(Real, Real)>),
}
impl Waveform {
    /// Checks finite values and strictly increasing nonnegative knot times.
    /// # Errors
    /// Invalid source data.
    pub fn validate(&self) -> SpiceResult<()> {
        let valid = match self {
            Self::Constant(v) => v.is_finite(),
            Self::Step {
                before,
                after,
                time,
            } => before.is_finite() && after.is_finite() && time.is_finite() && *time >= 0.,
            Self::Pwl(p) => {
                !p.is_empty()
                    && p.iter()
                        .all(|(t, v)| t.is_finite() && *t >= 0. && v.is_finite())
                    && p.windows(2).all(|w| w[0].0 < w[1].0)
            }
        };
        if !valid {
            return Err(SpiceError::circuit("invalid source waveform"));
        }
        Ok(())
    }
    /// Value at time t; left limit is used at a segment's terminating jump.
    pub fn value(&self, t: Real, left_limit: bool) -> Real {
        match self {
            Self::Constant(v) => *v,
            Self::Step {
                before,
                after,
                time,
            } => {
                if t < *time || (left_limit && t == *time) {
                    *before
                } else {
                    *after
                }
            }
            Self::Pwl(p) => {
                if t <= p[0].0 {
                    return p[0].1;
                }
                for w in p.windows(2) {
                    if t <= w[1].0 {
                        let fraction = (t - w[0].0) / (w[1].0 - w[0].0);
                        return (1. - fraction) * w[0].1 + fraction * w[1].1;
                    }
                }
                p.last().unwrap().1
            }
        }
    }
    /// Known knot/jump times.
    pub fn breakpoints(&self) -> Vec<Real> {
        match self {
            Self::Constant(_) => vec![],
            Self::Step { time, .. } => vec![*time],
            Self::Pwl(p) => p.iter().map(|p| p.0).collect(),
        }
    }
}

/// A source bound to RHS rows with signed contributions.
#[derive(Debug, Clone)]
pub struct LinearSource {
    /// Instance name (for sweep selection).
    pub name: String,
    /// Signed RHS locations, omitting ground.
    pub rows: Vec<(usize, Real)>,
    /// Operating-point value.
    pub dc: Real,
    /// Small-signal phasor.
    pub ac: Complex,
    /// Transient forcing.
    pub waveform: Waveform,
}

/// Preassembled state-independent `E x' + A x = b(t)`.
#[derive(Debug, Clone)]
pub struct LinearSystem {
    /// Static conductances and branch constraints.
    pub a: SparseMatrix,
    /// Capacitive/inductive operator, not companion stamps.
    pub e: SparseMatrix,
    /// Independent sources.
    pub sources: Vec<LinearSource>,
    /// Rows with explicit device initial conditions (not yet supported).
    pub has_initial_conditions: bool,
}
impl LinearSystem {
    /// Empty assembly for n unknowns.
    pub fn new(n: usize) -> Self {
        Self {
            a: SparseMatrix::new(n, n),
            e: SparseMatrix::new(n, n),
            sources: vec![],
            has_initial_conditions: false,
        }
    }
    /// DC forcing, optionally overriding one independent source.
    pub fn dc_rhs(&self, sweep: Option<(&str, Real)>) -> SpiceResult<Vector> {
        let mut rhs = Vector::zeros(self.a.rows());
        for s in &self.sources {
            let value = sweep
                .filter(|(name, _)| s.name.eq_ignore_ascii_case(name))
                .map_or(s.dc, |(_, v)| v);
            for (row, sign) in &s.rows {
                rhs.add_to(*row, sign * value)?;
            }
        }
        if !rhs.is_finite() {
            return Err(SpiceError::circuit("non-finite source sum"));
        }
        Ok(rhs)
    }
    /// Real transient RHS, with explicit one-sided breakpoint evaluation.
    pub fn transient_rhs(&self, t: Real, left_limit: bool) -> Vector {
        let mut rhs = Vector::zeros(self.a.rows());
        for s in &self.sources {
            for (r, sign) in &s.rows {
                rhs.as_mut_slice()[*r] += sign * s.waveform.value(t, left_limit);
            }
        }
        rhs
    }
    /// Complex AC forcing.
    pub fn ac_rhs(&self) -> Vec<Complex> {
        let mut rhs = vec![Complex::ZERO; self.a.rows()];
        for s in &self.sources {
            for (r, sign) in &s.rows {
                rhs[*r] = rhs[*r] + Complex::real(*sign) * s.ac;
            }
        }
        rhs
    }
    /// Sorted unique breakpoints inside the requested run.
    pub fn breakpoints(&self, end: Real) -> Vec<Real> {
        let mut times: Vec<_> = self
            .sources
            .iter()
            .flat_map(|s| s.waveform.breakpoints())
            .filter(|t| *t > 0. && *t < end)
            .collect();
        times.sort_by(f64::total_cmp);
        times.dedup();
        times
    }
}

/// Per-device immutable equation assembly context.
pub struct LinearContext<'a> {
    /// Operators and forcing being assembled.
    pub system: &'a mut LinearSystem,
    /// Node numbering, with ground eliminated.
    pub unknowns: &'a MnaUnknowns,
    /// The first allocated branch row for this device, if any.
    pub branch: Option<usize>,
}
impl LinearContext<'_> {
    /// Stamps a two-terminal nodal operator into A or E.
    pub fn nodal(&mut self, terminals: [NodeId; 2], value: Real, dynamic: bool) -> SpiceResult<()> {
        let matrix = if dynamic {
            &mut self.system.e
        } else {
            &mut self.system.a
        };
        nodal_stamp(matrix, self.unknowns, terminals, value)
    }
    /// Stamps a voltage/inductor branch, positive current from terminal 0 to 1.
    pub fn branch(&mut self, terminals: [NodeId; 2]) -> SpiceResult<usize> {
        let branch = self
            .branch
            .ok_or_else(|| SpiceError::circuit("missing branch-row binding"))?;
        branch_stamp(&mut self.system.a, self.unknowns, terminals, branch)?;
        Ok(branch)
    }
}

pub(crate) fn nodal_stamp(
    matrix: &mut SparseMatrix,
    unknowns: &MnaUnknowns,
    terminals: [NodeId; 2],
    value: Real,
) -> SpiceResult<()> {
    for (node, sign) in [(terminals[0], 1.), (terminals[1], -1.)] {
        for (other, other_sign) in [(terminals[0], 1.), (terminals[1], -1.)] {
            if let (Some(r), Some(c)) = (unknowns.node_row(node), unknowns.node_row(other)) {
                matrix.add(r, c, value * sign * other_sign)?;
            }
        }
    }
    Ok(())
}
pub(crate) fn branch_stamp(
    matrix: &mut SparseMatrix,
    unknowns: &MnaUnknowns,
    terminals: [NodeId; 2],
    branch: usize,
) -> SpiceResult<()> {
    for (node, sign) in [(terminals[0], 1.), (terminals[1], -1.)] {
        if let Some(row) = unknowns.node_row(node) {
            matrix.add(row, branch, sign)?;
            matrix.add(branch, row, sign)?;
        }
    }
    Ok(())
}
