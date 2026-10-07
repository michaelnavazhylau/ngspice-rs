//! `.option`/`.options`/`.opt` and `.global` grammars.
//!
//! C: `inp2dot.c` dispatch, `inpdoopt.c::INPdoOpts` (left-to-right setters) and
//! `frontend/subckt.c::collect_global_nodes`. Only syntax is accepted here;
//! names and ranges are validated by the run-configuration consumer.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use super::grammar::{Input, ParsedCard, Result, gap, keyword};
use super::syntax::{canonical_node, equals, malformed, name};
use crate::ast::{GlobalCard, GlobalNode, OptionCard, OptionSetting, PositionedValue};
use crate::token::{Token, TokenKind};

pub(super) fn options_or_global(input: &mut Input<'_>) -> Result<ParsedCard> {
    alt((options, global)).parse_next(input)
}

fn options(input: &mut Input<'_>) -> Result<ParsedCard> {
    alt((keyword(".options"), keyword(".option"), keyword(".opt"))).parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let settings: Vec<OptionSetting> = repeat(1.., setting).parse_next(input)?;
        Ok(ParsedCard::Options(OptionCard {
            settings,
            location: input.state.card.location.clone(),
        }))
    })
    .parse_next(input)
}

fn setting(input: &mut Input<'_>) -> Result<OptionSetting> {
    // Option names are words. A bare number is C's "ignored value" case; here it
    // is a committed error so that no setting is silently dropped.
    if input.input.first().is_some_and(|t| t.number().is_some()) {
        return Err(malformed(
            input,
            "option value without a name; use name=value",
        ));
    }
    let key = any
        .verify(|t: &Token| t.kind == TokenKind::Word)
        .context("an option name")
        .parse_next(input)?;
    let value = opt(equals).parse_next(input)?;
    let value = if value.is_some() {
        Some(cut_err(option_value).parse_next(input)?)
    } else {
        None
    };
    Ok(OptionSetting {
        name: key.text.to_ascii_lowercase(),
        value,
        location: key.location.clone(),
    })
}

fn option_value(input: &mut Input<'_>) -> Result<PositionedValue> {
    if input
        .input
        .first()
        .is_some_and(|t| matches!(t.kind, TokenKind::Expression(_) | TokenKind::Quoted(_)))
    {
        return Err(gap(input, "parameter expressions or quoted option values"));
    }
    let token = any
        .verify(|t: &Token| match t.kind {
            TokenKind::Number(v) => v.is_finite(),
            TokenKind::Word => true,
            _ => false,
        })
        .context("a finite numeric literal or a word")
        .parse_next(input)?;
    // A word directly followed by '=' is the next option name, i.e. the value
    // was missing: `reltol= temp=30`.
    if token.kind == TokenKind::Word
        && input
            .input
            .first()
            .is_some_and(|t| t.kind == TokenKind::Equals)
    {
        return Err(ErrMode::Cut(super::grammar::Failure(
            spice_core::SpiceError::parse(token.location.clone(), "missing option value"),
        )));
    }
    Ok(PositionedValue {
        text: token.text.clone(),
        location: token.location.clone(),
    })
}

fn global(input: &mut Input<'_>) -> Result<ParsedCard> {
    keyword(".global").parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let nodes: Vec<GlobalNode> = repeat(1.., |input: &mut Input<'_>| {
            let token = name("a global node name").parse_next(input)?;
            Ok(GlobalNode {
                name: canonical_node(token, input.state.auto_gnd),
                location: token.location.clone(),
            })
        })
        .parse_next(input)?;
        Ok(ParsedCard::Global(GlobalCard {
            nodes,
            location: input.state.card.location.clone(),
        }))
    })
    .parse_next(input)
}
