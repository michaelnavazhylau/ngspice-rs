//! Bounded numeric vectors for INPgetValue(IF_REALVEC), in `inpgval.c`.
//! C gobbles punctuation permissively; this grammar deliberately requires a
//! balanced optional outer pair and a value after every comma. Bare vectors
//! end at the next keyword, not by swallowing an overflowing numeric value.

use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::PositionedValue;
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, Result};
use super::syntax::{literal, malformed};

pub(super) struct NumericVector<'a> {
    pub values: Vec<&'a Token>,
    pub text: String,
}

pub(super) fn punctuation<'a>(
    kind: TokenKind,
) -> impl winnow::Parser<Input<'a>, &'a Token, ErrMode<Failure>> {
    any.verify(move |token: &Token| token.kind == kind)
}

pub(super) fn positioned(token: &Token) -> PositionedValue {
    PositionedValue {
        text: token.text.clone(),
        location: token.location.clone(),
    }
}

pub(super) fn numeric<'a>(input: &mut Input<'a>, maximum: usize) -> Result<NumericVector<'a>> {
    let start = super::grammar::location(input).column as usize - 1;
    let parenthesized = opt(punctuation(TokenKind::LParen))
        .parse_next(input)?
        .is_some();
    let first = cut_err(literal).parse_next(input)?;
    let mut count = 1;
    let following: Vec<&Token> = repeat(0.., |input: &mut Input<'a>| {
        // A comma claims another value. So do numeric and expression tokens;
        // their errors must survive repeat's optional stopping rule.
        let comma = opt(punctuation(TokenKind::Comma)).parse_next(input)?;
        if comma.is_none() {
            peek(any.verify(|token: &Token| {
                token.number().is_some()
                    || matches!(token.kind, TokenKind::Expression(_) | TokenKind::Quoted(_))
            }))
            .parse_next(input)?;
        }
        if count == maximum {
            return Err(malformed(input, "too many numeric vector fields"));
        }
        let value = cut_err(literal).parse_next(input)?;
        count += 1;
        Ok(value)
    })
    .parse_next(input)?;
    let mut values = vec![first];
    values.extend(following);
    let last = if parenthesized {
        cut_err(punctuation(TokenKind::RParen).context("')' after numeric vector"))
            .parse_next(input)?
    } else {
        values[values.len() - 1]
    };
    let end = last.location.column as usize - 1 + last.text.len();
    // Token columns are byte offsets in this exact joined raw card.
    let text = input
        .state
        .card
        .raw
        .get(start..end)
        .ok_or_else(|| malformed(input, "numeric vector has invalid source span"))?
        .to_owned();
    Ok(NumericVector { values, text })
}
