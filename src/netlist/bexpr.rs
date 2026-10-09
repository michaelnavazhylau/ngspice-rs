//! Behavioural-source expression syntax tree (B sources and the E/G/F/H
//! `VALUE`/`TABLE`/`POLY` forms lowered onto them).
//!
//! These expressions are **not** numparam expressions ([`crate::netlist::expr`]): they
//! reference circuit quantities (`v(node)`, `v(n1,n2)`, `i(vsource)`, `time`,
//! `temper`, `hertz`) and are evaluated at every Newton iteration by the
//! device, not once by the front end. The grammar follows ngspice's B-source
//! parser, `src/spicelib/parser/inpptree-parser.y` and the lexer `PTlex()` in
//! `inpptree.c`, applied to the card text exactly as the front end leaves it
//! (`inpcom.c` `inp_bsource_compat()` / `inp_modify_exp()`). See
//! `docs/port/BEHAVIOURAL_SOURCES.md`.
//!
//! # Grammar (C precedence, lowest first)
//!
//! ```text
//! expr    := or [ '?' expr ':' expr ]             (right associative)
//! or      := and { '||' and }
//! and     := eq { '&&' eq }
//! eq      := rel { ('==' | '!=' | '<>') rel }
//! rel     := add { ('<' | '>' | '<=' | '>=') add }
//! add     := mul { ('+' | '-') mul }
//! mul     := unary { ('*' | '/') unary }
//! unary   := ('-' | '+' | '!') unary | power
//! power   := primary { ('^' | '**') (primary | ('-' | '+' | '!') unary) }
//! primary := number | name | v(node[,node]) | i(source) | call | '(' expr ')'
//! call    := name '(' expr { ',' expr } ')'
//! ```
//!
//! All binary operators are left associative (`2^3^2` is 64, as in C). A
//! unary sign binds looser than `^` (`-2^2` is -4) and tighter than `*`.
//!
//! Nothing here is evaluated. The front end resolves `.param` names and
//! `.func` calls ([`crate::netlist::elaborate`]); `devices` compiles the resolved
//! tree into its value-and-derivative evaluator.

use crate::primitives::Real;

use crate::netlist::expr::{ParameterExpression, SourceSpan};

/// A parsed behavioural expression with the text it was written as.
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviouralExpression {
    /// The expression text exactly as written, trimmed, delimiters included
    /// (braces and single quotes are transparent in B expressions).
    pub text: String,
    /// Span of [`text`](Self::text).
    pub span: SourceSpan,
    /// True when C hands the text to the B-source parser without
    /// `inp_modify_exp()`: a `=pwl(` B line (`inp_bsource_compat()` skips
    /// such cards) or a POLY/TABLE model the front end generated. Then
    /// `{...}` groups are numparam values ([`BExprKind::Value`]), bare
    /// `.param` names are **not** substituted and numeric literals keep their
    /// full precision. Otherwise braces and single quotes are mere whitespace
    /// and literals are rounded to 11 significant digits (`inp_modify_exp()`
    /// prints them with `%18.10e`).
    pub verbatim: bool,
    /// The syntax tree.
    pub root: BExpr,
}

/// One syntax-tree node with its byte span.
#[derive(Debug, Clone, PartialEq)]
pub struct BExpr {
    /// What the node is.
    pub kind: BExprKind,
    /// The text the node covers.
    pub span: SourceSpan,
}

/// Node shapes.
#[derive(Debug, Clone, PartialEq)]
pub enum BExprKind {
    /// A finite numeric literal with its scale factor applied; `spelling` is
    /// the original text (unit letters included).
    Number {
        /// Value with the scale factor applied.
        value: Real,
        /// Original spelling.
        spelling: String,
    },
    /// A bare name, lowercased: `time`, `temper`, `hertz`, the constants `pi`
    /// and `e`, or a `.param` name (substituted by the front end).
    Name(String),
    /// `v(positive)` or `v(positive, negative)`: a node voltage or a
    /// difference. Node names are canonical (lowercased, `gnd` aliasing
    /// applied like any other node).
    Voltage {
        /// First node.
        positive: String,
        /// Second node of a difference.
        negative: Option<String>,
    },
    /// `i(source)`: the branch current of a voltage source, E/H or voltage B
    /// source (C `CKTfndBranch`), lowercased.
    Current(String),
    /// A prefix operator.
    Unary {
        /// The operator.
        op: BUnaryOp,
        /// Its operand.
        operand: Box<BExpr>,
    },
    /// A binary operator.
    Binary {
        /// The operator.
        op: BBinaryOp,
        /// Left operand.
        lhs: Box<BExpr>,
        /// Right operand.
        rhs: Box<BExpr>,
    },
    /// `condition ? then : otherwise`; C selects by `condition != 0`.
    Ternary {
        /// The condition.
        condition: Box<BExpr>,
        /// Value when the condition is nonzero.
        then: Box<BExpr>,
        /// Value otherwise.
        otherwise: Box<BExpr>,
    },
    /// A function call by lowercased name: an `inpptree.c` built-in or a
    /// user `.func` (expanded by the front end before the device sees it).
    Call {
        /// Lowercased function name.
        name: String,
        /// Arguments in order; at least one.
        arguments: Vec<BExpr>,
    },
    /// A parenthesised sub-expression, kept so spans and shape survive.
    Group(Box<BExpr>),
    /// A numparam `{...}` value inside a `=pwl(` line, a TABLE point or a
    /// POLY coefficient: evaluated once by the front end, then a constant.
    Value(Box<ParameterExpression>),
    /// The transfer function of the XSPICE `pwl` code model that `inpcom.c`
    /// creates for an E/G `TABLE` (never written by a user; produced by
    /// [`crate::netlist::lower`]). See [`TableTransfer`].
    Table(Box<TableTransfer>),
}

/// Prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BUnaryOp {
    /// `-x` (C `PTF_UMINUS`).
    Minus,
    /// `+x` (no node in C).
    Plus,
    /// `!x`, C `eq0(x)`.
    Not,
}

/// Binary operators. Comparisons and logical operators are rewritten by C into
/// the `eq0`/`ne0`/`gt0`/`lt0`/`ge0`/`le0` functions of a difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BBinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `^` or `**` (C `PT_POWER`, `PTpowerH`).
    Pow,
    /// `==`, C `eq0(a-b)`.
    Eq,
    /// `!=` or `<>`, C `ne0(a-b)`.
    Ne,
    /// `<`, C `lt0(a-b)`.
    Lt,
    /// `>`, C `gt0(a-b)`.
    Gt,
    /// `<=`, C `le0(a-b)`.
    Le,
    /// `>=`, C `ge0(a-b)`.
    Ge,
    /// `&&`, C `eq0(eq0(a)+eq0(b))`.
    And,
    /// `||`, C `ne0(ne0(a)+ne0(b))`.
    Or,
}

impl BBinaryOp {
    /// The operator's canonical spelling.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Pow => "^",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::Le => "<=",
            Self::Ge => ">=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

/// The input-to-output map of the XSPICE `pwl` code model
/// (`src/xspice/icm/analog/pwl/cfunc.mod`) with the parameters `inpcom.c`
/// gives it for a `TABLE`: `fraction=TRUE limit=TRUE` and an
/// `input_domain` of 0.1 (or 0.001 for the four-node LTspice form).
#[derive(Debug, Clone, PartialEq)]
pub struct TableTransfer {
    /// The input expression.
    pub input: BExpr,
    /// `(x, y)` points as written: [`BExprKind::Number`] or
    /// [`BExprKind::Value`] (possibly signed).
    pub points: Vec<(BExpr, BExpr)>,
    /// The smoothing domain as a fraction of the shorter adjacent segment.
    pub domain: Real,
}

/// The function names of `inpptree.c`'s `funcs[]` table (plus the
/// parse-time `ternary_fcn` and `gauss`), in that order. Only these names are
/// callable in a B expression; anything else must be a user `.func`.
pub const BUILTIN_FUNCTIONS: &[&str] = &[
    "abs",
    "acos",
    "acosh",
    "asin",
    "asinh",
    "atan",
    "atanh",
    "cos",
    "cosh",
    "exp",
    "ln",
    "log",
    "log10",
    "sgn",
    "sin",
    "sinh",
    "sqrt",
    "tan",
    "tanh",
    "u",
    "uramp",
    "ceil",
    "floor",
    "nint",
    "u2",
    "pwl",
    "pwl_derivative",
    "eq0",
    "ne0",
    "gt0",
    "lt0",
    "ge0",
    "le0",
    "pow",
    "pwr",
    "min",
    "max",
    "ddt",
    "ternary_fcn",
    "gauss",
];

/// The statistical functions C replaces in B lines before parsing them
/// (`src/frontend/inp.c` `eval_agauss()`, over the same set as
/// `src/frontend/inpcom.c` `inp_fix_agauss_in_param()`). A user `.func` of
/// the same name takes precedence, as C expands macros first.
pub const STATISTICAL_FUNCTIONS: &[&str] = &["agauss", "gauss", "aunif", "unif", "limit"];

/// True for a name the B-source parser knows as a function.
#[must_use]
pub fn is_builtin_function(name: &str) -> bool {
    BUILTIN_FUNCTIONS.contains(&name)
}

/// Names that `inp_modify_exp()` leaves unsubstituted and `PT_mksnode()`
/// turns into circuit quantities or constants.
pub const SPECIAL_NAMES: &[&str] = &["time", "temper", "hertz", "pi", "e"];

impl BExpr {
    /// Visits every node, parents before children.
    pub fn visit<'a>(&'a self, visitor: &mut impl FnMut(&'a BExpr)) {
        visitor(self);
        match &self.kind {
            BExprKind::Number { .. }
            | BExprKind::Name(_)
            | BExprKind::Voltage { .. }
            | BExprKind::Current(_)
            | BExprKind::Value(_) => {}
            BExprKind::Unary { operand, .. } | BExprKind::Group(operand) => {
                operand.visit(visitor);
            }
            BExprKind::Binary { lhs, rhs, .. } => {
                lhs.visit(visitor);
                rhs.visit(visitor);
            }
            BExprKind::Ternary {
                condition,
                then,
                otherwise,
            } => {
                condition.visit(visitor);
                then.visit(visitor);
                otherwise.visit(visitor);
            }
            BExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    argument.visit(visitor);
                }
            }
            BExprKind::Table(table) => {
                table.input.visit(visitor);
                for (x, y) in &table.points {
                    x.visit(visitor);
                    y.visit(visitor);
                }
            }
        }
    }

    /// Visits every node mutably, parents before children.
    pub fn visit_mut(&mut self, visitor: &mut impl FnMut(&mut BExpr)) {
        visitor(self);
        match &mut self.kind {
            BExprKind::Number { .. }
            | BExprKind::Name(_)
            | BExprKind::Voltage { .. }
            | BExprKind::Current(_)
            | BExprKind::Value(_) => {}
            BExprKind::Unary { operand, .. } | BExprKind::Group(operand) => {
                operand.visit_mut(visitor);
            }
            BExprKind::Binary { lhs, rhs, .. } => {
                lhs.visit_mut(visitor);
                rhs.visit_mut(visitor);
            }
            BExprKind::Ternary {
                condition,
                then,
                otherwise,
            } => {
                condition.visit_mut(visitor);
                then.visit_mut(visitor);
                otherwise.visit_mut(visitor);
            }
            BExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    argument.visit_mut(visitor);
                }
            }
            BExprKind::Table(table) => {
                table.input.visit_mut(visitor);
                for (x, y) in &mut table.points {
                    x.visit_mut(visitor);
                    y.visit_mut(visitor);
                }
            }
        }
    }

    /// Every node name referenced by `v(...)`, in source order with
    /// duplicates kept.
    #[must_use]
    pub fn node_references(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.visit(&mut |node| {
            if let BExprKind::Voltage { positive, negative } = &node.kind {
                names.push(positive.as_str());
                if let Some(negative) = negative {
                    names.push(negative.as_str());
                }
            }
        });
        names
    }

    /// Every source named by `i(...)`, in source order with duplicates kept.
    #[must_use]
    pub fn current_references(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.visit(&mut |node| {
            if let BExprKind::Current(name) = &node.kind {
                names.push(name.as_str());
            }
        });
        names
    }
}

/// Rounds a value to the 11 significant digits that `inp_modify_exp()`
/// (`src/frontend/inpcom.c`) keeps when it re-prints a B-source literal with
/// `%18.10e`. Non-finite input is returned unchanged.
#[must_use]
pub fn round_like_c_literal(value: Real) -> Real {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.10e}").parse().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_rounded_to_eleven_significant_digits() {
        assert_eq!(round_like_c_literal(1.234_567_890_123_456), 1.234_567_890_1);
        assert_eq!(round_like_c_literal(-2.5e-3), -2.5e-3);
        assert_eq!(round_like_c_literal(0.0), 0.0);
        assert_eq!(round_like_c_literal(1e300), 1e300);
        assert_eq!(round_like_c_literal(3.333_333_333_333_333), 3.333_333_333_3);
    }

    #[test]
    fn builtin_table_matches_inpptree() {
        for name in ["u", "uramp", "u2", "pwl", "ddt", "min", "max", "nint"] {
            assert!(is_builtin_function(name), "{name}");
        }
        for name in ["sqr", "arctan", "int", "limit", "if"] {
            assert!(!is_builtin_function(name), "{name}");
        }
    }
}
