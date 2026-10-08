//! Incremental semantic deck parser built with winnow token-stream combinators.
//!
//! Dispatch follows `src/spicelib/parser/inppas2.c`, `INPpas2()`;
//! device grammars follow `inp2r.c`, `inp2c.c`, `inp2l.c`, `inp2v.c`, and
//! `inp2i.c`, plus `inp2d.c`, `inp2q.c` and `inp2m.c` for bounded D/Q/M forms.
//! Scalar model cards follow
//! `inpdomod.c`/`inpgmod.c`. Dot-card dispatch follows `inp2dot.c`, not the front-end
//! `parse-bison.y` expression grammar.
//!
//! The implemented subset is M1a (scalar R/C/L, DC/AC sources, analysis cards)
//! plus M1b model cards, two-terminal D, three/four-terminal Q and
//! four-terminal M instances, bounded flags/IC vectors and numeric PULSE/PWL. Q/M use declared names for terminal disambiguation;
//! scoped subcircuits/X and source-relative include/library resolution. Model
//! types/backend availability and parameter validity are not checked yet.
//! Other constructs fail explicitly, never silently dropping cards. Values stay
//! textual; evaluation and circuit elaboration are separate passes. See `docs/port/ROADMAP.md` for the remaining M1 work.

use std::path::Path;

use spice_core::SpiceResult;

use crate::ast::{FourierCard, MeasureCard, Netlist, OutputCards};
use crate::card::{DotCommand, RawCard};
use crate::source::{Deck, load};
use crate::sources::{FileSystem, SourceProvider};

pub use resolution::SourceLimits;

mod diode;
mod expression;
mod flags;
mod fourier;
mod grammar;
mod hints;
mod ic;
mod linear;
mod measure;
mod model;
mod options;
mod param;
mod resolution;
mod save;
mod scopes;
mod structure;
mod syntax;
mod transistor;
mod vector;
mod waveform;

/// Turns decks into [`Netlist`]s for the currently supported syntax subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parser {
    auto_gnd: bool,
}

/// A parsed deck: the semantic netlist, the `.save`/`.print` cards, the
/// `.measure` cards and the `.four` cards.
///
/// The output and post-processing cards are returned beside the netlist rather
/// than inside it. They describe what an analysis should write and what should
/// be measured or transformed afterwards, not how the circuit is built, so they
/// never reach device elaboration; separating them also keeps the netlist
/// unchanged for every consumer that only simulates the circuit. See
/// [`Parser::parse_file_with_output`], `docs/port/OUTPUT_SELECTION.md`,
/// `docs/port/MEASURE.md` and `docs/port/FOURIER.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedDeck {
    /// The semantic netlist.
    pub netlist: Netlist,
    /// The `.save`/`.print` cards the deck contains, typed and positioned.
    pub output: OutputCards,
    /// The `.measure`/`.meas` cards the deck contains, in deck order, typed and
    /// positioned. Measurements are evaluated over the **full** plot, so the
    /// output selection never hides a measurable vector.
    pub measurements: Vec<MeasureCard>,
    /// The `.four` cards the deck contains, in deck order, typed and
    /// positioned. A `.four` card is evaluated over the **full** plot as well,
    /// so the output selection never hides a transformed vector and the written
    /// rawfile never changes because a `.four` exists.
    pub fourier: Vec<FourierCard>,
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
    /// Includes are retained without I/O; use [`Parser::parse_file`] for source
    /// resolution. Nested subcircuits own ordered cards and local declarations.
    /// Q/M require a model declaration in their scope or an ancestor before
    /// `.end`; forward declarations work. A read-only name index disambiguates optional ports,
    /// without validating model type/backend or applying selector/default rules.
    /// D references may remain unresolved. None of these are simulation inputs
    /// until the later elaboration pass validates them.
    ///
    /// `.save`/`.print` and `.measure` cards are validated here (a malformed
    /// request is an error, never a dropped card) and returned by
    /// [`Parser::parse_deck_with_output`].
    ///
    /// # Errors
    ///
    /// Returns [`spice_core::SpiceError::Parse`] for malformed supported syntax and
    /// [`spice_core::SpiceError::NotYetPorted`] for constructs outside the current subset.
    /// No partially parsed netlist is returned on failure.
    pub fn parse_deck(&self, deck: &Deck) -> SpiceResult<Netlist> {
        Ok(self.parse_deck_with_output(deck)?.netlist)
    }

    /// Builds a semantic netlist and reports the deck's `.save`/`.print` cards
    /// and `.measure` cards.
    ///
    /// # Errors
    ///
    /// The same failures as [`Parser::parse_deck`].
    pub fn parse_deck_with_output(&self, deck: &Deck) -> SpiceResult<ParsedDeck> {
        let (netlist, output, measurements, fourier) =
            scopes::assemble(deck, prepare_cards(deck), self.auto_gnd)?;
        Ok(ParsedDeck {
            netlist,
            output,
            measurements,
            fourier,
        })
    }

    /// Parses one unevaluated parameter expression, for instance the text of a
    /// `{...}` value. `location` is the position of the first byte of `text`
    /// (its column anchors every span and diagnostic). Whitespace is allowed
    /// between tokens, as inside braces. See [`crate::expr`] for the grammar.
    ///
    /// # Errors
    ///
    /// [`spice_core::SpiceError::Parse`] for malformed syntax and
    /// [`spice_core::SpiceError::NotYetPorted`] for valid numparam outside the
    /// bounded subset (other operators, functions or quoting).
    pub fn parse_expression(
        &self,
        text: &str,
        location: &spice_core::SourceLoc,
    ) -> SpiceResult<crate::expr::ParameterExpression> {
        expression::parse_expression(text, location, location.column, false)
    }

    /// Loads `path`, resolves source-relative `.include`/`.lib` directives and
    /// parses ordered scoped cards with default [`SourceLimits`]. Included files
    /// are fragments (no title); the root's first physical line is its title.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or for the syntax errors and unported
    /// constructs described by [`Parser::parse_deck`].
    pub fn parse_file(&self, path: impl AsRef<Path>) -> SpiceResult<Netlist> {
        Ok(self.parse_file_with_output(path)?.netlist)
    }

    /// Loads `path` and reports the deck's `.save`/`.print` and `.measure`
    /// cards as well.
    ///
    /// # Errors
    ///
    /// The same failures as [`Parser::parse_file`].
    pub fn parse_file_with_output(&self, path: impl AsRef<Path>) -> SpiceResult<ParsedDeck> {
        self.parse_file_with_limits_and_output(path, SourceLimits::default())
    }

    /// Resolves and parses a file with explicit source work limits.
    ///
    /// Paths are relative to the file containing each directive, not the process
    /// working directory. Canonical file/section dependency cycles are rejected.
    /// No home/environment/search-path substitution is performed.
    ///
    /// # Errors
    /// I/O, malformed structure, unavailable syntax, cycles or exhausted limits.
    pub fn parse_file_with_limits(
        &self,
        path: impl AsRef<Path>,
        limits: SourceLimits,
    ) -> SpiceResult<Netlist> {
        Ok(self
            .parse_file_with_limits_and_output(path, limits)?
            .netlist)
    }

    /// Resolves and parses a file with explicit limits and reports its output
    /// cards as well.
    ///
    /// # Errors
    ///
    /// The same failures as [`Parser::parse_file_with_limits`].
    pub fn parse_file_with_limits_and_output(
        &self,
        path: impl AsRef<Path>,
        limits: SourceLimits,
    ) -> SpiceResult<ParsedDeck> {
        self.parse_file_with_sources_and_output(path, &FileSystem, limits)
    }

    /// Resolves and parses a deck whose text, and that of every `.include`/`.lib`
    /// it reaches, comes from `sources` instead of the operating-system file
    /// system (for example [`crate::sources::MemorySources`] in a browser).
    /// Resolution rules and limits are those of [`Parser::parse_file_with_limits`].
    ///
    /// # Errors
    /// As [`Parser::parse_file_with_limits`].
    pub fn parse_file_with_sources(
        &self,
        path: impl AsRef<Path>,
        sources: &dyn SourceProvider,
        limits: SourceLimits,
    ) -> SpiceResult<Netlist> {
        Ok(self
            .parse_file_with_sources_and_output(path, sources, limits)?
            .netlist)
    }

    /// [`Parser::parse_file_with_sources`] that also reports the output cards
    /// (`.save`/`.print`, `.measure`, `.four`).
    ///
    /// # Errors
    /// As [`Parser::parse_file_with_limits`].
    pub fn parse_file_with_sources_and_output(
        &self,
        path: impl AsRef<Path>,
        sources: &dyn SourceProvider,
        limits: SourceLimits,
    ) -> SpiceResult<ParsedDeck> {
        let (deck, cards) = resolution::resolve(path.as_ref(), sources, limits)?;
        let (netlist, output, measurements, fourier) =
            scopes::assemble(&deck, cards, self.auto_gnd)?;
        Ok(ParsedDeck {
            netlist,
            output,
            measurements,
            fourier,
        })
    }
}

/// Cache lexical results for ordered replay; later errors do not mask earlier
/// semantic failures. Forward declaration lookup belongs to each scope.
fn prepare_cards(deck: &Deck) -> Vec<SpiceResult<scopes::InputCard>> {
    let mut cards = Vec::new();
    for line in &deck.lines {
        let card = RawCard::parse(line);
        let end = card
            .as_ref()
            .is_ok_and(|c| c.dot_command() == Some(&DotCommand::End));
        cards.push(card.map(Into::into));
        if end {
            break;
        }
    }
    cards
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
    fn parsing_retains_scoped_devices() {
        let deck = parse_deck_text(Path::new("rc.cir"), DECK);
        let netlist = Parser::new().parse_deck(&deck).unwrap();
        assert_eq!(netlist.devices.len(), 3);
        assert_eq!(netlist.subcircuits[0].devices.len(), 1);
        assert_eq!(netlist.subcircuits[0].location.line, 6);
    }

    #[test]
    fn parser_remembers_the_gnd_rule() {
        assert!(Parser::new().auto_gnd());
        assert!(!Parser::with_auto_gnd(false).auto_gnd());
        assert_eq!(Parser::default(), Parser::new());
    }
}
