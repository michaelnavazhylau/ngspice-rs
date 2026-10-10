//! The `.measure`/`.meas` card grammar.
//!
//! C: `inp_spsource()` (`src/frontend/inp.c`) pulls the deck's `.measure` lines
//! out into `ft_curckt->ci_meas`; after the run, `do_measure()`
//! (`src/frontend/measure.c`) hands each line to `get_measure2()`
//! (`src/frontend/com_measure2.c`). The port keeps the card's typed request
//! beside the netlist (`ParsedDeck::measurements`) and evaluates it over the
//! **full** plot, so a `.save`/`.print` selection never hides a measurable
//! vector. See `docs/port/MEASURE.md`.
//!
//! The accepted subset is explicit and bounded:
//!
//! ```text
//! .measure <analysis> <name> FIND <operand> AT=<value>            [FROM=<value>] [TO=<value>]
//! .measure <analysis> <name> MIN|MAX|AVG|RMS|INTEG|INTEGRAL <operand>
//!                                                                  [FROM=<value>] [TO=<value>]
//! .measure <analysis> <name> TRIG <event> TARG <event>
//! ```
//!
//! where `<analysis>` is `tran`, `ac` or `dc`, and `<event>` is either
//! `AT=<value>` or `<operand> VAL=<value> [RISE=<n>|FALL=<n>|CROSS=<n>|LAST]`,
//! and `<operand>` is the `.save`/`.print` vector spelling (`v(node)`,
//! `v(first,second)`, `i(source|inductor|E|H)`, `vm/vp/vr/vi/vdb(node[,second])`)
//! without `all`.
//!
//! Everything else is a positioned failure rather than a dropped card: the
//! operation words C implements but the port does not (`WHEN`, `MIN_AT`,
//! `MAX_AT`, `PP`, `DERIV`, `ERR*`, the margin measurements) are
//! [`SpiceError::NotYetPorted`], `TD=` is [`SpiceError::NotYetPorted`], and
//! malformed or unknown words are [`SpiceError::Parse`]. `sp`, an analysis
//! the port runs and C measures but whose measurement axis is not ported, is
//! [`SpiceError::NotYetPorted`]; an analysis the port runs but no measurement
//! can be taken on (`op`, `noise`, …) is [`SpiceError::Unsupported`].

use crate::primitives::{AnalysisKind, Real, SourceLoc, SpiceError};
use winnow::Parser as _;
use winnow::combinator::cut_err;
use winnow::error::{ErrMode, ParserError as _};
use winnow::token::any;

use crate::netlist::ast::{
    MeasureCard, MeasureEvent, MeasureRequest, MeasureStatistic, MeasureTransition, MeasureWindow,
    RequestedVector, VectorRequest,
};
use crate::netlist::card::DotCommand;
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Failure, Input, ParsedCard, Result};
use super::save;

/// The C files this card's contract comes from, quoted in "not ported" errors.
const C_REFERENCE: &str = "src/frontend/inp.c (inp_spsource), src/frontend/measure.c (do_measure), \
     src/frontend/com_measure2.c (get_measure2)";

/// The vector spellings an operand accepts.
const OPERANDS: &str =
    "v(node), v(first,second), i(source|inductor|E|H), vm/vp/vr/vi/vdb(node[,second])";

/// The parameter spellings a `.measure` request accepts.
const PARAMETERS: &str = "AT, VAL, RISE, FALL, CROSS, LAST, FROM, TO";

fn fail(at: &SourceLoc, message: impl Into<String>) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(at.clone(), message)))
}

fn gap(at: &SourceLoc, message: impl Into<String>) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::not_yet_ported(
        format!("{at}: {}", message.into()),
        C_REFERENCE,
    )))
}

/// A `.measure`/`.meas` card, dispatched by the card's own dot command so that
/// the bounded spelling (`.meas`) is accepted exactly as `DotCommand::parse`
/// classifies it.
pub(super) fn measure_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    if input.state.card.dot_command() != Some(&DotCommand::Measure) {
        return Err(ErrMode::Backtrack(Failure::from_input(input)));
    }
    any.parse_next(input)?;
    cut_err(card).parse_next(input)
}

fn card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let location = input.state.card.location.clone();
    let (analysis, analysis_location) = analysis(input)?;
    let Some(name) = input.input.first().cloned() else {
        return Err(fail(
            &analysis_location,
            format!(
                "expected a result name after the analysis on this .measure card: \
                 .measure {} <name> <operation> …",
                analysis.as_str()
            ),
        ));
    };
    if name.kind != TokenKind::Word {
        return Err(fail(
            &name.location,
            format!(
                "expected a result name after the analysis, found '{}'",
                name.text
            ),
        ));
    }
    any.parse_next(input)?;
    let name_location = name.location.clone();
    let operation = operation(input, &location)?;
    let request = match operation {
        Operation::Find => find(input, &location)?,
        Operation::Statistic(statistic) => statistic_request(input, statistic)?,
        Operation::Trig => trig_targ(input, &location)?,
    };
    Ok(ParsedCard::Measure(MeasureCard {
        analysis,
        analysis_location,
        name: name.text,
        name_location,
        request,
        location,
    }))
}

/// The analysis word: `tran`, `ac` or `dc`, with the port's classifications for
/// everything else (see the module docs).
fn analysis(input: &mut Input<'_>) -> Result<(AnalysisKind, SourceLoc)> {
    let location = input.state.card.location.clone();
    let Some(token) = input.input.first().cloned() else {
        return Err(fail(
            &location,
            "expected an analysis name after .measure (tran, ac or dc), then a result name",
        ));
    };
    if token.kind != TokenKind::Word {
        return Err(fail(
            &token.location,
            format!(
                "expected an analysis name after .measure (tran, ac or dc), found '{}'",
                token.text
            ),
        ));
    }
    any.parse_next(input)?;
    let spelling = token.text.to_ascii_lowercase();
    match AnalysisKind::parse(&spelling) {
        Some(AnalysisKind::Transient) => Ok((AnalysisKind::Transient, token.location)),
        Some(AnalysisKind::Ac) => Ok((AnalysisKind::Ac, token.location)),
        Some(AnalysisKind::DcSweep) => Ok((AnalysisKind::DcSweep, token.location)),
        Some(AnalysisKind::SParameter) => Ok((AnalysisKind::SParameter, token.location)),
        Some(kind) => Err(ErrMode::Cut(Failure(SpiceError::Unsupported {
            feature: format!(
                ".measure {}: C measures tran, dc, sp and ac only, and a .{} result has no \
                 measurement axis (docs/port/MEASURE.md)",
                kind.as_str(),
                kind.as_str()
            ),
            location: Some(token.location),
        }))),
        None if spelling == "sparam" => Ok((AnalysisKind::SParameter, token.location)),
        None => Err(fail(
            &token.location,
            format!(
                "expected an analysis name after .measure (tran, ac or dc), found '{}'",
                token.text
            ),
        )),
    }
}

/// The operation word after the result name.
enum Operation {
    Find,
    Statistic(MeasureStatistic),
    Trig,
}

/// C's `measure_function_type()` (`com_measure2.c`).
fn operation(input: &mut Input<'_>, card: &SourceLoc) -> Result<Operation> {
    let Some(token) = input.input.first().cloned() else {
        return Err(fail(
            card,
            "expected an operation after the result name (FIND, MIN, MAX, AVG, RMS, INTEG or TRIG)",
        ));
    };
    if token.kind != TokenKind::Word {
        return Err(fail(
            &token.location,
            format!(
                "expected an operation after the result name (FIND, MIN, MAX, AVG, RMS, INTEG or \
                 TRIG), found '{}'",
                token.text
            ),
        ));
    }
    any.parse_next(input)?;
    let word = token.text.to_ascii_lowercase();
    let operation = match word.as_str() {
        "find" => Operation::Find,
        "min" => Operation::Statistic(MeasureStatistic::Min),
        "max" => Operation::Statistic(MeasureStatistic::Max),
        "avg" => Operation::Statistic(MeasureStatistic::Avg),
        "rms" => Operation::Statistic(MeasureStatistic::Rms),
        "integ" | "integral" => Operation::Statistic(MeasureStatistic::Integ),
        "trig" => Operation::Trig,
        "when" | "min_at" | "max_at" | "pp" | "deriv" | "derivative" | "err" | "err1" | "err2"
        | "err3" | "phase_margin" | "phasemargin" | "gain_margin" | "gainmargin" => {
            return Err(gap(
                &token.location,
                format!(
                    "the {word} measurement (supported operations: FIND, MIN, MAX, AVG, RMS, \
                     INTEG, TRIG)"
                ),
            ));
        }
        _ => {
            return Err(fail(
                &token.location,
                format!(
                    "no such measurement as '{}' (supported: FIND, MIN, MAX, AVG, RMS, INTEG, TRIG)",
                    token.text
                ),
            ));
        }
    };
    Ok(operation)
}

/// `FIND <operand> AT=<value> [FROM=<value>] [TO=<value>]`.
fn find(input: &mut Input<'_>, card: &SourceLoc) -> Result<MeasureRequest> {
    let operand = operand(input, "FIND")?;
    let mut setters = Setters::default();
    parameters(input, &mut setters, false)?;
    if let Some((_, at)) = setters.val() {
        return Err(fail(
            &at,
            "VAL= is not a FIND parameter: FIND <vector> AT=<value> [FROM=…] [TO=…]",
        ));
    }
    if let Some((_, selector)) = setters.transition() {
        return Err(fail(
            &selector,
            "a RISE=/FALL=/CROSS=/LAST selector is not accepted by FIND: it describes an \
             event, not a query point (FIND <vector> AT=<value> [FROM=…] [TO=…])",
        ));
    }
    let Some((at, at_location)) = setters.at() else {
        return Err(fail(
            card,
            "FIND needs AT=<value>: FIND <vector> AT=<value> [FROM=<value>] [TO=<value>]",
        ));
    };
    Ok(MeasureRequest::Find {
        operand,
        at,
        at_location,
        window: setters.window(),
    })
}

/// `MIN|MAX|AVG|RMS|INTEG <operand> [FROM=<value>] [TO=<value>]`.
fn statistic_request(input: &mut Input<'_>, statistic: MeasureStatistic) -> Result<MeasureRequest> {
    let operand = operand(input, statistic.name())?;
    let mut setters = Setters::default();
    parameters(input, &mut setters, false)?;
    let unsupported = setters.at().or_else(|| setters.val()).map(|(_, at)| at);
    if let Some(at) = unsupported {
        return Err(fail(
            &at,
            format!(
                "AT=/VAL= are not {} parameters: {} <vector> [FROM=<value>] [TO=<value>]",
                statistic.name(),
                statistic.name()
            ),
        ));
    }
    if let Some((_, selector)) = setters.transition() {
        return Err(fail(
            &selector,
            format!(
                "a RISE=/FALL=/CROSS=/LAST selector is not accepted by {}: {} <vector> \
                 [FROM=<value>] [TO=<value>]",
                statistic.name(),
                statistic.name()
            ),
        ));
    }
    Ok(MeasureRequest::Statistic {
        statistic,
        operand,
        window: setters.window(),
    })
}

/// `TRIG <event> TARG <event>`, where the axis distance is reported.
fn trig_targ(input: &mut Input<'_>, card: &SourceLoc) -> Result<MeasureRequest> {
    let (trig, trig_window) = event(input, "TRIG", true)?;
    if !next_is(input, "targ") {
        return Err(fail(
            card,
            "TRIG needs a TARG clause: TRIG <vector> VAL=<value> [RISE|FALL|CROSS=<n>|LAST] \
             TARG <vector> VAL=<value> […]",
        ));
    }
    any.parse_next(input)?;
    let (targ, targ_window) = event(input, "TARG", false)?;
    let window = merge(trig_window, targ_window, card)?;
    Ok(MeasureRequest::TrigTarg { trig, targ, window })
}

/// One `TRIG`/`TARG` clause: `AT=<value>` or
/// `<operand> VAL=<value> [RISE|FALL|CROSS=<n>|LAST] [FROM=…] [TO=…]`.
fn event(
    input: &mut Input<'_>,
    clause: &str,
    stop_at_targ: bool,
) -> Result<(MeasureEvent, MeasureWindow)> {
    let card = input.state.card.location.clone();
    let mut setters = Setters::default();
    let at_only = next_is(input, "at");
    let operand = if at_only {
        None
    } else {
        Some(operand(input, clause)?)
    };
    parameters(input, &mut setters, stop_at_targ)?;
    let (at_setter, val_setter) = (setters.at(), setters.val());
    let event = match (operand, at_setter, val_setter) {
        (None, Some((at, location)), None) => MeasureEvent::At { at, location },
        (Some(_), Some((_, at)), _) => {
            return Err(fail(
                &at,
                format!("{clause} takes AT=<value> or <vector> VAL=<value>, not both"),
            ));
        }
        (Some(operand), None, Some((value, value_location))) => MeasureEvent::Crossing {
            operand,
            value,
            value_location,
            transition: setters
                .transition()
                .map_or(MeasureTransition::First, |(transition, _)| transition),
        },
        (Some(_), None, None) => {
            return Err(fail(
                &card,
                format!("{clause} needs AT=<value> or VAL=<value> (see docs/port/MEASURE.md)"),
            ));
        }
        (None, None, _) => {
            return Err(fail(
                &card,
                format!("{clause} needs AT=<value> or <vector> VAL=<value>"),
            ));
        }
        (None, Some(_), Some((_, at))) => {
            return Err(fail(
                &at,
                format!("{clause} takes AT=<value> or <vector> VAL=<value>, not both"),
            ));
        }
    };
    if let Some((_, at)) = setters.transition()
        && operand_is_at(&event)
    {
        return Err(fail(
            &at,
            format!("{clause} AT=<value> takes no RISE/FALL/CROSS/LAST selector"),
        ));
    }
    Ok((event, setters.window()))
}

fn operand_is_at(event: &MeasureEvent) -> bool {
    matches!(event, MeasureEvent::At { .. })
}

/// The operand of a measurement: the `.save`/`.print` vector spelling, without
/// `all`.
fn operand(input: &mut Input<'_>, clause: &str) -> Result<VectorRequest> {
    if input.input.is_empty() {
        return Err(fail(
            &input.state.card.location,
            format!("{clause} needs a vector operand: {OPERANDS}"),
        ));
    }
    let request = save::request(input)?;
    if request.vector == RequestedVector::All {
        return Err(fail(
            &request.location,
            format!("'all' cannot be measured: name one vector ({OPERANDS})"),
        ));
    }
    Ok(request)
}

/// The optional parameter setters of one clause. `stop_at_targ` stops before
/// the `TARG` word of a `TRIG` clause.
fn parameters(input: &mut Input<'_>, setters: &mut Setters, stop_at_targ: bool) -> Result<()> {
    while let Some(token) = input.input.first().cloned() {
        if stop_at_targ && token.kind == TokenKind::Word && token.is_keyword("targ") {
            break;
        }
        if token.kind != TokenKind::Word {
            return Err(fail(
                &token.location,
                format!(
                    "expected a .measure parameter ({PARAMETERS}), found '{}'",
                    token.text
                ),
            ));
        }
        any.parse_next(input)?;
        let name = token.text.to_ascii_lowercase();
        // C accepts a bare `LAST` (`measure_parse_stdParams()`), with no `=`.
        if name == "last" {
            setters.set_transition(MeasureTransition::Last, token.location.clone())?;
            continue;
        }
        match name.as_str() {
            "when" => {
                return Err(gap(
                    &token.location,
                    "the WHEN form (this port measures FIND <vector> AT=<value> and TRIG/TARG \
                     crossings)",
                ));
            }
            "rise" | "fall" | "cross" => {
                let transition = transition(input, &name, &token.location)?;
                setters.set_transition(transition, token.location.clone())?;
            }
            "td" => {
                return Err(gap(
                    &token.location,
                    "TD=<value> (measurements start at the axis origin in this port)",
                ));
            }
            "at" => setters.set_at(number(input, &name, &token.location)?, token.location)?,
            "val" => setters.set_val(number(input, &name, &token.location)?, token.location)?,
            "from" => setters.set_from(number(input, &name, &token.location)?, token.location)?,
            "to" => setters.set_to(number(input, &name, &token.location)?, token.location)?,
            _ => {
                return Err(fail(
                    &token.location,
                    format!(
                        "no such .measure parameter as '{}'; supported: {PARAMETERS}",
                        token.text
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// The value of `<name>=`, a finite numeric literal. The name token has already
/// been consumed, so a missing `=` is reported at the name.
fn number(input: &mut Input<'_>, name: &str, at: &SourceLoc) -> Result<Real> {
    let Some(token) = input.input.first().cloned() else {
        return Err(fail(
            at,
            format!("'{name}' needs a value: write {name}=<value>"),
        ));
    };
    if token.kind != TokenKind::Equals {
        return Err(fail(
            at,
            format!("'{name}' needs a value: write {name}=<value>"),
        ));
    }
    any.parse_next(input)?;
    let Some(value) = input.input.first().cloned() else {
        return Err(fail(at, format!("'{name}=' has no value")));
    };
    match value.kind {
        TokenKind::Number(number) if number.is_finite() => {
            any.parse_next(input)?;
            Ok(number)
        }
        _ if super::expression::is_expression_token(&value) => Err(gap(
            &value.location,
            format!("a {{…}} expression as the value of {name}="),
        )),
        _ => Err(fail(
            &value.location,
            format!(
                "expected a finite numeric value after '{name}=', found '{}'",
                value.text
            ),
        )),
    }
}

/// The value of `RISE=`/`FALL=`/`CROSS=`: a positive count or `LAST`.
fn transition(input: &mut Input<'_>, name: &str, at: &SourceLoc) -> Result<MeasureTransition> {
    if input
        .input
        .first()
        .is_some_and(|token| token.kind == TokenKind::Equals)
        && input
            .input
            .get(1)
            .is_some_and(|token| token.kind == TokenKind::Word && token.is_keyword("last"))
    {
        any.parse_next(input)?;
        any.parse_next(input)?;
        return Ok(MeasureTransition::Last);
    }
    let value = number(input, name, at)?;
    if !(value >= 1.0 && value.fract() == 0.0 && value <= f64::from(u32::MAX)) {
        return Err(fail(
            at,
            format!(
                "{name}= takes a whole crossing number of at least 1, or LAST (found {})",
                crate::primitives::format_spice_number(value)
            ),
        ));
    }
    let count = value as u32;
    Ok(match name {
        "rise" => MeasureTransition::Rise(count),
        "fall" => MeasureTransition::Fall(count),
        _ => MeasureTransition::Cross(count),
    })
}

/// True when the next token is the bare keyword `word`.
fn next_is(input: &Input<'_>, word: &str) -> bool {
    input
        .input
        .first()
        .is_some_and(|token: &Token| token.kind == TokenKind::Word && token.is_keyword(word))
}

/// The union of two clauses' windows: a bound written in both must agree.
fn merge(left: MeasureWindow, right: MeasureWindow, card: &SourceLoc) -> Result<MeasureWindow> {
    for (written, other, name) in [(left.from, right.from, "FROM"), (left.to, right.to, "TO")] {
        if let (Some(a), Some(b)) = (written, other)
            && a != b
        {
            return Err(fail(
                card,
                format!(
                    "{name}= is given twice with different values ({} and {}); write it in one \
                     TRIG/TARG clause",
                    crate::primitives::format_spice_number(a),
                    crate::primitives::format_spice_number(b)
                ),
            ));
        }
    }
    Ok(MeasureWindow {
        from: left.from.or(right.from),
        to: left.to.or(right.to),
    })
}

/// The optional setters of one clause, with duplicate detection.
#[derive(Default)]
struct Setters {
    at: Option<(Real, SourceLoc)>,
    val: Option<(Real, SourceLoc)>,
    from: Option<(Real, SourceLoc)>,
    to: Option<(Real, SourceLoc)>,
    transition: Option<(MeasureTransition, SourceLoc)>,
}

impl Setters {
    fn set_at(&mut self, value: Real, at: SourceLoc) -> Result<()> {
        set(&mut self.at, value, at, "AT")
    }

    fn set_val(&mut self, value: Real, at: SourceLoc) -> Result<()> {
        set(&mut self.val, value, at, "VAL")
    }

    fn set_from(&mut self, value: Real, at: SourceLoc) -> Result<()> {
        set(&mut self.from, value, at, "FROM")
    }

    fn set_to(&mut self, value: Real, at: SourceLoc) -> Result<()> {
        set(&mut self.to, value, at, "TO")
    }

    fn set_transition(&mut self, transition: MeasureTransition, at: SourceLoc) -> Result<()> {
        if self.transition.is_some() {
            return Err(fail(
                &at,
                "write at most one crossing selector (RISE=, FALL=, CROSS= or LAST)",
            ));
        }
        self.transition = Some((transition, at));
        Ok(())
    }

    fn at(&self) -> Option<(Real, SourceLoc)> {
        copy(self.at.as_ref())
    }

    fn val(&self) -> Option<(Real, SourceLoc)> {
        copy(self.val.as_ref())
    }

    fn transition(&self) -> Option<(MeasureTransition, SourceLoc)> {
        copy(self.transition.as_ref())
    }

    fn window(&self) -> MeasureWindow {
        MeasureWindow {
            from: self.from.as_ref().map(|(value, _)| *value),
            to: self.to.as_ref().map(|(value, _)| *value),
        }
    }
}

/// A copy of one positional setter.
fn copy<T: Copy>(setter: Option<&(T, SourceLoc)>) -> Option<(T, SourceLoc)> {
    setter.map(|(value, location)| (*value, location.clone()))
}

fn set(slot: &mut Option<(Real, SourceLoc)>, value: Real, at: SourceLoc, name: &str) -> Result<()> {
    if slot.is_some() {
        return Err(fail(&at, format!("{name}= is given more than once")));
    }
    *slot = Some((value, at));
    Ok(())
}
