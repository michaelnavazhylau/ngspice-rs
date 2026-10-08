//! Location-free semantic comparison of parsed decks.
//!
//! A re-parsed deck never has the original [`SourceLoc`]s, joined-card text or
//! token spans, so [`PartialEq`] on [`Netlist`] cannot be used for round-trip
//! checks. [`semantic_form`] returns a copy of a netlist with every position
//! and every purely lexical field neutralised; [`semantic_eq`] compares two
//! netlists through it and [`semantic_diff`] explains the first difference.
//!
//! What is **kept** (it is semantics): titles, device/model/subcircuit/
//! analysis (including the `.tran` `uic` flag)/directive/`.param`/`.option`/`.global`/`.ic`/`.nodeset` content, ordered parameter
//! assignments with their kinds and value spelling, parsed expression trees and
//! their original text, the card order with typed indexes, include-chain depth
//! and resolved include paths.
//!
//! What is **neutralised**: every [`SourceLoc`] and span (file, line, column);
//! [`Netlist::path`]; the joined text/tokens of each [`RawCard`] (formatting
//! only; the typed AST carries the meaning); [`IncludeDirective::path_spelling`]
//! (quoting only; the decoded `path` is compared); and the original vector text
//! of waveform and `ic` setters (their structured values are compared, and the
//! normalized writer re-spells the vector).

use std::path::PathBuf;

use spice_core::SourceLoc;

use crate::ast::{
    AnalysisCard, ArgumentExpression, DeviceInstance, GlobalCard, GlobalNode, IncludeDirective,
    InitialCondition, LibrarySection, ModelCard, Netlist, NodeHint, NodeHintCard, NodeHintValue,
    OptionCard, OptionSetting, ParamAssignment, ParamCard, ParameterAssignment, ParameterKind,
    PositionedValue, PulseWaveform, PwlPoint, ScopedCard, ScopedCardKind, SourceWaveform,
    Subcircuit,
};
use crate::card::{CardKind, RawCard};
use crate::expr::{Expr, ExprKind, ParameterExpression, SourceSpan};

fn blank() -> SourceLoc {
    SourceLoc::new(PathBuf::new(), 0, 0)
}

fn span() -> SourceSpan {
    SourceSpan {
        start: blank(),
        end: blank(),
    }
}

fn raw() -> RawCard {
    RawCard {
        location: blank(),
        raw: String::new(),
        tokens: Vec::new(),
        kind: CardKind::Unknown,
    }
}

/// Strips spans from an expression tree, keeping shape, spellings and groups.
#[must_use]
pub fn expr_form(expr: &Expr) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Number { value, spelling } => ExprKind::Number {
            value: *value,
            spelling: spelling.clone(),
        },
        ExprKind::Identifier(name) => ExprKind::Identifier(name.clone()),
        ExprKind::Unary { op, operand } => ExprKind::Unary {
            op: *op,
            operand: Box::new(expr_form(operand)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(expr_form(lhs)),
            rhs: Box::new(expr_form(rhs)),
        },
        ExprKind::Call {
            function,
            arguments,
        } => ExprKind::Call {
            function: *function,
            arguments: arguments.iter().map(expr_form).collect(),
        },
        ExprKind::Group(inner) => ExprKind::Group(Box::new(expr_form(inner))),
    };
    Expr { kind, span: span() }
}

fn expression_form(expression: &ParameterExpression) -> ParameterExpression {
    ParameterExpression {
        text: expression.text.clone(),
        braced: expression.braced,
        span: span(),
        root: expr_form(&expression.root),
    }
}

fn positioned(value: &PositionedValue) -> PositionedValue {
    PositionedValue {
        text: value.text.clone(),
        location: blank(),
    }
}

fn waveform(waveform: &SourceWaveform) -> SourceWaveform {
    match waveform {
        SourceWaveform::Pulse(pulse) => {
            let optional = |value: &Option<PositionedValue>| value.as_ref().map(positioned);
            SourceWaveform::Pulse(Box::new(PulseWaveform {
                initial: positioned(&pulse.initial),
                pulsed: positioned(&pulse.pulsed),
                delay: optional(&pulse.delay),
                rise: optional(&pulse.rise),
                fall: optional(&pulse.fall),
                width: optional(&pulse.width),
                period: optional(&pulse.period),
            }))
        }
        SourceWaveform::Pwl(points) => SourceWaveform::Pwl(
            points
                .iter()
                .map(|point| PwlPoint {
                    time: positioned(&point.time),
                    value: positioned(&point.value),
                })
                .collect(),
        ),
    }
}

fn assignment(parameter: &ParameterAssignment) -> ParameterAssignment {
    let (kind, value) = match &parameter.kind {
        ParameterKind::Scalar => (ParameterKind::Scalar, parameter.value.clone()),
        ParameterKind::Textual => (ParameterKind::Textual, parameter.value.clone()),
        ParameterKind::Flag => (ParameterKind::Flag, parameter.value.clone()),
        ParameterKind::Expression(expression) => (
            ParameterKind::Expression(Box::new(expression_form(expression))),
            parameter.value.clone(),
        ),
        ParameterKind::InitialConditions(values) => (
            ParameterKind::InitialConditions(
                values
                    .iter()
                    .map(|component| InitialCondition {
                        name: component.name.clone(),
                        value: positioned(&component.value),
                    })
                    .collect(),
            ),
            String::new(),
        ),
        ParameterKind::Waveform(source) => {
            (ParameterKind::Waveform(waveform(source)), String::new())
        }
    };
    ParameterAssignment {
        name: parameter.name.clone(),
        value,
        kind,
        location: blank(),
    }
}

fn assignments(parameters: &[ParameterAssignment]) -> Vec<ParameterAssignment> {
    parameters.iter().map(assignment).collect()
}

fn device(device: &DeviceInstance) -> DeviceInstance {
    DeviceInstance {
        name: device.name.clone(),
        designator: device.designator,
        nodes: device.nodes.clone(),
        model: device.model.clone(),
        parameters: assignments(&device.parameters),
        location: blank(),
    }
}

fn model(model: &ModelCard) -> ModelCard {
    ModelCard {
        name: model.name.clone(),
        base: model.base.clone(),
        level: model.level,
        parameters: assignments(&model.parameters),
        location: blank(),
    }
}

fn analysis(card: &AnalysisCard) -> AnalysisCard {
    AnalysisCard {
        kind: card.kind,
        arguments: card.arguments.clone(),
        expressions: card
            .expressions
            .iter()
            .map(|argument| ArgumentExpression {
                index: argument.index,
                expression: expression_form(&argument.expression),
            })
            .collect(),
        uic: card.uic,
        uic_location: card.uic_location.as_ref().map(|_| blank()),
        location: blank(),
    }
}

fn include(directive: &IncludeDirective) -> IncludeDirective {
    IncludeDirective {
        path: directive.path.clone(),
        path_spelling: String::new(),
        resolved_path: directive.resolved_path.clone(),
        section: directive.section.clone(),
        selected_section: directive
            .selected_section
            .as_ref()
            .map(|section| LibrarySection {
                name: section.name.clone(),
                opening: raw(),
                closing: raw(),
            }),
        location: blank(),
    }
}

fn param(card: &ParamCard) -> ParamCard {
    ParamCard {
        assignments: card
            .assignments
            .iter()
            .map(|assignment| ParamAssignment {
                name: assignment.name.clone(),
                name_span: span(),
                expression: expression_form(&assignment.expression),
            })
            .collect(),
        location: blank(),
    }
}

fn option(card: &OptionCard) -> OptionCard {
    OptionCard {
        settings: card
            .settings
            .iter()
            .map(|setting| OptionSetting {
                name: setting.name.clone(),
                value: setting.value.as_ref().map(positioned),
                location: blank(),
            })
            .collect(),
        location: blank(),
    }
}

fn global(card: &GlobalCard) -> GlobalCard {
    GlobalCard {
        nodes: card
            .nodes
            .iter()
            .map(|node| GlobalNode {
                name: node.name.clone(),
                location: blank(),
            })
            .collect(),
        location: blank(),
    }
}

fn hints(card: &NodeHintCard) -> NodeHintCard {
    NodeHintCard {
        entries: card
            .entries
            .iter()
            .map(|entry| NodeHint {
                node: entry.node.clone(),
                node_location: blank(),
                value: match &entry.value {
                    NodeHintValue::Literal { text, value } => NodeHintValue::Literal {
                        text: text.clone(),
                        value: *value,
                    },
                    NodeHintValue::Expression(expression) => {
                        NodeHintValue::Expression(Box::new(expression_form(expression)))
                    }
                },
                value_location: blank(),
                location: blank(),
            })
            .collect(),
        location: blank(),
    }
}

fn cards(cards: &[ScopedCard]) -> Vec<ScopedCard> {
    cards
        .iter()
        .map(|card| ScopedCard {
            kind: card.kind,
            // `.save`/`.print`/`.measure`/`.four` carry no scope-local index in
            // the netlist and their typed requests live in `OutputCards` /
            // `ParsedDeck::measurements` / `ParsedDeck::fourier`, so the card's
            // own spelling is the netlist's only record of what was requested.
            // Keeping it is what lets `semantic_eq` tell two such cards apart
            // and lets the writer reproduce them from a semantic form; every
            // other card is compared through its typed payload.
            source: match card.kind {
                ScopedCardKind::Output | ScopedCardKind::Measure | ScopedCardKind::Fourier => {
                    RawCard {
                        raw: card.source.raw.clone(),
                        ..raw()
                    }
                }
                _ => raw(),
            },
            include_chain: card.include_chain.iter().map(|_| blank()).collect(),
        })
        .collect()
}

fn subcircuit(sub: &Subcircuit) -> Subcircuit {
    Subcircuit {
        name: sub.name.clone(),
        terminals: sub.terminals.clone(),
        parameters: assignments(&sub.parameters),
        devices: sub.devices.iter().map(device).collect(),
        models: sub.models.iter().map(model).collect(),
        subcircuits: sub.subcircuits.iter().map(subcircuit).collect(),
        analyses: sub.analyses.iter().map(analysis).collect(),
        includes: sub.includes.iter().map(include).collect(),
        params: sub.params.iter().map(param).collect(),
        cards: cards(&sub.cards),
        end_location: blank(),
        location: blank(),
    }
}

/// A copy of `netlist` with positions and lexical-only fields neutralised; see
/// the module documentation for exactly what is kept and what is dropped.
#[must_use]
pub fn semantic_form(netlist: &Netlist) -> Netlist {
    Netlist {
        title: netlist.title.clone(),
        path: PathBuf::new(),
        devices: netlist.devices.iter().map(device).collect(),
        models: netlist.models.iter().map(model).collect(),
        subcircuits: netlist.subcircuits.iter().map(subcircuit).collect(),
        analyses: netlist.analyses.iter().map(analysis).collect(),
        includes: netlist.includes.iter().map(include).collect(),
        params: netlist.params.iter().map(param).collect(),
        options: netlist.options.iter().map(option).collect(),
        globals: netlist.globals.iter().map(global).collect(),
        initial_conditions: netlist.initial_conditions.iter().map(hints).collect(),
        nodesets: netlist.nodesets.iter().map(hints).collect(),
        cards: cards(&netlist.cards),
        location: blank(),
    }
}

/// True when the two netlists are equal after [`semantic_form`].
#[must_use]
pub fn semantic_eq(left: &Netlist, right: &Netlist) -> bool {
    semantic_form(left) == semantic_form(right)
}

fn compare<T: PartialEq + std::fmt::Debug>(what: &str, left: &[T], right: &[T]) -> Option<String> {
    if left.len() != right.len() {
        return Some(format!(
            "{what}: {} entries on the left, {} on the right",
            left.len(),
            right.len()
        ));
    }
    left.iter()
        .zip(right)
        .position(|(l, r)| l != r)
        .map(|index| {
            format!(
                "{what}[{index}] differs\n  left:  {:?}\n  right: {:?}",
                left[index], right[index]
            )
        })
}

/// Describes the first semantic difference between two netlists, or `None` when
/// [`semantic_eq`] holds. The text is for diagnostics, not a stable format.
#[must_use]
pub fn semantic_diff(left: &Netlist, right: &Netlist) -> Option<String> {
    let (l, r) = (semantic_form(left), semantic_form(right));
    if l.title != r.title {
        return Some(format!("title: {:?} vs {:?}", l.title, r.title));
    }
    compare("cards", &l.cards, &r.cards)
        .or_else(|| compare("devices", &l.devices, &r.devices))
        .or_else(|| compare("models", &l.models, &r.models))
        .or_else(|| compare("subcircuits", &l.subcircuits, &r.subcircuits))
        .or_else(|| compare("analyses", &l.analyses, &r.analyses))
        .or_else(|| compare("includes", &l.includes, &r.includes))
        .or_else(|| compare("params", &l.params, &r.params))
        .or_else(|| compare("options", &l.options, &r.options))
        .or_else(|| compare("globals", &l.globals, &r.globals))
        .or_else(|| {
            compare(
                "initial conditions",
                &l.initial_conditions,
                &r.initial_conditions,
            )
        })
        .or_else(|| compare("nodesets", &l.nodesets, &r.nodesets))
}
