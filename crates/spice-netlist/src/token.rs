//! Tokenizing a logical card.
//!
//! The C front end has no separate tokenizer: `src/spicelib/parser/inp2*.c`
//! functions walk the card text with `INPgetTok()` / `INPgetNetTok()` from
//! `src/spicelib/parser/inpgtok.c` as they parse. The port separates the two
//! steps so that the parser can be written against a positioned token stream.
//!
//! Delimiters are whitespace, `(`, `)`, `,`, `=` and the quote characters. That
//! means a **numeric node name becomes a [`TokenKind::Number`] token**, and
//! `.model d d(is=1e-14)` becomes the same sequence of tokens a human would
//! expect — but the parser must accept a `Number` wherever a name is legal,
//! because `0` is the ground node. The [`Token::text`] field always holds the
//! original spelling, so the parser can rely on it when the classification is
//! merely a hint.

use std::fmt;

use spice_core::{Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number};

use crate::source::LogicalLine;

/// What kind of token this is.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An identifier, node name, model name or keyword.
    Word,
    /// A numeric literal. [`Token::text`] keeps the original spelling, which may
    /// carry a scale suffix (`1k`) and a trailing unit name (`5V`).
    Number(Real),
    /// A `{ ... }` numparam expression, kept verbatim and unparsed.
    Expression(String),
    /// A quoted string, with its quotes removed and escapes resolved.
    Quoted(String),
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `=`
    Equals,
}

impl TokenKind {
    /// A short human-readable name, for diagnostics and the CLI's token dump.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Number(_) => "number",
            Self::Expression(_) => "expression",
            Self::Quoted(_) => "quoted",
            Self::LParen => "'('",
            Self::RParen => "')'",
            Self::Comma => "','",
            Self::Equals => "'='",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One lexical token, with its position in the (already joined) card.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The token's classification.
    pub kind: TokenKind,
    /// The exact source spelling.
    pub text: String,
    /// Where the token starts. On a card that was joined from continuation
    /// lines, the column is relative to the joined text, not to the original
    /// file line.
    pub location: SourceLoc,
}

impl Token {
    /// Builds a token.
    #[must_use]
    pub fn new(kind: TokenKind, text: impl Into<String>, location: SourceLoc) -> Self {
        Self {
            kind,
            text: text.into(),
            location,
        }
    }

    /// The value, when this token is a number.
    #[must_use]
    pub fn number(&self) -> Option<Real> {
        match self.kind {
            TokenKind::Number(value) => Some(value),
            _ => None,
        }
    }

    /// Compares the token's spelling to a keyword, case-insensitively.
    #[must_use]
    pub fn is_keyword(&self, keyword: &str) -> bool {
        self.text.eq_ignore_ascii_case(keyword)
    }

    /// True when the token is a word or a number, i.e. something that can be
    /// used as a name.
    #[must_use]
    pub fn is_name_like(&self) -> bool {
        matches!(self.kind, TokenKind::Word | TokenKind::Number(_))
    }
}

/// Characters that end a bare word.
const DELIMITERS: &[u8] = b"(),={}\"'";

fn char_at(text: &str, index: usize) -> char {
    text[index..]
        .chars()
        .next()
        .expect("index is on a character boundary")
}

/// Splits a logical card into tokens.
///
/// # Errors
///
/// Returns [`SpiceError::Parse`] for an unterminated `{` expression or an
/// unterminated quoted string.
pub fn tokenize(line: &LogicalLine) -> SpiceResult<Vec<Token>> {
    let text = line.text.as_str();
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }

        let start = index;
        let location = line
            .location
            .at_column(u32::try_from(start).unwrap_or(u32::MAX) + 1);

        match bytes[index] {
            b'(' => {
                tokens.push(Token::new(TokenKind::LParen, "(", location));
                index += 1;
            }
            b')' => {
                tokens.push(Token::new(TokenKind::RParen, ")", location));
                index += 1;
            }
            b',' => {
                tokens.push(Token::new(TokenKind::Comma, ",", location));
                index += 1;
            }
            b'=' => {
                tokens.push(Token::new(TokenKind::Equals, "=", location));
                index += 1;
            }
            b'{' => {
                let Some(end) = find_closing_brace(bytes, index) else {
                    return Err(SpiceError::parse(
                        location,
                        "unterminated '{' expression on this card",
                    ));
                };
                let inner = text[index + 1..end].trim().to_owned();
                tokens.push(Token::new(
                    TokenKind::Expression(inner),
                    &text[index..=end],
                    location,
                ));
                index = end + 1;
            }
            quote @ (b'"' | b'\'') => {
                let quote = quote as char;
                let mut cursor = index + 1;
                let mut value = String::new();
                let mut closed = false;
                while cursor < text.len() {
                    let character = char_at(text, cursor);
                    if character == '\\' && cursor + 1 < text.len() {
                        let escaped_at = cursor + 1;
                        let escaped = char_at(text, escaped_at);
                        value.push(escaped);
                        cursor = escaped_at + escaped.len_utf8();
                        continue;
                    }
                    cursor += character.len_utf8();
                    if character == quote {
                        closed = true;
                        break;
                    }
                    value.push(character);
                }
                if !closed {
                    return Err(SpiceError::parse(
                        location,
                        format!("unterminated {quote} quoted string on this card"),
                    ));
                }
                tokens.push(Token::new(
                    TokenKind::Quoted(value),
                    &text[index..cursor],
                    location,
                ));
                index = cursor;
            }
            _ => {
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !DELIMITERS.contains(&bytes[index])
                {
                    index += 1;
                }
                let word = &text[start..index];
                let kind = match parse_spice_number(word) {
                    Some(value) => TokenKind::Number(value),
                    None => TokenKind::Word,
                };
                tokens.push(Token::new(kind, word, location));
            }
        }
    }

    Ok(tokens)
}

/// Finds the `}` matching the `{` at `open`, accounting for nesting.
fn find_closing_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{TokenKind, tokenize};
    use crate::source::parse_deck_text;
    use std::path::Path;

    fn tokens(text: &str) -> Vec<super::Token> {
        let deck = parse_deck_text(Path::new("test.cir"), &format!("title\n{text}\n"));
        tokenize(&deck.lines[0]).expect("card tokenizes")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        tokens(text).into_iter().map(|token| token.kind).collect()
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(
            kinds("r1 in out 1k"),
            vec![
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Number(1000.0),
            ]
        );
    }

    #[test]
    fn numbers_keep_their_spelling() {
        let tokens = tokens("r1 in out 1kohm");
        assert_eq!(tokens[3].kind, TokenKind::Number(1000.0));
        assert_eq!(tokens[3].text, "1kohm");
    }

    #[test]
    fn numeric_node_names_become_number_tokens() {
        // `0` is the ground node; device letters and node names that look like
        // numbers must still be usable by the parser.
        assert_eq!(
            kinds("v1 1 0 dc 5"),
            vec![
                TokenKind::Word,
                TokenKind::Number(1.0),
                TokenKind::Number(0.0),
                TokenKind::Word,
                TokenKind::Number(5.0),
            ]
        );
    }

    #[test]
    fn part_numbers_stay_words() {
        // `2N2222` is not `2e-9` followed by junk, so it is not a number.
        assert_eq!(
            kinds("q1 c b 0 2N2222"),
            vec![
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Number(0.0),
                TokenKind::Word,
            ]
        );
    }

    #[test]
    fn punctuation_is_split_out() {
        assert_eq!(
            kinds(".model d d(is=1e-14)"),
            vec![
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::LParen,
                TokenKind::Word,
                TokenKind::Equals,
                TokenKind::Number(1e-14),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn braced_expressions_are_one_token() {
        let tokens = tokens("r1 a b {r2*2}");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[3].kind, TokenKind::Expression("r2*2".to_owned()));
        assert_eq!(tokens[3].text, "{r2*2}");
    }

    #[test]
    fn nested_braces_are_matched() {
        let tokens = tokens("r1 a b {max(1,{x})}");
        assert_eq!(
            tokens[3].kind,
            TokenKind::Expression("max(1,{x})".to_owned())
        );
    }

    #[test]
    fn quoted_strings_are_unwrapped() {
        let tokens = tokens(r#"title "a b" 'c d'"#);
        assert_eq!(tokens[1].kind, TokenKind::Quoted("a b".to_owned()));
        assert_eq!(tokens[1].text, "\"a b\"");
        assert_eq!(tokens[2].kind, TokenKind::Quoted("c d".to_owned()));
    }

    #[test]
    fn escaped_quotes_survive() {
        let tokens = tokens(r#""a \" b""#);
        assert_eq!(tokens[0].kind, TokenKind::Quoted("a \" b".to_owned()));
    }

    #[test]
    fn columns_are_one_based() {
        let tokens = tokens("r1 in out 1k");
        assert_eq!(tokens[0].location.column, 1);
        assert_eq!(tokens[1].location.column, 4);
        assert_eq!(tokens[3].location.column, 11);
        assert_eq!(tokens[0].location.line, 2);
    }

    #[test]
    fn unterminated_constructs_are_errors() {
        assert!(tokenize(&card("{r2*2")).is_err());
        assert!(tokenize(&card("\"oops")).is_err());
    }

    fn card(text: &str) -> crate::source::LogicalLine {
        let deck = parse_deck_text(Path::new("test.cir"), &format!("title\n{text}\n"));
        deck.lines[0].clone()
    }

    #[test]
    fn keyword_comparison_is_case_insensitive() {
        let tokens = tokens("R1 in out 1k");
        assert!(tokens[0].is_keyword("r1"));
        assert!(tokens[0].is_name_like());
        assert!(!tokens[3].is_keyword("r1"));
    }
}
