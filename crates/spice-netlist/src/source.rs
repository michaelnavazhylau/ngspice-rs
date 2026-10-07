//! Deck loading: physical lines become logical cards.
//!
//! Ported from:
//!
//! - `src/frontend/inp.c`, `inp_readall()` — the title line, comment-line
//!   detection and `.control` section tracking.
//! - `src/frontend/inpcom.c`, `inp_stripcomments_line()` — end-of-line
//!   comments, and the whitespace trimming that goes with them.
//!
//! # What a logical card is
//!
//! A SPICE deck is a sequence of cards, each of which may be spread over several
//! physical lines. Continuation lines start with `+`. The first line of the deck
//! is always the title, even if it looks like a card or a comment.
//!
//! Comments may appear between the continuation lines of a card, and a blank
//! line terminates the card being built.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use spice_core::{SourceLoc, SpiceError, SpiceResult};

/// One physical line of a deck, with its 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalLine {
    /// 1-based line number in the file.
    pub number: u32,
    /// The line with its terminator removed.
    pub text: String,
}

/// One card: continuation lines joined, comments removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalLine {
    /// Where the card starts.
    pub location: SourceLoc,
    /// 1-based line number of the last physical line folded into this card.
    pub end_line: u32,
    /// Number of `+` continuation lines folded into this card.
    pub continuations: u32,
    /// The joined card text, free of comments and trailing whitespace.
    pub text: String,
}

impl LogicalLine {
    /// The card text with its first token removed, as ngspice's callers do.
    #[must_use]
    pub fn body(&self) -> &str {
        match self.text.split_once(char::is_whitespace) {
            Some((_, body)) => body.trim_start(),
            None => "",
        }
    }
}

/// A loaded deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    /// The file this deck came from.
    pub path: PathBuf,
    /// The first line of the deck. Always the title, even when it looks like a card.
    pub title: String,
    /// Where the title line is.
    pub title_location: SourceLoc,
    /// The cards, in order, with comments and blank lines removed.
    pub lines: Vec<LogicalLine>,
}

impl Deck {
    /// The number of cards, excluding the title.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// True when the deck has no cards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Reads a deck from disk.
pub fn load(path: impl AsRef<Path>) -> SpiceResult<Deck> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|error| SpiceError::io(path, &error))?;
    Ok(parse_deck_text(path, &text))
}

/// Splits text into physical lines, numbering from 1.
///
/// `str::lines()` handles both `\n` and `\r\n`; a trailing newline does not
/// produce a final empty line.
#[must_use]
pub fn split_physical_lines(text: &str) -> Vec<PhysicalLine> {
    text.lines()
        .enumerate()
        .map(|(index, line)| PhysicalLine {
            number: u32::try_from(index + 1).unwrap_or(u32::MAX),
            text: line.to_owned(),
        })
        .collect()
}

/// True when this physical line continues the previous card.
///
/// ngspice tests the first non-whitespace character for `+`.
#[must_use]
pub fn is_continuation(text: &str) -> bool {
    text.trim_start().starts_with('+')
}

/// True when this line is a whole-line comment.
///
/// From `inp_readall()`: the first non-whitespace character is `*`, except for
/// the special directive `*#`, which is only a directive when `*` is in column 1.
/// A leading `#` is also a comment, as `inp_stripcomments_line()` converts it to
/// the normal `*` form.
#[must_use]
pub fn is_comment_line(text: &str) -> bool {
    let trimmed = text.trim_start();
    if text.starts_with("*#") {
        return false;
    }
    trimmed.starts_with('*') || trimmed.starts_with('#')
}

/// Removes an end-of-line comment, returning the retained text.
///
/// Ported from `inp_stripcomments_line()`. The delimiters are:
///
/// - `;` anywhere outside quotes
/// - `//` anywhere outside quotes
/// - `$` when it starts a token — at the start of the line, or preceded by a
///   space, tab or comma — outside a `.control` section
/// - `$` followed by a space inside a `.control` section
///
/// Quoted strings (`"…"`, `'…'`) are skipped, with `\` escaping the closing
/// quote. Whitespace before the comment is trimmed, matching the C code's
/// "eat white space at new end of line" step.
///
/// The PS/LTPS compatibility mode, in which `$` is an ordinary character, is not
/// modelled; the port implements the default mode.
#[must_use]
pub fn strip_comment(text: &str, in_control: bool) -> &str {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' | b'\'' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index += 2;
                        continue;
                    }
                    if bytes[index] == quote {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            b';' => return text[..index].trim_end(),
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                return text[..index].trim_end();
            }
            b'$' => {
                let at_token_start = index == 0 || matches!(bytes[index - 1], b' ' | b'\t' | b',');
                let is_comment = if in_control {
                    matches!(bytes.get(index + 1), None | Some(b' '))
                } else {
                    at_token_start
                };
                if is_comment {
                    return text[..index].trim_end();
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    text.trim_end()
}

/// Folds physical lines into a [`Deck`].
#[must_use]
pub fn fold_logical_lines(file: &Path, lines: &[PhysicalLine]) -> Deck {
    let file: Arc<PathBuf> = Arc::new(file.to_path_buf());
    let mut deck = Deck {
        path: file.as_ref().clone(),
        title: String::new(),
        title_location: SourceLoc::new(file.clone(), 1, 1),
        lines: Vec::new(),
    };

    let Some((first, rest)) = lines.split_first() else {
        return deck;
    };
    deck.title = first.text.trim_end().to_owned();

    let mut pending: Option<LogicalLine> = None;
    let mut in_control = false;

    for line in rest {
        let begins_control = line
            .text
            .trim_start()
            .to_ascii_lowercase()
            .starts_with(".control");
        let begins_endc = line
            .text
            .trim_start()
            .to_ascii_lowercase()
            .starts_with(".endc");

        if is_comment_line(&line.text) {
            // Comments may sit between the continuation lines of a card, so a
            // comment never terminates the card being built.
            continue;
        }

        if is_continuation(&line.text) {
            let text = strip_comment(&line.text, in_control);
            let body = text.trim_start().trim_start_matches('+').trim();
            if let Some(card) = pending.as_mut() {
                if !body.is_empty() {
                    card.text.push(' ');
                    card.text.push_str(body);
                }
                card.continuations += 1;
                card.end_line = line.number;
            } else {
                // ngspice would reject this while building the card list; there is
                // no card to attach it to.
                deck.lines.push(LogicalLine {
                    location: SourceLoc::new(file.clone(), line.number, 1),
                    end_line: line.number,
                    continuations: 0,
                    text: body.to_owned(),
                });
            }
            continue;
        }

        let text = strip_comment(&line.text, in_control);
        let text = text.trim();
        if text.is_empty() {
            // A blank line (or a line holding nothing but a comment) ends the
            // current card.
            if let Some(card) = pending.take() {
                deck.lines.push(card);
            }
        } else {
            if let Some(card) = pending.take() {
                deck.lines.push(card);
            }
            pending = Some(LogicalLine {
                location: SourceLoc::new(file.clone(), line.number, 1),
                end_line: line.number,
                continuations: 0,
                text: text.to_owned(),
            });
        }

        if begins_control {
            in_control = true;
        } else if begins_endc {
            in_control = false;
        }
    }

    if let Some(card) = pending.take() {
        deck.lines.push(card);
    }
    deck
}

/// Splits `text` into a [`Deck`], as if it had been read from `file`.
#[must_use]
pub fn parse_deck_text(file: &Path, text: &str) -> Deck {
    fold_logical_lines(file, &split_physical_lines(text))
}

/// Folds an included source fragment without treating its first line as a
/// title. Physical line numbers and continuation/comment rules are unchanged.
#[must_use]
pub fn parse_fragment_text(file: &Path, text: &str) -> Deck {
    let mut lines = split_physical_lines(text);
    lines.insert(
        0,
        PhysicalLine {
            number: 0,
            text: String::new(),
        },
    );
    fold_logical_lines(file, &lines)
}

#[cfg(test)]
mod tests {
    use super::{is_comment_line, is_continuation, parse_deck_text, strip_comment};
    use std::path::Path;

    fn deck(text: &str) -> super::Deck {
        parse_deck_text(Path::new("test.cir"), text)
    }

    #[test]
    fn first_line_is_the_title_even_when_it_looks_like_a_card() {
        let deck = deck("* not a comment\nr1 1 0 1k\n.end\n");
        assert_eq!(deck.title, "* not a comment");
        assert_eq!(deck.lines.len(), 2);
        assert_eq!(deck.lines[0].text, "r1 1 0 1k");
    }

    #[test]
    fn empty_deck_has_an_empty_title() {
        let deck = deck("");
        assert_eq!(deck.title, "");
        assert!(deck.is_empty());
        assert_eq!(deck.title_location.line, 1);
    }

    #[test]
    fn continuation_lines_are_joined() {
        let deck = deck(
            "divider\n\
             r1 in out 1k\n\
             + tc1=0.01 tc2=0.02\n\
             + temp=27\n\
             .end\n",
        );
        assert_eq!(deck.lines[0].text, "r1 in out 1k tc1=0.01 tc2=0.02 temp=27");
        assert_eq!(deck.lines[0].continuations, 2);
        assert_eq!(deck.lines[0].location.line, 2);
        assert_eq!(deck.lines[0].end_line, 4);
        assert_eq!(deck.lines[1].text, ".end");
    }

    #[test]
    fn comments_between_continuations_do_not_break_the_card() {
        let deck = deck("t\nr1 1 0\n* a comment\n+ 1k\n.end\n");
        assert_eq!(deck.lines[0].text, "r1 1 0 1k");
    }

    #[test]
    fn blank_line_terminates_a_card() {
        let deck = deck("t\nr1 1 0\n\n+ 1k\n");
        assert_eq!(deck.lines.len(), 2);
        assert_eq!(deck.lines[0].text, "r1 1 0");
        // The stray continuation becomes its own card rather than being dropped.
        assert_eq!(deck.lines[1].text, "1k");
    }

    #[test]
    fn comment_line_detection_matches_inp_readall() {
        assert!(is_comment_line("* comment"));
        assert!(is_comment_line("   * indented comment"));
        assert!(is_comment_line("# comment"));
        assert!(!is_comment_line("*# directive"));
        assert!(!is_comment_line("r1 1 0 1k"));
        assert!(is_continuation("  + 1k"));
        assert!(!is_continuation("r1 1 0"));
    }

    #[test]
    fn end_of_line_comments_are_stripped() {
        assert_eq!(strip_comment("r1 1 0 1k ; a resistor", false), "r1 1 0 1k");
        assert_eq!(strip_comment("r1 1 0 1k //comment", false), "r1 1 0 1k");
        assert_eq!(strip_comment("r1 1 0 1k $ tail", false), "r1 1 0 1k");
        assert_eq!(strip_comment("r1 1 0 1k", false), "r1 1 0 1k");
        assert_eq!(strip_comment("r1 1 0 1k   ", false), "r1 1 0 1k");
    }

    #[test]
    fn dollar_needs_a_token_boundary_outside_control_sections() {
        assert_eq!(strip_comment("v1 1 0 dc$x$", false), "v1 1 0 dc$x$");
        assert_eq!(strip_comment("v1 1 0 dc $ x$", false), "v1 1 0 dc");
        assert_eq!(strip_comment("$ whole line", false), "");
    }

    #[test]
    fn dollar_inside_control_requires_a_following_space() {
        assert_eq!(strip_comment("echo $HOME", true), "echo $HOME");
        assert_eq!(strip_comment("echo $ HOME", true), "echo");
    }

    #[test]
    fn quoted_strings_hide_comment_characters() {
        assert_eq!(
            strip_comment(r#"title "a ; b" ; real comment"#, false),
            r#"title "a ; b""#
        );
        assert_eq!(strip_comment("'a;b' ; c", false), "'a;b'");
        assert_eq!(strip_comment(r#""a \"; b" ; c"#, false), r#""a \"; b""#);
    }

    #[test]
    fn deck_body_helper_drops_the_first_token() {
        let deck = deck("t\n.model d d is=1e-14\n");
        assert_eq!(deck.lines[0].body(), "d d is=1e-14");
    }
}
