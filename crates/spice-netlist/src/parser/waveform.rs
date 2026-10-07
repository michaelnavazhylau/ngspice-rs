//! Source syntax from inp2v.c/inp2i.c, INPgetValue in inpgval.c and
//! VSRCparam/ISRCparam. This is not VSRCload/ISRCload time evaluation.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt};

use crate::ast::{ParameterAssignment, ParameterKind, PulseWaveform, PwlPoint, SourceWaveform};

use super::grammar::{Input, Result, keyword};
use super::syntax::{equals, malformed};
use super::vector::{numeric, positioned};

pub(super) fn parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let token = alt((keyword("pulse"), keyword("pwl"))).parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        opt(equals).parse_next(input)?;
        // The seven-field PULSE subset excludes NCYCLES; PWL is bounded to
        // 2048 pairs. Runtime restrictions (times, slopes) are not syntax.
        let pulse = token.is_keyword("pulse");
        let vector = numeric(input, if pulse { 7 } else { 4096 })?;
        if vector.values.len() < 2 {
            return Err(malformed(
                input,
                "waveform requires at least two numeric fields",
            ));
        }
        let waveform = if pulse {
            let value = |index: usize| vector.values.get(index).map(|&token| positioned(token));
            SourceWaveform::Pulse(Box::new(PulseWaveform {
                initial: positioned(vector.values[0]),
                pulsed: positioned(vector.values[1]),
                delay: value(2),
                rise: value(3),
                fall: value(4),
                width: value(5),
                period: value(6),
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
            name: token.text.to_ascii_lowercase(),
            value: vector.text,
            kind: ParameterKind::Waveform(waveform),
            location: token.location.clone(),
        }])
    })
    .parse_next(input)
}
