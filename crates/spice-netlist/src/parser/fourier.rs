//! The `.four` Fourier card grammar.
//!
//! C: `inp_spsource()` (`src/frontend/inp.c`) filters the deck's `.four` lines
//! out into `ft_first`/`ft_last`; `ft_savedotargs()` (`src/frontend/dotcards.c`)
//! registers the named vectors for the `TRAN` plot so the transient keeps them,
//! and `ft_cktcoms()` hands each line to `fourier()`
//! (`src/frontend/fourier.c`), which transforms the last `nperiods/fundamental`
//! seconds of the `tran` plot. The port keeps the card's typed request beside
//! the netlist (`ParsedDeck::fourier`) and evaluates it over the **full** plot,
//! so a `.save`/`.print` selection never hides a transformed vector, and a
//! `.four` card never changes the written rawfile. See `docs/port/FOURIER.md`.
//!
//! The accepted subset is explicit and bounded:
//!
//! ```text
//! .four <fundamental-frequency> [HARMONICS=<n>] <vector> [<vector> …]
//! ```
//!
//! where `<fundamental-frequency>` is a finite, strictly positive literal
//! (`1k`, `2.5e3`), `<n>` is a whole number of harmonics in
//! `1..=`[`MAX_HARMONICS`], `<vector>` is the `.save`/`.print` spelling of one
//! vector (`v(node)`, `v(first,second)`, `i(source|inductor|E|H)`) without `all`,
//! and `HARMONICS=` may be written anywhere after the frequency. Without
//! `HARMONICS=` the card tabulates harmonics `1..=`[`DEFAULT_HARMONICS`].
//!
//! Everything else is a positioned failure rather than a dropped card:
//!
//! * every malformed or unknown word, a missing or non-positive fundamental, a
//!   non-whole or repeated `HARMONICS=`, a missing vector, `all` and an AC
//!   component spelling (`vm(out)`, …) are [`SpiceError::Parse`];
//! * `NFREQS=`/`NPERIODS=`/`POLYDEGREE=`/`FOURGRIDSIZE=` and a `{…}` value are
//!   [`SpiceError::NotYetPorted`]: C takes them from interactive `set`
//!   variables, and this port implements none of them;
//! * a harmonic count beyond this port's bounded resampling budget is
//!   [`SpiceError::Unsupported`], naming the budget rather than clamping it.

use spice_core::{AnalysisKind, Real, SourceLoc, SpiceError};
use winnow::Parser as _;
use winnow::combinator::cut_err;
use winnow::error::{ErrMode, ParserError as _};
use winnow::token::any;

use crate::ast::{DEFAULT_HARMONICS, FourierCard, MAX_HARMONICS, RequestedVector, VectorRequest};
use crate::card::DotCommand;
use crate::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result};
use super::save;

/// The C files this card's contract comes from, quoted in "not ported" errors.
const C_REFERENCE: &str = "src/frontend/inp.c (inp_spsource), src/frontend/dotcards.c \
     (ft_savedotargs, ft_cktcoms), src/frontend/fourier.c (fourier, CKTfour)";

/// The vector spellings a `.four` card accepts.
const VECTORS: &str = "v(node), v(first,second), i(source|inductor|E|H)";

/// The parameter spellings a `.four` card accepts.
const PARAMETERS: &str = "HARMONICS";

/// A `.four` card, dispatched by the card's own dot command so that the analysis
/// classification (`DotCommand::Analysis(AnalysisKind::Fourier)`) is the single
/// spelling of this directive.
pub(super) fn fourier_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    if input.state.card.dot_command() != Some(&DotCommand::Analysis(AnalysisKind::Fourier)) {
        return Err(ErrMode::Backtrack(Failure::from_input(input)));
    }
    any.parse_next(input)?;
    cut_err(card).parse_next(input)
}

fn fail(at: &SourceLoc, message: impl Into<String>) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(at.clone(), message)))
}

fn gap(at: &SourceLoc, message: impl Into<String>) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::not_yet_ported(
        format!("{at}: {}", message.into()),
        C_REFERENCE,
    )))
}

fn card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let location = input.state.card.location.clone();
    let (fundamental, fundamental_location) = fundamental(input, &location)?;
    let mut harmonics = DEFAULT_HARMONICS;
    let mut harmonics_location = None;
    let mut vectors: Vec<VectorRequest> = Vec::new();
    while !input.input.is_empty() {
        if parameter_ahead(input) {
            let token = any.parse_next(input)?;
            let name = token.text.to_ascii_lowercase();
            if name != "harmonics" {
                parameter(&name, token)?;
            }
            let value = harmonics_value(input, &token.location)?;
            if harmonics_location.is_some() {
                return Err(fail(&token.location, "HARMONICS= is given more than once"));
            }
            harmonics = value;
            harmonics_location = Some(token.location.clone());
            continue;
        }
        vectors.push(operand(input)?);
    }
    if vectors.is_empty() {
        return Err(fail(
            &location,
            format!(
                "expected at least one vector to transform: .four <frequency> \
                 [HARMONICS=<n>] <vector> [<vector> …], where <vector> is {VECTORS}"
            ),
        ));
    }
    Ok(ParsedCard::Fourier(FourierCard {
        fundamental,
        fundamental_location,
        harmonics,
        harmonics_location,
        vectors,
        location,
    }))
}

/// The fundamental frequency: the card's first argument.
fn fundamental(input: &mut Input<'_>, card: &SourceLoc) -> Result<(Real, SourceLoc)> {
    let Some(token) = input.input.first().cloned() else {
        return Err(fail(
            card,
            format!(
                "expected a fundamental frequency after .four: .four <frequency> [HARMONICS=<n>] <vector> ({VECTORS})"
            ),
        ));
    };
    match token.kind {
        TokenKind::Number(value) if value.is_finite() && value > 0.0 => {
            any.parse_next(input)?;
            Ok((value, token.location))
        }
        TokenKind::Number(_) => Err(fail(
            &token.location,
            format!(
                "the fundamental frequency must be a finite value greater than zero; C reports \
                 'bad fundamental freq' for {} (docs/port/FOURIER.md)",
                token.text
            ),
        )),
        _ if super::expression::is_expression_token(&token) => Err(gap(
            &token.location,
            "a {…} expression as the .four fundamental frequency",
        )),
        _ => Err(fail(
            &token.location,
            format!(
                "expected a fundamental frequency after .four (a finite value greater than zero), \
                 found '{}'",
                token.text
            ),
        )),
    }
}

/// True when the next two tokens are a `name=` parameter spelling.
fn parameter_ahead(input: &Input<'_>) -> bool {
    input
        .input
        .first()
        .is_some_and(|token: &Token| token.kind == TokenKind::Word)
        && input
            .input
            .get(1)
            .is_some_and(|token| token.kind == TokenKind::Equals)
}

/// A `name=` word this port does not accept as a `.four` parameter.
fn parameter(name: &str, at: &Token) -> Result<()> {
    match name {
        "nfreqs" | "nperiods" | "polydegree" | "fourgridsize" => Err(gap(
            &at.location,
            format!(
                "the {name}= .four parameter (C reads it from an interactive `set` variable; this \
                 port transforms the final complete period with linear interpolation and \
                 tabulates DC plus harmonics 1..=n)"
            ),
        )),
        _ => Err(fail(
            &at.location,
            format!(
                "no such .four parameter as '{}'; supported: {PARAMETERS}=<n>",
                at.text
            ),
        )),
    }
}

/// The value of `HARMONICS=`: a whole harmonic count inside this port's bounded
/// resampling budget.
fn harmonics_value(input: &mut Input<'_>, at: &SourceLoc) -> Result<u32> {
    any.parse_next(input)?;
    let Some(value) = input.input.first().cloned() else {
        return Err(fail(at, "HARMONICS= has no value: write HARMONICS=<n>"));
    };
    match value.kind {
        TokenKind::Number(number) if number.is_finite() => {
            any.parse_next(input)?;
            if number.fract() != 0.0 || number < 1.0 {
                return Err(fail(
                    &value.location,
                    format!(
                        "HARMONICS= takes a whole number of harmonics of at least 1 (found {})",
                        spice_core::format_spice_number(number)
                    ),
                ));
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let harmonics = number as u32;
            if harmonics > MAX_HARMONICS {
                return Err(ErrMode::Cut(Failure(SpiceError::Unsupported {
                    feature: format!(
                        "HARMONICS={harmonics}: this port resamples the period onto 4 * max(n, 50) \
                         subintervals per vector, so its bounded Fourier budget is {MAX_HARMONICS} \
                         harmonics ({MAX_GRID} grid subintervals); C has no such bound \
                         (docs/port/FOURIER.md)"
                    ),
                    location: Some(value.location),
                })));
            }
            Ok(harmonics)
        }
        _ if super::expression::is_expression_token(&value) => Err(gap(
            &value.location,
            "a {…} expression as the value of HARMONICS=",
        )),
        _ => Err(fail(
            &value.location,
            format!(
                "expected a whole number after 'HARMONICS=', found '{}'",
                value.text
            ),
        )),
    }
}

/// The widest resampling grid `MAX_HARMONICS` can ask for, quoted in the budget
/// error.
const MAX_GRID: usize = (4 * MAX_HARMONICS) as usize;

/// One vector to transform: the `.save` spelling without `all` and without an
/// AC component, which only a complex plot carries.
fn operand(input: &mut Input<'_>) -> Result<VectorRequest> {
    let request = save::request(input)?;
    let at = &request.location;
    match &request.vector {
        RequestedVector::All => Err(fail(
            at,
            format!("'all' cannot be transformed: name one vector ({VECTORS})"),
        )),
        RequestedVector::Component { component, .. } => Err(fail(
            at,
            format!(
                "{}(...) is an AC component, which needs a complex plot; .four transforms a \
                 transient result ({VECTORS})",
                component.function()
            ),
        )),
        RequestedVector::Voltage { .. } | RequestedVector::Current { .. } => Ok(request),
    }
}
