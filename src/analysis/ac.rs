//! Linear complex small-signal analysis, using faer (not diffsol).
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
    let grid = frequency_grid(&positional)?;
    let mut small_signal = SmallSignal::prepare(circuit, request, context, settings)?;
    let mut plot = plot(
        circuit,
        "ac1",
        "AC Analysis",
        Some(("frequency", "frequency")),
        true,
    )?;
    for f in grid {
        let x = small_signal.at_frequency(circuit, context, f, |system| {
            let matrix =
                ComplexMatrix::from_operators(&system.a, &system.e, 2. * std::f64::consts::PI * f)?;
            matrix.factorize()?.solve(&system.ac_rhs())
        })?;
        let mut point = vec![Complex::real(f)];
        point.extend(x);
        plot.push_point(point)?;
    }
    Ok(plot)
}

/// The `acan.c`/`span.c` frequency grid of `lin|dec|oct points start stop`,
/// the first four positional arguments of the card.
pub(crate) fn frequency_grid(arguments: &[String]) -> SpiceResult<Vec<Real>> {
    let argument = |index: usize| arguments.get(index).map(String::as_str);
    let mode = argument(0).unwrap_or_default().to_ascii_lowercase();
    let points: usize = argument(1)
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0 && *n <= 100_000)
        .ok_or_else(|| unsupported("AC points must be in 1..=100000"))?;
    let start = number(argument(2), "start frequency")?;
    let end = number(argument(3), "stop frequency")?;
    if start <= 0. || end < start {
        return Err(unsupported("invalid AC frequency bounds"));
    }
    let grid = match mode.as_str() {
        "lin" => (0..points)
            .map(|i| {
                if points == 1 {
                    start
                } else {
                    (1. - (i as f64) / ((points - 1) as f64)) * start
                        + (i as f64) / ((points - 1) as f64) * end
                }
            })
            .collect::<Vec<_>>(),
        "dec" | "oct" => {
            let base: f64 = if mode == "dec" { 10. } else { 2. };
            let span = (end.ln() - start.ln()) / base.ln();
            let steps = span * (points as f64);
            // Logarithms can put an exact integer span just below that integer
            // (3 points/decade, 1..1000 gives 8.999999999999998). Snap only
            // within a small floating-arithmetic bound, preserving the endpoint.
            // This does not relax any golden/value comparison tolerance.
            let rounded = steps.round();
            let steps = if (steps - rounded).abs() <= 32. * f64::EPSILON * steps.abs().max(1.) {
                rounded
            } else {
                steps
            };
            let count = steps.floor() + 1.;
            if !count.is_finite() || count > 100_000. {
                return Err(unsupported("AC sample limit exceeded"));
            }
            (0..count as usize)
                .map(|i| {
                    (start.ln() + base.ln() * (i as f64) / (points as f64))
                        .exp()
                        .min(end)
                })
                .collect()
        }
        _ => return Err(unsupported("AC sweep mode must be lin, dec or oct")),
    };
    if grid.windows(2).any(|w| w[0] >= w[1]) {
        return Err(unsupported("AC frequency grid makes no progress"));
    }
    Ok(grid)
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
