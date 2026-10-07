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
//! Subcircuit bodies are left untouched (their expressions need formal
//! binding, which is not ported; elaboration keeps rejecting subcircuits).
//! Bare names in device cards are not references (see
//! `docs/port/PARAM_EXPRESSIONS.md`), so terminals and model names are never
//! substituted.

use std::sync::Arc;

use spice_core::{Real, SourceLoc, SpiceError, SpiceResult};

use crate::ast::{AnalysisCard, Netlist, ParameterAssignment, ParameterKind};
use crate::eval::{EvalBudget, EvalLimits, ParamScope};

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
    let scope = Arc::new(ParamScope::resolve(
        None,
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
    Ok(ElaboratedNetlist {
        netlist: out,
        scope,
        sites,
    })
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
