//! Source-relative preprocessing, referencing inpcom.c's inp_readall and
//! library section processing. Deliberately no sourcepath/env/home expansion.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use petgraph::algo::has_path_connecting;
use petgraph::graph::{DiGraph, NodeIndex};
use spice_core::{SourceLoc, SpiceError, SpiceResult};

use super::grammar::{self, ParsedCard};
use super::scopes::InputCard;
use crate::ast::LibrarySection;
use crate::card::{DotCommand, RawCard};
use crate::source::{Deck, LogicalLine, parse_deck_text, parse_fragment_text};
use crate::sources::SourceProvider;

/// Per-parse source work limits. Repeated includes consume budgets again;
/// sharing a dependency never suppresses its ordered content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLimits {
    /// Maximum nested includes below the root. Values above 64 are rejected.
    pub max_depth: usize,
    /// Total file reads including the root and repeated includes.
    pub max_files: usize,
    /// Total UTF-8 source bytes read, including unselected library sections.
    pub max_bytes: usize,
    /// Total processed cards, including directives and structural terminators.
    pub max_cards: usize,
}

impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_files: 1024,
            max_bytes: 16 * 1024 * 1024,
            max_cards: 100_000,
        }
    }
}

type SourceKey = (PathBuf, Option<String>);

struct Resolver<'a> {
    sources: &'a dyn SourceProvider,
    limits: SourceLimits,
    files: usize,
    bytes: usize,
    cards: usize,
    dependencies: DiGraph<SourceKey, ()>,
    nodes: BTreeMap<SourceKey, NodeIndex>,
}

pub(super) fn resolve(
    path: &Path,
    sources: &dyn SourceProvider,
    limits: SourceLimits,
) -> SpiceResult<(Deck, Vec<SpiceResult<InputCard>>)> {
    let location = SourceLoc::new(path.to_path_buf(), 1, 1);
    if limits.max_depth > 64 {
        return Err(SpiceError::parse(
            location,
            "source depth limit must be at most 64",
        ));
    }
    let mut resolver = Resolver {
        sources,
        limits,
        files: 0,
        bytes: 0,
        cards: 0,
        dependencies: DiGraph::new(),
        nodes: BTreeMap::new(),
    };
    let canonical = sources
        .canonicalize(path)
        .map_err(|e| SpiceError::io(path, &e))?;
    let text = resolver.read(&canonical, &location)?;
    // Keep the caller's root path/title API; fragments use canonical paths.
    let deck = parse_deck_text(path, &text);
    let root = resolver.node((canonical, None));
    let mut output = Vec::new();
    if let Err(error) = resolver.expand(&deck.lines, root, &[], &mut output) {
        // Replay resolution errors alongside cards so an earlier unsupported
        // directive/device keeps its diagnostic instead of a later I/O error.
        output.push(Err(error));
    }
    Ok((deck, output))
}

impl Resolver<'_> {
    fn read(&mut self, path: &Path, location: &SourceLoc) -> SpiceResult<String> {
        if self.files >= self.limits.max_files {
            return Err(SpiceError::parse(
                location.clone(),
                "source file work limit exceeded",
            ));
        }
        self.files += 1;
        let remaining = self.limits.max_bytes.saturating_sub(self.bytes);
        let bytes = self
            .sources
            .read(path, u64::try_from(remaining).unwrap_or(u64::MAX))
            .map_err(|e| SpiceError::io(path, &e))?;
        if bytes.len() > remaining {
            return Err(SpiceError::parse(
                location.clone(),
                "source byte work limit exceeded",
            ));
        }
        self.bytes += bytes.len();
        String::from_utf8(bytes)
            .map_err(|e| SpiceError::parse(location.clone(), format!("source is not UTF-8: {e}")))
    }

    fn node(&mut self, key: SourceKey) -> NodeIndex {
        *self
            .nodes
            .entry(key.clone())
            .or_insert_with(|| self.dependencies.add_node(key))
    }

    fn expand(
        &mut self,
        lines: &[LogicalLine],
        parent: NodeIndex,
        chain: &[SourceLoc],
        output: &mut Vec<SpiceResult<InputCard>>,
    ) -> SpiceResult<bool> {
        for line in lines {
            if self.cards >= self.limits.max_cards {
                return Err(SpiceError::parse(
                    line.location.clone(),
                    "source card work limit exceeded",
                ));
            }
            self.cards += 1;
            let card = match RawCard::parse(line) {
                Ok(card) => card,
                Err(error) => {
                    output.push(Err(error));
                    continue;
                }
            };
            let end = card.dot_command() == Some(&DotCommand::End);
            let include = matches!(
                card.dot_command(),
                Some(DotCommand::Include | DotCommand::Lib)
            );
            let mut entry = InputCard {
                source: card,
                include_chain: chain.to_vec(),
                resolved_path: None,
                selected_section: None,
            };
            if include {
                let parsed = grammar::parse_card(&entry.source, true, &BTreeSet::new())?;
                if let ParsedCard::Include(directive) = parsed {
                    if chain.len() >= self.limits.max_depth {
                        return Err(SpiceError::parse(
                            directive.location,
                            "source include depth limit exceeded",
                        ));
                    }
                    let target = directive
                        .location
                        .path()
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(&directive.path);
                    let canonical = self.sources.canonicalize(&target).map_err(|e| {
                        SpiceError::parse(
                            directive.location.clone(),
                            format!("cannot resolve source '{}': {e}", target.display()),
                        )
                    })?;
                    let child = self.node((canonical.clone(), directive.section.clone()));
                    // Canonical path plus selected section is the dependency identity.
                    // Petgraph detects reachable cycles (including self/symlink loops).
                    if has_path_connecting(&self.dependencies, child, parent, None) {
                        return Err(SpiceError::parse(
                            directive.location,
                            format!(
                                "source cycle through '{}'{}",
                                canonical.display(),
                                directive
                                    .section
                                    .as_ref()
                                    .map_or(String::new(), |s| format!(" section '{s}'"))
                            ),
                        ));
                    }
                    self.dependencies.update_edge(parent, child, ());
                    entry.resolved_path = Some(canonical.clone());
                    let text = self.read(&canonical, &directive.location).map_err(|e| {
                        SpiceError::parse(
                            directive.location.clone(),
                            format!("loading source '{}': {e}", canonical.display()),
                        )
                    })?;
                    let fragment = parse_fragment_text(&canonical, &text);
                    let selected = match directive.section {
                        Some(section) => {
                            let (lines, boundaries) =
                                select_section(&fragment.lines, &section, &directive.location)?;
                            entry.selected_section = Some(boundaries);
                            lines
                        }
                        None => fragment.lines,
                    };
                    output.push(Ok(entry));
                    let mut nested_chain = chain.to_vec();
                    nested_chain.push(directive.location);
                    if self.expand(&selected, child, &nested_chain, output)? {
                        return Ok(true);
                    }
                    continue;
                }
            }
            output.push(Ok(entry));
            if end {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Validate section boundaries even in unselected blocks; do not tokenize or
/// resolve ordinary cards in those blocks. Section names are case-insensitive.
fn select_section(
    lines: &[LogicalLine],
    requested: &str,
    location: &SourceLoc,
) -> SpiceResult<(Vec<LogicalLine>, LibrarySection)> {
    let mut current: Option<(String, RawCard)> = None;
    let mut sections = BTreeSet::new();
    let mut selected = Vec::new();
    let mut boundaries = None;
    for line in lines {
        let keyword = line.text.split_whitespace().next().unwrap_or_default();
        if keyword.eq_ignore_ascii_case(".lib") || keyword.eq_ignore_ascii_case(".endl") {
            let card = RawCard::parse(line)?;
            match grammar::parse_card(&card, true, &BTreeSet::new())? {
                ParsedCard::LibStart(name) => {
                    if current.is_some() {
                        return Err(SpiceError::parse(
                            line.location.clone(),
                            "nested library sections are unsupported",
                        ));
                    }
                    if !sections.insert(name.clone()) {
                        return Err(SpiceError::parse(
                            line.location.clone(),
                            format!("duplicate library section '{name}'"),
                        ));
                    }
                    current = Some((name, card));
                    continue;
                }
                ParsedCard::LibEnd(name) => {
                    let Some((expected, opening)) = current.take() else {
                        return Err(SpiceError::parse(line.location.clone(), "unmatched .endl"));
                    };
                    if name.as_deref().is_some_and(|n| n != expected) {
                        return Err(SpiceError::parse(
                            line.location.clone(),
                            format!("mismatched .endl: expected '{expected}'"),
                        ));
                    }
                    if expected == requested {
                        boundaries = Some(LibrarySection {
                            name: expected,
                            opening,
                            closing: card,
                        });
                    }
                    continue;
                }
                ParsedCard::Include(_) => {}
                _ => {
                    return Err(SpiceError::parse(
                        line.location.clone(),
                        "expected a library directive",
                    ));
                }
            }
        }
        if current.as_ref().is_some_and(|(name, _)| name == requested) {
            selected.push(line.clone());
        }
    }
    if let Some((name, start)) = current {
        return Err(SpiceError::parse(
            start.location,
            format!("missing .endl for '{name}'"),
        ));
    }
    let Some(boundaries) = boundaries else {
        return Err(SpiceError::parse(
            location.clone(),
            format!("library section '{requested}' not found"),
        ));
    };
    Ok((selected, boundaries))
}
