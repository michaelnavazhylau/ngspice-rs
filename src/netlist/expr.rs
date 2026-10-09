//! Parameter-expression syntax tree (bounded numparam subset).
//!
//! These types describe **syntax only**. Nothing here is evaluated, resolved or
//! range-checked: a later evaluation pass (GitHub #15) walks [`Expr`] with its
//! own environment. The grammar and its limits are documented in
//! `docs/port/PARAM_EXPRESSIONS.md`; the C behaviour it follows lives in
//! `src/frontend/numparam/xpressn.c` (`formula()`, `fetchoperator()`,
//! `fmathS`) and the `.param` line splitting in `inpcom.c`
//! (`inp_split_multi_param_lines()`).
//!
//! # Accepted syntax
//!
//! ```text
//! sum     := [sign] term { ('+' | '-') term }
//! term    := factor { ('*' | '/') factor }          (left associative)
//! factor  := atom { ('^' | '**') atom }             (left associative, as in C)
//! atom    := number | '-' number | identifier | call | '(' sum ')'
//! call    := function '(' sum { ',' sum } ')'
//!          | name '(' [ sum { ',' sum } ] ')'     (user `.func`, see below)
//! ```
//!
//! A call to a name outside the [`Function`] allowlist is an
//! [`ExprKind::UserCall`]; whether a `.func` defines it is decided during
//! evaluation ([`crate::netlist::eval::FunctionScope`]).
//!
//! Precedence, tightest first: `^`/`**`, then `*` `/`, then `+` `-`. A sign at
//! the start of an expression, group or argument binds like C's
//! additive-level operator (`-2^2` is `-(2^2)`, `-a*b+1` is `(-(a*b))+1`).
//! After a binary operator C accepts only `-` directly before a numeric
//! literal, which then binds tightest (`2*-3^2` is `2*((-3)^2)`); the parser
//! rejects the other C-erroring forms (`2*+3`, `2*-a`, `2*-(3)`).

use crate::primitives::{Real, SourceLoc};

/// A half-open byte range on one logical card: `start` is the first byte,
/// `end` the column just past the last byte. Columns follow the tokenizer's
/// contract (1-based, relative to the joined logical card).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    /// First byte of the spanned text.
    pub start: SourceLoc,
    /// Column one past the last byte (same file and line as `start`).
    pub end: SourceLoc,
}

impl SourceSpan {
    /// Span length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.end.column.saturating_sub(self.start.column) as usize
    }

    /// True when the span covers no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A parsed expression together with the text it was written as.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterExpression {
    /// The expression text exactly as written: without the surrounding braces
    /// when [`braced`](Self::braced), including any inner whitespace.
    pub text: String,
    /// True for a `{...}` or `'...'` expression, false for an unbraced
    /// `.param` value or a bare name at an `X`/`.subckt` parameter site.
    pub braced: bool,
    /// True when the delimiters were single quotes (`'...'`) rather than braces.
    /// C's `inpcom.c` `inp_change_quotes()` rewrites every quote pair to a
    /// brace pair before numparam runs, so a quoted expression means exactly
    /// the same as the braced one; the flag only preserves the spelling for
    /// the writer and dumps. Implies [`braced`](Self::braced).
    pub quoted: bool,
    /// Span of [`text`](Self::text). For a braced expression the braces sit one
    /// column before `span.start` and at `span.end`.
    pub span: SourceSpan,
    /// The syntax tree. `root.span` excludes surrounding whitespace.
    pub root: Expr,
}

impl ParameterExpression {
    /// The expression as written, delimiters included: `{text}`, `'text'` or
    /// the bare text.
    #[must_use]
    pub fn spelling(&self) -> String {
        if self.quoted {
            format!("'{}'", self.text)
        } else if self.braced {
            format!("{{{}}}", self.text)
        } else {
            self.text.clone()
        }
    }

    /// Names referenced by the expression, lowercased, in source order with
    /// duplicates kept. Function names are not references.
    #[must_use]
    pub fn references(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.root.collect_references(&mut names);
        names
    }
}

/// One syntax-tree node with its byte span.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What the node is.
    pub kind: ExprKind,
    /// The text the node covers (parentheses of a [`ExprKind::Group`] and a
    /// signed literal's sign included).
    pub span: SourceSpan,
}

impl Expr {
    fn collect_references<'a>(&'a self, names: &mut Vec<&'a str>) {
        match &self.kind {
            ExprKind::Number { .. } => {}
            ExprKind::Identifier(name) => names.push(name),
            ExprKind::Unary { operand, .. } | ExprKind::Group(operand) => {
                operand.collect_references(names);
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                lhs.collect_references(names);
                rhs.collect_references(names);
            }
            ExprKind::Call { arguments, .. } | ExprKind::UserCall { arguments, .. } => {
                for argument in arguments {
                    argument.collect_references(names);
                }
            }
        }
    }
}

/// Node shapes. Children are owned; trees are bounded by the parser's nesting
/// limit ([`MAX_NESTING`]).
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A finite numeric literal. `value` already includes the SPICE scale
    /// factor (`2.5meg` is `2.5e6`, `5V` is `5`); `spelling` is the original
    /// text. Overflowing spellings such as `1e999` are rejected, never stored.
    Number {
        /// Value with the scale factor applied.
        value: Real,
        /// Original spelling, including suffix and unit letters.
        spelling: String,
    },
    /// A parameter reference, lowercased (identifiers are case-insensitive).
    /// Never a function name.
    Identifier(String),
    /// A prefix sign.
    Unary {
        /// The sign.
        op: UnaryOp,
        /// Its operand.
        operand: Box<Expr>,
    },
    /// A binary arithmetic operation.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        lhs: Box<Expr>,
        /// Right operand.
        rhs: Box<Expr>,
    },
    /// An allowlisted function call; `arguments.len() == function.arity()`.
    Call {
        /// The function.
        function: Function,
        /// Arguments in source order.
        arguments: Vec<Expr>,
    },
    /// A parenthesised sub-expression, kept so spans and shape survive.
    Group(Box<Expr>),
    /// A call to a name outside the [`Function`] allowlist: a user `.func`
    /// (C: `inpcom.c` `inp_expand_macro_in_str()`), resolved during
    /// evaluation in the scope of the site. Any argument count (including
    /// zero) is syntactically valid; arity is checked against the definition.
    /// Names of numparam built-ins outside the allowlist (`agauss`, `limit`,
    /// ...) parse here too, so a `.func` may define them; without one they are
    /// reported as not yet ported when evaluated.
    UserCall {
        /// Lowercased function name.
        name: String,
        /// Arguments in source order.
        arguments: Vec<Expr>,
    },
}

/// Prefix signs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `+x`
    Plus,
    /// `-x`
    Minus,
}

/// Binary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `^` or `**` (both spellings are one operator in C). Left associative:
    /// `2^3^2` is `(2^3)^2 = 64` in C. C's default `^` is `pow(fabs(x), y)`
    /// (xpressn.c `operate()`), so an evaluator must not use plain `powf`.
    Pow,
}

/// Maximum parenthesis/call nesting accepted by the parser; deeper input is a
/// committed error instead of unbounded recursion.
pub const MAX_NESTING: usize = 64;

macro_rules! functions {
    ($($variant:ident => $name:literal / $arity:literal,)*) => {
        /// The allowlisted numparam functions (a subset of `fmathS` in
        /// `xpressn.c`). Randomised functions (`agauss`, `gauss`, `unif`,
        /// `aunif`, `limit`), `ternary_fcn`, and the string-argument `vec`/`var`
        /// are deliberately excluded.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Function {
            $(
                #[doc = concat!("`", $name, "`")]
                $variant,
            )*
        }

        impl Function {
            /// Every allowlisted function, in `fmathS` order.
            pub const ALL: &'static [Function] = &[$(Self::$variant,)*];

            /// Looks a function up by (case-insensitive) name.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                match name.to_ascii_lowercase().as_str() {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// Canonical lowercase name.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// Required argument count: 2 for `pow`/`pwr`/`max`/`min`, else 1.
            #[must_use]
            pub const fn arity(self) -> usize {
                match self {
                    $(Self::$variant => $arity,)*
                }
            }
        }
    };
}

functions! {
    Sqr => "sqr" / 1,
    Sqrt => "sqrt" / 1,
    Sin => "sin" / 1,
    Cos => "cos" / 1,
    Exp => "exp" / 1,
    Ln => "ln" / 1,
    Arctan => "arctan" / 1,
    Abs => "abs" / 1,
    Pow => "pow" / 2,
    Pwr => "pwr" / 2,
    Max => "max" / 2,
    Min => "min" / 2,
    Int => "int" / 1,
    Log => "log" / 1,
    Log10 => "log10" / 1,
    Sinh => "sinh" / 1,
    Cosh => "cosh" / 1,
    Tanh => "tanh" / 1,
    Sgn => "sgn" / 1,
    Ceil => "ceil" / 1,
    Floor => "floor" / 1,
    Asin => "asin" / 1,
    Acos => "acos" / 1,
    Atan => "atan" / 1,
    Asinh => "asinh" / 1,
    Acosh => "acosh" / 1,
    Atanh => "atanh" / 1,
    Tan => "tan" / 1,
    Nint => "nint" / 1,
}

/// C `fmathS` names outside the allowlist. A call to one parses as
/// [`ExprKind::UserCall`] and is rejected as not yet ported during evaluation
/// unless a `.func` of that name is in scope.
pub const EXCLUDED_FUNCTIONS: &[&str] = &[
    "ternary_fcn",
    "agauss",
    "gauss",
    "unif",
    "aunif",
    "limit",
    "vec",
    "var",
];
