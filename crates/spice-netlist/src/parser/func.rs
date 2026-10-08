//! `.func` card grammar (user-defined numparam functions).
//!
//! C: `src/frontend/inpcom.c`, `inp_get_func_from_line()`: `.func name(p1,
//! p2, ...) body`, with an optional (undocumented) `=` before the body. C then
//! strips every brace and every whitespace character from the body
//! (`inp_strip_braces()`), after `inp_change_quotes()` has turned quote pairs
//! into braces. The port accepts the body as one `{...}` expression, one
//! `'...'` expression, or the bare rest of the card, and parses it with the
//! ordinary expression grammar ([`crate::expr`]); nothing is evaluated here.
//! Mixed spellings such as `{a}+{b}` (which C would glue together) are
//! reported as not yet ported rather than reinterpreted.
//!
//! `.param name(p1, ...) [=] body` is the same definition: C rewrites a
//! `.param` card whose first token contains `(` into `.func`
//! (`inp_fix_macro_param_func_paren_io()`), so [`super::param`] hands such a
//! card to [`definition`] with [`FuncSpelling::Param`].

use spice_core::{SourceLoc, SpiceError, SpiceResult};
use winnow::Parser as _;
use winnow::combinator::{opt, peek};
use winnow::error::ErrMode;
use winnow::stream::Stream;
use winnow::token::{any, literal, one_of, take_while};

use crate::ast::{FuncCard, FuncParameter, FuncSpelling};
use crate::card::DotCommand;
use crate::expr::{EXCLUDED_FUNCTIONS, Function, ParameterExpression, SourceSpan};
use crate::token::Token;

use super::expression::{
    Ctx, In, Res, cut, cut_unsupported, into_error, is_ident_continue, is_ident_start,
    parse_delimited, parse_expression, ws,
};
use super::grammar::{Failure, Input, ParsedCard, Result};
use super::param::resolved;

/// C reference for `.func` diagnostics.
const C_REFERENCE: &str = "src/frontend/inpcom.c (inp_get_func_from_line)";

/// C reference for the body text C concatenates after removing braces.
const C_STRIP_REFERENCE: &str =
    "src/frontend/inpcom.c (inp_get_func_from_line, inp_strip_braces, inp_split_multi_param_lines)";

pub(super) fn func_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    any.verify(|token: &Token| {
        token.text.starts_with('.') && DotCommand::parse(&token.text) == DotCommand::Func
    })
    .parse_next(input)?;
    let location = input.state.card.location.clone();
    let Some(first) = input.input.first().map(|token| token.location.column) else {
        return Err(super::syntax::malformed(
            input,
            "expected a function name after .func",
        ));
    };
    let raw = input.state.card.raw.as_str();
    // The text grammar owns the whole tail (the tokenizer already matched
    // braces and quotes), so drain the token slice.
    winnow::token::rest.parse_next(input)?;
    let start = (first as usize).saturating_sub(1).min(raw.len());
    let card = definition(raw, start, &location, FuncSpelling::Func)
        .map_err(|error| ErrMode::Cut(Failure(error)))?;
    Ok(ParsedCard::Func(card))
}

/// Parses `raw[start..]` as `name(params) [=] body`.
pub(super) fn definition(
    raw: &str,
    start: usize,
    origin: &SourceLoc,
    spelling: FuncSpelling,
) -> SpiceResult<FuncCard> {
    let text = &raw[start..];
    let column = u32::try_from(start).unwrap_or(u32::MAX).saturating_add(1);
    let mut input = In {
        input: text,
        state: Ctx {
            origin,
            column,
            total: text.len(),
            depth: 0,
        },
    };
    let card = match card(&mut input, spelling) {
        Ok(card) => card,
        Err(ErrMode::Backtrack(fail) | ErrMode::Cut(fail)) => {
            return Err(into_error(origin, column, text.len(), fail));
        }
        Err(ErrMode::Incomplete(_)) => unreachable!("complete input"),
    };
    validate(&card)?;
    Ok(card)
}

fn identifier<'a>(input: &mut In<'a>) -> Res<(&'a str, SourceSpan)> {
    let start = input.eof_offset();
    let name: &str = (one_of(is_ident_start), take_while(0.., is_ident_continue))
        .take()
        .parse_next(input)?;
    let end = input.eof_offset();
    Ok((
        name,
        SourceSpan {
            start: input.state.location(start),
            end: input.state.location(end),
        },
    ))
}

fn card(input: &mut In<'_>, spelling: FuncSpelling) -> Res<FuncCard> {
    ws.parse_next(input)?;
    let here = input.eof_offset();
    let Some((name, name_span)) = opt(identifier).parse_next(input)? else {
        return Err(cut(here, "expected a function name after .func"));
    };
    ws.parse_next(input)?;
    let here = input.eof_offset();
    if opt(literal("(")).parse_next(input)?.is_none() {
        return Err(cut(
            here,
            format!("expected '(' and a parameter list after function name '{name}'"),
        ));
    }
    let mut parameters = Vec::new();
    ws.parse_next(input)?;
    if opt(literal(")")).parse_next(input)?.is_none() {
        loop {
            ws.parse_next(input)?;
            let here = input.eof_offset();
            let Some((parameter, span)) = opt(identifier).parse_next(input)? else {
                return Err(cut(here, "expected a parameter name in the .func list"));
            };
            parameters.push(FuncParameter {
                name: parameter.to_ascii_lowercase(),
                span,
            });
            ws.parse_next(input)?;
            let here = input.eof_offset();
            match opt(one_of([',', ')'])).parse_next(input)? {
                Some(',') => {}
                Some(_) => break,
                None => {
                    return Err(cut(here, "expected ',' or ')' in the .func parameter list"));
                }
            }
        }
    }
    ws.parse_next(input)?;
    // C skips an optional, undocumented '=' before a `.func` body. The
    // `.param` spelling needs it: without one C does not keep the card as a
    // definition and the deck fails (checked against the C binary).
    let here = input.eof_offset();
    if opt(literal("=")).parse_next(input)?.is_some() {
        ws.parse_next(input)?;
    } else if spelling == FuncSpelling::Param {
        return Err(cut(
            here,
            format!("expected '=' after the parameter list of function '{name}'"),
        ));
    }
    let body = body(input, name)?;
    Ok(FuncCard {
        name: name.to_ascii_lowercase(),
        name_span,
        parameters,
        body,
        spelling,
        location: input.state.origin.clone(),
    })
}

fn body(input: &mut In<'_>, name: &str) -> Res<ParameterExpression> {
    let start = input.eof_offset();
    let expression = match peek(opt(any)).parse_next(input)? {
        None => {
            return Err(cut(
                start,
                format!("expected a body expression for function '{name}'"),
            ));
        }
        Some(open @ ('{' | '\'')) => {
            let close = if open == '{' { '}' } else { '\'' };
            any.void().parse_next(input)?;
            let body_start = input.eof_offset();
            let text: &str =
                take_while(0.., |c: char| c != close && c != '{' && c != '}').parse_next(input)?;
            let here = input.eof_offset();
            if opt(one_of([close])).parse_next(input)?.is_none() {
                return Err(cut(
                    if here == 0 { start } else { here },
                    if here == 0 {
                        "unterminated function body".to_owned()
                    } else {
                        "nested braces are not supported in a function body".to_owned()
                    },
                ));
            }
            let column = input.state.location(body_start).column;
            parse_delimited(text, input.state.origin, column, true, open == '\'')
                .map_err(resolved)?
        }
        Some('"') => {
            return Err(cut_unsupported(
                start,
                "a double-quoted string is not a function body",
            ));
        }
        Some(_) => {
            let text: &str = winnow::token::rest.parse_next(input)?;
            let text = text.trim_end();
            let column = input.state.location(start).column;
            parse_expression(text, input.state.origin, column, false).map_err(resolved)?
        }
    };
    ws.parse_next(input)?;
    let here = input.eof_offset();
    if peek(opt(any)).parse_next(input)?.is_some() {
        // Only a delimited body can be followed by text (a bare body is the
        // rest of the card). C accepts it: `inp_strip_braces()` glues
        // `{x}+{1}` into `x+1`, and a `.param` card with a further
        // assignment is split first. Neither is reproduced.
        return Err(resolved(SpiceError::not_yet_ported(
            format!(
                "{}: text after the delimited body of function '{name}' (C removes the \
                 braces and joins the pieces, or splits a multi-assignment .param card)",
                input.state.location(here)
            ),
            C_STRIP_REFERENCE,
        )));
    }
    Ok(expression)
}

/// Checks that need the whole card: formal names, and redefinitions of the
/// allowlisted built-ins.
fn validate(card: &FuncCard) -> SpiceResult<()> {
    for (index, parameter) in card.parameters.iter().enumerate() {
        if Function::from_name(&parameter.name).is_some()
            || EXCLUDED_FUNCTIONS.contains(&parameter.name.as_str())
        {
            return Err(SpiceError::parse(
                parameter.span.start.clone(),
                format!(
                    "function parameter '{}' is the name of a built-in function",
                    parameter.name
                ),
            ));
        }
        if let Some(first) = card.parameters[..index]
            .iter()
            .find(|other| other.name == parameter.name)
        {
            // C accepts this and binds only the first of two equal formals;
            // the port does not reproduce that binding.
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: duplicate parameter '{}' in function '{}' (first at {})",
                    parameter.span.start, parameter.name, card.name, first.span.start
                ),
                C_REFERENCE,
            ));
        }
    }
    if let Some(function) = Function::from_name(&card.name)
        && function.arity() != card.parameters.len()
    {
        // C would replace the built-in with this definition everywhere; the
        // port parses built-in calls with their fixed arity, so a
        // different-arity override cannot be represented yet.
        return Err(SpiceError::not_yet_ported(
            format!(
                "{}: .func '{}' redefines a built-in that takes {} argument(s) with {}",
                card.name_span.start,
                card.name,
                function.arity(),
                card.parameters.len()
            ),
            C_REFERENCE,
        ));
    }
    Ok(())
}
