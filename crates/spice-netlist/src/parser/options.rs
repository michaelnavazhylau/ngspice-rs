//! `.option`/`.options`/`.opt` and `.global` grammars.
//!
//! C: `inp2dot.c` dispatch, `inpdoopt.c::INPdoOpts` (left-to-right setters) and
//! `frontend/subckt.c::collect_global_nodes`. Only syntax is accepted here;
//! names and ranges are validated by the run-configuration consumer. `{expr}`
//! and `'expr'` values are parsed (numparam grammar) but not evaluated.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use super::grammar::{Failure, Input, ParsedCard, Result, keyword};
use super::syntax::{canonical_node, equals, malformed, name};
use crate::ast::{GlobalCard, GlobalNode, OptionCard, OptionSetting, PositionedValue};
use crate::expr::ParameterExpression;
use crate::token::{Token, TokenKind};
use spice_core::{SpiceError, SpiceResult};

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
    let (value, expression) = if value.is_some() {
        let (value, expression) = cut_err(option_value).parse_next(input)?;
        (Some(value), expression)
    } else {
        (None, None)
    };
    Ok(OptionSetting {
        name: key.text.to_ascii_lowercase(),
        value,
        expression,
        location: key.location.clone(),
    })
}

type OptionValue = (PositionedValue, Option<Box<ParameterExpression>>);

fn option_value(input: &mut Input<'_>) -> Result<OptionValue> {
    if let Some(token) = input.input.first() {
        // `{expr}` and single-quoted `'expr'` are numparam expressions in C;
        // they are parsed here and evaluated by the run configuration.
        let expression = match &token.kind {
            TokenKind::Expression(_) => Some(super::expression::from_brace_token(token)),
            TokenKind::Quoted(inner) => Some(quoted_expression(token, inner)),
            _ => None,
        };
        if let Some(expression) = expression {
            let expression = expression.map_err(|error| ErrMode::Cut(Failure(error)))?;
            let token = any.parse_next(input)?;
            return Ok((
                PositionedValue {
                    text: token.text.clone(),
                    location: token.location.clone(),
                },
                Some(Box::new(expression)),
            ));
        }
    }
    let token = any
        .verify(|t: &Token| match t.kind {
            TokenKind::Number(v) => v.is_finite(),
            TokenKind::Word => true,
            _ => false,
        })
        .context("a finite numeric literal, a word, {expression} or 'expression'")
        .parse_next(input)?;
    // A word directly followed by '=' is the next option name, i.e. the value
    // was missing: `reltol= temp=30`.
    if token.kind == TokenKind::Word
        && input
            .input
            .first()
            .is_some_and(|t| t.kind == TokenKind::Equals)
    {
        return Err(ErrMode::Cut(Failure(SpiceError::parse(
            token.location.clone(),
            "missing option value",
        ))));
    }
    Ok((
        PositionedValue {
            text: token.text.clone(),
            location: token.location.clone(),
        },
        None,
    ))
}

/// A single-quoted option value as an (unbraced) expression. Double-quoted
/// strings and quotes holding escapes are rejected: their byte columns would
/// not map onto the card text.
fn quoted_expression(token: &Token, inner: &str) -> SpiceResult<ParameterExpression> {
    if token.text != format!("'{inner}'") {
        return Err(SpiceError::parse(
            token.location.clone(),
            "only single-quoted 'expression' option values (without escapes) are accepted",
        ));
    }
    super::expression::parse_expression(inner, &token.location, token.location.column + 1, false)
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
