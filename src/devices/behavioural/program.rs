//! Compiled behavioural expressions: value and analytic derivatives.
//!
//! C references (behaviour only): `src/spicelib/parser/inpptree.c`
//! (`PT_mkfnode`, `PTdifferentiate`, `prepare_PTF_PWL`), `ptfuncs.c` (the
//! function values and domain rules) and `ifeval.c` (`PTeval`). Like ngspice,
//! the port keeps the parse tree unsimplified, differentiates it symbolically
//! once per quantity with C's rules and its `mkb`/`mkf` simplifications
//! (constant folding, the 0/1 identities), and evaluates the value tree and
//! each derivative tree node by node, so values and derivatives are C's
//! operation for operation. That includes the deliberate non-derivatives
//! (`floor`, comparisons, `u` and `pwl_derivative` are zero or piecewise) and
//! the domain quirks of the value functions:
//!
//! - division adds `gmin * 1e-20` to the divisor away from zero (`PTdivide`),
//!   so `1/0` is `1e32` with the default gmin;
//! - `exp(x)` is `1e99` above `x = 227.9559242`; `log(0)` and `log10(0)` are
//!   `-1e99`; `log`, `log10` and `sqrt` of a negative number are errors;
//! - `sin`, `cos` and `tan` reduce their argument with C's
//!   `x - (int)(x / 2pi) * 2pi` (`pi` for `tan`) first;
//! - `^` is `pow(a, b)` for `a >= 0`, `pow(a, round(b))` for `a < 0` and a
//!   quasi-integer `b` (10 ulps), otherwise 0 (`PTpowerH`, default
//!   compatibility); `pow()` is the same except `pow(0, b) = 0` (`PTpower`);
//!   `pwr(a, b) = sign(a) |a|^b`;
//! - `u(0) = 0.5`; `nint` rounds half to even; `pwl()` extrapolates the end
//!   segments linearly, and its derivative uses C's ascending-only search
//!   (`PTpwl_derivative`) even for descending tables.
//!
//! Where C silently continues with a NaN (for example `acos(2)`) or an
//! infinity, the port stops with an explicit numerical error naming the
//! function. The PSPICE/HSPICE/LTspice compatibility variants (`newcompat`)
//! are not modelled: the reference binary runs without a compatibility mode.

use crate::netlist::bexpr::{BBinaryOp, BExpr, BExprKind, BUnaryOp, STATISTICAL_FUNCTIONS};
use crate::primitives::{Real, SpiceError, SpiceResult};

use std::sync::Arc;

use super::xspice::XspicePwl;

/// A circuit quantity an expression reads, in first-use order (C
/// `mkvnode`/`mkinode` deduplicate them the same way).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quantity {
    /// A node voltage, by canonical node name.
    Node(String),
    /// The branch current of a named device (`i(name)`).
    Branch(String),
}

/// What an evaluation reads besides the variables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment<'a> {
    /// Present values of the [`Program::quantities`], in order.
    pub values: &'a [Real],
    /// `time` in seconds (0 outside transient analysis, as `CKTtime`).
    pub time: Real,
    /// `temper`: the circuit temperature in degrees Celsius.
    pub temperature: Real,
    /// The circuit gmin; `PTdivide` adds `gmin * 1e-20` to divisors.
    pub gmin: Real,
    /// `hertz`: the analysis frequency in Hz (C `CKTomega / 2 pi`).
    pub frequency: Real,
}

/// A value and its partial derivatives with respect to each quantity.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// The expression value.
    pub value: Real,
    /// `d value / d quantity[i]`.
    pub gradient: Vec<Real>,
}

/// One-argument functions (`inpptree.c` `funcs[]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unary {
    Abs,
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Cos,
    Cosh,
    Exp,
    Log,
    Log10,
    Sgn,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
    Ustep,
    Uramp,
    Ceil,
    Floor,
    Nint,
    Uminus,
    Ustep2,
    Eq0,
    Ne0,
    Gt0,
    Lt0,
    Ge0,
    Le0,
}

impl Unary {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "abs" => Self::Abs,
            "acos" => Self::Acos,
            "acosh" => Self::Acosh,
            "asin" => Self::Asin,
            "asinh" => Self::Asinh,
            "atan" => Self::Atan,
            "atanh" => Self::Atanh,
            "cos" => Self::Cos,
            "cosh" => Self::Cosh,
            "exp" => Self::Exp,
            "ln" | "log" => Self::Log,
            "log10" => Self::Log10,
            "sgn" => Self::Sgn,
            "sin" => Self::Sin,
            "sinh" => Self::Sinh,
            "sqrt" => Self::Sqrt,
            "tan" => Self::Tan,
            "tanh" => Self::Tanh,
            "u" => Self::Ustep,
            "uramp" => Self::Uramp,
            "ceil" => Self::Ceil,
            "floor" => Self::Floor,
            "nint" => Self::Nint,
            "u2" => Self::Ustep2,
            "eq0" => Self::Eq0,
            "ne0" => Self::Ne0,
            "gt0" => Self::Gt0,
            "lt0" => Self::Lt0,
            "ge0" => Self::Ge0,
            "le0" => Self::Le0,
            _ => return None,
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Acos => "acos",
            Self::Acosh => "acosh",
            Self::Asin => "asin",
            Self::Asinh => "asinh",
            Self::Atan => "atan",
            Self::Atanh => "atanh",
            Self::Cos => "cos",
            Self::Cosh => "cosh",
            Self::Exp => "exp",
            Self::Log => "log",
            Self::Log10 => "log10",
            Self::Sgn => "sgn",
            Self::Sin => "sin",
            Self::Sinh => "sinh",
            Self::Sqrt => "sqrt",
            Self::Tan => "tan",
            Self::Tanh => "tanh",
            Self::Ustep => "u",
            Self::Uramp => "uramp",
            Self::Ceil => "ceil",
            Self::Floor => "floor",
            Self::Nint => "nint",
            Self::Uminus => "-",
            Self::Ustep2 => "u2",
            Self::Eq0 => "eq0",
            Self::Ne0 => "ne0",
            Self::Gt0 => "gt0",
            Self::Lt0 => "lt0",
            Self::Ge0 => "ge0",
            Self::Le0 => "le0",
        }
    }
}

/// Binary operators (`PT_PLUS` ... `PT_POWER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Plus,
    Minus,
    Times,
    Divide,
    Power,
}

/// Two-argument functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binary {
    Pow,
    Pwr,
    Min,
    Max,
}

/// A shared subtree: C shares subtrees between the function and its
/// derivative trees (`PTdifferentiate`, `inc_usage`).
type Tree = Arc<Node>;

/// A parse-tree node (`INPparseNode`).
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Constant(Real),
    Variable(usize),
    Time,
    Temperature,
    Frequency,
    Unary(Unary, Tree),
    Operator(Operator, Tree, Tree),
    Binary(Binary, Tree, Tree),
    Ternary(Tree, Tree, Tree),
    /// `pwl(x, x0, y0, x1, y1, ...)`: flattened literal points.
    Pwl(Tree, Vec<Real>),
    /// `PTF_PWL_DERIVATIVE`: the slope `PTdifferentiate` builds for `pwl()`.
    PwlDerivative(Tree, Vec<Real>),
    /// The XSPICE `pwl` transfer of a lowered TABLE.
    Table(Tree, XspicePwl),
    /// The slope of [`Node::Table`] (the code model's partial).
    TableSlope(Tree, XspicePwl),
}

impl Node {
    const fn constant(&self) -> Option<Real> {
        match self {
            Self::Constant(value) => Some(*value),
            _ => None,
        }
    }
}

/// A compiled behavioural expression: the value tree and, per quantity, the
/// derivative tree `PTdifferentiate` builds for it (`INPgetTree`).
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    root: Tree,
    derivatives: Vec<Tree>,
    quantities: Vec<Quantity>,
}

fn unsupported(expr: &BExpr, message: impl Into<String>) -> SpiceError {
    SpiceError::Unsupported {
        feature: message.into(),
        location: Some(expr.span.start.clone()),
    }
}

fn parse_error(expr: &BExpr, message: impl Into<String>) -> SpiceError {
    SpiceError::parse(expr.span.start.clone(), message)
}

impl Program {
    /// Compiles a front-end-resolved expression tree and builds its
    /// derivative trees.
    ///
    /// # Errors
    /// Names the front end should have resolved (`.param` values, `.func`
    /// calls), unknown functions or wrong argument counts (positioned parse
    /// errors), non-literal or non-monotonic `pwl()` points (C:
    /// `prepare_PTF_PWL`), and [`SpiceError::NotYetPorted`] for `ddt()` and
    /// the statistical functions (`agauss`, `gauss`, `aunif`, `unif`,
    /// `limit`; normally already rejected by the front end).
    pub fn compile(expr: &BExpr) -> SpiceResult<Self> {
        let mut quantities = Vec::new();
        let root = Arc::new(compile(expr, &mut quantities)?);
        let derivatives = (0..quantities.len())
            .map(|variable| differentiate(&root, variable))
            .collect();
        Ok(Self {
            root,
            derivatives,
            quantities,
        })
    }

    /// True when the expression reads `hertz`.
    #[must_use]
    pub fn uses_frequency(&self) -> bool {
        fn walk(node: &Node) -> bool {
            match node {
                Node::Frequency => true,
                Node::Constant(_) | Node::Variable(_) | Node::Time | Node::Temperature => false,
                Node::Unary(_, a)
                | Node::Pwl(a, _)
                | Node::PwlDerivative(a, _)
                | Node::Table(a, _)
                | Node::TableSlope(a, _) => walk(a),
                Node::Operator(_, a, b) | Node::Binary(_, a, b) => walk(a) || walk(b),
                Node::Ternary(a, b, c) => walk(a) || walk(b) || walk(c),
            }
        }
        walk(&self.root)
    }

    /// The circuit quantities the expression reads, in variable order.
    #[must_use]
    pub fn quantities(&self) -> &[Quantity] {
        &self.quantities
    }

    /// The value and every partial derivative at `environment`: `IFeval`
    /// evaluates the value tree, then each derivative tree (`PTeval`).
    ///
    /// # Errors
    /// [`SpiceError::Numerical`] when a function is evaluated outside its
    /// domain or any value/derivative is not finite.
    pub fn evaluate(&self, environment: &Environment<'_>) -> SpiceResult<Evaluation> {
        if environment.values.len() != self.quantities.len() {
            return Err(SpiceError::circuit(
                "behavioural expression evaluated with the wrong number of values",
            ));
        }
        let evaluator = Evaluator {
            environment,
            fudge: environment.gmin * 1e-20,
        };
        let value = evaluator.eval(&self.root)?;
        let gradient = self
            .derivatives
            .iter()
            .map(|tree| evaluator.eval(tree))
            .collect::<SpiceResult<Vec<_>>>()?;
        Ok(Evaluation { value, gradient })
    }
}

fn variable(quantities: &mut Vec<Quantity>, quantity: Quantity) -> Node {
    let index = quantities
        .iter()
        .position(|q| *q == quantity)
        .unwrap_or_else(|| {
            quantities.push(quantity);
            quantities.len() - 1
        });
    Node::Variable(index)
}

fn boxed(expr: &BExpr, quantities: &mut Vec<Quantity>) -> SpiceResult<Tree> {
    compile(expr, quantities).map(Arc::new)
}

fn compile(expr: &BExpr, quantities: &mut Vec<Quantity>) -> SpiceResult<Node> {
    Ok(match &expr.kind {
        BExprKind::Number { value, .. } => Node::Constant(*value),
        BExprKind::Name(name) => match name.as_str() {
            "time" => Node::Time,
            "temper" => Node::Temperature,
            "pi" => Node::Constant(std::f64::consts::PI),
            "e" => Node::Constant(std::f64::consts::E),
            "hertz" => Node::Frequency,
            other => {
                return Err(parse_error(
                    expr,
                    format!(
                        "unresolved name '{other}' in a behavioural expression (the front end \
                         substitutes .param values; elaborate the netlist first)"
                    ),
                ));
            }
        },
        BExprKind::Voltage { positive, negative } => {
            let plus = variable(quantities, Quantity::Node(positive.clone()));
            match negative {
                None => plus,
                Some(negative) => {
                    let minus = variable(quantities, Quantity::Node(negative.clone()));
                    Node::Operator(Operator::Minus, Arc::new(plus), Arc::new(minus))
                }
            }
        }
        BExprKind::Current(name) => variable(quantities, Quantity::Branch(name.clone())),
        BExprKind::Unary { op, operand } => {
            let operand = boxed(operand, quantities)?;
            match op {
                BUnaryOp::Plus => Arc::unwrap_or_clone(operand),
                BUnaryOp::Minus => Node::Unary(Unary::Uminus, operand),
                BUnaryOp::Not => Node::Unary(Unary::Eq0, operand),
            }
        }
        BExprKind::Binary { op, lhs, rhs } => {
            let lhs = boxed(lhs, quantities)?;
            let rhs = boxed(rhs, quantities)?;
            let difference = |lhs, rhs| Arc::new(Node::Operator(Operator::Minus, lhs, rhs));
            match op {
                BBinaryOp::Add => Node::Operator(Operator::Plus, lhs, rhs),
                BBinaryOp::Sub => Node::Operator(Operator::Minus, lhs, rhs),
                BBinaryOp::Mul => Node::Operator(Operator::Times, lhs, rhs),
                BBinaryOp::Div => Node::Operator(Operator::Divide, lhs, rhs),
                BBinaryOp::Pow => Node::Operator(Operator::Power, lhs, rhs),
                BBinaryOp::Eq => Node::Unary(Unary::Eq0, difference(lhs, rhs)),
                BBinaryOp::Ne => Node::Unary(Unary::Ne0, difference(lhs, rhs)),
                BBinaryOp::Lt => Node::Unary(Unary::Lt0, difference(lhs, rhs)),
                BBinaryOp::Gt => Node::Unary(Unary::Gt0, difference(lhs, rhs)),
                BBinaryOp::Le => Node::Unary(Unary::Le0, difference(lhs, rhs)),
                BBinaryOp::Ge => Node::Unary(Unary::Ge0, difference(lhs, rhs)),
                // inpptree-parser.y: a && b is eq0(eq0(a) + eq0(b)), a || b is
                // ne0(ne0(a) + ne0(b)).
                BBinaryOp::And => Node::Unary(
                    Unary::Eq0,
                    Arc::new(Node::Operator(
                        Operator::Plus,
                        Arc::new(Node::Unary(Unary::Eq0, lhs)),
                        Arc::new(Node::Unary(Unary::Eq0, rhs)),
                    )),
                ),
                BBinaryOp::Or => Node::Unary(
                    Unary::Ne0,
                    Arc::new(Node::Operator(
                        Operator::Plus,
                        Arc::new(Node::Unary(Unary::Ne0, lhs)),
                        Arc::new(Node::Unary(Unary::Ne0, rhs)),
                    )),
                ),
            }
        }
        BExprKind::Ternary {
            condition,
            then,
            otherwise,
        } => Node::Ternary(
            boxed(condition, quantities)?,
            boxed(then, quantities)?,
            boxed(otherwise, quantities)?,
        ),
        BExprKind::Group(inner) => compile(inner, quantities)?,
        BExprKind::Call { name, arguments } => call(expr, name, arguments, quantities)?,
        BExprKind::Value(_) => {
            return Err(parse_error(
                expr,
                "unevaluated numparam value in a behavioural expression (elaborate the netlist \
                 first)",
            ));
        }
        BExprKind::Table(table) => {
            let mut points = Vec::with_capacity(table.points.len());
            for (x, y) in &table.points {
                points.push((literal(x)?, literal(y)?));
            }
            let map = XspicePwl::new(&points, table.domain).map_err(|error| match error {
                SpiceError::Unsupported { feature, .. } => unsupported(expr, feature),
                other => other,
            })?;
            Node::Table(boxed(&table.input, quantities)?, map)
        }
    })
}

/// A literal number, possibly parenthesised and/or signed once
/// (`prepare_PTF_PWL` accepts a constant or `-constant`).
fn literal(expr: &BExpr) -> SpiceResult<Real> {
    match &expr.kind {
        BExprKind::Number { value, .. } => Ok(*value),
        BExprKind::Group(inner) => literal(inner),
        BExprKind::Unary {
            op: BUnaryOp::Plus,
            operand,
        } => literal(operand),
        BExprKind::Unary {
            op: BUnaryOp::Minus,
            operand,
        } => match &strip_groups(operand).kind {
            BExprKind::Number { value, .. } => Ok(-value),
            _ => Err(not_literal(expr)),
        },
        _ => Err(not_literal(expr)),
    }
}

fn strip_groups(expr: &BExpr) -> &BExpr {
    match &expr.kind {
        BExprKind::Group(inner) => strip_groups(inner),
        _ => expr,
    }
}

fn not_literal(expr: &BExpr) -> SpiceError {
    parse_error(
        expr,
        "PWL(expr, points...) only *literal* points are supported (C: prepare_PTF_PWL)",
    )
}

fn call(
    expr: &BExpr,
    name: &str,
    arguments: &[BExpr],
    quantities: &mut Vec<Quantity>,
) -> SpiceResult<Node> {
    let arity = |expected: usize| -> SpiceResult<()> {
        if arguments.len() == expected {
            Ok(())
        } else {
            Err(parse_error(
                expr,
                format!(
                    "function '{name}' takes {expected} argument(s), found {}",
                    arguments.len()
                ),
            ))
        }
    };
    if let Some(function) = Unary::from_name(name) {
        arity(1)?;
        return Ok(Node::Unary(function, boxed(&arguments[0], quantities)?));
    }
    let two = |function: Binary, quantities: &mut Vec<Quantity>| -> SpiceResult<Node> {
        arity(2)?;
        Ok(Node::Binary(
            function,
            boxed(&arguments[0], quantities)?,
            boxed(&arguments[1], quantities)?,
        ))
    };
    match name {
        "pow" => two(Binary::Pow, quantities),
        "pwr" => two(Binary::Pwr, quantities),
        "min" => two(Binary::Min, quantities),
        "max" => two(Binary::Max, quantities),
        "ternary_fcn" => {
            arity(3)?;
            Ok(Node::Ternary(
                boxed(&arguments[0], quantities)?,
                boxed(&arguments[1], quantities)?,
                boxed(&arguments[2], quantities)?,
            ))
        }
        "pwl" => pwl(expr, arguments, quantities),
        "ddt" => Err(SpiceError::not_yet_ported(
            format!(
                "{}: ddt() in a behavioural source (a stateful time derivative of \
                 iteration values)",
                expr.span.start
            ),
            "src/spicelib/parser/ptfuncs.c (PTddt)",
        )),
        name if STATISTICAL_FUNCTIONS.contains(&name) => Err(SpiceError::not_yet_ported(
            format!(
                "{}: {name}() in a behavioural source (a random value drawn once before \
                 the expression is parsed)",
                expr.span.start
            ),
            "src/frontend/inp.c (eval_agauss); src/frontend/inpcom.c \
             (inp_fix_agauss_in_param)",
        )),
        "pwl_derivative" => Err(unsupported(
            expr,
            "pwl_derivative() is internal to pwl() and has no table when called directly \
             (C dereferences a missing table)",
        )),
        _ => Err(parse_error(
            expr,
            format!("no such function '{name}' (src/spicelib/parser/inpptree.c PT_mkfnode)"),
        )),
    }
}

fn pwl(expr: &BExpr, arguments: &[BExpr], quantities: &mut Vec<Quantity>) -> SpiceResult<Node> {
    let Some((input, points)) = arguments.split_first() else {
        return Err(parse_error(expr, "pwl() needs an input and points"));
    };
    if points.len() < 4 || points.len() % 2 != 0 {
        return Err(parse_error(
            expr,
            format!(
                "PWL(expr, points...) needs an even number (>= 4) of point values, found {} \
                 (C: prepare_PTF_PWL)",
                points.len()
            ),
        ));
    }
    let values = points
        .iter()
        .map(literal)
        .collect::<SpiceResult<Vec<_>>>()?;
    let abscissas: Vec<Real> = values.iter().step_by(2).copied().collect();
    let monotonic = if abscissas[0] < abscissas[1] {
        abscissas.windows(2).all(|w| w[0] <= w[1])
    } else if abscissas[0] > abscissas[1] {
        abscissas.windows(2).all(|w| w[0] >= w[1])
    } else {
        false
    };
    if !monotonic {
        return Err(parse_error(
            expr,
            "PWL(expr, points...) the abscissa of points must be monotonic \
             (C: prepare_PTF_PWL)",
        ));
    }
    Ok(Node::Pwl(boxed(input, quantities)?, values))
}

/// `PTpwl`: binary search (ascending or descending abscissa), then linear
/// interpolation in the found segment, extrapolating at the ends.
fn pwl_value(arg: Real, values: &[Real]) -> Real {
    let x = |k: usize| values[2 * k];
    let y = |k: usize| values[2 * k + 1];
    let (mut k0, mut k1) = (0usize, values.len() / 2 - 1);
    let ascending = x(0) < x(1);
    while k1 - k0 > 1 {
        let k = usize::midpoint(k0, k1);
        let upper = if ascending { x(k) > arg } else { x(k) < arg };
        if upper {
            k1 = k;
        } else {
            k0 = k;
        }
    }
    y(k0) + (y(k1) - y(k0)) * (arg - x(k0)) / (x(k1) - x(k0))
}

/// `PTpwl_derivative`: the slope of the segment an **ascending** search
/// selects, whatever the table order (C quirk, kept).
fn pwl_slope(arg: Real, values: &[Real]) -> Real {
    let x = |k: usize| values[2 * k];
    let y = |k: usize| values[2 * k + 1];
    let (mut k0, mut k1) = (0usize, values.len() / 2 - 1);
    while k1 - k0 > 1 {
        let k = usize::midpoint(k0, k1);
        if x(k) > arg {
            k1 = k;
        } else {
            k0 = k;
        }
    }
    (y(k1) - y(k0)) / (x(k1) - x(k0))
}

/// Dawson's ULP comparison (`AlmostEqualUlps`, `src/maths/misc/equality.c`).
fn almost_equal_ulps(a: Real, b: Real, max_ulps: i64) -> bool {
    if a == b {
        return true;
    }
    let ordered = |value: Real| {
        let bits = i64::from_ne_bytes(value.to_bits().to_ne_bytes());
        if bits < 0 { i64::MIN - bits } else { bits }
    };
    ordered(a)
        .checked_sub(ordered(b))
        .is_some_and(|difference| difference.abs() <= max_ulps)
}

/// `PTpowerH` without a compatibility mode (the `^` operator).
fn power_h(a: Real, b: Real) -> Real {
    if a >= 0. {
        c_pow(a, b)
    } else if almost_equal_ulps(b.round_ties_even(), b, 10) {
        c_pow(a, b.round())
    } else {
        0.
    }
}

/// `PTpower` (the `pow()` function).
fn power(a: Real, b: Real) -> Real {
    if a == 0. {
        0.
    } else if a > 0. {
        c_pow(a, b)
    } else if almost_equal_ulps(b.round_ties_even(), b, 10) {
        c_pow(a, b.round())
    } else {
        0.
    }
}

/// `PTpwr` without a compatibility mode.
fn pwr(a: Real, b: Real) -> Real {
    if a < 0. { -c_pow(-a, b) } else { c_pow(a, b) }
}

/// C's `MODULUS(x, limit)`: `x - (int)(x / limit) * limit` (the `int`
/// conversion saturates, as on the reference platform).
fn modulus(x: Real, limit: Real) -> Real {
    #[allow(clippy::cast_possible_truncation)]
    let whole = (x / limit) as i32;
    x - Real::from(whole) * limit
}

const TWO_PI: Real = 2. * std::f64::consts::PI;
const EXP_LIMIT: Real = 227.955_924_2;

/// C's `pow()`. The exponent is opaque to the optimiser so that a constant
/// exponent (`x^2`) is not rewritten to `x * x`: the platform `pow` is not
/// always correctly rounded, and C calls it.
fn c_pow(a: Real, b: Real) -> Real {
    a.powf(std::hint::black_box(b))
}

/// The `ptfuncs.c` one-argument functions. C's `HUGE` (out of range) is
/// [`Real::INFINITY`].
fn unary_value(function: Unary, x: Real) -> Real {
    match function {
        Unary::Abs => x.abs(),
        Unary::Sgn => {
            if x > 0. {
                1.
            } else if x < 0. {
                -1.
            } else {
                0.
            }
        }
        Unary::Acos => x.acos(),
        Unary::Acosh => x.acosh(),
        Unary::Asin => x.asin(),
        Unary::Asinh => x.asinh(),
        Unary::Atan => x.atan(),
        Unary::Atanh => x.atanh(),
        Unary::Cos => modulus(x, TWO_PI).cos(),
        Unary::Cosh => x.cosh(),
        Unary::Exp => {
            if x > EXP_LIMIT {
                1e99
            } else {
                x.exp()
            }
        }
        Unary::Log => {
            if x < 0. {
                Real::INFINITY
            } else if x == 0. {
                -1e99
            } else {
                x.ln()
            }
        }
        Unary::Log10 => {
            if x < 0. {
                Real::INFINITY
            } else if x == 0. {
                -1e99
            } else {
                x.log10()
            }
        }
        Unary::Sin => modulus(x, TWO_PI).sin(),
        Unary::Sinh => x.sinh(),
        Unary::Sqrt => {
            if x < 0. {
                Real::INFINITY
            } else {
                x.sqrt()
            }
        }
        Unary::Tan => modulus(x, std::f64::consts::PI).tan(),
        Unary::Tanh => x.tanh(),
        Unary::Ustep => {
            if x < 0. {
                0.
            } else if x > 0. {
                1.
            } else {
                0.5
            }
        }
        Unary::Uramp => {
            if x < 0. {
                0.
            } else {
                x
            }
        }
        Unary::Ceil => x.ceil(),
        Unary::Floor => x.floor(),
        Unary::Nint => x.round_ties_even(),
        Unary::Uminus => -x,
        Unary::Ustep2 => {
            if x <= 0. {
                0.
            } else if x <= 1. {
                x
            } else {
                1.
            }
        }
        Unary::Eq0 => flag(x == 0.),
        Unary::Ne0 => flag(x != 0.),
        Unary::Gt0 => flag(x > 0.),
        Unary::Lt0 => flag(x < 0.),
        Unary::Ge0 => flag(x >= 0.),
        Unary::Le0 => flag(x <= 0.),
    }
}

const fn flag(condition: bool) -> Real {
    if condition { 1. } else { 0. }
}

/// `PTdivide`: the divisor moves `fudge` away from zero; a zero divisor
/// after that is `HUGE`.
fn divide(a: Real, b: Real, fudge: Real) -> Real {
    let b = if b >= 0. { b + fudge } else { b - fudge };
    if b == 0. { Real::INFINITY } else { a / b }
}

fn constant(value: Real) -> Tree {
    Arc::new(Node::Constant(value))
}

/// `mkcon(0.0)` / `mkcon(1.0)` for a variable.
fn zero() -> Tree {
    constant(0.)
}

/// `mkb()` for the arithmetic operators: constant folding (plain C
/// arithmetic and `pow`, not the `PT*` functions) and the 0/1 identities.
fn mk_operator(op: Operator, left: Tree, right: Tree) -> Tree {
    if let (Some(l), Some(r)) = (left.constant(), right.constant()) {
        return constant(match op {
            Operator::Times => l * r,
            Operator::Divide => l / r,
            Operator::Plus => l + r,
            Operator::Minus => l - r,
            Operator::Power => c_pow(l, r),
        });
    }
    let (l, r) = (left.constant(), right.constant());
    match op {
        Operator::Times => {
            // C's order: a zero factor (either side) wins, then a unit one.
            if l == Some(0.) {
                return left;
            } else if r == Some(0.) || l == Some(1.) {
                return right;
            } else if r == Some(1.) {
                return left;
            }
        }
        Operator::Divide => {
            if l == Some(0.) || r == Some(1.) {
                return left;
            }
        }
        Operator::Plus => {
            if l == Some(0.) {
                return right;
            } else if r == Some(0.) {
                return left;
            }
        }
        Operator::Minus => {
            if r == Some(0.) {
                return left;
            } else if l == Some(0.) {
                return mk_unary(Unary::Uminus, right);
            }
        }
        Operator::Power => {
            if r == Some(0.) {
                return constant(1.);
            } else if r == Some(1.) {
                return left;
            }
        }
    }
    Arc::new(Node::Operator(op, left, right))
}

/// `mkb(PT_TERN, condition, mkb(PT_COMMA, then, otherwise))`.
fn mk_ternary(condition: Tree, then: Tree, otherwise: Tree) -> Tree {
    if let Some(condition) = condition.constant() {
        return if condition != 0. { then } else { otherwise };
    }
    if let (Some(a), Some(b)) = (then.constant(), otherwise.constant())
        && a == b
    {
        return then;
    }
    Arc::new(Node::Ternary(condition, then, otherwise))
}

/// `mkf()` for a one-argument function: a constant argument is folded.
fn mk_unary(function: Unary, argument: Tree) -> Tree {
    match argument.constant() {
        Some(x) => constant(unary_value(function, x)),
        None => Arc::new(Node::Unary(function, argument)),
    }
}

/// `mkf(PTF_POW/PTF_PWR, mkb(PT_COMMA, a, b))`: never folded (a comma node
/// is not a constant).
fn mk_binary(function: Binary, a: &Tree, b: Tree) -> Tree {
    Arc::new(Node::Binary(function, a.clone(), b))
}

/// `mkb(PT_TIMES, ...)` shorthand.
fn times(left: Tree, right: Tree) -> Tree {
    mk_operator(Operator::Times, left, right)
}

/// `mkb(PT_DIVIDE, ...)` shorthand.
fn over(left: Tree, right: Tree) -> Tree {
    mk_operator(Operator::Divide, left, right)
}

/// `mkb(PT_POWER, u, mkcon(2.0))`.
fn squared(u: &Tree) -> Tree {
    mk_operator(Operator::Power, u.clone(), constant(2.))
}

/// `mkf(PTF_LOG, mkf(PTF_ABS, a))`.
fn log_abs(a: &Tree) -> Tree {
    mk_unary(Unary::Log, mk_unary(Unary::Abs, a.clone()))
}

/// The general power rule `f(a,b) * (b * (D(a) / a) + D(b) * log(abs(a)))`
/// of `PTdifferentiate` (`^`, `pow()`, `pwr()`), with `outer = f(a,b)`.
fn general_power(outer: Tree, a: &Tree, b: &Tree, variable: usize) -> Tree {
    times(
        outer,
        mk_operator(
            Operator::Plus,
            times(b.clone(), over(differentiate(a, variable), a.clone())),
            times(differentiate(b, variable), log_abs(a)),
        ),
    )
}

/// `PTdifferentiate` (`inpptree.c`): the derivative tree of `p` with respect
/// to quantity `variable`, built with C's `mkb`/`mkf` simplifications so its
/// evaluation is C's operation for operation. The PSPICE/HSPICE/LTspice
/// `newcompat` variants are not modelled.
fn differentiate(p: &Tree, variable: usize) -> Tree {
    match &**p {
        Node::Constant(_) | Node::Time | Node::Temperature | Node::Frequency => zero(),
        Node::Variable(index) => constant(if *index == variable { 1. } else { 0. }),
        Node::Operator(op @ (Operator::Plus | Operator::Minus), a, b) => {
            mk_operator(*op, differentiate(a, variable), differentiate(b, variable))
        }
        // d(a * b) = d(a) * b + a * d(b)
        Node::Operator(Operator::Times, a, b) => mk_operator(
            Operator::Plus,
            times(differentiate(a, variable), b.clone()),
            times(a.clone(), differentiate(b, variable)),
        ),
        // d(a / b) = (d(a) * b - a * d(b)) / b^2
        Node::Operator(Operator::Divide, a, b) => over(
            mk_operator(
                Operator::Minus,
                times(differentiate(a, variable), b.clone()),
                times(a.clone(), differentiate(b, variable)),
            ),
            squared(b),
        ),
        Node::Operator(Operator::Power, a, b) => {
            if let Some(exponent) = b.constant() {
                // b * pwr(a, b - 1) * d(a)
                times(
                    times(
                        constant(exponent),
                        mk_binary(Binary::Pwr, a, constant(exponent - 1.)),
                    ),
                    differentiate(a, variable),
                )
            } else if a.constant().is_some() {
                // pow(a, b) * (d(b) * log(abs(a)))
                times(
                    mk_binary(Binary::Pow, a, b.clone()),
                    times(differentiate(b, variable), log_abs(a)),
                )
            } else {
                general_power(mk_binary(Binary::Pow, a, b.clone()), a, b, variable)
            }
        }
        Node::Ternary(condition, then, otherwise) => mk_ternary(
            condition.clone(),
            differentiate(then, variable),
            differentiate(otherwise, variable),
        ),
        Node::Binary(function @ (Binary::Min | Binary::Max), a, b) => {
            // min(a,b)' = lt0(a-b) ? a' : b'; max uses gt0.
            let comparison = if *function == Binary::Min {
                Unary::Lt0
            } else {
                Unary::Gt0
            };
            mk_ternary(
                mk_unary(
                    comparison,
                    mk_operator(Operator::Minus, a.clone(), b.clone()),
                ),
                differentiate(a, variable),
                differentiate(b, variable),
            )
        }
        Node::Binary(Binary::Pow, a, b) => {
            if let Some(exponent) = b.constant() {
                times(
                    times(
                        constant(exponent),
                        mk_binary(Binary::Pwr, a, constant(exponent - 1.)),
                    ),
                    differentiate(a, variable),
                )
            } else if a.constant().is_some() {
                times(
                    mk_binary(Binary::Pow, a, b.clone()),
                    times(differentiate(b, variable), log_abs(a)),
                )
            } else {
                general_power(mk_binary(Binary::Pow, a, b.clone()), a, b, variable)
            }
        }
        Node::Binary(Binary::Pwr, a, b) => {
            if let Some(exponent) = b.constant() {
                // b * pow(a, b - 1) * d(a)
                times(
                    times(
                        constant(exponent),
                        mk_binary(Binary::Pow, a, constant(exponent - 1.)),
                    ),
                    differentiate(a, variable),
                )
            } else {
                general_power(mk_binary(Binary::Pwr, a, b.clone()), a, b, variable)
            }
        }
        Node::Unary(function, u) => {
            let one = || constant(1.);
            let slope = match function {
                Unary::Abs => mk_unary(Unary::Sgn, u.clone()),
                Unary::Acos => over(
                    constant(-1.),
                    mk_unary(Unary::Sqrt, mk_operator(Operator::Minus, one(), squared(u))),
                ),
                Unary::Acosh => over(
                    one(),
                    mk_unary(Unary::Sqrt, mk_operator(Operator::Minus, squared(u), one())),
                ),
                Unary::Asin => over(
                    one(),
                    mk_unary(Unary::Sqrt, mk_operator(Operator::Minus, one(), squared(u))),
                ),
                Unary::Asinh => over(
                    one(),
                    mk_unary(Unary::Sqrt, mk_operator(Operator::Plus, squared(u), one())),
                ),
                Unary::Atan => over(one(), mk_operator(Operator::Plus, squared(u), one())),
                Unary::Atanh => over(one(), mk_operator(Operator::Minus, one(), squared(u))),
                Unary::Cos => mk_unary(Unary::Uminus, mk_unary(Unary::Sin, u.clone())),
                Unary::Cosh => mk_unary(Unary::Sinh, u.clone()),
                Unary::Exp => mk_unary(Unary::Exp, u.clone()),
                Unary::Log => over(one(), u.clone()),
                Unary::Log10 => over(constant(std::f64::consts::LOG10_E), u.clone()),
                Unary::Sin => mk_unary(Unary::Cos, u.clone()),
                Unary::Sinh => mk_unary(Unary::Cosh, u.clone()),
                Unary::Sqrt => over(one(), times(constant(2.), mk_unary(Unary::Sqrt, u.clone()))),
                Unary::Tan => mk_operator(
                    Operator::Plus,
                    one(),
                    squared(&mk_unary(Unary::Tan, u.clone())),
                ),
                Unary::Tanh => mk_operator(
                    Operator::Minus,
                    one(),
                    squared(&mk_unary(Unary::Tanh, u.clone())),
                ),
                Unary::Uramp => mk_unary(Unary::Ustep, u.clone()),
                Unary::Ustep2 => mk_operator(
                    Operator::Minus,
                    mk_unary(Unary::Ustep, u.clone()),
                    mk_unary(Unary::Ustep, mk_operator(Operator::Minus, u.clone(), one())),
                ),
                Unary::Uminus => constant(-1.),
                Unary::Sgn
                | Unary::Ustep
                | Unary::Eq0
                | Unary::Ne0
                | Unary::Gt0
                | Unary::Lt0
                | Unary::Ge0
                | Unary::Le0
                | Unary::Floor
                | Unary::Ceil
                | Unary::Nint => zero(),
            };
            times(slope, differentiate(u, variable))
        }
        Node::Pwl(u, values) => {
            let slope = match u.constant() {
                Some(x) => constant(pwl_slope(x, values)),
                None => Arc::new(Node::PwlDerivative(u.clone(), values.clone())),
            };
            times(slope, differentiate(u, variable))
        }
        Node::PwlDerivative(u, _) => times(zero(), differentiate(u, variable)),
        Node::Table(u, map) => {
            let slope = match u.constant() {
                Some(x) => constant(map.evaluate(x).1),
                None => Arc::new(Node::TableSlope(u.clone(), map.clone())),
            };
            times(slope, differentiate(u, variable))
        }
        Node::TableSlope(u, _) => times(zero(), differentiate(u, variable)),
    }
}

struct Evaluator<'a> {
    environment: &'a Environment<'a>,
    fudge: Real,
}

fn out_of_range(node: &Node, arguments: &[Real]) -> SpiceError {
    let arguments: Vec<String> = arguments.iter().map(ToString::to_string).collect();
    SpiceError::Numerical {
        context: "behavioural source".into(),
        message: format!(
            "{} out of range for {}",
            arguments.join(", "),
            describe(node)
        ),
    }
}

impl Evaluator<'_> {
    /// `PTeval` (`ifeval.c`): a `HUGE` function or operator result is an
    /// error, as in C; any other non-finite value is one too (C would carry
    /// on with a NaN or an infinity).
    fn eval(&self, node: &Node) -> SpiceResult<Real> {
        let value = match node {
            Node::Constant(value) => *value,
            Node::Variable(index) => self.environment.values[*index],
            Node::Time => self.environment.time,
            Node::Temperature => self.environment.temperature,
            Node::Frequency => self.environment.frequency,
            Node::Unary(function, argument) => {
                let x = self.eval(argument)?;
                let value = unary_value(*function, x);
                if value == Real::INFINITY {
                    return Err(out_of_range(node, &[x]));
                }
                value
            }
            Node::Operator(op, lhs, rhs) => {
                let a = self.eval(lhs)?;
                let b = self.eval(rhs)?;
                let value = match op {
                    Operator::Plus => a + b,
                    Operator::Minus => a - b,
                    Operator::Times => a * b,
                    Operator::Divide => divide(a, b, self.fudge),
                    Operator::Power => power_h(a, b),
                };
                if value == Real::INFINITY {
                    return Err(out_of_range(node, &[a, b]));
                }
                value
            }
            Node::Binary(function, lhs, rhs) => {
                let a = self.eval(lhs)?;
                let b = self.eval(rhs)?;
                let value = match function {
                    Binary::Pow => power(a, b),
                    Binary::Pwr => pwr(a, b),
                    Binary::Min => {
                        if a > b {
                            b
                        } else {
                            a
                        }
                    }
                    Binary::Max => {
                        if a > b {
                            a
                        } else {
                            b
                        }
                    }
                };
                if value == Real::INFINITY {
                    return Err(out_of_range(node, &[a, b]));
                }
                value
            }
            Node::Ternary(condition, then, otherwise) => {
                if self.eval(condition)? != 0. {
                    self.eval(then)?
                } else {
                    self.eval(otherwise)?
                }
            }
            Node::Pwl(argument, values) => pwl_value(self.eval(argument)?, values),
            Node::PwlDerivative(argument, values) => pwl_slope(self.eval(argument)?, values),
            Node::Table(argument, map) => map.evaluate(self.eval(argument)?).0,
            Node::TableSlope(argument, map) => map.evaluate(self.eval(argument)?).1,
        };
        if !value.is_finite() {
            return Err(SpiceError::Numerical {
                context: "behavioural source".into(),
                message: format!("non-finite value {value} in {}", describe(node)),
            });
        }
        Ok(value)
    }
}

fn describe(node: &Node) -> String {
    match node {
        Node::Constant(_) => "a constant".into(),
        Node::Variable(_) => "a circuit quantity".into(),
        Node::Time => "time".into(),
        Node::Temperature => "temper".into(),
        Node::Frequency => "hertz".into(),
        Node::Unary(function, _) => format!("{}()", function.name()),
        Node::Operator(op, _, _) => format!(
            "'{}'",
            match op {
                Operator::Plus => "+",
                Operator::Minus => "-",
                Operator::Times => "*",
                Operator::Divide => "/",
                Operator::Power => "^",
            }
        ),
        Node::Binary(function, _, _) => format!(
            "{}()",
            match function {
                Binary::Pow => "pow",
                Binary::Pwr => "pwr",
                Binary::Min => "min",
                Binary::Max => "max",
            }
        ),
        Node::Ternary(..) => "'?:'".into(),
        Node::Pwl(..) => "pwl()".into(),
        Node::PwlDerivative(..) => "pwl_derivative()".into(),
        Node::Table(..) | Node::TableSlope(..) => "a TABLE".into(),
    }
}

#[cfg(test)]
pub(super) fn power_h_for_tests(a: Real, b: Real) -> Real {
    power_h(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulps_comparison_matches_dawson() {
        assert!(almost_equal_ulps(2.0, 2.0, 10));
        assert!(almost_equal_ulps(2.0, 2.0 + 4. * f64::EPSILON, 10));
        assert!(!almost_equal_ulps(2.0, 2.0001, 10));
        assert!(almost_equal_ulps(-0.0, 0.0, 10));
    }

    #[test]
    fn power_follows_ptpowerh() {
        assert_eq!(power_h(-2., 2.), 4.);
        assert_eq!(power_h(-2., 2.5), 0.);
        assert_eq!(power_h(0., 0.), 1.);
        assert_eq!(power(0., 0.), 0.);
        assert_eq!(pwr(-2., 2.), -4.);
        assert_eq!(power_h_for_tests(4., 0.5), 2.);
    }

    #[test]
    fn pwl_extrapolates_and_searches_like_c() {
        let ascending = [0., 0., 1., 1., 3., 5.];
        assert_eq!(pwl_value(2., &ascending), 3.);
        assert_eq!(pwl_value(-1., &ascending), -1.);
        assert_eq!(pwl_value(4., &ascending), 7.);
        let descending = [3., 5., 1., 1., 0., 0.];
        assert_eq!(pwl_value(2., &descending), 3.);
        assert_eq!(pwl_slope(2., &ascending), 2.);
    }

    #[test]
    fn modulus_reduces_like_c() {
        assert_eq!(modulus(7., TWO_PI), 7. - TWO_PI);
        assert_eq!(modulus(-7., TWO_PI), -7. + TWO_PI);
        assert_eq!(modulus(1., TWO_PI), 1.);
    }
}
