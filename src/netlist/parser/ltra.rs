//! Lossy transmission-line grammar: `src/spicelib/parser/inp2o.c` and the
//! `LTRApTable` of `src/spicelib/devices/ltra/ltra.c`.
//!
//! ```text
//! Oname p1 n1 p2 n2 model [v1|i1|v2|i2 [=] v] [ic [=] v1[,i1[,v2[,i2]]]]
//! ```
//!
//! `INP2O` reads four nodes and a model name, then hands the rest to
//! `INPdevParse`: setters are kept in written order (last wins), the `=` is
//! optional as in C's tokenizer, and `ic=` is an `IF_REALVEC` of one to four
//! values stored as [`ParameterKind::InitialConditions`] components `v1`,
//! `i1`, `v2`, `i2` (`LTRAparam`'s fallthrough order). Values are finite
//! literals or `{...}` expressions (evaluated by elaboration); the `ic=`
//! vector takes literals.
//!
//! The model name is required: a missing one makes `INPgetMod` fail ("unable
//! to find definition of model"), after which C's default `O` model has no
//! length and setup fails. Explicit gaps: a leading positional value (C's
//! `waslead`, silently ignored by `INP2O`) is refused; any other word is a
//! parse error ("unknown parameter" in C). Model lookup and validation belong
//! to the device (`crate::devices::ltra`).

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

/// Scalar setters of `LTRApTable`.
const SCALARS: [&str; 4] = ["v1", "v2", "i1", "i2"];

/// `ic=` components in `LTRAparam`'s fallthrough order.
const IC_COMPONENTS: [&str; 4] = ["v1", "i1", "v2", "i2"];

pub(super) fn ltra_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .chars()
                    .next()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'o'))
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
    if input.input.is_empty() {
        return Err(malformed(
            input,
            "expected the lossy-line model name (C: unable to find definition of model)",
        ));
    }
    let model = name("lossy-line model name").parse_next(input)?;
    if input
        .input
        .first()
        .is_some_and(|token| token.number().is_some())
    {
        return Err(ErrMode::Cut(Failure(SpiceError::Unsupported {
            feature: "a leading value after the lossy-line model (C silently ignores it)".into(),
            location: Some(location(input)),
        })));
    }
    let parameters: Vec<ParameterAssignment> =
        repeat(0.., alt((initial_conditions, scalar, invalid))).parse_next(input)?;
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 'o',
        nodes,
        model: Some(model.text.to_ascii_lowercase()),
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
            "unknown lossy-line parameter '{}' (expected v1, i1, v2, i2 or ic)",
            token.text
        ),
    ))
}
