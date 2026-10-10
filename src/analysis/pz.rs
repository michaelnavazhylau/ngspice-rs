//! `.pz` pole-zero analysis of the small-signal transfer function.
//!
//! ```text
//! .pz in+ in- out+ out- {cur|vol} {pol|zer|pz}
//! ```
//!
//! C references (behaviour only): `src/spicelib/analysis/pzan.c` (`PZan`,
//! `PZinit`, `PZpost`), `cktpzset.c` (`CKTpzSetup`: which nodes drive and
//! which column is replaced), `cktpzld.c` (`CKTpzLoad`), the device
//! `*pzld.c` loads and `src/spicelib/parser/inp2dot.c`/`pzsetp.c` (the card).
//!
//! # What is computed
//!
//! After the operating point, the circuit is linearised exactly as for AC
//! (`MODEINITSMSIG`) into the pencil `Y(s) = A + s E`
//! ([`Circuit::pole_zero_system_at`]): every C pole-zero load is affine in
//! `s`. An independent voltage source with an AC value is removed (its
//! current forced to zero, `vsrcpzld.c`): the analysis drives the input with
//! a unit current instead. `CKTpzSetup`/`CKTpzLoad` then modify `Y`:
//!
//! * the *solution column* (the output node, or the input node for `vol`
//!   poles) is zeroed and receives the drive `+1`/`-1` in the input rows; a
//!   non-ground negative output node first has the solution column added to
//!   its own column (`SMPcAddCol`), so the unknown becomes the differential
//!   output voltage;
//! * `cur` poles use `Y` itself, unmodified.
//!
//! By Cramer's rule the modified determinant is `det Y(s)` times the
//! transimpedance from the input current to the solution voltage, so its
//! roots are the zeros of `Vout/Iin` (`zer`) and the zeros of `Vin/Iin`,
//! i.e. the poles of `Vout/Vin` (`vol pol`); `cur pol` takes the roots of
//! `det Y(s)`. C searches for these roots with a deflated Muller iteration;
//! the port computes them as the finite eigenvalues of the modified pencil
//! ([`crate::maths::pencil`]), see `docs/port/POLE_ZERO_ADR.md` for the
//! decision and every divergence (ordering, accuracy, C's search failures,
//! singular pencils).
//!
//! # Output
//!
//! One plot `Pole-Zero Analysis` with a single complex point: `pole(1)`,
//! `pole(2)`, ... then `zero(1)`, ..., named as C's rawfile writes them
//! (`v(pole(1))`, type `voltage`). Roots are ordered as `PZpost` lists them
//! (ascending real part, then imaginary part, each complex root followed by
//! its conjugate). A plot without roots has no variables and is flagged
//! `real`, as C writes it.

use crate::analysis::linear::unsupported;
use crate::analysis::results::{PlotFlags, Variable};
use crate::analysis::{AnalysisRequest, Plot};
use crate::devices::Circuit;
use crate::maths::Matrix;
use crate::maths::pencil::pencil_roots;
use crate::primitives::{Complex, NodeId, SpiceError, SpiceResult};

/// `cur` or `vol` (C `PZ_IN_CUR`/`PZ_IN_VOL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Input {
    Current,
    Voltage,
}

/// The parsed card: four node names, the input type and what to compute.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Card {
    nodes: [String; 4],
    input: Input,
    poles: bool,
    zeros: bool,
}

fn card(request: &AnalysisRequest) -> SpiceResult<Card> {
    let positional: Vec<&String> = request
        .arguments
        .iter()
        .filter(|a| !a.contains('='))
        .collect();
    let usage = || unsupported(".pz in+ in- out+ out- cur|vol pol|zer|pz");
    if positional.len() != 6 {
        return Err(usage());
    }
    // `INPapName` matches the two flags by name, so C accepts them in either
    // order; each kind must still be given exactly once here.
    let (mut input, mut which) = (None, None);
    for flag in &positional[4..] {
        match flag.to_ascii_lowercase().as_str() {
            "cur" if input.is_none() => input = Some(Input::Current),
            "vol" if input.is_none() => input = Some(Input::Voltage),
            "pol" if which.is_none() => which = Some((true, false)),
            "zer" if which.is_none() => which = Some((false, true)),
            "pz" if which.is_none() => which = Some((true, true)),
            _ => return Err(usage()),
        }
    }
    let (Some(input), Some((poles, zeros))) = (input, which) else {
        return Err(usage());
    };
    Ok(Card {
        nodes: [0, 1, 2, 3].map(|i| positional[i].clone()),
        input,
        poles,
        zeros,
    })
}

/// A card node: its id (for C's node-number comparisons) and matrix row
/// (`None` for ground).
fn node(circuit: &Circuit, name: &str) -> SpiceResult<(NodeId, Option<usize>)> {
    let id = circuit.nodes().get(name).ok_or_else(|| {
        unsupported(format!(
            ".pz node '{name}' does not exist in the circuit (C would create an isolated node)"
        ))
    })?;
    Ok((id, circuit.unknowns().node_row(id)))
}

fn short(message: &str) -> SpiceError {
    SpiceError::circuit(format!("pole-zero analysis: {message}"))
}

/// The pencil whose roots are the requested poles or zeros, built as
/// `CKTpzSetup` + `CKTpzLoad` modify the circuit matrix.
fn modified_pencil(
    a: &Matrix,
    e: &Matrix,
    input: [Option<usize>; 2],
    output: [Option<usize>; 2],
) -> SpiceResult<(Matrix, Matrix)> {
    let (mut a, mut e) = (a.clone(), e.clone());
    let n = a.rows();
    let [mut input_pos, mut input_neg] = input;
    let (solution, balance) = match output {
        [Some(pos), neg] => (Some(pos), neg),
        [None, neg] => {
            // `CKTpzSetup`: a ground positive output takes the negative node
            // as the solution column and swaps the drive.
            std::mem::swap(&mut input_pos, &mut input_neg);
            (neg, None)
        }
    };
    let Some(solution) = solution else {
        return Ok((a, e));
    };
    for m in [&mut a, &mut e] {
        if let Some(balance) = balance {
            for row in 0..n {
                let value = m.get(row, solution).unwrap_or(0.0);
                m.add_to(row, balance, value)?;
            }
        }
        for row in 0..n {
            m.set(row, solution, 0.0)?;
        }
    }
    if let Some(row) = input_pos {
        a.set(row, solution, 1.0)?;
    }
    if let Some(row) = input_neg {
        a.set(row, solution, -1.0)?;
    }
    Ok((a, e))
}

/// The finite roots of a modified pencil. A pencil whose determinant
/// vanishes for every `s` has no defined roots; C reports that case (its
/// search finds a "root" at every trial) as `E_SHORT`.
fn roots(a: &Matrix, e: &Matrix, what: &str) -> SpiceResult<Vec<Complex>> {
    pencil_roots(a, e)
        .map(|roots| roots.roots)
        .map_err(|error| match error {
            SpiceError::Numerical { message, .. } if message.starts_with("singular pencil") => {
                SpiceError::Numerical {
                    context: format!("pole-zero analysis ({what})"),
                    message: format!(
                        "{message}; the transfer function is identically zero or undefined \
                         (C: \"The input signal is shorted on the way to the output\")"
                    ),
                }
            }
            SpiceError::Numerical { message, .. } => SpiceError::Numerical {
                context: format!("pole-zero analysis ({what})"),
                message,
            },
            other => other,
        })
}

fn dense(matrix: &crate::maths::SparseMatrix) -> SpiceResult<Matrix> {
    let mut dense = Matrix::zeros(matrix.rows(), matrix.cols());
    for triplet in matrix.triplets() {
        dense.add_to(triplet.row, triplet.col, triplet.value)?;
    }
    Ok(dense)
}

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Plot> {
    let settings = crate::analysis::bias::DcSettings::from_request(request)?;
    let card = card(request)?;
    circuit.finalize()?;
    let [
        (in_pos_id, in_pos),
        (in_neg_id, in_neg),
        (out_pos_id, out_pos),
        (out_neg_id, out_neg),
    ] = [
        node(circuit, &card.nodes[0])?,
        node(circuit, &card.nodes[1])?,
        node(circuit, &card.nodes[2])?,
        node(circuit, &card.nodes[3])?,
    ];
    // `PZinit`.
    if in_pos_id == in_neg_id {
        return Err(short("Input is shorted"));
    }
    if out_pos_id == out_neg_id {
        return Err(short("Output is shorted"));
    }
    if card.input == Input::Voltage {
        if in_pos_id == out_pos_id && in_neg_id == out_neg_id {
            return Err(short("Transfer function is unity"));
        }
        if in_pos_id == out_neg_id && in_neg_id == out_pos_id {
            return Err(short("Transfer function is -1"));
        }
    }
    // PZan uses the same CKTop/MODEINITSMSIG preparation as acan.c.
    let small_signal =
        crate::analysis::ac::SmallSignal::prepare(circuit, request, context, settings)?;
    let system = circuit.pole_zero_system_at(
        &context.model_context(),
        small_signal.bias(),
        Some(small_signal.state()),
    )?;
    let (a, e) = (dense(&system.a)?, dense(&system.e)?);
    // `PZan` runs the pole search before the zero search; `PZpost` lists
    // poles first.
    let mut poles = Vec::new();
    if card.poles {
        let (a, e) = match card.input {
            Input::Voltage => modified_pencil(&a, &e, [in_pos, in_neg], [in_pos, in_neg])?,
            Input::Current => (a.clone(), e.clone()),
        };
        poles = roots(&a, &e, "poles")?;
    }
    let mut zeros = Vec::new();
    if card.zeros {
        let (a, e) = modified_pencil(&a, &e, [in_pos, in_neg], [out_pos, out_neg])?;
        zeros = roots(&a, &e, "zeros")?;
    }
    let flags = if poles.is_empty() && zeros.is_empty() {
        PlotFlags::Real
    } else {
        PlotFlags::Complex
    };
    let mut plot = Plot::new("pz1", "Pole-Zero Analysis", flags);
    for (index, _) in poles.iter().enumerate() {
        plot.push_variable(Variable::complex(
            format!("v(pole({}))", index + 1),
            "voltage",
        ));
    }
    for (index, _) in zeros.iter().enumerate() {
        plot.push_variable(Variable::complex(
            format!("v(zero({}))", index + 1),
            "voltage",
        ));
    }
    poles.extend(zeros);
    plot.push_point(poles)?;
    Ok(plot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(arguments: &[&str]) -> AnalysisRequest {
        AnalysisRequest::with_arguments(
            crate::primitives::AnalysisKind::PoleZero,
            arguments.iter().copied(),
        )
    }

    #[test]
    fn cards_take_both_flags_in_either_order() {
        let parsed = card(&request(&["in", "0", "out", "0", "vol", "pz"])).unwrap();
        assert_eq!(parsed.input, Input::Voltage);
        assert!(parsed.poles && parsed.zeros);
        let parsed = card(&request(&["in", "0", "out", "0", "ZER", "CUR"])).unwrap();
        assert_eq!(parsed.input, Input::Current);
        assert!(!parsed.poles && parsed.zeros);
        for bad in [
            &["in", "0", "out", "0", "vol"][..],
            &["in", "0", "out", "0", "vol", "vol"],
            &["in", "0", "out", "0", "pol", "zer"],
            &["in", "0", "out", "0", "vol", "poles"],
            &["in", "0", "out", "0", "vol", "pz", "extra"],
        ] {
            assert!(card(&request(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_solution_column_takes_the_drive() {
        // Y = [[1, 2], [3, 4]] + s I; zeros for in = row 0, out = row 1.
        let mut a = Matrix::zeros(2, 2);
        a.data_mut().copy_from_slice(&[1., 2., 3., 4.]);
        let mut e = Matrix::zeros(2, 2);
        e.data_mut().copy_from_slice(&[1., 0., 0., 1.]);
        let (ma, me) = modified_pencil(&a, &e, [Some(0), None], [Some(1), None]).unwrap();
        assert_eq!(ma.data(), &[1., 1., 3., 0.]);
        assert_eq!(me.data(), &[1., 0., 0., 0.]);
        // Differential output: column 0 += column 1, then column 1 replaced.
        let (ma, me) = modified_pencil(&a, &e, [Some(0), None], [Some(1), Some(0)]).unwrap();
        assert_eq!(ma.data(), &[3., 1., 7., 0.]);
        assert_eq!(me.data(), &[1., 0., 1., 0.]);
        // Ground positive output: the negative node is the column and the
        // drive is swapped.
        let (ma, _) = modified_pencil(&a, &e, [Some(0), None], [None, Some(1)]).unwrap();
        assert_eq!(ma.data(), &[1., -1., 3., 0.]);
        let (ma, _) = modified_pencil(&a, &e, [None, Some(0)], [None, Some(1)]).unwrap();
        assert_eq!(ma.data(), &[1., 1., 3., 0.]);
    }
}
