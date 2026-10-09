//! Mutual-inductance grammar: `src/spicelib/parser/inp2k.c` and the
//! multi-inductor rewrite in `inp_compat()` (`src/frontend/inpcom.c`).
//!
//! ```text
//! Kname L1 L2 [L3 ...] coupling
//! coupling := value | k=value | coefficient=value
//! ```
//!
//! `INP2K` reads two `IF_INSTANCE` inductor names (`inductor1`, `inductor2`)
//! and then hands the rest of the card to `INPdevParse`, which accepts a
//! leading value (stored as `coefficient`) or the named `k`/`coefficient`
//! setters (`MUT_COEFF`, `ind/ind.c`). Before that, `inp_compat()` rewrites a
//! card with more than two inductors, `K1 L1 L2 L3 c`, into one card per
//! pair (`k1_1_2 l1 l2 c`, `k1_1_3 l1 l3 c`, `k1_2_3 l2 l3 c`): every
//! whitespace-separated word except the instance name and the **last** one is
//! an inductor name, and the last word is the coupling of every pair.
//!
//! The AST keeps the card as written: inductor references are
//! [`ParameterKind::Instance`] setters `inductor1`, `inductor2`, … (renamed by
//! subcircuit expansion like any instance name, as `translate()` in
//! `subckt.c` does for K cards), followed by exactly one `coefficient` setter
//! (positional values are located at the value, named ones at the keyword).
//! The pairwise expansion happens in device elaboration.
//!
//! Rejected although C accepts them, as documented divergences:
//!
//! - A card without a coupling value (`K1 L1 L2`): C silently builds a
//!   coupling of zero (`MUTcoupling` stays at its calloc default).
//! - Anything after the coupling, or two couplings: C either treats the extra
//!   words as inductor names (and fails to find them) or reports an unknown
//!   parameter, so the port reports a parse error at the extra token.
//!
//! A card naming fewer than two inductors is an error, as in C (which looks
//! for an inductor named after the coupling value and fails).

use crate::primitives::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::{DeviceInstance, ParameterAssignment, ParameterKind};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, location};
use super::syntax::{assignment, equals, leading_value, name, named, value};

pub(super) fn mutual_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| {
            token.is_name_like()
                && token
                    .text
                    .chars()
                    .next()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'k'))
        })
        .parse_next(input)?;
    // The designator matched: every failure below is definitive.
    cut_err(move |input: &mut Input<'_>| body(input, instance)).parse_next(input)
}

fn body(input: &mut Input<'_>, instance: &Token) -> Result<ParsedCard> {
    let mut parameters: Vec<ParameterAssignment> = Vec::new();
    let coupling = loop {
        if input.input.is_empty() {
            return Err(parse_error(
                input,
                if parameters.len() < 2 {
                    "expected an inductor name (a K card couples at least two inductors)"
                } else {
                    "expected the coupling coefficient after the inductor names \
                     (C silently uses 0 when it is missing)"
                },
            ));
        }
        if peek_coupling_setter(input) {
            let (keyword, _, value) = (any, equals, value).parse_next(input)?;
            break named("coefficient", keyword.location.clone(), value);
        }
        if let Some(value) = opt(leading_value).parse_next(input)? {
            break assignment("coefficient", value);
        }
        let inductor = name("an inductor name").parse_next(input)?;
        parameters.push(ParameterAssignment {
            name: format!("inductor{}", parameters.len() + 1),
            value: inductor.text.to_ascii_lowercase(),
            kind: ParameterKind::Instance,
            location: inductor.location.clone(),
        });
    };
    if parameters.len() < 2 {
        return Err(ErrMode::Cut(Failure(SpiceError::parse(
            coupling.location.clone(),
            "a K card couples at least two inductors before its coupling coefficient",
        ))));
    }
    if !input.input.is_empty() {
        return Err(parse_error(
            input,
            "unexpected token after the coupling coefficient (the coupling must be the \
             last word of a K card)",
        ));
    }
    parameters.push(coupling);
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator: 'k',
        nodes: Vec::new(),
        model: None,
        parameters,
        location: input.state.card.location.clone(),
    }))
}

/// `k=` or `coefficient=` (both `MUT_COEFF` in `ind/ind.c`).
fn peek_coupling_setter(input: &Input<'_>) -> bool {
    input
        .input
        .first()
        .is_some_and(|t| t.is_keyword("k") || t.is_keyword("coefficient"))
        && input
            .input
            .get(1)
            .is_some_and(|t| t.kind == TokenKind::Equals)
}

fn parse_error(input: &Input<'_>, message: &str) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(location(input), message)))
}
