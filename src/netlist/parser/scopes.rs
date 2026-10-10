//! Ordered scope assembly; parsing X references is deliberately not elaboration.

use super::grammar::{self, ParsedCard};
use super::save::OutputCard;
use crate::netlist::ast::{
    AnalysisCard, DeviceInstance, FourierCard, FuncCard, GlobalCard, IncludeDirective, MeasureCard,
    ModelCard, Netlist, NodeHintCard, OptionCard, OutputCards, ParamCard, ScopedCard,
    ScopedCardKind, Subcircuit,
};
use crate::netlist::card::{DotCommand, RawCard};
use crate::netlist::source::Deck;
use crate::primitives::{SourceLoc, SpiceError, SpiceResult};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub(super) struct InputCard {
    pub source: RawCard,
    pub include_chain: Vec<SourceLoc>,
    pub resolved_path: Option<PathBuf>,
    pub selected_section: Option<crate::netlist::ast::LibrarySection>,
}

impl From<RawCard> for InputCard {
    fn from(source: RawCard) -> Self {
        Self {
            source,
            include_chain: Vec::new(),
            resolved_path: None,
            selected_section: None,
        }
    }
}

#[derive(Default)]
struct Scope {
    devices: Vec<DeviceInstance>,
    models: Vec<ModelCard>,
    subcircuits: Vec<Subcircuit>,
    analyses: Vec<AnalysisCard>,
    includes: Vec<IncludeDirective>,
    options: Vec<OptionCard>,
    globals: Vec<GlobalCard>,
    initial_conditions: Vec<NodeHintCard>,
    nodesets: Vec<NodeHintCard>,
    params: Vec<ParamCard>,
    functions: Vec<FuncCard>,
    output: OutputCards,
    measurements: Vec<MeasureCard>,
    fourier: Vec<FourierCard>,
    cards: Vec<ScopedCard>,
}

pub(super) fn assemble(
    deck: &Deck,
    cards: Vec<SpiceResult<InputCard>>,
    auto_gnd: bool,
) -> SpiceResult<(Netlist, OutputCards, Vec<MeasureCard>, Vec<FourierCard>)> {
    let mut cursor = 0;
    let scope = scope(&cards, &mut cursor, auto_gnd, &BTreeSet::new(), None, 0)?;
    Ok((
        Netlist {
            title: deck.title.clone(),
            path: deck.path.clone(),
            location: deck.title_location.clone(),
            devices: scope.devices,
            models: scope.models,
            subcircuits: scope.subcircuits,
            analyses: scope.analyses,
            includes: scope.includes,
            cards: scope.cards,
            params: scope.params,
            functions: scope.functions,
            options: scope.options,
            globals: scope.globals,
            initial_conditions: scope.initial_conditions,
            nodesets: scope.nodesets,
        },
        scope.output,
        scope.measurements,
        scope.fourier,
    ))
}

// Read-only forward-name scan, bounded to this body and excluding children and
// control scripts. Ancestors are visible; siblings/children never leak names.
fn model_names(
    cards: &[SpiceResult<InputCard>],
    start: usize,
    inherited: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut names = inherited.clone();
    let mut depth = 0usize;
    let mut in_control = false;
    for entry in &cards[start..] {
        let Ok(entry) = entry else { continue };
        match entry.source.dot_command() {
            Some(DotCommand::Control) => in_control = true,
            Some(DotCommand::Endc) => in_control = false,
            Some(DotCommand::Subckt) if !in_control => depth += 1,
            Some(DotCommand::Ends) if !in_control => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Some(DotCommand::End) => break,
            Some(DotCommand::Model) if !in_control && depth == 0 => {
                if let Some(name) = entry.source.tokens.get(1).filter(|t| t.is_name_like()) {
                    names.insert(name.text.to_ascii_lowercase());
                }
            }
            _ => {}
        }
    }
    names
}

fn scope(
    cards: &[SpiceResult<InputCard>],
    cursor: &mut usize,
    auto_gnd: bool,
    inherited: &BTreeSet<String>,
    opening: Option<(&str, &SourceLoc)>,
    depth: usize,
) -> SpiceResult<Scope> {
    let declared = model_names(cards, *cursor, inherited);
    let mut result = Scope::default();
    let mut definitions = BTreeSet::new();
    let mut control = false;
    let mut control_ran = false;
    let mut control_quit = false;
    let mut deck_ran = false;
    while let Some(entry) = cards.get(*cursor) {
        let entry = entry.as_ref().map_err(Clone::clone)?;
        *cursor += 1;
        let card = &entry.source;
        if matches!(
            card.dot_command(),
            Some(DotCommand::Control | DotCommand::Endc)
        ) {
            let begin = card.dot_command() == Some(&DotCommand::Control);
            if opening.is_some() || card.tokens.len() != 1 || begin == control {
                return Err(SpiceError::parse(
                    card.location.clone(),
                    "invalid or nested .control/.endc settings block",
                ));
            }
            control = begin;
            if begin {
                control_ran = false;
                control_quit = false;
            }
            result.cards.push(ordered(entry, ScopedCardKind::Output));
            continue;
        }
        if control {
            if control_quit {
                return Err(SpiceError::not_yet_ported(
                    format!("{}: commands after quit", card.location),
                    "src/frontend/control.c",
                ));
            }
            if let Some(head) = card.tokens.first()
                && (head.is_keyword("run") || head.is_keyword("quit"))
            {
                if card.tokens.len() != 1 || (head.is_keyword("run") && deck_ran) {
                    return Err(SpiceError::not_yet_ported(
                        format!(
                            "{}: repeated or argument-bearing control run/quit",
                            card.location
                        ),
                        "src/frontend/runcoms.c",
                    ));
                }
                if head.is_keyword("run") {
                    control_ran = true;
                    deck_ran = true;
                } else {
                    if !control_ran {
                        return Err(SpiceError::not_yet_ported(
                            format!("{}: quit before run", card.location),
                            "src/frontend/inp.c, src/frontend/control.c",
                        ));
                    }
                    control_quit = true;
                }
                result.cards.push(ordered(entry, ScopedCardKind::Output));
                continue;
            }
        }
        let parsed = if control && card.tokens.first().is_some_and(|t| t.is_keyword("fourier")) {
            if !control_ran {
                return Err(SpiceError::parse(
                    card.location.clone(),
                    "fourier needs a preceding run in this control block",
                ));
            }
            let mut converted = card.clone();
            converted.tokens[0].text = ".four".into();
            converted.kind = crate::netlist::card::CardKind::DotCommand(DotCommand::Analysis(
                crate::primitives::AnalysisKind::Fourier,
            ));
            let mut parsed = grammar::parse_card(&converted, auto_gnd, &declared)?;
            if let ParsedCard::Fourier(card) = &mut parsed {
                card.frontend_command = true;
            }
            parsed
        } else if control {
            if deck_ran && card.tokens.first().is_some_and(|t| t.is_keyword("set")) {
                return Err(SpiceError::not_yet_ported(
                    format!(
                        "{}: set after run; set Fourier variables before run",
                        card.location
                    ),
                    "src/frontend/variable.c",
                ));
            }
            grammar::parse_frontend_setting(card, auto_gnd, &declared)?
        } else {
            grammar::parse_card(card, auto_gnd, &declared)?
        };
        let kind = match parsed {
            ParsedCard::Device(d) => {
                result.devices.push(d);
                ScopedCardKind::Device(result.devices.len() - 1)
            }
            ParsedCard::Model(m) => {
                result.models.push(m);
                ScopedCardKind::Model(result.models.len() - 1)
            }
            ParsedCard::Analysis(a) => {
                result.analyses.push(a);
                ScopedCardKind::Analysis(result.analyses.len() - 1)
            }
            ParsedCard::Options(o) => {
                result.options.push(o);
                ScopedCardKind::Options(result.options.len() - 1)
            }
            ParsedCard::Global(g) => {
                result.globals.push(g);
                ScopedCardKind::Global(result.globals.len() - 1)
            }
            ParsedCard::InitialCondition(c) => {
                result.initial_conditions.push(c);
                ScopedCardKind::InitialCondition(result.initial_conditions.len() - 1)
            }
            ParsedCard::Nodeset(c) => {
                result.nodesets.push(c);
                ScopedCardKind::Nodeset(result.nodesets.len() - 1)
            }
            ParsedCard::Param(p) => {
                result.params.push(p);
                ScopedCardKind::Param(result.params.len() - 1)
            }
            ParsedCard::Func(f) => {
                result.functions.push(f);
                ScopedCardKind::Func(result.functions.len() - 1)
            }
            ParsedCard::Output(output) => {
                // `.save`/`.print` describe the analysis output, not the
                // circuit: their typed requests travel beside the netlist
                // (`OutputCards`), so the card carries no scope-local index.
                match output {
                    OutputCard::Save(save) => {
                        result.output.saves.push(save);
                    }
                    OutputCard::Print(print) => {
                        reject_in_body(
                            card,
                            opening,
                            ".print",
                            "src/frontend/dotcards.c (ft_savedotargs/com_save2)",
                        )?;
                        result.output.prints.push(print);
                    }
                }
                ScopedCardKind::Output
            }
            ParsedCard::Measure(measure) => {
                // Like `.save`/`.print`, `.measure` describes the analysis
                // output rather than the circuit: the typed request travels
                // beside the netlist (`ParsedDeck::measurements`) and the card
                // carries no scope-local index. C collects `.meas` lines into
                // `ft_curckt->ci_meas` after expansion in `inp_spsource()`;
                // body requests remain attached until instance expansion.
                result.measurements.push(measure);
                ScopedCardKind::Measure
            }
            ParsedCard::Fourier(fourier) => {
                // Like `.measure`, a `.four` card describes the analysis output
                // rather than the circuit: the typed request travels beside the
                // netlist (`ParsedDeck::fourier`) and the card carries no
                // scope-local index. C filters `.four` lines out of the deck in
                // `inp_spsource()` before expansion and runs them after the
                // transient. Used body definitions contribute one copy.
                result.fourier.push(fourier);
                ScopedCardKind::Fourier
            }
            ParsedCard::Include(mut i) => {
                i.resolved_path = entry.resolved_path.clone();
                i.selected_section = entry.selected_section.clone();
                result.includes.push(i);
                ScopedCardKind::Include(result.includes.len() - 1)
            }
            ParsedCard::Subckt(mut s) => {
                if !definitions.insert(s.name.clone()) {
                    return Err(SpiceError::parse(
                        s.location,
                        format!("duplicate subcircuit definition '{}' in this scope", s.name),
                    ));
                }
                if depth >= 64 {
                    return Err(SpiceError::parse(
                        s.location,
                        "subcircuit nesting limit (64) exceeded",
                    ));
                }
                let body = scope(
                    cards,
                    cursor,
                    auto_gnd,
                    &declared,
                    Some((&s.name, &s.location)),
                    depth + 1,
                )?;
                s.end_location = body
                    .cards
                    .last()
                    .expect("closed scope has .ends")
                    .source
                    .location
                    .clone();
                s.devices = body.devices;
                s.models = body.models;
                s.subcircuits = body.subcircuits;
                s.analyses = body.analyses;
                s.includes = body.includes;
                s.params = body.params;
                s.functions = body.functions;
                s.options = body.options;
                s.globals = body.globals;
                s.initial_conditions = body.initial_conditions;
                s.nodesets = body.nodesets;
                s.output = body.output;
                s.measurements = body.measurements;
                s.fourier = body.fourier;
                s.cards = body.cards;
                result.subcircuits.push(s);
                ScopedCardKind::Subcircuit(result.subcircuits.len() - 1)
            }
            ParsedCard::Ends(name) => {
                let Some((expected, _)) = opening else {
                    return Err(SpiceError::parse(card.location.clone(), "unmatched .ends"));
                };
                if name.as_deref().is_some_and(|name| name != expected) {
                    return Err(SpiceError::parse(
                        card.location.clone(),
                        format!("mismatched .ends: expected '{expected}'"),
                    ));
                }
                result.cards.push(ordered(entry, ScopedCardKind::Ends));
                return Ok(result);
            }
            ParsedCard::End => {
                if let Some((name, _)) = opening {
                    return Err(SpiceError::parse(
                        card.location.clone(),
                        format!(".end before .ends for '{name}'"),
                    ));
                }
                result.cards.push(ordered(entry, ScopedCardKind::End));
                return Ok(result);
            }
            ParsedCard::LibStart(_) | ParsedCard::LibEnd(_) => {
                return Err(SpiceError::parse(
                    card.location.clone(),
                    "library section markers require .lib path section resolution",
                ));
            }
        };
        result.cards.push(ordered(entry, kind));
    }
    if control {
        return Err(SpiceError::circuit("missing .endc for settings block"));
    }
    if let Some((name, location)) = opening {
        return Err(SpiceError::parse(
            location.clone(),
            format!("missing .ends for '{name}'"),
        ));
    }
    Ok(result)
}

fn ordered(entry: &InputCard, kind: ScopedCardKind) -> ScopedCard {
    ScopedCard {
        kind,
        source: entry.source.clone(),
        include_chain: entry.include_chain.clone(),
    }
}

// Body-local options/globals/output cards need per-subcircuit storage and
// flattening rules (inpcom.c/subckt.c); until then they must not be dropped or
// hoisted silently.
fn reject_in_body(
    card: &RawCard,
    opening: Option<(&str, &SourceLoc)>,
    what: &str,
    c_reference: &'static str,
) -> SpiceResult<()> {
    if opening.is_some() {
        return Err(SpiceError::not_yet_ported(
            format!("{}: {what} inside a .subckt body", card.location),
            c_reference,
        ));
    }
    Ok(())
}
