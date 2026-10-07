//! `.save` and `.print` output-vector grammars.
//!
//! C: `ft_dotsaves()` and `ft_savedotargs()` in `src/frontend/dotcards.c` collect
//! the deck's `.save`/`.print` lines and hand each word to `com_save()` /
//! `com_save2()` (`src/frontend/breakp2.c`), which normalises it with
//! `copynode()` (`v(2)` → node `2`, `i(vds)` → the `vds#branch` vector) and
//! stores it in the `dbs` save list. The AC component spellings are `fixem()`
//! (`vm(a,b)` → `mag(v(a)-v(b))`, and `vp`/`vr`/`vi`/`vdb` likewise). `.save` is
//! deck-wide; `.print` names one analysis.
//!
//! The subset here is explicit and bounded. C accepts more words than the port
//! can observe, so everything outside the subset is a positioned failure rather
//! than a silently dropped request:
//!
//! - `v(node)`, `v(first,second)`, `i(device)` and the `vm`/`vp`/`vr`/`vi`/`vdb`
//!   voltage components, plus `all`;
//! - `i(<device>)` is accepted only for a voltage source or an inductor, the two
//!   branch currents the port's plots carry; anything else (a resistor, a
//!   nonlinear instance) is [`SpiceError::NotYetPorted`], never a synthesised
//!   value;
//! - `@instance[param]` names are [`SpiceError::NotYetPorted`]: the port has no
//!   observation API for instance parameters;
//! - `v(a,a)` and `v(0,0)` are identically zero and are rejected rather than
//!   written;
//! - a card with no requests, a missing `)`, a bare `v`, a third terminal and an
//!   unknown function word are parse errors naming the position.

use spice_core::{AnalysisKind, SourceLoc, SpiceError};
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::ast::{PrintCard, RequestedVector, SaveCard, VectorComponent, VectorRequest};
use crate::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result, keyword};
use super::syntax::canonical_node;
use super::vector::punctuation;

/// The C reference for the device currents the port cannot observe yet.
const C_REFERENCE_CURRENTS: &str =
    "src/frontend/outitf.c (beginPlot save set), src/spicelib/analysis/cktnames.c";

/// A parsed `.save` or `.print` card.
pub(super) enum OutputCard {
    /// A deck-wide `.save` card.
    Save(SaveCard),
    /// An analysis-specific `.print` card.
    Print(PrintCard),
}

pub(super) fn output_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    if opt(keyword(".save")).parse_next(input)?.is_some() {
        return cut_err(|input: &mut Input<'_>| {
            Ok(ParsedCard::Output(OutputCard::Save(SaveCard {
                requests: requests(input, ".save")?,
                location: input.state.card.location.clone(),
            })))
        })
        .parse_next(input);
    }
    keyword(".print").parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        let name = any
            .verify(|token: &Token| token.is_name_like())
            .context("an analysis name after .print")
            .parse_next(input)?;
        let Some(analysis) = AnalysisKind::parse(&name.text) else {
            return Err(fail(
                &name.location,
                format!(
                    "expected an analysis name after .print (op, dc, ac, tran, noise, disto, pz, \
                     sens, tf or four), found '{}'",
                    name.text
                ),
            ));
        };
        let requests = requests(input, ".print")?;
        Ok(ParsedCard::Output(OutputCard::Print(PrintCard {
            analysis,
            analysis_location: name.location.clone(),
            requests,
            location: input.state.card.location.clone(),
        })))
    })
    .parse_next(input)
}

fn fail(at: &SourceLoc, message: impl Into<String>) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(at.clone(), message)))
}

fn requests(input: &mut Input<'_>, card: &str) -> Result<Vec<VectorRequest>> {
    if input.input.is_empty() {
        return Err(fail(
            &input.state.card.location,
            format!("{card} needs at least one vector request: {SUPPORTED}"),
        ));
    }
    cut_err(repeat(1.., request)).parse_next(input)
}

/// The supported request spellings, quoted in diagnostics.
const SUPPORTED: &str =
    "all, v(node), v(first,second), i(source|inductor), vm/vp/vr/vi/vdb(node[,second])";

fn request(input: &mut Input<'_>) -> Result<VectorRequest> {
    // Backtracks at the end of the card so `repeat` stops; everything else cuts.
    peek(any).parse_next(input)?;
    let head = any.parse_next(input)?;
    let location = head.location.clone();
    if head.kind == TokenKind::Word && head.is_keyword("all") {
        return Ok(VectorRequest {
            vector: RequestedVector::All,
            location,
        });
    }
    if head.text.starts_with('@') {
        return Err(ErrMode::Cut(Failure(SpiceError::not_yet_ported(
            format!(
                "{}: {}: instance parameters are not observable",
                location, head.text
            ),
            C_REFERENCE_CURRENTS,
        ))));
    }
    if head.kind != TokenKind::Word {
        return Err(fail(
            &location,
            format!(
                "expected a vector request ({SUPPORTED}), found '{}'",
                head.text
            ),
        ));
    }
    let name = head.text.to_ascii_lowercase();
    if !matches!(name.as_str(), "v" | "i" | "vm" | "vp" | "vr" | "vi" | "vdb") {
        return Err(fail(
            &location,
            format!(
                "unknown vector request '{}'; supported: {SUPPORTED}",
                head.text
            ),
        ));
    }
    if !input
        .input
        .first()
        .is_some_and(|token| token.kind == TokenKind::LParen)
    {
        return Err(fail(
            &location,
            format!(
                "'{}' needs a parenthesised argument ({SUPPORTED})",
                head.text
            ),
        ));
    }
    any.parse_next(input)?;
    let (positive, negative) = terminals(input, &location, &name)?;
    if let Some(negative) = &negative
        && *negative == positive
    {
        return Err(fail(
            &location,
            format!(
                "{}({positive},{negative}) is identically zero; leave out the repeated node",
                name
            ),
        ));
    }
    let vector = match name.as_str() {
        "v" => RequestedVector::Voltage { positive, negative },
        "i" => {
            if negative.is_some() {
                return Err(fail(
                    &location,
                    format!("i({positive},...) takes one device name, not a difference"),
                ));
            }
            RequestedVector::Current { device: positive }
        }
        function => RequestedVector::Component {
            component: component(function),
            positive,
            negative,
        },
    };
    Ok(VectorRequest { vector, location })
}

/// One or two node names, then `)`.
fn terminals(
    input: &mut Input<'_>,
    at: &SourceLoc,
    function: &str,
) -> Result<(String, Option<String>)> {
    let first = node(input, at, function)?;
    let mut second = None;
    if input
        .input
        .first()
        .is_some_and(|token| token.kind == TokenKind::Comma)
    {
        any.parse_next(input)?;
        second = Some(node(input, at, function)?);
    }
    cut_err(punctuation(TokenKind::RParen).context("')' after the node name(s)"))
        .parse_next(input)?;
    Ok((first, second))
}

fn node(input: &mut Input<'_>, at: &SourceLoc, function: &str) -> Result<String> {
    let Some(token) = input.input.first() else {
        return Err(fail(at, format!("{function}(...) is missing a node name")));
    };
    if !token.is_name_like() {
        return Err(fail(
            &token.location,
            format!(
                "expected a node or device name in {function}(...), found '{}'",
                token.text
            ),
        ));
    }
    let token = any.parse_next(input)?;
    if function == "i" {
        return device(token);
    }
    Ok(canonical_node(token, input.state.auto_gnd))
}

/// A device name for `i(...)`: only a source or inductor branch current is
/// observable in this port.
fn device(token: &Token) -> Result<String> {
    let name = token.text.to_ascii_lowercase();
    match name.chars().next() {
        Some('v' | 'l') => Ok(name),
        Some(designator) if designator.is_ascii_alphabetic() => {
            Err(ErrMode::Cut(Failure(SpiceError::not_yet_ported(
                format!(
                    "{}: i({name}): only a voltage source or inductor branch current is observable; \
                     an '{designator}' instance current needs a device observation API",
                    token.location
                ),
                C_REFERENCE_CURRENTS,
            ))))
        }
        _ => Err(fail(
            &token.location,
            format!("expected a device name in i(...), found '{name}'"),
        )),
    }
}

fn component(function: &str) -> VectorComponent {
    match function {
        "vm" => VectorComponent::Magnitude,
        "vp" => VectorComponent::Phase,
        "vr" => VectorComponent::Real,
        "vi" => VectorComponent::Imaginary,
        _ => VectorComponent::Decibels,
    }
}
