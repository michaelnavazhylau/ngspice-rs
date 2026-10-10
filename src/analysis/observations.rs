//! Discover requested device quantities without widening ordinary plots.
use crate::netlist::ast::{
    FourierCard, MeasureCard, MeasureEvent, MeasureRequest, OutputCards, RequestedVector,
    VectorRequest,
};
use crate::primitives::AnalysisKind;

/// Device observations needed by output and post-processing cards for one analysis.
#[must_use]
pub fn requests(
    kind: AnalysisKind,
    output: &OutputCards,
    measures: &[MeasureCard],
    fourier: &[FourierCard],
) -> Vec<String> {
    fn vector(v: &VectorRequest, out: &mut Vec<String>) {
        let name = match &v.vector {
            RequestedVector::Current { .. } => Some(v.vector.name()),
            RequestedVector::Named { name, .. } if name.starts_with('@') => Some(name.clone()),
            _ => None,
        };
        if let Some(name) = name
            && !out.contains(&name)
        {
            out.push(name);
        }
    }
    fn event(e: &MeasureEvent, out: &mut Vec<String>) {
        match e {
            MeasureEvent::Crossing { operand, .. } => vector(operand, out),
            MeasureEvent::VectorCrossing {
                operand, reference, ..
            } => {
                vector(operand, out);
                vector(reference, out);
            }
            MeasureEvent::Delayed { event: e, .. } => event(e, out),
            MeasureEvent::At { .. } => {}
        }
    }
    fn measure(m: &MeasureRequest, out: &mut Vec<String>) {
        match m {
            MeasureRequest::Deferred { template, .. } => measure(template, out),
            MeasureRequest::Parameter(_) => {}
            MeasureRequest::Find { operand, .. } | MeasureRequest::Statistic { operand, .. } => {
                vector(operand, out)
            }
            MeasureRequest::AtEvent {
                operand, event: e, ..
            } => {
                vector(operand, out);
                event(e, out);
            }
            MeasureRequest::When { event: e, .. } => event(e, out),
            MeasureRequest::TrigTarg { trig, targ, .. } => {
                event(trig, out);
                event(targ, out);
            }
        }
    }
    let mut out = Vec::new();
    for card in &output.saves {
        for v in &card.requests {
            vector(v, &mut out);
        }
    }
    for card in &output.prints {
        if card.analysis == kind {
            for v in &card.requests {
                vector(v, &mut out);
            }
        }
    }
    for card in measures {
        if card.analysis == kind {
            measure(&card.request, &mut out);
        }
    }
    if kind == AnalysisKind::Transient {
        for card in fourier {
            for v in &card.vectors {
                vector(v, &mut out);
            }
        }
    }
    out
}
