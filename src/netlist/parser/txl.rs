//! `inp2y.c`: four terminals, a model, and the optional instance `len`.
//! The Padé device binds only the first and third terminals; the fallback
//! capacitors use the second terminal. CPL is explicitly outside this port.
use super::grammar::{Input, ParsedCard, Result};
use super::syntax::{canonical_node, equals, malformed, name, named, value};
use crate::netlist::ast::{DeviceInstance, ParameterAssignment};
use crate::netlist::token::Token;
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, repeat};
use winnow::token::any;

pub(super) fn txl_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|t: &Token| {
            t.is_name_like()
                && t.text
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&b'y'))
        })
        .parse_next(input)?;
    cut_err(move |input: &mut Input<'_>| {
        let mut nodes = Vec::new();
        for what in [
            "TXL input",
            "TXL input reference",
            "TXL output",
            "TXL output reference",
        ] {
            let node = name(what).parse_next(input)?;
            nodes.push(canonical_node(node, input.state.auto_gnd));
        }
        let model = name("TXL model").parse_next(input)?;
        let parameters: Vec<ParameterAssignment> = repeat(0.., setter).parse_next(input)?;
        if !input.input.is_empty() {
            return Err(malformed(
                input,
                "TXL accepts only len=<value> after the model (inp2y.c); length is a model setter",
            ));
        }
        Ok(ParsedCard::Device(DeviceInstance {
            name: instance.text.to_ascii_lowercase(),
            designator: 'y',
            nodes,
            model: Some(model.text.to_ascii_lowercase()),
            parameters,
            location: input.state.card.location.clone(),
        }))
    })
    .parse_next(input)
}
fn setter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = any
        .verify(|t: &Token| t.is_keyword("len"))
        .parse_next(input)?;
    let (_, v) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named("len", token.location.clone(), v))
}
