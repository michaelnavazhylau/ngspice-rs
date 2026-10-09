//! Winnow grammars for `src/spicelib/parser/inp2{r,c,l,v,i}.c`.
//!
//! `INPdevParse()` (`inpdpar.c`) handles leading values and named parameters;
//! scalar names come from `res/res.c`, `cap/cap.c`, and `ind/ind.c`. Cuts after
//! recognised prefixes prevent optional/repeated parsers from hiding errors.

use crate::primitives::{SourceLoc, SpiceError};
use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, ParameterAssignment, ParameterKind};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, gap, keyword, location};
use super::syntax::{
    assignment, canonical_node, equals, leading_value, malformed, name as node, named, value,
};

pub(super) fn device_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let name = any
        .verify(|token: &Token| matches!(designator(token), 'r' | 'c' | 'l' | 'v' | 'i'))
        .parse_next(input)?;
    let designator = designator(name);
    // Once the device prefix matches, failure is definitive. Without this cut,
    // alt could fall through to the generic unported-device branch.
    let (positive, negative) =
        cut_err((node("positive terminal"), node("negative terminal"))).parse_next(input)?;
    let (model, parameters) = cut_err(|input: &mut Input<'_>| {
        if matches!(designator, 'v' | 'i') {
            source_parameters(input).map(|parameters| (None, parameters))
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
        model,
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

fn primary_name(designator: char) -> &'static str {
    match designator {
        'r' => "resistance",
        'c' => "capacitance",
        _ => "inductance",
    }
}

/// INP2R/C/L set a pre-model scalar before named setters. A scalar immediately
/// after the model is INPdevParse's leading value, applied *after* named setters.
/// No scalar is injected for a model-only/geometry instance. Declared numeric
/// names cannot steal the first numeric slot; model semantics stay in devices.
fn passive_parameters(
    input: &mut Input<'_>,
    designator: char,
) -> Result<(Option<String>, Vec<ParameterAssignment>)> {
    let primary = primary_name(designator);
    let leading = opt(leading_value).parse_next(input)?;
    let model = opt(passive_model).parse_next(input)?;
    let after_model = if model.is_some() {
        opt(leading_value).parse_next(input)?
    } else {
        None
    };
    let remaining: Vec<ParameterAssignment> =
        repeat(0.., alt((passive_assignment, invalid_passive))).parse_next(input)?;
    let mut parameters = Vec::new();
    if let Some(value) = leading {
        parameters.push(assignment(primary, value));
    }
    parameters.extend(remaining);
    if let Some(value) = after_model {
        parameters.push(assignment(primary, value));
    }
    if model.is_none() && !parameters.iter().any(|parameter| parameter.name == primary) {
        return Err(ErrMode::Cut(Failure(SpiceError::parse(
            location(input),
            format!("expected {primary} or a declared passive model"),
        ))));
    }
    Ok((
        model.map(|token| token.text.to_ascii_lowercase()),
        parameters,
    ))
}

fn passive_model<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    let designator = input.state.card.designator().expect("passive card");
    // C reserves r/c/l for scalar assignment. inpcom's named-assignment
    // preprocessing also preserves keyword=value even if keyword is declared
    // as a model. Bare declared keywords in this slot *are* model references.
    let assignment = input
        .input
        .get(1)
        .is_some_and(|token| token.kind == TokenKind::Equals);
    let declared = input.state.declared_models;
    if input.input.first().is_some_and(|token| {
        token.number().is_some() && declared.contains(&token.text.to_ascii_lowercase())
    }) {
        return Err(gap(input, "numeric-looking passive model references"));
    }
    if assignment
        && input.input.first().is_some_and(|token| {
            declared.contains(&token.text.to_ascii_lowercase())
                && scalar_name(designator, &token.text).is_none()
        })
    {
        return Err(malformed(
            input,
            "passive model reference does not take '='",
        ));
    }
    any.verify(|token: &Token| {
        token.is_name_like()
            && !assignment
            && !token.text.eq_ignore_ascii_case(&designator.to_string())
            && declared.contains(&token.text.to_ascii_lowercase())
    })
    .parse_next(input)
}

fn passive_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let designator = input.state.card.designator().expect("device card");
    let (token, name) = any
        .verify_map(move |token: &Token| {
            scalar_name(designator, &token.text).map(|name| (token, name))
        })
        .parse_next(input)?;
    // INPgetTok() gobbles '=': both tc1=0.01 and tc1 0.01 are legal.
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(&name, token.location.clone(), value))
}

fn scalar_name(designator: char, text: &str) -> Option<String> {
    let name = text.to_ascii_lowercase();
    let canonical = match (designator, name.as_str()) {
        ('r', "r" | "resistance") => primary_name(designator),
        ('c', "c" | "cap" | "capacitance") => primary_name(designator),
        ('l', "l" | "inductance") => primary_name(designator),
        (_, "temp" | "dtemp" | "m" | "tc1" | "tc2" | "scale") => &name,
        ('r' | 'c', "w" | "l" | "bv_max") => &name,
        ('r', "ac" | "tc" | "tce" | "noisy") => &name,
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
        "undeclared passive model names, expressions or non-scalar parameters",
    ))
}

/// `INP2V()`/`INP2I()` apply leading DC after `INPdevParse()` named parameters.
/// VSRCtemp/ISRCtemp default bare AC to magnitude 1 and phase 0.
fn source_parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let leading = opt(leading_value).parse_next(input)?;
    let mut parameters = repeat(
        0..,
        alt((
            dc_parameters,
            ac_parameters,
            distortion_parameters,
            port_parameter,
            super::waveform::parameters,
            super::waveform::pwl_options,
            invalid_source,
        )),
    )
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
    let (token, (_, value)) = (keyword("dc"), cut_err((opt(equals), value))).parse_next(input)?;
    Ok(vec![named("dc", token.location.clone(), value)])
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

/// `distof1 [mag [phase]]` / `distof2 [mag [phase]]` (`vsrc.c`/`isrc.c`
/// `IF_REALVEC` setters, `vsrcpar.c`: a missing magnitude is 1 and a missing
/// phase 0), kept as the ordered pair `distof<k>mag`, `distof<k>phase`.
fn distortion_parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let (token, which) = alt((
        keyword("distof1").map(|token| (token, 1)),
        keyword("distof2").map(|token| (token, 2)),
    ))
    .parse_next(input)?;
    let (magnitude, phase) = if which == 1 {
        ("distof1mag", "distof1phase")
    } else {
        ("distof2mag", "distof2phase")
    };
    cut_err((
        opt(equals),
        ac_value(magnitude, "1", &token.location),
        ac_value(phase, "0", &token.location),
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
            matches!(token.kind, TokenKind::Quoted(_))
                && !super::expression::is_expression_token(token)
        }) {
            return Err(gap(input, "double-quoted AC parameter strings"));
        }
        opt(leading_value)
            .map(|value| {
                value.map_or_else(
                    || ParameterAssignment {
                        name: name.to_owned(),
                        value: default.to_owned(),
                        kind: ParameterKind::Scalar,
                        location: default_location.clone(),
                    },
                    |token| assignment(name, token),
                )
            })
            .parse_next(input)
    }
}

/// The RFSPICE port setters of a voltage source (`vsrc.c`: `portnum`, `z0`,
/// `pwr`, `freq`, `phase`), each `name [=] value`, kept as ordered scalar
/// assignments: `vsrcpar.c` applies them in deck order and `portnum` reads the
/// `z0` set so far. Current sources have no such parameters.
fn port_parameter(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let voltage = input.state.card.designator() == Some('v');
    let (token, name) = any
        .verify_map(move |token: &Token| {
            let name = token.text.to_ascii_lowercase();
            (voltage && matches!(name.as_str(), "portnum" | "z0" | "pwr" | "freq" | "phase"))
                .then_some((token, name))
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(vec![named(&name, token.location.clone(), value)])
}

fn invalid_source(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let token = peek(any).parse_next(input)?;
    if matches!(
        token.kind,
        TokenKind::Equals | TokenKind::LParen | TokenKind::RParen | TokenKind::Comma
    ) {
        return Err(malformed(input, "expected a named source parameter"));
    }
    Err(gap(
        input,
        "unsupported source waveforms, expressions or additional source parameters",
    ))
}
