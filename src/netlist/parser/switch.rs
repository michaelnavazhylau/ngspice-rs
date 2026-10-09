//! Switch instance grammars: `src/spicelib/parser/inp2s.c` and `inp2w.c`.
//!
//! ```text
//! Sname n+ n- nc+ nc- model [on|off]...   voltage-controlled switch (sw/sw.c)
//! Wname n+ n- vname   model [on|off]...   current-controlled switch (csw/csw.c)
//! ```
//!
//! The model name is required: C reports "Unable to find definition of model"
//! and abandons the simulation when it is missing or undeclared. `on`/`off` are
//! the `IF_FLAG` instance setters of `SWpTable`/`CSWpTable`, kept in written
//! order (the last one wins, as `SWparam`/`CSWparam` apply them in turn). W's
//! controlling voltage source is stored first as `control`
//! ([`ParameterKind::Instance`]), as `INP2W` sets it before `INPdevParse`, and is
//! resolved to a branch row only after elaboration.
//!
//! Explicit gaps: C silently ignores a leading number after the model
//! (`waslead`, "ignore a number"); the port refuses it instead. Any other
//! instance parameter, `flag=value` forms and parenthesized node lists are
//! rejected rather than guessed.

use crate::primitives::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{cut_err, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, ParameterAssignment, ParameterKind};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, location};
use super::syntax::{canonical_node, malformed, name};

pub(super) fn switch_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| token.is_name_like() && switch_designator(token).is_some())
        .parse_next(input)?;
    let designator = switch_designator(instance).expect("verified designator");
    cut_err(move |input: &mut Input<'_>| body(input, instance, designator)).parse_next(input)
}

fn switch_designator(token: &Token) -> Option<char> {
    let designator = token.text.chars().next()?.to_ascii_lowercase();
    matches!(designator, 's' | 'w').then_some(designator)
}

fn body(input: &mut Input<'_>, instance: &Token, designator: char) -> Result<ParsedCard> {
    let auto_gnd = input.state.auto_gnd;
    let positive = name("positive switch terminal").parse_next(input)?;
    let negative = name("negative switch terminal").parse_next(input)?;
    let mut nodes = vec![
        canonical_node(positive, auto_gnd),
        canonical_node(negative, auto_gnd),
    ];
    let mut parameters = Vec::new();
    if designator == 's' {
        let control_positive = name("positive controlling node").parse_next(input)?;
        let control_negative = name("negative controlling node").parse_next(input)?;
        nodes.push(canonical_node(control_positive, auto_gnd));
        nodes.push(canonical_node(control_negative, auto_gnd));
    } else {
        let control = name("controlling voltage source name").parse_next(input)?;
        parameters.push(ParameterAssignment {
            name: "control".to_owned(),
            value: control.text.to_ascii_lowercase(),
            kind: ParameterKind::Instance,
            location: control.location.clone(),
        });
    }
    if input.input.is_empty() {
        return Err(malformed(
            input,
            "expected the switch model name (C: unable to find definition of model)",
        ));
    }
    let model = name("switch model name").parse_next(input)?;
    let flags: Vec<ParameterAssignment> = repeat(0.., state_flag).parse_next(input)?;
    if !input.input.is_empty() {
        return Err(trailing(input, designator));
    }
    parameters.extend(flags);
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator,
        nodes,
        model: Some(model.text.to_ascii_lowercase()),
        parameters,
        location: input.state.card.location.clone(),
    }))
}

/// A bare `on`/`off` initial-state flag (`IF_FLAG`, value 1 without consuming
/// a token, as `INPgetValue` supplies it).
fn state_flag(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = any
        .verify(|token: &Token| {
            token.kind == TokenKind::Word && (token.is_keyword("on") || token.is_keyword("off"))
        })
        .parse_next(input)?;
    if input
        .input
        .first()
        .is_some_and(|next| next.kind == TokenKind::Equals)
    {
        return Err(malformed(
            input,
            "switch on/off is a bare flag and does not take a value",
        ));
    }
    Ok(ParameterAssignment {
        name: token.text.to_ascii_lowercase(),
        value: String::new(),
        kind: ParameterKind::Flag,
        location: token.location.clone(),
    })
}

fn trailing(input: &Input<'_>, designator: char) -> ErrMode<Failure> {
    let token = &input.input[0];
    let at = location(input);
    let error = if token.number().is_some() {
        SpiceError::Unsupported {
            feature: format!(
                "a leading value after the '{designator}' switch model (C silently ignores it)"
            ),
            location: Some(at),
        }
    } else {
        SpiceError::parse(
            at,
            format!(
                "unexpected '{}' on a '{designator}' switch (only bare on/off flags follow the \
                 model)",
                token.text
            ),
        )
    };
    ErrMode::Cut(Failure(error))
}
