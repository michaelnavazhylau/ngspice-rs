//! Linear controlled-source grammars: `src/spicelib/parser/inp2{e,f,g,h}.c`.
//!
//! ```text
//! Ename n+ n- [vcvs] nc+ nc- gain          VCVS (vcvs/vcvs.c)
//! Gname n+ n- [vccs] nc+ nc- gain [m=..]   VCCS (vccs/vccs.c)
//! Fname n+ n- [cccs] vname   gain [m=..]   CCCS (cccs/cccs.c)
//! Hname n+ n- [ccvs] vname   gain          CCVS (ccvs/ccvs.c)
//! ```
//!
//! C-accepted variants of the linear form:
//!
//! - Node and controlling-source names may be wrapped in `(`, `)` and `,`
//!   (`e1 out 0 (in,0) 10`, `g1 0 o (in) (0) 1m`): `INPgetNetTok()` and
//!   `INPgetTok()` skip those characters before a token, and `INPevaluate()`
//!   skips a stray `)` before the gain.
//! - `inp_compat()` (`src/frontend/inpcom.c`) deletes the HSPICE keyword
//!   `vcvs`/`vccs`/`cccs`/`ccvs` when it is the fourth whitespace-separated
//!   token of a card with exactly seven (E/G) or six (F/H) such tokens.
//!   Otherwise the word is an ordinary node name, as in C.
//! - The gain is the leading value (a number or `{expression}`) or a named
//!   `gain=value`. C applies the leading value *after* the named setters
//!   (`INPdevParse` then `GCA(INPpName, "gain")`), so a leading gain is stored
//!   last; F/H's controlling source (`control`, C `IF_INSTANCE`) is set before
//!   `INPdevParse` and is stored first.
//! - G/F additionally accept `m=` (and further `gain=`/`m=` setters) after the
//!   gain: `inp_check_syntax()` lets an `m=` tail through and `INPdevParse`
//!   applies the setters in order. `VCCSparam`/`CCCSparam` multiply a gain by
//!   `m` only if `m` was already given, so the order is semantic.
//!
//! The nonlinear forms (`POLY(n)`, `VALUE=`/`VOL=`/`CUR=`, `TABLE`, and the
//! implicit spice2g6 one-dimensional polynomial that `inp_poly_2g6_compat()`
//! creates when more values follow the gain) are parsed by
//! [`super::behavioural`] (#79) and lowered onto B sources by
//! [`crate::behavioural`]. `LAPLACE` is reported as
//! [`spice_core::SpiceError::NotYetPorted`]. A card without a gain is a parse
//! error ("not enough parameters" in C); a card whose first value after the
//! controls is `m=` is rejected too, although C would silently build a
//! zero-gain source (a documented divergence).

use spice_core::SpiceError;
use winnow::Parser as _;
use winnow::combinator::{cut_err, opt, peek, repeat};
use winnow::error::ErrMode;
use winnow::token::any;

use crate::ast::{DeviceInstance, ParameterAssignment, ParameterKind};
use crate::token::{Token, TokenKind};

use super::behavioural;
use super::grammar::{Failure, Input, ParsedCard, Result, location};
use super::syntax::{assignment, canonical_node, equals, leading_value, name, named, value};

/// C reference for the still unported LAPLACE rewrite.
const NONLINEAR_REFERENCE: &str =
    "src/frontend/inpcom.c (inp_compat); src/xspice/icm/xtradev (s_xfer, LAPLACE)";

pub(super) fn controlled_card(input: &mut Input<'_>) -> Result<ParsedCard> {
    let instance = any
        .verify(|token: &Token| token.is_name_like() && controlled_designator(token).is_some())
        .parse_next(input)?;
    let designator = controlled_designator(instance).expect("verified designator");
    // The designator matched: every failure below is definitive.
    cut_err(move |input: &mut Input<'_>| body(input, instance, designator)).parse_next(input)
}

fn controlled_designator(token: &Token) -> Option<char> {
    let designator = token.text.chars().next()?.to_ascii_lowercase();
    matches!(designator, 'e' | 'f' | 'g' | 'h').then_some(designator)
}

fn body(input: &mut Input<'_>, instance: &Token, designator: char) -> Result<ParsedCard> {
    let auto_gnd = input.state.auto_gnd;
    let positive = terminal(input, "positive output terminal")?;
    let negative = terminal(input, "negative output terminal")?;
    let mut nodes = vec![
        canonical_node(positive, auto_gnd),
        canonical_node(negative, auto_gnd),
    ];
    let device = |nodes: Vec<String>, parameters: Vec<ParameterAssignment>, input: &Input<'_>| {
        ParsedCard::Device(DeviceInstance {
            name: instance.text.to_ascii_lowercase(),
            designator,
            nodes,
            model: None,
            parameters,
            location: input.state.card.location.clone(),
        })
    };
    if let Some(parameters) = nonlinear_form(input, designator, &mut nodes)? {
        return Ok(device(nodes, parameters, input));
    }
    hspice_keyword(input, designator)?;
    let mut parameters = Vec::new();
    if matches!(designator, 'e' | 'g') {
        let control_positive = terminal(input, "positive controlling node")?;
        let control_negative = terminal(input, "negative controlling node")?;
        nodes.push(canonical_node(control_positive, auto_gnd));
        nodes.push(canonical_node(control_negative, auto_gnd));
        if input.input.first().is_some_and(|t| t.is_keyword("table")) {
            if designator == 'g' {
                return Err(parse_error(
                    input,
                    "a four-node G TABLE is not accepted (C: bad syntax; only the LTspice \
                     E form Ename n+ n- nc+ nc- table=(...) exists)",
                ));
            }
            let parameters = behavioural::table_form(input, designator, true)?;
            return Ok(device(nodes, parameters, input));
        }
    } else {
        let control = terminal(input, "controlling voltage source name")?;
        parameters.push(ParameterAssignment {
            name: "control".to_owned(),
            value: control.text.to_ascii_lowercase(),
            kind: ParameterKind::Instance,
            location: control.location.clone(),
        });
    }
    // INPevaluate()/INPgetTok() skip a closing parenthesis or comma left over
    // from a parenthesized node list.
    let _: () = repeat(0.., closing_punctuation).parse_next(input)?;
    if implicit_poly(input) {
        // inp_poly_2g6_compat(): more values after the gain make a spice2g6
        // one-dimensional polynomial, the gain being its first coefficient.
        let location = input.input[0].location.clone();
        parameters.insert(
            0,
            ParameterAssignment {
                name: "poly".to_owned(),
                value: "1".to_owned(),
                kind: ParameterKind::Scalar,
                location,
            },
        );
        behavioural::coefficients(input, designator, &mut parameters)?;
        return Ok(device(nodes, parameters, input));
    }
    let (leading, setters) = gain_and_setters(input, designator)?;
    parameters.extend(setters);
    if let Some(gain) = leading {
        parameters.push(gain);
    }
    Ok(device(nodes, parameters, input))
}

/// The gain is followed by another token that is neither `m=` nor `ic=`.
fn implicit_poly(input: &Input<'_>) -> bool {
    let tokens = &input.input;
    let Some(first) = tokens.first() else {
        return false;
    };
    if !(first.number().is_some() || super::expression::is_expression_token(first)) {
        return false;
    }
    let Some(second) = tokens.get(1) else {
        return false;
    };
    let named = |keyword: &str| {
        second.is_keyword(keyword) && tokens.get(2).is_some_and(|t| t.kind == TokenKind::Equals)
    };
    let setter_tail = named("m")
        || named("ic")
        || named("gain")
        || matches!(second.kind, TokenKind::Equals)
        || second.text.to_ascii_lowercase().starts_with("sens_")
        || second.is_keyword("control");
    !setter_tail
}

/// The fourth-token forms that `inpcom.c` turns into B sources or XSPICE
/// models before `INP2E`..`INP2H` ever see the card. Returns their
/// parameters (and extends `nodes` with POLY controlling nodes).
fn nonlinear_form(
    input: &mut Input<'_>,
    designator: char,
    nodes: &mut Vec<String>,
) -> Result<Option<Vec<ParameterAssignment>>> {
    let Some(token) = input.input.first() else {
        return Ok(None);
    };
    if token.kind != TokenKind::Word {
        return Ok(None);
    }
    let named = input
        .input
        .get(1)
        .is_some_and(|next| next.kind == TokenKind::Equals);
    let lowered = token.text.to_ascii_lowercase();
    match lowered.as_str() {
        "poly" => behavioural::poly_form(input, designator, nodes).map(Some),
        "value" | "vol" | "cur" if named => {
            let accepted = match designator {
                'e' => lowered != "cur",
                'g' => lowered != "vol",
                _ => false,
            };
            if !accepted {
                return Err(parse_error(
                    input,
                    &format!(
                        "'{lowered}=' is not a form of a '{designator}' source (C: E accepts \
                         VALUE=/VOL=, G accepts VALUE=/CUR=)"
                    ),
                ));
            }
            let keyword = token.clone();
            behavioural::value_form(input, designator, &keyword).map(Some)
        }
        "table" if matches!(designator, 'e' | 'g') => {
            behavioural::table_form(input, designator, false).map(Some)
        }
        "laplace" => Err(nonlinear(input, "LAPLACE controlled sources")),
        _ => Ok(None),
    }
}

/// A node or instance name, after any `(`, `)` or `,` that C's token readers
/// skip.
fn terminal<'a>(input: &mut Input<'a>, expected: &'static str) -> Result<&'a Token> {
    let _: () = repeat(0.., punctuation).parse_next(input)?;
    name(expected).parse_next(input)
}

fn punctuation<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|token: &Token| {
        matches!(
            token.kind,
            TokenKind::LParen | TokenKind::RParen | TokenKind::Comma
        )
    })
    .parse_next(input)
}

fn closing_punctuation<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    any.verify(|token: &Token| matches!(token.kind, TokenKind::RParen | TokenKind::Comma))
        .parse_next(input)
}

fn nonlinear(input: &Input<'_>, what: &str) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::not_yet_ported(
        format!(
            "{}: {what} (only the linear gain form is ported)",
            location(input)
        ),
        NONLINEAR_REFERENCE,
    )))
}

/// `inp_compat()`: `replace_token(line, "vcvs", 4, 7)` and friends. The
/// keyword is removed only at the fourth whitespace-separated token of a card
/// with exactly the expected token count; everywhere else it is a node name.
fn hspice_keyword(input: &mut Input<'_>, designator: char) -> Result<()> {
    let (keyword, total) = match designator {
        'e' => ("vcvs", 7),
        'g' => ("vccs", 7),
        'f' => ("cccs", 6),
        _ => ("ccvs", 6),
    };
    let compacted = remove_ws(&input.state.card.raw);
    let words: Vec<&str> = compacted.split_whitespace().collect();
    let Some(token) = input.input.first() else {
        return Ok(());
    };
    // The raw fourth word must be the very token the grammar is looking at.
    let fourth_is_next = words.len() == total
        && words.get(3).is_some_and(|word| *word == token.text)
        && token.kind == TokenKind::Word;
    if !fourth_is_next {
        return Ok(());
    }
    let lowered = token.text.to_ascii_lowercase();
    if lowered == keyword {
        any.parse_next(input)?;
        return Ok(());
    }
    if lowered.starts_with(keyword) {
        // C blanks the first four characters of the token, renaming the node.
        return Err(ErrMode::Cut(Failure(SpiceError::not_yet_ported(
            format!(
                "{}: node name '{}' that inp_compat() would truncate to '{}'",
                location(input),
                token.text,
                &token.text[keyword.len()..]
            ),
            "src/frontend/inpcom.c (inp_compat, replace_token)",
        ))));
    }
    Ok(())
}

/// The card text as `inp_remove_ws()` (`src/frontend/inpcom.c`) leaves it
/// before `inp_compat()` counts its words: whitespace before or after `=` is
/// dropped, and inside `{...}` also around arithmetic characters
/// (`is_arith_char()`: `+-*/()<>?:|&^!%\`) and `,`. So `gain = 2` is one
/// word, as in C.
pub(super) fn remove_ws(raw: &str) -> String {
    let joins = |c: char, braces: i32| c == '=' || (braces > 0 && is_arith_or_comma(c));
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut braces = 0_i32;
    while let Some(c) = chars.next() {
        match c {
            '{' => braces += 1,
            '}' => braces -= 1,
            _ => {}
        }
        if c.is_whitespace() {
            while chars.next_if(|c| c.is_whitespace()).is_some() {}
            if chars.peek().is_some_and(|next| !joins(*next, braces)) {
                out.push(' ');
            }
            continue;
        }
        out.push(c);
        if joins(c, braces) {
            while chars.next_if(|c| c.is_whitespace()).is_some() {}
        }
    }
    out
}

fn is_arith_or_comma(c: char) -> bool {
    c == ',' || "+-*/()<>?:|&^!%\\".contains(c)
}

/// The gain slot and any trailing setters, in C application order. Returns
/// the leading (positional) gain separately because C applies it last.
fn gain_and_setters(
    input: &mut Input<'_>,
    designator: char,
) -> Result<(Option<ParameterAssignment>, Vec<ParameterAssignment>)> {
    let mut setters = Vec::new();
    let leading = if let Some(value) = opt(leading_value).parse_next(input)? {
        Some(assignment("gain", value))
    } else if peek_named(input, "gain") {
        setters.push(named_setter(input)?);
        None
    } else if input.input.is_empty() {
        return Err(parse_error(
            input,
            "expected the gain (C: not enough parameters)",
        ));
    } else {
        return Err(parse_error(
            input,
            "expected the gain as a number, {expression} or gain=value right after the controls",
        ));
    };
    if input.input.is_empty() {
        return Ok((leading, setters));
    }
    let multiplier = matches!(designator, 'f' | 'g');
    if peek_named(input, "m") && multiplier {
        // inp_check_syntax() accepts an m= tail; INPdevParse then applies every
        // instance setter in order.
        let tail: Vec<ParameterAssignment> =
            repeat(1.., |input: &mut Input<'_>| tail_setter(input)).parse_next(input)?;
        setters.extend(tail);
        return Ok((leading, setters));
    }
    if peek_named(input, "m") || peek_named(input, "ic") {
        let token = &input.input[0];
        return Err(parse_error(
            input,
            &format!(
                "unknown parameter '{}' on a '{designator}' instance",
                token.text.to_ascii_lowercase()
            ),
        ));
    }
    Err(parse_error(
        input,
        &format!(
            "unexpected '{}' after the gain",
            input.input.first().map_or("", |t| t.text.as_str())
        ),
    ))
}

fn tail_setter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    peek(any).parse_next(input)?;
    if peek_named(input, "m") || peek_named(input, "gain") {
        return named_setter(input);
    }
    let token = &input.input[0];
    let lowered = token.text.to_ascii_lowercase();
    if lowered.starts_with("sens_") {
        return Err(ErrMode::Cut(Failure(SpiceError::not_yet_ported(
            format!("{}: sensitivity flag '{lowered}'", location(input)),
            "src/spicelib/analysis/sens*.c",
        ))));
    }
    if lowered == "control" {
        return Err(ErrMode::Cut(Failure(SpiceError::not_yet_ported(
            format!(
                "{}: a named control= setter after the positional controlling source",
                location(input)
            ),
            "src/spicelib/devices/cccs/cccspar.c",
        ))));
    }
    Err(parse_error(
        input,
        "expected m=value or gain=value after the gain",
    ))
}

fn peek_named(input: &Input<'_>, keyword: &str) -> bool {
    input.input.first().is_some_and(|t| t.is_keyword(keyword))
        && input
            .input
            .get(1)
            .is_some_and(|t| t.kind == TokenKind::Equals)
}

fn named_setter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let (token, _, value) = (any, equals, value).parse_next(input)?;
    Ok(named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    ))
}

fn parse_error(input: &Input<'_>, message: &str) -> ErrMode<Failure> {
    ErrMode::Cut(Failure(SpiceError::parse(location(input), message)))
}
