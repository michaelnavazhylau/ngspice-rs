//! Lossless transmission-line grammar: `src/spicelib/parser/inp2t.c` and
//! the `TRApTable` of `src/spicelib/devices/tra/tra.c`.
//!
//! ```text
//! Tname p1 n1 p2 n2 [z0|zo [=] v] [td [=] v] [f [=] v] [nl [=] v]
//!       [v1|i1|v2|i2 [=] v] [rel [=] v] [abs [=] v] [ic [=] v1[,i1[,v2[,i2]]]]
//! ```
//!
//! `INP2T` reads four nodes and hands the rest to `INPdevParse`: setters are
//! kept in written order (the device applies them in turn, last wins), the
//! `=` is optional as in C's tokenizer, and `ic=` is an `IF_REALVEC` of one
//! to four values stored as [`ParameterKind::InitialConditions`] components
//! `v1`, `i1`, `v2`, `i2`. Values are finite literals or `{...}`
//! expressions (evaluated by elaboration); the `ic=` vector takes literals.
//!
//! Explicit gaps: a leading positional value (C's `waslead`, silently
//! ignored by `INP2T`) is refused; any other word is a parse error ("unknown
//! parameter" in C). Requiring `z0` and validating values belongs to the
//! device (`crate::devices::tline`).

use crate::primitives::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, InitialCondition, ParameterAssignment, ParameterKind};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, keyword, location};
use super::syntax::{canonical_node, equals, malformed, name, named, value};
use super::vector::{numeric, positioned};

/// Scalar setters of `TRApTable` (`zo` is C's redundant alias of `z0`).
const SCALARS: [&str; 11] = [
    "z0", "zo", "td", "f", "nl", "v1", "v2", "i1", "i2", "rel", "abs",
];

/// `ic=` components in `TRAparam`'s fallthrough order.
const IC_COMPONENTS: [&str; 4] = ["v1", "i1", "v2", "i2"];

pub(super) fn tline_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .chars()
                    .next()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'t'))
        })
        .parse_next(input)?;
    cut_err(move |input: &mut Input<'_>| body(input, instance)).parse_next(input)
}

fn body(input: &mut Input<'_>, instance: &Token) -> Result<ParsedCard> {
    let auto_gnd = input.state.auto_gnd;
    let mut nodes = Vec::with_capacity(4);
    for what in [
        "port 1 positive node",
        "port 1 negative node",
        "port 2 positive node",
        "port 2 negative node",
    ] {
        let node = name(what).parse_next(input)?;
        nodes.push(canonical_node(node, auto_gnd));
    }
    if input
        .input
        .first()
        .is_some_and(|token| token.number().is_some())
    {
        return Err(ErrMode::Cut(Failure(SpiceError::Unsupported {
            feature: "a leading value after the transmission-line nodes (C silently ignores it)"
                .into(),
            location: Some(location(input)),
        })));
    }
    let parameters: Vec<ParameterAssignment> =
        repeat(0.., alt((initial_conditions, scalar, invalid))).parse_next(input)?;
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 't',
        nodes,
        model: None,
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn scalar(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = any
        .verify(|token: &Token| {
            token.kind == TokenKind::Word && SCALARS.iter().any(|keyword| token.is_keyword(keyword))
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    ))
}

fn initial_conditions(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = keyword("ic").parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        opt(equals).parse_next(input)?;
        let vector = numeric(input, IC_COMPONENTS.len())?;
        let values = vector
            .values
            .iter()
            .zip(IC_COMPONENTS)
            .map(|(value, name)| InitialCondition {
                name: name.to_owned(),
                value: positioned(value),
            })
            .collect();
        Ok(ParameterAssignment {
            name: "ic".to_owned(),
            value: vector.text,
            kind: ParameterKind::InitialConditions(values),
            location: token.location.clone(),
        })
    })
    .parse_next(input)
}

fn invalid(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = peek(any).parse_next(input)?;
    Err(malformed(
        input,
        &format!(
            "unknown transmission-line parameter '{}' (expected z0, zo, td, f, nl, v1, i1, \
             v2, i2, rel, abs or ic)",
            token.text
        ),
    ))
}
