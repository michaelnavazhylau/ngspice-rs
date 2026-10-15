//! Three-terminal Z-instance grammar from `inp2z.c` and `mes/mes.c`.
//!
//! `Zname nd ng ns model [off] [ic=vds,vgs] [area=..] [m=..] [icvds=..]
//! [icvgs=..]`. INP2Z always reads exactly three terminals and then the
//! model name. The `ic` vector fills `icvds` then `icvgs` (`mesparam.c`'s
//! `MES_IC` fallthrough). Model references are retained, not resolved, at
//! this syntax boundary; the `nmf`/`pmf` level selects the C device during
//! elaboration (level 1 `mes/`, the others are not ported).
//!
//! An unlabeled leading value is refused: INP2Z applies it as `area` only for
//! MESA/HFET models and silently discards it for a level-1 MES model, so
//! accepting it would either guess the model or ignore input.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, ParameterAssignment};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Input, ParsedCard, Result, gap};
use super::syntax::{canonical_node, equals, malformed, name, named, value};

pub(super) fn mesfet_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&b'z'))
        })
        .parse_next(input)?;
    let (drain, gate, source, model, parameters) = cut_err((
        name("drain terminal"),
        name("gate terminal"),
        name("source terminal"),
        name("MESFET model name"),
        parameters,
    ))
    .parse_next(input)?;
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 'z',
        nodes: [drain, gate, source]
            .into_iter()
            .map(|node| canonical_node(node, input.state.auto_gnd))
            .collect(),
        model: Some(model.text.to_ascii_lowercase()),
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    if input.input.first().is_some_and(|token| {
        token.number().is_some() || super::expression::is_expression_token(token)
    }) {
        return Err(gap(
            input,
            "unlabeled leading Z value (INP2Z discards it for a level-1 MES model and \
             applies it as area only for MESA/HFET; write area=<value>)",
        ));
    }
    repeat(
        0..,
        alt((
            super::flags::instance,
            super::ic::vector,
            scalar_assignment,
            invalid_parameter,
        )),
    )
    .parse_next(input)
}

/// The scalar `IF_REAL` setters of `MESpTable`.
fn scalar_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = any
        .verify(|token: &Token| {
            matches!(
                token.text.to_ascii_lowercase().as_str(),
                "area" | "m" | "icvds" | "icvgs"
            )
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    ))
}

fn invalid_parameter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = peek(any).parse_next(input)?;
    if matches!(
        token.kind,
        TokenKind::Equals | TokenKind::LParen | TokenKind::RParen | TokenKind::Comma
    ) {
        return Err(malformed(input, "expected a named scalar MESFET parameter"));
    }
    Err(gap(
        input,
        "unsupported MESFET flags, extra terminals or non-scalar parameters",
    ))
}
