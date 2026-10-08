//! Deterministic, bounded evaluation of `.param` definitions and expressions.
//!
//! This is the evaluation half of the numparam subset parsed by [`crate::expr`]
//! (GitHub #15). Syntax is documented in `docs/port/PARAM_EXPRESSIONS.md`.
//!
//! # C-backed definition rules (verified against the C binary)
//!
//! C: `inpcom.c` `inp_reorder_params()`/`inp_sort_params()`, then
//! `xpressn.c` `nupa_assignment()`/`formula()`.
//!
//! - All `.param` cards of a scope are hoisted: a device or analysis card may
//!   use a parameter defined **later** in the deck.
//! - A name defined more than once keeps only its **last** definition; earlier
//!   definitions are dropped entirely (not evaluated, never visible, even to the
//!   later definition: `.param a=1` then `.param a={a+1}` is an undefined `a`).
//!   The port keeps every definition in order ([`ParamScope::entries`]) and marks
//!   the dropped ones [`ParamState::Superseded`].
//! - Definitions are ordered by dependency level, then deck order, so forward
//!   references among `.param` cards are fine. A reference cycle is fatal in C
//!   (a "level depth greater 1000" abort); here it is an explicit error that
//!   prints the cycle.
//! - A reference to a name that is not defined is an error (`Undefined
//!   parameter`), even from an unused definition.
//! - Names are case-insensitive (lowercased by the parser).
//!
//! # Deliberate divergences
//!
//! C silently yields `inf`/`nan` for `1/0`, `sqrt(-1)`, `ln(0)` and overflow.
//! Here every operation must produce a finite value or the evaluation fails with
//! the source location of the offending sub-expression. Function semantics
//! otherwise follow `mathfunction()`/`operate()`: `^` and `**` are
//! `pow(fabs(x), y)`, `pwr(x,y)` is `pow(fabs(x), y)`, `pow(x,y)` is plain
//! `pow`, `int` truncates, `nint` rounds half to even, `sgn` is -1/0/1, `log`
//! is the natural logarithm.
//!
//! # Scopes
//!
//! [`ParamScope`] is an immutable resolved scope with an optional parent, so the
//! subcircuit pass creates a child scope seeded with [`ParamBinding`]s (formal
//! defaults and instance overrides) through [`ParamScope::resolve_instance`].
//! Nothing here flattens or substitutes into identifiers; expansion lives in
//! `spice_devices::subckt`.

use std::collections::HashMap;
use std::sync::Arc;

use petgraph::Direction;
use petgraph::algo::{tarjan_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use spice_core::{Real, SourceLoc, SpiceError, SpiceResult};

use crate::ast::ParamCard;
use crate::expr::{BinaryOp, Expr, ExprKind, Function, ParameterExpression, UnaryOp};

/// C reference used in diagnostics.
pub const C_REFERENCE: &str = "src/frontend/numparam/xpressn.c (formula, operate, mathfunction), src/frontend/inpcom.c (inp_sort_params)";

/// Work bounds for evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalLimits {
    /// Maximum number of `name=expression` definitions in one scope.
    pub max_definitions: usize,
    /// Maximum expression nodes evaluated by one [`EvalBudget`].
    pub max_nodes: usize,
    /// Maximum expression tree depth (left-associative chains nest without
    /// parentheses, so the parser's nesting bound alone is not enough).
    pub max_depth: usize,
}

impl Default for EvalLimits {
    fn default() -> Self {
        Self {
            max_definitions: 100_000,
            max_nodes: 4_000_000,
            max_depth: 1024,
        }
    }
}

/// Running node counter shared by every evaluation that should be bounded
/// together.
#[derive(Debug, Clone)]
pub struct EvalBudget {
    limits: EvalLimits,
    nodes: usize,
}

impl EvalBudget {
    /// A fresh budget.
    #[must_use]
    pub fn new(limits: EvalLimits) -> Self {
        Self { limits, nodes: 0 }
    }

    /// Expression nodes evaluated so far.
    #[must_use]
    pub fn nodes_used(&self) -> usize {
        self.nodes
    }

    /// The limits in force.
    #[must_use]
    pub fn limits(&self) -> EvalLimits {
        self.limits
    }
}

impl Default for EvalBudget {
    fn default() -> Self {
        Self::new(EvalLimits::default())
    }
}

/// A value bound into a scope before its `.param` cards are resolved (a
/// subcircuit formal default or instance override).
#[derive(Debug, Clone, PartialEq)]
pub struct ParamBinding {
    /// Lowercased name.
    pub name: String,
    /// Finite value.
    pub value: Real,
    /// Where the binding was written, for diagnostics.
    pub location: SourceLoc,
}

/// Outcome of one definition.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamState {
    /// Evaluated to a finite value.
    Resolved(Real),
    /// Dropped because a later definition of the same name exists (C keeps only
    /// the last). Not evaluated.
    Superseded {
        /// The overriding definition's name location.
        by: SourceLoc,
    },
}

/// One definition (or binding) in deck order.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamEntry {
    /// Lowercased name.
    pub name: String,
    /// Where the name was written.
    pub location: SourceLoc,
    /// The expression as written (braces re-added when braced); `None` for a
    /// [`ParamBinding`].
    pub source: Option<String>,
    /// Result.
    pub state: ParamState,
}

/// A resolved parameter scope.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamScope {
    parent: Option<Arc<ParamScope>>,
    entries: Vec<ParamEntry>,
    active: HashMap<String, usize>,
}

struct Failure {
    location: SourceLoc,
    message: String,
}

type Lookup<'a> = &'a dyn Fn(&str) -> Option<Real>;

/// What a card that redefines a bound name does. [`Bindings::Reject`] is the
/// ordinary `.param` rule; [`Bindings::Win`] is the subcircuit rule, where the
/// binding came from the instance's own override list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bindings {
    Reject,
    Win,
}

/// The original spelling of a definition (braces re-added when braced).
fn source_text(expression: &ParameterExpression) -> String {
    if expression.braced {
        format!("{{{}}}", expression.text)
    } else {
        expression.text.clone()
    }
}

impl ParamScope {
    /// Resolve top-level `.param` cards with default limits.
    ///
    /// # Errors
    /// Undefined names, cycles, domain errors, non-finite results or exceeded
    /// limits, all with source locations.
    pub fn root(cards: &[ParamCard]) -> SpiceResult<Self> {
        Self::resolve(None, &[], cards, &mut EvalBudget::default())
    }

    /// Resolve with explicit limits and a shared budget.
    ///
    /// `parent` supplies outer names; `bindings` are visible to the cards and
    /// may not be redefined by them (an explicit error; see
    /// [`Self::resolve_instance`] for the subcircuit rule, where the bound
    /// value wins instead).
    ///
    /// # Errors
    /// As [`Self::root`].
    pub fn resolve(
        parent: Option<Arc<ParamScope>>,
        bindings: &[ParamBinding],
        cards: &[ParamCard],
        budget: &mut EvalBudget,
    ) -> SpiceResult<Self> {
        Self::resolve_with(parent, bindings, cards, budget, Bindings::Reject)
    }

    /// Resolve one subcircuit body's scope, where `bindings` are that
    /// instance's parameter overrides and therefore beat every card.
    ///
    /// C: `src/frontend/numparam/spicenum.c`. Precedence runs from the instance
    /// override, through a body `.param`, down to the formal default, so a card
    /// that redefines a bound name is kept in [`Self::entries`] as
    /// [`ParamState::Superseded`] and never evaluated instead of failing as
    /// [`Self::resolve`] does. A formal default belongs in `cards`, before the
    /// body's own `.param` cards, which makes it the losing definition for a
    /// name the body also defines.
    ///
    /// # Errors
    /// As [`Self::root`].
    pub fn resolve_instance(
        parent: Option<Arc<ParamScope>>,
        bindings: &[ParamBinding],
        cards: &[ParamCard],
        budget: &mut EvalBudget,
    ) -> SpiceResult<Self> {
        Self::resolve_with(parent, bindings, cards, budget, Bindings::Win)
    }

    fn resolve_with(
        parent: Option<Arc<ParamScope>>,
        bindings: &[ParamBinding],
        cards: &[ParamCard],
        budget: &mut EvalBudget,
        policy: Bindings,
    ) -> SpiceResult<Self> {
        let mut entries: Vec<ParamEntry> = Vec::new();
        let mut exprs: Vec<Option<&ParameterExpression>> = Vec::new();
        let mut active: HashMap<String, usize> = HashMap::new();
        for binding in bindings {
            if !binding.value.is_finite() {
                return Err(SpiceError::parse(
                    binding.location.clone(),
                    format!("binding '{}' is not finite", binding.name),
                ));
            }
            if let Some(&old) = active.get(&binding.name) {
                entries[old].state = ParamState::Superseded {
                    by: binding.location.clone(),
                };
            }
            active.insert(binding.name.clone(), entries.len());
            entries.push(ParamEntry {
                name: binding.name.clone(),
                location: binding.location.clone(),
                source: None,
                state: ParamState::Resolved(binding.value),
            });
            exprs.push(None);
        }
        let binding_count = entries.len();
        let limits = budget.limits;
        for assignment in cards.iter().flat_map(|card| &card.assignments) {
            let location = assignment.name_span.start.clone();
            if entries.len() - binding_count >= limits.max_definitions {
                return Err(SpiceError::parse(
                    location,
                    format!(
                        "more than {} parameter definitions in one scope",
                        limits.max_definitions
                    ),
                ));
            }
            if let Some(&old) = active.get(&assignment.name) {
                if old < binding_count {
                    let bound = entries[old].location.clone();
                    if policy == Bindings::Reject {
                        return Err(SpiceError::parse(
                            location,
                            format!(
                                "parameter '{}' redefines a bound value (defined at {bound}); \
                                 use ParamScope::resolve_instance for subcircuit scopes",
                                assignment.name
                            ),
                        ));
                    }
                    // The instance value wins, so the card is dropped unevaluated.
                    let e = &assignment.expression;
                    entries.push(ParamEntry {
                        name: assignment.name.clone(),
                        location,
                        source: Some(source_text(e)),
                        state: ParamState::Superseded { by: bound },
                    });
                    exprs.push(None);
                    continue;
                }
                entries[old].state = ParamState::Superseded {
                    by: location.clone(),
                };
            }
            active.insert(assignment.name.clone(), entries.len());
            let e = &assignment.expression;
            entries.push(ParamEntry {
                name: assignment.name.clone(),
                location,
                source: Some(source_text(e)),
                // Placeholder until evaluated; replaced below.
                state: ParamState::Resolved(0.0),
            });
            exprs.push(Some(e));
        }

        let scope_parent = parent.as_deref();
        let is_active_def = |i: usize| {
            i >= binding_count && exprs[i].is_some() && active.get(&entries[i].name) == Some(&i)
        };
        let live: Vec<usize> = (binding_count..entries.len())
            .filter(|&i| is_active_def(i))
            .collect();

        // Dependency graph: edge a -> b means "a references b".
        let mut graph: DiGraph<usize, ()> = DiGraph::new();
        let mut node_of: HashMap<usize, NodeIndex> = HashMap::new();
        for &i in &live {
            node_of.insert(i, graph.add_node(i));
        }
        let mut undefined: Option<(usize, String, SourceLoc)> = None;
        for &i in &live {
            let Some(expr) = exprs[i] else { continue };
            let mut refs = Vec::new();
            collect_refs(&expr.root, &mut refs);
            for (name, loc) in refs {
                let known = |n: &str| {
                    active.get(n).is_some_and(|&j| j < binding_count)
                        || scope_parent.is_some_and(|p| p.get(n).is_some())
                };
                if name != entries[i].name
                    && let Some(&j) = active.get(name)
                    && j >= binding_count
                {
                    let (a, b) = (node_of[&i], node_of[&j]);
                    if !graph.contains_edge(a, b) {
                        graph.add_edge(a, b, ());
                    }
                } else if !known(name) && undefined.is_none() {
                    undefined = Some((i, name.to_owned(), loc));
                }
            }
        }
        if let Some((i, name, loc)) = undefined {
            let mut message = format!(
                "undefined parameter '{name}' (in the definition of '{}' at {})",
                entries[i].name, entries[i].location
            );
            let mut seen = vec![i];
            let mut at = i;
            while let Some(next) = graph
                .neighbors_directed(node_of[&at], Direction::Incoming)
                .map(|n| graph[n])
                .filter(|j| !seen.contains(j))
                .min()
            {
                message.push_str(&format!(
                    "\n  required by parameter '{}' defined at {}",
                    entries[next].name, entries[next].location
                ));
                seen.push(next);
                at = next;
            }
            return Err(SpiceError::parse(loc, message));
        }

        // Cycles: report the one containing the earliest definition.
        let mut cyclic: Vec<Vec<NodeIndex>> = tarjan_scc(&graph)
            .into_iter()
            .filter(|c| c.len() > 1)
            .collect();
        if !cyclic.is_empty() {
            cyclic.sort_by_key(|c| c.iter().map(|&n| graph[n]).min());
            let component = &cyclic[0];
            let start = *component
                .iter()
                .min_by_key(|&&n| graph[n])
                .expect("non-empty component");
            let path = shortest_cycle(&graph, component, start);
            let chain: Vec<String> = path
                .iter()
                .map(|&n| {
                    format!(
                        "'{}' ({})",
                        entries[graph[n]].name, entries[graph[n]].location
                    )
                })
                .collect();
            return Err(SpiceError::parse(
                entries[graph[start]].location.clone(),
                format!("circular parameter definition: {}", chain.join(" -> ")),
            ));
        }

        // Evaluate by (dependency level, deck order) like inp_sort_params.
        let order = toposort(&graph, None).expect("acyclic after SCC check");
        let mut level: HashMap<NodeIndex, usize> = HashMap::new();
        for &n in order.iter().rev() {
            let l = graph
                .neighbors_directed(n, Direction::Outgoing)
                .map(|m| level[&m] + 1)
                .max()
                .unwrap_or(0);
            level.insert(n, l);
        }
        let mut sequence: Vec<usize> = live.clone();
        sequence.sort_by_key(|&i| (level[&node_of[&i]], i));

        let mut values: Vec<Option<Real>> = (0..entries.len())
            .map(|i| match entries[i].state {
                ParamState::Resolved(v) if i < binding_count => Some(v),
                _ => None,
            })
            .collect();
        for &i in &sequence {
            let Some(expr) = exprs[i] else { continue };
            let name = entries[i].name.as_str();
            let lookup = |n: &str| -> Option<Real> {
                if n != name
                    && let Some(&j) = active.get(n)
                {
                    return values[j];
                }
                scope_parent.and_then(|p| p.get(n))
            };
            let value = evaluate_root(expr, &lookup, budget).map_err(|f| {
                SpiceError::parse(
                    f.location,
                    format!(
                        "{}\n  while evaluating parameter '{name}' defined at {}",
                        f.message, entries[i].location
                    ),
                )
            })?;
            values[i] = Some(value);
        }
        for &i in &live {
            entries[i].state = ParamState::Resolved(values[i].expect("evaluated"));
        }
        Ok(Self {
            parent,
            entries,
            active,
        })
    }

    /// Every definition and binding in order, duplicates included.
    #[must_use]
    pub fn entries(&self) -> &[ParamEntry] {
        &self.entries
    }

    /// The enclosing scope, if any.
    #[must_use]
    pub fn parent(&self) -> Option<&Arc<ParamScope>> {
        self.parent.as_ref()
    }

    /// The value of `name` (case-insensitive), searching enclosing scopes.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Real> {
        let key = name.to_ascii_lowercase();
        if let Some(&i) = self.active.get(&key)
            && let ParamState::Resolved(v) = self.entries[i].state
        {
            return Some(v);
        }
        self.parent.as_ref().and_then(|p| p.get(&key))
    }

    /// Evaluate a site expression against this scope, drawing on `budget`.
    ///
    /// # Errors
    /// Undefined names, domain errors, non-finite results, exceeded limits.
    pub fn evaluate(
        &self,
        expression: &ParameterExpression,
        budget: &mut EvalBudget,
    ) -> SpiceResult<Real> {
        evaluate_root(expression, &|n| self.get(n), budget)
            .map_err(|f| SpiceError::parse(f.location, f.message))
    }
}

fn collect_refs<'a>(expr: &'a Expr, out: &mut Vec<(&'a str, SourceLoc)>) {
    match &expr.kind {
        ExprKind::Number { .. } => {}
        ExprKind::Identifier(name) => out.push((name, expr.span.start.clone())),
        ExprKind::Unary { operand, .. } | ExprKind::Group(operand) => collect_refs(operand, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_refs(lhs, out);
            collect_refs(rhs, out);
        }
        ExprKind::Call { arguments, .. } => {
            for a in arguments {
                collect_refs(a, out);
            }
        }
    }
}

/// Shortest cycle through `start` inside `component` (BFS), as `start .. start`.
fn shortest_cycle(
    graph: &DiGraph<usize, ()>,
    component: &[NodeIndex],
    start: NodeIndex,
) -> Vec<NodeIndex> {
    let mut parent: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(n) = queue.pop_front() {
        let mut next: Vec<NodeIndex> = graph
            .neighbors_directed(n, Direction::Outgoing)
            .filter(|m| component.contains(m))
            .collect();
        next.sort_by_key(|&m| graph[m]);
        for m in next {
            if m == start {
                let mut chain = vec![n];
                let mut at = n;
                while at != start {
                    at = parent[&at];
                    chain.push(at);
                }
                chain.reverse();
                chain.push(start);
                return chain;
            }
            if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(m) {
                e.insert(n);
                queue.push_back(m);
            }
        }
    }
    vec![start, start]
}

fn evaluate_root(
    root: &ParameterExpression,
    lookup: Lookup<'_>,
    budget: &mut EvalBudget,
) -> Result<Real, Failure> {
    eval(&root.root, root, lookup, budget, 0)
}

fn snippet<'a>(root: &'a ParameterExpression, node: &Expr) -> &'a str {
    let base = root.span.start.column as usize;
    let s = (node.span.start.column as usize).saturating_sub(base);
    let e = (node.span.end.column as usize).saturating_sub(base);
    root.text.get(s..e).unwrap_or(root.text.as_str())
}

fn fail(node: &Expr, root: &ParameterExpression, message: String) -> Failure {
    Failure {
        location: node.span.start.clone(),
        message: format!("{message} in `{}`", snippet(root, node)),
    }
}

fn checked(
    value: Real,
    node: &Expr,
    root: &ParameterExpression,
    what: impl FnOnce() -> String,
) -> Result<Real, Failure> {
    if value.is_finite() {
        Ok(value)
    } else {
        let kind = if value.is_nan() {
            "domain error (result is not a number)"
        } else {
            "overflow or pole (result is infinite)"
        };
        Err(fail(node, root, format!("{}: {kind}", what())))
    }
}

fn eval(
    node: &Expr,
    root: &ParameterExpression,
    lookup: Lookup<'_>,
    budget: &mut EvalBudget,
    depth: usize,
) -> Result<Real, Failure> {
    if depth > budget.limits.max_depth {
        return Err(fail(
            node,
            root,
            format!("expression deeper than {} levels", budget.limits.max_depth),
        ));
    }
    budget.nodes += 1;
    if budget.nodes > budget.limits.max_nodes {
        return Err(fail(
            node,
            root,
            format!(
                "evaluation exceeded the budget of {} expression nodes",
                budget.limits.max_nodes
            ),
        ));
    }
    match &node.kind {
        ExprKind::Number { value, .. } => Ok(*value),
        ExprKind::Identifier(name) => lookup(name).ok_or_else(|| Failure {
            location: node.span.start.clone(),
            message: format!("undefined parameter '{name}'"),
        }),
        ExprKind::Group(inner) => eval(inner, root, lookup, budget, depth + 1),
        ExprKind::Unary { op, operand } => {
            let v = eval(operand, root, lookup, budget, depth + 1)?;
            Ok(match op {
                UnaryOp::Plus => v,
                UnaryOp::Minus => -v,
            })
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let x = eval(lhs, root, lookup, budget, depth + 1)?;
            let y = eval(rhs, root, lookup, budget, depth + 1)?;
            let result = match op {
                BinaryOp::Add => x + y,
                BinaryOp::Sub => x - y,
                BinaryOp::Mul => x * y,
                BinaryOp::Div => {
                    if y == 0.0 {
                        return Err(Failure {
                            location: rhs.span.start.clone(),
                            message: format!(
                                "division by zero ({x} / {y}) in `{}`",
                                snippet(root, node)
                            ),
                        });
                    }
                    x / y
                }
                // xpressn.c operate(), default compatibility.
                BinaryOp::Pow => x.abs().powf(y),
            };
            checked(result, node, root, || match op {
                BinaryOp::Add => format!("{x} + {y}"),
                BinaryOp::Sub => format!("{x} - {y}"),
                BinaryOp::Mul => format!("{x} * {y}"),
                BinaryOp::Div => format!("{x} / {y}"),
                BinaryOp::Pow => format!("pow(fabs({x}), {y})"),
            })
        }
        ExprKind::Call {
            function,
            arguments,
        } => {
            let mut args = Vec::with_capacity(arguments.len());
            for a in arguments {
                args.push(eval(a, root, lookup, budget, depth + 1)?);
            }
            let result = apply(*function, &args);
            checked(result, node, root, || {
                let list: Vec<String> = args.iter().map(ToString::to_string).collect();
                format!("{}({})", function.name(), list.join(", "))
            })
        }
    }
}

/// xpressn.c `mathfunction(f, z, x)`: `z` is the first argument of two.
fn apply(function: Function, args: &[Real]) -> Real {
    let x = args[args.len() - 1];
    let z = args[0];
    match function {
        Function::Sqr => x * x,
        Function::Sqrt => x.sqrt(),
        Function::Sin => x.sin(),
        Function::Cos => x.cos(),
        Function::Exp => x.exp(),
        Function::Ln | Function::Log => x.ln(),
        Function::Arctan | Function::Atan => x.atan(),
        Function::Abs => x.abs(),
        Function::Pow => z.powf(x),
        Function::Pwr => z.abs().powf(x),
        Function::Max => {
            if x > z {
                x
            } else {
                z
            }
        }
        Function::Min => {
            if x < z {
                x
            } else {
                z
            }
        }
        Function::Int => x.trunc(),
        Function::Nint => x.round_ties_even(),
        Function::Log10 => x.log10(),
        Function::Sinh => x.sinh(),
        Function::Cosh => x.cosh(),
        Function::Tanh => x.tanh(),
        Function::Sgn => {
            if x > 0.0 {
                1.0
            } else if x == 0.0 {
                0.0
            } else {
                -1.0
            }
        }
        Function::Ceil => x.ceil(),
        Function::Floor => x.floor(),
        Function::Asin => x.asin(),
        Function::Acos => x.acos(),
        Function::Asinh => x.asinh(),
        Function::Acosh => x.acosh(),
        Function::Atanh => x.atanh(),
        Function::Tan => x.tan(),
    }
}
