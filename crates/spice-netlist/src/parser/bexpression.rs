//! Winnow grammar for behavioural-source expressions over raw card text.
//!
//! Follows `src/spicelib/parser/inpptree-parser.y` (precedence) and `PTlex()`
//! in `inpptree.c` (tokens), applied to the text `inp_modify_exp()`
//! (`src/frontend/inpcom.c`) hands it. See [`crate::bexpr`] for the grammar.
//!
//! Like C's bison parser, the expression ends at the first token that cannot
//! continue it; the caller decides whether the rest of the card is valid
//! (instance setters such as `m=2`) or an error. Every failure after a
//! recognised prefix is committed and carries a byte column.

use spice_core::{SourceLoc, SpiceError, SpiceResult, parse_spice_number_prefix};
use winnow::Parser as _;
use winnow::combinator::{opt, peek};
use winnow::error::ErrMode;
use winnow::stream::Stream;
use winnow::token::{any, literal, take, take_while};

use crate::bexpr::{BBinaryOp, BExpr, BExprKind, BUnaryOp, BehaviouralExpression};
use crate::expr::{MAX_NESTING, SourceSpan};

use super::expression::{Ctx, Fail, In, Res, cut, into_error};

/// Parses the longest behavioural expression at the start of `text` (whose
/// first byte is at `column` on `origin`'s line). Returns the expression and
/// the number of bytes consumed, trailing whitespace included.
///
/// In a `=pwl(` line (`verbatim`) a `{...}` or `'...'` group is a numparam
/// value; otherwise braces and single quotes are whitespace, as after
/// `inp_modify_exp()`. `auto_gnd` maps a `gnd` node in `v(...)` to `0`.
///
/// # Errors
/// A positioned [`SpiceError::Parse`] for malformed input, including an empty
/// expression.
pub(super) fn parse_prefix(
    text: &str,
    origin: &SourceLoc,
    column: u32,
    verbatim: bool,
    auto_gnd: bool,
) -> SpiceResult<(BehaviouralExpression, usize)> {
    let prepared: String = if verbatim {
        text.to_owned()
    } else {
        text.chars()
            .map(|c| {
                if matches!(c, '{' | '}' | '\'') {
                    ' '
                } else {
                    c
                }
            })
            .collect()
    };
    let mut input = In {
        input: prepared.as_str(),
        state: Ctx {
            origin,
            column,
            total: prepared.len(),
            depth: 0,
        },
    };
    let options = Options { verbatim, auto_gnd };
    let run = |input: &mut In<'_>| -> Res<BExpr> {
        ws(input)?;
        if input.input.is_empty() {
            return Err(cut(input.eof_offset(), "empty behavioural expression"));
        }
        let root = ternary(input, options)?;
        ws(input)?;
        Ok(root)
    };
    match run(&mut input) {
        Ok(root) => {
            let consumed = prepared.len() - input.input.len();
            let start_offset = text.len() - text.trim_start().len();
            let written = text[start_offset..consumed].trim_end();
            let span = SourceSpan {
                start: origin.at_column(column + offset_u32(start_offset)),
                end: origin.at_column(column + offset_u32(start_offset + written.len())),
            };
            Ok((
                BehaviouralExpression {
                    text: written.to_owned(),
                    span,
                    verbatim,
                    root,
                },
                consumed,
            ))
        }
        Err(ErrMode::Backtrack(fail) | ErrMode::Cut(fail)) => {
            Err(into_error(origin, column, prepared.len(), fail))
        }
        Err(ErrMode::Incomplete(_)) => unreachable!("complete input"),
    }
}

/// Parses all of `text` as one behavioural expression.
///
/// # Errors
/// As [`parse_prefix`], plus anything left after a complete expression.
pub(super) fn parse_complete(
    text: &str,
    origin: &SourceLoc,
    column: u32,
    verbatim: bool,
    auto_gnd: bool,
) -> SpiceResult<BehaviouralExpression> {
    let (expression, consumed) = parse_prefix(text, origin, column, verbatim, auto_gnd)?;
    if consumed < text.len() {
        return Err(SpiceError::parse(
            origin.at_column(column + offset_u32(consumed)),
            format!(
                "unexpected '{}' after a complete behavioural expression",
                text[consumed..].chars().next().unwrap_or(' ')
            ),
        ));
    }
    Ok(expression)
}

fn offset_u32(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy)]
struct Options {
    verbatim: bool,
    auto_gnd: bool,
}

fn ws(input: &mut In<'_>) -> Res<()> {
    take_while(0.., |c: char| c.is_ascii_whitespace())
        .void()
        .parse_next(input)
}

fn span(input: &In<'_>, start: usize, end: usize) -> SourceSpan {
    SourceSpan {
        start: input.state.location(start),
        end: input.state.location(end),
    }
}

fn joined(lhs: &BExpr, rhs: &BExpr) -> SourceSpan {
    SourceSpan {
        start: lhs.span.start.clone(),
        end: rhs.span.end.clone(),
    }
}

fn enter(input: &mut In<'_>, at: usize) -> Res<()> {
    input.state.depth += 1;
    if input.state.depth > MAX_NESTING {
        return Err(cut(
            at,
            format!("behavioural expression nesting limit ({MAX_NESTING}) exceeded"),
        ));
    }
    Ok(())
}

fn leave(input: &mut In<'_>) {
    input.state.depth = input.state.depth.saturating_sub(1);
}

fn next_char(input: &In<'_>) -> Option<char> {
    input.input.chars().next()
}

/// `or [ '?' expr ':' expr ]`, right associative.
fn ternary(input: &mut In<'_>, options: Options) -> Res<BExpr> {
    let condition = binary_level(input, options, 0)?;
    ws(input)?;
    if next_char(input) != Some('?') {
        return Ok(condition);
    }
    let at = input.eof_offset();
    literal("?").void().parse_next(input)?;
    enter(input, at)?;
    let then = ternary(input, options)?;
    ws(input)?;
    let colon = input.eof_offset();
    if opt(literal(":")).parse_next(input)?.is_none() {
        return Err(cut(colon, "expected ':' of the '?' operator"));
    }
    let otherwise = ternary(input, options)?;
    leave(input);
    let span = joined(&condition, &otherwise);
    Ok(BExpr {
        kind: BExprKind::Ternary {
            condition: Box::new(condition),
            then: Box::new(then),
            otherwise: Box::new(otherwise),
        },
        span,
    })
}

/// Binary operators of one precedence level, lowest first (yacc `%left`).
const LEVELS: &[&[(&str, BBinaryOp)]] = &[
    &[("||", BBinaryOp::Or)],
    &[("&&", BBinaryOp::And)],
    &[
        ("==", BBinaryOp::Eq),
        ("!=", BBinaryOp::Ne),
        ("<>", BBinaryOp::Ne),
    ],
    &[
        ("<=", BBinaryOp::Le),
        (">=", BBinaryOp::Ge),
        ("<", BBinaryOp::Lt),
        (">", BBinaryOp::Gt),
    ],
    &[("+", BBinaryOp::Add), ("-", BBinaryOp::Sub)],
    &[("*", BBinaryOp::Mul), ("/", BBinaryOp::Div)],
];

fn operator_at(input: &In<'_>, level: usize) -> Option<(&'static str, BBinaryOp)> {
    let rest = input.input;
    for &(symbol, op) in LEVELS[level] {
        if !rest.starts_with(symbol) {
            continue;
        }
        // `*` must not be the first half of `**` (power, a tighter level).
        if symbol == "*" && rest.starts_with("**") {
            return None;
        }
        // `<` must not be the first half of `<>` (not-equal, a looser level).
        if symbol == "<" && rest.starts_with("<>") {
            return None;
        }
        return Some((symbol, op));
    }
    None
}

fn binary_level(input: &mut In<'_>, options: Options, level: usize) -> Res<BExpr> {
    if level == LEVELS.len() {
        return unary(input, options);
    }
    let mut lhs = binary_level(input, options, level + 1)?;
    loop {
        ws(input)?;
        let Some((symbol, op)) = operator_at(input, level) else {
            return Ok(lhs);
        };
        take(symbol.len()).void().parse_next(input)?;
        let rhs = binary_level(input, options, level + 1)?;
        let span = joined(&lhs, &rhs);
        lhs = BExpr {
            kind: BExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }
}

fn prefix_operator(input: &In<'_>) -> Option<BUnaryOp> {
    let rest = input.input;
    match rest.chars().next()? {
        '-' => Some(BUnaryOp::Minus),
        '+' => Some(BUnaryOp::Plus),
        '!' if !rest.starts_with("!=") => Some(BUnaryOp::Not),
        _ => None,
    }
}

/// `('-' | '+' | '!') unary | power`: a sign binds looser than `^`.
fn unary(input: &mut In<'_>, options: Options) -> Res<BExpr> {
    ws(input)?;
    let start = input.eof_offset();
    let Some(op) = prefix_operator(input) else {
        return power(input, options);
    };
    take(1usize).void().parse_next(input)?;
    enter(input, start)?;
    let operand = unary(input, options)?;
    leave(input);
    let end = operand.span.end.clone();
    Ok(BExpr {
        kind: BExprKind::Unary {
            op,
            operand: Box::new(operand),
        },
        span: SourceSpan {
            start: input.state.location(start),
            end,
        },
    })
}

/// `primary { ('^' | '**') (primary | prefix unary) }`, left associative.
fn power(input: &mut In<'_>, options: Options) -> Res<BExpr> {
    let mut lhs = primary(input, options)?;
    loop {
        ws(input)?;
        let width: usize = if input.input.starts_with("**") {
            2
        } else if input.input.starts_with('^') {
            1
        } else {
            return Ok(lhs);
        };
        take(width).void().parse_next(input)?;
        ws(input)?;
        // yacc: the right operand may itself start with a sign, which then
        // takes the whole following power (`2^-1^2` is `2^(-(1^2))`).
        let rhs = if prefix_operator(input).is_some() {
            unary(input, options)?
        } else {
            primary(input, options)?
        };
        let span = joined(&lhs, &rhs);
        lhs = BExpr {
            kind: BExprKind::Binary {
                op: BBinaryOp::Pow,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Identifier characters of `inp_modify_exp()` (`isalnum`, `#`, `$`, `%`,
/// `_`, `[`, `]`; `!` is left out so that `a!=b` is a comparison).
fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '#' | '$' | '%' | '[' | ']')
}

fn primary(input: &mut In<'_>, options: Options) -> Res<BExpr> {
    ws(input)?;
    let start = input.eof_offset();
    match peek(opt(any)).parse_next(input)? {
        None => Err(cut(
            start,
            "expected an operand, found the end of the expression",
        )),
        Some('(') => group(input, options),
        Some(c) if c.is_ascii_digit() || c == '.' => number(input),
        Some(c) if is_name_start(c) => name_or_call(input, options),
        Some('{' | '\'') if options.verbatim => numparam_value(input),
        Some(c) => Err(cut(
            start,
            format!("unexpected '{c}' where an operand is required"),
        )),
    }
}

fn group(input: &mut In<'_>, options: Options) -> Res<BExpr> {
    let start = input.eof_offset();
    literal("(").void().parse_next(input)?;
    enter(input, start)?;
    let inner = ternary(input, options)?;
    close(input, start)?;
    leave(input);
    Ok(BExpr {
        kind: BExprKind::Group(Box::new(inner)),
        span: span(input, start, input.eof_offset()),
    })
}

fn close(input: &mut In<'_>, open: usize) -> Res<()> {
    ws(input)?;
    let here = input.eof_offset();
    if opt(literal(")")).parse_next(input)?.is_some() {
        return Ok(());
    }
    let found = next_char(input).map_or_else(
        || "the end of the expression".to_owned(),
        |c| format!("'{c}'"),
    );
    Err(cut(
        here,
        format!(
            "expected ')' to close the '(' at column {}, found {found}",
            input.state.location(open).column
        ),
    ))
}

/// `INPevaluate()` number with its scale factor; trailing letters (units)
/// are swallowed as `inp_modify_exp()`/`PTlex()` do.
fn number(input: &mut In<'_>) -> Res<BExpr> {
    let start = input.eof_offset();
    let before = input.input;
    let Some(parsed) = parse_spice_number_prefix(before) else {
        return Err(cut(start, "malformed numeric literal"));
    };
    take(parsed.consumed).void().parse_next(input)?;
    take_while(0.., |c: char| c.is_ascii_alphabetic())
        .void()
        .parse_next(input)?;
    let spelling = &before[..before.len() - input.input.len()];
    if !parsed.value.is_finite() {
        return Err(cut(
            start,
            format!("numeric literal '{spelling}' overflows to a non-finite value"),
        ));
    }
    Ok(BExpr {
        kind: BExprKind::Number {
            value: parsed.value,
            spelling: spelling.to_owned(),
        },
        span: span(input, start, input.eof_offset()),
    })
}

fn name_or_call(input: &mut In<'_>, options: Options) -> Res<BExpr> {
    let start = input.eof_offset();
    let name: &str = (
        any.verify(|c: &char| is_name_start(*c)),
        take_while(0.., is_name_continue),
    )
        .take()
        .parse_next(input)?;
    let lowered = name.to_ascii_lowercase();
    let adjacent = next_char(input) == Some('(');
    let checkpoint = input.checkpoint();
    ws(input)?;
    let call = next_char(input) == Some('(');
    if !call {
        input.reset(&checkpoint);
        return Ok(BExpr {
            kind: BExprKind::Name(lowered),
            span: span(input, start, input.eof_offset()),
        });
    }
    if matches!(lowered.as_str(), "v" | "i") {
        if !adjacent && !options.verbatim {
            return Err(cut(
                start,
                format!(
                    "'{name}' followed by a space and '(': write {name}(...) without a space \
                     (inp_modify_exp() reads a lone '{name}' as a parameter name)"
                ),
            ));
        }
        return circuit_quantity(input, start, lowered == "v", options);
    }
    let open = input.eof_offset();
    literal("(").void().parse_next(input)?;
    enter(input, open)?;
    ws(input)?;
    let mut arguments = Vec::new();
    if next_char(input) != Some(')') {
        loop {
            arguments.push(ternary(input, options)?);
            ws(input)?;
            if opt(literal(",")).parse_next(input)?.is_none() {
                break;
            }
        }
    }
    close(input, open)?;
    leave(input);
    Ok(BExpr {
        kind: BExprKind::Call {
            name: lowered,
            arguments,
        },
        span: span(input, start, input.eof_offset()),
    })
}

/// A node or source name inside `v(...)`/`i(...)`: `PTlex()` reads any run
/// of characters other than whitespace, `,`, `(` and `)`.
fn quantity_name<'a>(input: &mut In<'a>, what: &str) -> Res<&'a str> {
    ws(input)?;
    let at = input.eof_offset();
    let name: &str = take_while(0.., |c: char| {
        !c.is_ascii_whitespace() && !matches!(c, ',' | '(' | ')')
    })
    .parse_next(input)?;
    if name.is_empty() {
        return Err(cut(at, format!("expected a {what} name")));
    }
    Ok(name)
}

fn circuit_quantity(
    input: &mut In<'_>,
    start: usize,
    voltage: bool,
    options: Options,
) -> Res<BExpr> {
    ws(input)?;
    literal("(").void().parse_next(input)?;
    let canonical = |name: &str| {
        let lowered = name.to_ascii_lowercase();
        if options.auto_gnd && lowered == "gnd" {
            "0".to_owned()
        } else {
            lowered
        }
    };
    let kind = if voltage {
        let positive = canonical(quantity_name(input, "node")?);
        ws(input)?;
        let negative = if opt(literal(",")).parse_next(input)?.is_some() {
            Some(canonical(quantity_name(input, "node")?))
        } else {
            None
        };
        BExprKind::Voltage { positive, negative }
    } else {
        BExprKind::Current(quantity_name(input, "source")?.to_ascii_lowercase())
    };
    ws(input)?;
    let here = input.eof_offset();
    if opt(literal(")")).parse_next(input)?.is_none() {
        return Err(cut(
            here,
            if voltage {
                "expected ')' after v(node) or v(node, node)"
            } else {
                "expected ')' after i(source); i() takes one source name"
            },
        ));
    }
    Ok(BExpr {
        kind,
        span: span(input, start, input.eof_offset()),
    })
}

/// A `{...}`/`'...'` numparam value inside a `=pwl(` line.
fn numparam_value(input: &mut In<'_>) -> Res<BExpr> {
    let start = input.eof_offset();
    let open = next_char(input).unwrap_or('{');
    let close = if open == '{' { '}' } else { '\'' };
    let text = input.input;
    let Some(length) = text[1..].find(close) else {
        return Err(cut(start, format!("unterminated '{open}' value")));
    };
    let inner = &text[1..=length];
    let column = input.state.location(start).column + 1;
    let parsed =
        super::expression::parse_delimited(inner, input.state.origin, column, true, open == '\'')
            .map_err(|error| {
            ErrMode::Cut(Fail {
                remaining: start,
                message: error.to_string(),
                unsupported: false,
                resolved: Some(error),
            })
        })?;
    take(length + 2).void().parse_next(input)?;
    Ok(BExpr {
        kind: BExprKind::Value(Box::new(parsed)),
        span: span(input, start, input.eof_offset()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse(text: &str) -> BehaviouralExpression {
        let origin = SourceLoc::new(PathBuf::from("t.cir"), 1, 1);
        parse_complete(text, &origin, 1, false, true).unwrap()
    }

    fn shape(expr: &BExpr) -> String {
        match &expr.kind {
            BExprKind::Number { spelling, .. } => spelling.clone(),
            BExprKind::Name(name) => name.clone(),
            BExprKind::Voltage { positive, negative } => match negative {
                Some(negative) => format!("v({positive},{negative})"),
                None => format!("v({positive})"),
            },
            BExprKind::Current(name) => format!("i({name})"),
            BExprKind::Unary { op, operand } => format!("({op:?} {})", shape(operand)),
            BExprKind::Binary { op, lhs, rhs } => {
                format!("({} {} {})", shape(lhs), op.symbol(), shape(rhs))
            }
            BExprKind::Ternary {
                condition,
                then,
                otherwise,
            } => format!(
                "({} ? {} : {})",
                shape(condition),
                shape(then),
                shape(otherwise)
            ),
            BExprKind::Call { name, arguments } => format!(
                "{name}[{}]",
                arguments.iter().map(shape).collect::<Vec<_>>().join(",")
            ),
            BExprKind::Group(inner) => shape(inner),
            BExprKind::Value(value) => format!("{{{}}}", value.text),
            BExprKind::Table(_) => "table".into(),
        }
    }

    #[test]
    fn precedence_matches_the_bison_grammar() {
        for (text, expected) in [
            ("-2^2", "(Minus (2 ^ 2))"),
            ("2^3^2", "((2 ^ 3) ^ 2)"),
            ("2^-1^2", "(2 ^ (Minus (1 ^ 2)))"),
            ("-a*b", "((Minus a) * b)"),
            ("1+2>2", "((1 + 2) > 2)"),
            ("!0+1", "((Not 0) + 1)"),
            ("a || b && c", "(a || (b && c))"),
            ("1?2:3?4:5", "(1 ? 2 : (3 ? 4 : 5))"),
            ("a == b < c", "(a == (b < c))"),
            ("2**3*4", "((2 ^ 3) * 4)"),
            ("a<>b", "(a != b)"),
            ("v(a, b)*i(V1)", "(v(a,b) * i(v1))"),
            ("v(gnd)", "v(0)"),
            ("{2*3}", "(2 * 3)"),
            ("2*{1+1}", "((2 * 1) + 1)"),
            ("pow(v(1),2)", "pow[v(1),2]"),
            ("1k*2meg", "(1k * 2meg)"),
        ] {
            assert_eq!(shape(&parse(text).root), expected, "{text}");
        }
    }

    #[test]
    fn the_expression_stops_where_c_stops() {
        let origin = SourceLoc::new(PathBuf::from("t.cir"), 1, 1);
        let (expression, consumed) = parse_prefix("2*v(1) m=3", &origin, 1, false, true).unwrap();
        assert_eq!(expression.text, "2*v(1)");
        assert_eq!(consumed, 7);
        let (_, consumed) = parse_prefix("2 3", &origin, 1, false, true).unwrap();
        assert_eq!(consumed, 2);
    }

    #[test]
    fn malformed_expressions_are_positioned_errors() {
        let origin = SourceLoc::new(PathBuf::from("t.cir"), 1, 1);
        for (text, column) in [("2*", 3), ("(1+2", 5), ("v(a", 4), ("1 ? 2", 6), ("", 1)] {
            let error = parse_complete(text, &origin, 1, false, true).unwrap_err();
            match error {
                SpiceError::Parse { location, .. } => assert_eq!(location.column, column, "{text}"),
                other => panic!("{text}: {other}"),
            }
        }
    }

    #[test]
    fn verbatims_keep_numparam_values() {
        let origin = SourceLoc::new(PathBuf::from("t.cir"), 1, 1);
        let parsed = parse_complete("pwl(v(1), 0, 0, {a*2}, 1)", &origin, 1, true, true).unwrap();
        assert_eq!(shape(&parsed.root), "pwl[v(1),0,0,{a*2},1]");
    }
}
