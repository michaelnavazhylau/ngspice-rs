//! Ordered scope assembly; parsing X references is deliberately not elaboration.

use super::grammar::{self, ParsedCard};
use crate::ast::{
    AnalysisCard, DeviceInstance, GlobalCard, IncludeDirective, ModelCard, Netlist, OptionCard,
    ScopedCard, ScopedCardKind, Subcircuit,
};
use crate::card::{DotCommand, RawCard};
use crate::source::Deck;
use spice_core::{SourceLoc, SpiceError, SpiceResult};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub(super) struct InputCard {
    pub source: RawCard,
    pub include_chain: Vec<SourceLoc>,
    pub resolved_path: Option<PathBuf>,
    pub selected_section: Option<crate::ast::LibrarySection>,
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
    cards: Vec<ScopedCard>,
}

pub(super) fn assemble(
    deck: &Deck,
    cards: Vec<SpiceResult<InputCard>>,
    auto_gnd: bool,
) -> SpiceResult<Netlist> {
    let mut cursor = 0;
    let scope = scope(&cards, &mut cursor, auto_gnd, &BTreeSet::new(), None, 0)?;
    Ok(Netlist {
        title: deck.title.clone(),
        path: deck.path.clone(),
        location: deck.title_location.clone(),
        devices: scope.devices,
        models: scope.models,
        subcircuits: scope.subcircuits,
        analyses: scope.analyses,
        includes: scope.includes,
        cards: scope.cards,
        params: Vec::new(),
        options: scope.options,
        globals: scope.globals,
    })
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
    while let Some(entry) = cards.get(*cursor) {
        let entry = entry.as_ref().map_err(Clone::clone)?;
        *cursor += 1;
        let card = &entry.source;
        let kind = match grammar::parse_card(card, auto_gnd, &declared)? {
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
                reject_in_body(card, opening, ".option")?;
                result.options.push(o);
                ScopedCardKind::Options(result.options.len() - 1)
            }
            ParsedCard::Global(g) => {
                reject_in_body(card, opening, ".global")?;
                result.globals.push(g);
                ScopedCardKind::Global(result.globals.len() - 1)
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

// Body-local options/globals need per-subcircuit storage and flattening rules
// (inpcom.c/subckt.c); until then they must not be dropped or hoisted silently.
fn reject_in_body(
    card: &RawCard,
    opening: Option<(&str, &SourceLoc)>,
    what: &str,
) -> SpiceResult<()> {
    if opening.is_some() {
        return Err(SpiceError::not_yet_ported(
            format!("{}: {what} inside a .subckt body", card.location),
            "src/frontend/inpcom.c, src/frontend/subckt.c",
        ));
    }
    Ok(())
}
