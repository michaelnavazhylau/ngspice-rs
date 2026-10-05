//! Winnow grammars for `src/spicelib/parser/inp2{r,c,l,v,i}.c`.
//!
//! `INPdevParse()` (`inpdpar.c`) handles leading values and named parameters;
//! scalar names come from `res/res.c`, `cap/cap.c`, and `ind/ind.c`. Cuts after
//! recognised prefixes prevent optional/repeated parsers from hiding errors.

use spice_core::{SourceLoc, SpiceError};
use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::ast::{DeviceInstance, ParameterAssignment};
use crate::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, gap, keyword, location};

pub(super) fn device_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let name = any
        .verify(|token: &Token| matches!(designator(token), 'r' | 'c' | 'l' | 'v' | 'i'))
        .parse_next(input)?;
    let designator = designator(name);
    // Once the device prefix matches, failure is definitive. Without this cut,
    // alt could fall through to the generic unported-device branch.
    let (positive, negative) =
        cut_err((node("positive terminal"), node("negative terminal"))).parse_next(input)?;
    let parameters = cut_err(|input: &mut Input<'_>| {
        if matches!(designator, 'v' | 'i') {
            source_parameters(input)
        } else {
            passive_parameters(input, designator)
        }
    })
    .parse_next(input)?;
    Ok(ParsedCard::Device(DeviceInstance {
        name: name.text.to_ascii_lowercase(),
        designator,
        nodes: vec![
            canonical_node(positive, input.state.auto_gnd),
            canonical_node(negative, input.state.auto_gnd),
        ],
        model: None,
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn designator(token: &Token) -> char {
    token
        .text
        .chars()
        .next()
        .unwrap_or('\0')
        .to_ascii_lowercase()
}

fn node<'a>(expected: &'static str) -> impl winnow::Parser<Input<'a>, &'a Token, ErrMode<Failure>> {
    any.verify(|token: &Token| token.is_name_like())
        .context(expected)
}

fn canonical_node(token: &Token, auto_gnd: bool) -> String {
    // inpcom.c: inp_fix_gnd_name(); never rewrite values or instance names.
    let name = token.text.to_ascii_lowercase();
    if auto_gnd && name == "gnd" {
        "0".to_owned()
    } else {
        name
    }
}

fn primary_name(designator: char) -> &'static str {
    match designator {
        'r' => "resistance",
        'c' => "capacitance",
        _ => "inductance",
    }
}

fn passive_parameters(input: &mut Input<'_>, designator: char) -> Result<Vec<ParameterAssignment>> {
    let primary = primary_name(designator);
    let (leading, remaining): (Option<&Token>, Vec<ParameterAssignment>) = (
        opt(leading_literal),
        repeat(0.., alt((passive_assignment, invalid_passive))),
    )
        .parse_next(input)?;
    let mut parameters = Vec::with_capacity(remaining.len() + usize::from(leading.is_some()));
    if let Some(value) = leading {
        parameters.push(assignment(primary, value));
    }
    parameters.extend(remaining);
    if !parameters.iter().any(|parameter| parameter.name == primary) {
        return Err(ErrMode::Cut(Failure(SpiceError::parse(
            location(input),
            format!("expected {primary} (model-backed instances are not ported)"),
        ))));
    }
    Ok(parameters)
}

fn passive_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let designator = input.state.card.designator().expect("device card");
    let (token, name) = any
        .verify_map(move |token: &Token| {
            scalar_name(designator, &token.text).map(|name| (token, name))
        })
        .parse_next(input)?;
    // INPgetTok() gobbles '=': both tc1=0.01 and tc1 0.01 are legal.
    let (_, value) = cut_err((opt(equals), literal)).parse_next(input)?;
    Ok(ParameterAssignment {
        name,
        value: value.text.clone(),
        location: token.location.clone(),
    })
}

fn scalar_name(designator: char, text: &str) -> Option<String> {
    let name = text.to_ascii_lowercase();
    let canonical = match (designator, name.as_str()) {
        ('r', "r" | "resistance") => primary_name(designator),
        ('c', "c" | "cap" | "capacitance") => primary_name(designator),
        ('l', "l" | "inductance") => primary_name(designator),
        (_, "temp" | "dtemp" | "m" | "tc1" | "tc2" | "scale") => &name,
        ('r' | 'c', "w" | "l" | "bv_max") => &name,
        ('r', "ac" | "tc" | "tce") => &name,
        ('c' | 'l', "ic") => &name,
        ('l', "nt") => &name,
        _ => return None,
    };
    Some(canonical.to_owned())
}

fn invalid_passive(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    peek(any).parse_next(input)?;
    Err(gap(
        input,
        "passive models, expressions or non-scalar parameters",
    ))
}

/// `INP2V()`/`INP2I()` apply leading DC after `INPdevParse()` named parameters.
/// VSRCtemp/ISRCtemp default bare AC to magnitude 1 and phase 0.
fn source_parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let leading = opt(leading_literal).parse_next(input)?;
    let mut parameters = repeat(0.., alt((dc_parameters, ac_parameters, invalid_source)))
        .fold(Vec::new, |mut parameters, chunk| {
            parameters.extend(chunk);
            parameters
        })
        .parse_next(input)?;
    if let Some(value) = leading {
        parameters.push(assignment("dc", value));
    }
    // No explicit source value is legal: keep the implicit DC zero implicit.
    Ok(parameters)
}

fn dc_parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let (token, (_, value)) = (keyword("dc"), cut_err((opt(equals), literal))).parse_next(input)?;
    Ok(vec![ParameterAssignment {
        name: "dc".to_owned(),
        value: value.text.clone(),
        location: token.location.clone(),
    }])
}

fn ac_parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let token = keyword("ac").parse_next(input)?;
    cut_err((
        opt(equals),
        ac_value("acmag", "1", &token.location),
        ac_value("acphase", "0", &token.location),
    ))
    .map(|(_, magnitude, phase)| vec![magnitude, phase])
    .parse_next(input)
}

fn ac_value<'a>(
    name: &'static str,
    default: &'static str,
    default_location: &SourceLoc,
) -> impl winnow::Parser<Input<'a>, ParameterAssignment, ErrMode<Failure>> {
    let default_location = default_location.clone();
    move |input: &mut Input<'a>| {
        if input.input.first().is_some_and(|token| {
            matches!(token.kind, TokenKind::Expression(_) | TokenKind::Quoted(_))
        }) {
            return Err(gap(input, "AC parameter expressions"));
        }
        opt(leading_literal)
            .map(|value| {
                value.map_or_else(
                    || ParameterAssignment {
                        name: name.to_owned(),
                        value: default.to_owned(),
                        location: default_location.clone(),
                    },
                    |token| assignment(name, token),
                )
            })
            .parse_next(input)
    }
}

fn invalid_source(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    peek(any).parse_next(input)?;
    Err(gap(
        input,
        "source waveforms, expressions or additional source parameters",
    ))
}

fn equals<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|token: &Token| token.kind == TokenKind::Equals)
        .parse_next(input)
}

/// Only a numeric prefix claims the optional positional slot. Once claimed,
/// numeric overflow is a committed syntax error, not a missing optional value.
fn leading_literal<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    peek(any.verify(|token: &Token| token.number().is_some())).parse_next(input)?;
    cut_err(literal).parse_next(input)
}

fn literal<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
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

fn assignment(name: &str, value: &Token) -> ParameterAssignment {
    ParameterAssignment {
        name: name.to_owned(),
        value: value.text.clone(),
        location: value.location.clone(),
    }
}
