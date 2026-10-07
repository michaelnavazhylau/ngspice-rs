//! Winnow grammar for bounded numparam expressions over `&str`.
//!
//! Follows `src/frontend/numparam/xpressn.c` (`formula()`, `fetchoperator()`,
//! `fetchnumber()`, `fetchid()`, `fmathS`): `^`/`**` binds tighter than `*` `/`,
//! which bind tighter than `+` `-`, all left associative (`2^3^2` is 64 in C).
//! A sign at the start of an expression, group or argument behaves like the
//! additive operator (`-2^2` is -4); after a binary operator C only accepts
//! `-` before a numeric literal. Multiplicative and power levels use winnow's
//! `expression()` precedence climber; the additive level is a fold so the
//! leading sign can apply to the first term only.
//!
//! Every failure after a recognised prefix is a committed error carrying a
//! byte column. Nothing is evaluated.

use spice_core::{SourceLoc, SpiceError, SpiceResult, parse_spice_number_prefix};
use winnow::Parser as _;
use winnow::combinator::{Infix, alt, expression, opt, peek, repeat, separated};
use winnow::error::{AddContext, ErrMode, ModalResult, ParserError};
use winnow::stream::{Stateful, Stream};
use winnow::token::{any, literal, one_of, take, take_while};

use crate::expr::{
    BinaryOp, EXCLUDED_FUNCTIONS, Expr, ExprKind, Function, MAX_NESTING, ParameterExpression,
    SourceSpan, UnaryOp,
};
use crate::token::Token;

pub(super) const C_REFERENCE: &str = "src/frontend/numparam/xpressn.c";

/// Read-only position context plus the nesting counter.
#[derive(Debug)]
pub(super) struct Ctx<'a> {
    pub origin: &'a SourceLoc,
    /// Column of the first byte of the parsed text.
    pub column: u32,
    /// Byte length of the parsed text.
    pub total: usize,
    pub depth: usize,
}

impl Ctx<'_> {
    pub(super) fn location(&self, remaining: usize) -> SourceLoc {
        let offset = self.total.saturating_sub(remaining);
        self.origin.at_column(
            self.column
                .saturating_add(u32::try_from(offset).unwrap_or(u32::MAX)),
        )
    }
}

pub(super) type In<'a> = Stateful<&'a str, Ctx<'a>>;
pub(super) type Res<T> = ModalResult<T, Fail>;

/// Error carrying the remaining byte count where it occurred, so the entry
/// points can compute an absolute byte column.
#[derive(Debug)]
pub(super) struct Fail {
    pub remaining: usize,
    pub message: String,
    /// Valid numparam outside the bounded subset (NotYetPorted, not Parse).
    pub unsupported: bool,
    /// An already positioned error from a nested parse.
    pub resolved: Option<SpiceError>,
}

impl<I: Stream> ParserError<I> for Fail {
    type Inner = Self;

    fn from_input(input: &I) -> Self {
        Self {
            remaining: input.eof_offset(),
            message: "unexpected input".to_owned(),
            unsupported: false,
            resolved: None,
        }
    }

    fn into_inner(self) -> std::result::Result<Self::Inner, Self> {
        Ok(self)
    }
}

impl<I: Stream> AddContext<I, &'static str> for Fail {
    fn add_context(mut self, _input: &I, _start: &I::Checkpoint, expected: &'static str) -> Self {
        self.message = format!("expected {expected}");
        self
    }
}

pub(super) fn cut(remaining: usize, message: impl Into<String>) -> ErrMode<Fail> {
    ErrMode::Cut(Fail {
        remaining,
        message: message.into(),
        unsupported: false,
        resolved: None,
    })
}

pub(super) fn cut_unsupported(remaining: usize, message: impl Into<String>) -> ErrMode<Fail> {
    ErrMode::Cut(Fail {
        remaining,
        message: message.into(),
        unsupported: true,
        resolved: None,
    })
}

/// Converts a grammar failure to the port's error type.
pub(super) fn into_error(
    ctx_origin: &SourceLoc,
    column: u32,
    total: usize,
    fail: Fail,
) -> SpiceError {
    if let Some(error) = fail.resolved {
        return error;
    }
    let location = Ctx {
        origin: ctx_origin,
        column,
        total,
        depth: 0,
    }
    .location(fail.remaining);
    if fail.unsupported {
        SpiceError::not_yet_ported(format!("{location}: {}", fail.message), C_REFERENCE)
    } else {
        SpiceError::parse(location, fail.message)
    }
}

pub(super) fn ws(input: &mut In<'_>) -> Res<()> {
    take_while(0.., |c: char| c.is_ascii_whitespace())
        .void()
        .parse_next(input)
}

pub(super) fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

pub(super) fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn span(input: &In<'_>, start: usize, end: usize) -> SourceSpan {
    SourceSpan {
        start: input.state.location(start),
        end: input.state.location(end),
    }
}

/// Describes the character that cannot start/continue an expression here.
fn describe(c: char) -> String {
    match c {
        '%' | '\\' | '!' | '<' | '>' | '=' | '&' | '|' | '?' | ':' => format!(
            "operator '{c}' is outside the bounded subset (only + - * / ^ ** are parsed; \
             comparison, logical, ternary, '%' and '\\' operators are not)"
        ),
        '\'' | '"' => "quoted expressions are outside the bounded subset".to_owned(),
        '{' => "nested '{' is not supported inside an expression".to_owned(),
        '}' => "unmatched '}'".to_owned(),
        ')' => "unmatched ')'".to_owned(),
        other => format!("unexpected '{other}'"),
    }
}

fn is_unsupported(c: char) -> bool {
    matches!(
        c,
        '%' | '\\' | '!' | '<' | '>' | '=' | '&' | '|' | '?' | ':' | '\'' | '"'
    )
}

fn unexpected(remaining: usize, c: char, context: &str) -> ErrMode<Fail> {
    let message = if matches!(c, ')' | '}' | '{') || is_unsupported(c) {
        describe(c)
    } else {
        format!("{} {context}", describe(c))
    };
    if is_unsupported(c) {
        cut_unsupported(remaining, message)
    } else {
        cut(remaining, message)
    }
}

fn enter(input: &mut In<'_>, remaining: usize) -> Res<()> {
    input.state.depth += 1;
    if input.state.depth > MAX_NESTING {
        return Err(cut(
            remaining,
            format!("expression nesting limit ({MAX_NESTING}) exceeded"),
        ));
    }
    Ok(())
}

fn leave(input: &mut In<'_>) {
    input.state.depth = input.state.depth.saturating_sub(1);
}

/// `sum := [sign] term { ('+' | '-') term }`
fn sum(input: &mut In<'_>) -> Res<Expr> {
    ws.parse_next(input)?;
    let open = input.eof_offset();
    let lead = opt(one_of(['+', '-'])).parse_next(input)?;
    let first = term.parse_next(input)?;
    let first = match lead {
        Some(sign) => {
            let op = if sign == '-' {
                UnaryOp::Minus
            } else {
                UnaryOp::Plus
            };
            let span = SourceSpan {
                start: input.state.location(open),
                end: first.span.end.clone(),
            };
            Expr {
                kind: ExprKind::Unary {
                    op,
                    operand: Box::new(first),
                },
                span,
            }
        }
        None => first,
    };
    let rest: Vec<(BinaryOp, Expr)> = repeat(0.., (additive, term)).parse_next(input)?;
    Ok(rest
        .into_iter()
        .fold(first, |lhs, (op, rhs)| binary(op, lhs, rhs)))
}

fn additive(input: &mut In<'_>) -> Res<BinaryOp> {
    ws.parse_next(input)?;
    one_of(['+', '-'])
        .map(|c| {
            if c == '+' {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            }
        })
        .parse_next(input)
}

fn binary(op: BinaryOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = SourceSpan {
        start: lhs.span.start.clone(),
        end: rhs.span.end.clone(),
    };
    Expr {
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span,
    }
}

type InfixOp<'a> = Infix<In<'a>, Expr, ErrMode<Fail>>;

fn fold_mul(_: &mut In<'_>, lhs: Expr, rhs: Expr) -> Res<Expr> {
    Ok(binary(BinaryOp::Mul, lhs, rhs))
}

fn fold_div(_: &mut In<'_>, lhs: Expr, rhs: Expr) -> Res<Expr> {
    Ok(binary(BinaryOp::Div, lhs, rhs))
}

fn fold_pow(_: &mut In<'_>, lhs: Expr, rhs: Expr) -> Res<Expr> {
    Ok(binary(BinaryOp::Pow, lhs, rhs))
}

/// Binding powers: `^`/`**` (30) over `*` `/` (20), all left associative as in
/// C's accumulator machine (`xpressn.c` levels 2 and 3).
fn multiplicative<'a>(input: &mut In<'a>) -> Res<InfixOp<'a>> {
    ws.parse_next(input)?;
    alt((
        literal("**").value(Infix::Left(30, fold_pow as _)),
        literal("^").value(Infix::Left(30, fold_pow as _)),
        literal("*").value(Infix::Left(20, fold_mul as _)),
        literal("/").value(Infix::Left(20, fold_div as _)),
    ))
    .parse_next(input)
}

fn term(input: &mut In<'_>) -> Res<Expr> {
    expression(operand).infix(multiplicative).parse_next(input)
}

/// Every operand position is mandatory, so failures here are committed.
fn operand(input: &mut In<'_>) -> Res<Expr> {
    ws.parse_next(input)?;
    let start = input.eof_offset();
    match peek(opt(any)).parse_next(input)? {
        None => Err(cut(
            start,
            "expected an operand, found the end of the expression",
        )),
        Some('(') => group(input),
        Some(c) if c.is_ascii_digit() || c == '.' => number(input),
        Some('-') => signed_number(input),
        Some(c) if is_ident_start(c) => identifier_or_call(input),
        Some(')') => Err(cut(start, "expected an operand, found ')'")),
        Some(c) => Err(unexpected(start, c, "where an operand is required")),
    }
}

fn number(input: &mut In<'_>) -> Res<Expr> {
    let start = input.eof_offset();
    let before = input.input;
    let Some(parsed) = parse_spice_number_prefix(before) else {
        return Err(cut(start, "malformed numeric literal"));
    };
    let digits = input.input[..parsed.consumed].chars().count();
    take(digits).void().parse_next(input)?;
    // fetchnumber() swallows the unit letters after the scale factor.
    take_while(0.., |c: char| c.is_ascii_alphabetic())
        .void()
        .parse_next(input)?;
    let end = input.eof_offset();
    let spelling = &before[..before.len() - input.input.len()];
    if !parsed.value.is_finite() {
        return Err(cut(
            start,
            format!("numeric literal '{spelling}' overflows to a non-finite value"),
        ));
    }
    Ok(Expr {
        kind: ExprKind::Number {
            value: parsed.value,
            spelling: spelling.to_owned(),
        },
        span: span(input, start, end),
    })
}

/// `-` directly before a numeric literal, valid after a binary operator
/// (xpressn.c's `negate` flag, which only `fetchnumber()` consumes).
fn signed_number(input: &mut In<'_>) -> Res<Expr> {
    let start = input.eof_offset();
    literal("-").void().parse_next(input)?;
    ws.parse_next(input)?;
    let here = input.eof_offset();
    match peek(opt(any)).parse_next(input)? {
        Some(c) if c.is_ascii_digit() || c == '.' => {}
        _ => {
            return Err(cut(
                here,
                "a second sign is accepted only directly before a numeric literal \
                 (C numparam rejects `-` before names, groups and calls here)",
            ));
        }
    }
    let literal_value = number(input)?;
    let span = SourceSpan {
        start: input.state.location(start),
        end: literal_value.span.end.clone(),
    };
    Ok(Expr {
        kind: ExprKind::Unary {
            op: UnaryOp::Minus,
            operand: Box::new(literal_value),
        },
        span,
    })
}

fn group(input: &mut In<'_>) -> Res<Expr> {
    let start = input.eof_offset();
    literal("(").void().parse_next(input)?;
    enter(input, start)?;
    let inner = sum.parse_next(input)?;
    close(input, start)?;
    leave(input);
    Ok(Expr {
        kind: ExprKind::Group(Box::new(inner)),
        span: span(input, start, input.eof_offset()),
    })
}

fn close(input: &mut In<'_>, open: usize) -> Res<()> {
    ws.parse_next(input)?;
    let here = input.eof_offset();
    match opt(literal(")")).parse_next(input)? {
        Some(_) => Ok(()),
        None => match peek(opt(any)).parse_next(input)? {
            None => Err(cut(
                here,
                format!(
                    "expected ')' to close the '(' at column {}",
                    input.state.location(open).column
                ),
            )),
            Some(c) => Err(unexpected(
                here,
                c,
                "after a complete expression (missing operator?)",
            )),
        },
    }
}

fn identifier_or_call(input: &mut In<'_>) -> Res<Expr> {
    let start = input.eof_offset();
    let name: &str = (one_of(is_ident_start), take_while(0.., is_ident_continue))
        .take()
        .parse_next(input)?;
    let after = input.eof_offset();
    let call = peek((ws, opt(literal("(")))).parse_next(input)?.1.is_some();
    let lowered = name.to_ascii_lowercase();
    if !call {
        if Function::from_name(&lowered).is_some() || EXCLUDED_FUNCTIONS.contains(&lowered.as_str())
        {
            return Err(cut(
                start,
                format!("function name '{name}' requires an argument list"),
            ));
        }
        return Ok(Expr {
            kind: ExprKind::Identifier(lowered),
            span: span(input, start, after),
        });
    }
    let Some(function) = Function::from_name(&lowered) else {
        let why = if EXCLUDED_FUNCTIONS.contains(&lowered.as_str()) {
            "is a numparam function outside the bounded allowlist"
        } else {
            "is not in the bounded function allowlist"
        };
        return Err(cut_unsupported(start, format!("function '{name}' {why}")));
    };
    ws.parse_next(input)?;
    let open = input.eof_offset();
    literal("(").void().parse_next(input)?;
    enter(input, open)?;
    let arguments: Vec<Expr> = separated(1.., sum, (ws, literal(","))).parse_next(input)?;
    close(input, open)?;
    leave(input);
    if arguments.len() != function.arity() {
        return Err(cut(
            start,
            format!(
                "function '{}' takes {} argument(s), found {}",
                function.name(),
                function.arity(),
                arguments.len()
            ),
        ));
    }
    Ok(Expr {
        kind: ExprKind::Call {
            function,
            arguments,
        },
        span: span(input, start, input.eof_offset()),
    })
}

/// Parses `text` (the content of a `{...}` expression, or an unbraced `.param`
/// value / bare name) whose first byte is at `column` on `origin`'s line.
///
/// # Errors
///
/// A committed [`SpiceError::Parse`] (malformed, with the byte column of the
/// offending text) or [`SpiceError::NotYetPorted`] (valid numparam outside the
/// bounded subset).
pub(super) fn parse_expression(
    text: &str,
    origin: &SourceLoc,
    column: u32,
    braced: bool,
) -> SpiceResult<ParameterExpression> {
    let mut input = In {
        input: text,
        state: Ctx {
            origin,
            column,
            total: text.len(),
            depth: 0,
        },
    };
    let run = |input: &mut In<'_>| -> Res<Expr> {
        let root = sum(input)?;
        ws.parse_next(input)?;
        let here = input.eof_offset();
        if let Some(c) = peek(opt(any)).parse_next(input)? {
            return Err(unexpected(
                here,
                c,
                "after a complete expression (missing operator?)",
            ));
        }
        Ok(root)
    };
    if text.trim().is_empty() {
        return Err(SpiceError::parse(
            origin.at_column(column),
            "empty expression",
        ));
    }
    match run(&mut input) {
        Ok(root) => Ok(ParameterExpression {
            text: text.to_owned(),
            braced,
            span: SourceSpan {
                start: origin.at_column(column),
                end: origin.at_column(
                    column.saturating_add(u32::try_from(text.len()).unwrap_or(u32::MAX)),
                ),
            },
            root,
        }),
        Err(ErrMode::Backtrack(fail) | ErrMode::Cut(fail)) => {
            Err(into_error(origin, column, text.len(), fail))
        }
        Err(ErrMode::Incomplete(_)) => unreachable!("complete input"),
    }
}

/// Parses the text of a `{...}` token (tokenizer-matched braces).
///
/// # Errors
///
/// As [`parse_expression`].
pub(super) fn from_brace_token(token: &Token) -> SpiceResult<ParameterExpression> {
    let inner = token
        .text
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap_or(&token.text);
    parse_expression(inner, &token.location, token.location.column + 1, true)
}

/// Parses a token that is a bare identifier, as an unbraced expression.
pub(super) fn from_name_token(token: &Token) -> SpiceResult<ParameterExpression> {
    parse_expression(&token.text, &token.location, token.location.column, false)
}
