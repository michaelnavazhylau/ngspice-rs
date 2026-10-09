//! `.param` card grammar (bounded numparam assignments).
//!
//! C splits a multi-assignment line in `inpcom.c`,
//! `inp_split_multi_param_lines()`: each right-hand side runs to the next
//! whitespace outside `{}`/`()` or to a comma outside parentheses, and
//! `nupa_assignment()` (`xpressn.c`) evaluates `name = expression` pairs in
//! order. A card whose first token contains `(` is a function definition
//! (`.param f(x) = {x*3}`), parsed by [`super::func`]. This grammar applies that extent rule to every card, including
//! single-assignment ones where C is more permissive: spaces inside an
//! unbraced expression must be written as `{ ... }` (or `'...'`, which
//! `inp_change_quotes()` makes identical to braces). Duplicates and order are
//! preserved; nothing is evaluated.

use std::cell::Cell;

use spice_core::{SourceLoc, SpiceResult};
use winnow::Parser as _;
use winnow::combinator::{opt, peek, preceded, repeat};
use winnow::error::ErrMode;
use winnow::stream::Stream;
use winnow::token::{any, literal, one_of, take_while};

use crate::ast::{FuncSpelling, ParamAssignment, ParamCard};
use crate::card::DotCommand;
use crate::expr::{ParameterExpression, SourceSpan};
use crate::token::Token;

use super::expression::{
    Ctx, Fail, In, Res, cut, cut_unsupported, into_error, is_ident_continue, is_ident_start,
    parse_delimited, parse_expression, ws,
};
use super::grammar::{Failure, Input, ParsedCard, Result};

pub(super) fn param_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    any.verify(|token: &Token| {
        token.text.starts_with('.') && DotCommand::parse(&token.text) == DotCommand::Param
    })
    .parse_next(input)?;
    let location = input.state.card.location.clone();
    let Some(first) = input.input.first().map(|token| token.location.column) else {
        return Err(super::syntax::malformed(
            input,
            "expected a name=expression assignment after .param",
        ));
    };
    let raw = input.state.card.raw.as_str();
    // The text grammar owns the whole tail; drain the token slice so the card
    // is fully consumed (the tokenizer already matched braces and quotes).
    winnow::token::rest.parse_next(input)?;
    let start = (first as usize).saturating_sub(1).min(raw.len());
    if defines_function(&raw[start..]) {
        let card = super::func::definition(raw, start, &location, FuncSpelling::Param)
            .map_err(|error| ErrMode::Cut(Failure(error)))?;
        return Ok(ParsedCard::Func(card));
    }
    let assignments =
        assignments(raw, start, &location).map_err(|error| ErrMode::Cut(Failure(error)))?;
    Ok(ParsedCard::Param(ParamCard {
        assignments,
        location,
    }))
}

/// C's test in `inpcom.c` `inp_fix_macro_param_func_paren_io()`: a `.param`
/// card whose first token (up to whitespace or `=`) contains `(` is a
/// function definition and is rewritten to `.func`, unconditionally.
fn defines_function(text: &str) -> bool {
    text.chars()
        .take_while(|&c| !c.is_ascii_whitespace() && c != '=')
        .any(|c| c == '(')
}

/// Parses `raw[start..]` (columns are 1-based bytes of the joined card).
pub(super) fn assignments(
    raw: &str,
    start: usize,
    origin: &SourceLoc,
) -> SpiceResult<Vec<ParamAssignment>> {
    let text = &raw[start..];
    let column = u32::try_from(start).unwrap_or(u32::MAX).saturating_add(1);
    let mut input = In {
        input: text,
        state: Ctx {
            origin,
            column,
            total: text.len(),
            depth: 0,
            shadowing: &[],
        },
    };
    match list(&mut input) {
        Ok(assignments) => Ok(assignments),
        Err(ErrMode::Backtrack(fail) | ErrMode::Cut(fail)) => {
            Err(into_error(origin, column, text.len(), fail))
        }
        Err(ErrMode::Incomplete(_)) => unreachable!("complete input"),
    }
}

fn list(input: &mut In<'_>) -> Res<Vec<ParamAssignment>> {
    ws.parse_next(input)?;
    let here = input.eof_offset();
    let Some(first) = opt(assignment).parse_next(input)? else {
        return Err(cut(
            here,
            "expected a parameter name at the start of a 'name = expression' assignment",
        ));
    };
    let rest: Vec<ParamAssignment> =
        repeat(0.., preceded(separator, assignment)).parse_next(input)?;
    ws.parse_next(input)?;
    let here = input.eof_offset();
    if let Some(c) = peek(opt(any)).parse_next(input)? {
        return Err(cut(
            here,
            match c {
                ')' => "unmatched ')'".to_owned(),
                '}' => "unmatched '}'".to_owned(),
                other => format!(
                    "expected a 'name = expression' assignment, found '{other}' \
                     (an unbraced expression ends at whitespace; write spaces inside {{...}})"
                ),
            },
        ));
    }
    let mut all = vec![first];
    all.extend(rest);
    Ok(all)
}

/// Assignments are separated by whitespace and/or commas (C splits on both).
fn separator(input: &mut In<'_>) -> Res<()> {
    take_while(1.., |c: char| c.is_ascii_whitespace() || c == ',')
        .void()
        .parse_next(input)
}

fn assignment(input: &mut In<'_>) -> Res<ParamAssignment> {
    let start = input.eof_offset();
    let name: &str = (one_of(is_ident_start), take_while(0.., is_ident_continue))
        .take()
        .parse_next(input)?;
    let end = input.eof_offset();
    if peek(opt(literal("("))).parse_next(input)?.is_some() {
        // A later assignment of a multi-assignment card that defines a
        // function: C splits the card first, then rewrites that piece to
        // `.func`. Only a card that is a single definition is ported.
        return Err(resolved(spice_core::SpiceError::not_yet_ported(
            format!(
                "{}: function definition '{name}(...)' inside a multi-assignment .param \
                 card (write it on its own .param or .func card)",
                input.state.location(end)
            ),
            "src/frontend/inpcom.c (inp_split_multi_param_lines, \
             inp_fix_macro_param_func_paren_io)",
        )));
    }
    // Committed from here: a name must be followed by '='.
    ws.parse_next(input)?;
    let here = input.eof_offset();
    if opt(literal("=")).parse_next(input)?.is_none() {
        return Err(cut(
            here,
            format!("expected '=' after parameter name '{name}'"),
        ));
    }
    ws.parse_next(input)?;
    let expression = value(input)?;
    Ok(ParamAssignment {
        name: name.to_ascii_lowercase(),
        name_span: SourceSpan {
            start: input.state.location(start),
            end: input.state.location(end),
        },
        expression,
    })
}

fn value(input: &mut In<'_>) -> Res<ParameterExpression> {
    let start = input.eof_offset();
    match peek(opt(any)).parse_next(input)? {
        None => Err(cut(start, "expected an expression after '='")),
        Some('{') => braced(input),
        Some('\'') => quoted(input),
        Some('"') => Err(cut_unsupported(
            start,
            "double-quoted string parameters are outside the bounded subset",
        )),
        Some(_) => unbraced(input),
    }
}

fn braced(input: &mut In<'_>) -> Res<ParameterExpression> {
    let open = input.eof_offset();
    literal("{").void().parse_next(input)?;
    let body_start = input.eof_offset();
    let body: &str = take_while(0.., |c: char| c != '{' && c != '}').parse_next(input)?;
    let here = input.eof_offset();
    match opt(literal("}")).parse_next(input)? {
        Some(_) => {}
        None => {
            return Err(cut(
                if here == 0 { open } else { here },
                if here == 0 {
                    "unterminated '{' expression".to_owned()
                } else {
                    "nested '{' is not supported inside an expression".to_owned()
                },
            ));
        }
    }
    parse_slice(input, body, body_start, true)
}

/// `'expr'`: C's `inp_change_quotes()` (`inpcom.c`) rewrites the quote pair
/// to braces before the card is split, so this is exactly a braced value.
fn quoted(input: &mut In<'_>) -> Res<ParameterExpression> {
    let open = input.eof_offset();
    literal("'").void().parse_next(input)?;
    let body_start = input.eof_offset();
    let body: &str =
        take_while(0.., |c: char| c != '\'' && c != '{' && c != '}').parse_next(input)?;
    let here = input.eof_offset();
    if opt(literal("'")).parse_next(input)?.is_none() {
        return Err(cut(
            if here == 0 { open } else { here },
            if here == 0 {
                "unterminated quoted expression".to_owned()
            } else {
                "braces are not supported inside a quoted expression".to_owned()
            },
        ));
    }
    let column = input.state.location(body_start).column;
    parse_delimited(body, input.state.origin, column, true, true).map_err(resolved)
}

/// Extent rule from `inp_split_multi_param_lines()`: whitespace and commas end
/// the value at parenthesis depth 0; `{`/`}`/`)` at depth 0 end it too (they
/// are reported by the caller or the expression grammar).
fn unbraced(input: &mut In<'_>) -> Res<ParameterExpression> {
    let start = input.eof_offset();
    let depth = Cell::new(0usize);
    let slice: &str = take_while(1.., |c: char| match c {
        '(' => {
            depth.set(depth.get() + 1);
            true
        }
        ')' if depth.get() > 0 => {
            depth.set(depth.get() - 1);
            true
        }
        ')' => false,
        '{' | '}' => depth.get() > 0,
        c if c.is_ascii_whitespace() || c == ',' => depth.get() > 0,
        _ => true,
    })
    .parse_next(input)?;
    parse_slice(input, slice, start, false)
}

fn parse_slice(
    input: &In<'_>,
    slice: &str,
    start_remaining: usize,
    braced: bool,
) -> Res<ParameterExpression> {
    let column = input.state.location(start_remaining).column;
    parse_expression(slice, input.state.origin, column, braced).map_err(resolved)
}

/// Wraps an already positioned error from a nested parse.
pub(super) fn resolved(error: spice_core::SpiceError) -> ErrMode<Fail> {
    ErrMode::Cut(Fail {
        remaining: 0,
        message: String::new(),
        unsupported: false,
        resolved: Some(error),
    })
}
