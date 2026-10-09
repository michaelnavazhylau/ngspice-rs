//! Deterministic, bounded evaluation of `.param` definitions and expressions.
//!
//! This is the evaluation half of the numparam subset parsed by [`crate::netlist::expr`]
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
//! `crate::devices::subckt`.
//!
//! # User functions (`.func`, GitHub #107)
//!
//! A [`FunctionScope`] holds the `.func` definitions of one lexical scope
//! (deck or `.subckt` body) and is attached to a [`ParamScope`]; calls
//! ([`ExprKind::UserCall`], or a built-in name a `.func` redefines) are
//! evaluated by value, with the body's free names resolved at the call site,
//! as C's textual expansion (`inpcom.c` `inp_expand_macro_in_str()`) implies.
//! Recursion is rejected with the cycle (petgraph SCC) instead of C's
//! unbounded expansion. [`ParamScope::for_netlist`] is the entry point for
//! evaluating anything against a deck's top-level parameters and functions.

use std::collections::HashMap;
use std::sync::Arc;

use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};
use petgraph::Direction;
use petgraph::algo::{tarjan_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};

use crate::netlist::ast::{FuncCard, Netlist, ParamCard};
use crate::netlist::expr::{
    BinaryOp, EXCLUDED_FUNCTIONS, Expr, ExprKind, Function, ParameterExpression, UnaryOp,
};

/// C reference used in diagnostics.
pub const C_REFERENCE: &str = "src/frontend/numparam/xpressn.c (formula, operate, mathfunction), src/frontend/inpcom.c (inp_sort_params, inp_expand_macros_in_deck)";

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
    functions: Option<Arc<FunctionScope>>,
}

struct Failure {
    location: SourceLoc,
    message: String,
    /// Valid numparam outside the port's subset (NotYetPorted, not Parse).
    unsupported: bool,
}

impl Failure {
    fn into_error(self) -> SpiceError {
        if self.unsupported {
            SpiceError::not_yet_ported(format!("{}: {}", self.location, self.message), C_REFERENCE)
        } else {
            SpiceError::parse(self.location, self.message)
        }
    }

    fn context(mut self, context: impl FnOnce() -> String) -> Self {
        self.message = format!("{}\n  {}", self.message, context());
        self
    }
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

    /// The top-level scope of a deck: its `.func` definitions
    /// ([`Netlist::functions`]) and `.param` cards ([`Netlist::params`]), with
    /// default limits.
    ///
    /// This is the reusable entry point for anything that evaluates an
    /// expression against the deck's top-level parameters (device and analysis
    /// sites, and other consumers such as option values):
    /// `ParamScope::for_netlist(&netlist)?.evaluate(&expression, &mut budget)`.
    ///
    /// # Errors
    /// As [`Self::root`], plus invalid `.func` definitions
    /// ([`FunctionScope::new`]).
    pub fn for_netlist(netlist: &Netlist) -> SpiceResult<Self> {
        let mut budget = EvalBudget::default();
        let functions = FunctionScope::for_netlist(netlist, &budget)?;
        Self::resolve_scoped(None, Some(functions), &[], &netlist.params, &mut budget)
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
        let functions = parent.as_ref().and_then(|p| p.functions.clone());
        Self::resolve_with(parent, functions, bindings, cards, budget, Bindings::Reject)
    }

    /// As [`Self::resolve`], with the `.func` definitions visible in this
    /// scope given explicitly (`None`: no user functions). [`Self::resolve`]
    /// inherits the parent's functions instead.
    ///
    /// # Errors
    /// As [`Self::root`].
    pub fn resolve_scoped(
        parent: Option<Arc<ParamScope>>,
        functions: Option<Arc<FunctionScope>>,
        bindings: &[ParamBinding],
        cards: &[ParamCard],
        budget: &mut EvalBudget,
    ) -> SpiceResult<Self> {
        Self::resolve_with(parent, functions, bindings, cards, budget, Bindings::Reject)
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
        let functions = parent.as_ref().and_then(|p| p.functions.clone());
        Self::resolve_with(parent, functions, bindings, cards, budget, Bindings::Win)
    }

    /// As [`Self::resolve_instance`], with the `.func` definitions visible in
    /// the body given explicitly. C's function environments follow the
    /// **lexical** `.subckt` nesting (`inpcom.c` `inp_expand_macros_in_deck()`),
    /// so a body sees its own definitions and those of the enclosing
    /// definitions, not those of the instantiating body: pass the definition's
    /// [`FunctionScope`], not the caller's.
    ///
    /// # Errors
    /// As [`Self::root`].
    pub fn resolve_instance_scoped(
        parent: Option<Arc<ParamScope>>,
        functions: Option<Arc<FunctionScope>>,
        bindings: &[ParamBinding],
        cards: &[ParamCard],
        budget: &mut EvalBudget,
    ) -> SpiceResult<Self> {
        Self::resolve_with(parent, functions, bindings, cards, budget, Bindings::Win)
    }

    fn resolve_with(
        parent: Option<Arc<ParamScope>>,
        functions: Option<Arc<FunctionScope>>,
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
        let scope_functions = functions.as_deref();
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
        let mut scan = RefScan {
            use_site: scope_functions,
            budget: &mut *budget,
            memo: HashMap::new(),
            stack: Vec::new(),
            cuts: 0,
        };
        for &i in &live {
            let Some(expr) = exprs[i] else { continue };
            let mut refs = Vec::new();
            scan.refs(&expr.root, 0, &mut refs).map_err(|f| {
                f.context(|| {
                    format!(
                        "while collecting the dependencies of parameter '{}' defined at {}",
                        entries[i].name, entries[i].location
                    )
                })
                .into_error()
            })?;
            for (name, loc) in refs {
                let known = |n: &str| {
                    active.get(n).is_some_and(|&j| j < binding_count)
                        || scope_parent.is_some_and(|p| p.get(n).is_some())
                };
                if name != entries[i].name
                    && let Some(&j) = active.get(name.as_str())
                    && j >= binding_count
                {
                    let (a, b) = (node_of[&i], node_of[&j]);
                    if !graph.contains_edge(a, b) {
                        graph.add_edge(a, b, ());
                    }
                } else if !known(&name) && undefined.is_none() {
                    undefined = Some((i, name, loc));
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
            let value = evaluate_root(expr, &lookup, scope_functions, budget).map_err(|f| {
                f.context(|| {
                    format!(
                        "while evaluating parameter '{name}' defined at {}",
                        entries[i].location
                    )
                })
                .into_error()
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
            functions,
        })
    }

    /// The `.func` definitions visible in this scope, if any.
    #[must_use]
    pub fn functions(&self) -> Option<&Arc<FunctionScope>> {
        self.functions.as_ref()
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
        evaluate_root(
            expression,
            &|n| self.get(n),
            self.functions.as_deref(),
            budget,
        )
        .map_err(Failure::into_error)
    }
}

/// One `.func` definition, as collected into a [`FunctionScope`].
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    /// Lowercased name.
    pub name: String,
    /// Lowercased formal parameters, in order.
    pub parameters: Vec<String>,
    /// The body expression.
    pub body: ParameterExpression,
    /// Where the `.func` card was written.
    pub location: SourceLoc,
}

/// The `.func` definitions of one lexical scope (the deck or one `.subckt`
/// body), linked to the enclosing scope.
///
/// C: `src/frontend/inpcom.c`, `inp_grab_func()` / `find_function()` /
/// `inp_expand_macros_in_func()`. Rules (verified against the C binary,
/// `c_func_eval.rs`):
///
/// - Every `.func` of a scope is visible throughout that scope (hoisted) and
///   in nested `.subckt` definitions, never outside it.
/// - A definition shadows one of the same name in an enclosing scope; within
///   one scope the **last** definition wins.
/// - A `.func` may redefine a built-in function (same arity only; see
///   [`crate::netlist::ast::FuncCard`]).
/// - Calls are by value: each formal is bound to its argument's value. A name
///   in the body that is not a formal is resolved where the call is written
///   (C expands the body textually there), after the formals of any
///   enclosing `.func` calls.
/// - Recursion (direct or mutual) is fatal in C even if unused (unbounded
///   expansion); here it is an error naming the cycle. A call with the wrong
///   number of arguments is an error, including inside an unused body.
/// - Free names in an unused body are not checked, as in C.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionScope {
    parent: Option<Arc<FunctionScope>>,
    definitions: Vec<FunctionDef>,
    active: HashMap<String, usize>,
}

impl FunctionScope {
    /// Collects and checks one scope's `.func` cards.
    ///
    /// # Errors
    /// More definitions than [`EvalLimits::max_definitions`], recursive
    /// definitions (a cycle among the lexically resolved calls of this
    /// scope's bodies), and calls in a body with the wrong number of
    /// arguments for the definition they resolve to.
    pub fn new(
        parent: Option<Arc<FunctionScope>>,
        cards: &[FuncCard],
        budget: &EvalBudget,
    ) -> SpiceResult<Self> {
        let limit = budget.limits.max_definitions;
        if let Some(card) = cards.get(limit) {
            return Err(SpiceError::parse(
                card.name_span.start.clone(),
                format!("more than {limit} .func definitions in one scope"),
            ));
        }
        let mut definitions = Vec::with_capacity(cards.len());
        let mut active = HashMap::new();
        for card in cards {
            active.insert(card.name.clone(), definitions.len());
            definitions.push(FunctionDef {
                name: card.name.clone(),
                parameters: card.parameters.iter().map(|p| p.name.clone()).collect(),
                body: card.body.clone(),
                location: card.name_span.start.clone(),
            });
        }
        let scope = Self {
            parent,
            definitions,
            active,
        };
        scope.check()?;
        Ok(scope)
    }

    /// The deck's top-level scope.
    ///
    /// Only the top-level `.func` cards are checked here. C expands and
    /// checks a `.subckt` body's `.func` cards only when the subcircuit is
    /// instantiated (`inpcom.c` `inp_expand_macros_in_deck()` runs on the
    /// flattened deck), so a recursive or mis-called definition inside a
    /// never-instantiated subcircuit is accepted by C. Each body's scope is
    /// built and checked with [`Self::new`] when an instance of it is
    /// expanded (`devices` subcircuit expansion).
    ///
    /// # Errors
    /// As [`Self::new`], for the top-level scope.
    pub fn for_netlist(netlist: &Netlist, budget: &EvalBudget) -> SpiceResult<Arc<Self>> {
        Ok(Arc::new(Self::new(None, &netlist.functions, budget)?))
    }

    /// Arity of every resolvable call in the active bodies, then cycles.
    fn check(&self) -> SpiceResult<()> {
        let mut live: Vec<usize> = self.active.values().copied().collect();
        live.sort_unstable();
        let mut graph: DiGraph<usize, ()> = DiGraph::new();
        let mut node_of: HashMap<usize, NodeIndex> = HashMap::new();
        for &i in &live {
            node_of.insert(i, graph.add_node(i));
        }
        for &i in &live {
            let def = &self.definitions[i];
            let mut calls = Vec::new();
            collect_calls(&def.body.root, &mut calls);
            for (name, arguments, location) in calls {
                let local = self.active.get(name).copied();
                let callee = match local {
                    Some(j) => Some(&self.definitions[j]),
                    None => self
                        .parent
                        .as_deref()
                        .and_then(|p| p.get(name))
                        .map(|(d, _)| d),
                };
                let Some(callee) = callee else { continue };
                if callee.parameters.len() != arguments {
                    return Err(SpiceError::parse(
                        location,
                        format!(
                            "function '{}' (defined at {}) takes {} argument(s), found {} \
                             (in the body of function '{}' defined at {})",
                            callee.name,
                            callee.location,
                            callee.parameters.len(),
                            arguments,
                            def.name,
                            def.location
                        ),
                    ));
                }
                if let Some(j) = local {
                    let (a, b) = (node_of[&i], node_of[&j]);
                    if !graph.contains_edge(a, b) {
                        graph.add_edge(a, b, ());
                    }
                }
            }
        }
        let mut cyclic: Vec<Vec<NodeIndex>> = tarjan_scc(&graph)
            .into_iter()
            .filter(|c| c.len() > 1 || graph.contains_edge(c[0], c[0]))
            .collect();
        if cyclic.is_empty() {
            return Ok(());
        }
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
                let def = &self.definitions[graph[n]];
                format!("'{}' ({})", def.name, def.location)
            })
            .collect();
        Err(SpiceError::parse(
            self.definitions[graph[start]].location.clone(),
            format!("recursive .func definition: {}", chain.join(" -> ")),
        ))
    }

    /// Every definition of this scope in deck order, superseded ones included.
    #[must_use]
    pub fn definitions(&self) -> &[FunctionDef] {
        &self.definitions
    }

    /// The enclosing scope, if any.
    #[must_use]
    pub fn parent(&self) -> Option<&Arc<FunctionScope>> {
        self.parent.as_ref()
    }

    /// The definition `name` (case-insensitive) resolves to, searching
    /// enclosing scopes, with the scope it was defined in.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<(&FunctionDef, &FunctionScope)> {
        let key = name.to_ascii_lowercase();
        let mut scope = self;
        loop {
            if let Some(&i) = scope.active.get(&key) {
                return Some((&scope.definitions[i], scope));
            }
            scope = scope.parent.as_deref()?;
        }
    }
}

/// `(name, argument count, location)` of every call in `expr`.
fn collect_calls<'a>(expr: &'a Expr, out: &mut Vec<(&'a str, usize, SourceLoc)>) {
    match &expr.kind {
        ExprKind::Number { .. } | ExprKind::Identifier(_) => {}
        ExprKind::Unary { operand, .. } | ExprKind::Group(operand) => collect_calls(operand, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_calls(lhs, out);
            collect_calls(rhs, out);
        }
        ExprKind::Call { arguments, .. } | ExprKind::UserCall { arguments, .. } => {
            out.push((
                call_name(&expr.kind),
                arguments.len(),
                expr.span.start.clone(),
            ));
            for a in arguments {
                collect_calls(a, out);
            }
        }
    }
}

/// The dependency pre-pass of one [`ParamScope`]: the names a `.param`
/// expression depends on, with the location to blame. Identifiers count at
/// their own location; a call of a user function visible at the site counts
/// with the names its body leaves free (at the call).
///
/// The work is bounded like evaluation: every visited `.func` body node is
/// charged to the [`EvalBudget`], the walk is cut at
/// [`EvalLimits::max_depth`] levels (expression nesting plus call nesting,
/// counted as in [`eval`]), and the free names
/// of each definition are computed once per scope (memoized), so a DAG of
/// functions costs linear rather than exponential work.
struct RefScan<'a, 'b> {
    /// The site's `.func` scope; also where a body's unresolved call names
    /// are looked up (C expands what is left at the outermost call).
    use_site: Option<&'a FunctionScope>,
    budget: &'b mut EvalBudget,
    /// Free names per definition (keyed by address; definitions live in the
    /// scope chain for the whole scan).
    memo: HashMap<*const FunctionDef, Arc<[String]>>,
    /// Definitions being expanded, to cut a cycle formed through the
    /// call-site fallback (evaluation reports it).
    stack: Vec<*const FunctionDef>,
    /// Number of cycles cut so far: a result computed while a cycle was cut
    /// may be partial and is not memoized.
    cuts: usize,
}

impl<'a> RefScan<'a, '_> {
    /// Checks the depth of `node`; inside a `.func` body (`in_body`) also
    /// charges it to the budget. Site expressions are not charged: their
    /// scan is linear and evaluation charges them anyway.
    fn charge(&mut self, node: &Expr, depth: usize, in_body: bool) -> Result<(), Failure> {
        let limits = self.budget.limits;
        if depth > limits.max_depth {
            return Err(Failure {
                location: node.span.start.clone(),
                message: format!(
                    "expression deeper than {} levels{}",
                    limits.max_depth,
                    if in_body { CALL_DEPTH_NOTE } else { "" }
                ),
                unsupported: false,
            });
        }
        if !in_body {
            return Ok(());
        }
        self.budget.nodes += 1;
        if self.budget.nodes > limits.max_nodes {
            return Err(Failure {
                location: node.span.start.clone(),
                message: format!(
                    "evaluation exceeded the budget of {} expression nodes \
                     (while collecting the names used through .func calls)",
                    limits.max_nodes
                ),
                unsupported: false,
            });
        }
        Ok(())
    }

    /// Names `expr` (a site expression) depends on.
    fn refs(
        &mut self,
        expr: &'a Expr,
        depth: usize,
        out: &mut Vec<(String, SourceLoc)>,
    ) -> Result<(), Failure> {
        self.charge(expr, depth, false)?;
        match &expr.kind {
            ExprKind::Number { .. } => {}
            ExprKind::Identifier(name) => out.push((name.clone(), expr.span.start.clone())),
            ExprKind::Unary { operand, .. } | ExprKind::Group(operand) => {
                self.refs(operand, depth + 1, out)?;
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.refs(lhs, depth + 1, out)?;
                self.refs(rhs, depth + 1, out)?;
            }
            ExprKind::Call { arguments, .. } | ExprKind::UserCall { arguments, .. } => {
                for a in arguments {
                    self.refs(a, depth + 1, out)?;
                }
                let name = call_name(&expr.kind);
                if let Some((def, scope)) = self.use_site.and_then(|f| f.get(name)) {
                    let free = self.free_names(def, scope, depth)?;
                    for name in free.iter() {
                        out.push((name.clone(), expr.span.start.clone()));
                    }
                }
            }
        }
        Ok(())
    }

    /// Names a call of `def` leaves free for the call site to supply: names
    /// in its body that are not its formals, plus (transitively) the free
    /// names of the user functions it calls that its own formals do not
    /// capture. `depth` is the call node's; the body is [`CALL_DEPTH`]
    /// levels deeper, as in [`call`].
    ///
    /// C expands `.func` bodies textually (`inp_expand_macro_in_str()`), so a
    /// callee's free name is captured by the formals of every enclosing call
    /// and otherwise resolved where the outermost call was written.
    fn free_names(
        &mut self,
        def: &'a FunctionDef,
        scope: &'a FunctionScope,
        depth: usize,
    ) -> Result<Arc<[String]>, Failure> {
        let key = std::ptr::from_ref(def);
        if let Some(free) = self.memo.get(&key) {
            return Ok(Arc::clone(free));
        }
        if self.stack.contains(&key) {
            // A dynamic cycle; evaluation reports it.
            self.cuts += 1;
            return Ok(Arc::from([]));
        }
        let cuts = self.cuts;
        self.stack.push(key);
        let mut names = Vec::new();
        let result = self.body_names(&def.body.root, scope, depth + CALL_DEPTH, &mut names);
        self.stack.pop();
        result?;
        let mut free: Vec<String> = Vec::new();
        for name in names {
            if !def.parameters.contains(&name) && !free.contains(&name) {
                free.push(name);
            }
        }
        let free: Arc<[String]> = free.into();
        if self.cuts == cuts {
            self.memo.insert(key, Arc::clone(&free));
        }
        Ok(free)
    }

    fn body_names(
        &mut self,
        expr: &'a Expr,
        scope: &'a FunctionScope,
        depth: usize,
        out: &mut Vec<String>,
    ) -> Result<(), Failure> {
        self.charge(expr, depth, true)?;
        match &expr.kind {
            ExprKind::Number { .. } => {}
            ExprKind::Identifier(name) => out.push(name.clone()),
            ExprKind::Unary { operand, .. } | ExprKind::Group(operand) => {
                self.body_names(operand, scope, depth + 1, out)?;
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.body_names(lhs, scope, depth + 1, out)?;
                self.body_names(rhs, scope, depth + 1, out)?;
            }
            ExprKind::Call { arguments, .. } | ExprKind::UserCall { arguments, .. } => {
                for a in arguments {
                    self.body_names(a, scope, depth + 1, out)?;
                }
                let name = call_name(&expr.kind);
                let callee = scope
                    .get(name)
                    .or_else(|| self.use_site.and_then(|u| u.get(name)));
                if let Some((callee, callee_scope)) = callee {
                    let free = self.free_names(callee, callee_scope, depth)?;
                    out.extend(free.iter().cloned());
                }
            }
        }
        Ok(())
    }
}

/// The lowercased callee name of a call node.
fn call_name(kind: &ExprKind) -> &str {
    match kind {
        ExprKind::Call { function, .. } => function.name(),
        ExprKind::UserCall { name, .. } => name,
        _ => "",
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

/// Depth units a `.func` call adds before its body, against
/// [`EvalLimits::max_depth`]. A call nests several evaluator frames, so it is
/// charged more than one level of expression nesting; this keeps the stack
/// used by the deepest accepted input within that of a plain expression of
/// [`EvalLimits::max_depth`] levels (a 2 MiB thread stack, unoptimized).
const CALL_DEPTH: usize = 4;

const CALL_DEPTH_NOTE: &str = " (each nested .func call counts as 4 levels)";

/// One active `.func` call: the definition, the scope it was defined in and
/// its argument values, linked to the enclosing call.
struct Frame<'a> {
    def: &'a FunctionDef,
    scope: &'a FunctionScope,
    args: Vec<Real>,
    call: SourceLoc,
    outer: Option<&'a Frame<'a>>,
}

/// What an expression is evaluated against: the site's parameter lookup and
/// `.func` scope, plus the chain of active calls.
#[derive(Clone, Copy)]
struct Env<'a> {
    lookup: Lookup<'a>,
    functions: Option<&'a FunctionScope>,
    frame: Option<&'a Frame<'a>>,
}

impl<'a> Env<'a> {
    /// A name: the formals of the active calls, innermost first (C's textual
    /// expansion lets an enclosing call's formal capture a callee's free
    /// name), then the site's parameters.
    fn value(&self, name: &str) -> Option<Real> {
        let mut frame = self.frame;
        while let Some(f) = frame {
            if let Some(i) = f.def.parameters.iter().position(|p| p == name) {
                return Some(f.args[i]);
            }
            frame = f.outer;
        }
        (self.lookup)(name)
    }

    /// A user function: looked up where the calling body was defined (or at
    /// the site, outside any call), then at the site. C expands bodies in
    /// their own environment first (`inp_expand_macros_in_func()`), and what
    /// is left is expanded where the outermost call was written.
    fn function(&self, name: &str) -> Option<(&'a FunctionDef, &'a FunctionScope)> {
        let lexical = self.frame.map(|f| f.scope).or(self.functions);
        lexical
            .and_then(|scope| scope.get(name))
            .or_else(|| self.functions.and_then(|scope| scope.get(name)))
    }
}

fn evaluate_root(
    root: &ParameterExpression,
    lookup: Lookup<'_>,
    functions: Option<&FunctionScope>,
    budget: &mut EvalBudget,
) -> Result<Real, Failure> {
    let env = Env {
        lookup,
        functions,
        frame: None,
    };
    eval(&root.root, root, env, budget, 0)
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
        unsupported: false,
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

/// The recursive evaluator. Kept to a small dispatch (every diagnostic and
/// call is built in a separate, non-inlined function) so that one level of
/// nesting costs little stack even unoptimized: [`EvalLimits::max_depth`]
/// levels must fit a 2 MiB thread stack.
fn eval(
    node: &Expr,
    root: &ParameterExpression,
    env: Env<'_>,
    budget: &mut EvalBudget,
    depth: usize,
) -> Result<Real, Failure> {
    if depth > budget.limits.max_depth {
        return Err(too_deep(node, root, env, budget.limits.max_depth));
    }
    budget.nodes += 1;
    if budget.nodes > budget.limits.max_nodes {
        return Err(over_budget(node, root, budget.limits.max_nodes));
    }
    match &node.kind {
        ExprKind::Number { value, .. } => Ok(*value),
        ExprKind::Identifier(name) => match env.value(name) {
            Some(value) => Ok(value),
            None => Err(undefined_parameter(node, name)),
        },
        ExprKind::Group(inner) => eval(inner, root, env, budget, depth + 1),
        ExprKind::Unary { op, operand } => {
            let v = eval(operand, root, env, budget, depth + 1)?;
            Ok(match op {
                UnaryOp::Plus => v,
                UnaryOp::Minus => -v,
            })
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let x = eval(lhs, root, env, budget, depth + 1)?;
            let y = eval(rhs, root, env, budget, depth + 1)?;
            binary(*op, x, y, node, rhs, root)
        }
        ExprKind::Call {
            function,
            arguments,
        } => builtin_call(node, root, env, budget, depth, *function, arguments),
        ExprKind::UserCall { name, arguments } => {
            user_call(node, root, env, budget, depth, name, arguments)
        }
    }
}

#[inline(never)]
#[cold]
fn too_deep(node: &Expr, root: &ParameterExpression, env: Env<'_>, max: usize) -> Failure {
    let note = if env.frame.is_some() {
        CALL_DEPTH_NOTE
    } else {
        ""
    };
    fail(
        node,
        root,
        format!("expression deeper than {max} levels{note}"),
    )
}

#[inline(never)]
#[cold]
fn over_budget(node: &Expr, root: &ParameterExpression, max: usize) -> Failure {
    fail(
        node,
        root,
        format!("evaluation exceeded the budget of {max} expression nodes"),
    )
}

#[inline(never)]
#[cold]
fn undefined_parameter(node: &Expr, name: &str) -> Failure {
    Failure {
        location: node.span.start.clone(),
        message: format!("undefined parameter '{name}'"),
        unsupported: false,
    }
}

/// One binary operation on evaluated operands.
#[inline(never)]
fn binary(
    op: BinaryOp,
    x: Real,
    y: Real,
    node: &Expr,
    rhs: &Expr,
    root: &ParameterExpression,
) -> Result<Real, Failure> {
    let result = match op {
        BinaryOp::Add => x + y,
        BinaryOp::Sub => x - y,
        BinaryOp::Mul => x * y,
        BinaryOp::Div => {
            if y == 0.0 {
                return Err(Failure {
                    location: rhs.span.start.clone(),
                    message: format!("division by zero ({x} / {y}) in `{}`", snippet(root, node)),
                    unsupported: false,
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

/// A built-in call, or the `.func` that redefines it.
#[inline(never)]
fn builtin_call(
    node: &Expr,
    root: &ParameterExpression,
    env: Env<'_>,
    budget: &mut EvalBudget,
    depth: usize,
    function: Function,
    arguments: &[Expr],
) -> Result<Real, Failure> {
    let mut args = Vec::with_capacity(arguments.len());
    for a in arguments {
        args.push(eval(a, root, env, budget, depth + 1)?);
    }
    // A `.func` of the same name replaces the built-in, as C's textual
    // expansion does before numparam sees the call.
    if let Some((def, scope)) = env.function(function.name()) {
        return call(node, root, env, budget, depth, def, scope, args);
    }
    let result = apply(function, &args);
    checked(result, node, root, || {
        let list: Vec<String> = args.iter().map(ToString::to_string).collect();
        format!("{}({})", function.name(), list.join(", "))
    })
}

/// The behavioural-source probe functions. C accepts `v(...)` and `i(...)`
/// in a device value such as `r1 1 0 {1/i(v1)}` by rewriting the device into
/// a behavioural one, so a call of either with no `.func` in scope is
/// reported as not yet ported rather than as an invalid deck.
const PROBE_FUNCTIONS: &[&str] = &["v", "i"];

/// A call of a name outside the built-in allowlist.
#[inline(never)]
fn user_call(
    node: &Expr,
    root: &ParameterExpression,
    env: Env<'_>,
    budget: &mut EvalBudget,
    depth: usize,
    name: &str,
    arguments: &[Expr],
) -> Result<Real, Failure> {
    let Some((def, scope)) = env.function(name) else {
        if EXCLUDED_FUNCTIONS.contains(&name) {
            return Err(Failure {
                location: node.span.start.clone(),
                message: format!(
                    "function '{name}' is a numparam function outside the bounded \
                     allowlist in `{}`",
                    snippet(root, node)
                ),
                unsupported: true,
            });
        }
        if PROBE_FUNCTIONS
            .iter()
            .any(|probe| probe.eq_ignore_ascii_case(name))
        {
            return Err(Failure {
                location: node.span.start.clone(),
                message: format!(
                    "behavioural probe function '{name}' in `{}`: C turns a device \
                     value that reads a node voltage or branch current into a \
                     behavioural source (see src/frontend/inpcom.c \
                     `b_transformation_wanted()`), which is not \
                     ported",
                    snippet(root, node)
                ),
                unsupported: true,
            });
        }
        return Err(fail(
            node,
            root,
            format!("undefined function '{name}' (no .func definition is in scope)"),
        ));
    };
    let mut args = Vec::with_capacity(arguments.len());
    for a in arguments {
        args.push(eval(a, root, env, budget, depth + 1)?);
    }
    call(node, root, env, budget, depth, def, scope, args)
}

/// Evaluates one `.func` call with already evaluated arguments.
#[allow(clippy::too_many_arguments)]
fn call(
    node: &Expr,
    root: &ParameterExpression,
    env: Env<'_>,
    budget: &mut EvalBudget,
    depth: usize,
    def: &FunctionDef,
    scope: &FunctionScope,
    args: Vec<Real>,
) -> Result<Real, Failure> {
    if args.len() != def.parameters.len() {
        return Err(fail(
            node,
            root,
            format!(
                "function '{}' (defined at {}) takes {} argument(s), found {}",
                def.name,
                def.location,
                def.parameters.len(),
                args.len()
            ),
        ));
    }
    let mut outer = env.frame;
    while let Some(f) = outer {
        if std::ptr::eq(f.def, def) {
            // Static checks reject lexical cycles; this catches one formed
            // through a call-site fallback. C recurses without bound.
            return Err(fail(
                node,
                root,
                format!(
                    "recursive call of function '{}' (defined at {})",
                    def.name, def.location
                ),
            ));
        }
        outer = f.outer;
    }
    let frame = Frame {
        def,
        scope,
        args,
        call: node.span.start.clone(),
        outer: env.frame,
    };
    let inner = Env {
        lookup: env.lookup,
        functions: env.functions,
        frame: Some(&frame),
    };
    let value =
        eval(&def.body.root, &def.body, inner, budget, depth + CALL_DEPTH).map_err(|f| {
            f.context(|| {
                format!(
                    "in function '{}' (defined at {}) called at {}",
                    def.name, def.location, frame.call
                )
            })
        })?;
    checked(value, node, root, || format!("{}(...)", def.name))
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
