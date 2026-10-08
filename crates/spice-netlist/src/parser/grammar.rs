//! Winnow card grammar and the adapter to the port's structured error type.
//!
//! Input is a borrowed, already-lexed token stream. Read-only state supplies
//! source locations, ground-alias configuration and C references; backtracking
//! never mutates the AST or parser configuration.

use std::collections::BTreeSet;

use spice_core::{AnalysisKind, SourceLoc, SpiceError, SpiceResult};
use winnow::Parser as _;
use winnow::combinator::{alt, peek};
use winnow::error::{AddContext, ErrMode, ModalResult, ParserError};
use winnow::stream::{Stateful, Stream, TokenSlice};
use winnow::token::{any, rest};

use crate::ast::{
    AnalysisCard, ArgumentExpression, DeviceInstance, GlobalCard, IncludeDirective, ModelCard,
    NodeHintCard, OptionCard, ParamCard, Subcircuit,
};
use crate::card::{CardKind, DotCommand, RawCard};
use crate::token::Token;

use super::{
    diode, expression, fourier, hints, linear, measure, model, options, param, save, structure,
    transistor,
};

pub(super) enum ParsedCard {
    Device(DeviceInstance),
    Model(ModelCard),
    Analysis(AnalysisCard),
    Options(OptionCard),
    Global(GlobalCard),
    InitialCondition(NodeHintCard),
    Nodeset(NodeHintCard),
    Subckt(Subcircuit),
    Ends(Option<String>),
    Include(IncludeDirective),
    Param(ParamCard),
    /// A `.save` or `.print` output card; see [`crate::ast::OutputCards`].
    Output(save::OutputCard),
    /// A `.measure`/`.meas` card; see [`crate::ast::MeasureCard`].
    Measure(crate::ast::MeasureCard),
    /// A `.four` card; see [`crate::ast::FourierCard`].
    Fourier(crate::ast::FourierCard),
    LibStart(String),
    LibEnd(Option<String>),
    End,
}

#[derive(Debug)]
pub(super) struct Context<'a> {
    pub card: &'a RawCard,
    pub auto_gnd: bool,
    pub declared_models: &'a BTreeSet<String>,
}

pub(super) type Input<'a> = Stateful<TokenSlice<'a, Token>, Context<'a>>;
pub(super) type Result<T> = ModalResult<T, Failure>;

/// Keeps domain errors intact through `alt`, `opt`, `repeat`, and cuts.
/// In particular a missing feature must never become an optional successful
/// parse, or lose its C reference when another alternative is tried.
#[derive(Debug)]
pub(super) struct Failure(pub SpiceError);

impl ParserError<Input<'_>> for Failure {
    type Inner = Self;

    fn from_input(input: &Input<'_>) -> Self {
        Self(SpiceError::parse(
            location(input),
            "unexpected token or end of card",
        ))
    }

    fn into_inner(self) -> std::result::Result<Self::Inner, Self> {
        Ok(self)
    }
}

impl AddContext<Input<'_>, &'static str> for Failure {
    fn add_context(
        mut self,
        _input: &Input<'_>,
        _start: &<Input<'_> as Stream>::Checkpoint,
        expected: &'static str,
    ) -> Self {
        if let SpiceError::Parse { message, .. } = &mut self.0 {
            *message = format!("expected {expected}");
        }
        self
    }
}

pub(super) fn parse_card(
    card: &RawCard,
    auto_gnd: bool,
    declared_models: &BTreeSet<String>,
) -> SpiceResult<ParsedCard> {
    let input = Input {
        input: TokenSlice::new(&card.tokens),
        state: Context {
            card,
            auto_gnd,
            declared_models,
        },
    };
    // `parse` requires full token consumption. The .end branch intentionally
    // consumes its tail: INP2dot ignores additional input after .end.
    alt((
        end_card,
        alt((
            fourier::fourier_card,
            analysis_card,
            options::options_or_global,
        )),
        alt((
            model::model_card,
            param::param_card,
            hints::hint_card,
            save::output_card,
            measure::measure_card,
        )),
        structure::structural_card,
        linear::device_card,
        diode::diode_card,
        transistor::transistor_card,
        unported_card,
        unknown_card,
    ))
    .parse(input)
    .map_err(|error| error.into_inner().0)
}

fn end_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    (keyword(".end"), rest)
        .map(|_| ParsedCard::End)
        .parse_next(input)
}

fn analysis_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let (kind, arguments) = (
        any.verify_map(|token: &Token| {
            if !token.text.starts_with('.') {
                return None;
            }
            DotCommand::parse(&token.text).analysis()
        }),
        rest,
    )
        .parse_next(input)?;
    let (arguments, uic) = if kind == AnalysisKind::Transient {
        split_uic(arguments)?
    } else {
        (arguments.iter().collect(), None)
    };
    // Braced arguments are validated and parsed (never evaluated here); the
    // remaining arguments stay opaque text for the analysis drivers.
    let mut expressions = Vec::new();
    for (index, token) in arguments.iter().enumerate() {
        if matches!(token.kind, crate::token::TokenKind::Expression(_)) {
            let expression = expression::from_brace_token(token)
                .map_err(|error| ErrMode::Cut(Failure(error)))?;
            expressions.push(ArgumentExpression { index, expression });
        }
    }
    Ok(ParsedCard::Analysis(AnalysisCard {
        kind,
        arguments: arguments.iter().map(|token| token.text.clone()).collect(),
        expressions,
        uic: uic.is_some(),
        uic_location: uic,
        location: input.state.card.location.clone(),
    }))
}

/// C: `dot_tran()` in `inp2dot.c` reads `Tstep Tstop [Tstart [Tmax]]` and then
/// one trailing word, `uic`; any other trailing word is ignored with a litmsg.
/// The port removes a standalone `uic` word from the positional arguments and
/// records it as a flag. It may be followed only by the port's `name=value`
/// driver options, must appear once, and is a bare flag (`uic=1` is rejected).
fn split_uic(tokens: &[Token]) -> Result<(Vec<&Token>, Option<SourceLoc>)> {
    let cut = |at: &Token, message: &str| {
        ErrMode::Cut(Failure(SpiceError::parse(at.location.clone(), message)))
    };
    let mut kept: Vec<&Token> = tokens.iter().collect();
    let mut flag: Option<(usize, SourceLoc)> = None;
    let mut i = 0;
    while i < kept.len() {
        let token = kept[i];
        let is_value = i > 0 && kept[i - 1].kind == crate::token::TokenKind::Equals;
        if token.kind == crate::token::TokenKind::Word && token.is_keyword("uic") && !is_value {
            if kept
                .get(i + 1)
                .is_some_and(|next| next.kind == crate::token::TokenKind::Equals)
            {
                return Err(cut(token, "uic is a bare flag; write 'uic' without '='"));
            }
            if flag.is_some() {
                return Err(cut(token, "duplicate uic flag on .tran"));
            }
            flag = Some((i, token.location.clone()));
            kept.remove(i);
            continue;
        }
        i += 1;
    }
    if let Some((index, _)) = &flag {
        let mut j = *index;
        while j < kept.len() {
            if kept
                .get(j + 1)
                .is_some_and(|next| next.kind == crate::token::TokenKind::Equals)
            {
                j += 3;
            } else {
                return Err(cut(
                    kept[j],
                    "uic must follow the .tran time arguments (Tstep Tstop [Tstart [Tmax]] uic)",
                ));
            }
        }
    }
    Ok((kept, flag.map(|(_, location)| location)))
}

fn unported_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    peek(any).parse_next(input)?;
    let what = match &input.state.card.kind {
        CardKind::Device { designator } => format!("device grammar '{designator}'"),
        CardKind::DotCommand(command) => format!("{} directive", command.card_name()),
        CardKind::Unknown => return Err(ErrMode::Backtrack(Failure::from_input(input))),
    };
    Err(gap(input, &what))
}

fn unknown_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    Err(ErrMode::Cut(Failure(SpiceError::parse(
        location(input),
        format!("unrecognised card: {}", input.state.card.raw),
    ))))
}

pub(super) fn keyword<'a>(
    expected: &'static str,
) -> impl winnow::Parser<Input<'a>, &'a Token, ErrMode<Failure>> {
    any.verify(move |token: &Token| token.is_keyword(expected))
}

pub(super) fn location(input: &Input<'_>) -> SourceLoc {
    input.input.first().map_or_else(
        || {
            input.state.card.location.at_column(
                u32::try_from(input.state.card.raw.len())
                    .unwrap_or(u32::MAX)
                    .saturating_add(1),
            )
        },
        |token| token.location.clone(),
    )
}

pub(super) fn gap(input: &Input<'_>, what: &str) -> ErrMode<Failure> {
    let reference = match &input.state.card.kind {
        CardKind::Device { designator: 'x' } => "src/frontend/subckt.c".to_owned(),
        CardKind::Device { designator: 'a' } => "src/xspice/".to_owned(),
        CardKind::Device { designator } => format!("src/spicelib/parser/inp2{designator}.c"),
        CardKind::DotCommand(command) => match command {
            DotCommand::Model => "src/spicelib/parser/inpdomod.c",
            DotCommand::Subckt | DotCommand::Ends => "src/frontend/subckt.c",
            DotCommand::Include | DotCommand::Lib => "src/frontend/inpcom.c",
            DotCommand::Param => "src/frontend/numparam/spicenum.c",
            DotCommand::Measure => "src/frontend/measure.c, src/frontend/com_measure2.c",
            DotCommand::Control | DotCommand::Endc => "src/frontend/inp.c",
            _ => "src/spicelib/parser/inp2dot.c",
        }
        .to_owned(),
        CardKind::Unknown => "src/spicelib/parser/inppas2.c".to_owned(),
    };
    ErrMode::Cut(Failure(SpiceError::not_yet_ported(
        format!("{}: {what}", location(input)),
        reference,
    )))
}
