//! Literalization: replace evaluated numeric sites with finite scalars.
//!
//! [`literalize`] resolves the top-level `.param` cards ([`ParamScope`]) and
//! produces a *copy* of the [`Netlist`] in which every top-level
//! [`ParameterKind::Expression`] device/model parameter and every braced
//! analysis argument is replaced by its finite value, spelled so that
//! `parse_spice_number` reads back the exact `f64` (plain integer digits for
//! integral values, otherwise `{:e}`). The input AST is
//! never modified; the copy keeps `.param` cards, locations and card order. The
//! original spelling and location of each replaced site are kept in
//! [`ElaboratedNetlist::sites`].
//!
//! Top-level `.ic`/`.nodeset` expression values are literalized too (the
//! ordered entries are available from [`ElaboratedNetlist::initial_conditions`]
//! and [`ElaboratedNetlist::nodesets`]).
//!
//! Subcircuit bodies are left untouched here (their expressions need formal
//! binding, which happens during expansion in `spice_devices::subckt`; this
//! pass does not reject subcircuit decks).
//! Bare names in device cards are not references (see
//! `docs/port/PARAM_EXPRESSIONS.md`), so terminals and model names are never
//! substituted.

use std::sync::Arc;

use spice_core::{Real, SourceLoc, SpiceError, SpiceResult};

use crate::ast::{
    AnalysisCard, Netlist, NodeHint, NodeHintCard, NodeHintValue, ParameterAssignment,
    ParameterKind,
};
use crate::eval::{EvalBudget, EvalLimits, FunctionScope, ParamScope};

/// Which site was literalized (indexes into the *input* netlist).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteKind {
    /// `Netlist::devices[device].parameters[parameter]`.
    DeviceParameter {
        /// Device index.
        device: usize,
        /// Parameter index.
        parameter: usize,
    },
    /// `Netlist::models[model].parameters[parameter]`.
    ModelParameter {
        /// Model index.
        model: usize,
        /// Parameter index.
        parameter: usize,
    },
    /// `Netlist::analyses[analysis].arguments[argument]`.
    AnalysisArgument {
        /// Analysis index.
        analysis: usize,
        /// Argument index.
        argument: usize,
    },
    /// `Netlist::initial_conditions[card].entries[entry]`.
    InitialCondition {
        /// `.ic` card index.
        card: usize,
        /// Entry index within the card.
        entry: usize,
    },
    /// `Netlist::nodesets[card].entries[entry]`.
    Nodeset {
        /// `.nodeset` card index.
        card: usize,
        /// Entry index within the card.
        entry: usize,
    },
}

/// One evaluated site with its original text.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSite {
    /// Where in the input netlist.
    pub kind: SiteKind,
    /// Original spelling, braces included when written.
    pub original: String,
    /// Location of the expression text.
    pub location: SourceLoc,
    /// The evaluated finite value.
    pub value: Real,
}

/// A literalized copy plus the evaluation record.
#[derive(Debug, Clone, PartialEq)]
pub struct ElaboratedNetlist {
    /// The netlist copy with evaluated sites replaced by scalars.
    pub netlist: Netlist,
    /// The resolved top-level parameter scope.
    pub scope: Arc<ParamScope>,
    /// Every replaced site in deck order.
    pub sites: Vec<ResolvedSite>,
}

/// One `.ic`/`.nodeset` entry with a finite value (see
/// [`ElaboratedNetlist::initial_conditions`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedNodeHint<'a> {
    /// Canonical node name.
    pub node: &'a str,
    /// Finite value in volts.
    pub value: Real,
    /// Where the entry (`V(node)=value`) started.
    pub location: &'a SourceLoc,
}

impl ElaboratedNetlist {
    /// Ordered `.ic` entries `(node, value, location)`, duplicates preserved
    /// (card order, then entry order). Every expression was evaluated by
    /// [`literalize`], so values are finite. Syntax only: no precedence among
    /// duplicates, `uic`, instance `ic=` or DC bias is applied here.
    #[must_use]
    pub fn initial_conditions(&self) -> Vec<ResolvedNodeHint<'_>> {
        resolved(self.netlist.initial_conditions())
    }

    /// Ordered `.nodeset` entries, as [`Self::initial_conditions`]. A nodeset
    /// is a convergence hint, not a constraint.
    #[must_use]
    pub fn nodesets(&self) -> Vec<ResolvedNodeHint<'_>> {
        resolved(self.netlist.nodesets())
    }
}

fn resolved<'a>(entries: impl Iterator<Item = &'a NodeHint>) -> Vec<ResolvedNodeHint<'a>> {
    // `literalize` replaces every expression, so `literal()` is always `Some`
    // for an elaborated netlist; an unevaluated entry is never reported with a
    // made-up value.
    entries
        .filter_map(|entry| {
            Some(ResolvedNodeHint {
                node: &entry.node,
                value: entry.literal()?,
                location: &entry.location,
            })
        })
        .collect()
}

/// Spell a finite value so it parses back exactly.
#[must_use]
pub fn format_literal(value: Real) -> String {
    // Integral values keep a plain integer spelling: count-like analysis
    // arguments (`.ac dec 5 ..`) are parsed as integers by the drivers.
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value:e}")
    }
}

/// Resolve `.param` cards and literalize all top-level sites with default limits.
///
/// # Errors
/// Any parameter or site evaluation failure (undefined, cyclic, domain,
/// non-finite, over budget), with source locations.
pub fn literalize(netlist: &Netlist) -> SpiceResult<ElaboratedNetlist> {
    let mut budget = EvalBudget::new(EvalLimits::default());
    let functions = FunctionScope::for_netlist(netlist, &budget)?;
    let scope = Arc::new(ParamScope::resolve_scoped(
        None,
        Some(functions),
        &[],
        &netlist.params,
        &mut budget,
    )?);
    literalize_with(netlist, scope, &mut budget)
}

/// Literalize against an already resolved scope.
///
/// # Errors
/// As [`literalize`].
pub fn literalize_with(
    netlist: &Netlist,
    scope: Arc<ParamScope>,
    budget: &mut EvalBudget,
) -> SpiceResult<ElaboratedNetlist> {
    let mut out = netlist.clone();
    let mut sites = Vec::new();
    for (di, device) in out.devices.iter_mut().enumerate() {
        literalize_parameters(&mut device.parameters, &scope, budget, &mut sites, |p| {
            SiteKind::DeviceParameter {
                device: di,
                parameter: p,
            }
        })?;
    }
    for (mi, model) in out.models.iter_mut().enumerate() {
        literalize_parameters(&mut model.parameters, &scope, budget, &mut sites, |p| {
            SiteKind::ModelParameter {
                model: mi,
                parameter: p,
            }
        })?;
    }
    for (ai, card) in out.analyses.iter_mut().enumerate() {
        let before = card.clone();
        *card = literalize_analysis(&before, &scope, budget)?;
        for e in &before.expressions {
            let value = card.arguments[e.index]
                .parse::<Real>()
                .expect("literalized argument");
            sites.push(ResolvedSite {
                kind: SiteKind::AnalysisArgument {
                    analysis: ai,
                    argument: e.index,
                },
                original: before.arguments[e.index].clone(),
                location: e.expression.span.start.clone(),
                value,
            });
        }
    }
    literalize_hints(
        &mut out.initial_conditions,
        &scope,
        budget,
        &mut sites,
        |c, e| SiteKind::InitialCondition { card: c, entry: e },
    )?;
    literalize_hints(&mut out.nodesets, &scope, budget, &mut sites, |c, e| {
        SiteKind::Nodeset { card: c, entry: e }
    })?;
    Ok(ElaboratedNetlist {
        netlist: out,
        scope,
        sites,
    })
}

/// Evaluates only the `.ic` and `.nodeset` entries against `scope`, returning
/// literalized copies of those cards (`.ic` first, `.nodeset` second) without
/// elaborating the rest of the deck. Order and duplicates are preserved.
///
/// # Errors
/// Any entry evaluation failure, located at the expression.
pub fn literalize_node_hints(
    netlist: &Netlist,
    scope: &ParamScope,
    budget: &mut EvalBudget,
) -> SpiceResult<(Vec<NodeHintCard>, Vec<NodeHintCard>)> {
    let mut sites = Vec::new();
    let mut initial = netlist.initial_conditions.clone();
    let mut nodesets = netlist.nodesets.clone();
    literalize_hints(&mut initial, scope, budget, &mut sites, |c, e| {
        SiteKind::InitialCondition { card: c, entry: e }
    })?;
    literalize_hints(&mut nodesets, scope, budget, &mut sites, |c, e| {
        SiteKind::Nodeset { card: c, entry: e }
    })?;
    Ok((initial, nodesets))
}

fn literalize_hints(
    cards: &mut [NodeHintCard],
    scope: &ParamScope,
    budget: &mut EvalBudget,
    sites: &mut Vec<ResolvedSite>,
    kind: impl Fn(usize, usize) -> SiteKind,
) -> SpiceResult<()> {
    for (ci, card) in cards.iter_mut().enumerate() {
        for (ei, hint) in card.entries.iter_mut().enumerate() {
            let NodeHintValue::Expression(expression) = &hint.value else {
                continue;
            };
            let value = scope
                .evaluate(expression, budget)
                .map_err(|error| match error {
                    SpiceError::Parse { location, message } => SpiceError::parse(
                        location,
                        format!(
                            "{message}\n  while evaluating the value of V({}) at {}",
                            hint.node, hint.location
                        ),
                    ),
                    other => other,
                })?;
            sites.push(ResolvedSite {
                kind: kind(ci, ei),
                original: if expression.braced {
                    format!("{{{}}}", expression.text)
                } else {
                    expression.text.clone()
                },
                location: expression.span.start.clone(),
                value,
            });
            hint.value = NodeHintValue::Literal {
                text: format_literal(value),
                value,
            };
        }
    }
    Ok(())
}

fn literalize_parameters(
    parameters: &mut [ParameterAssignment],
    scope: &ParamScope,
    budget: &mut EvalBudget,
    sites: &mut Vec<ResolvedSite>,
    kind: impl Fn(usize) -> SiteKind,
) -> SpiceResult<()> {
    for (index, parameter) in parameters.iter_mut().enumerate() {
        let ParameterKind::Expression(expression) = &parameter.kind else {
            continue;
        };
        let value = scope
            .evaluate(expression, budget)
            .map_err(|error| match error {
                SpiceError::Parse { location, message } => SpiceError::parse(
                    location,
                    format!(
                        "{message}\n  while evaluating parameter '{}' = {} at {}",
                        parameter.name, parameter.value, parameter.location
                    ),
                ),
                other => other,
            })?;
        sites.push(ResolvedSite {
            kind: kind(index),
            original: parameter.value.clone(),
            location: expression.span.start.clone(),
            value,
        });
        parameter.value = format_literal(value);
        parameter.kind = ParameterKind::Scalar;
    }
    Ok(())
}

/// Replace the braced arguments of one analysis card with their values.
///
/// # Errors
/// Evaluation failures of the argument expressions.
pub fn literalize_analysis(
    card: &AnalysisCard,
    scope: &ParamScope,
    budget: &mut EvalBudget,
) -> SpiceResult<AnalysisCard> {
    let mut out = card.clone();
    for e in &card.expressions {
        let value = scope
            .evaluate(&e.expression, budget)
            .map_err(|error| match error {
                SpiceError::Parse { location, message } => SpiceError::parse(
                    location,
                    format!(
                        "{message}\n  while evaluating argument {} of the {:?} card at {}",
                        e.index + 1,
                        card.kind,
                        card.location
                    ),
                ),
                other => other,
            })?;
        out.arguments[e.index] = format_literal(value);
    }
    out.expressions.clear();
    Ok(out)
}
