//! Shared positioned-token primitives for scalar device and model grammars.

use spice_core::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{cut_err, peek};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::ast::ParameterAssignment;
use crate::token::{Token, TokenKind};

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

/// Only a numeric prefix claims an optional positional slot. Once claimed,
/// numeric overflow is a committed syntax error, not a missing optional value.
pub(super) fn leading_literal<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    peek(any.verify(|token: &Token| token.number().is_some())).parse_next(input)?;
    cut_err(literal).parse_next(input)
}

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

pub(super) fn assignment(name: &str, value: &Token) -> ParameterAssignment {
    ParameterAssignment {
        name: name.to_owned(),
        value: value.text.clone(),
        location: value.location.clone(),
    }
}

pub(super) fn malformed(input: &Input<'_>, message: &str) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(
        super::grammar::location(input),
        message,
    )))
}
