//! Immutable linear MNA assembly, distinct from timestep-dependent companions.
use crate::MnaUnknowns;
use crate::pulse::{Pulse, PulseSpec, TransientTiming};
use spice_core::{Complex, NodeId, Real, SpiceError, SpiceResult};
use spice_maths::{SparseMatrix, Vector};

/// Which one-sided limit to take where a waveform has a jump.
///
/// Away from jumps both limits agree. Transient drivers evaluate the forcing at
/// the *end* of a segment with [`Limit::Left`] and at the *start* of the next
/// segment with [`Limit::Right`]; they never interpolate across a jump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The limit approaching `t` from earlier times.
    Left,
    /// The limit approaching `t` from later times (the value just after `t`).
    Right,
}

/// A bounded source waveform. Breakpoints are explicit and values finite.
///
/// Time forcing is evaluated with [`Waveform::value_at`] and its corners and
/// jumps are enumerated lazily with [`Waveform::breakpoints_in`]; DC and AC
/// excitations are separate fields of a source and never derived from this.
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
    /// A fully specified periodic pulse; see [`Pulse`].
    Pulse(Pulse),
    /// A PULSE as written, with C's analysis-dependent defaults still pending.
    /// [`LinearSystem::bind_transient_timing`] (or [`Waveform::resolve`]) turns
    /// it into [`Waveform::Pulse`]; evaluating it earlier is an error.
    PulseDefaults(PulseSpec),
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
            // Pulse::new is the only constructor, so a Pulse is always valid.
            Self::Pulse(_) => true,
            Self::PulseDefaults(spec) => {
                // Resolve against unit timing: checks everything that does not
                // depend on the analysis (finiteness, prefix, delay sign).
                spec.resolve(&TransientTiming::new(1., 1.)?)?;
                true
            }
        };
        if !valid {
            return Err(SpiceError::circuit("invalid source waveform"));
        }
        Ok(())
    }

    /// Resolves analysis-dependent defaults; other variants are returned as is.
    ///
    /// # Errors
    /// The resolved pulse is invalid.
    pub fn resolve(&self, timing: &TransientTiming) -> SpiceResult<Self> {
        match self {
            Self::PulseDefaults(spec) => Ok(Self::Pulse(spec.resolve(timing)?)),
            other => Ok(other.clone()),
        }
    }

    /// Time forcing at `t` with an explicit one-sided limit.
    ///
    /// This is the evaluation API for transient drivers. `limit` only matters at
    /// a jump (a [`Waveform::Step`] time, a zero-duration pulse edge, or a pulse
    /// period boundary that cuts a ramp); elsewhere both limits are equal.
    ///
    /// # Errors
    /// Nonfinite `t`, unresolved [`Waveform::PulseDefaults`], or a time too many
    /// periods from the origin to resolve.
    pub fn value_at(&self, t: Real, limit: Limit) -> SpiceResult<Real> {
        if !t.is_finite() {
            return Err(SpiceError::circuit("nonfinite waveform evaluation time"));
        }
        let left = limit == Limit::Left;
        Ok(match self {
            Self::Constant(v) => *v,
            Self::Step {
                before,
                after,
                time,
            } => {
                if t < *time || (left && t == *time) {
                    *before
                } else {
                    *after
                }
            }
            Self::Pwl(p) => {
                if t <= p[0].0 {
                    return Ok(p[0].1);
                }
                for w in p.windows(2) {
                    if t <= w[1].0 {
                        let fraction = (t - w[0].0) / (w[1].0 - w[0].0);
                        return Ok((1. - fraction) * w[0].1 + fraction * w[1].1);
                    }
                }
                p.last().map_or(0., |p| p.1)
            }
            Self::Pulse(pulse) => return pulse.value_at(t, limit),
            Self::PulseDefaults(_) => {
                return Err(SpiceError::circuit(
                    "PULSE defaults (TR/TF/PW/PER) are unresolved; bind transient timing first",
                ));
            }
        })
    }

    /// Value at time t; left limit is used at a segment's terminating jump.
    /// Returns NaN where [`Waveform::value_at`] would fail; prefer `value_at`.
    pub fn value(&self, t: Real, left_limit: bool) -> Real {
        let limit = if left_limit {
            Limit::Left
        } else {
            Limit::Right
        };
        self.value_at(t, limit).unwrap_or(Real::NAN)
    }

    /// Lazily enumerates corner and jump times in the closed window `[t0, t1]`,
    /// ascending without repeats.
    ///
    /// Periodic pulses are generated one cycle at a time and never expanded;
    /// callers bound a run by taking only what they need. Constant waveforms
    /// have none; Step has its jump; Pwl has its knots.
    ///
    /// # Errors
    /// Nonfinite or reversed window, unresolved [`Waveform::PulseDefaults`], or a
    /// window spanning more periods than can be resolved.
    pub fn breakpoints_in(&self, t0: Real, t1: Real) -> SpiceResult<WaveformBreakpoints> {
        if !(t0.is_finite() && t1.is_finite() && t0 <= t1) {
            return Err(SpiceError::circuit(
                "breakpoint window must be finite with t0 <= t1",
            ));
        }
        let fixed: Vec<Real> = match self {
            Self::Constant(_) => vec![],
            Self::Step { time, .. } => vec![*time],
            Self::Pwl(p) => p.iter().map(|p| p.0).collect(),
            Self::Pulse(pulse) => {
                return Ok(WaveformBreakpoints(Inner::Pulse(
                    pulse.breakpoints_in(t0, t1)?,
                )));
            }
            Self::PulseDefaults(_) => {
                return Err(SpiceError::circuit(
                    "PULSE defaults are unresolved; bind transient timing before enumerating breakpoints",
                ));
            }
        };
        Ok(WaveformBreakpoints(Inner::Fixed(
            fixed
                .into_iter()
                .filter(|t| (t0..=t1).contains(t))
                .collect::<Vec<_>>()
                .into_iter(),
        )))
    }
}

/// Lazy breakpoint sequence of one [`Waveform`] (see [`Waveform::breakpoints_in`]).
#[derive(Debug, Clone)]
pub struct WaveformBreakpoints(Inner);
#[derive(Debug, Clone)]
enum Inner {
    Fixed(std::vec::IntoIter<Real>),
    Pulse(crate::pulse::PulseBreakpoints),
}
impl Iterator for WaveformBreakpoints {
    type Item = Real;
    fn next(&mut self) -> Option<Real> {
        match &mut self.0 {
            Inner::Fixed(it) => it.next(),
            Inner::Pulse(it) => it.next(),
        }
    }
}

/// Lazy, merged breakpoints of every source in a [`LinearSystem`].
#[derive(Debug, Clone)]
pub struct SystemBreakpoints {
    sources: Vec<std::iter::Peekable<WaveformBreakpoints>>,
}
impl Iterator for SystemBreakpoints {
    type Item = Real;
    fn next(&mut self) -> Option<Real> {
        let next = self
            .sources
            .iter_mut()
            .filter_map(|s| s.peek().copied())
            .min_by(f64::total_cmp)?;
        for source in &mut self.sources {
            while source.next_if(|t| *t == next).is_some() {}
        }
        Some(next)
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
    /// Binds the analysis quantities that C uses for PULSE defaults (`CKTstep`,
    /// `CKTfinalTime`), turning every [`Waveform::PulseDefaults`] into a
    /// resolved [`Waveform::Pulse`]. Call once before time forcing is evaluated.
    ///
    /// # Errors
    /// A pulse is invalid after resolution; the system is left unchanged.
    pub fn bind_transient_timing(&mut self, timing: &TransientTiming) -> SpiceResult<()> {
        let resolved = self
            .sources
            .iter()
            .map(|s| s.waveform.resolve(timing))
            .collect::<SpiceResult<Vec<_>>>()?;
        for (source, waveform) in self.sources.iter_mut().zip(resolved) {
            source.waveform = waveform;
        }
        Ok(())
    }
    /// Real transient RHS `b(t)` with an explicit one-sided limit at jumps.
    ///
    /// # Errors
    /// Nonfinite time, unresolved PULSE defaults or a nonfinite source sum.
    pub fn transient_rhs(&self, t: Real, limit: Limit) -> SpiceResult<Vector> {
        let mut rhs = Vector::zeros(self.a.rows());
        for s in &self.sources {
            let value = s.waveform.value_at(t, limit)?;
            for (r, sign) in &s.rows {
                rhs.add_to(*r, sign * value)?;
            }
        }
        if !rhs.is_finite() {
            return Err(SpiceError::circuit("non-finite source sum"));
        }
        Ok(rhs)
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
    /// Lazily merges every source's corners and jumps in the closed window
    /// `[t0, t1]` into one ascending, duplicate-free sequence.
    ///
    /// Nothing is materialized: a periodic pulse over a long run yields one
    /// cycle at a time, so a driver must bound how many it consumes.
    ///
    /// # Errors
    /// Invalid window or unresolved PULSE defaults.
    pub fn breakpoints_in(&self, t0: Real, t1: Real) -> SpiceResult<SystemBreakpoints> {
        Ok(SystemBreakpoints {
            sources: self
                .sources
                .iter()
                .map(|s| s.waveform.breakpoints_in(t0, t1).map(Iterator::peekable))
                .collect::<SpiceResult<_>>()?,
        })
    }
}

/// Per-device immutable equation assembly context.
pub struct LinearContext<'a> {
    /// Explicit circuit/nominal temperatures for immutable model evaluation.
    pub model_context: &'a crate::models::ModelContext,
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
