//! Compiled behavioural expressions: value and analytic derivatives.
//!
//! C references (behaviour only): `src/spicelib/parser/inpptree.c`
//! (`PT_mkfnode`, `PTdifferentiate`, `prepare_PTF_PWL`), `ptfuncs.c` (the
//! function values and domain rules) and `ifeval.c` (`PTeval`). ngspice
//! differentiates the parse tree symbolically once and evaluates the
//! derivative trees with the same C functions; the port evaluates value and
//! gradient together (forward mode), applying **C's derivative rule for every
//! node** so both agree, including the deliberate non-derivatives (`floor`,
//! comparisons, `u` and `pwl_derivative` are zero or piecewise) and the
//! domain quirks of the value functions:
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

use spice_core::{Real, SpiceError, SpiceResult};
use spice_netlist::bexpr::{BBinaryOp, BExpr, BExprKind, BUnaryOp};

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

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Constant(Real),
    Variable(usize),
    Time,
    Temperature,
    Frequency,
    Unary(Unary, Box<Node>),
    Operator(Operator, Box<Node>, Box<Node>),
    Binary(Binary, Box<Node>, Box<Node>),
    Ternary(Box<Node>, Box<Node>, Box<Node>),
    /// `pwl(x, x0, y0, x1, y1, ...)`: flattened literal points.
    Pwl(Box<Node>, Vec<Real>),
    /// The XSPICE `pwl` transfer of a lowered TABLE.
    Table(Box<Node>, XspicePwl),
}

impl Node {
    const fn is_constant(&self) -> bool {
        matches!(self, Self::Constant(_))
    }
}

/// A compiled behavioural expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    root: Node,
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
    /// Compiles a front-end-resolved expression tree.
    ///
    /// # Errors
    /// Names the front end should have resolved (`.param` values, `.func`
    /// calls), unknown functions or wrong argument counts (positioned parse
    /// errors), non-literal or non-monotonic `pwl()` points (C:
    /// `prepare_PTF_PWL`), and [`SpiceError::NotYetPorted`] for `ddt()` and
    /// `gauss()`.
    pub fn compile(expr: &BExpr) -> SpiceResult<Self> {
        let mut quantities = Vec::new();
        let root = compile(expr, &mut quantities)?;
        Ok(Self { root, quantities })
    }

    /// True when the expression reads `hertz`.
    #[must_use]
    pub fn uses_frequency(&self) -> bool {
        fn walk(node: &Node) -> bool {
            match node {
                Node::Frequency => true,
                Node::Constant(_) | Node::Variable(_) | Node::Time | Node::Temperature => false,
                Node::Unary(_, a) | Node::Pwl(a, _) | Node::Table(a, _) => walk(a),
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

    /// The value and every partial derivative at `environment`.
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
            size: self.quantities.len(),
        };
        let (value, gradient) = evaluator.eval(&self.root)?;
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

fn boxed(expr: &BExpr, quantities: &mut Vec<Quantity>) -> SpiceResult<Box<Node>> {
    compile(expr, quantities).map(Box::new)
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
                    Node::Operator(Operator::Minus, Box::new(plus), Box::new(minus))
                }
            }
        }
        BExprKind::Current(name) => variable(quantities, Quantity::Branch(name.clone())),
        BExprKind::Unary { op, operand } => {
            let operand = boxed(operand, quantities)?;
            match op {
                BUnaryOp::Plus => *operand,
                BUnaryOp::Minus => Node::Unary(Unary::Uminus, operand),
                BUnaryOp::Not => Node::Unary(Unary::Eq0, operand),
            }
        }
        BExprKind::Binary { op, lhs, rhs } => {
            let lhs = boxed(lhs, quantities)?;
            let rhs = boxed(rhs, quantities)?;
            let difference = |lhs, rhs| Box::new(Node::Operator(Operator::Minus, lhs, rhs));
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
                    Box::new(Node::Operator(
                        Operator::Plus,
                        Box::new(Node::Unary(Unary::Eq0, lhs)),
                        Box::new(Node::Unary(Unary::Eq0, rhs)),
                    )),
                ),
                BBinaryOp::Or => Node::Unary(
                    Unary::Ne0,
                    Box::new(Node::Operator(
                        Operator::Plus,
                        Box::new(Node::Unary(Unary::Ne0, lhs)),
                        Box::new(Node::Unary(Unary::Ne0, rhs)),
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
        "gauss" => Err(SpiceError::not_yet_ported(
            format!(
                "{}: gauss() in a behavioural source (a random value drawn at parse time)",
                expr.span.start
            ),
            "src/spicelib/parser/inpptree.c (PT_mkfnode, gauss)",
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
        a.powf(b)
    } else if almost_equal_ulps(b.round_ties_even(), b, 10) {
        a.powf(b.round())
    } else {
        0.
    }
}

/// `PTpower` (the `pow()` function).
fn power(a: Real, b: Real) -> Real {
    if a == 0. {
        0.
    } else if a > 0. {
        a.powf(b)
    } else if almost_equal_ulps(b.round_ties_even(), b, 10) {
        a.powf(b.round())
    } else {
        0.
    }
}

/// `PTpwr` without a compatibility mode.
fn pwr(a: Real, b: Real) -> Real {
    if a < 0. { -(-a).powf(b) } else { a.powf(b) }
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

struct Evaluator<'a> {
    environment: &'a Environment<'a>,
    fudge: Real,
    size: usize,
}

type Dual = (Real, Vec<Real>);

fn domain(function: &str, argument: Real) -> SpiceError {
    SpiceError::Numerical {
        context: "behavioural source".into(),
        message: format!("{argument} out of range for {function}"),
    }
}

impl Evaluator<'_> {
    fn zero(&self) -> Vec<Real> {
        vec![0.; self.size]
    }

    /// `PTdivide`: the divisor moves `fudge` away from zero.
    fn divide(&self, a: Real, b: Real) -> Real {
        let b = if b >= 0. {
            b + self.fudge
        } else {
            b - self.fudge
        };
        a / b
    }

    fn eval(&self, node: &Node) -> SpiceResult<Dual> {
        let (value, gradient) = match node {
            Node::Constant(value) => (*value, self.zero()),
            Node::Variable(index) => {
                let mut gradient = self.zero();
                gradient[*index] = 1.;
                (self.environment.values[*index], gradient)
            }
            Node::Time => (self.environment.time, self.zero()),
            Node::Temperature => (self.environment.temperature, self.zero()),
            Node::Frequency => (self.environment.frequency, self.zero()),
            Node::Unary(function, argument) => {
                let (x, dx) = self.eval(argument)?;
                let (value, slope) = self.unary(*function, x)?;
                (value, scale(&dx, slope))
            }
            Node::Operator(op, lhs, rhs) => self.operator(*op, lhs, rhs)?,
            Node::Binary(function, lhs, rhs) => self.binary(*function, lhs, rhs)?,
            Node::Ternary(condition, then, otherwise) => {
                let (condition, _) = self.eval(condition)?;
                if condition != 0. {
                    self.eval(then)?
                } else {
                    self.eval(otherwise)?
                }
            }
            Node::Pwl(argument, values) => {
                let (x, dx) = self.eval(argument)?;
                (pwl_value(x, values), scale(&dx, pwl_slope(x, values)))
            }
            Node::Table(argument, map) => {
                let (x, dx) = self.eval(argument)?;
                let (value, slope) = map.evaluate(x);
                (value, scale(&dx, slope))
            }
        };
        if !value.is_finite() || gradient.iter().any(|d| !d.is_finite()) {
            return Err(SpiceError::Numerical {
                context: "behavioural source".into(),
                message: format!(
                    "non-finite value or derivative ({value}) in {}",
                    describe(node)
                ),
            });
        }
        Ok((value, gradient))
    }

    /// The value of a one-argument function and C's derivative rule for it.
    fn unary(&self, function: Unary, x: Real) -> SpiceResult<(Real, Real)> {
        let square = power_h(x, 2.);
        let sqrt = |argument: Real| {
            if argument < 0. {
                Err(domain("sqrt", argument))
            } else {
                Ok(argument.sqrt())
            }
        };
        let log = |argument: Real| {
            if argument < 0. {
                Err(domain("log", argument))
            } else if argument == 0. {
                Ok(-1e99)
            } else {
                Ok(argument.ln())
            }
        };
        let exp = |argument: Real| {
            if argument > EXP_LIMIT {
                1e99
            } else {
                argument.exp()
            }
        };
        let step = |argument: Real| {
            if argument < 0. {
                0.
            } else if argument > 0. {
                1.
            } else {
                0.5
            }
        };
        let sgn = |argument: Real| {
            if argument > 0. {
                1.
            } else if argument < 0. {
                -1.
            } else {
                0.
            }
        };
        let flag = |condition: bool| if condition { 1. } else { 0. };
        Ok(match function {
            Unary::Abs => (x.abs(), sgn(x)),
            Unary::Sgn => (sgn(x), 0.),
            Unary::Acos => (x.acos(), self.divide(-1., sqrt(1. - square)?)),
            Unary::Acosh => (x.acosh(), self.divide(1., sqrt(square - 1.)?)),
            Unary::Asin => (x.asin(), self.divide(1., sqrt(1. - square)?)),
            Unary::Asinh => (x.asinh(), self.divide(1., sqrt(square + 1.)?)),
            Unary::Atan => (x.atan(), self.divide(1., square + 1.)),
            Unary::Atanh => (x.atanh(), self.divide(1., 1. - square)),
            Unary::Cos => (modulus(x, TWO_PI).cos(), -modulus(x, TWO_PI).sin()),
            Unary::Cosh => (x.cosh(), x.sinh()),
            Unary::Exp => (exp(x), exp(x)),
            Unary::Log => (log(x)?, self.divide(1., x)),
            Unary::Log10 => {
                let value = if x < 0. {
                    return Err(domain("log10", x));
                } else if x == 0. {
                    -1e99
                } else {
                    x.log10()
                };
                (value, self.divide(std::f64::consts::LOG10_E, x))
            }
            Unary::Sin => (modulus(x, TWO_PI).sin(), modulus(x, TWO_PI).cos()),
            Unary::Sinh => (x.sinh(), x.cosh()),
            Unary::Sqrt => {
                let root = sqrt(x)?;
                (root, self.divide(1., 2. * root))
            }
            Unary::Tan => {
                let tangent = modulus(x, std::f64::consts::PI).tan();
                (tangent, 1. + power_h(tangent, 2.))
            }
            Unary::Tanh => {
                let tangent = x.tanh();
                (tangent, 1. - power_h(tangent, 2.))
            }
            Unary::Ustep => (step(x), 0.),
            Unary::Uramp => (if x < 0. { 0. } else { x }, step(x)),
            Unary::Ceil => (x.ceil(), 0.),
            Unary::Floor => (x.floor(), 0.),
            Unary::Nint => (x.round_ties_even(), 0.),
            Unary::Uminus => (-x, -1.),
            Unary::Ustep2 => (
                if x <= 0. {
                    0.
                } else if x <= 1. {
                    x
                } else {
                    1.
                },
                step(x) - step(x - 1.),
            ),
            Unary::Eq0 => (flag(x == 0.), 0.),
            Unary::Ne0 => (flag(x != 0.), 0.),
            Unary::Gt0 => (flag(x > 0.), 0.),
            Unary::Lt0 => (flag(x < 0.), 0.),
            Unary::Ge0 => (flag(x >= 0.), 0.),
            Unary::Le0 => (flag(x <= 0.), 0.),
        })
    }

    fn operator(&self, op: Operator, lhs: &Node, rhs: &Node) -> SpiceResult<Dual> {
        let (a, da) = self.eval(lhs)?;
        let (b, db) = self.eval(rhs)?;
        Ok(match op {
            Operator::Plus => (a + b, combine(&da, 1., &db, 1.)),
            Operator::Minus => (a - b, combine(&da, 1., &db, -1.)),
            Operator::Times => (a * b, combine(&da, b, &db, a)),
            Operator::Divide => {
                // d(a/b) = (a'b - b'a) / b^2, the quotient through PTdivide.
                let denominator = power_h(b, 2.);
                let gradient = da
                    .iter()
                    .zip(&db)
                    .map(|(da, db)| self.divide(da * b - db * a, denominator))
                    .collect();
                (self.divide(a, b), gradient)
            }
            Operator::Power => {
                let value = power_h(a, b);
                (
                    value,
                    self.power_gradient(lhs, rhs, (a, &da), (b, &db), Power::Operator)?,
                )
            }
        })
    }

    fn binary(&self, function: Binary, lhs: &Node, rhs: &Node) -> SpiceResult<Dual> {
        let (a, da) = self.eval(lhs)?;
        let (b, db) = self.eval(rhs)?;
        Ok(match function {
            Binary::Pow => (
                power(a, b),
                self.power_gradient(lhs, rhs, (a, &da), (b, &db), Power::Pow)?,
            ),
            Binary::Pwr => (
                pwr(a, b),
                self.power_gradient(lhs, rhs, (a, &da), (b, &db), Power::Pwr)?,
            ),
            // min: lt0(a-b) ? a' : b'; max: gt0(a-b) ? a' : b'.
            Binary::Min => (if a > b { b } else { a }, if a - b < 0. { da } else { db }),
            Binary::Max => (if a > b { a } else { b }, if a - b > 0. { da } else { db }),
        })
    }

    /// `PTdifferentiate` for `^`, `pow()` and `pwr()`:
    ///
    /// - constant exponent: `b * pwr(a, b-1) * a'` (`pow(a, b-1)` for `pwr`);
    /// - constant base (`^` and `pow`): `pow(a, b) * b' * log|a|`;
    /// - otherwise `f(a, b) * (b * a'/a + b' * log|a|)` with `f` the
    ///   function itself for `pwr` and `pow()` for the others.
    fn power_gradient(
        &self,
        lhs: &Node,
        rhs: &Node,
        (a, da): (Real, &[Real]),
        (b, db): (Real, &[Real]),
        kind: Power,
    ) -> SpiceResult<Vec<Real>> {
        let log_abs = || -> Real {
            let magnitude = a.abs();
            if magnitude == 0. {
                -1e99
            } else {
                magnitude.ln()
            }
        };
        if rhs.is_constant() {
            // C builds `b * pwr(a, b-1) * a'` with mkb(), which folds a zero
            // constant factor away: no 0 * inf at a = 0.
            if b == 0. {
                return Ok(vec![0.; da.len()]);
            }
            let factor = b * match kind {
                Power::Pwr => power(a, b - 1.),
                Power::Operator | Power::Pow => pwr(a, b - 1.),
            };
            return Ok(scale(da, factor));
        }
        if lhs.is_constant() && kind != Power::Pwr {
            let factor = power(a, b) * log_abs();
            return Ok(scale(db, factor));
        }
        let outer = match kind {
            Power::Pwr => pwr(a, b),
            Power::Operator | Power::Pow => power(a, b),
        };
        let log = log_abs();
        Ok(da
            .iter()
            .zip(db)
            .map(|(da, db)| outer * (b * self.divide(*da, a) + db * log))
            .collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Power {
    Operator,
    Pow,
    Pwr,
}

fn scale(gradient: &[Real], factor: Real) -> Vec<Real> {
    gradient
        .iter()
        .map(|d| if *d == 0. { 0. } else { d * factor })
        .collect()
}

fn combine(a: &[Real], fa: Real, b: &[Real], fb: Real) -> Vec<Real> {
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            let left = if *a == 0. { 0. } else { a * fa };
            let right = if *b == 0. { 0. } else { b * fb };
            left + right
        })
        .collect()
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
        Node::Table(..) => "a TABLE".into(),
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
