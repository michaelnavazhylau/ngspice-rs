//! Normalized deck writer: [`Netlist`] in, SPICE text out (GitHub #20).
//!
//! The writer serializes the **raw, unevaluated, unflattened** AST. It is not a
//! debug dump (see `spice-rs parse`), not an evaluated/expanded deck (that is
//! elaboration, #15/#18) and not a byte-exact source reproducer. Its contract is
//! *semantic*: re-parsing the output through the production [`Parser`] yields a
//! netlist that is [`semantic_eq`](crate::semantic::semantic_eq) to the input,
//! and writing that netlist again yields the identical text (fixed point).
//!
//! # Normalization contract
//!
//! Lexical:
//! - One logical card per line, no continuation lines, no comments, no blank
//!   lines; `\n` terminators; 2-space indentation per `.subckt` nesting level.
//! - Line 1 is the title, verbatim (it may not contain a line break or end in
//!   whitespace). Identifiers are written as stored (the parser lowercases
//!   them; `gnd` aliasing is already applied, so re-parse with the same
//!   [`Parser`] configuration, in particular `auto_gnd`).
//! - Numeric and expression spellings are preserved exactly as stored
//!   (`1meg`, `5V`, `{ a + b }`); numbers are never re-formatted.
//! - Directives use their canonical names: `.options`, `.include`, `.lib`.
//!
//! Card order: `Netlist::cards`/`Subcircuit::cards` is authoritative. Cards are
//! written in that order (so forward model references, subcircuit placement and
//! directive order are unchanged); the typed vectors are only addressed by the
//! card indexes. Directives are never reordered or hoisted. The root `.end` is
//! written only if the deck had one.
//!
//! Parameter application order: every device/model/subcircuit assignment list is
//! written in stored (application) order, duplicates retained, so that C's
//! "later setter wins" semantics survive. The *position* a value had in the
//! source is not meaningful beyond that order, so values are written by name:
//! - R/C/L: a first scalar of the primary name (`resistance`, ...) on a
//!   model-less instance is written positionally (`r1 a b 1k tc1=1m`);
//!   otherwise every value is a named setter (`rpost a 0 rm resistance=5k
//!   resistance=4k`), which is how C's post-model positional value (applied
//!   last) is kept last. Likewise D/Q's leading area is written as `area=...`
//!   at its stored (last) position, and V/I's leading DC as `dc <value>`.
//! - V/I: `dc v`, `ac mag phase` (explicit defaults written out),
//!   `pulse(...)`/`pwl(...)` in stored order. PULSE/PWL optional fields that
//!   were omitted stay omitted; PWL pairs and PULSE fields are space separated
//!   inside one pair of parentheses (the stored vector text is re-spelled, so
//!   it is excluded from semantic comparison).
//! - D/Q/M: bare flags (`off`), `name=value` scalars, `ic=(a,b[,c])` vectors
//!   with omitted trailing components omitted. An omitted Q substrate stays
//!   omitted.
//! - `.model`: `.model name base(flag name=value ...)`; level spelling is
//!   whatever the `level=` assignment says.
//! - `.subckt name terminals [params: name=value ...]` ... `.ends name`;
//!   X cards are `xname nodes target name=value ...` without a `params:` marker
//!   (the marker is not represented in the AST; both spellings parse the same).
//! - `.param name=text ...`: expression text is preserved (braced expressions as
//!   `{text}`); the writer verifies at write time that the text re-parses to the
//!   stored syntax tree, and that each `.param` card re-parses to the same
//!   assignments, instead of re-printing from the tree. Parentheses are
//!   therefore exactly those of the source, which are always sufficient.
//! - Analysis cards: `.name` plus the stored argument tokens joined with single
//!   spaces (no space around `(`, `)`, `=` or before `,`). A `.tran` `uic` flag
//!   is written after the positional arguments, before any `name=value` options.
//! - `.ic`/`.nodeset`: one card per stored card, `v(node)=value` entries in
//!   order (values keep their literal spelling or braced expression text).
//!
//! # Includes
//!
//! A parsed [`Netlist`] built with `parse_file` contains resolved include/lib
//! content in its typed vectors and `cards`, marked by a non-empty
//! `include_chain`. The writer emits the **directive** (`.include path`,
//! `.lib path section`) and **skips** all cards whose chain is non-empty:
//! inlining would silently flatten source structure, break source-relative path
//! semantics and duplicate library files into every deck. The directive's path
//! is written with its original spelling (quotes included) when that spelling
//! still decodes to the stored `path`; otherwise `path` is written bare when it
//! is a plain word and double-quoted with `\\`/`\"` escapes otherwise. Paths are
//! never rewritten: a written deck must live where its relative includes
//! resolve, and re-parsing it with `parse_file` re-reads the same files.
//! A scope that opens inside an included file but closes outside it (or the
//! reverse) cannot be represented and is an error.
//!
//! # Source locations
//!
//! Locations (file, line, column, spans, joined card text) describe a specific
//! text and always differ on re-parse. Compare with
//! [`semantic_eq`](crate::semantic::semantic_eq)/
//! [`semantic_diff`](crate::semantic::semantic_diff), which ignore them.
//!
//! # Errors
//!
//! [`write_netlist`] returns [`SpiceError::Unsupported`] (with the offending
//! card's location when known) for anything that would not re-parse to the same
//! semantics: unknown designators or parameter names, kinds a card cannot carry,
//! non-contiguous PULSE fields, inconsistent expression text, inconsistent card
//! indexes, names that are not single tokens or would be treated as comments,
//! and scopes crossing include boundaries.

use std::fmt::Write as _;

use spice_core::{AnalysisKind, SourceLoc, SpiceError, SpiceResult, parse_spice_number};

use crate::Parser;
use crate::ast::{
    AnalysisCard, DeviceInstance, GlobalCard, IncludeDirective, ModelCard, Netlist, NodeHintCard,
    NodeHintValue, OptionCard, ParamCard, ParameterAssignment, ParameterKind, PositionedValue,
    ScopedCard, ScopedCardKind, SourceWaveform, Subcircuit,
};
use crate::expr::ParameterExpression;
use crate::semantic::expr_form;
use crate::source::{LogicalLine, parse_deck_text, strip_comment};
use crate::token::{Token, TokenKind, tokenize};

/// Serializes `netlist` as a normalized deck. See the [module documentation](self)
/// for the contract.
///
/// # Errors
///
/// [`SpiceError::Unsupported`] for constructs that cannot be written so that
/// they re-parse to the same semantics.
pub fn write_netlist(netlist: &Netlist) -> SpiceResult<String> {
    if netlist.title.contains(['\n', '\r']) || netlist.title != netlist.title.trim_end() {
        return Err(refuse(
            "the title must be one line without trailing whitespace",
            Some(&netlist.location),
        ));
    }
    let mut writer = Writer { out: String::new() };
    writer.out.push_str(&netlist.title);
    writer.out.push('\n');
    writer.scope(
        &Scope {
            devices: &netlist.devices,
            models: &netlist.models,
            subcircuits: &netlist.subcircuits,
            analyses: &netlist.analyses,
            includes: &netlist.includes,
            params: &netlist.params,
            options: &netlist.options,
            globals: &netlist.globals,
            initial_conditions: &netlist.initial_conditions,
            nodesets: &netlist.nodesets,
            cards: &netlist.cards,
        },
        0,
        false,
    )?;
    Ok(writer.out)
}

fn refuse(what: impl Into<String>, location: Option<&SourceLoc>) -> SpiceError {
    SpiceError::Unsupported {
        feature: format!("netlist writer: {}", what.into()),
        location: location.cloned(),
    }
}

/// Typed vectors and ordered cards of one scope.
struct Scope<'a> {
    devices: &'a [DeviceInstance],
    models: &'a [ModelCard],
    subcircuits: &'a [Subcircuit],
    analyses: &'a [AnalysisCard],
    includes: &'a [IncludeDirective],
    params: &'a [ParamCard],
    options: &'a [OptionCard],
    globals: &'a [GlobalCard],
    initial_conditions: &'a [NodeHintCard],
    nodesets: &'a [NodeHintCard],
    cards: &'a [ScopedCard],
}

struct Writer {
    out: String,
}

fn entry<'a, T>(items: &'a [T], index: usize, what: &str) -> SpiceResult<&'a T> {
    items.get(index).ok_or_else(|| {
        refuse(
            format!("card index {index} is out of range for {what}"),
            None,
        )
    })
}

impl Writer {
    fn line(&mut self, depth: usize, text: &str, location: &SourceLoc) -> SpiceResult<()> {
        if text.contains(['\n', '\r']) || strip_comment(text, false) != text {
            return Err(refuse(
                format!("card would not survive comment/line handling: {text:?}"),
                Some(location),
            ));
        }
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
        Ok(())
    }

    fn scope(&mut self, scope: &Scope<'_>, depth: usize, body: bool) -> SpiceResult<()> {
        let mut used = [0usize; 10];
        for card in scope.cards {
            // Cards read from an include/lib file are represented by their
            // directive; they are counted but not written.
            let skip = !card.include_chain.is_empty();
            let location = &card.source.location;
            match card.kind {
                ScopedCardKind::Device(i) => {
                    used[0] += 1;
                    let device = entry(scope.devices, i, "devices")?;
                    if !skip {
                        self.device(device, depth)?;
                    }
                }
                ScopedCardKind::Model(i) => {
                    used[1] += 1;
                    let model = entry(scope.models, i, "models")?;
                    if !skip {
                        self.model(model, depth)?;
                    }
                }
                ScopedCardKind::Subcircuit(i) => {
                    used[2] += 1;
                    let sub = entry(scope.subcircuits, i, "subcircuits")?;
                    check_closed(sub, &card.include_chain)?;
                    if !skip {
                        self.subcircuit(sub, depth)?;
                    }
                }
                ScopedCardKind::Analysis(i) => {
                    used[3] += 1;
                    let analysis = entry(scope.analyses, i, "analyses")?;
                    if !skip {
                        self.analysis(analysis, depth)?;
                    }
                }
                ScopedCardKind::Include(i) => {
                    used[4] += 1;
                    let include = entry(scope.includes, i, "includes")?;
                    if !skip {
                        self.include(include, depth)?;
                    }
                }
                ScopedCardKind::Param(i) => {
                    used[5] += 1;
                    let param = entry(scope.params, i, "params")?;
                    if !skip {
                        self.param(param, depth)?;
                    }
                }
                ScopedCardKind::Options(i) => {
                    used[6] += 1;
                    let options = entry(scope.options, i, "options")?;
                    if !skip {
                        self.options(options, depth)?;
                    }
                }
                ScopedCardKind::Global(i) => {
                    used[7] += 1;
                    let globals = entry(scope.globals, i, "globals")?;
                    if !skip {
                        self.global(globals, depth)?;
                    }
                }
                ScopedCardKind::InitialCondition(i) => {
                    used[8] += 1;
                    let card_ = entry(scope.initial_conditions, i, "initial conditions")?;
                    if !skip {
                        self.hints(".ic", card_, depth)?;
                    }
                }
                ScopedCardKind::Nodeset(i) => {
                    used[9] += 1;
                    let card_ = entry(scope.nodesets, i, "nodesets")?;
                    if !skip {
                        self.hints(".nodeset", card_, depth)?;
                    }
                }
                // `.save`/`.print` requests (`ast::OutputCards`) and
                // `.measure` requests (`ParsedDeck::measurements`) are returned
                // beside the netlist, so their typed index lives outside the
                // netlist and the card is reproduced from its own spelling.
                ScopedCardKind::Output | ScopedCardKind::Measure => {
                    if !skip {
                        self.line(depth, card.source.raw.trim(), location)?;
                    }
                }
                // The closing `.ends` is written by `subcircuit`; it was
                // verified there to be the final card.
                ScopedCardKind::Ends => {
                    if !body {
                        return Err(refuse("`.ends` outside a subcircuit", Some(location)));
                    }
                }
                ScopedCardKind::End => {
                    if body {
                        return Err(refuse("`.end` inside a subcircuit", Some(location)));
                    }
                    if !skip {
                        self.line(depth, ".end", location)?;
                    }
                }
            }
        }
        let lengths = [
            scope.devices.len(),
            scope.models.len(),
            scope.subcircuits.len(),
            scope.analyses.len(),
            scope.includes.len(),
            scope.params.len(),
            scope.options.len(),
            scope.globals.len(),
            scope.initial_conditions.len(),
            scope.nodesets.len(),
        ];
        if used != lengths {
            return Err(refuse(
                "the ordered cards do not reference every typed entry exactly once",
                None,
            ));
        }
        Ok(())
    }

    fn subcircuit(&mut self, sub: &Subcircuit, depth: usize) -> SpiceResult<()> {
        let location = &sub.location;
        let mut parts = vec![".subckt".to_owned(), node(&sub.name, location)?];
        for terminal in &sub.terminals {
            parts.push(node(terminal, location)?);
        }
        if !sub.parameters.is_empty() {
            parts.push("params:".to_owned());
            for parameter in &sub.parameters {
                parts.push(named_value(parameter, true)?);
            }
        }
        self.line(depth, &parts.join(" "), location)?;
        self.scope(
            &Scope {
                devices: &sub.devices,
                models: &sub.models,
                subcircuits: &sub.subcircuits,
                analyses: &sub.analyses,
                includes: &sub.includes,
                params: &sub.params,
                options: &[],
                globals: &[],
                initial_conditions: &[],
                nodesets: &[],
                cards: &sub.cards,
            },
            depth + 1,
            true,
        )?;
        self.line(depth, &format!(".ends {}", sub.name), &sub.end_location)
    }

    fn analysis(&mut self, card: &AnalysisCard, depth: usize) -> SpiceResult<()> {
        let mut text = format!(".{}", card.kind.as_str());
        let mut arguments: Vec<&str> = card.arguments.iter().map(String::as_str).collect();
        if card.uic {
            if card.kind != AnalysisKind::Transient {
                return Err(refuse("uic is only valid on .tran", Some(&card.location)));
            }
            // The flag follows the positional arguments and precedes any
            // `name=value` driver options, where the parser accepts it.
            let mut at = 0;
            while at < arguments.len() && arguments.get(at + 1) != Some(&"=") {
                at += 1;
            }
            arguments.insert(at, "uic");
        }
        let mut previous: Option<&str> = None;
        for argument in &arguments {
            let glued =
                matches!(*argument, "(" | ")" | "," | "=") || matches!(previous, Some("(" | "="));
            if !glued {
                text.push(' ');
            }
            text.push_str(argument);
            previous = Some(argument);
        }
        // The arguments must re-tokenize to exactly the stored tokens.
        let tokens = lex(&text, &card.location)?;
        if tokens
            .iter()
            .skip(1)
            .map(|t| t.text.as_str())
            .ne(arguments.iter().copied())
        {
            return Err(refuse(
                "analysis arguments do not re-tokenize to the stored tokens",
                Some(&card.location),
            ));
        }
        self.line(depth, &text, &card.location)
    }

    fn include(&mut self, include: &IncludeDirective, depth: usize) -> SpiceResult<()> {
        let location = &include.location;
        let path = match single_token(&include.path_spelling, location) {
            Some(token) if path_token(&token) == Some(include.path.as_str()) => {
                include.path_spelling.clone()
            }
            _ => quote_path(&include.path, location)?,
        };
        let text = match &include.section {
            Some(section) => format!(".lib {path} {}", quote_path(section, location)?),
            None => format!(".include {path}"),
        };
        if include.path.is_empty() {
            return Err(refuse("empty include path", Some(location)));
        }
        self.line(depth, &text, location)
    }

    fn param(&mut self, card: &ParamCard, depth: usize) -> SpiceResult<()> {
        let location = &card.location;
        let mut text = ".param".to_owned();
        for assignment in &card.assignments {
            check_expression(&assignment.expression, location)?;
            let value = if assignment.expression.braced {
                format!("{{{}}}", assignment.expression.text)
            } else {
                assignment.expression.text.clone()
            };
            let _ = write!(text, " {}={value}", assignment.name);
        }
        if card.assignments.is_empty() {
            return Err(refuse("empty .param card", Some(location)));
        }
        // The whole card, including the extent of each unbraced value, must
        // re-parse to the same assignments.
        let deck = parse_deck_text(location.path(), &format!("t\n{text}\n"));
        let reparsed = Parser::new().parse_deck(&deck).map_err(|error| {
            refuse(
                format!("`.param` card does not re-parse: {error}"),
                Some(location),
            )
        })?;
        let same = reparsed.params.len() == 1
            && reparsed.params[0].assignments.len() == card.assignments.len()
            && reparsed.params[0]
                .assignments
                .iter()
                .zip(&card.assignments)
                .all(|(a, b)| {
                    a.name == b.name
                        && a.expression.braced == b.expression.braced
                        && a.expression.text == b.expression.text
                        && expr_form(&a.expression.root) == expr_form(&b.expression.root)
                });
        if !same {
            return Err(refuse(
                "`.param` card does not re-parse to the same assignments",
                Some(location),
            ));
        }
        self.line(depth, &text, location)
    }

    fn options(&mut self, card: &OptionCard, depth: usize) -> SpiceResult<()> {
        let location = &card.location;
        if card.settings.is_empty() {
            return Err(refuse("empty .options card", Some(location)));
        }
        let mut text = ".options".to_owned();
        for setting in &card.settings {
            if !matches!(
                single_token(&setting.name, location),
                Some(Token {
                    kind: TokenKind::Word,
                    ..
                })
            ) {
                return Err(refuse(
                    format!("option name {:?} is not a plain word", setting.name),
                    Some(location),
                ));
            }
            text.push(' ');
            text.push_str(&setting.name);
            if let Some(value) = &setting.value {
                let ok = match single_token(&value.text, location) {
                    Some(Token {
                        kind: TokenKind::Word,
                        ..
                    }) => true,
                    Some(Token {
                        kind: TokenKind::Number(v),
                        ..
                    }) => v.is_finite(),
                    _ => false,
                };
                if !ok {
                    return Err(refuse(
                        format!(
                            "option value {:?} is not a word or finite number",
                            value.text
                        ),
                        Some(location),
                    ));
                }
                text.push('=');
                text.push_str(&value.text);
            }
        }
        self.line(depth, &text, location)
    }

    fn hints(&mut self, name: &str, card: &NodeHintCard, depth: usize) -> SpiceResult<()> {
        if card.entries.is_empty() {
            return Err(refuse(format!("empty {name} card"), Some(&card.location)));
        }
        let mut text = name.to_owned();
        for hint in &card.entries {
            let (kind, value) = match &hint.value {
                NodeHintValue::Literal { text, .. } => (ParameterKind::Scalar, text.clone()),
                NodeHintValue::Expression(expression) => {
                    let spelled = if expression.braced {
                        format!("{{{}}}", expression.text)
                    } else {
                        expression.text.clone()
                    };
                    (ParameterKind::Expression(expression.clone()), spelled)
                }
            };
            let value = value_text(
                &ParameterAssignment {
                    name: format!("v({})", hint.node),
                    value,
                    kind,
                    location: hint.value_location.clone(),
                },
                false,
            )?;
            let _ = write!(text, " v({})={value}", node(&hint.node, &hint.location)?);
        }
        self.line(depth, &text, &card.location)
    }

    fn global(&mut self, card: &GlobalCard, depth: usize) -> SpiceResult<()> {
        if card.nodes.is_empty() {
            return Err(refuse("empty .global card", Some(&card.location)));
        }
        let mut text = ".global".to_owned();
        for global in &card.nodes {
            text.push(' ');
            text.push_str(&node(&global.name, &card.location)?);
        }
        self.line(depth, &text, &card.location)
    }

    fn model(&mut self, model: &ModelCard, depth: usize) -> SpiceResult<()> {
        let location = &model.location;
        let base = model.base.as_str();
        if !matches!(
            base,
            "d" | "npn" | "pnp" | "nmos" | "pmos" | "r" | "res" | "c" | "l"
        ) {
            return Err(refuse(
                format!("model type {base:?} is outside the supported syntax"),
                Some(location),
            ));
        }
        let mut items = Vec::new();
        for parameter in &model.parameters {
            match &parameter.kind {
                ParameterKind::Flag => {
                    let allowed: &[&str] = match base {
                        "d" => &["d"],
                        "npn" | "pnp" => &["npn", "pnp"],
                        "nmos" | "pmos" => &["nmos", "pmos"],
                        _ => &[],
                    };
                    if !allowed.contains(&parameter.name.as_str()) || !parameter.value.is_empty() {
                        return Err(refuse(
                            format!("flag {:?} is not valid for a {base} model", parameter.name),
                            Some(location),
                        ));
                    }
                    items.push(parameter.name.clone());
                }
                ParameterKind::Expression(_) if parameter.name == "level" => {
                    return Err(refuse("expression-valued model level", Some(location)));
                }
                _ => items.push(named_value(parameter, false)?),
            }
        }
        let mut text = format!(".model {} {base}", node(&model.name, location)?);
        if !items.is_empty() {
            let _ = write!(text, "({})", items.join(" "));
        }
        self.line(depth, &text, location)
    }

    fn device(&mut self, device: &DeviceInstance, depth: usize) -> SpiceResult<()> {
        let location = &device.location;
        let designator = device.designator;
        if !device.name.to_ascii_lowercase().starts_with(designator) {
            return Err(refuse(
                format!(
                    "instance {:?} does not start with '{designator}'",
                    device.name
                ),
                Some(location),
            ));
        }
        let mut parts = vec![node(&device.name, location)?];
        let count = device.nodes.len();
        let count_ok = match designator {
            'r' | 'c' | 'l' | 'v' | 'i' | 'd' => count == 2,
            'q' => count == 3 || count == 4,
            'm' => count == 4,
            'x' => true,
            _ => {
                return Err(refuse(
                    format!("device designator '{designator}' has no writer"),
                    Some(location),
                ));
            }
        };
        if !count_ok {
            return Err(refuse(
                format!("'{designator}' instance with {count} nodes"),
                Some(location),
            ));
        }
        for n in &device.nodes {
            parts.push(node(n, location)?);
        }
        match (designator, &device.model) {
            ('v' | 'i', Some(_)) => {
                return Err(refuse("source with a model", Some(location)));
            }
            ('d' | 'q' | 'm' | 'x', None) => {
                return Err(refuse("device without a model/target", Some(location)));
            }
            (_, Some(model)) => {
                if designator == 'q'
                    && (!model.bytes().any(|b| b.is_ascii_alphabetic())
                        || parse_spice_number(model).is_some())
                {
                    return Err(refuse(
                        "BJT model name must be a non-numeric word with a letter",
                        Some(location),
                    ));
                }
                if designator == 'm' && parse_spice_number(model).is_some() {
                    return Err(refuse("numeric-looking MOS model name", Some(location)));
                }
                parts.push(node(model, location)?);
            }
            (_, None) => {}
        }
        match designator {
            'r' | 'c' | 'l' => passive_parameters(device, &mut parts)?,
            'v' | 'i' => source_parameters(device, &mut parts)?,
            'x' => {
                for parameter in &device.parameters {
                    parts.push(named_value(parameter, true)?);
                }
            }
            _ => transistor_parameters(device, &mut parts)?,
        }
        self.line(depth, &parts.join(" "), location)
    }
}

/// An opening card in `chain` must be closed by a final `.ends` in the same
/// file (same include chain length), else the scope crosses an include boundary.
fn check_closed(sub: &Subcircuit, chain: &[SourceLoc]) -> SpiceResult<()> {
    match sub.cards.last() {
        Some(last)
            if last.kind == ScopedCardKind::Ends && last.include_chain.len() == chain.len() =>
        {
            Ok(())
        }
        _ => Err(refuse(
            format!(
                "subcircuit '{}' is not closed by `.ends` in the file that opened it",
                sub.name
            ),
            Some(&sub.location),
        )),
    }
}

fn lex(text: &str, location: &SourceLoc) -> SpiceResult<Vec<Token>> {
    tokenize(&LogicalLine {
        location: location.clone(),
        end_line: location.line,
        continuations: 0,
        text: text.to_owned(),
    })
    .map_err(|error| {
        refuse(
            format!("text {text:?} does not tokenize: {error}"),
            Some(location),
        )
    })
}

/// `text` as one token whose spelling is exactly `text`, safe from comment
/// stripping.
fn single_token(text: &str, location: &SourceLoc) -> Option<Token> {
    if text.is_empty() || strip_comment(text, false) != text {
        return None;
    }
    let mut tokens = lex(text, location).ok()?;
    (tokens.len() == 1 && tokens[0].text == text).then(|| tokens.remove(0))
}

fn path_token(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Quoted(decoded) => Some(decoded),
        TokenKind::Word | TokenKind::Number(_) => Some(&token.text),
        _ => None,
    }
}

fn quote_path(path: &str, location: &SourceLoc) -> SpiceResult<String> {
    if path.is_empty() || path.contains(['\n', '\r']) {
        return Err(refuse(
            "path or section is empty or contains a line break",
            Some(location),
        ));
    }
    if let Some(token) = single_token(path, location)
        && path_token(&token) == Some(path)
        && !matches!(token.kind, TokenKind::Quoted(_))
    {
        return Ok(path.to_owned());
    }
    let mut quoted = String::from("\"");
    for character in path.chars() {
        if matches!(character, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    Ok(quoted)
}

/// A node/instance/model name: one word or number token.
fn node(name: &str, location: &SourceLoc) -> SpiceResult<String> {
    match single_token(name, location) {
        Some(Token {
            kind: TokenKind::Word | TokenKind::Number(_),
            ..
        }) => Ok(name.to_owned()),
        _ => Err(refuse(
            format!("{name:?} is not a single name token"),
            Some(location),
        )),
    }
}

/// The expression text must re-parse to the stored tree, so writing the text
/// preserves the grouping (parentheses) of the source.
fn check_expression(expression: &ParameterExpression, location: &SourceLoc) -> SpiceResult<()> {
    let reparsed = Parser::new()
        .parse_expression(&expression.text, location)
        .map_err(|error| {
            refuse(
                format!(
                    "expression {:?} does not re-parse: {error}",
                    expression.text
                ),
                Some(location),
            )
        })?;
    if expr_form(&reparsed.root) != expr_form(&expression.root) {
        return Err(refuse(
            format!(
                "expression text {:?} does not match its stored syntax tree",
                expression.text
            ),
            Some(location),
        ));
    }
    Ok(())
}

/// The spelling of a scalar-or-expression value (`text_ok` also allows a
/// single `Textual` token, used at `.subckt`/X parameter sites).
fn value_text(parameter: &ParameterAssignment, text_ok: bool) -> SpiceResult<String> {
    let location = &parameter.location;
    let bad = |what: &str| {
        refuse(
            format!("parameter {:?}: {what}", parameter.name),
            Some(location),
        )
    };
    let token = single_token(&parameter.value, location);
    match &parameter.kind {
        ParameterKind::Scalar => match token {
            Some(Token {
                kind: TokenKind::Number(v),
                ..
            }) if v.is_finite() => {}
            _ => return Err(bad("scalar value is not one finite numeric token")),
        },
        ParameterKind::Textual if text_ok => {
            if !matches!(
                token,
                Some(Token {
                    kind: TokenKind::Word | TokenKind::Quoted(_),
                    ..
                })
            ) {
                return Err(bad("textual value is not one word or quoted token"));
            }
        }
        ParameterKind::Expression(expression) => {
            check_expression(expression, location)?;
            let consistent = if expression.braced {
                parameter.value == format!("{{{}}}", expression.text)
            } else {
                parameter.value == expression.text
            };
            let kind_ok = matches!(
                (&token, expression.braced),
                (
                    Some(Token {
                        kind: TokenKind::Expression(_),
                        ..
                    }),
                    true
                ) | (
                    Some(Token {
                        kind: TokenKind::Word,
                        ..
                    }),
                    false
                )
            );
            if !consistent || !kind_ok {
                return Err(bad("value text does not match its expression"));
            }
        }
        _ => return Err(bad("value kind is not allowed here")),
    }
    Ok(parameter.value.clone())
}

fn named_value(parameter: &ParameterAssignment, text_ok: bool) -> SpiceResult<String> {
    let location = &parameter.location;
    if !matches!(
        single_token(&parameter.name, location),
        Some(Token {
            kind: TokenKind::Word,
            ..
        })
    ) {
        return Err(refuse(
            format!("parameter name {:?} is not a plain word", parameter.name),
            Some(location),
        ));
    }
    Ok(format!(
        "{}={}",
        parameter.name,
        value_text(parameter, text_ok)?
    ))
}

fn positioned_number(value: &PositionedValue, location: &SourceLoc) -> SpiceResult<String> {
    match single_token(&value.text, location) {
        Some(Token {
            kind: TokenKind::Number(v),
            ..
        }) if v.is_finite() => Ok(value.text.clone()),
        _ => Err(refuse(
            format!(
                "vector component {:?} is not a finite numeric literal",
                value.text
            ),
            Some(location),
        )),
    }
}

fn passive_parameters(device: &DeviceInstance, parts: &mut Vec<String>) -> SpiceResult<()> {
    let designator = device.designator;
    let primary = match designator {
        'r' => "resistance",
        'c' => "capacitance",
        _ => "inductance",
    };
    let allowed = |name: &str| match (designator, name) {
        (_, "temp" | "dtemp" | "m" | "tc1" | "tc2" | "scale") => true,
        ('r' | 'c', "w" | "l" | "bv_max") => true,
        ('r', "ac" | "tc" | "tce") => true,
        ('c' | 'l', "ic") => true,
        ('l', "nt") => true,
        (_, name) => name == primary,
    };
    for parameter in &device.parameters {
        if !allowed(&parameter.name) || matches!(parameter.kind, ParameterKind::Textual) {
            return Err(refuse(
                format!("parameter {:?} on '{designator}' instance", parameter.name),
                Some(&parameter.location),
            ));
        }
    }
    if device.model.is_none() && !device.parameters.iter().any(|p| p.name == primary) {
        return Err(refuse(
            format!("model-less '{designator}' instance without {primary}"),
            Some(&device.location),
        ));
    }
    let positional = device.model.is_none()
        && device
            .parameters
            .first()
            .is_some_and(|parameter| parameter.name == primary);
    for (index, parameter) in device.parameters.iter().enumerate() {
        if positional && index == 0 {
            parts.push(value_text(parameter, false)?);
        } else {
            parts.push(named_value(parameter, false)?);
        }
    }
    Ok(())
}

fn source_parameters(device: &DeviceInstance, parts: &mut Vec<String>) -> SpiceResult<()> {
    let mut parameters = device.parameters.iter().peekable();
    while let Some(parameter) = parameters.next() {
        let location = &parameter.location;
        match (&parameter.kind, parameter.name.as_str()) {
            (ParameterKind::Scalar | ParameterKind::Expression(_), "dc") => {
                parts.push("dc".to_owned());
                parts.push(value_text(parameter, false)?);
            }
            (ParameterKind::Scalar | ParameterKind::Expression(_), "acmag") => {
                let Some(phase) = parameters.next_if(|next| next.name == "acphase") else {
                    return Err(refuse("acmag without acphase", Some(location)));
                };
                parts.push("ac".to_owned());
                parts.push(value_text(parameter, false)?);
                parts.push(value_text(phase, false)?);
            }
            (ParameterKind::Waveform(waveform), name) => {
                parts.push(waveform_text(waveform, name, location)?);
            }
            _ => {
                return Err(refuse(
                    format!("source parameter {:?}", parameter.name),
                    Some(location),
                ));
            }
        }
    }
    Ok(())
}

fn waveform_text(
    waveform: &SourceWaveform,
    name: &str,
    location: &SourceLoc,
) -> SpiceResult<String> {
    let (keyword, fields) = match waveform {
        SourceWaveform::Pulse(pulse) => {
            let optional = [
                &pulse.delay,
                &pulse.rise,
                &pulse.fall,
                &pulse.width,
                &pulse.period,
            ];
            let mut fields = vec![&pulse.initial, &pulse.pulsed];
            let mut omitted = false;
            for value in optional {
                match value {
                    Some(value) if !omitted => fields.push(value),
                    Some(_) => {
                        return Err(refuse(
                            "PULSE field present after an omitted field",
                            Some(location),
                        ));
                    }
                    None => omitted = true,
                }
            }
            ("pulse", fields)
        }
        SourceWaveform::Pwl(points) => {
            if points.is_empty() || points.len() > 2048 {
                return Err(refuse("PWL needs 1 to 2048 points", Some(location)));
            }
            (
                "pwl",
                points.iter().flat_map(|p| [&p.time, &p.value]).collect(),
            )
        }
    };
    if name != keyword {
        return Err(refuse(
            format!("waveform setter named {name:?} holds a {keyword}"),
            Some(location),
        ));
    }
    let mut texts = Vec::new();
    for field in fields {
        texts.push(positioned_number(field, location)?);
    }
    Ok(format!("{keyword}({})", texts.join(" ")))
}

fn transistor_parameters(device: &DeviceInstance, parts: &mut Vec<String>) -> SpiceResult<()> {
    let designator = device.designator;
    let scalars: &[&str] = match designator {
        'd' => &[
            "pj", "area", "w", "l", "m", "ic", "temp", "dtemp", "lm", "lp", "wm", "wp",
        ],
        'q' => &[
            "area", "areab", "areac", "m", "icvbe", "icvce", "temp", "dtemp",
        ],
        _ => &[
            "m", "l", "w", "ad", "as", "pd", "ps", "nrd", "nrs", "icvds", "icvgs", "icvbs", "temp",
            "dtemp",
        ],
    };
    for parameter in &device.parameters {
        let location = &parameter.location;
        match &parameter.kind {
            ParameterKind::Flag if parameter.name == "off" && parameter.value.is_empty() => {
                parts.push("off".to_owned());
            }
            ParameterKind::InitialConditions(components) if parameter.name == "ic" => {
                let names: &[&str] = match designator {
                    'q' => &["icvbe", "icvce"],
                    'm' => &["icvds", "icvgs", "icvbs"],
                    _ => &[],
                };
                if components.is_empty()
                    || components.len() > names.len()
                    || components.iter().zip(names).any(|(c, n)| c.name != *n)
                {
                    return Err(refuse("malformed ic vector", Some(location)));
                }
                let mut texts = Vec::new();
                for component in components {
                    texts.push(positioned_number(&component.value, location)?);
                }
                parts.push(format!("ic=({})", texts.join(",")));
            }
            ParameterKind::Scalar | ParameterKind::Expression(_)
                if scalars.contains(&parameter.name.as_str()) =>
            {
                parts.push(named_value(parameter, false)?);
            }
            _ => {
                return Err(refuse(
                    format!("parameter {:?} on '{designator}' instance", parameter.name),
                    Some(location),
                ));
            }
        }
    }
    Ok(())
}
