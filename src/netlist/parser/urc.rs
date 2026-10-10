//! Uniform distributed RC line instance grammar: `src/spicelib/parser/inp2u.c`.
//!
//! ```text
//! Uname n1 n2 n3 model [l=len] [n=lumps]
//! ```
//!
//! `n1`/`n2` are the line's ends and `n3` its capacitive reference (C's
//! `URCnames` "P1", "P2", "Ref"). The model name is required: `INP2U` reports
//! "Unable to find definition of model" for a missing or undeclared one. `l`
//! (`URC_LEN`, `IF_REAL`) and `n` (`URC_LUMPS`, `IF_INTEGER`) are the only
//! instance setters of `urc.c`'s `URCpTable`; they are kept in written order
//! with an optional `=` (C's `INPdevParse` treats `=` as a delimiter), so the
//! last setter wins. `n` stays textual here: its `floor(value + 0.5)` integer
//! rounding (`inpgval.c`) belongs to elaboration (`crate::devices::urc`).
//!
//! Explicit gaps: C silently ignores a leading number after the model
//! (`INPdevParse`'s `waslead`, unused by `INP2U`); the port refuses it. Any
//! other setter is rejected, as C rejects it ("unknown parameter").

use crate::primitives::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, ParameterAssignment};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, location};
use super::syntax::{canonical_node, equals, malformed, name, named, value};

pub(super) fn urc_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&b'u'))
        })
        .parse_next(input)?;
    cut_err(move |input: &mut Input<'_>| body(input, instance)).parse_next(input)
}

fn body(input: &mut Input<'_>, instance: &Token) -> Result<ParsedCard> {
    let auto_gnd = input.state.auto_gnd;
    let first = name("first URC terminal").parse_next(input)?;
    let second = name("second URC terminal").parse_next(input)?;
    let reference = name("URC reference (capacitor) terminal").parse_next(input)?;
    if input.input.is_empty() {
        return Err(malformed(
            input,
            "expected the URC model name (C: unable to find definition of model)",
        ));
    }
    let model = name("URC model name").parse_next(input)?;
    let parameters: Vec<ParameterAssignment> = repeat(0.., setter).parse_next(input)?;
    if !input.input.is_empty() {
        return Err(trailing(input));
    }
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 'u',
        nodes: vec![
            canonical_node(first, auto_gnd),
            canonical_node(second, auto_gnd),
            canonical_node(reference, auto_gnd),
        ],
        model: Some(model.text.to_ascii_lowercase()),
        parameters,
        location: input.state.card.location.clone(),
    }))
}

/// `l` or `n`, an optional `=` and a scalar (literal or brace expression).
fn setter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = any
        .verify(|token: &Token| {
            token.kind == TokenKind::Word && (token.is_keyword("l") || token.is_keyword("n"))
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    ))
}

fn trailing(input: &Input<'_>) -> ErrMode<Failure> {
    let token = &input.input[0];
    let at = location(input);
    let error = if token.number().is_some() {
        SpiceError::Unsupported {
            feature: "a leading value after the URC model (C silently ignores it)".to_owned(),
            location: Some(at),
        }
    } else {
        SpiceError::parse(
            at,
            format!(
                "unknown URC parameter '{}' (urc.c accepts only l= and n=)",
                token.text
            ),
        )
    };
    ErrMode::Cut(Failure(error))
}
