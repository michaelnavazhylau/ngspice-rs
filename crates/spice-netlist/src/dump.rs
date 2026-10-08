//! Deterministic, versioned text dumps of tokens and the semantic AST.
//!
//! These dumps exist so parser changes are reviewable as text diffs. They are
//! **not** Rust `Debug` output: the schema below is explicit, line oriented and
//! versioned by a header line. Changing any line shape, spelling or ordering
//! requires bumping the version constant ([`TOKEN_DUMP_HEADER`] /
//! [`AST_DUMP_HEADER`]); see `conformance/snapshots/README.md`.
//!
//! # Common conventions
//!
//! - Output is UTF-8 with `\n` line endings and a final newline.
//! - Strings are double quoted with `\\`, `\"`, `\n`, `\r`, `\t` and
//!   `\u{hex}` escapes for other control characters. Everything else is verbatim.
//! - A location is `path:line:column` (1-based line, 1-based **byte** column
//!   relative to the joined logical card). Paths are rendered by [`PathMapper`]:
//!   relative to the fixture root with `/` separators, never absolute.
//! - Reals are printed with Rust's shortest round-trip exponent form (`{:e}`).
//! - Nesting is shown with two-space indentation.
//!
//! # Token dump (`# ngspice-rs token-dump v1`)
//!
//! ```text
//! # ngspice-rs token-dump v1
//! file: netlists/rc_divider.cir
//! title: "RC divider" @netlists/rc_divider.cir:1:1
//! card 1 @netlists/rc_divider.cir:2:1 lines=2-2 continuations=0
//!   raw: "v1 in 0 dc 5"
//!   kind: device(v)
//!   token 1 1:1 word "v1"
//!   token 3 1:7 number "0" value=0e0
//! ```
//!
//! Token lines are `token <n> <line>:<column> <kind> <spelling>` followed by
//! `value=` (numbers), `inner=` (braced expressions) or `unquoted=` (quoted
//! strings). A card that fails to tokenize prints an `error` block instead of
//! tokens; later cards are still dumped.
//!
//! # AST dump (`# ngspice-rs ast-dump v1`)
//!
//! The root and every `.subckt` body are *scopes*. Each scope lists its ordered
//! `cards` (with scope-local typed indexes and `via=[...]` include chains) and
//! then its typed vectors (`devices`, `models`, `subcircuits`, `analyses`,
//! `includes`, `params`, `functions` (only when the scope has `.func` cards),
//! and at the root `options`, `globals`, plus
//! `initial-conditions` and `nodesets` only when the deck has such cards, so
//! decks without them keep the earlier v1 shape; `.tran` `uic` is an extra
//! `uic @loc` line under the analysis, only when set). Parameter
//! assignments keep application order and duplicates. Expressions are printed as
//! trees with byte spans. A parse failure yields a single `error` block with the
//! positioned diagnostic; no partial AST is dumped.

use std::fmt::Write as _;
use std::path::Path;

use spice_core::{SourceLoc, SpiceError, SpiceResult};

use crate::ast::{
    AnalysisCard, DeviceInstance, FuncCard, FuncSpelling, GlobalCard, IncludeDirective, ModelCard,
    Netlist, NodeHintCard, NodeHintValue, OptionCard, ParamCard, ParameterAssignment,
    ParameterKind, PositionedValue, ScopedCard, ScopedCardKind, SourceWaveform, Subcircuit,
};
use crate::card::{CardKind, RawCard};
use crate::expr::{BinaryOp, Expr, ExprKind, ParameterExpression, SourceSpan, UnaryOp};
use crate::parser::Parser;
use crate::source::{self, Deck};
use crate::token::{Token, TokenKind};

/// First line of every token dump. Bump the version on any schema change.
pub const TOKEN_DUMP_HEADER: &str = "# ngspice-rs token-dump v1";
/// First line of every AST dump. Bump the version on any schema change.
pub const AST_DUMP_HEADER: &str = "# ngspice-rs ast-dump v1";

/// Rewrites file paths so dumps never contain machine-specific prefixes.
///
/// Only the fixture root is normalized: a path under the root becomes the
/// remaining components joined with `/`. Windows separators (`\`), drive-letter
/// case and the `\\?\` verbatim prefix are handled at the string level, so the
/// result is identical on every host and is unit-testable on any platform. A
/// path that is absolute but outside the root becomes `<external>/<file name>`
/// instead of leaking a local path. A relative path is only separator-normalized.
#[derive(Debug, Clone)]
pub struct PathMapper {
    roots: Vec<String>,
}

fn slashes(text: &str) -> String {
    let mut text = text.replace('\\', "/");
    if let Some(rest) = text.strip_prefix("//?/") {
        text = rest.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut previous_slash = false;
    for character in text.chars() {
        if character == '/' {
            if previous_slash {
                continue;
            }
            previous_slash = true;
        } else {
            previous_slash = false;
        }
        out.push(character);
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

fn is_absolute(normalized: &str) -> bool {
    let bytes = normalized.as_bytes();
    normalized.starts_with('/')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

fn strip_root<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let drive = root.as_bytes().get(1) == Some(&b':');
    let head = path.get(..root.len())?;
    let same = if drive {
        head.eq_ignore_ascii_case(root)
    } else {
        head == root
    };
    if !same {
        return None;
    }
    let rest = &path[root.len()..];
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix('/')
    }
}

impl PathMapper {
    /// A mapper for one fixture root. The canonical form of the root is also
    /// accepted, because include resolution reports canonical paths.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        let mut roots = vec![slashes(&root.to_string_lossy())];
        if let Ok(canonical) = root.canonicalize() {
            let canonical = slashes(&canonical.to_string_lossy());
            if !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }
        Self::from_roots(roots)
    }

    /// A mapper from root spellings, for tests (any separator style).
    #[must_use]
    pub fn from_roots<S: AsRef<str>>(roots: impl IntoIterator<Item = S>) -> Self {
        let mut roots: Vec<String> = roots.into_iter().map(|r| slashes(r.as_ref())).collect();
        roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
        Self { roots }
    }

    /// Normalizes a path given as text (any separator style).
    #[must_use]
    pub fn map_str(&self, path: &str) -> String {
        let normalized = slashes(path);
        for root in &self.roots {
            if let Some(rest) = strip_root(&normalized, root) {
                return if rest.is_empty() {
                    ".".to_owned()
                } else {
                    rest.to_owned()
                };
            }
        }
        if is_absolute(&normalized) {
            let name = normalized.rsplit('/').next().unwrap_or("");
            return format!("<external>/{name}");
        }
        normalized
    }

    /// Normalizes a filesystem path.
    #[must_use]
    pub fn map(&self, path: &Path) -> String {
        self.map_str(&path.to_string_lossy())
    }

    /// Removes the root prefix from free-form diagnostic text (messages embed
    /// canonical paths), in both separator styles.
    #[must_use]
    pub fn scrub(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for root in &self.roots {
            text = text.replace(&format!("{root}/"), "");
            let backslash = root.replace('/', "\\");
            text = text.replace(&format!("{backslash}\\"), "");
            text = text.replace(&format!("\\\\?\\{backslash}\\"), "");
        }
        let mut text = text.replace("//?/", "");
        // OS error wording differs per platform ("No such file or directory"
        // vs "The system cannot find the file specified."); keep only the
        // stable positioned prefix.
        if let Some(position) = text.find(" (os error ")
            && let Some(start) = text[..position].rfind(": ")
            && let Some(close) = text[position..].find(')')
        {
            text.replace_range(start..position + close + 1, ": <os error>");
        }
        text
    }
}

/// Quotes a string with the dump escape rules.
#[must_use]
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn real(value: f64) -> String {
    format!("{value:e}")
}

struct Out {
    text: String,
}

impl Out {
    fn new(header: &str) -> Self {
        Self {
            text: format!("{header}\n"),
        }
    }

    fn line(&mut self, indent: usize, content: impl AsRef<str>) {
        for _ in 0..indent {
            self.text.push_str("  ");
        }
        // Values are quoted, so trimming only drops separators before an
        // empty list and keeps snapshots free of trailing whitespace.
        self.text.push_str(content.as_ref().trim_end());
        self.text.push('\n');
    }
}

struct Ctx<'a> {
    paths: &'a PathMapper,
}

impl Ctx<'_> {
    fn loc(&self, location: &SourceLoc) -> String {
        format!(
            "{}:{}:{}",
            self.paths.map(location.path()),
            location.line,
            location.column
        )
    }

    fn span(&self, span: &SourceSpan) -> String {
        if span.start.line == span.end.line {
            format!(
                "{}:{}..{}",
                span.start.line, span.start.column, span.end.column
            )
        } else {
            format!(
                "{}:{}..{}:{}",
                span.start.line, span.start.column, span.end.line, span.end.column
            )
        }
    }

    fn error(&self, out: &mut Out, indent: usize, error: &SpiceError) {
        let m = |text: &str| quote(&self.paths.scrub(text));
        match error {
            SpiceError::Parse { location, message } => {
                out.line(indent, format!("error parse @{}", self.loc(location)));
                out.line(indent + 1, format!("message: {}", m(message)));
            }
            SpiceError::UnknownDevice {
                name,
                designator,
                location,
            } => {
                out.line(
                    indent,
                    format!(
                        "error unknown-device name={} designator={} @{}",
                        quote(name),
                        designator,
                        self.loc(location)
                    ),
                );
            }
            SpiceError::Unsupported { feature, location } => {
                let at = location
                    .as_ref()
                    .map_or("none".to_owned(), |l| format!("@{}", self.loc(l)));
                out.line(indent, format!("error unsupported {at}"));
                out.line(indent + 1, format!("feature: {}", m(feature)));
            }
            SpiceError::NotYetPorted { what, c_reference } => {
                out.line(indent, "error not-yet-ported");
                out.line(indent + 1, format!("what: {}", m(what)));
                out.line(indent + 1, format!("c_reference: {}", quote(c_reference)));
            }
            SpiceError::Io { path, message } => {
                out.line(
                    indent,
                    format!("error io path={}", quote(&self.paths.map(path))),
                );
                out.line(indent + 1, format!("message: {}", m(message)));
            }
            SpiceError::Numerical { context, message } => {
                out.line(indent, format!("error numerical context={}", m(context)));
                out.line(indent + 1, format!("message: {}", m(message)));
            }
            SpiceError::Circuit { message } => {
                out.line(indent, "error circuit");
                out.line(indent + 1, format!("message: {}", m(message)));
            }
        }
    }
}

fn card_kind(kind: &CardKind) -> String {
    match kind {
        CardKind::Device { designator } => format!("device({designator})"),
        CardKind::DotCommand(command) => format!("dot({})", command.card_name()),
        CardKind::Unknown => "unknown".to_owned(),
    }
}

fn token_line(ctx: &Ctx<'_>, out: &mut Out, indent: usize, number: usize, token: &Token) {
    let _ = ctx;
    let (kind, extra) = match &token.kind {
        TokenKind::Word => ("word", String::new()),
        TokenKind::Number(value) => ("number", format!(" value={}", real(*value))),
        TokenKind::Expression(inner) => ("expression", format!(" inner={}", quote(inner))),
        TokenKind::Quoted(value) => ("quoted", format!(" unquoted={}", quote(value))),
        TokenKind::LParen => ("lparen", String::new()),
        TokenKind::RParen => ("rparen", String::new()),
        TokenKind::Comma => ("comma", String::new()),
        TokenKind::Equals => ("equals", String::new()),
    };
    out.line(
        indent,
        format!(
            "token {number} {}:{} {kind} {}{extra}",
            token.location.line,
            token.location.column,
            quote(&token.text)
        ),
    );
}

/// Dumps the token stream of every card of a loaded deck (the root file only;
/// included files appear in the AST dump). `paths` normalizes file names.
#[must_use]
pub fn dump_tokens(deck: &Deck, paths: &PathMapper) -> String {
    let ctx = Ctx { paths };
    let mut out = Out::new(TOKEN_DUMP_HEADER);
    out.line(0, format!("file: {}", paths.map(&deck.path)));
    out.line(
        0,
        format!(
            "title: {} @{}",
            quote(&deck.title),
            ctx.loc(&deck.title_location)
        ),
    );
    for (index, line) in deck.lines.iter().enumerate() {
        out.line(
            0,
            format!(
                "card {} @{} lines={}-{} continuations={}",
                index + 1,
                ctx.loc(&line.location),
                line.location.line,
                line.end_line,
                line.continuations
            ),
        );
        out.line(1, format!("raw: {}", quote(&line.text)));
        match RawCard::parse(line) {
            Ok(card) => {
                out.line(1, format!("kind: {}", card_kind(&card.kind)));
                for (n, token) in card.tokens.iter().enumerate() {
                    token_line(&ctx, &mut out, 1, n + 1, token);
                }
            }
            Err(error) => ctx.error(&mut out, 1, &error),
        }
    }
    out.text
}

/// Loads `path` and dumps its tokens; paths are relative to `root`.
///
/// # Errors
///
/// Returns the I/O error if the file cannot be read.
pub fn dump_tokens_file(path: &Path, root: &Path) -> SpiceResult<String> {
    let deck = source::load(path)?;
    Ok(dump_tokens(&deck, &PathMapper::new(root)))
}

/// Parses `path` (resolving includes) and dumps the AST, or the positioned
/// error when parsing fails; the output is deterministic either way.
#[must_use]
pub fn dump_ast_file(parser: &Parser, path: &Path, root: &Path) -> String {
    let paths = PathMapper::new(root);
    match parser.parse_file(path) {
        Ok(netlist) => dump_ast(&netlist, &paths),
        Err(error) => dump_ast_error(path, &error, &paths),
    }
}

/// The AST-dump form of a failed parse.
#[must_use]
pub fn dump_ast_error(path: &Path, error: &SpiceError, paths: &PathMapper) -> String {
    let ctx = Ctx { paths };
    let mut out = Out::new(AST_DUMP_HEADER);
    out.line(0, format!("file: {}", paths.map(path)));
    ctx.error(&mut out, 0, error);
    out.text
}

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

/// Dumps a parsed netlist.
#[must_use]
pub fn dump_ast(netlist: &Netlist, paths: &PathMapper) -> String {
    let ctx = Ctx { paths };
    let mut out = Out::new(AST_DUMP_HEADER);
    out.line(0, format!("file: {}", paths.map(&netlist.path)));
    out.line(
        0,
        format!(
            "title: {} @{}",
            quote(&netlist.title),
            ctx.loc(&netlist.location)
        ),
    );
    out.line(0, "scope root");
    let scope = Scope {
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
    };
    write_scope(&ctx, &mut out, 1, &scope);
    out.text
}

fn section(out: &mut Out, indent: usize, name: &str, count: usize) {
    out.line(indent, format!("{name} ({count}):"));
}

fn write_scope(ctx: &Ctx<'_>, out: &mut Out, indent: usize, scope: &Scope<'_>) {
    section(out, indent, "cards", scope.cards.len());
    for (n, card) in scope.cards.iter().enumerate() {
        write_card(ctx, out, indent + 1, n, card);
    }
    section(out, indent, "devices", scope.devices.len());
    for (n, device) in scope.devices.iter().enumerate() {
        write_device(ctx, out, indent + 1, n, device);
    }
    section(out, indent, "models", scope.models.len());
    for (n, model) in scope.models.iter().enumerate() {
        out.line(
            indent + 1,
            format!(
                "model [{n}] name={} base={} level={} @{}",
                quote(&model.name),
                quote(&model.base),
                model.level.map_or("none".to_owned(), real),
                ctx.loc(&model.location)
            ),
        );
        write_parameters(ctx, out, indent + 2, &model.parameters);
    }
    section(out, indent, "subcircuits", scope.subcircuits.len());
    for (n, sub) in scope.subcircuits.iter().enumerate() {
        out.line(
            indent + 1,
            format!(
                "subcircuit [{n}] name={} @{} end=@{}",
                quote(&sub.name),
                ctx.loc(&sub.location),
                ctx.loc(&sub.end_location)
            ),
        );
        let terminals: Vec<String> = sub.terminals.iter().map(|t| quote(t)).collect();
        out.line(indent + 2, format!("terminals: {}", terminals.join(" ")));
        write_parameters(ctx, out, indent + 2, &sub.parameters);
        out.line(indent + 2, "scope body");
        let inner = Scope {
            devices: &sub.devices,
            models: &sub.models,
            subcircuits: &sub.subcircuits,
            analyses: &sub.analyses,
            includes: &sub.includes,
            params: &sub.params,
            functions: &sub.functions,
            options: &[],
            globals: &[],
            initial_conditions: &[],
            nodesets: &[],
            cards: &sub.cards,
        };
        write_scope(ctx, out, indent + 3, &inner);
    }
    section(out, indent, "analyses", scope.analyses.len());
    for (n, analysis) in scope.analyses.iter().enumerate() {
        out.line(
            indent + 1,
            format!(
                "analysis [{n}] kind={} @{}",
                analysis.kind.as_str(),
                ctx.loc(&analysis.location)
            ),
        );
        let arguments: Vec<String> = analysis.arguments.iter().map(|a| quote(a)).collect();
        out.line(indent + 2, format!("arguments: {}", arguments.join(" ")));
        if let Some(location) = &analysis.uic_location {
            out.line(indent + 2, format!("uic @{}", ctx.loc(location)));
        }
        for argument in &analysis.expressions {
            out.line(
                indent + 2,
                format!("argument-expression index={}", argument.index),
            );
            write_expression(ctx, out, indent + 3, &argument.expression);
        }
    }
    section(out, indent, "includes", scope.includes.len());
    for (n, include) in scope.includes.iter().enumerate() {
        out.line(
            indent + 1,
            format!(
                "include [{n}] path={} spelling={} resolved={} section={} @{}",
                quote(&include.path),
                quote(&include.path_spelling),
                include
                    .resolved_path
                    .as_ref()
                    .map_or("none".to_owned(), |p| quote(&ctx.paths.map(p))),
                include
                    .section
                    .as_ref()
                    .map_or("none".to_owned(), |s| quote(s)),
                ctx.loc(&include.location)
            ),
        );
        if let Some(selected) = &include.selected_section {
            out.line(
                indent + 2,
                format!(
                    "selected {} opening=@{} {} closing=@{} {}",
                    quote(&selected.name),
                    ctx.loc(&selected.opening.location),
                    quote(&selected.opening.raw),
                    ctx.loc(&selected.closing.location),
                    quote(&selected.closing.raw)
                ),
            );
        }
    }
    section(out, indent, "params", scope.params.len());
    for (n, card) in scope.params.iter().enumerate() {
        out.line(
            indent + 1,
            format!("param-card [{n}] @{}", ctx.loc(&card.location)),
        );
        for (m, assignment) in card.assignments.iter().enumerate() {
            out.line(
                indent + 2,
                format!(
                    "assignment [{m}] name={} name_span={}",
                    quote(&assignment.name),
                    ctx.span(&assignment.name_span)
                ),
            );
            write_expression(ctx, out, indent + 3, &assignment.expression);
        }
    }
    // Only decks with `.func` cards get the section, so other snapshots keep
    // the earlier v1 shape.
    if !scope.functions.is_empty() {
        section(out, indent, "functions", scope.functions.len());
        for (n, card) in scope.functions.iter().enumerate() {
            let parameters: Vec<String> = card
                .parameters
                .iter()
                .map(|p| format!("{}@{}", quote(&p.name), ctx.span(&p.span)))
                .collect();
            out.line(
                indent + 1,
                format!(
                    "func [{n}] name={} name_span={} parameters=[{}]{} @{}",
                    quote(&card.name),
                    ctx.span(&card.name_span),
                    parameters.join(", "),
                    // Printed only for the `.param` spelling, so `.func`
                    // snapshots keep their shape.
                    if card.spelling == FuncSpelling::Param {
                        " spelling=param"
                    } else {
                        ""
                    },
                    ctx.loc(&card.location)
                ),
            );
            write_expression(ctx, out, indent + 2, &card.body);
        }
    }
    section(out, indent, "options", scope.options.len());
    for (n, card) in scope.options.iter().enumerate() {
        out.line(
            indent + 1,
            format!("option-card [{n}] @{}", ctx.loc(&card.location)),
        );
        for (m, setting) in card.settings.iter().enumerate() {
            let value = setting
                .value
                .as_ref()
                .map_or("flag".to_owned(), |v| positioned(ctx, v));
            out.line(
                indent + 2,
                format!(
                    "setting [{m}] name={} value={value} @{}",
                    quote(&setting.name),
                    ctx.loc(&setting.location)
                ),
            );
            if let Some(expression) = &setting.expression {
                write_expression(ctx, out, indent + 3, expression);
            }
        }
    }
    section(out, indent, "globals", scope.globals.len());
    for (n, card) in scope.globals.iter().enumerate() {
        out.line(
            indent + 1,
            format!("global-card [{n}] @{}", ctx.loc(&card.location)),
        );
        for (m, node) in card.nodes.iter().enumerate() {
            out.line(
                indent + 2,
                format!(
                    "node [{m}] {} @{}",
                    quote(&node.name),
                    ctx.loc(&node.location)
                ),
            );
        }
    }
    for (name, label, cards) in [
        ("initial-conditions", "ic-card", scope.initial_conditions),
        ("nodesets", "nodeset-card", scope.nodesets),
    ] {
        // Emitted only when present, so decks without these cards keep the
        // exact v1 shape.
        if cards.is_empty() {
            continue;
        }
        section(out, indent, name, cards.len());
        for (n, card) in cards.iter().enumerate() {
            out.line(
                indent + 1,
                format!("{label} [{n}] @{}", ctx.loc(&card.location)),
            );
            for (m, hint) in card.entries.iter().enumerate() {
                out.line(
                    indent + 2,
                    format!(
                        "entry [{m}] node={} node_at={} @{}",
                        quote(&hint.node),
                        ctx.loc(&hint.node_location),
                        ctx.loc(&hint.location)
                    ),
                );
                match &hint.value {
                    NodeHintValue::Literal { text, value } => out.line(
                        indent + 3,
                        format!(
                            "value literal {} value={} @{}",
                            quote(text),
                            real(*value),
                            ctx.loc(&hint.value_location)
                        ),
                    ),
                    NodeHintValue::Expression(expression) => {
                        out.line(
                            indent + 3,
                            format!("value expression @{}", ctx.loc(&hint.value_location)),
                        );
                        write_expression(ctx, out, indent + 4, expression);
                    }
                }
            }
        }
    }
}

fn write_card(ctx: &Ctx<'_>, out: &mut Out, indent: usize, n: usize, card: &ScopedCard) {
    let kind = match card.kind {
        ScopedCardKind::Device(i) => format!("device[{i}]"),
        ScopedCardKind::Model(i) => format!("model[{i}]"),
        ScopedCardKind::Subcircuit(i) => format!("subcircuit[{i}]"),
        ScopedCardKind::Analysis(i) => format!("analysis[{i}]"),
        ScopedCardKind::Include(i) => format!("include[{i}]"),
        ScopedCardKind::Options(i) => format!("options[{i}]"),
        ScopedCardKind::Global(i) => format!("global[{i}]"),
        ScopedCardKind::Param(i) => format!("param[{i}]"),
        ScopedCardKind::Func(i) => format!("func[{i}]"),
        ScopedCardKind::InitialCondition(i) => format!("ic[{i}]"),
        ScopedCardKind::Nodeset(i) => format!("nodeset[{i}]"),
        ScopedCardKind::Output => "output".to_owned(),
        ScopedCardKind::Measure => "measure".to_owned(),
        ScopedCardKind::Fourier => "four".to_owned(),
        ScopedCardKind::Ends => "ends".to_owned(),
        ScopedCardKind::End => "end".to_owned(),
    };
    let chain = if card.include_chain.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = card.include_chain.iter().map(|l| ctx.loc(l)).collect();
        format!(" via=[{}]", parts.join(", "))
    };
    out.line(
        indent,
        format!(
            "[{n}] {kind} @{} {}{chain}",
            ctx.loc(&card.source.location),
            quote(&card.source.raw)
        ),
    );
}

fn write_device(ctx: &Ctx<'_>, out: &mut Out, indent: usize, n: usize, device: &DeviceInstance) {
    out.line(
        indent,
        format!(
            "device [{n}] name={} designator={} @{}",
            quote(&device.name),
            device.designator,
            ctx.loc(&device.location)
        ),
    );
    let nodes: Vec<String> = device.nodes.iter().map(|x| quote(x)).collect();
    out.line(indent + 1, format!("nodes: {}", nodes.join(" ")));
    out.line(
        indent + 1,
        format!(
            "model: {}",
            device
                .model
                .as_ref()
                .map_or("none".to_owned(), |m| quote(m))
        ),
    );
    write_parameters(ctx, out, indent + 1, &device.parameters);
}

fn positioned(ctx: &Ctx<'_>, value: &PositionedValue) -> String {
    format!("{}@{}", quote(&value.text), ctx.loc(&value.location))
}

fn optional(ctx: &Ctx<'_>, value: Option<&PositionedValue>) -> String {
    value.map_or("none".to_owned(), |v| positioned(ctx, v))
}

fn write_parameters(
    ctx: &Ctx<'_>,
    out: &mut Out,
    indent: usize,
    parameters: &[ParameterAssignment],
) {
    out.line(indent, format!("parameters ({}):", parameters.len()));
    for (n, parameter) in parameters.iter().enumerate() {
        let kind = match &parameter.kind {
            ParameterKind::Scalar => "scalar",
            ParameterKind::Textual => "textual",
            ParameterKind::Expression(_) => "expression",
            ParameterKind::Flag => "flag",
            ParameterKind::InitialConditions(_) => "initial-conditions",
            ParameterKind::Waveform(_) => "waveform",
        };
        out.line(
            indent + 1,
            format!(
                "[{n}] name={} kind={kind} value={} @{}",
                quote(&parameter.name),
                quote(&parameter.value),
                ctx.loc(&parameter.location)
            ),
        );
        match &parameter.kind {
            ParameterKind::Expression(expression) => {
                write_expression(ctx, out, indent + 2, expression);
            }
            ParameterKind::InitialConditions(components) => {
                for component in components {
                    out.line(
                        indent + 2,
                        format!(
                            "component {} = {}",
                            quote(&component.name),
                            positioned(ctx, &component.value)
                        ),
                    );
                }
            }
            ParameterKind::Waveform(SourceWaveform::Pulse(pulse)) => {
                out.line(indent + 2, "pulse");
                out.line(
                    indent + 3,
                    format!("initial: {}", positioned(ctx, &pulse.initial)),
                );
                out.line(
                    indent + 3,
                    format!("pulsed: {}", positioned(ctx, &pulse.pulsed)),
                );
                out.line(
                    indent + 3,
                    format!("delay: {}", optional(ctx, pulse.delay.as_ref())),
                );
                out.line(
                    indent + 3,
                    format!("rise: {}", optional(ctx, pulse.rise.as_ref())),
                );
                out.line(
                    indent + 3,
                    format!("fall: {}", optional(ctx, pulse.fall.as_ref())),
                );
                out.line(
                    indent + 3,
                    format!("width: {}", optional(ctx, pulse.width.as_ref())),
                );
                out.line(
                    indent + 3,
                    format!("period: {}", optional(ctx, pulse.period.as_ref())),
                );
                // Only printed when supplied, so seven-field snapshots are stable.
                if let Some(count) = &pulse.count {
                    out.line(indent + 3, format!("count: {}", positioned(ctx, count)));
                }
            }
            ParameterKind::Waveform(SourceWaveform::Function(function)) => {
                out.line(indent + 2, function.function.keyword());
                for (name, value) in function.function.fields().iter().zip(&function.values) {
                    out.line(indent + 3, format!("{name}: {}", positioned(ctx, value)));
                }
            }
            ParameterKind::Waveform(SourceWaveform::Pwl(points)) => {
                out.line(indent + 2, "pwl");
                for (m, point) in points.iter().enumerate() {
                    out.line(
                        indent + 3,
                        format!(
                            "point [{m}] time={} value={}",
                            positioned(ctx, &point.time),
                            positioned(ctx, &point.value)
                        ),
                    );
                }
            }
            ParameterKind::Scalar | ParameterKind::Textual | ParameterKind::Flag => {}
        }
    }
}

fn write_expression(ctx: &Ctx<'_>, out: &mut Out, indent: usize, expression: &ParameterExpression) {
    out.line(
        indent,
        format!(
            "expression braced={}{} text={} span={}",
            expression.braced,
            if expression.quoted {
                " quoted=true"
            } else {
                ""
            },
            quote(&expression.text),
            ctx.span(&expression.span)
        ),
    );
    write_expr(ctx, out, indent + 1, &expression.root);
}

fn write_expr(ctx: &Ctx<'_>, out: &mut Out, indent: usize, expr: &Expr) {
    let span = ctx.span(&expr.span);
    match &expr.kind {
        ExprKind::Number { value, spelling } => out.line(
            indent,
            format!(
                "number {} value={} span={span}",
                quote(spelling),
                real(*value)
            ),
        ),
        ExprKind::Identifier(name) => {
            out.line(indent, format!("identifier {} span={span}", quote(name)));
        }
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnaryOp::Plus => "plus",
                UnaryOp::Minus => "minus",
            };
            out.line(indent, format!("unary {op} span={span}"));
            write_expr(ctx, out, indent + 1, operand);
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let op = match op {
                BinaryOp::Add => "add",
                BinaryOp::Sub => "sub",
                BinaryOp::Mul => "mul",
                BinaryOp::Div => "div",
                BinaryOp::Pow => "pow",
            };
            out.line(indent, format!("binary {op} span={span}"));
            write_expr(ctx, out, indent + 1, lhs);
            write_expr(ctx, out, indent + 1, rhs);
        }
        ExprKind::Call {
            function,
            arguments,
        } => {
            out.line(indent, format!("call {} span={span}", function.name()));
            for argument in arguments {
                write_expr(ctx, out, indent + 1, argument);
            }
        }
        ExprKind::Group(inner) => {
            out.line(indent, format!("group span={span}"));
            write_expr(ctx, out, indent + 1, inner);
        }
        ExprKind::UserCall { name, arguments } => {
            out.line(indent, format!("user-call {} span={span}", quote(name)));
            for argument in arguments {
                write_expr(ctx, out, indent + 1, argument);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PathMapper, quote};

    #[test]
    fn unix_root_is_stripped() {
        let m = PathMapper::from_roots(["/home/u/repo/conformance"]);
        assert_eq!(
            m.map_str("/home/u/repo/conformance/netlists/a.cir"),
            "netlists/a.cir"
        );
        assert_eq!(m.map_str("/home/u/repo/conformance"), ".");
    }

    #[test]
    fn windows_separators_and_prefixes_normalize() {
        let m = PathMapper::from_roots([r"C:\work\repo\conformance"]);
        assert_eq!(
            m.map_str(r"C:\work\repo\conformance\netlists\a.cir"),
            "netlists/a.cir"
        );
        assert_eq!(
            m.map_str(r"c:\WORK\repo\conformance\parser\sources\x.inc")
                .to_lowercase(),
            "parser/sources/x.inc"
        );
        assert_eq!(
            m.map_str(r"\\?\C:\work\repo\conformance\a\b.cir"),
            "a/b.cir"
        );
        assert_eq!(m.map_str(r"parts\divider.inc"), "parts/divider.inc");
        assert_eq!(m.map_str(r"C:\elsewhere\secret\x.cir"), "<external>/x.cir");
    }

    #[test]
    fn sibling_prefix_is_not_a_match() {
        let m = PathMapper::from_roots(["/r/conformance"]);
        assert_eq!(m.map_str("/r/conformance-other/a.cir"), "<external>/a.cir");
    }

    #[test]
    fn scrub_removes_roots_in_both_styles() {
        let m = PathMapper::from_roots(["/r/c"]);
        assert_eq!(m.scrub("loading '/r/c/p/a.inc': x"), "loading 'p/a.inc': x");
        assert_eq!(m.scrub(r"loading '\r\c\p\a.inc'"), r"loading 'p\a.inc'");
        let w = PathMapper::from_roots([r"C:\r\c"]);
        assert_eq!(w.scrub(r"loading 'C:\r\c\p\a.inc'"), "loading 'p\\a.inc'");
    }

    #[test]
    fn scrub_neutralizes_platform_os_error_text() {
        let m = PathMapper::from_roots(["/r"]);
        assert_eq!(
            m.scrub("cannot resolve 'a.inc': No such file or directory (os error 2)"),
            "cannot resolve 'a.inc': <os error>"
        );
        assert_eq!(
            m.scrub(
                "cannot resolve 'a.inc': The system cannot find the file specified. (os error 2)"
            ),
            "cannot resolve 'a.inc': <os error>"
        );
    }

    #[test]
    fn quoting_escapes() {
        assert_eq!(quote("a\"b\\c\n\u{1}"), "\"a\\\"b\\\\c\\n\\u{1}\"");
    }
}
