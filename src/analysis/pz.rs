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
//! (`MODEINITSMSIG`); every C pole-zero load is affine in `s`, so each
//! device contributes `g + c s` per matrix element
//! ([`Circuit::pole_zero_terms_at`]). An independent voltage source with an
//! AC value is removed (its current forced to zero, `vsrcpzld.c`): the
//! analysis drives the input with a unit current instead. As `CKTpzSetup`
//! does, each search (poles, then zeros) gets a fresh matrix holding the
//! `DEVpzSetup` elements ([`Circuit::pole_zero_elements`]) and the drive
//! elements, and every trial `s` is loaded as `CKTpzLoad` does, device by
//! device in load order, before the matrix `Y(s)` is modified:
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
//! `det Y(s)`. The roots are found with C's deflated Muller search on
//! SPARSE determinants ([`crate::maths::pencil`]).
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
use crate::maths::pencil::{PencilLoad, PencilTerm, pencil_load_roots};
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

/// The matrix whose determinant's roots are the requested poles or zeros:
/// `CKTpzSetup` (`cktpzset.c`) picks the solution and balance columns and
/// allocates the drive elements, which `CKTpzLoad` (`cktpzld.c`) fills.
/// `output` is `None` for `cur` poles (C clears every node number, so the
/// matrix is loaded unmodified).
fn modified_load(
    order: usize,
    elements: &[(usize, usize)],
    terms: &[PencilTerm],
    input: [Option<usize>; 2],
    output: Option<[Option<usize>; 2]>,
) -> PencilLoad {
    let mut load = PencilLoad {
        order,
        elements: elements.to_vec(),
        terms: terms.to_vec(),
        ..PencilLoad::default()
    };
    let Some(output) = output else {
        return load;
    };
    let [mut input_pos, mut input_neg] = input;
    let (solution, balance) = match output {
        [Some(pos), neg] => (Some(pos), neg),
        [None, neg] => {
            // A ground positive output takes the negative node as the
            // solution column and swaps the drive.
            std::mem::swap(&mut input_pos, &mut input_neg);
            (neg, None)
        }
    };
    if solution.is_some() {
        load.solution = solution;
        load.balance = balance;
        load.drive = [input_pos, input_neg];
    }
    load
}

/// The finite roots of a modified pencil. A pencil whose determinant
/// vanishes for every `s` has no defined roots; C reports that case (its
/// search finds a "root" at every trial) as `E_SHORT`.
fn roots(load: &PencilLoad, what: &str) -> SpiceResult<Vec<Complex>> {
    pencil_load_roots(load)
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
    let terms = circuit.pole_zero_terms_at(
        &context.model_context(),
        small_signal.bias(),
        Some(small_signal.state()),
    )?;
    let elements = circuit.pole_zero_elements()?;
    let order = circuit.unknown_count();
    let input = [in_pos, in_neg];
    // `PZan` runs the pole search before the zero search, each on a fresh
    // `CKTpzSetup` matrix; `PZpost` lists poles first.
    let mut poles = Vec::new();
    if card.poles {
        let output = match card.input {
            Input::Voltage => Some(input),
            Input::Current => None,
        };
        let load = modified_load(order, &elements, &terms, input, output);
        poles = roots(&load, "poles")?;
    }
    let mut zeros = Vec::new();
    if card.zeros {
        let load = modified_load(order, &elements, &terms, input, Some([out_pos, out_neg]));
        zeros = roots(&load, "zeros")?;
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
        let terms = [PencilTerm {
            row: 0,
            col: 0,
            conductance: Some(1.),
            capacitance: Some(1.),
        }];
        let load = modified_load(2, &[(0, 0)], &terms, [Some(0), None], Some([Some(1), None]));
        assert_eq!((load.solution, load.balance), (Some(1), None));
        assert_eq!(load.drive, [Some(0), None]);
        // Differential output: the negative node is the balance column.
        let load = modified_load(2, &[], &terms, [Some(0), None], Some([Some(1), Some(0)]));
        assert_eq!((load.solution, load.balance), (Some(1), Some(0)));
        // Ground positive output: the negative node is the column and the
        // drive is swapped.
        let load = modified_load(2, &[], &terms, [Some(0), None], Some([None, Some(1)]));
        assert_eq!((load.solution, load.balance), (Some(1), None));
        assert_eq!(load.drive, [None, Some(0)]);
        // `cur` poles load the matrix unmodified.
        let load = modified_load(2, &[], &terms, [Some(0), None], None);
        assert_eq!((load.solution, load.drive), (None, [None, None]));
    }
}
