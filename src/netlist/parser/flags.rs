//! Initial-engine IF_FLAG table from dio.c, bjt.c, mos1.c and their setters.
//! INPgetValue(IF_FLAG), in inpgval.c, supplies 1 without consuming a value.
//! Base model type tokens are not assignments; tail type flags are setters.
//! See docs/port/FRONTEND_VALUES.md for the exhaustive supported/gap table.

use winnow::Parser as _;
use winnow::token::any;

use crate::netlist::ast::{ParameterAssignment, ParameterKind};
use crate::netlist::token::{Token, TokenKind};

use super::grammar::{Input, Result, keyword};
use super::syntax::malformed;

pub(super) fn instance(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = keyword("off").parse_next(input)?;
    flag(input, token)
}

pub(super) fn model(input: &mut Input<'_>, base: &str) -> Result<ParameterAssignment> {
    let token = any
        .verify(|token: &Token| {
            let key = token.text.to_ascii_lowercase();
            match base.to_ascii_lowercase().as_str() {
                "d" => key == "d",
                "npn" | "pnp" => matches!(key.as_str(), "npn" | "pnp"),
                "nmos" | "pmos" => matches!(key.as_str(), "nmos" | "pmos"),
                // SW_MOD_SW / CSW_CSW: "just says that this is a switch".
                "sw" => key == "sw",
                "csw" => key == "csw",
                // JFETmPTable: JFET_MOD_NJF / JFET_MOD_PJF set the type.
                "njf" | "pjf" => matches!(key.as_str(), "njf" | "pjf"),
                // URC_MOD_URC: "already know we are a URC" (urcmpar.c no-op).
                "urc" => key == "urc",
                _ => false,
            }
        })
        .parse_next(input)?;
    flag(input, token)
}

fn flag(input: &mut Input<'_>, token: &Token) -> Result<ParameterAssignment> {
    if input.input.first().is_some_and(|next| {
        next.kind == TokenKind::Equals
            || next.number().is_some()
            || matches!(next.kind, TokenKind::Expression(_) | TokenKind::Quoted(_))
    }) {
        return Err(malformed(input, "bare flag does not take a value"));
    }
    Ok(ParameterAssignment {
        name: token.text.to_ascii_lowercase(),
        value: String::new(),
        kind: ParameterKind::Flag,
        location: token.location.clone(),
    })
}
