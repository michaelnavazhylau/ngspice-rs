//! Incremental semantic deck parser built with winnow token-stream combinators.
//!
//! Dispatch follows `src/spicelib/parser/inppas2.c`, `INPpas2()`;
//! device grammars follow `inp2r.c`, `inp2c.c`, `inp2l.c`, `inp2v.c`, and
//! `inp2i.c`. Dot-card dispatch follows `inp2dot.c`, not the front-end
//! `parse-bison.y` expression grammar.
//!
//! M1a supports scalar R/C/L instances, independent DC/AC sources, and analysis
//! cards. Everything else fails explicitly rather than constructing an
//! incomplete netlist. Values stay textual; evaluation and circuit elaboration
//! are separate passes. See `docs/port/ROADMAP.md` for the remaining M1 work.

use std::path::Path;

use spice_core::SpiceResult;

use crate::ast::Netlist;
use crate::card::RawCard;
use crate::source::{Deck, load};

mod grammar;
mod linear;

use grammar::ParsedCard;

/// Turns decks into [`Netlist`]s for the currently supported syntax subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parser {
    auto_gnd: bool,
}

impl Parser {
    /// A parser with ngspice's default behaviour, where `gnd` means ground.
    #[must_use]
    pub fn new() -> Self {
        Self { auto_gnd: true }
    }

    /// A parser with the aliasing rule set explicitly.
    ///
    /// `auto_gnd == false` matches ngspice with `no_auto_gnd` set, where `gnd`
    /// is an ordinary node.
    #[must_use]
    pub fn with_auto_gnd(auto_gnd: bool) -> Self {
        Self { auto_gnd }
    }

    /// Whether `gnd` is aliased to node `0`.
    #[must_use]
    pub fn auto_gnd(&self) -> bool {
        self.auto_gnd
    }

    /// Classifies every card in a deck, independently of semantic parsing.
    ///
    /// # Errors
    ///
    /// Propagates tokenizer errors, for instance an unterminated `{` expression.
    pub fn classify_deck(&self, deck: &Deck) -> SpiceResult<Vec<RawCard>> {
        classify_deck(deck)
    }

    /// Builds a semantic netlist, stopping at `.end` as `INP2dot()` does.
    ///
    /// Analysis arguments are preserved without validation, per the AST
    /// contract; parsing a request does not imply its driver is implemented.
    /// Deck fragments without `.end` are accepted, as by `INPpas2()`.
    ///
    /// # Errors
    ///
    /// Returns [`spice_core::SpiceError::Parse`] for malformed supported syntax and
    /// [`spice_core::SpiceError::NotYetPorted`] for constructs outside the current subset.
    /// No partially parsed netlist is returned on failure.
    pub fn parse_deck(&self, deck: &Deck) -> SpiceResult<Netlist> {
        let mut netlist = Netlist {
            title: deck.title.clone(),
            path: deck.path.clone(),
            devices: Vec::new(),
            models: Vec::new(),
            subcircuits: Vec::new(),
            analyses: Vec::new(),
            includes: Vec::new(),
            params: Vec::new(),
            options: Vec::new(),
            globals: Vec::new(),
            location: deck.title_location.clone(),
        };
        for line in &deck.lines {
            let card = RawCard::parse(line)?;
            match grammar::parse_card(&card, self.auto_gnd)? {
                ParsedCard::Device(device) => netlist.devices.push(device),
                ParsedCard::Analysis(analysis) => netlist.analyses.push(analysis),
                ParsedCard::End => break,
            }
        }
        Ok(netlist)
    }

    /// Loads `path` and parses it.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or for the syntax errors and unported
    /// constructs described by [`Parser::parse_deck`].
    pub fn parse_file(&self, path: impl AsRef<Path>) -> SpiceResult<Netlist> {
        let deck = load(path)?;
        self.parse_deck(&deck)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Classifies every card in a deck.
///
/// # Errors
///
/// Propagates tokenizer errors.
pub fn classify_deck(deck: &Deck) -> SpiceResult<Vec<RawCard>> {
    deck.lines.iter().map(RawCard::parse).collect()
}

/// Loads a deck from disk and classifies it.
///
/// # Errors
///
/// Fails if the file cannot be read or a card cannot be tokenized.
pub fn load_classified(path: impl AsRef<Path>) -> SpiceResult<(Deck, Vec<RawCard>)> {
    let deck = load(path)?;
    let cards = classify_deck(&deck)?;
    Ok((deck, cards))
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::source::parse_deck_text;
    use std::path::Path;

    const DECK: &str = "\
RC divider
v1 in 0 dc 5
r1 in out 1k
r2 out 0 1k
.model dm d(is=1e-14)
.subckt pair a b
r1 a b 10k
.ends pair
.tran 1u 10u
.end
";

    #[test]
    fn classification_still_works_on_unported_syntax() {
        let deck = parse_deck_text(Path::new("rc.cir"), DECK);
        let cards = Parser::new().classify_deck(&deck).expect("classifies");
        assert_eq!(cards.len(), 9);
        assert_eq!(cards.iter().filter(|c| c.kind.is_device()).count(), 4);
        assert_eq!(cards.iter().filter(|c| c.kind.is_dot_command()).count(), 5);
    }

    #[test]
    fn parsing_reports_the_first_specific_gap() {
        let deck = parse_deck_text(Path::new("rc.cir"), DECK);
        let error = Parser::new()
            .parse_deck(&deck)
            .expect_err("model not ported");
        assert!(error.is_not_yet_ported());
        let message = error.to_string();
        assert!(message.contains("rc.cir:5:1"), "{message}");
        assert!(message.contains(".model directive"), "{message}");
        assert!(
            message.contains("src/spicelib/parser/inpdomod.c"),
            "{message}"
        );
    }

    #[test]
    fn parser_remembers_the_gnd_rule() {
        assert!(Parser::new().auto_gnd());
        assert!(!Parser::with_auto_gnd(false).auto_gnd());
        assert_eq!(Parser::default(), Parser::new());
    }
}
