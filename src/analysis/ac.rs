//! Linear complex small-signal analysis on the SPARSE 1.3 complex LU.
//!
//! The frequency grid ([`frequency_grid`]) and the linearized system
//! ([`SmallSignal`]) are shared with the `.sp` driver
//! (`crate::analysis::sparam`), because `span.c` steps frequencies and
//! prepares its operating point exactly as `acan.c` does.
use crate::analysis::linear::{number, plot, unsupported};
use crate::analysis::{AnalysisRequest, Plot};
use crate::devices::Circuit;
use crate::devices::linear::LinearSystem;
use crate::maths::Vector;
use crate::maths::complex::ComplexMatrix;
use crate::primitives::{Complex, Real, SpiceResult};

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Plot> {
    let settings = crate::analysis::bias::DcSettings::from_request(request)?;
    let positional: Vec<String> = request
        .arguments
        .iter()
        .filter(|a| !a.contains('='))
        .cloned()
        .collect();
    if positional.len() != 4 {
        return Err(unsupported(".ac lin|dec|oct points start stop"));
    }
    let grid = frequency_grid(&positional, settings.newton.reltol, GridKind::Ac)?;
    let mut small_signal = SmallSignal::prepare(circuit, request, context, settings)?;
    let mut plot = plot(
        circuit,
        "ac1",
        "AC Analysis",
        Some(("frequency", "frequency")),
        true,
    )?;
    // NIacIter on the circuit's own matrix: the first AC solve of the
    // circuit reorders (`SMPcReorder`, starting from the real pivot order),
    // later frequencies refactor (`SMPcLUfac`) and reorder only after a zero
    // pivot. See [`ac_matrix`].
    let shared: &Circuit = circuit;
    for f in grid {
        let x = small_signal.at_frequency(shared, context, f, |system| {
            ac_matrix(shared, system, 2. * std::f64::consts::PI * f)?
                .factorize()?
                .solve(&system.ac_rhs())
        })?;
        let mut point = vec![Complex::real(f)];
        point.extend(x);
        point.extend(circuit.observe_parameters(&context.model_context().with_frequency(f))?);
        plot.push_point(point)?;
    }
    Ok(plot)
}

/// Which C frequency loop a grid follows: `acan.c` has a special decade delta
/// that `span.c` lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GridKind {
    /// `.ac` (`ACan`).
    Ac,
    /// `.sp` (`SPan`).
    Sp,
}

/// The `acan.c`/`span.c` frequency grid of `lin|dec|oct points start stop`,
/// the first four positional arguments of the card, generated as C does:
/// iteratively (`freq *= delta` or `freq += delta`) while
/// `freq <= stop + freqTol`, with `freqTol = delta * stop * reltol` (log) or
/// `delta * reltol` (linear).
///
/// C quirks kept on purpose: a linear sweep of fewer than three points uses
/// `delta = 0` and so yields only the start frequency (Richard McRoberts'
/// patch tests `ACnumberSteps - 1 > 1`); `.ac dec` spanning a decade or more
/// uses `delta = exp(ln(stop/start) / floor(|log10(stop/start)| * points))`
/// so the last point lands on `stop`.
pub(crate) fn frequency_grid(
    arguments: &[String],
    reltol: Real,
    kind: GridKind,
) -> SpiceResult<Vec<Real>> {
    let argument = |index: usize| arguments.get(index).map(String::as_str);
    let mode = argument(0).unwrap_or_default().to_ascii_lowercase();
    // ACnumberSteps is IF_INTEGER: floor(0.5 + value).
    let points: usize = argument(1)
        .and_then(crate::primitives::parse_spice_number)
        .and_then(crate::primitives::c_integer)
        .filter(|n| *n > 0 && *n <= 100_000)
        .map(|n| n as usize)
        .ok_or_else(|| unsupported("AC points must be in 1..=100000"))?;
    let start = number(argument(2), "start frequency")?;
    let stop = number(argument(3), "stop frequency")?;
    let steps = points as Real;
    let logarithmic = mode != "lin";
    if logarithmic && start <= 0. {
        // acan.c/span.c: "ERROR: AC startfreq <= 0", E_PARMVAL.
        return Err(unsupported("AC start frequency must be > 0 for dec/oct"));
    }
    if start < 0. || stop < start {
        return Err(unsupported("invalid AC frequency bounds"));
    }
    let delta = match mode.as_str() {
        "dec" if kind == GridKind::Ac && stop / 10. >= start => {
            let num_steps = ((stop / start).log10().abs() * steps).floor();
            ((stop / start).ln() / num_steps).exp()
        }
        "dec" if stop == start && kind == GridKind::Ac => 1.,
        "dec" => (10f64.ln() / steps).exp(),
        "oct" => (2f64.ln() / steps).exp(),
        "lin" if points - 1 > 1 => (stop - start) / (steps - 1.),
        "lin" => 0.,
        _ => return Err(unsupported("AC sweep mode must be lin, dec or oct")),
    };
    let tolerance = if logarithmic {
        delta * stop * reltol
    } else {
        delta * reltol
    };
    let mut grid = Vec::new();
    let mut freq = start;
    while freq <= stop + tolerance {
        if grid.len() >= 100_000 {
            return Err(unsupported("AC sample limit exceeded"));
        }
        grid.push(freq);
        if logarithmic {
            freq *= delta;
            if delta == 1. {
                break;
            }
        } else {
            freq += delta;
            if delta == 0. {
                break;
            }
        }
    }
    Ok(grid)
}

/// `CKTacLoad`: the small-signal system at `omega` loaded into the circuit's
/// own matrix, so that factoring it shares SPARSE's frame and pivot order
/// with the real solves (`NIacIter`). Falls back to a standalone matrix when
/// a small-signal stamp lies outside the circuit's allocated elements.
///
/// # Errors
/// Invalid assembly.
pub(crate) fn ac_matrix(
    circuit: &Circuit,
    system: &LinearSystem,
    omega: Real,
) -> SpiceResult<ComplexMatrix> {
    if let Some(pattern) = circuit.matrix_pattern()
        && let Some(matrix) = ComplexMatrix::from_operators_grouped_on(
            &pattern,
            &system.a,
            &system.e,
            &system.e_groups,
            omega,
        )?
    {
        return Ok(matrix);
    }
    ComplexMatrix::from_operators_grouped(&system.a, &system.e, &system.e_groups, omega)
}

/// The circuit linearized at its DC operating point, as `acan.c` and `span.c`
/// prepare it before their frequency loops.
pub(crate) struct SmallSignal {
    settings: crate::analysis::bias::DcSettings,
    state1: Vec<Real>,
    system: LinearSystem,
    /// A `hertz`-dependent device forces a new operating point per frequency.
    varies: bool,
    previous: Vector,
}

impl SmallSignal {
    /// Solves the operating point and assembles the small-signal system.
    pub(crate) fn prepare(
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &crate::analysis::AnalysisContext,
        settings: crate::analysis::bias::DcSettings,
    ) -> SpiceResult<Self> {
        circuit.finalize()?;
        let hints = crate::analysis::initial::resolve(circuit, request)?;
        let nodes = crate::analysis::bias::NodeForcing {
            initial: Vec::new(),
            nodesets: crate::analysis::initial::forced_nodesets(circuit, &hints.nodesets, &[]),
        };
        let mut seed = Vector::zeros(circuit.unknown_count());
        for hint in hints.nodesets {
            seed.as_mut_slice()[hint.row] = hint.value;
        }
        // AC is linearized only after a valid physical DC solution, never at zero.
        let solved = crate::analysis::bias::solve_dc_forced(
            circuit,
            &context.model_context(),
            &settings,
            &[],
            Some(&seed),
            None,
            &nodes,
        )?
        .solution;
        // acan.c reloads with MODEINITSMSIG after CKTop, and SWload/CSWload then
        // copy CKTstate1 into CKTstate0. CKTop never accepts or rotates states, so
        // CKTstate1 is still the zero ("really off") vector: the AC conductance of
        // a switch is that of the accepted history, not of the converged
        // operating point (`swacload.c`/`cswacld.c`). The operating point's own
        // trial state is therefore deliberately not used here.
        let history = circuit.state_history();
        let state1 = history
            .accepted(1)
            .map_or_else(|| vec![0.; history.len()], <[Real]>::to_vec);
        let system = circuit.small_signal_system_at(
            &context.model_context(),
            &solved.values,
            Some(&state1),
        )?;
        // acan.c: with a `hertz`-dependent device (CKTvarHertz) the operating
        // point is re-solved at every frequency, warm-started from the previous.
        let varies = circuit
            .devices()
            .iter()
            .any(|device| device.depends_on_frequency());
        Ok(Self {
            settings,
            state1,
            system,
            varies,
            previous: solved.values,
        })
    }

    /// The system linearized at the operating point (re-solved at the last
    /// requested frequency for a `hertz`-dependent circuit).
    pub(crate) const fn system(&self) -> &LinearSystem {
        &self.system
    }

    /// The operating point the system is linearized at (re-solved at the last
    /// requested frequency for a `hertz`-dependent circuit).
    pub(crate) const fn bias(&self) -> &Vector {
        &self.previous
    }

    /// The small-signal state (C's `CKTstate0` after the `MODEINITSMSIG`
    /// load): the zero accepted state.
    pub(crate) fn state(&self) -> &[Real] {
        &self.state1
    }

    /// Whether the operating point is re-solved at every frequency.
    pub(crate) const fn varies(&self) -> bool {
        self.varies
    }

    /// Calls `with` on the small-signal system valid at frequency `f`.
    pub(crate) fn at_frequency<R>(
        &mut self,
        circuit: &Circuit,
        context: &crate::analysis::AnalysisContext,
        f: Real,
        with: impl FnOnce(&LinearSystem) -> SpiceResult<R>,
    ) -> SpiceResult<R> {
        if self.varies {
            let model = context.model_context().with_frequency(f);
            self.previous = crate::analysis::bias::solve_dc_with(
                circuit,
                &model,
                &self.settings,
                &[],
                Some(&self.previous),
                None,
            )?
            .solution
            .values;
            self.system =
                circuit.small_signal_system_at(&model, &self.previous, Some(&self.state1))?;
        }
        with(&self.system)
    }
}
