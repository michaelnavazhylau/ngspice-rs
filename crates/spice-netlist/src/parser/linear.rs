//! Scalar linear-instance grammars from `src/spicelib/parser/inp2{r,c,l,v,i}.c`.
//!
//! `INPdevParse()` (`inpdpar.c`) handles the leading value and named parameters;
//! R/C/L parameter names come from `res/res.c`, `cap/cap.c`, and `ind/ind.c`.
//! Model-backed passives, expressions, flags and waveforms remain explicit gaps.

use spice_core::{SourceLoc, SpiceError, SpiceResult};

use crate::ast::{DeviceInstance, ParameterAssignment};
use crate::card::RawCard;
use crate::token::{Token, TokenKind};

pub(super) fn parse(
    card: &RawCard,
    designator: char,
    auto_gnd: bool,
) -> SpiceResult<DeviceInstance> {
    let reference = match designator {
        'r' => "src/spicelib/parser/inp2r.c",
        'c' => "src/spicelib/parser/inp2c.c",
        'l' => "src/spicelib/parser/inp2l.c",
        'v' => "src/spicelib/parser/inp2v.c",
        'i' => "src/spicelib/parser/inp2i.c",
        'x' => "src/frontend/subckt.c",
        'a' => "src/xspice/",
        _ => {
            return Err(SpiceError::not_yet_ported(
                format!("{}: device grammar '{designator}'", card.location),
                format!("src/spicelib/parser/inp2{designator}.c"),
            ));
        }
    };
    if !matches!(designator, 'r' | 'c' | 'l' | 'v' | 'i') {
        return Err(SpiceError::not_yet_ported(
            format!("{}: device grammar '{designator}'", card.location),
            reference,
        ));
    }

    let mut cursor = Cursor {
        card,
        tokens: card.arguments(),
        index: 0,
        reference,
    };
    let mut nodes = Vec::with_capacity(2);
    for terminal in ["positive terminal", "negative terminal"] {
        let token = cursor.take(terminal)?;
        if !token.is_name_like() {
            return Err(SpiceError::parse(
                token.location.clone(),
                format!("expected a node name for {terminal}"),
            ));
        }
        // inpcom.c: inp_fix_gnd_name(); fold case only in identifiers, never in
        // numeric text. Node ids are assigned later by circuit elaboration.
        let name = token.text.to_ascii_lowercase();
        nodes.push(if auto_gnd && name == "gnd" {
            "0".to_owned()
        } else {
            name
        });
    }
    let parameters = if matches!(designator, 'v' | 'i') {
        source_parameters(&mut cursor)?
    } else {
        passive_parameters(&mut cursor, designator)?
    };
    Ok(DeviceInstance {
        name: card
            .first_token()
            .expect("device has a first token")
            .text
            .to_ascii_lowercase(),
        designator,
        nodes,
        model: None,
        parameters,
        location: card.location.clone(),
    })
}

fn passive_parameters(
    cursor: &mut Cursor<'_>,
    designator: char,
) -> SpiceResult<Vec<ParameterAssignment>> {
    let primary = match designator {
        'r' => "resistance",
        'c' => "capacitance",
        _ => "inductance",
    };
    let mut parameters = Vec::new();
    if cursor.peek().is_some_and(|t| t.number().is_some()) {
        let value = cursor.literal(primary)?;
        parameters.push(assignment(primary, value));
    }
    while let Some(token) = cursor.peek() {
        let name = token.text.to_ascii_lowercase();
        let canonical = match (designator, name.as_str()) {
            ('r', "r" | "resistance") => primary,
            ('c', "c" | "cap" | "capacitance") => primary,
            ('l', "l" | "inductance") => primary,
            (_, "temp" | "dtemp" | "m" | "tc1" | "tc2" | "scale") => &name,
            ('r' | 'c', "w" | "l" | "bv_max") => &name,
            ('r', "ac" | "tc" | "tce") => &name,
            ('c' | 'l', "ic") => &name,
            ('l', "nt") => &name,
            _ => return Err(cursor.gap("passive models, expressions or non-scalar parameters")),
        };
        let location = token.location.clone();
        cursor.index += 1;
        // INPgetTok() gobbles '=', so both `tc1=0.01` and `tc1 0.01`
        // are legal in the C grammar.
        cursor.skip_equals();
        let value = cursor.literal(canonical)?;
        parameters.push(ParameterAssignment {
            name: canonical.to_owned(),
            value: value.text.clone(),
            location,
        });
    }
    if !parameters.iter().any(|p| p.name == primary) {
        return Err(SpiceError::parse(
            cursor.location(),
            format!("expected {primary} (model-backed instances are not ported)"),
        ));
    }
    Ok(parameters)
}

/// `INP2V()`/`INP2I()` apply the leading DC value *after* `INPdevParse()`.
/// AC defaults (magnitude 1, phase 0) follow `VSRCtemp()`/`ISRCtemp()`;
/// `VSRCparam()`/`ISRCparam()` accept zero, one or two AC vector entries.
fn source_parameters(cursor: &mut Cursor<'_>) -> SpiceResult<Vec<ParameterAssignment>> {
    let leading_dc = if cursor.peek().is_some_and(|t| t.number().is_some()) {
        Some(assignment("dc", cursor.literal("DC value")?))
    } else {
        None
    };
    let mut parameters = Vec::new();
    while let Some(token) = cursor.peek() {
        if token.is_keyword("dc") {
            let location = token.location.clone();
            cursor.index += 1;
            cursor.skip_equals();
            let value = cursor.literal("DC value")?;
            parameters.push(ParameterAssignment {
                name: "dc".to_owned(),
                value: value.text.clone(),
                location,
            });
        } else if token.is_keyword("ac") {
            let location = token.location.clone();
            cursor.index += 1;
            cursor.skip_equals();
            let magnitude = optional_ac_literal(cursor, "acmag", "1", &location)?;
            let phase = optional_ac_literal(cursor, "acphase", "0", &location)?;
            parameters.extend([magnitude, phase]);
        } else {
            return Err(cursor.gap("source waveforms, expressions or additional source parameters"));
        }
    }
    if let Some(value) = leading_dc {
        parameters.push(value);
    }
    // An empty source specification is legal: C uses DC 0. Do not invent an
    // explicit assignment; absence must remain distinguishable from DC 0.
    Ok(parameters)
}

fn optional_ac_literal(
    cursor: &mut Cursor<'_>,
    name: &str,
    default: &str,
    location: &SourceLoc,
) -> SpiceResult<ParameterAssignment> {
    if cursor.peek().is_some_and(|t| t.number().is_some()) {
        return Ok(assignment(name, cursor.literal(name)?));
    }
    if cursor
        .peek()
        .is_some_and(|t| matches!(t.kind, TokenKind::Expression(_) | TokenKind::Quoted(_)))
    {
        return Err(cursor.gap("AC parameter expressions"));
    }
    Ok(ParameterAssignment {
        name: name.to_owned(),
        value: default.to_owned(),
        location: location.clone(),
    })
}

fn assignment(name: &str, value: &Token) -> ParameterAssignment {
    ParameterAssignment {
        name: name.to_owned(),
        value: value.text.clone(),
        location: value.location.clone(),
    }
}

struct Cursor<'a> {
    card: &'a RawCard,
    tokens: &'a [Token],
    index: usize,
    reference: &'static str,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.index)
    }

    fn location(&self) -> SourceLoc {
        self.peek().map_or_else(
            || {
                self.card.location.at_column(
                    u32::try_from(self.card.raw.len())
                        .unwrap_or(u32::MAX)
                        .saturating_add(1),
                )
            },
            |token| token.location.clone(),
        )
    }

    fn take(&mut self, expected: &str) -> SpiceResult<&'a Token> {
        let token = self
            .peek()
            .ok_or_else(|| SpiceError::parse(self.location(), format!("expected {expected}")))?;
        self.index += 1;
        Ok(token)
    }

    fn skip_equals(&mut self) {
        if self.peek().is_some_and(|t| t.kind == TokenKind::Equals) {
            self.index += 1;
        }
    }

    fn literal(&mut self, expected: &str) -> SpiceResult<&'a Token> {
        let token = self
            .peek()
            .ok_or_else(|| SpiceError::parse(self.location(), format!("expected {expected}")))?;
        match &token.kind {
            TokenKind::Number(value) if value.is_finite() => {
                self.index += 1;
                Ok(token)
            }
            TokenKind::Word | TokenKind::Expression(_) | TokenKind::Quoted(_) => {
                Err(self.gap("parameter expressions, model references or extended numeric syntax"))
            }
            _ => Err(SpiceError::parse(
                token.location.clone(),
                format!("expected a finite numeric literal for {expected}"),
            )),
        }
    }

    fn gap(&self, what: &str) -> SpiceError {
        SpiceError::not_yet_ported(format!("{}: {what}", self.location()), self.reference)
    }
}
