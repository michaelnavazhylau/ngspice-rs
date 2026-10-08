//! Front-end passes for behavioural sources (GitHub #79).
//!
//! Two jobs that ngspice's front end (`src/frontend/inpcom.c`) performs on
//! the card text before the circuit parser sees it:
//!
//! 1. [`lower_nonlinear_sources`] rewrites the nonlinear E/G/F/H forms into
//!    the primitive devices C builds (`inp_compat()`, `inp_poly_2g6_compat()`,
//!    `src/xspice/enh/enhtrans.c`):
//!
//!    | Form | Becomes |
//!    | --- | --- |
//!    | `e1 n+ n- value={f}` | `e1 n+ n- e1_int1 0 1` and `be1 e1_int1 0 v=f` |
//!    | `g1 n+ n- value={f} [m=k]` | `g1 n+ n- g1_int1 0 k` and `bg1 g1_int1 0 v=f` |
//!    | `e1 n+ n- table {f} = (x,y)...` | `e1 n+ n- e1_int1 0 1`, `be1 e1_int2 0 v=f` and the XSPICE `pwl` instance `ae1` from `e1_int2` to `e1_int1` |
//!    | `g1 ... table ... [m=k]` | the same with gain `k` (node names with `[`, `]`, `%` replaced by `_`) |
//!    | `e1 n+ n- nc+ nc- table=(...)` | `e1 n+ n- e1_int1 0 1` and `ae1` from `v(nc+,nc-)` (domain 0.001) |
//!    | one TABLE pair `(x0, y0)` | `v<name> <name>_int1 0 y0` instead of the B/A pair |
//!    | `e1 n+ n- poly(n) ...` (also F/G/H and the implicit `POLY(1)`) | the XSPICE `spice2poly` instance `a$poly$e1` |
//!
//!    The generated XSPICE instances (`ae1`, `a$poly$e1`) are designator-`a`
//!    devices carrying a behavioural `v=`/`i=` expression; they are internal
//!    to this port (user `a` cards remain unported). The rewrite runs before
//!    subcircuit expansion, as in C, so generated names and nodes are renamed
//!    like any other (`x1.e1_int1`, `b.x1.be1`).
//!
//! 2. [`resolve_expression`] performs what numparam and macro expansion do to
//!    a B card: `.param` names become values, `.func` calls are expanded
//!    (with parenthesised arguments, `inp_expand_macro_in_str()`), numeric
//!    literals are rounded to the 11 significant digits `inp_modify_exp()`
//!    prints and numparam `{...}` values in `=pwl(` lines are evaluated.

use std::sync::Arc;

use spice_core::{Real, SourceLoc, SpiceError, SpiceResult};

use crate::ast::{DeviceInstance, Netlist, ParameterAssignment, ParameterKind, Subcircuit};
use crate::bexpr::{
    BBinaryOp, BExpr, BExprKind, BUnaryOp, BehaviouralExpression, SPECIAL_NAMES, TableTransfer,
    is_builtin_function, round_like_c_literal,
};
use crate::elaborate::format_literal;
use crate::eval::{EvalBudget, FunctionScope, ParamScope};
use crate::expr::{BinaryOp, Expr, ExprKind, SourceSpan, UnaryOp};

/// C references of the front-end rewrites.
pub const C_REFERENCE: &str = "src/frontend/inpcom.c (inp_compat, inp_poly_2g6_compat, \
     inp_bsource_compat, inp_modify_exp); src/xspice/enh/enhtrans.c";

/// The XSPICE `pwl` `input_domain` `inp_compat()` uses for a TABLE.
pub const TABLE_DOMAIN: Real = 0.1;
/// The `input_domain` of the four-node LTspice TABLE form.
pub const LTSPICE_TABLE_DOMAIN: Real = 0.001;

/// Largest number of nodes a resolved expression may have (a bound on
/// nested `.func` expansion).
const MAX_RESOLVED_NODES: usize = 100_000;

/// Rewrites the nonlinear E/G/F/H forms of the root and every subcircuit
/// body into primitive devices (see the [module documentation](self)).
/// Everything else is copied unchanged; the input is never modified.
///
/// # Errors
/// A malformed nonlinear form (no expression, an odd TABLE point list, a
/// POLY without its controls), positioned at the card.
pub fn lower_nonlinear_sources(netlist: &Netlist) -> SpiceResult<Netlist> {
    let mut out = netlist.clone();
    out.devices = lower_devices(&netlist.devices)?;
    for subcircuit in &mut out.subcircuits {
        lower_subcircuit(subcircuit)?;
    }
    Ok(out)
}

fn lower_subcircuit(subcircuit: &mut Subcircuit) -> SpiceResult<()> {
    subcircuit.devices = lower_devices(&subcircuit.devices)?;
    for nested in &mut subcircuit.subcircuits {
        lower_subcircuit(nested)?;
    }
    Ok(())
}

fn lower_devices(devices: &[DeviceInstance]) -> SpiceResult<Vec<DeviceInstance>> {
    let mut out = Vec::with_capacity(devices.len());
    for device in devices {
        out.extend(lower_device(device)?);
    }
    Ok(out)
}

fn has(device: &DeviceInstance, name: &str) -> bool {
    device.parameters.iter().any(|p| p.name == name)
}

fn lower_device(device: &DeviceInstance) -> SpiceResult<Vec<DeviceInstance>> {
    if !matches!(device.designator, 'e' | 'f' | 'g' | 'h') {
        return Ok(vec![device.clone()]);
    }
    if has(device, "poly") {
        return lower_poly(device).map(|device| vec![device]);
    }
    if has(device, "value") {
        return lower_value(device);
    }
    if has(device, "table") {
        return lower_table(device);
    }
    Ok(vec![device.clone()])
}

fn scalar(name: &str, value: &str, location: &SourceLoc) -> ParameterAssignment {
    ParameterAssignment {
        name: name.to_owned(),
        value: value.to_owned(),
        kind: ParameterKind::Scalar,
        location: location.clone(),
    }
}

fn instance(
    name: String,
    designator: char,
    nodes: Vec<String>,
    parameters: Vec<ParameterAssignment>,
    location: &SourceLoc,
) -> DeviceInstance {
    DeviceInstance {
        name,
        designator,
        nodes,
        model: None,
        parameters,
        location: location.clone(),
    }
}

fn malformed(device: &DeviceInstance, message: &str) -> SpiceError {
    SpiceError::parse(
        device.location.clone(),
        format!("{}: {message}", device.name),
    )
}

/// The controlled source driven by the generated internal node: gain 1 for
/// E, the `m=` value (default 1) for G (`inp_compat`, "find multiplier m").
fn driven_source(device: &DeviceInstance, internal: &str) -> DeviceInstance {
    let gain = device
        .parameters
        .iter()
        .rev()
        .find(|p| p.name == "m")
        .filter(|_| device.designator == 'g')
        .map_or_else(
            || scalar("gain", "1", &device.location),
            |m| ParameterAssignment {
                name: "gain".to_owned(),
                ..m.clone()
            },
        );
    instance(
        device.name.clone(),
        device.designator,
        vec![
            device.nodes[0].clone(),
            device.nodes[1].clone(),
            internal.to_owned(),
            "0".to_owned(),
        ],
        vec![gain],
        &device.location,
    )
}

fn lower_value(device: &DeviceInstance) -> SpiceResult<Vec<DeviceInstance>> {
    if device.nodes.len() != 2 {
        return Err(malformed(device, "VALUE form needs two output nodes"));
    }
    let internal = format!("{}_int1", device.name);
    let mut b_parameters = Vec::new();
    for parameter in &device.parameters {
        match parameter.name.as_str() {
            "value" => b_parameters.push(ParameterAssignment {
                name: "v".to_owned(),
                ..parameter.clone()
            }),
            // G: m is the controlled source's gain, not a B setter.
            "m" if device.designator == 'g' => {}
            _ => b_parameters.push(parameter.clone()),
        }
    }
    Ok(vec![
        driven_source(device, &internal),
        instance(
            format!("b{}", device.name),
            'b',
            vec![internal, "0".to_owned()],
            b_parameters,
            &device.location,
        ),
    ])
}

/// `inp_compat()` replaces `[`, `]` and `%` by `_` in the G TABLE names it
/// uses for nodes and the XSPICE instance.
fn sanitized(name: &str, designator: char) -> String {
    if designator == 'g' {
        name.replace(['[', ']', '%'], "_")
    } else {
        name.to_owned()
    }
}

fn point(parameter: &ParameterAssignment) -> SpiceResult<BExpr> {
    let span = SourceSpan {
        start: parameter.location.clone(),
        end: parameter.location.clone(),
    };
    let kind = match &parameter.kind {
        ParameterKind::Scalar => BExprKind::Number {
            value: spice_core::parse_spice_number(&parameter.value)
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    SpiceError::parse(
                        parameter.location.clone(),
                        format!("'{}' is not a finite number", parameter.value),
                    )
                })?,
            spelling: parameter.value.clone(),
        },
        ParameterKind::Expression(expression) => BExprKind::Value(expression.clone()),
        _ => {
            return Err(SpiceError::parse(
                parameter.location.clone(),
                format!("'{}' is not a numeric value", parameter.value),
            ));
        }
    };
    Ok(BExpr { kind, span })
}

fn generated(root: BExpr, text: String, location: &SourceLoc) -> ParameterKind {
    ParameterKind::Behavioural(Box::new(BehaviouralExpression {
        text,
        span: SourceSpan {
            start: location.clone(),
            end: location.clone(),
        },
        verbatim: true,
        root,
    }))
}

fn quantity(kind: BExprKind, location: &SourceLoc) -> BExpr {
    BExpr {
        kind,
        span: SourceSpan {
            start: location.clone(),
            end: location.clone(),
        },
    }
}

fn lower_table(device: &DeviceInstance) -> SpiceResult<Vec<DeviceInstance>> {
    let four_node = device.nodes.len() == 4;
    if !matches!(device.designator, 'e' | 'g') || !(device.nodes.len() == 2 || four_node) {
        return Err(malformed(
            device,
            "TABLE form needs an E/G with 2 or 4 nodes",
        ));
    }
    let stok = sanitized(&device.name, device.designator);
    let int1 = format!("{stok}_int1");
    let int2 = format!("{stok}_int2");
    let mut points = Vec::new();
    let mut pending: Option<BExpr> = None;
    let mut input = None;
    for parameter in &device.parameters {
        match parameter.name.as_str() {
            "table" => input = Some(parameter),
            "x" => pending = Some(point(parameter)?),
            "y" => {
                let x = pending
                    .take()
                    .ok_or_else(|| malformed(device, "TABLE y value without x"))?;
                points.push((x, point(parameter)?));
            }
            "m" if device.designator == 'g' => {}
            other => {
                return Err(malformed(
                    device,
                    &format!("unexpected TABLE parameter '{other}'"),
                ));
            }
        }
    }
    let input = input.ok_or_else(|| malformed(device, "TABLE without an input"))?;
    if points.is_empty() || pending.is_some() {
        return Err(malformed(device, "TABLE needs (x, y) pairs"));
    }
    let mut out = vec![driven_source(device, &int1)];
    if points.len() == 1 {
        // "A single pair (x0, y0) will return a constant voltage y0".
        let y = device
            .parameters
            .iter()
            .find(|p| p.name == "y")
            .expect("one point");
        out.push(instance(
            format!("v{}", device.name),
            'v',
            vec![int1, "0".to_owned()],
            vec![ParameterAssignment {
                name: "dc".to_owned(),
                ..y.clone()
            }],
            &device.location,
        ));
        return Ok(out);
    }
    let location = &input.location;
    let (source, domain) = if four_node {
        (
            BExprKind::Voltage {
                positive: device.nodes[2].clone(),
                negative: Some(device.nodes[3].clone()),
            },
            LTSPICE_TABLE_DOMAIN,
        )
    } else {
        let ParameterKind::Behavioural(expression) = &input.kind else {
            return Err(malformed(device, "TABLE input is not an expression"));
        };
        out.push(instance(
            format!("b{}", device.name),
            'b',
            vec![int2.clone(), "0".to_owned()],
            vec![ParameterAssignment {
                name: "v".to_owned(),
                value: expression.text.clone(),
                kind: input.kind.clone(),
                location: input.location.clone(),
            }],
            &device.location,
        ));
        (
            BExprKind::Voltage {
                positive: int2,
                negative: None,
            },
            TABLE_DOMAIN,
        )
    };
    let text = format!(
        "xspice-pwl input_domain={domain} fraction=TRUE limit=TRUE ({} points)",
        points.len()
    );
    let root = quantity(
        BExprKind::Table(Box::new(TableTransfer {
            input: quantity(source, location),
            points,
            domain,
        })),
        location,
    );
    out.push(instance(
        format!("a{stok}"),
        'a',
        vec![int1, "0".to_owned()],
        vec![ParameterAssignment {
            name: "v".to_owned(),
            value: text.clone(),
            kind: generated(root, text, location),
            location: location.clone(),
        }],
        &device.location,
    ));
    Ok(out)
}

/// Exponent vectors of the SPICE2 polynomial terms in coefficient order
/// (after the constant): total degree ascending, then lexicographically
/// descending, which is the sequence `nxtpwr()` generates (`a`, `b`, `a²`,
/// `ab`, `b²`, `a³`, ...). Returns `count` vectors of `dimension` entries.
#[must_use]
pub fn poly_exponents(dimension: usize, count: usize) -> Vec<Vec<u32>> {
    let mut out = Vec::with_capacity(count);
    let mut degree = 1u32;
    while out.len() < count {
        let mut current = vec![0u32; dimension];
        compositions(&mut current, 0, degree, &mut out, count);
        degree += 1;
    }
    out
}

/// All exponent vectors of total `remaining` from `index` on, the largest
/// leading exponent first.
fn compositions(
    current: &mut [u32],
    index: usize,
    remaining: u32,
    out: &mut Vec<Vec<u32>>,
    count: usize,
) {
    if out.len() >= count {
        return;
    }
    if index + 1 == current.len() {
        current[index] = remaining;
        out.push(current.to_vec());
        current[index] = 0;
        return;
    }
    for value in (0..=remaining).rev() {
        current[index] = value;
        compositions(current, index + 1, remaining - value, out, count);
        if out.len() >= count {
            break;
        }
    }
    current[index] = 0;
}

fn lower_poly(device: &DeviceInstance) -> SpiceResult<DeviceInstance> {
    let voltage_controlled = matches!(device.designator, 'e' | 'g');
    let dimension = device
        .parameters
        .iter()
        .find(|p| p.name == "poly")
        .and_then(|p| p.value.parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .ok_or_else(|| malformed(device, "POLY needs a positive integer dimension"))?;
    let location = &device.location;
    let mut controls = Vec::new();
    if voltage_controlled {
        if device.nodes.len() != 2 + 2 * dimension {
            return Err(malformed(
                device,
                &format!(
                    "POLY({dimension}) needs {} controlling nodes",
                    2 * dimension
                ),
            ));
        }
        for pair in device.nodes[2..].chunks(2) {
            controls.push(BExprKind::Voltage {
                positive: pair[0].clone(),
                negative: Some(pair[1].clone()),
            });
        }
    } else {
        for parameter in device.parameters.iter().filter(|p| p.name == "control") {
            controls.push(BExprKind::Current(parameter.value.clone()));
        }
        if controls.len() != dimension || device.nodes.len() != 2 {
            return Err(malformed(
                device,
                &format!("POLY({dimension}) needs {dimension} controlling source(s)"),
            ));
        }
    }
    let coefficients: Vec<&ParameterAssignment> = device
        .parameters
        .iter()
        .filter(|p| p.name == "coef")
        .collect();
    let Some((first, rest)) = coefficients.split_first() else {
        return Err(malformed(device, "POLY needs at least one coefficient"));
    };
    let node = |kind: BExprKind| quantity(kind, location);
    let binary = |op, lhs: BExpr, rhs: BExpr| {
        node(BExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    };
    let mut sum = point(first)?;
    for (exponents, coefficient) in poly_exponents(dimension, rest.len()).iter().zip(rest) {
        let mut product: Option<BExpr> = None;
        for (control, power) in controls.iter().zip(exponents) {
            for _ in 0..*power {
                let factor = node(control.clone());
                product = Some(match product {
                    None => factor,
                    Some(lhs) => binary(BBinaryOp::Mul, lhs, factor),
                });
            }
        }
        let term = binary(
            BBinaryOp::Mul,
            point(coefficient)?,
            product.expect("degree >= 1"),
        );
        sum = binary(BBinaryOp::Add, sum, term);
    }
    let output = if matches!(device.designator, 'e' | 'h') {
        "v"
    } else {
        "i"
    };
    let text = format!(
        "spice2poly({dimension}) with {} coefficient(s)",
        coefficients.len()
    );
    let mut parameters = vec![ParameterAssignment {
        name: output.to_owned(),
        value: text.clone(),
        kind: generated(sum, text, location),
        location: location.clone(),
    }];
    parameters.extend(device.parameters.iter().filter(|p| p.name == "m").cloned());
    Ok(instance(
        format!("a$poly${}", device.name),
        'a',
        device.nodes[..2].to_vec(),
        parameters,
        location,
    ))
}

/// Resolves a behavioural expression against a parameter scope: `.param`
/// names become numbers, `.func` calls are expanded, numparam `{...}` values
/// are evaluated and (outside verbatim text) literals are rounded like C's
/// `inp_modify_exp()`. Circuit quantities, `time`, `temper`, `hertz`, `pi`
/// and `e` stay symbolic.
///
/// # Errors
/// An undefined parameter, an unknown function (C: "no such function"), a
/// `.func` called with the wrong number of arguments, a bare name in a
/// verbatim (`=pwl(`) expression, a failing numparam value, or an expansion
/// larger than the node limit.
pub fn resolve_expression(
    expression: &BehaviouralExpression,
    scope: &ParamScope,
    budget: &mut EvalBudget,
) -> SpiceResult<BehaviouralExpression> {
    let mut resolver = Resolver {
        scope,
        functions: scope.functions().cloned(),
        budget,
        verbatim: expression.verbatim,
        nodes: 0,
    };
    let root = resolver.resolve(&expression.root, 0)?;
    Ok(BehaviouralExpression {
        text: expression.text.clone(),
        span: expression.span.clone(),
        verbatim: expression.verbatim,
        root,
    })
}

struct Resolver<'a> {
    scope: &'a ParamScope,
    functions: Option<Arc<FunctionScope>>,
    budget: &'a mut EvalBudget,
    verbatim: bool,
    nodes: usize,
}

fn parse_error(span: &SourceSpan, message: String) -> SpiceError {
    SpiceError::parse(span.start.clone(), message)
}

impl Resolver<'_> {
    fn resolve(&mut self, expr: &BExpr, depth: usize) -> SpiceResult<BExpr> {
        self.nodes += 1;
        if self.nodes > MAX_RESOLVED_NODES || depth > 4 * crate::expr::MAX_NESTING {
            return Err(parse_error(
                &expr.span,
                format!(
                    "behavioural expression too large after .func expansion \
                     (more than {MAX_RESOLVED_NODES} nodes or nesting deeper than {})",
                    4 * crate::expr::MAX_NESTING
                ),
            ));
        }
        let boxed = |this: &mut Self, expr: &BExpr| this.resolve(expr, depth + 1).map(Box::new);
        let kind = match &expr.kind {
            BExprKind::Number { value, spelling } => BExprKind::Number {
                value: if self.verbatim {
                    *value
                } else {
                    round_like_c_literal(*value)
                },
                spelling: spelling.clone(),
            },
            BExprKind::Name(name) => return self.name(expr, name),
            BExprKind::Voltage { .. } | BExprKind::Current(_) => expr.kind.clone(),
            BExprKind::Unary { op, operand } => BExprKind::Unary {
                op: *op,
                operand: boxed(self, operand)?,
            },
            BExprKind::Binary { op, lhs, rhs } => BExprKind::Binary {
                op: *op,
                lhs: boxed(self, lhs)?,
                rhs: boxed(self, rhs)?,
            },
            BExprKind::Ternary {
                condition,
                then,
                otherwise,
            } => BExprKind::Ternary {
                condition: boxed(self, condition)?,
                then: boxed(self, then)?,
                otherwise: boxed(self, otherwise)?,
            },
            BExprKind::Group(inner) => BExprKind::Group(boxed(self, inner)?),
            BExprKind::Call { name, arguments } => {
                return self.call(expr, name, arguments, depth);
            }
            BExprKind::Value(value) => {
                let number = self.scope.evaluate(value, self.budget)?;
                BExprKind::Number {
                    value: number,
                    spelling: format_literal(number),
                }
            }
            BExprKind::Table(table) => {
                let input = self.resolve(&table.input, depth + 1)?;
                let mut points = Vec::with_capacity(table.points.len());
                for (x, y) in &table.points {
                    points.push((self.resolve(x, depth + 1)?, self.resolve(y, depth + 1)?));
                }
                BExprKind::Table(Box::new(TableTransfer {
                    input,
                    points,
                    domain: table.domain,
                }))
            }
        };
        Ok(BExpr {
            kind,
            span: expr.span.clone(),
        })
    }

    fn name(&mut self, expr: &BExpr, name: &str) -> SpiceResult<BExpr> {
        if SPECIAL_NAMES.contains(&name) {
            return Ok(expr.clone());
        }
        if self.verbatim {
            return Err(parse_error(
                &expr.span,
                format!(
                    "bare name '{name}' in a verbatim (=pwl) behavioural expression: C does not \
                     substitute it (the B-source parser fails); write {{{name}}}"
                ),
            ));
        }
        let value = self.scope.get(name).ok_or_else(|| {
            parse_error(
                &expr.span,
                format!("undefined parameter [{name}] in a behavioural expression"),
            )
        })?;
        Ok(BExpr {
            kind: BExprKind::Number {
                value,
                spelling: format_literal(value),
            },
            span: expr.span.clone(),
        })
    }

    fn call(
        &mut self,
        expr: &BExpr,
        name: &str,
        arguments: &[BExpr],
        depth: usize,
    ) -> SpiceResult<BExpr> {
        let definition = self
            .functions
            .as_deref()
            .and_then(|scope| scope.get(name))
            .map(|(definition, _)| definition.clone());
        if let Some(definition) = definition {
            if definition.parameters.len() != arguments.len() {
                return Err(parse_error(
                    &expr.span,
                    format!(
                        "function '{name}' (defined at {}) takes {} argument(s), found {}",
                        definition.location,
                        definition.parameters.len(),
                        arguments.len()
                    ),
                ));
            }
            // inp_expand_macro_in_str(): the body replaces the call, each
            // formal by its parenthesised argument text.
            let formals: Vec<(String, BExpr)> = definition
                .parameters
                .iter()
                .cloned()
                .zip(arguments.iter().map(|argument| BExpr {
                    kind: BExprKind::Group(Box::new(argument.clone())),
                    span: argument.span.clone(),
                }))
                .collect();
            let body = from_numparam(&definition.body.root, &formals)?;
            let expanded = self.resolve(&body, depth + 1)?;
            return Ok(BExpr {
                kind: BExprKind::Group(Box::new(expanded)),
                span: expr.span.clone(),
            });
        }
        if !is_builtin_function(name) {
            return Err(SpiceError::parse(
                expr.span.start.clone(),
                format!(
                    "no such function '{name}' in a behavioural expression \
                     (src/spicelib/parser/inpptree.c PT_mkfnode)"
                ),
            ));
        }
        let mut resolved = Vec::with_capacity(arguments.len());
        for argument in arguments {
            resolved.push(self.resolve(argument, depth + 1)?);
        }
        Ok(BExpr {
            kind: BExprKind::Call {
                name: name.to_owned(),
                arguments: resolved,
            },
            span: expr.span.clone(),
        })
    }
}

/// Converts a numparam `.func` body into a behavioural tree, binding formals.
/// `v(name[, name])`/`i(name)` calls in a body name nodes/sources literally:
/// C does not substitute formals inside them (verified with the C binary).
fn from_numparam(expr: &Expr, formals: &[(String, BExpr)]) -> SpiceResult<BExpr> {
    let convert = |expr: &Expr| from_numparam(expr, formals).map(Box::new);
    let kind = match &expr.kind {
        ExprKind::Number { value, spelling } => BExprKind::Number {
            value: *value,
            spelling: spelling.clone(),
        },
        ExprKind::Identifier(name) => {
            if let Some((_, argument)) = formals.iter().find(|(formal, _)| formal == name) {
                return Ok(argument.clone());
            }
            BExprKind::Name(name.clone())
        }
        ExprKind::Unary { op, operand } => BExprKind::Unary {
            op: match op {
                UnaryOp::Plus => BUnaryOp::Plus,
                UnaryOp::Minus => BUnaryOp::Minus,
            },
            operand: convert(operand)?,
        },
        ExprKind::Binary { op, lhs, rhs } => BExprKind::Binary {
            op: match op {
                BinaryOp::Add => BBinaryOp::Add,
                BinaryOp::Sub => BBinaryOp::Sub,
                BinaryOp::Mul => BBinaryOp::Mul,
                BinaryOp::Div => BBinaryOp::Div,
                BinaryOp::Pow => BBinaryOp::Pow,
            },
            lhs: convert(lhs)?,
            rhs: convert(rhs)?,
        },
        ExprKind::Call {
            function,
            arguments,
        } => BExprKind::Call {
            name: function.name().to_owned(),
            arguments: arguments
                .iter()
                .map(|argument| from_numparam(argument, formals))
                .collect::<SpiceResult<_>>()?,
        },
        ExprKind::Group(inner) => BExprKind::Group(convert(inner)?),
        ExprKind::UserCall { name, arguments } => {
            if let Some(kind) = circuit_quantity(name, arguments) {
                kind
            } else {
                BExprKind::Call {
                    name: name.clone(),
                    arguments: arguments
                        .iter()
                        .map(|argument| from_numparam(argument, formals))
                        .collect::<SpiceResult<_>>()?,
                }
            }
        }
    };
    Ok(BExpr {
        kind,
        span: expr.span.clone(),
    })
}

fn literal_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name.clone()),
        ExprKind::Number { spelling, .. } => Some(spelling.to_ascii_lowercase()),
        _ => None,
    }
}

fn circuit_quantity(name: &str, arguments: &[Expr]) -> Option<BExprKind> {
    match (name, arguments) {
        ("v", [positive]) => Some(BExprKind::Voltage {
            positive: literal_name(positive)?,
            negative: None,
        }),
        ("v", [positive, negative]) => Some(BExprKind::Voltage {
            positive: literal_name(positive)?,
            negative: Some(literal_name(negative)?),
        }),
        ("i", [source]) => Some(BExprKind::Current(literal_name(source)?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::poly_exponents;

    #[test]
    fn poly_terms_follow_the_spice2_order() {
        assert_eq!(
            poly_exponents(2, 5),
            vec![vec![1, 0], vec![0, 1], vec![2, 0], vec![1, 1], vec![0, 2]]
        );
        assert_eq!(
            poly_exponents(3, 9),
            vec![
                vec![1, 0, 0],
                vec![0, 1, 0],
                vec![0, 0, 1],
                vec![2, 0, 0],
                vec![1, 1, 0],
                vec![1, 0, 1],
                vec![0, 2, 0],
                vec![0, 1, 1],
                vec![0, 0, 2],
            ]
        );
        assert_eq!(poly_exponents(1, 3), vec![vec![1], vec![2], vec![3]]);
        assert_eq!(poly_exponents(2, 7)[5], vec![3, 0]);
    }
}
