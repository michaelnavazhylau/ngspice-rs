//! Structural card grammars. C references: frontend/subckt.c (doit/x instances),
//! inpcom.c (subckt parameters, include/lib preprocessing). No expansion of Xs.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::error::ParserError;
use winnow::token::any;

use super::grammar::{Input, ParsedCard, Result, keyword};
use super::syntax::{canonical_node, equals, malformed, name};
use crate::ast::{
    DeviceInstance, IncludeDirective, ParameterAssignment, ParameterKind, Subcircuit,
};
use crate::token::{Token, TokenKind};

pub(super) fn structural_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    alt((subckt, ends, instance, include, library, endl)).parse_next(input)
}

fn connection<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    // A name followed by '=' belongs to the parameter tail, not the ports.
    if input.input.first().is_some_and(|t| t.is_keyword("params:"))
        || input
            .input
            .get(1)
            .is_some_and(|t| t.kind == TokenKind::Equals)
    {
        return Err(winnow::error::ErrMode::Backtrack(
            super::grammar::Failure::from_input(input),
        ));
    }
    name("terminal or subcircuit name").parse_next(input)
}

fn subckt(input: &mut Input<'_>) -> Result<ParsedCard> {
    keyword(".subckt").parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let (name, terminals, parameters): (_, Vec<_>, _) =
            (name("subcircuit name"), repeat(0.., connection), parameters).parse_next(input)?;
        let location = input.state.card.location.clone();
        Ok(ParsedCard::Subckt(Subcircuit {
            name: name.text.to_ascii_lowercase(),
            terminals: terminals
                .iter()
                .map(|t| canonical_node(t, input.state.auto_gnd))
                .collect(),
            parameters,
            devices: Vec::new(),
            models: Vec::new(),
            subcircuits: Vec::new(),
            analyses: Vec::new(),
            includes: Vec::new(),
            cards: Vec::new(),
            end_location: location.clone(),
            location,
        }))
    })
    .parse_next(input)
}

fn ends(input: &mut Input<'_>) -> Result<ParsedCard> {
    keyword(".ends").parse_next(input)?;
    cut_err(opt(name("optional subcircuit name")))
        .map(|name| ParsedCard::Ends(name.map(|t| t.text.to_ascii_lowercase())))
        .parse_next(input)
}

fn instance(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|t: &Token| t.is_name_like() && t.text.starts_with(['x', 'X']))
        .parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let (mut connections, parameters): (Vec<_>, _) =
            (repeat(1.., connection), parameters).parse_next(input)?;
        let target = connections.pop().expect("at least one name");
        Ok(ParsedCard::Device(DeviceInstance {
            name: instance.text.to_ascii_lowercase(),
            designator: 'x',
            nodes: connections
                .iter()
                .map(|t| canonical_node(t, input.state.auto_gnd))
                .collect(),
            model: Some(target.text.to_ascii_lowercase()),
            parameters,
            location: input.state.card.location.clone(),
        }))
    })
    .parse_next(input)
}

fn parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let marker = opt(keyword("params:")).parse_next(input)?;
    let values: Vec<_> = repeat(0.., parameter).parse_next(input)?;
    if marker.is_some() && values.is_empty() {
        return Err(malformed(input, "params: requires an assignment"));
    }
    Ok(values)
}

fn parameter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    // Only a complete name/'=' prefix claims an assignment. Committing here
    // ensures repeat cannot swallow missing values or malformed tails.
    peek((opt(comma), name("parameter name"), equals)).parse_next(input)?;
    let (_, key, _, value) =
        cut_err((opt(comma), name("parameter name"), equals, value)).parse_next(input)?;
    Ok(ParameterAssignment {
        name: key.text.to_ascii_lowercase(),
        value: value.text.clone(),
        kind: if value.number().is_some() {
            ParameterKind::Scalar
        } else {
            ParameterKind::Textual
        },
        location: key.location.clone(),
    })
}

fn comma<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|t: &Token| t.kind == TokenKind::Comma)
        .parse_next(input)
}

fn value<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|t: &Token| match t.kind {
        TokenKind::Number(v) => v.is_finite(),
        TokenKind::Word | TokenKind::Expression(_) | TokenKind::Quoted(_) => true,
        _ => false,
    })
    .context("finite literal or unevaluated parameter value")
    .parse_next(input)
}

fn path<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|t: &Token| t.is_name_like() || matches!(t.kind, TokenKind::Quoted(_)))
        .context("source path or library section")
        .parse_next(input)
}

fn path_value(t: &Token) -> String {
    match &t.kind {
        TokenKind::Quoted(s) => s.clone(),
        _ => t.text.clone(),
    }
}

fn directive(input: &Input<'_>, path: &Token, section: Option<&Token>) -> Result<ParsedCard> {
    let decoded_path = path_value(path);
    if decoded_path.is_empty() {
        return Err(malformed(input, "source path cannot be empty"));
    }
    if section.is_some_and(|t| path_value(t).is_empty()) {
        return Err(malformed(input, "library section cannot be empty"));
    }
    Ok(ParsedCard::Include(IncludeDirective {
        path: decoded_path,
        path_spelling: path.text.clone(),
        resolved_path: None,
        section: section.map(|t| path_value(t).to_ascii_lowercase()),
        selected_section: None,
        location: input.state.card.location.clone(),
    }))
}

fn include(input: &mut Input<'_>) -> Result<ParsedCard> {
    alt((keyword(".include"), keyword(".inc"))).parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let path = path.parse_next(input)?;
        directive(input, path, None)
    })
    .parse_next(input)
}

fn library(input: &mut Input<'_>) -> Result<ParsedCard> {
    keyword(".lib").parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let (first, second) = (path, opt(path)).parse_next(input)?;
        match second {
            Some(section) => directive(input, first, Some(section)),
            None => {
                let section = path_value(first).to_ascii_lowercase();
                if section.is_empty() {
                    return Err(malformed(input, "library section cannot be empty"));
                }
                Ok(ParsedCard::LibStart(section))
            }
        }
    })
    .parse_next(input)
}

fn endl(input: &mut Input<'_>) -> Result<ParsedCard> {
    keyword(".endl").parse_next(input)?;
    cut_err(opt(path))
        .map(|t| ParsedCard::LibEnd(t.map(|t| path_value(t).to_ascii_lowercase())))
        .parse_next(input)
}
