//! Shared positioned-token primitives for scalar device and model grammars.

use crate::primitives::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, peek};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::primitives::SourceLoc;

use crate::netlist::ast::{ParameterAssignment, ParameterKind};
use crate::netlist::expr::ParameterExpression;
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, Result, gap};

pub(super) fn name<'a>(
    expected: &'static str,
) -> impl winnow::Parser<Input<'a>, &'a Token, ErrMode<Failure>> {
    any.verify(|token: &Token| token.is_name_like())
        .context(expected)
}

pub(super) fn canonical_node(token: &Token, auto_gnd: bool) -> String {
    // Node-only aliasing is the port's AST contract. inpcom.c's
    // inp_fix_gnd_name() replaces more broadly in card text; deliberately keep
    // model/value/instance spelling unchanged here (see ARCHITECTURE.md).
    let name = token.text.to_ascii_lowercase();
    if auto_gnd && name == "gnd" {
        "0".to_owned()
    } else {
        name
    }
}

pub(super) fn equals<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|token: &Token| token.kind == TokenKind::Equals)
        .parse_next(input)
}

/// A scalar value site: a finite literal or a parsed (unevaluated) `{...}`.
pub(super) enum Value<'a> {
    Literal(&'a Token),
    Expression(&'a Token, Box<ParameterExpression>),
}

impl Value<'_> {
    pub(super) fn token(&self) -> &Token {
        match self {
            Self::Literal(token) | Self::Expression(token, _) => token,
        }
    }
}

/// Only a numeric prefix or a brace expression claims an optional positional
/// slot. Once claimed, overflow or a malformed expression is a committed
/// syntax error, not a missing optional value. Bare names are never claimed:
/// they may be model names (C does not substitute unbraced names in device
/// cards).
pub(super) fn leading_value<'a>(input: &mut Input<'a>) -> Result<Value<'a>> {
    peek(any.verify(|token: &Token| {
        token.number().is_some() || super::expression::is_expression_token(token)
    }))
    .parse_next(input)?;
    cut_err(value).parse_next(input)
}

/// A finite numeric literal or a brace expression.
pub(super) fn value<'a>(input: &mut Input<'a>) -> Result<Value<'a>> {
    if let Some(token) =
        opt(any.verify(super::expression::is_expression_token)).parse_next(input)?
    {
        let expression = super::expression::from_brace_token(token)
            .map_err(|error| ErrMode::Cut(Failure(error)))?;
        return Ok(Value::Expression(token, Box::new(expression)));
    }
    literal.map(Value::Literal).parse_next(input)
}

/// A strictly numeric literal. Expressions are a gap at sites that have not
/// adopted [`value`] (waveform and IC-vector components).
pub(super) fn literal<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    if input.input.first().is_some_and(|token| {
        matches!(
            token.kind,
            TokenKind::Word | TokenKind::Expression(_) | TokenKind::Quoted(_)
        )
    }) {
        return Err(gap(
            input,
            "parameter expressions, model references or extended numeric syntax",
        ));
    }
    any.verify(|token: &Token| token.number().is_some_and(f64::is_finite))
        .context("a finite numeric literal")
        .parse_next(input)
}

/// Positional/leading assignment located at its value token.
pub(super) fn assignment(name: &str, value: Value<'_>) -> ParameterAssignment {
    let location = value.token().location.clone();
    named(name, location, value)
}

/// Named assignment located at its setter keyword.
pub(super) fn named(name: &str, location: SourceLoc, value: Value<'_>) -> ParameterAssignment {
    let text = value.token().text.clone();
    ParameterAssignment {
        name: name.to_owned(),
        value: text,
        kind: match value {
            Value::Literal(_) => ParameterKind::Scalar,
            Value::Expression(_, expression) => ParameterKind::Expression(expression),
        },
        location,
    }
}

pub(super) fn malformed(input: &Input<'_>, message: &str) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(
        super::grammar::location(input),
        message,
    )))
}
