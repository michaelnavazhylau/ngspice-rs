//! Normalized deck writer: [`Netlist`] in, SPICE text out (GitHub #20).
//!
//! The writer serializes the **raw, unevaluated, unflattened** AST. It is not a
//! debug dump (see `spice-rs parse`), not an evaluated/expanded deck (that is
//! elaboration, #15/#18) and not a byte-exact source reproducer. Its contract is
//! *semantic*: re-parsing the output through the production [`Parser`] yields a
//! netlist that is [`semantic_eq`](crate::netlist::semantic::semantic_eq) to the input,
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
//!   `pulse(...)`/`pwl(...)`/`sin(...)` (or `sine`)/`exp(...)`/`sffm(...)`/
//!   `am(...)` and PWL `td=`/`r=` in stored order. Optional fields that
//!   were omitted stay omitted; PWL pairs and other fields are space separated
//!   inside one pair of parentheses (the stored vector text is re-spelled, so
//!   it is excluded from semantic comparison).
//! - E/F/G/H (linear gain forms): `e1 n+ n- nc+ nc- <gain>`,
//!   `f1 n+ n- vname <gain>`; parentheses and the HSPICE `vcvs`-style keyword
//!   are not written. A gain stored last after `m=` setters (C's leading value,
//!   applied after the named setters) is written positionally before them; a
//!   gain stored first is written as `gain=`. Other orders are refused.
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
//! - `.func name(p1,p2) body`: the body keeps its delimiters (`{...}`, `'...'`
//!   or bare) and text, verified to re-parse like `.param`.
//! - Single-quoted expressions (`'a*2'`) keep their quotes everywhere; C's
//!   `inp_change_quotes()` makes them identical to braces.
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
//! [`semantic_eq`](crate::netlist::semantic::semantic_eq)/
//! [`semantic_diff`](crate::netlist::semantic::semantic_diff), which ignore them.
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

use crate::primitives::{AnalysisKind, SourceLoc, SpiceError, SpiceResult, parse_spice_number};

use crate::netlist::Parser;
use crate::netlist::ast::{
    AnalysisCard, DeviceInstance, FuncCard, FuncSpelling, GlobalCard, IncludeDirective, ModelCard,
    Netlist, NodeHintCard, NodeHintValue, OptionCard, ParamCard, ParameterAssignment,
    ParameterKind, PositionedValue, ScopedCard, ScopedCardKind, SourceFunction, SourceWaveform,
    Subcircuit,
};
use crate::netlist::expr::ParameterExpression;
use crate::netlist::semantic::{bexpr_form, expr_form};
use crate::netlist::source::{LogicalLine, parse_deck_text, strip_comment};
use crate::netlist::token::{Token, TokenKind, tokenize};

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
            functions: &netlist.functions,
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
    functions: &'a [FuncCard],
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
        let mut used = [0usize; 11];
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
                ScopedCardKind::Func(i) => {
                    used[10] += 1;
                    let function = entry(scope.functions, i, "functions")?;
                    if !skip {
                        self.func(function, depth)?;
                    }
                }
                ScopedCardKind::Options(i) => {
                    used[6] += 1;
                    let options = entry(scope.options, i, "options")?;
                    if !skip {
                        if card
                            .source
                            .tokens
                            .first()
                            .is_some_and(|token| token.text.eq_ignore_ascii_case("set"))
                        {
                            self.settings(options, depth, "set")?;
                        } else {
                            self.options(options, depth)?;
                        }
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
                // `.save`/`.print` requests (`ast::OutputCards`), `.measure`
                // requests (`ParsedDeck::measurements`) and `.four` requests
                // (`ParsedDeck::fourier`) are returned beside the netlist, so
                // their typed index lives outside the netlist and the card is
                // reproduced from its own spelling.
                ScopedCardKind::Output | ScopedCardKind::Measure | ScopedCardKind::Fourier => {
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
            scope.functions.len(),
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
                functions: &sub.functions,
                options: &sub.options,
                globals: &sub.globals,
                initial_conditions: &sub.initial_conditions,
                nodesets: &sub.nodesets,
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
            let value = assignment.expression.spelling();
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
                        && a.expression.quoted == b.expression.quoted
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

    /// `.func name(p1,p2) body`, the body in its original delimiters. Like
    /// `.param`, the card is verified to re-parse to the same definition.
    fn func(&mut self, card: &FuncCard, depth: usize) -> SpiceResult<()> {
        let location = &card.location;
        // A formal named like a built-in only parses inside its own card;
        // the whole-card re-parse below still compares the tree.
        let shadows = card.parameters.iter().any(|p| {
            crate::netlist::expr::Function::from_name(&p.name).is_some()
                || crate::netlist::expr::EXCLUDED_FUNCTIONS.contains(&p.name.as_str())
        });
        if !shadows {
            check_expression(&card.body, location)?;
        }
        let formals: Vec<&str> = card.parameters.iter().map(|p| p.name.as_str()).collect();
        let text = match card.spelling {
            FuncSpelling::Func => format!(
                ".func {}({}) {}",
                card.name,
                formals.join(","),
                card.body.spelling()
            ),
            FuncSpelling::Param => format!(
                ".param {}({})={}",
                card.name,
                formals.join(","),
                card.body.spelling()
            ),
        };
        let deck = parse_deck_text(location.path(), &format!("t\n{text}\n"));
        let reparsed = Parser::new().parse_deck(&deck).map_err(|error| {
            refuse(
                format!("`.func` card does not re-parse: {error}"),
                Some(location),
            )
        })?;
        let same = reparsed.functions.len() == 1 && {
            let other = &reparsed.functions[0];
            other.name == card.name
                && other.spelling == card.spelling
                && other
                    .parameters
                    .iter()
                    .map(|p| p.name.as_str())
                    .eq(formals.iter().copied())
                && other.body.braced == card.body.braced
                && other.body.quoted == card.body.quoted
                && other.body.text == card.body.text
                && expr_form(&other.body.root) == expr_form(&card.body.root)
        };
        if !same {
            return Err(refuse(
                "`.func` card does not re-parse to the same definition",
                Some(location),
            ));
        }
        self.line(depth, &text, location)
    }

    fn options(&mut self, card: &OptionCard, depth: usize) -> SpiceResult<()> {
        self.settings(card, depth, ".options")
    }

    fn settings(&mut self, card: &OptionCard, depth: usize, prefix: &str) -> SpiceResult<()> {
        let location = &card.location;
        if card.settings.is_empty() {
            return Err(refuse("empty .options card", Some(location)));
        }
        let mut text = prefix.to_owned();
        for setting in &card.settings {
            if prefix == "set"
                && !matches!(
                    setting.name.as_str(),
                    "sqrnoise"
                        | "ngbehavior"
                        | "filetype"
                        | "nfreqs"
                        | "nperiods"
                        | "polydegree"
                        | "fourgridsize"
                )
            {
                return Err(refuse("unsupported front-end setting", Some(location)));
            }
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
            if setting.expression.is_some() && setting.value.is_none() {
                return Err(refuse(
                    format!(
                        "option {:?} has an expression but no value text",
                        setting.name
                    ),
                    Some(location),
                ));
            }
            if let Some(value) = &setting.value {
                let expression = setting.expression.is_some();
                let ok = match single_token(&value.text, location) {
                    Some(Token {
                        kind: TokenKind::Word,
                        ..
                    }) => !expression,
                    Some(Token {
                        kind: TokenKind::Number(v),
                        ..
                    }) => v.is_finite() && !expression,
                    Some(Token {
                        kind: TokenKind::Expression(_) | TokenKind::Quoted(_),
                        ..
                    }) => expression,
                    _ => false,
                };
                if !ok {
                    return Err(refuse(
                        format!(
                            "option value {:?} is not a word, finite number or parsed \
                             expression",
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
                NodeHintValue::Expression(expression) => (
                    ParameterKind::Expression(expression.clone()),
                    expression.spelling(),
                ),
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
            "d" | "npn" | "pnp" | "nmos" | "pmos" | "r" | "res" | "c" | "l" | "sw" | "csw"
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
                        "sw" => &["sw"],
                        "csw" => &["csw"],
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
        if designator == 'b'
            || (matches!(designator, 'e' | 'f' | 'g' | 'h')
                && device
                    .parameters
                    .iter()
                    .any(|p| matches!(p.name.as_str(), "value" | "table" | "poly")))
        {
            let line = behavioural_card(device)?;
            return self.line(depth, &line, location);
        }
        let mut parts = vec![node(&device.name, location)?];
        let count = device.nodes.len();
        let count_ok = match designator {
            'r' | 'c' | 'l' | 'v' | 'i' | 'd' | 'f' | 'h' | 'w' => count == 2,
            'q' => count == 3 || count == 4,
            'm' | 'e' | 'g' | 's' | 't' => count == 4,
            'x' => true,
            'k' => count == 0,
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
        let mut setters = device.parameters.as_slice();
        if designator == 'w' {
            // INP2W reads the controlling source before the model name.
            match setters.split_first() {
                Some((control, rest))
                    if control.name == "control" && control.kind == ParameterKind::Instance =>
                {
                    parts.push(node(&control.value, &control.location)?);
                    setters = rest;
                }
                _ => {
                    return Err(refuse(
                        "'w' instance without a leading controlling source",
                        Some(location),
                    ));
                }
            }
        }
        match (designator, &device.model) {
            ('v' | 'i' | 'e' | 'f' | 'g' | 'h' | 'k' | 't', Some(_)) => {
                return Err(refuse("source with a model", Some(location)));
            }
            ('d' | 'q' | 'm' | 'x' | 's' | 'w', None) => {
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
            'e' | 'f' | 'g' | 'h' => controlled_parameters(device, &mut parts)?,
            'k' => mutual_parameters(device, &mut parts)?,
            's' | 'w' => switch_parameters(setters, designator, &mut parts)?,
            't' => tline_parameters(device, &mut parts)?,
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
            let consistent = parameter.value == expression.spelling();
            let kind_ok = matches!(
                (&token, expression.braced, expression.quoted),
                (
                    Some(Token {
                        kind: TokenKind::Expression(_),
                        ..
                    }),
                    true,
                    false
                ) | (
                    Some(Token {
                        kind: TokenKind::Quoted(_),
                        ..
                    }),
                    true,
                    true
                ) | (
                    Some(Token {
                        kind: TokenKind::Word,
                        ..
                    }),
                    false,
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
        ('r', "ac" | "tc" | "tce" | "noisy") => true,
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

/// S/W (`parser/switch.rs`): only bare `on`/`off` flags follow the model, in
/// their written order; W's `control` was already written before the model.
fn switch_parameters(
    setters: &[ParameterAssignment],
    designator: char,
    parts: &mut Vec<String>,
) -> SpiceResult<()> {
    for parameter in setters {
        if parameter.kind != ParameterKind::Flag
            || !matches!(parameter.name.as_str(), "on" | "off")
            || !parameter.value.is_empty()
        {
            return Err(refuse(
                format!("parameter {:?} on '{designator}' instance", parameter.name),
                Some(&parameter.location),
            ));
        }
        parts.push(parameter.name.clone());
    }
    Ok(())
}

/// T (`parser/tline.rs`): `TRApTable` scalars and the `ic` vector, in order.
fn tline_parameters(device: &DeviceInstance, parts: &mut Vec<String>) -> SpiceResult<()> {
    const SCALARS: [&str; 11] = [
        "z0", "zo", "td", "f", "nl", "v1", "v2", "i1", "i2", "rel", "abs",
    ];
    const IC: [&str; 4] = ["v1", "i1", "v2", "i2"];
    for parameter in &device.parameters {
        let location = &parameter.location;
        match &parameter.kind {
            ParameterKind::InitialConditions(components) if parameter.name == "ic" => {
                if components.is_empty()
                    || components.len() > IC.len()
                    || components.iter().zip(IC).any(|(c, n)| c.name != n)
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
                if SCALARS.contains(&parameter.name.as_str()) =>
            {
                parts.push(named_value(parameter, false)?);
            }
            _ => {
                return Err(refuse(
                    format!("parameter {:?} on 't' instance", parameter.name),
                    Some(location),
                ));
            }
        }
    }
    Ok(())
}

/// E/F/G/H (`parser/controlled.rs`): the F/H controlling source first, then
/// the gain slot, then any `m=`/`gain=` tail. A gain stored last after other
/// setters was C's leading value (applied after the named setters), so it is
/// written positionally; a gain stored first is written as `gain=`. Any other
/// order cannot be re-parsed to the same setter sequence and is refused.
fn controlled_parameters(device: &DeviceInstance, parts: &mut Vec<String>) -> SpiceResult<()> {
    let designator = device.designator;
    let mut setters = device.parameters.as_slice();
    if matches!(designator, 'f' | 'h') {
        match setters.split_first() {
            Some((control, rest))
                if control.name == "control" && control.kind == ParameterKind::Instance =>
            {
                parts.push(node(&control.value, &control.location)?);
                setters = rest;
            }
            _ => {
                return Err(refuse(
                    format!("'{designator}' instance without a leading controlling source"),
                    Some(&device.location),
                ));
            }
        }
    }
    let multiplier = matches!(designator, 'f' | 'g');
    for parameter in setters {
        let allowed = parameter.name == "gain" || (multiplier && parameter.name == "m");
        if !allowed
            || !matches!(
                parameter.kind,
                ParameterKind::Scalar | ParameterKind::Expression(_)
            )
        {
            return Err(refuse(
                format!("parameter {:?} on '{designator}' instance", parameter.name),
                Some(&parameter.location),
            ));
        }
    }
    let unrepresentable = || {
        refuse(
            format!("'{designator}' setter order cannot be written as a linear gain card"),
            Some(&device.location),
        )
    };
    match setters {
        [gain] if gain.name == "gain" => parts.push(value_text(gain, false)?),
        [first, rest @ ..] if first.name == "gain" => {
            if rest.first().is_some_and(|next| next.name != "m") {
                return Err(unrepresentable());
            }
            for parameter in setters {
                parts.push(named_value(parameter, false)?);
            }
        }
        [tail @ .., gain]
            if gain.name == "gain" && tail.first().is_some_and(|first| first.name == "m") =>
        {
            parts.push(value_text(gain, false)?);
            for parameter in tail {
                parts.push(named_value(parameter, false)?);
            }
        }
        _ if !setters.iter().any(|parameter| parameter.name == "gain") => {
            return Err(refuse(
                format!("'{designator}' instance without a gain"),
                Some(&device.location),
            ));
        }
        _ => return Err(unrepresentable()),
    }
    Ok(())
}

/// The behavioural expression text must re-parse to the stored tree.
fn behavioural_text(parameter: &ParameterAssignment) -> SpiceResult<String> {
    let location = &parameter.location;
    let ParameterKind::Behavioural(expression) = &parameter.kind else {
        return Err(refuse(
            format!(
                "parameter {:?} is not a behavioural expression",
                parameter.name
            ),
            Some(location),
        ));
    };
    let reparsed = Parser::new()
        .parse_behavioural_expression(&expression.text, location, expression.verbatim)
        .map_err(|error| {
            refuse(
                format!(
                    "behavioural expression {:?} does not re-parse: {error}",
                    expression.text
                ),
                Some(location),
            )
        })?;
    if expression.text.contains(['\n', '\r'])
        || bexpr_form(&reparsed.root) != bexpr_form(&expression.root)
    {
        return Err(refuse(
            format!(
                "behavioural expression text {:?} does not match its stored syntax tree",
                expression.text
            ),
            Some(location),
        ));
    }
    Ok(expression.text.clone())
}

/// B sources and the nonlinear E/G/F/H forms (`parser/behavioural.rs`):
/// `b1 n+ n- v=<text> [setters]`, `e1 n+ n- value=<text> [setters]`,
/// `e1 n+ n- table {<text>} = (x, y) ... [m=..]`,
/// `e1 n+ n- nc+ nc- table=(x0, y0, ...)` and
/// `e1 n+ n- poly(n) <controls> <coefficients> [m=..]`.
fn behavioural_card(device: &DeviceInstance) -> SpiceResult<String> {
    let location = &device.location;
    let designator = device.designator;
    if device.model.is_some() {
        return Err(refuse("behavioural source with a model", Some(location)));
    }
    let mut parts = vec![node(&device.name, location)?];
    let [positive, negative, controls @ ..] = device.nodes.as_slice() else {
        return Err(refuse(
            format!("'{designator}' instance with fewer than two nodes"),
            Some(location),
        ));
    };
    parts.push(node(positive, location)?);
    parts.push(node(negative, location)?);
    let setter = |parameter: &ParameterAssignment, allowed: &[&str]| {
        if allowed.contains(&parameter.name.as_str()) {
            named_value(parameter, false)
        } else {
            Err(refuse(
                format!("parameter {:?} on '{designator}' instance", parameter.name),
                Some(&parameter.location),
            ))
        }
    };
    let b_setters = [
        "m",
        "tc1",
        "tc2",
        "temp",
        "dtemp",
        "reciproctc",
        "reciprocm",
    ];
    let parameters = device.parameters.as_slice();
    let first = parameters.first().ok_or_else(|| {
        refuse(
            format!("'{designator}' instance without a behavioural form"),
            Some(location),
        )
    })?;
    match (designator, first.name.as_str()) {
        ('b', "v" | "i") | ('e' | 'g', "value") if controls.is_empty() => {
            parts.push(format!("{}={}", first.name, behavioural_text(first)?));
            let allowed: &[&str] = if designator == 'g' {
                &["m"]
            } else {
                &b_setters
            };
            for parameter in &parameters[1..] {
                parts.push(setter(parameter, allowed)?);
            }
        }
        ('e' | 'g', "table") => {
            let four_node = match (&first.kind, controls) {
                (ParameterKind::Behavioural(_), []) => false,
                (ParameterKind::Flag, [nc_positive, nc_negative]) if designator == 'e' => {
                    parts.push(node(nc_positive, location)?);
                    parts.push(node(nc_negative, location)?);
                    true
                }
                _ => {
                    return Err(refuse(
                        format!("'{designator}' TABLE with {} nodes", device.nodes.len()),
                        Some(location),
                    ));
                }
            };
            let mut rest = &parameters[1..];
            let mut values = Vec::new();
            while let [x, y, tail @ ..] = rest {
                if x.name != "x" || y.name != "y" {
                    break;
                }
                values.push((value_text(x, false)?, value_text(y, false)?));
                rest = tail;
            }
            if values.is_empty() {
                return Err(refuse("TABLE without points", Some(location)));
            }
            if four_node {
                let flat: Vec<String> = values
                    .iter()
                    .flat_map(|(x, y)| [x.clone(), y.clone()])
                    .collect();
                parts.push(format!("table=({})", flat.join(", ")));
            } else {
                parts.push(format!("table {{{}}} =", behavioural_text(first)?));
                for (x, y) in &values {
                    parts.push(format!("({x}, {y})"));
                }
            }
            let allowed: &[&str] = if designator == 'g' { &["m"] } else { &[] };
            for parameter in rest {
                parts.push(setter(parameter, allowed)?);
            }
        }
        (_, "poly") => {
            let dimension = value_text(first, false)?
                .parse::<usize>()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| refuse("POLY dimension is not an integer", Some(location)))?;
            parts.push(format!("poly({dimension})"));
            let mut rest = &parameters[1..];
            if matches!(designator, 'e' | 'g') {
                if controls.len() != 2 * dimension {
                    return Err(refuse(
                        format!(
                            "POLY({dimension}) with {} controlling nodes",
                            controls.len()
                        ),
                        Some(location),
                    ));
                }
                for control in controls {
                    parts.push(node(control, location)?);
                }
            } else {
                for _ in 0..dimension {
                    match rest.split_first() {
                        Some((control, tail))
                            if control.name == "control"
                                && control.kind == ParameterKind::Instance =>
                        {
                            parts.push(node(&control.value, &control.location)?);
                            rest = tail;
                        }
                        _ => {
                            return Err(refuse(
                                format!("POLY({dimension}) without its controlling sources"),
                                Some(location),
                            ));
                        }
                    }
                }
                if !controls.is_empty() {
                    return Err(refuse("F/H POLY with controlling nodes", Some(location)));
                }
            }
            let mut coefficients = 0;
            while let Some((coefficient, tail)) = rest.split_first() {
                if coefficient.name != "coef" {
                    break;
                }
                parts.push(value_text(coefficient, false)?);
                coefficients += 1;
                rest = tail;
            }
            if coefficients == 0 {
                return Err(refuse("POLY without coefficients", Some(location)));
            }
            let allowed: &[&str] = if matches!(designator, 'g' | 'f') {
                &["m"]
            } else {
                &[]
            };
            for parameter in rest {
                parts.push(setter(parameter, allowed)?);
            }
        }
        _ => {
            return Err(refuse(
                format!(
                    "'{designator}' instance whose first parameter {:?} is not a behavioural form",
                    first.name
                ),
                Some(location),
            ));
        }
    }
    Ok(parts.join(" "))
}

/// K (`parser/mutual.rs`): the inductor references `inductor1`, `inductor2`,
/// … in order, then exactly one `coefficient`, written positionally (a named
/// `k=`/`coefficient=` setter re-parses to the same setter).
fn mutual_parameters(device: &DeviceInstance, parts: &mut Vec<String>) -> SpiceResult<()> {
    let Some((coupling, inductors)) = device.parameters.split_last() else {
        return Err(refuse(
            "'k' instance without inductors and a coupling",
            Some(&device.location),
        ));
    };
    if inductors.len() < 2 {
        return Err(refuse(
            "'k' instance with fewer than two inductors",
            Some(&device.location),
        ));
    }
    for (index, inductor) in inductors.iter().enumerate() {
        if inductor.kind != ParameterKind::Instance
            || inductor.name != format!("inductor{}", index + 1)
        {
            return Err(refuse(
                format!("parameter {:?} on 'k' instance", inductor.name),
                Some(&inductor.location),
            ));
        }
        if parse_spice_number(&inductor.value).is_some() {
            return Err(refuse(
                "numeric-looking inductor name on a 'k' instance",
                Some(&inductor.location),
            ));
        }
        parts.push(node(&inductor.value, &inductor.location)?);
    }
    if coupling.name != "coefficient"
        || !matches!(
            coupling.kind,
            ParameterKind::Scalar | ParameterKind::Expression(_)
        )
    {
        return Err(refuse(
            format!("parameter {:?} on 'k' instance", coupling.name),
            Some(&coupling.location),
        ));
    }
    parts.push(value_text(coupling, false)?);
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
            (
                ParameterKind::Scalar | ParameterKind::Expression(_),
                name @ ("distof1mag" | "distof2mag"),
            ) => {
                let phase_name = if name == "distof1mag" {
                    "distof1phase"
                } else {
                    "distof2phase"
                };
                let Some(phase) = parameters.next_if(|next| next.name == phase_name) else {
                    return Err(refuse(
                        format!("{name} without {phase_name}"),
                        Some(location),
                    ));
                };
                parts.push(name.trim_end_matches("mag").to_owned());
                parts.push(value_text(parameter, false)?);
                parts.push(value_text(phase, false)?);
            }
            (ParameterKind::Waveform(waveform), name) => {
                parts.push(waveform_text(waveform, name, location)?);
            }
            // PWL delay/repeat setters stay separate ordered scalars (vsrc.c).
            (ParameterKind::Scalar | ParameterKind::Expression(_), name @ ("r" | "td")) => {
                parts.push(format!("{name}={}", value_text(parameter, false)?));
            }
            // RFSPICE port setters of a V source, ordered like the PWL ones.
            (
                ParameterKind::Scalar | ParameterKind::Expression(_),
                name @ ("portnum" | "z0" | "pwr" | "freq" | "phase"),
            ) if device.designator == 'v' => {
                parts.push(format!("{name}={}", value_text(parameter, false)?));
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
                &pulse.count,
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
        SourceWaveform::Function(function) => {
            let fields = function.function.fields().len();
            if function.values.len() < 2 || function.values.len() > fields {
                return Err(refuse(
                    format!("{} needs 2 to {fields} fields", function.function.keyword()),
                    Some(location),
                ));
            }
            // `sine` is C's alias of `sin`; keep the spelling that was parsed.
            let keyword = if function.function == SourceFunction::Sin && name == "sine" {
                "sine"
            } else {
                function.function.keyword()
            };
            (keyword, function.values.iter().collect())
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
