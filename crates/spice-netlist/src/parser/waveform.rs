//! Source syntax from inp2v.c/inp2i.c, INPgetValue in inpgval.c and
//! VSRCparam/ISRCparam. This is not VSRCload/ISRCload time evaluation.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt};

use crate::ast::{
    FunctionWaveform, ParameterAssignment, ParameterKind, PulseWaveform, PwlPoint, SourceFunction,
    SourceWaveform,
};

use super::grammar::{Input, Result, keyword};
use super::syntax::{equals, malformed, named, value};
use super::vector::{numeric, positioned};

/// The vector-valued transient setters of `vsrc.c`/`isrc.c` (`IF_REALVEC`).
pub(super) fn parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let token = alt((
        keyword("pulse"),
        keyword("pwl"),
        keyword("sin"),
        keyword("sine"),
        keyword("exp"),
        keyword("sffm"),
        keyword("am"),
    ))
    .parse_next(input)?;
    let name = token.text.to_ascii_lowercase();
    let function = match name.as_str() {
        "sin" | "sine" => Some(SourceFunction::Sin),
        "exp" => Some(SourceFunction::Exp),
        "sffm" => Some(SourceFunction::Sffm),
        "am" => Some(SourceFunction::Am),
        _ => None,
    };
    cut_err(|input: &mut Input<'_>| {
        opt(equals).parse_next(input)?;
        // C reads at most eight PULSE coefficients, six SIN/EXP and eight
        // SFFM/AM ones; more would be silently ignored there, so they are a
        // syntax error here. PWL is bounded to 2048 pairs. Runtime
        // restrictions (times, slopes, defaults) are not syntax.
        let maximum = match (name.as_str(), function) {
            (_, Some(function)) => function.fields().len(),
            ("pulse", None) => 8,
            _ => 4096,
        };
        let vector = numeric(input, maximum)?;
        if vector.values.len() < 2 {
            return Err(malformed(
                input,
                "waveform requires at least two numeric fields",
            ));
        }
        let waveform = if let Some(function) = function {
            SourceWaveform::Function(Box::new(FunctionWaveform {
                function,
                values: vector
                    .values
                    .iter()
                    .map(|&token| positioned(token))
                    .collect(),
            }))
        } else if name == "pulse" {
            let value = |index: usize| vector.values.get(index).map(|&token| positioned(token));
            SourceWaveform::Pulse(Box::new(PulseWaveform {
                initial: positioned(vector.values[0]),
                pulsed: positioned(vector.values[1]),
                delay: value(2),
                rise: value(3),
                fall: value(4),
                width: value(5),
                period: value(6),
                count: value(7),
            }))
        } else {
            if vector.values.len() % 2 != 0 {
                return Err(malformed(
                    input,
                    "PWL requires strictly paired time/value fields",
                ));
            }
            SourceWaveform::Pwl(
                vector
                    .values
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| PwlPoint {
                        time: positioned(pair[0]),
                        value: positioned(pair[1]),
                    })
                    .collect(),
            )
        };
        Ok(vec![ParameterAssignment {
            name: name.clone(),
            value: vector.text,
            kind: ParameterKind::Waveform(waveform),
            location: token.location.clone(),
        }])
    })
    .parse_next(input)
}

/// PWL repeat/delay scalars (`IP("r", VSRC_R)`, `IP("td", VSRC_TD)` in
/// `vsrc.c`/`isrc.c`). They stay ordered setters: whether they apply depends
/// on the waveform set before them, which device elaboration checks.
pub(super) fn pwl_options(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let token = alt((keyword("r"), keyword("td"))).parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(vec![named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    )])
}
