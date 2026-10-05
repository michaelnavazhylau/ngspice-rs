//! The parser — **not ported**.
//!
//! What exists here is the part of the front end that is implemented: a deck can
//! be loaded and every card classified. Turning those cards into an
//! [`ast::Netlist`] is the M1 milestone; see `docs/port/ROADMAP.md`.
//!
//! The C code to port is `src/spicelib/parser/inp2*.c` (one file per device
//! designator), `src/spicelib/parser/inppas*.c` and `ifeval.c` (`.param`
//! evaluation), and `inpcom.c` (the `.` command dispatch). The Bison grammar in
//! `src/frontend/parse-bison.y` is only 180 lines; the port uses a hand-written
//! recursive-descent parser instead, which is easier to keep in step with the
//! C behaviour and gives better error messages.

use std::path::Path;

use spice_core::{SpiceError, SpiceResult};

use crate::C_REFERENCE;
use crate::ast::Netlist;
use crate::card::RawCard;
use crate::source::{Deck, load};

/// Turns decks into [`Netlist`]s.
///
/// The configuration it carries — currently only the `gnd` aliasing rule — is
/// needed by the parser, not by the loader, because the rule is applied to node
/// names as they are parsed.
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

    /// Classifies every card in a deck. Implemented.
    ///
    /// # Errors
    ///
    /// Propagates tokenizer errors, for instance an unterminated `{` expression.
    pub fn classify_deck(&self, deck: &Deck) -> SpiceResult<Vec<RawCard>> {
        classify_deck(deck)
    }

    /// Builds a [`Netlist`] from a deck.
    ///
    /// # Errors
    ///
    /// Always returns [`SpiceError::NotYetPorted`], after reporting how much of
    /// the deck was successfully tokenized and classified.
    pub fn parse_deck(&self, deck: &Deck) -> SpiceResult<Netlist> {
        let cards = classify_deck(deck)?;
        let devices = cards.iter().filter(|card| card.kind.is_device()).count();
        let commands = cards
            .iter()
            .filter(|card| card.kind.is_dot_command())
            .count();
        Err(SpiceError::not_yet_ported(
            format!(
                "netlist parser: {} classified card(s) in {} ({devices} device instance(s), \
                 {commands} dot command(s)); loading, tokenizing and classification work, \
                 building the semantic netlist does not",
                cards.len(),
                deck.path.display()
            ),
            C_REFERENCE,
        ))
    }

    /// Loads `path` and parses it.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or with
    /// [`SpiceError::NotYetPorted`] — see [`Parser::parse_deck`].
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
    fn classification_counts_the_cards() {
        let deck = parse_deck_text(Path::new("rc.cir"), DECK);
        let cards = Parser::new().classify_deck(&deck).expect("classifies");
        assert_eq!(cards.len(), 9);

        let devices = cards.iter().filter(|card| card.kind.is_device()).count();
        let commands = cards
            .iter()
            .filter(|card| card.kind.is_dot_command())
            .count();
        assert_eq!(devices, 4, "v1, r1, r2 and the r1 inside the subcircuit");
        assert_eq!(
            commands, 5,
            ".model, .subckt, .ends, .tran and .end — classification is context-free, so the \
             subcircuit body is not folded away"
        );
    }

    #[test]
    fn parsing_reports_what_is_missing_without_guessing() {
        let deck = parse_deck_text(Path::new("rc.cir"), DECK);
        let error = Parser::new().parse_deck(&deck).expect_err("not ported");
        assert!(error.is_not_yet_ported());
        let message = error.to_string();
        assert!(message.contains("9 classified card(s)"), "{message}");
        assert!(message.contains("4 device instance(s)"), "{message}");
        assert!(message.contains("5 dot command(s)"), "{message}");
        assert!(message.contains("src/spicelib/parser"), "{message}");
    }

    #[test]
    fn parser_remembers_the_gnd_rule() {
        assert!(Parser::new().auto_gnd());
        assert!(!Parser::with_auto_gnd(false).auto_gnd());
        assert_eq!(Parser::default(), Parser::new());
    }
}
