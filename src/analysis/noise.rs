//! `.noise` — small-signal noise analysis (adjoint method).
//!
//! C references (behaviour only, reimplemented): `noisean.c` (`NOISEan`: the
//! card, the frequency loop and the two plots), `cktnoise.c` (`CKTnoise`:
//! naming and summing), `nevalsrc.c` (`NevalSrc`: generator gains and
//! densities), `ninteg.c` (`Nintegrate`: the power-law integration),
//! `src/maths/ni/niniter.c` (`NInzIter`: the adjoint solve), and the device
//! routines summarised in [`crate::devices::noise`].
//!
//! ```text
//! .noise v(out[,ref]) src {dec|oct|lin} pts fstart fstop [pts_per_summary]
//! ```
//!
//! At every frequency the complex small-signal matrix `A + j omega E` of the
//! `.ac` analysis is factored once and solved twice:
//!
//! * forward, with a unit excitation of the input source only (C's
//!   `MODEACNOISE` zeroes every other AC source and drives the input with
//!   `1 + 0j` whatever its `ac` value), for the gain `H` from the input to the
//!   output; the input-referred noise is the output noise divided by
//!   `max(abs(H)^2, 1e-20)`;
//! * transposed (`A^T y = e_out`, a unit current between the output nodes),
//!   whose solution `y` holds every generator's transfer impedance: a
//!   generator between `n1` and `n2` reaches the output with the squared gain
//!   `abs(y[n1] - y[n2])^2`.
//!
//! Each generator's output density is integrated over frequency with C's
//! piecewise power law (`Nintegrate`) into the integrated output and input
//! noise. Results, exactly as C names and orders them:
//!
//! * plot 1, `Noise Spectral Density Curves` (real, `frequency` scale): with
//!   `pts_per_summary` the per-generator densities `onoise_<inst><suffix>` and
//!   per-instance totals `onoise_<inst>`, then `onoise_spectrum` and
//!   `inoise_spectrum`; with a summary, only every `pts_per_summary`-th
//!   frequency (counting from the first) is written;
//! * plot 2, `Integrated Noise` (real, one point, no scale), only when the
//!   start and stop frequencies differ: `v(onoise_total_<inst><suffix>)` /
//!   `inoise_total_...` pairs with a summary, then `v(onoise_total)` and the
//!   input total. The `v(...)`/`i(...)` spelling and the `voltage`/`current`
//!   units are those C's rawfile writer gives these vectors.
//!
//! Every value is the square root of the noise power (C without
//! `set sqrnoise`, supported through bounded pre-run settings): V/sqrt(Hz)
//! or A/sqrt(Hz) in plot 1, V or A in plot 2. Input-referred values are
//! voltages for a V input and currents for an I input.
//!
//! Divergences, all explicit errors where C would carry on: an output node
//! the circuit does not have (C creates it), identical output nodes, a
//! nonpositive or decreasing frequency range, an input that is not an
//! independent source with an `ac` value, and devices whose noise is not
//! ported. `.option keepopinfo`'s extra operating-point plot is not produced
//! (the option is not accepted by the port).

use crate::analysis::linear::{number, unsupported};
use crate::analysis::results::{Plot, PlotFlags, Variable};
use crate::analysis::{AnalysisContext, AnalysisRequest};
use crate::devices::noise::{InstanceNoise, circuit_noise};
use crate::devices::{Circuit, LinearSystem, SourceKind};
use crate::maths::complex::ComplexMatrix;
use crate::primitives::{Complex, NodeId, Real, SpiceError, SpiceResult, parse_spice_number};

/// C `N_MINLOG`: the smallest number whose logarithm is taken.
const N_MINLOG: Real = 1e-38;
/// C `N_MINGAIN`: the smallest squared input-output gain.
const N_MINGAIN: Real = 1e-20;
/// C `N_INTFTHRESH`: the largest log-log slope still integrated as flat.
const N_INTFTHRESH: Real = 1e-10;
/// C `N_INTUSELOG`: the region around a slope of -1 integrated as `ln f`.
const N_INTUSELOG: Real = 1e-10;
/// C's default `CKTreltol`, the frequency-loop tolerance without `.option
/// reltol`.
const DEFAULT_RELTOL: Real = 1e-3;
/// The port's bound on the number of frequency points.
const MAX_POINTS: usize = 100_000;
/// The spectrum plot's title (`noisean.c`, without `sqrnoise`).
pub const SPECTRUM_PLOTNAME: &str = "Noise Spectral Density Curves";
/// The integrated plot's title (`noisean.c`, without `sqrnoise`).
pub const INTEGRATED_PLOTNAME: &str = "Integrated Noise";

/// The frequency stepping of a `.noise` card (`NOISEAN.NstpType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stepping {
    Decade,
    Octave,
    Linear,
}

/// A parsed `.noise` card, after C's single-frequency adjustments.
#[derive(Debug, Clone, PartialEq)]
struct NoiseCard {
    positive: NodeId,
    negative: NodeId,
    input: String,
    stepping: Stepping,
    steps: usize,
    start: Real,
    stop: Real,
    summary: usize,
}

/// C's `INPgetValue(IF_INTEGER)`: `floor(value + 0.5)`.
fn integer(text: Option<&str>, what: &str) -> SpiceResult<i64> {
    let value = text
        .and_then(parse_spice_number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| unsupported(format!(".noise expects an integer {what}")))?;
    let rounded = (value + 0.5).floor();
    if rounded.abs() > i64::from(i32::MAX) as Real {
        return Err(unsupported(format!(".noise {what} is out of range")));
    }
    Ok(rounded as i64)
}

/// `AlmostEqualUlps(a, b, max_ulps)` (`src/misc/equality.c`).
fn almost_equal_ulps(a: Real, b: Real, max_ulps: i64) -> bool {
    if a == b {
        return true;
    }
    // Lexicographically ordered integer representations.
    let ordered = |x: Real| {
        let bits = x.to_bits() as i64;
        if bits < 0 { i64::MIN - bits } else { bits }
    };
    (ordered(a) - ordered(b)).unsigned_abs() <= max_ulps.unsigned_abs()
}

/// The positional tokens of a `.noise` card: `v ( out [, ref] ) src type pts
/// fstart fstop [pts_per_summary]`, the output spelled as one or several
/// tokens.
struct Positional {
    output: Vec<String>,
    rest: Vec<String>,
}

fn split_output(arguments: &[String]) -> SpiceResult<Positional> {
    let syntax = || {
        unsupported(
            ".noise needs v(out[,ref]) src {dec|oct|lin} pts fstart fstop [pts_per_summary]",
        )
    };
    let joined_end = arguments
        .iter()
        .position(|token| token.contains(')'))
        .ok_or_else(syntax)?;
    let text: String = arguments[..=joined_end].concat();
    let lower = text.to_ascii_lowercase();
    let inner = lower
        .strip_prefix("v(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| {
            unsupported(format!(
                ".noise output must be v(out) or v(out,ref), not '{text}' (noisean.c measures \
                 a node voltage)"
            ))
        })?;
    let output: Vec<String> = inner.split(',').map(|s| s.trim().to_owned()).collect();
    if output.is_empty() || output.len() > 2 || output.iter().any(String::is_empty) {
        return Err(syntax());
    }
    Ok(Positional {
        output,
        rest: arguments[joined_end + 1..].to_vec(),
    })
}

/// How many plots a `.noise` card produces: two (the spectrum and the
/// integrated noise), or one when C collapses the sweep to a single frequency
/// (`lin` with one point, or start and stop within 3 ulps). Used by the batch
/// scheduler to name the plots before anything runs; a card the driver would
/// reject (or one with braced, not yet evaluated, arguments) counts two.
#[must_use]
pub fn plot_count(arguments: &[String]) -> usize {
    let positional: Vec<String> = arguments
        .iter()
        .filter(|argument| !argument.contains('='))
        .cloned()
        .collect();
    let Ok(Positional { rest, .. }) = split_output(&positional) else {
        return 2;
    };
    let (Some(kind), Ok(steps), Ok(start), Ok(stop)) = (
        rest.get(1),
        integer(rest.get(2).map(String::as_str), "point count"),
        number(rest.get(3).map(String::as_str), "start frequency"),
        number(rest.get(4).map(String::as_str), "stop frequency"),
    ) else {
        return 2;
    };
    let linear_single = kind.eq_ignore_ascii_case("lin") && steps == 1;
    if linear_single || almost_equal_ulps(start, stop, 3) {
        1
    } else {
        2
    }
}

fn parse(circuit: &Circuit, request: &AnalysisRequest) -> SpiceResult<NoiseCard> {
    let positional: Vec<String> = request
        .arguments
        .iter()
        .filter(|argument| !argument.contains('='))
        .cloned()
        .collect();
    let Positional { output, rest } = split_output(&positional)?;
    if !(5..=6).contains(&rest.len()) {
        return Err(unsupported(
            ".noise needs v(out[,ref]) src {dec|oct|lin} pts fstart fstop [pts_per_summary]",
        ));
    }
    let node = |name: &str| {
        circuit.nodes().get(name).ok_or_else(|| {
            unsupported(format!(
                ".noise output node '{name}' is not in the circuit (C would create a floating \
                 node)"
            ))
        })
    };
    let positive = node(&output[0])?;
    let negative = match output.get(1) {
        Some(name) => node(name)?,
        None => NodeId::GROUND,
    };
    if positive == negative {
        return Err(unsupported(
            ".noise output nodes must differ (the output voltage is identically zero)",
        ));
    }
    let stepping = match rest[1].to_ascii_lowercase().as_str() {
        "dec" => Stepping::Decade,
        "oct" => Stepping::Octave,
        "lin" => Stepping::Linear,
        other => {
            return Err(unsupported(format!(
                ".noise sweep must be dec, oct or lin, not '{other}'"
            )));
        }
    };
    let steps = integer(Some(&rest[2]), "point count")?;
    if steps < 1 {
        return Err(unsupported(format!(
            ".noise needs at least one step, got {steps} (noisean.c)"
        )));
    }
    let start = number(Some(&rest[3]), "start frequency")?;
    let mut stop = number(Some(&rest[4]), "stop frequency")?;
    let summary = match rest.get(5) {
        Some(text) => {
            let value = integer(Some(text), "pts_per_summary")?;
            usize::try_from(value).map_err(|_| {
                unsupported(format!(
                    ".noise pts_per_summary must not be negative, got {value}"
                ))
            })?
        }
        None => 0,
    };
    if start <= 0. || stop < start {
        return Err(unsupported(format!(
            ".noise needs 0 < fstart <= fstop, got {start} .. {stop}"
        )));
    }
    let mut steps = usize::try_from(steps).map_err(|_| unsupported(".noise point count"))?;
    // noisean.c: a one-point linear sweep measures at the start frequency
    // only; any other sweep whose start and stop are within 3 ulps becomes a
    // one-point sweep there.
    if stepping == Stepping::Linear && steps == 1 {
        stop = start;
    } else if almost_equal_ulps(start, stop, 3) {
        stop = start;
        steps = 1;
    }
    if steps > MAX_POINTS {
        return Err(unsupported(format!(
            ".noise point count is limited to {MAX_POINTS}"
        )));
    }
    Ok(NoiseCard {
        positive,
        negative,
        input: rest[0].clone(),
        stepping,
        steps,
        start,
        stop,
        summary,
    })
}

/// `ninteg.c`'s per-step frequency data.
struct Step {
    del_freq: Real,
    ln_freq: Real,
    ln_last_freq: Real,
    del_ln_freq: Real,
}

/// C's `limexp`.
fn limexp(x: Real) -> Real {
    if x > 700. {
        700f64.exp() * (1. + x - 700.)
    } else {
        x.exp()
    }
}

/// `Nintegrate`: the integral of `a f^k` between the previous and the present
/// frequency, given the density and its logarithm at both ends.
fn nintegrate(density: Real, ln_density: Real, ln_last_density: Real, step: &Step) -> Real {
    let mut exponent = (ln_density - ln_last_density) / step.del_ln_freq;
    if exponent.abs() < N_INTFTHRESH {
        density * step.del_freq
    } else {
        let a = limexp(ln_density - exponent * step.ln_freq);
        exponent += 1.;
        if exponent.abs() < N_INTUSELOG {
            a * (step.ln_freq - step.ln_last_freq)
        } else {
            a * ((limexp(exponent * step.ln_freq) - limexp(exponent * step.ln_last_freq))
                / exponent)
        }
    }
}

/// Per-generator integration history (C `<dev>nVar`).
#[derive(Debug, Clone, Copy, Default)]
struct History {
    ln_last: Real,
    output: Real,
    input: Real,
}

/// One instance's generators and their history; the last history entry is
/// the instance total when the instance reports one.
struct Generators {
    instance: InstanceNoise,
    history: Vec<History>,
}

impl Generators {
    fn new(instance: InstanceNoise) -> Self {
        let count = instance.sources.len() + usize::from(instance.total);
        Self {
            instance,
            history: vec![History::default(); count],
        }
    }
}

/// The spectrum and integrated-plot columns (`CKTnoise` `N_OPEN` and the
/// devices' `NOISE_ADD_OUTVAR`); per-instance columns only with a summary.
fn plot_names(
    generators: &[Generators],
    input_kind: SourceKind,
    summary: bool,
) -> (Vec<Variable>, Vec<Variable>) {
    let (input_density, input_unit, input_prefix) = match input_kind {
        SourceKind::Voltage => ("voltage-density", "voltage", "v"),
        SourceKind::Current => ("current-density", "current", "i"),
    };
    let mut spectrum = vec![Variable::new("frequency", "frequency")];
    let mut integrated = Vec::new();
    for g in generators.iter().filter(|_| summary) {
        let suffixes = g
            .instance
            .sources
            .iter()
            .map(|source| source.suffix)
            .chain(g.instance.total.then_some(""));
        for suffix in suffixes {
            let name = format!("{}{suffix}", g.instance.name);
            spectrum.push(Variable::new(format!("onoise_{name}"), "voltage-density"));
            integrated.push(Variable::new(format!("v(onoise_total_{name})"), "voltage"));
            integrated.push(Variable::new(
                format!("{input_prefix}(inoise_total_{name})"),
                input_unit,
            ));
        }
    }
    spectrum.push(Variable::new("onoise_spectrum", "voltage-density"));
    spectrum.push(Variable::new("inoise_spectrum", input_density));
    integrated.push(Variable::new("v(onoise_total)", "voltage"));
    integrated.push(Variable::new(
        format!("{input_prefix}(inoise_total)"),
        input_unit,
    ));
    (spectrum, integrated)
}

/// The squared magnitude of `y[n1] - y[n2]`, ground being zero.
fn gain(circuit: &Circuit, y: &[Complex], nodes: [NodeId; 2]) -> Real {
    let at = |node: NodeId| {
        circuit
            .unknowns()
            .node_row(node)
            .and_then(|row| y.get(row).copied())
            .unwrap_or(Complex::ZERO)
    };
    let difference = at(nodes[0]) - at(nodes[1]);
    difference.re * difference.re + difference.im * difference.im
}

/// The unit forcing of the input source alone (C `MODEACNOISE`).
fn input_rhs(system: &LinearSystem, input: &str) -> SpiceResult<Vec<Complex>> {
    let source = system
        .sources
        .iter()
        .find(|source| source.name.eq_ignore_ascii_case(input))
        .ok_or_else(|| SpiceError::circuit(format!("noise input source {input} has no AC row")))?;
    let mut rhs = vec![Complex::ZERO; system.a.rows()];
    for (row, sign) in &source.rows {
        rhs[*row] = rhs[*row] + Complex::real(*sign);
    }
    Ok(rhs)
}

/// Runs `.noise` and returns its plots: the spectrum and, unless the sweep is
/// a single frequency, the integrated noise.
pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Vec<Plot>> {
    let settings = crate::analysis::bias::DcSettings::from_request(request)?;
    let reltol = match request.named("rtol") {
        Some(text) => number(Some(text), "rtol")?,
        None => DEFAULT_RELTOL,
    };
    circuit.finalize()?;
    let card = parse(circuit, request)?;
    let input = circuit
        .device(&card.input)
        .ok_or_else(|| {
            unsupported(format!(
                ".noise input source {} is not in the circuit",
                card.input
            ))
        })?
        .input_source()
        .ok_or_else(|| {
            unsupported(format!(
                ".noise input {} is not an independent voltage or current source",
                card.input
            ))
        })?;
    if !input.ac_given {
        return Err(unsupported(format!(
            ".noise input source {} has no AC value (noisean.c E_NOACINPUT)",
            card.input
        )));
    }
    // The operating point and small-signal system exactly as `.ac` builds
    // them (`noisean.c`: `CKTop`, then a `MODEINITSMSIG` load; a
    // `hertz`-dependent circuit is re-solved at every frequency).
    let mut small = crate::analysis::ac::SmallSignal::prepare(circuit, request, context, settings)?;
    let mut model = context.model_context();
    model.spice3_noise = request.spice3_noise;
    let mut generators: Vec<Generators> =
        circuit_noise(circuit, &model, small.bias(), Some(small.state()))?
            .into_iter()
            .map(Generators::new)
            .collect();

    let (spectrum_variables, integrated_variables) =
        plot_names(&generators, input.kind, card.summary != 0);
    let mut spectrum = Plot::new("noise1", SPECTRUM_PLOTNAME, PlotFlags::Real);
    for variable in spectrum_variables {
        spectrum.push_variable(variable);
    }
    let output = [card.positive, card.negative];
    let mut adjoint_rhs = vec![Complex::ZERO; circuit.unknown_count()];
    if let Some(row) = circuit.unknowns().node_row(card.positive) {
        adjoint_rhs[row] = Complex::real(1.);
    }
    if let Some(row) = circuit.unknowns().node_row(card.negative) {
        adjoint_rhs[row] = Complex::real(-1.);
    }

    let delta = match card.stepping {
        Stepping::Decade => (10f64.ln() / card.steps as Real).exp(),
        Stepping::Octave => (2f64.ln() / card.steps as Real).exp(),
        Stepping::Linear if card.steps == 1 => 0.,
        Stepping::Linear => (card.stop - card.start) / (card.steps - 1) as Real,
    };
    let tolerance = match card.stepping {
        Stepping::Decade | Stepping::Octave => delta * card.stop * reltol,
        Stepping::Linear => delta * reltol,
    };
    let (mut out_noise, mut in_noise) = (0., 0.);
    let mut freq = card.start;
    let mut last = freq;
    let mut step = 0_usize;
    while freq <= card.stop + tolerance {
        if step >= MAX_POINTS {
            return Err(unsupported(format!(
                ".noise frequency loop exceeded {MAX_POINTS} points"
            )));
        }
        let input_name = card.input.as_str();
        let (forward, adjoint) = small.at_frequency(circuit, context, freq, |system| {
            let matrix = ComplexMatrix::from_operators(
                &system.a,
                &system.e,
                2. * std::f64::consts::PI * freq,
            )?;
            let lu = matrix.factorize()?;
            Ok((
                lu.solve(&input_rhs(system, input_name)?)?,
                lu.solve_transposed(&adjoint_rhs)?,
            ))
        })?;
        if small.varies() {
            let local = model.with_frequency(freq);
            let fresh = circuit_noise(circuit, &local, small.bias(), Some(small.state()))?;
            if fresh.len() != generators.len() {
                return Err(SpiceError::circuit(
                    "noise generators changed between frequencies",
                ));
            }
            for (g, instance) in generators.iter_mut().zip(fresh) {
                g.instance = instance;
            }
        }
        let gain_sq_inv = 1. / gain(circuit, &forward, output).max(N_MINGAIN);
        let ln_gain_inv = gain_sq_inv.ln();
        let data = Step {
            del_freq: freq - last,
            ln_freq: freq.max(N_MINLOG).ln(),
            ln_last_freq: last.max(N_MINLOG).ln(),
            del_ln_freq: freq.max(N_MINLOG).ln() - last.max(N_MINLOG).ln(),
        };
        let print_summary = card.summary != 0 && step.is_multiple_of(card.summary);
        let mut out_density = 0.;
        let mut row = vec![freq];
        for g in &mut generators {
            let mut densities = Vec::with_capacity(g.history.len());
            for source in &g.instance.sources {
                densities.push(
                    source
                        .kind
                        .output_density(gain(circuit, &adjoint, source.nodes), freq),
                );
            }
            let total: Real = densities.iter().sum();
            if g.instance.total {
                densities.push(total);
            }
            out_density += total;
            let generators_only = g.instance.sources.len();
            for (index, density) in densities.iter().enumerate() {
                let ln_density = density.max(N_MINLOG).ln();
                let history = &mut g.history[index];
                if data.del_freq == 0. {
                    history.ln_last = ln_density;
                    if freq == card.start {
                        history.output = 0.;
                        history.input = 0.;
                    }
                } else if index < generators_only {
                    let out = nintegrate(*density, ln_density, history.ln_last, &data);
                    let inp = nintegrate(
                        density * gain_sq_inv,
                        ln_density + ln_gain_inv,
                        history.ln_last + ln_gain_inv,
                        &data,
                    );
                    history.ln_last = ln_density;
                    out_noise += out;
                    in_noise += inp;
                    if card.summary != 0 {
                        history.output += out;
                        history.input += inp;
                        if g.instance.total {
                            let total = &mut g.history[generators_only];
                            total.output += out;
                            total.input += inp;
                        }
                    }
                }
            }
            if print_summary {
                row.extend(densities.iter().map(|d| d.sqrt()));
            }
        }
        if card.summary == 0 || print_summary {
            row.push(out_density.sqrt());
            row.push((out_density * gain_sq_inv).sqrt());
            spectrum.push_point(row.into_iter().map(Complex::real).collect())?;
        }
        last = freq;
        match card.stepping {
            Stepping::Decade | Stepping::Octave => freq *= delta,
            Stepping::Linear => freq += delta,
        }
        step += 1;
        if card.steps == 1 && card.stepping == Stepping::Linear {
            break;
        }
    }
    if !spectrum.is_finite() {
        return Err(SpiceError::Numerical {
            context: ".noise".into(),
            message: "nonfinite noise density".into(),
        });
    }
    let mut plots = vec![spectrum];
    if card.start != card.stop {
        let mut integrated = Plot::new("noise2", INTEGRATED_PLOTNAME, PlotFlags::Real);
        let mut values = Vec::new();
        for variable in integrated_variables {
            integrated.push_variable(variable);
        }
        if card.summary != 0 {
            for g in &generators {
                for history in &g.history {
                    values.push(history.output.sqrt());
                    values.push(history.input.sqrt());
                }
            }
        }
        values.push(out_noise.sqrt());
        values.push(in_noise.sqrt());
        integrated.push_point(values.into_iter().map(Complex::real).collect())?;
        if !integrated.is_finite() {
            return Err(SpiceError::Numerical {
                context: ".noise".into(),
                message: "nonfinite integrated noise".into(),
            });
        }
        plots.push(integrated);
    }
    if request.squared_noise {
        for plot in &mut plots {
            plot.plotname = if plot.plotname == SPECTRUM_PLOTNAME {
                "Noise Spectral Density Curves - (V^2 or A^2)/Hz"
            } else {
                "Integrated Noise - V^2 or A^2"
            }
            .to_owned();
            for (index, variable) in plot.variables.iter_mut().enumerate() {
                if variable.name == "frequency" {
                    continue;
                }
                if let Some(inner) = variable
                    .name
                    .strip_prefix("v(")
                    .or_else(|| variable.name.strip_prefix("i("))
                    .and_then(|n| n.strip_suffix(')'))
                {
                    variable.name = inner.to_owned();
                }
                variable.unit = match variable.unit.as_str() {
                    "voltage-density" => "voltage^2-density",
                    "current-density" => "current^2-density",
                    "voltage" => "voltage^2",
                    "current" => "current^2",
                    other => other,
                }
                .to_owned();
                for point in &mut plot.points {
                    point[index].re *= point[index].re;
                }
            }
        }
    }
    Ok(plots)
}

#[cfg(test)]
mod tests {
    use super::{Step, almost_equal_ulps, nintegrate, plot_count};

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn plot_count_follows_the_single_frequency_rules() {
        assert_eq!(plot_count(&args("v ( out ) v1 dec 10 1 1k")), 2);
        assert_eq!(plot_count(&args("v ( out ) v1 dec 10 1k 1k")), 1);
        assert_eq!(plot_count(&args("v ( out , ref ) v1 lin 1 1 1k 2")), 1);
        assert_eq!(plot_count(&args("v ( out ) v1 lin 2 1 1k rtol=1e-3")), 2);
        assert_eq!(plot_count(&args("garbage")), 2);
    }

    #[test]
    fn ulps_comparison_matches_c() {
        assert!(almost_equal_ulps(1.0, 1.0, 3));
        assert!(almost_equal_ulps(1.0, 1.0 + 2.0 * f64::EPSILON, 3));
        assert!(!almost_equal_ulps(1.0, 1.0 + 8.0 * f64::EPSILON, 3));
        assert!(!almost_equal_ulps(1.0, -1.0, 3));
    }

    #[test]
    fn nintegrate_is_exact_for_power_laws() {
        // A flat density integrates to density * df.
        let (f0, f1) = (10.0_f64, 100.0_f64);
        let step = Step {
            del_freq: f1 - f0,
            ln_freq: f1.ln(),
            ln_last_freq: f0.ln(),
            del_ln_freq: f1.ln() - f0.ln(),
        };
        let flat = nintegrate(2.0, 2f64.ln(), 2f64.ln(), &step);
        assert!((flat - 180.0).abs() < 1e-12);
        // 1/f integrates to ln(f1/f0).
        let pink = nintegrate(1.0 / f1, (1.0 / f1).ln(), (1.0 / f0).ln(), &step);
        assert!((pink - 10f64.ln()).abs() < 1e-12);
        // 1/f^2 integrates to 1/f0 - 1/f1.
        let brown = nintegrate(
            1.0 / (f1 * f1),
            (1.0 / (f1 * f1)).ln(),
            (1.0 / (f0 * f0)).ln(),
            &step,
        );
        assert!((brown - (1.0 / f0 - 1.0 / f1)).abs() < 1e-12);
    }
}
