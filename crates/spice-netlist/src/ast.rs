//! The semantic netlist model produced by the incremental parser.
//!
//! The parser, device registry and analyses share these types. The parser
//! constructs linear-device netlists, model cards, bounded D/Q/M flags/ICs and
//! numeric PULSE/PWL syntax, scoped subcircuits and resolved sources. ngspice's parsing quirks
//! are encoded at that boundary:
//!
//! - Parameter values are kept as **text**, not numbers. ngspice evaluates them
//!   with `INPevaluate()`/numparam and lets them depend on `.param` values and
//!   on `temp`, so evaluation is a separate pass. Current parser values are
//!   finite scalar literals, positioned waveform/IC/flag setters, and parsed
//!   but unevaluated `{...}` expressions ([`crate::expr`]); quoted values,
//!   waveform/IC-vector expressions and evaluation remain pending.
//! - A device's connection nodes are not resolved to [`spice_core::NodeId`]s
//!   here; that happens when the circuit is built, so that subcircuit
//!   flattening can rewrite them.
//! - Analysis cards keep their arguments unparsed. The analysis drivers in
//!   `spice-analysis` interpret them, because their grammars differ per card.

use std::path::PathBuf;

use spice_core::{AnalysisKind, Real, SourceLoc};

/// A canonical node name: ASCII-lowercased, with optional `gnd` → `0` aliasing.
pub type NodeName = String;

/// One device parameter, including positional values mapped to canonical names.
///
/// R/C/L leading values become `resistance`/`capacitance`/`inductance`; source
/// values become `dc`, `acmag`, and `acphase`; a diode/BJT's leading value
/// becomes `area`. AC defaults are made explicit. Parameters are in application
/// order: a passive scalar before its model precedes named setters; one after
/// the model follows them. C applies leading source DC and D/Q area last.
/// Duplicate assignments remain visible; consumers must apply them in order
/// rather than treating this vector as a map. Waveform, flag and vector IC
/// setters share this ordered storage with scalars; inspect `kind` before
/// interpreting `value`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterAssignment {
    /// Parameter name, lowercased; ngspice matches parameter names
    /// case-insensitively.
    pub name: String,
    /// The value as written: scalar or unevaluated formal/X token spelling,
    /// or original vector argument text (including parentheses when supplied).
    /// A bare flag has empty text.
    pub value: String,
    /// The setter's syntax/shape. Scalar consumers must reject other kinds,
    /// not interpret a flag or the first vector component as a scalar.
    pub kind: ParameterKind,
    /// Where the assignment was found.
    pub location: SourceLoc,
}

/// Syntax of an ordered parameter setter. No runtime defaults are applied.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterKind {
    /// One finite numeric literal, retained in [`ParameterAssignment::value`].
    Scalar,
    /// Unevaluated single-token formal/X parameter text that is neither a
    /// finite literal nor a parsed expression (for instance a quoted value or
    /// an extended numeric spelling such as `4k7`).
    Textual,
    /// A `{...}` expression, or a bare parameter name at an `X`/`.subckt`
    /// parameter site, parsed but **not evaluated**. [`ParameterAssignment::value`]
    /// keeps the original token spelling (braces included); the box holds the
    /// syntax tree and spans. Scalar consumers must treat this like any other
    /// non-scalar kind until an evaluation pass resolves it.
    Expression(Box<crate::expr::ParameterExpression>),
    /// A bare IF_FLAG keyword: C's INPgetValue supplies integer 1 without
    /// consuming a value. Explicit `flag=0`/`flag=1` forms are not accepted.
    Flag,
    /// Q/M IC values in C setter order, with omitted components left omitted.
    InitialConditions(Vec<InitialCondition>),
    /// A source waveform, not yet evaluated or enabled by device factories.
    Waveform(SourceWaveform),
}

/// One finite textual waveform/IC value and its byte-column position.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedValue {
    /// Original numeric spelling, including scale/unit suffixes.
    pub text: String,
    /// Position in the joined logical card (the tokenizer's location contract).
    pub location: SourceLoc,
}

/// One component of a Q/M `ic` vector.
#[derive(Debug, Clone, PartialEq)]
pub struct InitialCondition {
    /// Canonical scalar setter: icvbe/icvce or icvds/icvgs/icvbs.
    pub name: String,
    /// Value as supplied, not an initialization state.
    pub value: PositionedValue,
}

/// Numeric source syntax from VSRCparam/ISRCparam. Runtime validation and
/// analysis-dependent defaults belong to elaboration, not this AST.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceWaveform {
    /// Two required levels and up to five optional timing fields.
    Pulse(Box<PulseWaveform>),
    /// Strictly paired time/value arguments. No sorting or time repair occurs.
    Pwl(Vec<PwlPoint>),
}

/// `PULSE(V1 V2 [TD [TR [TF [PW [PER]]]]])`; omissions stay explicit.
#[derive(Debug, Clone, PartialEq)]
pub struct PulseWaveform {
    /// Initial level (volts for V, amperes for I).
    pub initial: PositionedValue,
    /// Pulsed level.
    pub pulsed: PositionedValue,
    /// Delay in seconds.
    pub delay: Option<PositionedValue>,
    /// Rise time in seconds.
    pub rise: Option<PositionedValue>,
    /// Fall time in seconds.
    pub fall: Option<PositionedValue>,
    /// Pulse width in seconds.
    pub width: Option<PositionedValue>,
    /// Period in seconds.
    pub period: Option<PositionedValue>,
}

/// One PWL knot, retained in supplied order (syntax is not runtime validation).
#[derive(Debug, Clone, PartialEq)]
pub struct PwlPoint {
    /// Time in seconds.
    pub time: PositionedValue,
    /// Level in volts for V, amperes for I.
    pub value: PositionedValue,
}

/// A device instance, e.g. `r1 in out 1k tc1=0.01`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInstance {
    /// Instance name, e.g. `r1`.
    pub name: String,
    /// Designator letter, lowercased.
    pub designator: char,
    /// Connection nodes, in terminal order, as supplied. Q instances retain
    /// three or four ports; an omitted substrate is not injected here. Circuit
    /// elaboration must ground it as INP2Q does.
    pub nodes: Vec<NodeName>,
    /// The model this instance refers to, if the device takes one.
    pub model: Option<String>,
    /// Parameter assignments, including positional values, in application order.
    pub parameters: Vec<ParameterAssignment>,
    /// Where the instance was written.
    pub location: SourceLoc,
}

/// A `.model` card.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCard {
    /// Model name.
    pub name: String,
    /// The model type, e.g. `d` in `.model d d(is=1e-14)`.
    pub base: String,
    /// The first explicit scalar `level` value, as scanned by `INPfindLev` for
    /// model families with selectors. No default or selector rounding is applied
    /// here. All level assignments also remain in `parameters`; interpreting
    /// them and validating device/backend availability belongs to elaboration.
    pub level: Option<Real>,
    /// Every model parameter.
    pub parameters: Vec<ParameterAssignment>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// A `.subckt` … `.ends` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Subcircuit {
    /// Subcircuit name.
    pub name: String,
    /// External terminals, in order.
    pub terminals: Vec<NodeName>,
    /// Ordered unevaluated formal assignments, with optional `params:` marker.
    pub parameters: Vec<ParameterAssignment>,
    /// Devices in the body.
    pub devices: Vec<DeviceInstance>,
    /// Models declared in the body.
    pub models: Vec<ModelCard>,
    /// Nested definitions; names are local to this body.
    pub subcircuits: Vec<Subcircuit>,
    /// Analysis requests retained in this body, not promoted to the root.
    pub analyses: Vec<AnalysisCard>,
    /// Source directives retained in this body.
    pub includes: Vec<IncludeDirective>,
    /// `.param` cards written in this body, unevaluated.
    pub params: Vec<ParamCard>,
    /// Ordered body cards, including the closing `.ends`.
    pub cards: Vec<ScopedCard>,
    /// Where the closing `.ends` was written.
    pub end_location: SourceLoc,
    /// Where the `.subckt` card was written.
    pub location: SourceLoc,
}

/// An `.include` or `.lib` directive.
#[derive(Debug, Clone, PartialEq)]
pub struct IncludeDirective {
    /// The file to read.
    pub path: String,
    /// Exact path token spelling, including quotes and escapes when supplied.
    pub path_spelling: String,
    /// Canonical path after file resolution; absent in syntax-only parsing.
    pub resolved_path: Option<PathBuf>,
    /// For `.lib`, the section to take from the file.
    pub section: Option<String>,
    /// Selected library boundaries, populated only by file resolution.
    pub selected_section: Option<LibrarySection>,
    /// Where the directive was written.
    pub location: SourceLoc,
}

/// Source boundaries of a selected `.lib name` … `.endl [name]` block.
#[derive(Debug, Clone, PartialEq)]
pub struct LibrarySection {
    /// Canonical section name.
    pub name: String,
    /// Positioned opening card, including its original spelling.
    pub opening: crate::card::RawCard,
    /// Positioned closing card.
    pub closing: crate::card::RawCard,
}

/// A `.param` card: one or more `name = expression` assignments in source
/// order (C's `inp_split_multi_param_lines()` splits them the same way).
/// Duplicates are kept; later assignments override earlier ones only during
/// evaluation (GitHub #15). Nothing here is evaluated or checked for
/// undefined references.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamCard {
    /// Ordered assignments; never empty for a parsed card.
    pub assignments: Vec<ParamAssignment>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// One `name = expression` pair from a `.param` card.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamAssignment {
    /// Parameter name, lowercased (numparam names are case-insensitive).
    pub name: String,
    /// Byte span of the name as written.
    pub name_span: crate::expr::SourceSpan,
    /// The unevaluated right-hand side with its original text and spans.
    pub expression: crate::expr::ParameterExpression,
}

/// An analysis request: which analysis, and its unparsed arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisCard {
    /// Which analysis.
    pub kind: AnalysisKind,
    /// The card's arguments, as written.
    pub arguments: Vec<String>,
    /// Parsed `{...}` arguments (unevaluated), by position in `arguments`.
    /// Other arguments stay opaque text.
    pub expressions: Vec<ArgumentExpression>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// A parsed `{...}` analysis argument.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentExpression {
    /// Index into [`AnalysisCard::arguments`] whose text is the braced form.
    pub index: usize,
    /// The unevaluated expression.
    pub expression: crate::expr::ParameterExpression,
}

/// A `.option`/`.options`/`.opt` card: ordered settings, duplicates preserved.
///
/// C: `inp2dot.c` hands the card to `INPdoOpts()` (`inpdoopt.c`), which applies
/// settings left to right. This AST validates syntax only; whether a name is a
/// supported option is decided by the run-configuration consumer.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionCard {
    /// Settings in source order. Repeats are retained; later settings override
    /// earlier ones when a consumer applies them in order.
    pub settings: Vec<OptionSetting>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// One `name=value` setter or bare flag from an option card.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionSetting {
    /// Option name, ASCII-lowercased (ngspice lowercases deck text).
    pub name: String,
    /// The value as written (numeric spelling or a bare word such as `gear`);
    /// `None` for a bare flag. Never evaluated or range-checked here.
    pub value: Option<PositionedValue>,
    /// Where the option name was written.
    pub location: SourceLoc,
}

/// A `.global` card: node names in written order, normalized like device nodes
/// (lowercased; `gnd` becomes `0` only when automatic gnd aliasing is on).
///
/// C: `collect_global_nodes()` in `frontend/subckt.c`, and `inpcom.c`, which
/// adds `.global gnd` unless `no_auto_gnd` is set. Ground `0` is always global.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalCard {
    /// Declared nodes, in order, duplicates retained.
    pub nodes: Vec<GlobalNode>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// One node named by a `.global` card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalNode {
    /// Canonical node name.
    pub name: NodeName,
    /// Where the node name was written.
    pub location: SourceLoc,
}

/// One ordered card in its owning scope. Indexes address that scope's typed
/// vectors, so semantic values are not duplicated. Source cards remain intact
/// for future serializers/snapshots; neither is implemented by this storage.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedCard {
    /// Semantic kind and scope-local index, where applicable.
    pub kind: ScopedCardKind,
    /// Positioned source spelling/tokens of this card.
    pub source: crate::card::RawCard,
    /// Include directives traversed, outermost first; empty for root cards.
    pub include_chain: Vec<SourceLoc>,
}

/// Scope-local semantic card references, including structural terminators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopedCardKind {
    /// Index into devices (X instances use designator `x` and `model` as target).
    Device(usize),
    /// Index into models.
    Model(usize),
    /// Index into nested subcircuit definitions.
    Subcircuit(usize),
    /// Index into analyses.
    Analysis(usize),
    /// Index into source directives; resolved content follows this entry.
    Include(usize),
    /// Index into [`Netlist::options`] (root scope only).
    Options(usize),
    /// Index into [`Netlist::globals`] (root scope only).
    Global(usize),
    /// Index into this scope's `.param` cards (`Netlist::params` at the root,
    /// `Subcircuit::params` in a body).
    Param(usize),
    /// End of a subcircuit body.
    Ends,
    /// End of a deck.
    End,
}

/// A semantic deck container. Scoped syntax and file resolution do not imply
/// flattening, parameter evaluation or simulation. `.param` cards are parsed
/// but unevaluated; `.option` cards are applied by `spice_analysis::RunConfig`,
/// not by the AST. See
/// `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and the central `TODO.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct Netlist {
    /// The deck's title line.
    pub title: String,
    /// The file the deck came from.
    pub path: PathBuf,
    /// Top-level device instances, in deck order.
    pub devices: Vec<DeviceInstance>,
    /// Top-level `.model` cards.
    pub models: Vec<ModelCard>,
    /// `.subckt` definitions.
    pub subcircuits: Vec<Subcircuit>,
    /// Requested analyses, in deck order.
    pub analyses: Vec<AnalysisCard>,
    /// `.include` and `.lib` directives.
    pub includes: Vec<IncludeDirective>,
    /// Top-level `.param` cards in deck order, unevaluated.
    pub params: Vec<ParamCard>,
    /// `.option` cards in deck order (root scope only; inside `.subckt` bodies
    /// they are rejected as not yet ported).
    pub options: Vec<OptionCard>,
    /// `.global` cards in deck order (root scope only).
    pub globals: Vec<GlobalCard>,
    /// All cards in source/expansion order, with scope-local typed indexes.
    pub cards: Vec<ScopedCard>,
    /// Where the deck started.
    pub location: SourceLoc,
}

impl Netlist {
    /// The analyses requested, in deck order.
    pub fn analysis_kinds(&self) -> impl Iterator<Item = AnalysisKind> + '_ {
        self.analyses.iter().map(|card| card.kind)
    }

    /// Number of top-level device instances, excluding subcircuit bodies.
    #[must_use]
    pub fn top_level_device_count(&self) -> usize {
        self.devices.len()
    }

    /// Whether `name` is a global node for subcircuit flattening: ground `0` is
    /// always global; other nodes are global only if a top-level `.global` card
    /// named them. Matching is ASCII case-insensitive on canonical names, so
    /// with automatic gnd aliasing `.global gnd` is the same as ground, while
    /// under `no_auto_gnd` `gnd` is a distinct ordinary global node.
    #[must_use]
    pub fn is_global_node(&self, name: &str) -> bool {
        name == "0"
            || self
                .globals
                .iter()
                .flat_map(|card| &card.nodes)
                .any(|node| node.name.eq_ignore_ascii_case(name))
    }

    /// Declared global nodes in first-declaration order without duplicates.
    /// Ground `0` is implicit and listed only if written (or aliased) explicitly.
    #[must_use]
    pub fn global_node_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for node in self.globals.iter().flat_map(|card| &card.nodes) {
            if !names.contains(&node.name.as_str()) {
                names.push(&node.name);
            }
        }
        names
    }

    /// Looks up a top-level device by instance name, case-insensitively.
    #[must_use]
    pub fn device(&self, name: &str) -> Option<&DeviceInstance> {
        self.devices
            .iter()
            .find(|device| device.name.eq_ignore_ascii_case(name))
    }

    /// Looks up a model by name, case-insensitively.
    #[must_use]
    pub fn model(&self, name: &str) -> Option<&ModelCard> {
        self.models
            .iter()
            .find(|model| model.name.eq_ignore_ascii_case(name))
    }

    /// Looks up a subcircuit by name, case-insensitively.
    #[must_use]
    pub fn subcircuit(&self, name: &str) -> Option<&Subcircuit> {
        self.subcircuits
            .iter()
            .find(|subcircuit| subcircuit.name.eq_ignore_ascii_case(name))
    }
}
