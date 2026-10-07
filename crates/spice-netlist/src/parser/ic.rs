//! BJTparam/MOS1param IC vector fallthrough order. A partial vector sets only
//! supplied components; keeping it ordered beside scalar IC setters preserves
//! duplicate/last-set semantics without enabling runtime initialization.

use winnow::Parser as _;
use winnow::combinator::{cut_err, opt};

use crate::ast::{InitialCondition, ParameterAssignment, ParameterKind};

use super::grammar::{Input, Result, keyword};
use super::syntax::equals;
use super::vector::{numeric, positioned};

pub(super) fn vector(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = keyword("ic").parse_next(input)?;
    cut_err(|input: &mut Input<'_>| {
        opt(equals).parse_next(input)?;
        let components: &[&str] = if input.state.card.designator() == Some('q') {
            &["icvbe", "icvce"]
        } else {
            &["icvds", "icvgs", "icvbs"]
        };
        let vector = numeric(input, components.len())?;
        let values = vector
            .values
            .iter()
            .zip(components)
            .map(|(value, name)| InitialCondition {
                name: (*name).to_owned(),
                value: positioned(value),
            })
            .collect();
        Ok(ParameterAssignment {
            name: "ic".to_owned(),
            value: vector.text,
            kind: ParameterKind::InitialConditions(values),
            location: token.location.clone(),
        })
    })
    .parse_next(input)
}
