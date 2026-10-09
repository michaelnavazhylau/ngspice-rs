//! `.ic` and `.nodeset` grammars: `V(node)=value ...`.
//!
//! C: `INPpas3()` in `src/spicelib/parser/inppas3.c` loops over the card,
//! accepting `V(name)` followed by an optional `=` and a value, and reports
//! ` Error: .ic syntax error.` for anything else; `.nodeset` additionally
//! accepts `all = value`. `inp2dot.c` ignores both cards in pass 2.
//!
//! Where C is permissive or silent this grammar is explicit:
//!
//! - `I(...)`, bare names, `V(a,b)` differentials, a missing node, a missing
//!   value, a non-finite value and a trailing `=` without value are errors.
//! - Ground (`0`, and `gnd` under auto-gnd) is rejected: an IC on the reference
//!   node is meaningless and C would silently store it on node 0.
//! - `.nodeset all=value` is [`SpiceError::NotYetPorted`].
//! - A card with no entries is an error (C silently accepts it).
//! - Unknown nodes cannot be detected without a circuit; C warns and ignores
//!   them, which the elaboration/analysis half must turn into an error.
//!
//! Values are finite literals or braced expressions (evaluated later against
//! `.param` cards by `elaborate::literalize`); bare parameter names are not
//! references, as in device cards. The `=` is optional, as in C.

use crate::primitives::{SourceLoc, SpiceError};
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::netlist::ast::{NodeHint, NodeHintCard, NodeHintValue};
use crate::netlist::token::TokenKind;

use super::grammar::{Failure, Input, ParsedCard, Result, keyword, location};
use super::syntax::{canonical_node, equals};
use super::vector::punctuation;

pub(super) fn hint_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let token = winnow::combinator::alt((keyword(".ic"), keyword(".nodeset"))).parse_next(input)?;
    let nodeset = token.is_keyword(".nodeset");
    let card_location = input.state.card.location.clone();
    if input.input.is_empty() {
        return Err(fail(
            &location(input),
            format!(
                "expected at least one V(node)=value entry after {}",
                token.text
            ),
        ));
    }
    let entries = cut_err(repeat(1.., move |input: &mut Input<'_>| {
        entry(input, nodeset)
    }))
    .parse_next(input)?;
    let card = NodeHintCard {
        entries,
        location: card_location,
    };
    Ok(if nodeset {
        ParsedCard::Nodeset(card)
    } else {
        ParsedCard::InitialCondition(card)
    })
}

fn fail(at: &SourceLoc, message: impl Into<String>) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(at.clone(), message)))
}

fn entry(input: &mut Input<'_>, nodeset: bool) -> Result<NodeHint> {
    // Backtracks at end of card so `repeat` stops; everything else is a cut.
    peek(any).parse_next(input)?;
    let head = any.parse_next(input)?;
    if !(head.kind == TokenKind::Word && head.is_keyword("v")) {
        let next_is_equals = input
            .input
            .first()
            .is_some_and(|t| t.kind == TokenKind::Equals);
        if nodeset && head.is_keyword("all") && next_is_equals {
            return Err(ErrMode::Cut(Failure(SpiceError::not_yet_ported(
                format!("{}: .nodeset all=value", head.location),
                "src/spicelib/parser/inppas3.c",
            ))));
        }
        return Err(fail(
            &head.location,
            format!(
                "expected V(node)=value, found '{}' (only voltage entries are accepted)",
                head.text
            ),
        ));
    }
    let (node, node_location) = node(input)?;
    opt(equals).parse_next(input)?;
    let (value, value_location) = value(input)?;
    Ok(NodeHint {
        node,
        node_location,
        value,
        value_location,
        location: head.location.clone(),
    })
}

fn node(input: &mut Input<'_>) -> Result<(String, SourceLoc)> {
    cut_err(punctuation(TokenKind::LParen).context("'(' after V")).parse_next(input)?;
    let Some(token) = input.input.first() else {
        return Err(fail(&location(input), "missing node name in V(...)"));
    };
    if !token.is_name_like() {
        return Err(fail(
            &token.location,
            format!("expected a node name in V(...), found '{}'", token.text),
        ));
    }
    let token = any.parse_next(input)?;
    let name = canonical_node(token, input.state.auto_gnd);
    if name == "0" {
        return Err(fail(
            &token.location,
            "an initial condition or nodeset on the ground node is not allowed",
        ));
    }
    if let Some(next) = input.input.first()
        && next.kind == TokenKind::Comma
    {
        {
            return Err(fail(
                &next.location,
                "differential V(a,b) is not accepted; give a single node",
            ));
        }
    }
    cut_err(punctuation(TokenKind::RParen).context("')' after the node name")).parse_next(input)?;
    Ok((name, token.location.clone()))
}

fn value(input: &mut Input<'_>) -> Result<(NodeHintValue, SourceLoc)> {
    let Some(token) = input.input.first() else {
        return Err(fail(&location(input), "missing value after V(node)="));
    };
    let value = match &token.kind {
        _ if super::expression::is_expression_token(token) => {
            let expression = super::expression::from_brace_token(token)
                .map_err(|error| ErrMode::Cut(Failure(error)))?;
            NodeHintValue::Expression(Box::new(expression))
        }
        TokenKind::Number(v) if v.is_finite() => NodeHintValue::Literal {
            text: token.text.clone(),
            value: *v,
        },
        TokenKind::Number(_) => {
            return Err(fail(
                &token.location,
                format!("value '{}' is not finite", token.text),
            ));
        }
        _ => {
            return Err(fail(
                &token.location,
                format!(
                    "expected a finite numeric literal or {{expression}} value, found '{}'",
                    token.text
                ),
            ));
        }
    };
    let token = any.parse_next(input)?;
    Ok((value, token.location.clone()))
}
