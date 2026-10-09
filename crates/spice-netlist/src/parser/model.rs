//! Bounded `.model` grammar (`inpdomod.c`, `inpgmod.c`, `inpfindl.c`).
//!
//! Retains scalar assignments and bounded bare type flags for D/BJT/MOS/R/C/L
//! and the SW/CSW switch models (`sw.c`/`csw.c` `SWmPTable`/`CSWmPTable`).
//! The base token remains distinct from ordered tail flag setters.
//! Device parameter validity, default levels, selector rounding, model lookup
//! and availability are elaboration concerns, not claims made by this grammar.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::ast::{ModelCard, ParameterAssignment};
use crate::token::{Token, TokenKind};

use super::grammar::{Input, ParsedCard, Result, gap, keyword};
use super::syntax::{equals, malformed, name, named, value};

pub(super) fn model_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    keyword(".model").parse_next(input)?;
    cut_err(model_body).parse_next(input)
}

fn model_body(input: &mut Input<'_>) -> Result<ParsedCard> {
    let (model_name, base) = (name("model name"), model_base).parse_next(input)?;
    // C gobbles commas as delimiters. We retain a single optional outer pair
    // of parentheses, diagnosing an unmatched pair instead of ignoring it.
    let parameters: Vec<ParameterAssignment> = alt((
        (
            punctuation(TokenKind::LParen),
            cut_err((
                |input: &mut Input<'_>| parameters(input, &base.text),
                punctuation(TokenKind::RParen).context("')' after model parameters"),
            )),
        )
            .map(|(_, (parameters, _))| parameters),
        |input: &mut Input<'_>| parameters(input, &base.text),
    ))
    .parse_next(input)?;
    // INPfindLev scans the first level. Keep its literal value here, without
    // injecting defaults or rounding it into a backend selector. Preserve all
    // assignments separately, including duplicates for future model setters.
    let level = parameters
        .iter()
        .find(|parameter| parameter.name == "level")
        .map(|parameter| {
            spice_core::parse_spice_number(&parameter.value).expect("validated scalar")
        });
    Ok(ParsedCard::Model(ModelCard {
        name: model_name.text.to_ascii_lowercase(),
        base: base.text.to_ascii_lowercase(),
        level,
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn model_base<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    peek(name("model type")).parse_next(input)?;
    let base = input.input.first().expect("peek succeeded");
    if !matches!(
        base.text.to_ascii_lowercase().as_str(),
        "d" | "npn" | "pnp" | "nmos" | "pmos" | "r" | "res" | "c" | "l" | "sw" | "csw"
    ) {
        return Err(gap(
            input,
            "model family outside D/BJT/MOS/R/C/L/SW/CSW scalar syntax",
        ));
    }
    any.parse_next(input)
}

fn parameters(input: &mut Input<'_>, base: &str) -> Result<Vec<ParameterAssignment>> {
    repeat(
        0..,
        alt((
            punctuation(TokenKind::Comma).value(None),
            (|input: &mut Input<'_>| super::flags::model(input, base)).map(Some),
            scalar_assignment.map(Some),
            invalid_parameter,
        )),
    )
    .fold(Vec::new, |mut parameters, parameter| {
        if let Some(parameter) = parameter {
            parameters.push(parameter);
        }
        parameters
    })
    .parse_next(input)
}

fn scalar_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    // A flag is not a scalar assignment, even when it is spelled as a word.
    if input.input.first().is_some_and(|token| {
        [
            "off",
            "nchan",
            "pchan",
            "thermal",
            "d",
            "npn",
            "pnp",
            "nmos",
            "pmos",
            "sw",
            "csw",
            "sens_area",
            "sens_l",
            "sens_w",
        ]
        .iter()
        .any(|flag| token.is_keyword(flag))
    }) {
        return Err(gap(input, "non-scalar model flags"));
    }
    let token = any
        .verify(|token: &Token| matches!(token.kind, TokenKind::Word))
        .parse_next(input)?;
    let name = token.text.to_ascii_lowercase();
    opt(equals).parse_next(input)?;
    if name == "level"
        && input
            .input
            .first()
            .is_some_and(super::expression::is_expression_token)
    {
        // INPfindLev reads the literal level; ModelCard::level cannot carry an
        // unevaluated expression, so refuse instead of defaulting it.
        return Err(gap(input, "expression-valued model level selectors"));
    }
    let value = cut_err(value).parse_next(input)?;
    Ok(named(&name, token.location.clone(), value))
}

fn invalid_parameter(input: &mut Input<'_>) -> Result<Option<ParameterAssignment>> {
    let token =
        peek(any.verify(|token: &Token| token.kind != TokenKind::RParen)).parse_next(input)?;
    if matches!(token.kind, TokenKind::Expression(_) | TokenKind::Quoted(_)) {
        return Err(gap(input, "non-scalar model parameters"));
    }
    Err(malformed(input, "expected a scalar model assignment"))
}

fn punctuation<'a>(
    kind: TokenKind,
) -> impl winnow::Parser<Input<'a>, &'a Token, ErrMode<super::grammar::Failure>> {
    any.verify(move |token: &Token| token.kind == kind)
}
