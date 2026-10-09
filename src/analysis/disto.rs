//! `.disto` — small-signal distortion analysis (Volterra series).
//!
//! C references (behaviour only, reimplemented): `distoan.c` (`DISTOan`: the
//! frequency loop, the solves and the plots), `cktdisto.c` (`CKTdisto`: the
//! source inputs and the device dispatch), `dkerproc.c` (`DkerProc`: the
//! output scaling), `dloadfns.c` (the kernel polynomials, see
//! [`crate::devices::distortion::add_products`]), `dsetparm.c`/`inp2dot.c`
//! (the card) and the device routines summarised in
//! [`crate::devices::distortion`].
//!
//! ```text
//! .disto {dec|oct|lin} pts fstart fstop [f2overf1]
//! ```
//!
//! The operating point is solved as for `.ac` and every device describes its
//! nonlinearities there as Taylor polynomials. At every `f1` of C's sweep the
//! linearized system `A + j omega E` is solved for the first-order kernel
//! `H1(f1)`, driven by the `distof1` inputs as the phasors `0.5 mag e^(j
//! phase)`, then for each higher-order kernel with a right-hand side built
//! from the lower-order ones:
//!
//! * without `f2overf1`: `H2(f1, f1)` at `2 f1` and `H3(f1, f1, f1)` at `3 f1`,
//!   written (times 2, `DkerProc`) as the plots `DISTORTION - 2nd harmonic`
//!   and `DISTORTION - 3rd harmonic`;
//! * with `f2overf1`: the second input `H1(f2)` at `f2 = f2overf1 * fstart`,
//!   **fixed** over the sweep as in C (`distoan.c`: "keeping f2 const to be
//!   compatible with spectre"), driven by the `distof2` inputs, and
//!   `H2(f1, f1)`; then `H2(f1, f2)` at `f1 + f2`, `H2(f1, -f2)` at `f1 - f2`
//!   and `H3(f1, f1, -f2)` at `2 f1 - f2`, written (times 4, 4 and 6) as
//!   `DISTORTION - IM: f1+f2`, `DISTORTION - IM: f1-f2` and
//!   `DISTORTION - IM: 2f1-f2`.
//!
//! Each plot is complex, along the `frequency` scale `f1`, with every node
//! voltage and branch current exactly as an `.ac` plot. The sweep is C's own:
//! `dec`/`oct` multiply by `exp(ln(10 or 2)/pts)` from `fstart` while
//! `f <= fstop + delta*fstop*reltol`, and `lin` adds `(fstop - fstart)/(pts +
//! 1)` (so it measures `pts + 2` frequencies) while `f <= fstop +
//! delta*reltol`.

use crate::analysis::linear::{number, plot, unsupported};
use crate::analysis::results::Plot;
use crate::analysis::{AnalysisContext, AnalysisRequest};
use crate::devices::distortion::{
    CircuitDistortion, DistortionInput, Kernels, Product, add_products, circuit_distortion,
};
use crate::devices::{Circuit, LinearSystem, SourceKind};
use crate::maths::complex::ComplexMatrix;
use crate::primitives::{Complex, Real, SpiceError, SpiceResult, parse_spice_number};

/// C's default `CKTreltol`, the frequency-loop tolerance without `.option
/// reltol`.
const DEFAULT_RELTOL: Real = 1e-3;
/// The port's bound on the number of frequency points.
const MAX_POINTS: usize = 100_000;
/// The harmonic plot titles (`distoan.c`).
pub const HARMONIC_PLOTNAMES: [&str; 2] =
    ["DISTORTION - 2nd harmonic", "DISTORTION - 3rd harmonic"];
/// The intermodulation plot titles (`distoan.c`).
pub const INTERMODULATION_PLOTNAMES: [&str; 3] = [
    "DISTORTION - IM: f1+f2",
    "DISTORTION - IM: f1-f2",
    "DISTORTION - IM: 2f1-f2",
];

/// The frequency stepping of a `.disto` card (`DISTOAN.DstepType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stepping {
    Decade,
    Octave,
    Linear,
}

/// A parsed `.disto` card.
#[derive(Debug, Clone, PartialEq)]
struct DistoCard {
    stepping: Stepping,
    steps: usize,
    start: Real,
    stop: Real,
    f2_over_f1: Option<Real>,
}

fn positional(arguments: &[String]) -> Vec<&str> {
    arguments
        .iter()
        .filter(|argument| !argument.contains('='))
        .map(String::as_str)
        .collect()
}

/// How many plots a `.disto` card produces: two harmonic plots, or three
/// intermodulation plots when the card gives `f2overf1`. Used by the batch
/// scheduler to name the plots before anything runs.
#[must_use]
pub fn plot_count(arguments: &[String]) -> usize {
    if positional(arguments).len() >= 5 {
        3
    } else {
        2
    }
}

/// C's `INPgetValue(IF_INTEGER)`: `floor(value + 0.5)`.
fn integer(text: &str) -> SpiceResult<i64> {
    let value = parse_spice_number(text)
        .filter(|v| v.is_finite())
        .ok_or_else(|| unsupported(".disto expects an integer point count"))?;
    let rounded = (value + 0.5).floor();
    if rounded.abs() > Real::from(i32::MAX) {
        return Err(unsupported(".disto point count is out of range"));
    }
    Ok(rounded as i64)
}

fn parse(request: &AnalysisRequest) -> SpiceResult<DistoCard> {
    let syntax = || unsupported(".disto needs {dec|oct|lin} pts fstart fstop [f2overf1]");
    let args = positional(&request.arguments);
    if !(4..=5).contains(&args.len()) {
        return Err(syntax());
    }
    let stepping = match args[0].to_ascii_lowercase().as_str() {
        "dec" => Stepping::Decade,
        "oct" => Stepping::Octave,
        "lin" => Stepping::Linear,
        other => {
            return Err(unsupported(format!(
                ".disto sweep must be dec, oct or lin, not '{other}'"
            )));
        }
    };
    let steps = integer(args[1])?;
    // dec/oct with no step never advance (`distoan.c` would loop forever); a
    // `lin` sweep with zero steps measures fstart and fstop.
    let minimum = if stepping == Stepping::Linear { 0 } else { 1 };
    if steps < minimum {
        return Err(unsupported(format!(
            ".disto needs at least {minimum} step(s) for this sweep, got {steps}"
        )));
    }
    let start = number(Some(args[2]), "start frequency")?;
    let stop = number(Some(args[3]), "stop frequency")?;
    if start <= 0. || stop <= 0. {
        // dsetparm.c: "Frequency of 0 is invalid".
        return Err(unsupported(format!(
            ".disto frequencies must be positive, got {start} .. {stop} (dsetparm.c)"
        )));
    }
    if stop < start {
        return Err(unsupported(format!(
            ".disto needs fstart <= fstop, got {start} .. {stop} (C writes empty plots)"
        )));
    }
    let f2_over_f1 = match args.get(4) {
        Some(text) => Some(number(Some(text), "f2overf1")?),
        None => None,
    };
    Ok(DistoCard {
        stepping,
        steps: usize::try_from(steps).map_err(|_| syntax())?,
        start,
        stop,
        f2_over_f1,
    })
}

/// `distoan.c`'s frequency sweep.
fn frequencies(card: &DistoCard, reltol: Real) -> SpiceResult<Vec<Real>> {
    let n = card.steps as Real;
    let (delta, tolerance) = match card.stepping {
        Stepping::Decade => {
            let delta = (10f64.ln() / n).exp();
            (delta, delta * card.stop * reltol)
        }
        Stepping::Octave => {
            let delta = (2f64.ln() / n).exp();
            (delta, delta * card.stop * reltol)
        }
        Stepping::Linear => {
            let delta = (card.stop - card.start) / (n + 1.);
            (delta, delta * reltol)
        }
    };
    let mut grid = Vec::new();
    let mut freq = card.start;
    while freq <= card.stop + tolerance {
        if grid.len() >= MAX_POINTS {
            return Err(unsupported(format!(
                ".disto frequency loop exceeded {MAX_POINTS} points"
            )));
        }
        grid.push(freq);
        match card.stepping {
            Stepping::Decade | Stepping::Octave => {
                freq *= delta;
                if delta == 1. {
                    break;
                }
            }
            Stepping::Linear => {
                freq += delta;
                if delta == 0. {
                    break;
                }
            }
        }
    }
    Ok(grid)
}

/// Solves `(A + j omega E) x = rhs`; a negative `omega` (`f1 - f2` below
/// `f2`) is solved as the conjugate of the positive-frequency system, which is
/// exact for the real operators `A` and `E`.
fn solve_at(system: &LinearSystem, omega: Real, rhs: &[Complex]) -> SpiceResult<Vec<Complex>> {
    let flip = omega < 0.;
    let matrix = ComplexMatrix::from_operators(&system.a, &system.e, omega.abs())?;
    let lu = matrix.factorize()?;
    if rhs.iter().all(|value| *value == Complex::ZERO) {
        return Ok(vec![Complex::ZERO; rhs.len()]);
    }
    if flip {
        let conjugated: Vec<Complex> = rhs.iter().map(|value| value.conj()).collect();
        Ok(lu
            .solve(&conjugated)?
            .into_iter()
            .map(Complex::conj)
            .collect())
    } else {
        lu.solve(rhs)
    }
}

/// The first-order forcing of the `distof1` (`second = false`) or `distof2`
/// inputs (`cktdisto.c` `D_RHSF1`/`D_RHSF2`).
fn input_rhs(
    system: &LinearSystem,
    distortion: &CircuitDistortion,
    second: bool,
) -> SpiceResult<Vec<Complex>> {
    let mut rhs = vec![Complex::ZERO; system.a.rows()];
    for (name, f1, f2) in &distortion.inputs {
        let Some(input): Option<DistortionInput> = (if second { *f2 } else { *f1 }) else {
            continue;
        };
        let source = system
            .sources
            .iter()
            .find(|source| source.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                SpiceError::circuit(format!("distortion input source {name} has no AC row"))
            })?;
        // cktdisto.c drives a current source's input as `rhs[pos] = -0.5 mag
        // e^(j phase)`, `rhs[neg] = +...`: the opposite of the AC and DC
        // stamps (`isrcacld.c`, `isrcload.c` add the value at `pos`), so a
        // current input enters with the inverted sign. A voltage source sets
        // its branch row as its AC stamp does.
        let phasor = match source.kind {
            SourceKind::Voltage => input.phasor(),
            SourceKind::Current => {
                let p = input.phasor();
                Complex::new(-p.re, -p.im)
            }
        };
        for (row, sign) in &source.rows {
            rhs[*row] = rhs[*row] + Complex::new(sign * phasor.re, sign * phasor.im);
        }
    }
    Ok(rhs)
}

fn scaled(values: &[Complex], k: Real) -> Vec<Complex> {
    values
        .iter()
        .map(|value| Complex::new(k * value.re, k * value.im))
        .collect()
}

/// Runs `.disto` and returns its plots: the 2nd and 3rd harmonics, or the
/// three intermodulation products with `f2overf1`.
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
    let card = parse(request)?;
    let grid = frequencies(&card, reltol)?;
    let small = crate::analysis::ac::SmallSignal::prepare(circuit, request, context, settings)?;
    if small.varies() {
        return Err(unsupported(
            ".disto of a circuit with a `hertz`-dependent device (distoan.c linearizes once, \
             at DC)",
        ));
    }
    let model = context.model_context();
    let distortion = circuit_distortion(circuit, &model, small.bias())?;
    let system = small.system();
    let unknowns = circuit.unknowns();
    let rhs_f1 = input_rhs(system, &distortion, false)?;
    let intermodulation = card.f2_over_f1.is_some();
    let rhs_f2 = if intermodulation {
        if !distortion.inputs.iter().any(|(_, _, f2)| f2.is_some()) {
            // distoan.c E_NOF2SRC.
            return Err(unsupported(
                ".disto with f2overf1 needs a source with a distof2 input (distoan.c: No \
                 source with f2 distortion input)",
            ));
        }
        Some(input_rhs(system, &distortion, true)?)
    } else {
        None
    };
    let two_pi = 2. * std::f64::consts::PI;
    let omega2 = two_pi * card.start * card.f2_over_f1.unwrap_or(0.);
    let titles: &[&str] = if intermodulation {
        &INTERMODULATION_PLOTNAMES
    } else {
        &HARMONIC_PLOTNAMES
    };
    let mut plots = titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            plot(
                circuit,
                &format!("disto{}", index + 1),
                title,
                Some(("frequency", "frequency")),
                true,
            )
        })
        .collect::<SpiceResult<Vec<_>>>()?;
    let n = circuit.unknown_count();
    let products = |product: Product, kernels: Kernels<'_>, omega: Real| {
        let mut rhs = vec![Complex::ZERO; n];
        add_products(
            &distortion.terms,
            product,
            kernels,
            omega,
            unknowns,
            &mut rhs,
        )?;
        solve_at(system, omega, &rhs)
    };
    for freq in grid {
        let omega1 = two_pi * freq;
        let h1 = solve_at(system, omega1, &rhs_f1)?;
        let base = Kernels {
            h1: &h1,
            h1_f2: None,
            h2: None,
            h2_minus: None,
        };
        let h2 = products(Product::TwoF1, base, 2. * omega1)?;
        let outputs: Vec<Vec<Complex>> = match &rhs_f2 {
            None => {
                let h3 = products(
                    Product::ThreeF1,
                    Kernels {
                        h2: Some(&h2),
                        ..base
                    },
                    3. * omega1,
                )?;
                vec![scaled(&h2, 2.), scaled(&h3, 2.)]
            }
            Some(rhs_f2) => {
                let h1_f2 = solve_at(system, omega2, rhs_f2)?;
                let with_f2 = Kernels {
                    h1_f2: Some(&h1_f2),
                    ..base
                };
                let sum = products(Product::F1PlusF2, with_f2, omega1 + omega2)?;
                let difference = products(Product::F1MinusF2, with_f2, omega1 - omega2)?;
                let third = products(
                    Product::TwoF1MinusF2,
                    Kernels {
                        h2: Some(&h2),
                        h2_minus: Some(&difference),
                        ..with_f2
                    },
                    2. * omega1 - omega2,
                )?;
                vec![
                    scaled(&sum, 4.),
                    scaled(&difference, 4.),
                    scaled(&third, 6.),
                ]
            }
        };
        for (plot, values) in plots.iter_mut().zip(outputs) {
            let mut point = Vec::with_capacity(values.len() + 1);
            point.push(Complex::real(freq));
            point.extend(values);
            plot.push_point(point)?;
        }
    }
    if plots.iter().any(|plot| !plot.is_finite()) {
        return Err(SpiceError::Numerical {
            context: ".disto".into(),
            message: "nonfinite distortion response".into(),
        });
    }
    Ok(plots)
}

#[cfg(test)]
mod tests {
    use super::{DistoCard, Stepping, frequencies, plot_count};

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn plot_count_follows_f2overf1() {
        assert_eq!(plot_count(&args("dec 10 1k 100k")), 2);
        assert_eq!(plot_count(&args("dec 10 1k 100k 0.9")), 3);
        assert_eq!(plot_count(&args("dec 10 1k 100k rtol=1e-3")), 2);
    }

    #[test]
    fn sweeps_follow_distoan() {
        let card = |stepping, steps, start, stop| DistoCard {
            stepping,
            steps,
            start,
            stop,
            f2_over_f1: None,
        };
        // lin measures pts + 2 frequencies.
        let lin = frequencies(&card(Stepping::Linear, 3, 1e3, 4e3), 1e-3).unwrap();
        assert_eq!(lin, vec![1e3, 1.75e3, 2.5e3, 3.25e3, 4e3]);
        let dec = frequencies(&card(Stepping::Decade, 2, 1e3, 1e5), 1e-3).unwrap();
        assert_eq!(dec.len(), 5);
        assert!((dec[4] / 1e5 - 1.).abs() < 1e-12);
        let one = frequencies(&card(Stepping::Linear, 0, 1e3, 1e3), 1e-3).unwrap();
        assert_eq!(one, vec![1e3]);
        let oct = frequencies(&card(Stepping::Octave, 1, 1., 8.), 1e-3).unwrap();
        assert_eq!(oct.len(), 4);
    }
}
