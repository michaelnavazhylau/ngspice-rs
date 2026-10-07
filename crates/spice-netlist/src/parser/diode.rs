//! Two-terminal D-instance grammar from `inp2d.c` and `dio/dio.c`.
//!
//! C applies an optional leading area after named assignments. Third/thermal
//! terminals, thermal/sensitivity flags, CIDER variants and parameter expressions
//! remain explicit gaps; bare OFF and scalar IC are syntax only. Model references are retained, not resolved, at this syntax boundary.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::token::any;

use crate::ast::{DeviceInstance, ParameterAssignment};
use crate::token::{Token, TokenKind};

use super::grammar::{Input, ParsedCard, Result, gap};
use super::syntax::{
    assignment, canonical_node, equals, leading_value, malformed, name, named, value,
};

pub(super) fn diode_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&b'd'))
        })
        .parse_next(input)?;
    let (anode, cathode, model, parameters) = cut_err((
        name("anode terminal"),
        name("cathode terminal"),
        name("diode model name"),
        parameters,
    ))
    .parse_next(input)?;
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 'd',
        nodes: vec![
            canonical_node(anode, input.state.auto_gnd),
            canonical_node(cathode, input.state.auto_gnd),
        ],
        model: Some(model.text.to_ascii_lowercase()),
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let leading = opt(leading_value).parse_next(input)?;
    let mut parameters: Vec<ParameterAssignment> = repeat(
        0..,
        alt((super::flags::instance, scalar_assignment, invalid_parameter)),
    )
    .parse_next(input)?;
    if let Some(area) = leading {
        parameters.push(assignment("area", area));
    }
    Ok(parameters)
}

fn scalar_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let (token, name) = any
        .verify_map(|token: &Token| {
            let name = token.text.to_ascii_lowercase();
            let canonical = match name.as_str() {
                "perim" => "pj",
                "area" | "pj" | "w" | "l" | "m" | "ic" | "temp" | "dtemp" | "lm" | "lp" | "wm"
                | "wp" => &name,
                _ => return None,
            };
            Some((token, canonical.to_owned()))
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(&name, token.location.clone(), value))
}

fn invalid_parameter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = peek(any).parse_next(input)?;
    if matches!(
        token.kind,
        TokenKind::Equals | TokenKind::LParen | TokenKind::RParen | TokenKind::Comma
    ) {
        return Err(malformed(input, "expected a scalar diode parameter"));
    }
    Err(gap(
        input,
        "unsupported diode flags, extra terminals or non-scalar parameters",
    ))
}
