//! Bounded Q/M instance grammars from `inp2q.c`, `inp2m.c`, `bjt/bjt.c`
//! and `mos1/mos1.c`. The declaration index follows C's earliest-model terminal
//! scan, including forward references and node/model-name collisions. It does
//! not select or validate a model backend, inject ports/defaults, or do binning.

use winnow::Parser as _;
use winnow::combinator::{alt, cut_err, opt, peek, repeat};
use winnow::token::any;

use crate::ast::{DeviceInstance, ParameterAssignment};
use crate::token::{Token, TokenKind};

use super::grammar::{Input, ParsedCard, Result, gap};
use super::syntax::{
    assignment, canonical_node, equals, leading_value, malformed, name, named, value,
};

type Connections<'a> = (Vec<&'a Token>, &'a Token);

pub(super) fn transistor_card<'a>(input: &mut Input<'a>) -> Result<ParsedCard> {
    let (instance, designator) = any
        .verify_map(|token: &Token| {
            match token.text.as_bytes().first().map(u8::to_ascii_lowercase) {
                Some(b'q') if token.is_name_like() => Some((token, 'q')),
                Some(b'm') if token.is_name_like() => Some((token, 'm')),
                _ => None,
            }
        })
        .parse_next(input)?;
    let (connections, parameters) = cut_err(|input: &mut Input<'a>| {
        let connections = if designator == 'q' {
            bjt_connections(input)?
        } else {
            mos_connections(input)?
        };
        let parameters = parameters(input, designator)?;
        Ok((connections, parameters))
    })
    .parse_next(input)?;
    let (nodes, model) = connections;
    Ok(ParsedCard::Device(DeviceInstance {
        name: instance.text.to_ascii_lowercase(),
        designator,
        nodes: nodes
            .into_iter()
            .map(|node| canonical_node(node, input.state.auto_gnd))
            .collect(),
        model: Some(model.text.to_ascii_lowercase()),
        parameters,
        location: input.state.card.location.clone(),
    }))
}

fn is_declared_model(input: &Input<'_>, token: &Token) -> bool {
    token.is_name_like()
        && input
            .state
            .declared_models
            .contains(&token.text.to_ascii_lowercase())
}

fn declared_model<'a>(input: &mut Input<'a>) -> Result<&'a Token> {
    let token = peek(name("declared model name")).parse_next(input)?;
    if !is_declared_model(input, token) {
        return Err(malformed(
            input,
            "expected a model declared before .end (forward declarations are allowed)",
        ));
    }
    // inpcom.c's inp_get_number_terminals assumes BJT model names contain an
    // alphabetic character, so a purely numeric name is not a supported Q form.
    if input.state.card.designator() == Some('q')
        && !token.text.bytes().any(|byte| byte.is_ascii_alphabetic())
    {
        return Err(malformed(
            input,
            "BJT model name must contain an ASCII alphabetic character",
        ));
    }
    // Numeric-looking model references are outside the bounded identifier
    // grammar. In particular, ngspice-47+ rejects Q models named 123 or 123n
    // during its front-end preprocessing, before terminal binding.
    if token.number().is_some() {
        return Err(gap(input, "numeric-looking transistor model names"));
    }
    any.parse_next(input)
}

fn bjt_connections<'a>(input: &mut Input<'a>) -> Result<Connections<'a>> {
    let (collector, base, emitter) = (
        name("collector terminal"),
        name("base terminal"),
        name("emitter terminal"),
    )
        .parse_next(input)?;
    let mut nodes = vec![collector, base, emitter];
    let candidate = peek(name("BJT model name or substrate terminal")).parse_next(input)?;
    // INP2Q recognises a declared model as soon as the first three ports exist.
    if !is_declared_model(input, candidate) {
        nodes.push(name("substrate terminal").parse_next(input)?);
        // VBIC's fifth/thermal terminal is deliberately outside this grammar.
        if input
            .input
            .get(1)
            .is_some_and(|token| is_declared_model(input, token))
            && input
                .input
                .first()
                .is_some_and(|token| !is_declared_model(input, token))
        {
            return Err(gap(input, "BJT fifth/thermal terminal syntax"));
        }
    }
    let model = declared_model(input)?;
    // Keep an omitted substrate omitted; INP2Q's implicit ground is elaboration.
    Ok((nodes, model))
}

fn mos_connections<'a>(input: &mut Input<'a>) -> Result<Connections<'a>> {
    let (drain, gate, source) = (
        name("drain terminal"),
        name("gate terminal"),
        name("source terminal"),
    )
        .parse_next(input)?;
    let candidate = peek(name("bulk terminal")).parse_next(input)?;
    // C scans from the third port (for VDMOS), so a declared model at this slot
    // cannot be silently treated as a bulk node for an ordinary MOS instance.
    if is_declared_model(input, candidate) {
        return Err(malformed(
            input,
            "expected bulk terminal before the MOS model (three-terminal VDMOS is not supported)",
        ));
    }
    let bulk = name("bulk terminal").parse_next(input)?;
    let candidate = peek(name("MOS model name")).parse_next(input)?;
    if !is_declared_model(input, candidate) {
        // C's SOI/HiSIM/thermal variants can take 5–7 ports; retain a loud gap.
        if input
            .input
            .iter()
            .skip(1)
            .take(3)
            .any(|token| is_declared_model(input, token))
        {
            return Err(gap(input, "MOS extra/thermal terminal syntax"));
        }
        let prefix = format!("{}.", candidate.text.to_ascii_lowercase());
        if input
            .state
            .declared_models
            .iter()
            .any(|model| model.starts_with(&prefix))
        {
            return Err(gap(input, "MOS model binning"));
        }
    }
    let model = declared_model(input)?;
    Ok((vec![drain, gate, source, bulk], model))
}

fn parameters(input: &mut Input<'_>, designator: char) -> Result<Vec<ParameterAssignment>> {
    let leading = if designator == 'q' {
        opt(leading_value).parse_next(input)?
    } else {
        None
    };
    let mut parameters: Vec<ParameterAssignment> = repeat(
        0..,
        alt((
            super::flags::instance,
            super::ic::vector,
            scalar_assignment,
            invalid_parameter,
        )),
    )
    .parse_next(input)?;
    if let Some(area) = leading {
        // INP2Q, like INP2D, applies the leading area after INPdevParse.
        parameters.push(assignment("area", area));
    }
    Ok(parameters)
}

fn scalar_assignment(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let designator = input.state.card.designator().expect("Q/M card");
    let token = any
        .verify(move |token: &Token| {
            let key = token.text.to_ascii_lowercase();
            match designator {
                'q' => matches!(
                    key.as_str(),
                    "area" | "areab" | "areac" | "m" | "icvbe" | "icvce" | "temp" | "dtemp"
                ),
                _ => matches!(
                    key.as_str(),
                    "m" | "l"
                        | "w"
                        | "ad"
                        | "as"
                        | "pd"
                        | "ps"
                        | "nrd"
                        | "nrs"
                        | "icvds"
                        | "icvgs"
                        | "icvbs"
                        | "temp"
                        | "dtemp"
                ),
            }
        })
        .parse_next(input)?;
    let (_, value) = cut_err((opt(equals), value)).parse_next(input)?;
    Ok(named(
        &token.text.to_ascii_lowercase(),
        token.location.clone(),
        value,
    ))
}

fn invalid_parameter(input: &mut Input<'_>) -> Result<ParameterAssignment> {
    let token = peek(any).parse_next(input)?;
    if input.state.card.designator() == Some('m')
        && matches!(token.kind, TokenKind::Number(_) | TokenKind::Expression(_))
    {
        return Err(malformed(
            input,
            "no unlabeled parameter permitted on MOSFET",
        ));
    }
    if matches!(
        token.kind,
        TokenKind::Equals | TokenKind::LParen | TokenKind::RParen | TokenKind::Comma
    ) {
        return Err(malformed(
            input,
            "expected a named scalar transistor parameter",
        ));
    }
    Err(gap(
        input,
        "unsupported transistor flags or additional/non-scalar parameters",
    ))
}
