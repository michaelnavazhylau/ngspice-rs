//! Card classification: what kind of thing is this logical line?
//!
//! The C front end dispatches on the first character of the card: `.` selects a
//! `.` command (`inp2dot.c`), and a letter selects a device parser
//! (`inp2<letter>.c`), dispatched by `inppas2.c`. Classification is separate
//! from semantic parsing, so the CLI can report even unported syntax.

use crate::primitives::{AnalysisKind, SourceLoc, SpiceResult};

use crate::netlist::source::LogicalLine;
use crate::netlist::token::{Token, tokenize};

/// Device designator letters recognised by the C front end.
///
/// The authoritative per-designator metadata — the C parser to port, the number
/// of terminals, whether the device adds a branch current — lives in
/// `devices::registry`. This list only answers "does this card start with
/// a device letter?".
pub const DEVICE_DESIGNATORS: &[char] = &[
    'r', 'c', 'l', 'v', 'i', 'd', 'q', 'm', 'j', 'x', 'e', 'f', 'g', 'h', 'b', 's', 'w', 'k', 't',
    'o', 'u', 'y', 'z', 'a', 'p', 'n',
];

/// A `.` command.
///
/// Analysis cards are kept typed, using the shared [`AnalysisKind`] taxonomy;
/// everything else is either a known directive or an opaque [`DotCommand::Other`]
/// so that unknown commands survive into the parser's diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DotCommand {
    /// `.op`, `.dc`, `.ac`, `.tran`, …
    Analysis(AnalysisKind),
    /// `.model` — a model definition.
    Model,
    /// `.subckt` — start of a subcircuit definition.
    Subckt,
    /// `.ends` — end of a subcircuit definition.
    Ends,
    /// `.end` — end of the deck.
    End,
    /// `.include` — inline another file.
    Include,
    /// `.lib` — include a section of another file.
    Lib,
    /// `.endl` — end of a library section.
    Endl,
    /// `.options` / `.option`.
    Options,
    /// `.param` — a parameter assignment.
    Param,
    /// `.global` — promote node names out of subcircuits.
    Global,
    /// `.temp` — set the circuit temperature.
    Temp,
    /// `.ic` — initial conditions.
    Ic,
    /// `.nodeset` — initial guesses for the operating point.
    Nodeset,
    /// `.save` — restrict what the analyses keep.
    Save,
    /// `.print`
    Print,
    /// `.plot` / `.plot` command.
    Plot,
    /// `.measure` / `.meas`.
    Measure,
    /// `.control` — start of a command script section.
    Control,
    /// `.endc` — end of a command script section.
    Endc,
    /// `.if`
    If,
    /// `.else`
    Else,
    /// `.elseif`
    ElseIf,
    /// `.endif`
    EndIf,
    /// `.width`
    Width,
    /// `.backanno`
    BackAnno,
    /// `.data` — start of an inline data block.
    Data,
    /// `.enddata` — end of an inline data block.
    EndData,
    /// `.func`
    Func,
    /// `.step`
    Step,
    /// `.mc`
    MonteCarlo,
    /// `.alter`
    Alter,
    /// Anything else, lowercased and without the leading dot.
    Other(String),
}

impl DotCommand {
    /// Classifies a `.` command, with or without its dot, case-insensitively.
    #[must_use]
    pub fn parse(name: &str) -> Self {
        let name = name.strip_prefix('.').unwrap_or(name);
        if let Some(kind) = AnalysisKind::parse(name) {
            return Self::Analysis(kind);
        }
        match name.to_ascii_lowercase().as_str() {
            "model" => Self::Model,
            "subckt" => Self::Subckt,
            "ends" => Self::Ends,
            "end" => Self::End,
            "include" | "inc" => Self::Include,
            "lib" => Self::Lib,
            "endl" => Self::Endl,
            "options" | "option" | "opt" => Self::Options,
            "param" | "params" => Self::Param,
            "global" => Self::Global,
            "temp" => Self::Temp,
            "ic" => Self::Ic,
            "nodeset" => Self::Nodeset,
            "save" => Self::Save,
            "print" => Self::Print,
            "plot" => Self::Plot,
            "measure" | "meas" => Self::Measure,
            "control" => Self::Control,
            "endc" => Self::Endc,
            "if" => Self::If,
            "else" => Self::Else,
            "elseif" => Self::ElseIf,
            "endif" => Self::EndIf,
            "width" => Self::Width,
            "backanno" => Self::BackAnno,
            "data" => Self::Data,
            "enddata" => Self::EndData,
            "func" => Self::Func,
            "step" => Self::Step,
            "mc" => Self::MonteCarlo,
            "alter" => Self::Alter,
            other => Self::Other(other.to_owned()),
        }
    }

    /// True for the analysis cards.
    #[must_use]
    pub const fn is_analysis(&self) -> bool {
        matches!(self, Self::Analysis(_))
    }

    /// The analysis this card requests, if any.
    #[must_use]
    pub const fn analysis(&self) -> Option<AnalysisKind> {
        match self {
            Self::Analysis(kind) => Some(*kind),
            _ => None,
        }
    }

    /// The card name including its leading dot, e.g. `".tran"`.
    #[must_use]
    pub fn card_name(&self) -> String {
        match self {
            Self::Analysis(kind) => format!(".{}", kind.as_str()),
            Self::Other(name) => format!(".{name}"),
            other => format!(".{}", other.keyword()),
        }
    }

    /// The lowercased keyword without a dot. Only valid for the non-`Other`
    /// variants, and only used by [`DotCommand::card_name`].
    const fn keyword(&self) -> &'static str {
        match self {
            Self::Analysis(_) => "analysis",
            Self::Model => "model",
            Self::Subckt => "subckt",
            Self::Ends => "ends",
            Self::End => "end",
            Self::Include => "include",
            Self::Lib => "lib",
            Self::Endl => "endl",
            Self::Options => "options",
            Self::Param => "param",
            Self::Global => "global",
            Self::Temp => "temp",
            Self::Ic => "ic",
            Self::Nodeset => "nodeset",
            Self::Save => "save",
            Self::Print => "print",
            Self::Plot => "plot",
            Self::Measure => "measure",
            Self::Control => "control",
            Self::Endc => "endc",
            Self::If => "if",
            Self::Else => "else",
            Self::ElseIf => "elseif",
            Self::EndIf => "endif",
            Self::Width => "width",
            Self::BackAnno => "backanno",
            Self::Data => "data",
            Self::EndData => "enddata",
            Self::Func => "func",
            Self::Step => "step",
            Self::MonteCarlo => "mc",
            Self::Alter => "alter",
            Self::Other(_) => "other",
        }
    }
}

/// What a card is, as far as the first token can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardKind {
    /// A device instance, keyed by its designator letter.
    Device {
        /// The designator letter, lowercased.
        designator: char,
    },
    /// A `.` command.
    DotCommand(DotCommand),
    /// Neither: the deck starts with something that is not a device letter or a
    /// dot command. This is not necessarily an error — `.` commands in a
    /// `.control` section and comment-like directives land here.
    Unknown,
}

impl CardKind {
    /// Classifies a first token.
    #[must_use]
    pub fn classify(first: &Token) -> Self {
        if first.text.starts_with('.') {
            return Self::DotCommand(DotCommand::parse(&first.text));
        }
        let mut characters = first.text.chars();
        match characters.next() {
            Some(letter) if letter.is_ascii_alphabetic() => {
                let designator = letter.to_ascii_lowercase();
                if DEVICE_DESIGNATORS.contains(&designator) {
                    Self::Device { designator }
                } else {
                    Self::Unknown
                }
            }
            _ => Self::Unknown,
        }
    }

    /// True for a `.` command.
    #[must_use]
    pub const fn is_dot_command(&self) -> bool {
        matches!(self, Self::DotCommand(_))
    }

    /// True for a device instance.
    #[must_use]
    pub const fn is_device(&self) -> bool {
        matches!(self, Self::Device { .. })
    }
}

/// A logical card with its token stream and coarse classification.
#[derive(Debug, Clone, PartialEq)]
pub struct RawCard {
    /// Where the card starts.
    pub location: SourceLoc,
    /// The joined card text.
    pub raw: String,
    /// The card's tokens, in order.
    pub tokens: Vec<Token>,
    /// What the first token says the card is.
    pub kind: CardKind,
}

impl RawCard {
    /// Tokenizes a logical card and classifies it.
    ///
    /// # Errors
    ///
    /// Propagates tokenizer errors, for instance an unterminated `{` expression.
    pub fn parse(line: &LogicalLine) -> SpiceResult<Self> {
        let tokens = tokenize(line)?;
        let kind = match tokens.first() {
            Some(first) => CardKind::classify(first),
            None => CardKind::Unknown,
        };
        Ok(Self {
            location: line.location.clone(),
            raw: line.text.clone(),
            tokens,
            kind,
        })
    }

    /// The card's first token.
    #[must_use]
    pub fn first_token(&self) -> Option<&Token> {
        self.tokens.first()
    }

    /// Every token except the first.
    #[must_use]
    pub fn arguments(&self) -> &[Token] {
        self.tokens.get(1..).unwrap_or_default()
    }

    /// The designator letter, for a device card.
    #[must_use]
    pub const fn designator(&self) -> Option<char> {
        match self.kind {
            CardKind::Device { designator } => Some(designator),
            _ => None,
        }
    }

    /// The `.` command, for a directive card.
    #[must_use]
    pub const fn dot_command(&self) -> Option<&DotCommand> {
        match &self.kind {
            CardKind::DotCommand(command) => Some(command),
            _ => None,
        }
    }

    /// The analysis this card requests, if any.
    #[must_use]
    pub fn analysis(&self) -> Option<AnalysisKind> {
        self.dot_command().and_then(DotCommand::analysis)
    }
}

#[cfg(test)]
mod tests {
    use super::{CardKind, DotCommand, RawCard};
    use crate::netlist::source::parse_deck_text;
    use crate::primitives::AnalysisKind;
    use std::path::Path;

    fn card(text: &str) -> RawCard {
        let deck = parse_deck_text(Path::new("test.cir"), &format!("title\n{text}\n"));
        RawCard::parse(&deck.lines[0]).expect("card tokenizes")
    }

    #[test]
    fn devices_are_recognised_by_designator() {
        assert_eq!(card("r1 a b 1k").kind, CardKind::Device { designator: 'r' });
        assert_eq!(
            card("XSUB a b sub").kind,
            CardKind::Device { designator: 'x' }
        );
        assert_eq!(card("r1 a b 1k").designator(), Some('r'));
    }

    #[test]
    fn analysis_cards_carry_their_kind() {
        let card = card(".tran 1u 10u");
        assert_eq!(
            card.dot_command(),
            Some(&DotCommand::Analysis(AnalysisKind::Transient))
        );
        assert_eq!(card.analysis(), Some(AnalysisKind::Transient));
        assert_eq!(card.arguments().len(), 2);
    }

    #[test]
    fn known_directives_are_typed() {
        assert_eq!(card(".MODEL d d").dot_command(), Some(&DotCommand::Model));
        assert_eq!(
            card(".subckt amp in out").dot_command(),
            Some(&DotCommand::Subckt)
        );
        assert_eq!(card(".ends amp").dot_command(), Some(&DotCommand::Ends));
        assert_eq!(
            card(".include \"x.cir\"").dot_command(),
            Some(&DotCommand::Include)
        );
        assert_eq!(
            card(".options reltol=1e-4").dot_command(),
            Some(&DotCommand::Options)
        );
    }

    #[test]
    fn unknown_directives_are_preserved() {
        let command = card(".frobnicate now").dot_command().cloned();
        assert_eq!(command, Some(DotCommand::Other("frobnicate".to_owned())));
        assert_eq!(
            command.expect("command").card_name(),
            ".frobnicate".to_owned()
        );
    }

    #[test]
    fn card_names_round_trip_through_parse() {
        for name in [".tran", ".op", ".model", ".ends", ".endc", ".measure"] {
            let command = DotCommand::parse(name);
            assert_eq!(command.card_name(), name);
            assert_eq!(DotCommand::parse(&command.card_name()), command);
        }
    }

    #[test]
    fn unrecognised_first_tokens_are_unknown() {
        assert_eq!(card("?what").kind, CardKind::Unknown);
        assert_eq!(card("!bang").kind, CardKind::Unknown);
        assert_eq!(card("42 7").kind, CardKind::Unknown);
    }

    #[test]
    fn non_analysis_directives_are_not_analyses() {
        assert!(card(".model d d").kind.is_dot_command());
        assert_eq!(card(".model d d").analysis(), None);
        assert!(!DotCommand::Model.is_analysis());
        assert!(DotCommand::Analysis(AnalysisKind::Ac).is_analysis());
    }
}
