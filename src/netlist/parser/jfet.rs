//! Three-terminal J-instance grammar from `inp2j.c` and `jfet/jfet.c`.
//!
//! `Jname nd ng ns model [area] [off] [ic=vds,vgs] [area=..] [m=..]
//! [ic-vds=..] [ic-vgs=..] [temp=..] [dtemp=..]`. INP2J always reads exactly
//! three terminals and then the model name (there is no terminal scan), and
//! applies an optional unlabeled leading area after the named setters, as
//! INP2D/INP2Q do. The `ic` vector fills `ic-vds` then `ic-vgs`
//! (`jfetpar.c`'s `JFET_IC` fallthrough). Model references are retained, not
//! resolved, at this syntax boundary; level 1/2 selection is elaboration.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, ParameterAssignment};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Input, ParsedCard, Result, gap};
use super::syntax::{
    assignment, canonical_node, equals, leading_value, malformed, name, named, value,
};

pub(super) fn jfet_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&b'j'))
        })
        .parse_next(input)?;
    let (drain, gate, source, model, parameters) = cut_err((
        name("drain terminal"),
        name("gate terminal"),
        name("source terminal"),
        name("JFET model name"),
        parameters,
    ))
    .parse_next(input)?;
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 'j',
        nodes: [drain, gate, source]
            .into_iter()
            .map(|node| canonical_node(node, input.state.auto_gnd))
            .collect(),
        model: Some(model.text.to_ascii_lowercase()),
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn parameters(input: &mut Input<'_>) -> Result<Vec<ParameterAssignment>> {
    let leading = opt(leading_value).parse_next(input)?;
    let mut parameters: Vec<ParameterAssignment> = repeat(
        0..,
        alt((
            super::flags::instance,
            super::ic::vector,
            scalar_assignment,
            invalid_parameter,
        )),
    )
    .parse_next(input)?;
    if let Some(area) = leading {
        // INP2J applies the leading area after INPdevParse.
        parameters.push(assignment("area", area));
    }
    Ok(parameters)
}

/// The scalar `IF_REAL` setters of `JFETpTable`.
fn scalar_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = any
        .verify(|token: &Token| {
            matches!(
                token.text.to_ascii_lowercase().as_str(),
                "area" | "m" | "ic-vds" | "ic-vgs" | "temp" | "dtemp"
            )
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    ))
}

fn invalid_parameter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = peek(any).parse_next(input)?;
    if matches!(
        token.kind,
        TokenKind::Equals | TokenKind::LParen | TokenKind::RParen | TokenKind::Comma
    ) {
        return Err(malformed(input, "expected a named scalar JFET parameter"));
    }
    Err(gap(
        input,
        "unsupported JFET flags, extra terminals or non-scalar parameters",
    ))
}
