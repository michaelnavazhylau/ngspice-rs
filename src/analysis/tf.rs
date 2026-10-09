//! `.tf` — the DC small-signal transfer function.
//!
//! C: `TFanal()` in `src/spicelib/analysis/tfanal.c`, with the card grammar of
//! `dot_tf()` in `src/spicelib/parser/inp2dot.c` and the job parameters of
//! `tfsetp.c`. The card is `.tf outvar insrc`, where `outvar` is `v(n)`,
//! `v(n,m)` or `i(src)`:
//!
//! 1. the DC operating point is solved exactly as for `.op` (`CKTop`), with the
//!    deck's `.nodeset` hints and DC options;
//! 2. the Jacobian at that point — the matrix C's last `CKTop` load left
//!    factored — is reloaded and factored **once**
//!    ([`Circuit::load`] in [`IterationPhase::Float`], C `MODEINITFLOAT`). The
//!    load asks for C's `DEVload` matrix
//!    ([`crate::devices::TrialState::with_c_jacobian`]): where the port's
//!    Newton Jacobian is deliberately exact and C's is not (the BJT's
//!    bias-dependent base resistance, `bjtload.c` stamps `gx` only), `.tf`
//!    reproduces C, which also keeps it consistent with `.ac` at low
//!    frequency. Switches stamp their operating-point state with `swload.c`'s
//!    rules, not `SWacLoad`'s;
//! 3. a unit excitation of the input source is solved: `+1` in the branch row of
//!    a voltage source (`rhs[branch] += 1`), or `-1`/`+1` in the node rows of a
//!    current source's first/second terminal (`rhs[n+] -= 1; rhs[n-] += 1`, a
//!    1 A source). The output in that solution is the transfer function and the
//!    input resistance is `-1/i(insrc)` for a voltage source (`1e20` when
//!    `|i| < 1e-20`) or `v(n-) - v(n+)` for a current source;
//! 4. the output resistance comes from a second solve with the same factors:
//!    a 1 A current drawn out of `n` into `m` for `v(n,m)` (`rhs[n] -= 1;
//!    rhs[m] += 1`, result `v(m) - v(n)`), or a unit voltage in the output
//!    branch for `i(src)` (result `-1/i`, `1e20` when `|i| < 1e-20`, the
//!    reference binary's Enhancement-179 clamp). When the output current is
//!    that of the input source itself, C copies the input resistance.
//!
//! The plot is C's `Transfer Function` plot (`tf1`, `tf2`, ... in a batch): one
//! real point with three `voltage` vectors named as `ngspice -b -r` writes
//! them, `v(Transfer_function)`, `v(<insrc>#Input_impedance)` and
//! `v(output_impedance_at_V(<n>[,<m>]))` or `v(<src>#Output_impedance)`. Names
//! are lowercased (C lowercases the deck) and node names are canonical, so
//! `gnd` reads `0` under the ground alias, as in C.
//!
//! Deliberate divergences, each an explicit error rather than C's behaviour:
//!
//! * C ignores whether `CKTop` converged and solves with whatever matrix is
//!   left; the port propagates the operating-point failure;
//! * C creates an unknown output node on the fly and finds no branch (row 0,
//!   i.e. ground) for an `i(x)` whose device has none; the port rejects an
//!   unknown node, an unknown output device and a device without a findable
//!   branch current (C `CKTfndBranch`: V, E, H and voltage B sources);
//! * C ignores tokens after the input source; the port rejects them.

use crate::analysis::linear::unsupported;
use crate::analysis::results::{PlotFlags, Variable};
use crate::analysis::{AnalysisContext, AnalysisRequest, Plot};
use crate::devices::{AnalysisMode, Circuit, IterationPhase, LoadRequest};
use crate::maths::{SparseMatrix, Vector};
use crate::primitives::{Complex, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// `TFanal()`'s threshold below which a branch current is treated as zero and
/// the resistance reported as [`OPEN_RESISTANCE`].
const ZERO_CURRENT: Real = 1e-20;

/// The resistance `TFanal()` reports when the unit excitation drives no
/// current.
const OPEN_RESISTANCE: Real = 1e20;

/// The `.tf` output variable as written on the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Output {
    /// `v(pos)` or `v(pos,neg)`.
    Voltage {
        /// The positive node as written.
        pos: String,
        /// The negative node as written, `None` for ground.
        neg: Option<String>,
    },
    /// `i(source)`: the branch current of a device with a findable branch.
    Current {
        /// The device name as written.
        source: String,
    },
}

/// A parsed `.tf` card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransferRequest {
    /// What is measured.
    pub output: Output,
    /// The independent V or I source that is excited.
    pub input: String,
}

fn syntax(message: impl std::fmt::Display) -> SpiceError {
    unsupported(format!(
        ".tf {message}; expected `.tf v(n[,m]) insrc` or `.tf i(vsrc) insrc` \
         (C: dot_tf in inp2dot.c)"
    ))
}

/// Parses the positional arguments of a `.tf` card (`dot_tf()`).
///
/// The tokenizer delivers `v(out,in) v1` as `v ( out , in ) v1`; a separating
/// comma is optional, as `INPgetNetTok()` also splits at whitespace.
///
/// # Errors
/// [`SpiceError::Unsupported`] for anything but `v(n)`, `v(n,m)` or `i(src)`
/// followed by exactly one input source name.
pub(crate) fn parse(positional: &[&str]) -> SpiceResult<TransferRequest> {
    let name = |token: Option<&&str>, what: &str| -> SpiceResult<String> {
        match token {
            Some(text) if !matches!(*text, "(" | ")" | ",") => Ok(text.to_ascii_lowercase()),
            Some(text) => Err(syntax(format!("expected {what}, found '{text}'"))),
            None => Err(syntax(format!("is missing {what}"))),
        }
    };
    let expect = |token: Option<&&str>, wanted: &str| -> SpiceResult<()> {
        match token {
            Some(text) if *text == wanted => Ok(()),
            Some(text) => Err(syntax(format!("expected '{wanted}', found '{text}'"))),
            None => Err(syntax(format!("is missing '{wanted}'"))),
        }
    };
    let mut tokens = positional.iter().peekable();
    let kind = tokens
        .next()
        .ok_or_else(|| syntax("has no output variable"))?
        .to_ascii_lowercase();
    let output = match kind.as_str() {
        "v" => {
            expect(tokens.next(), "(")?;
            let pos = name(tokens.next(), "an output node")?;
            if tokens.peek().is_some_and(|token| **token == ",") {
                tokens.next();
            }
            let neg = if tokens.peek().is_some_and(|token| **token == ")") {
                None
            } else {
                Some(name(tokens.next(), "the negative output node")?)
            };
            expect(tokens.next(), ")")?;
            Output::Voltage { pos, neg }
        }
        "i" => {
            expect(tokens.next(), "(")?;
            let source = name(tokens.next(), "an output source")?;
            expect(tokens.next(), ")")?;
            Output::Current { source }
        }
        other => {
            return Err(syntax(format!(
                "output '{other}' is neither a voltage v(...) nor a current i(...)"
            )));
        }
    };
    let input = name(tokens.next(), "the input source")?;
    if let Some(extra) = tokens.next() {
        return Err(syntax(format!(
            "has an unexpected argument '{extra}' after the input source"
        )));
    }
    Ok(TransferRequest { output, input })
}

/// The rawfile names of the three result vectors (`TFanal()`'s `IFnewUid`
/// calls, wrapped in `v(...)` by `raw_write()` because they are `voltage`
/// vectors). `pos`/`neg` must already be canonical node names.
fn names(request: &TransferRequest, pos: &str, neg: Option<&str>) -> [String; 3] {
    let output = match (&request.output, neg) {
        (Output::Current { source }, _) => format!("v({source}#Output_impedance)"),
        (Output::Voltage { .. }, Some(neg)) => format!("v(output_impedance_at_V({pos},{neg}))"),
        (Output::Voltage { .. }, None) => format!("v(output_impedance_at_V({pos}))"),
    };
    [
        "v(Transfer_function)".to_owned(),
        format!("v({}#Input_impedance)", request.input),
        output,
    ]
}

/// How the input source is excited and read back.
enum Input {
    /// An independent voltage source: its branch row.
    Voltage { branch: usize },
    /// An independent current source: the rows of its first and second
    /// terminals (`None` for ground).
    Current {
        plus: Option<usize>,
        minus: Option<usize>,
    },
}

/// Where the output is read.
enum Probe {
    /// `v(pos) - v(neg)`; `None` is ground.
    Voltage {
        pos: Option<usize>,
        neg: Option<usize>,
    },
    /// The branch current in `row` of device `device`.
    Current { device: usize, row: usize },
}

fn not_found(message: String) -> SpiceError {
    SpiceError::Circuit {
        message: format!("{message} (C: TFanal in tfanal.c)"),
    }
}

fn node_row(circuit: &Circuit, name: &str) -> SpiceResult<(String, Option<usize>)> {
    let canonical = NodeTable::canonical_name(name, circuit.nodes().auto_gnd());
    let id = circuit
        .nodes()
        .get(&canonical)
        .ok_or_else(|| not_found(format!(".tf output node {name} is not in the circuit")))?;
    let row = circuit.unknowns().node_row(id);
    let is_ground = circuit
        .nodes()
        .node(id)
        .is_some_and(|node| node.kind == NodeKind::Ground);
    if row.is_none() && !is_ground {
        return Err(not_found(format!(
            ".tf output node {name} has no matrix row"
        )));
    }
    Ok((canonical, row))
}

fn device_index(circuit: &Circuit, name: &str) -> Option<usize> {
    circuit
        .devices()
        .iter()
        .position(|device| device.name().eq_ignore_ascii_case(name))
}

fn resolve_input(circuit: &Circuit, name: &str) -> SpiceResult<Input> {
    let index = device_index(circuit, name)
        .ok_or_else(|| not_found(format!("Transfer function source {name} not in circuit")))?;
    let device = &circuit.devices()[index];
    match device.designator() {
        'v' => {
            let branch = circuit
                .branch_rows(index)
                .and_then(|rows| rows.clone().next())
                .ok_or_else(|| not_found(format!("{name}: missing branch row")))?;
            Ok(Input::Voltage { branch })
        }
        'i' => {
            let row = |terminal: usize| {
                device
                    .terminals()
                    .get(terminal)
                    .copied()
                    .ok_or_else(|| not_found(format!("{name}: missing terminal")))
                    .map(|node| circuit.unknowns().node_row(node))
            };
            Ok(Input::Current {
                plus: row(0)?,
                minus: row(1)?,
            })
        }
        _ => Err(not_found(format!(
            "Transfer function source {name} not of proper type (an independent V or I \
             source is required)"
        ))),
    }
}

fn resolve_output(circuit: &Circuit, output: &Output) -> SpiceResult<(Probe, [Option<String>; 2])> {
    match output {
        Output::Voltage { pos, neg } => {
            let (pos_name, pos) = node_row(circuit, pos)?;
            let (neg_name, neg) = match neg {
                Some(neg) => {
                    let (name, row) = node_row(circuit, neg)?;
                    (Some(name), row)
                }
                None => (None, None),
            };
            Ok((Probe::Voltage { pos, neg }, [Some(pos_name), neg_name]))
        }
        Output::Current { source } => {
            let device = device_index(circuit, source).ok_or_else(|| {
                not_found(format!(".tf output source {source} is not in the circuit"))
            })?;
            let branch = circuit.devices()[device].findable_branch().ok_or_else(|| {
                unsupported(format!(
                    ".tf i({source}): the device has no findable branch current (only \
                         independent voltage sources, E/H sources and voltage B sources can be \
                         sensed, as by CKTfndBranch; C would silently read ground)"
                ))
            })?;
            let row = circuit
                .branch_rows(device)
                .and_then(|mut rows| rows.nth(branch))
                .ok_or_else(|| not_found(format!("{source}: missing branch row")))?;
            Ok((Probe::Current { device, row }, [None, None]))
        }
    }
}

fn add(rhs: &mut Vector, row: Option<usize>, value: Real) -> SpiceResult<()> {
    match row {
        Some(row) => rhs.add_to(row, value),
        None => Ok(()),
    }
}

fn at(x: &Vector, row: Option<usize>) -> Real {
    row.and_then(|row| x.get(row)).unwrap_or(0.)
}

/// `TFanal()`'s resistance seen by a unit voltage that drives `current`.
fn resistance(current: Real) -> Real {
    if current.abs() < ZERO_CURRENT {
        OPEN_RESISTANCE
    } else {
        -1. / current
    }
}

/// Runs `.tf` on `circuit`; see the module documentation.
///
/// # Errors
/// Malformed cards, an unknown or wrong-type input source, an unknown output
/// node or device, an output device without a findable branch, operating-point
/// failures, and singular or non-finite small-signal solves.
pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Plot> {
    let settings = crate::analysis::bias::DcSettings::from_request(request)?;
    let positional: Vec<&str> = request
        .arguments
        .iter()
        .filter(|argument| !argument.contains('='))
        .map(String::as_str)
        .collect();
    let card = parse(&positional)?;

    circuit.finalize()?;
    let input = resolve_input(circuit, &card.input)?;
    let (probe, [pos_name, neg_name]) = resolve_output(circuit, &card.output)?;

    // The operating point, exactly as `.op` solves it (`CKTop` in MODEDCOP;
    // `.ic` applies only to a transient operating point, `.nodeset` here too).
    let hints = crate::analysis::initial::resolve(circuit, request)?;
    let nodes = crate::analysis::bias::NodeForcing {
        initial: Vec::new(),
        nodesets: crate::analysis::initial::forced_nodesets(circuit, &hints.nodesets, &[]),
    };
    let n = circuit.unknown_count();
    let mut seed = Vector::zeros(n);
    for hint in hints.nodesets {
        seed.as_mut_slice()[hint.row] = hint.value;
    }
    let model = context.model_context();
    let solved = crate::analysis::bias::solve_dc_forced(
        circuit,
        &model,
        &settings,
        &[],
        Some(&seed),
        None,
        &nodes,
    )?
    .solution;

    // The Newton Jacobian at the operating point: one MODEINITFLOAT load whose
    // previous iterate is the converged one, so no device limits its voltages.
    let history = circuit.state_history();
    let mut trial = history
        .trial_in(IterationPhase::Float, Some(&solved.trial))?
        .with_device_limiting(settings.newton.limiting.is_device())
        .with_c_jacobian(true);
    let mut jacobian = SparseMatrix::new(n, n);
    circuit.load(
        &LoadRequest {
            mode: AnalysisMode::OperatingPoint,
            solution: &solved.values,
            model_context: &model,
            integration: None,
            history: &history,
            forcing: None,
        },
        &mut jacobian,
        &mut Vector::zeros(n),
        &mut trial,
    )?;
    jacobian.fold_duplicates();
    let factors = jacobian.factorize()?;

    // Unit excitation of the input source.
    let mut rhs = Vector::zeros(n);
    match input {
        Input::Voltage { branch } => rhs.add_to(branch, 1.)?,
        Input::Current { plus, minus } => {
            add(&mut rhs, plus, -1.)?;
            add(&mut rhs, minus, 1.)?;
        }
    }
    let x = factors.solve(&rhs)?;
    let read = |x: &Vector| match probe {
        Probe::Voltage { pos, neg } => at(x, pos) - at(x, neg),
        Probe::Current { row, .. } => at(x, Some(row)),
    };
    let transfer = read(&x);
    let input_resistance = match input {
        Input::Voltage { branch } => resistance(at(&x, Some(branch))),
        Input::Current { plus, minus } => at(&x, minus) - at(&x, plus),
    };

    let same_source = match probe {
        Probe::Current { device, .. } => device_index(circuit, &card.input) == Some(device),
        Probe::Voltage { .. } => false,
    };
    let output_resistance = if same_source {
        input_resistance
    } else {
        let mut rhs = Vector::zeros(n);
        match probe {
            Probe::Voltage { pos, neg } => {
                add(&mut rhs, pos, -1.)?;
                add(&mut rhs, neg, 1.)?;
            }
            Probe::Current { row, .. } => rhs.add_to(row, 1.)?,
        }
        let y = factors.solve(&rhs)?;
        match probe {
            Probe::Voltage { pos, neg } => at(&y, neg) - at(&y, pos),
            Probe::Current { row, .. } => resistance(at(&y, Some(row))),
        }
    };

    let values = [transfer, input_resistance, output_resistance];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(SpiceError::Numerical {
            context: ".tf".to_owned(),
            message: format!("non-finite transfer function result {values:?}"),
        });
    }
    let names = names(
        &card,
        pos_name.as_deref().unwrap_or_default(),
        neg_name.as_deref(),
    );
    let mut plot = Plot::new("tf1", "Transfer Function", PlotFlags::Real);
    for name in names {
        plot.push_variable(Variable::new(name, "voltage"));
    }
    plot.push_point(values.into_iter().map(Complex::real).collect())?;
    Ok(plot)
}

#[cfg(test)]
mod tests {
    use super::{Output, TransferRequest, names, parse, resistance};

    fn tokens(text: &str) -> Vec<&str> {
        text.split_whitespace().collect()
    }

    #[test]
    fn voltage_current_and_differential_outputs_parse() {
        assert_eq!(
            parse(&tokens("v ( out ) v1")).unwrap(),
            TransferRequest {
                output: Output::Voltage {
                    pos: "out".into(),
                    neg: None
                },
                input: "v1".into()
            }
        );
        for text in ["V ( A , B ) I1", "v ( a b ) i1"] {
            assert_eq!(
                parse(&tokens(text)).unwrap(),
                TransferRequest {
                    output: Output::Voltage {
                        pos: "a".into(),
                        neg: Some("b".into())
                    },
                    input: "i1".into()
                }
            );
        }
        assert_eq!(
            parse(&tokens("I ( Vm ) v1")).unwrap().output,
            Output::Current {
                source: "vm".into()
            }
        );
    }

    #[test]
    fn malformed_cards_are_rejected() {
        for (text, message) in [
            ("", "no output variable"),
            ("out v1", "neither a voltage"),
            ("v out v1", "expected '('"),
            ("v ( out v1", "missing ')'"),
            ("v ( out )", "missing the input source"),
            ("v ( out ) v1 v2", "unexpected argument 'v2'"),
            ("i ( ) v1", "expected an output source"),
            ("i ( a , b ) v1", "expected ')'"),
            ("v ( ) v1", "expected an output node"),
        ] {
            let error = parse(&tokens(text)).expect_err(text);
            assert!(error.to_string().contains(message), "{text}: {error}");
        }
    }

    #[test]
    fn vector_names_follow_c_batch_rawfiles() {
        let request = parse(&tokens("v ( out , in ) V1")).unwrap();
        assert_eq!(
            names(&request, "out", Some("in")),
            [
                "v(Transfer_function)",
                "v(v1#Input_impedance)",
                "v(output_impedance_at_V(out,in))"
            ]
        );
        let request = parse(&tokens("v ( out ) v1")).unwrap();
        assert_eq!(
            names(&request, "out", None)[2],
            "v(output_impedance_at_V(out))"
        );
        let request = parse(&tokens("i ( VM ) i1")).unwrap();
        assert_eq!(
            names(&request, "", None),
            [
                "v(Transfer_function)",
                "v(i1#Input_impedance)",
                "v(vm#Output_impedance)"
            ]
        );
    }

    #[test]
    fn a_vanishing_current_reads_as_c_open_resistance() {
        assert_eq!(resistance(-1e-3), 1e3);
        assert_eq!(resistance(0.), 1e20);
        assert_eq!(resistance(9e-21), 1e20);
        assert_eq!(resistance(-1e-20), 1e20);
    }
}
