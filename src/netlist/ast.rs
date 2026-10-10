//! The semantic netlist model produced by the incremental parser.
//!
//! The parser, device registry and analyses share these types. The parser
//! constructs linear-device netlists, model cards, bounded D/Q/M flags/ICs and
//! numeric PULSE/PWL/SIN/EXP/SFFM/AM syntax, scoped subcircuits and resolved
//! sources. ngspice's parsing quirks are encoded at that boundary:
//!
//! - Parameter values are kept as **text**, not numbers. ngspice evaluates them
//!   with `INPevaluate()`/numparam and lets them depend on `.param` values and
//!   on `temp`, so evaluation is a separate pass. Current parser values are
//!   finite scalar literals, positioned waveform/IC/flag setters, and parsed
//!   but unevaluated `{...}`/`'...'` expressions ([`crate::netlist::expr`]), evaluated
//!   by [`crate::netlist::eval`]/[`crate::netlist::elaborate`]; waveform/IC-vector expressions
//!   remain pending.
//! - A device's connection nodes are not resolved to [`crate::primitives::NodeId`]s
//!   here; that happens when the circuit is built, so that subcircuit
//!   flattening can rewrite them.
//! - Analysis cards keep their arguments unparsed. The analysis drivers in
//!   `analysis` interpret them, because their grammars differ per card.

use std::path::PathBuf;

use crate::primitives::{AnalysisKind, Real, SourceLoc};

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
    /// finite literal nor a parsed expression (for instance a double-quoted
    /// string or an extended numeric spelling such as `4k7`).
    Textual,
    /// A `{...}` or single-quoted `'...'` expression, or a bare parameter name
    /// at an `X`/`.subckt` parameter site, parsed but **not evaluated**.
    /// [`ParameterAssignment::value`] keeps the original token spelling
    /// (braces or quotes included); the box holds the
    /// syntax tree and spans. Scalar consumers must treat this like any other
    /// non-scalar kind until an evaluation pass resolves it.
    Expression(Box<crate::netlist::expr::ParameterExpression>),
    /// A bare IF_FLAG keyword: C's INPgetValue supplies integer 1 without
    /// consuming a value. Explicit `flag=0`/`flag=1` forms are not accepted.
    Flag,
    /// Q/M IC values in C setter order, with omitted components left omitted.
    InitialConditions(Vec<InitialCondition>),
    /// A source waveform; analysis-dependent defaults are resolved by device elaboration.
    Waveform(SourceWaveform),
    /// A reference to another device instance by name (C `IF_INSTANCE`), such
    /// as the controlling voltage source of an F/H card (`control`, set by
    /// `INP2F`/`INP2H` before `INPdevParse`). [`ParameterAssignment::value`]
    /// holds the lowercased instance name as written; it is resolved to a
    /// branch row only after elaboration, and subcircuit expansion renames it
    /// like an instance name (`subckt.c`, `translate_inst_name`).
    Instance,
    /// A behavioural-source expression (C `IF_PARSETREE`): a B source's
    /// `v=`/`i=`, an E/G `VALUE=` (also spelled `vol=`/`cur=`) or the input
    /// expression of a `TABLE`. [`ParameterAssignment::value`] keeps the
    /// expression text; the box holds the syntax tree
    /// ([`crate::netlist::bexpr`]). Front-end elaboration substitutes `.param` names
    /// and expands `.func` calls but keeps this kind; the device evaluates it.
    Behavioural(Box<crate::netlist::bexpr::BehaviouralExpression>),
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
///
/// PWL `td=`/`r=` are separate ordered scalar setters (`VSRC_TD`/`VSRC_R` in
/// `vsrc.c`), not part of this value, exactly as C applies them.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceWaveform {
    /// Two required levels and up to six optional fields.
    Pulse(Box<PulseWaveform>),
    /// Strictly paired time/value arguments. No sorting or time repair occurs.
    Pwl(Vec<PwlPoint>),
    /// SIN/EXP/SFFM/AM: two required fields and a bounded optional prefix.
    Function(Box<FunctionWaveform>),
}

/// The analytic transient functions of `vsrcload.c`/`isrcload.c` that share
/// one "required pair plus optional prefix" vector shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFunction {
    /// `SIN(VO VA [FREQ [TD [THETA [PHASE]]]])` (keyword `sin` or `sine`).
    Sin,
    /// `EXP(V1 V2 [TD1 [TAU1 [TD2 [TAU2]]]])`.
    Exp,
    /// `SFFM(VO VA [FC [MDI [FM [TD [PHASEM [PHASEC]]]]]])`.
    Sffm,
    /// `AM(VO VMO [VMA [FM [FC [TD [PHASEM [PHASEC]]]]]])`, in the
    /// coefficient order `vsrcload.c` reads (`case AM`).
    Am,
}

impl SourceFunction {
    /// Field names in C coefficient order; the length is the maximum the
    /// runtime reads (extra fields are a parse error, not silently dropped).
    #[must_use]
    pub const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Sin => &["vo", "va", "freq", "td", "theta", "phase"],
            Self::Exp => &["v1", "v2", "td1", "tau1", "td2", "tau2"],
            Self::Sffm => &["vo", "va", "fc", "mdi", "fm", "td", "phasem", "phasec"],
            Self::Am => &["vo", "vmo", "vma", "fm", "fc", "td", "phasem", "phasec"],
        }
    }

    /// Canonical keyword (`sine` is an accepted alias of `sin`).
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Exp => "exp",
            Self::Sffm => "sffm",
            Self::Am => "am",
        }
    }
}

/// One SIN/EXP/SFFM/AM setter: its function and the supplied fields in C
/// coefficient order. Omitted trailing fields are simply absent; defaults that
/// depend on `.tran` (`CKTstep`, `CKTfinalTime`) belong to elaboration.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionWaveform {
    /// Which function.
    pub function: SourceFunction,
    /// Two to `function.fields().len()` positioned values.
    pub values: Vec<PositionedValue>,
}

/// `PULSE(V1 V2 [TD [TR [TF [PW [PER [NP]]]]]])`; omissions stay explicit.
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
    /// The eighth field (`PHASE` in `vsrcload.c`): in ngspice's default
    /// compatibility mode a positive value is the number of pulses.
    pub count: Option<PositionedValue>,
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
    /// `.func` definitions local to this body (visible to the body and to
    /// nested definitions, not outside).
    pub functions: Vec<FuncCard>,
    /// Options retained in the body; applied when instantiated.
    pub options: Vec<OptionCard>,
    /// Global declarations collected before instance node translation.
    pub globals: Vec<GlobalCard>,
    /// Instance-local initial conditions.
    pub initial_conditions: Vec<NodeHintCard>,
    /// Instance-local operating-point hints.
    pub nodesets: Vec<NodeHintCard>,
    /// Body output requests, translated for each instance.
    pub output: OutputCards,
    /// Body measurements, repeated without vector renaming as in C.
    pub measurements: Vec<MeasureCard>,
    /// Body Fourier requests, hoisted once per used definition without renaming.
    pub fourier: Vec<FourierCard>,
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
    pub opening: crate::netlist::card::RawCard,
    /// Positioned closing card.
    pub closing: crate::netlist::card::RawCard,
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
    pub name_span: crate::netlist::expr::SourceSpan,
    /// The unevaluated right-hand side with its original text and spans.
    pub expression: crate::netlist::expr::ParameterExpression,
}

/// A `.func name(p1, p2, ...) body` card: a user-defined numparam function.
///
/// C: `src/frontend/inpcom.c` (`inp_get_func_from_line()`,
/// `inp_expand_macro_in_str()`). Nothing here is evaluated; the definitions
/// of a scope are collected by [`crate::netlist::eval::FunctionScope`], which checks
/// recursion and arity, and calls are resolved during evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncCard {
    /// Function name, lowercased (numparam names are case-insensitive).
    pub name: String,
    /// Byte span of the name as written.
    pub name_span: crate::netlist::expr::SourceSpan,
    /// Formal parameters in order; may be empty (`.func f() {1}`). Names are
    /// distinct, lowercased and never a built-in function name.
    pub parameters: Vec<FuncParameter>,
    /// The unevaluated body (`{...}`, `'...'` or the bare rest of the card).
    pub body: crate::netlist::expr::ParameterExpression,
    /// Which card spelled the definition (kept for the writer only).
    pub spelling: FuncSpelling,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// How a [`FuncCard`] was written. Both spellings define the same function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FuncSpelling {
    /// `.func name(p1, p2) body`.
    #[default]
    Func,
    /// `.param name(p1, p2) = body`, which C rewrites to `.func`
    /// unconditionally (`inpcom.c` `inp_fix_macro_param_func_paren_io()`).
    Param,
}

/// One formal parameter of a [`FuncCard`].
#[derive(Debug, Clone, PartialEq)]
pub struct FuncParameter {
    /// Lowercased name.
    pub name: String,
    /// Byte span of the name as written.
    pub span: crate::netlist::expr::SourceSpan,
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
    /// Whether a `.tran` card carried the bare `uic` flag (C: `dot_tran()` in
    /// `inp2dot.c`, last token after `Tstep Tstop [Tstart [Tmax]]`). The word
    /// is **removed** from [`Self::arguments`], so it never appears as a stray
    /// positional argument. Always `false` for other analyses.
    pub uic: bool,
    /// Where the `uic` word was written, when [`Self::uic`] is set.
    pub uic_location: Option<SourceLoc>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// A parsed `{...}` analysis argument.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentExpression {
    /// Index into [`AnalysisCard::arguments`] whose text is the braced form.
    pub index: usize,
    /// The unevaluated expression.
    pub expression: crate::netlist::expr::ParameterExpression,
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
    /// The value as written (numeric spelling, a bare word such as `gear`, or
    /// the full `{expr}` / `'expr'` text); `None` for a bare flag. Never
    /// evaluated or range-checked here.
    pub value: Option<PositionedValue>,
    /// The parsed, unevaluated expression when the value was written as
    /// `{expr}` or `'expr'` (C: numparam substitutes both on `.option` lines);
    /// the run-configuration consumer evaluates it against top-level `.param`.
    pub expression: Option<Box<crate::netlist::expr::ParameterExpression>>,
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

/// An `.ic` or `.nodeset` card: `V(node)=value ...` entries in written order.
///
/// C: `INPpas3()` in `inppas3.c` walks these cards after the circuit exists
/// (`inp2dot.c` ignores them in pass 2). Entries are never deduplicated here:
/// a repeated node stays visible in order, and the consumer decides precedence
/// (in C each entry overwrites the node's `ic`/`nodeset` field in turn, so the
/// last one wins). This is syntax only; nothing is applied to a circuit, and
/// whether a node exists is decided when the circuit is built (C warns and
/// ignores unknown nodes; the analysis half must decide explicitly).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeHintCard {
    /// Entries in source order; never empty for a parsed card.
    pub entries: Vec<NodeHint>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// One `V(node)=value` entry of an `.ic`/`.nodeset` card.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeHint {
    /// Canonical node name (lowercased; `gnd` is `0` only with auto-gnd, and
    /// ground itself is rejected by the parser).
    pub node: NodeName,
    /// Where the node name was written.
    pub node_location: SourceLoc,
    /// The value, a finite literal or an unevaluated `{expression}`.
    pub value: NodeHintValue,
    /// Where the value was written.
    pub value_location: SourceLoc,
    /// Where the entry started (the `V` of `V(node)`).
    pub location: SourceLoc,
}

/// The value of a [`NodeHint`].
#[derive(Debug, Clone, PartialEq)]
pub enum NodeHintValue {
    /// A finite numeric literal, original spelling kept.
    Literal {
        /// Original spelling, e.g. `2.5m`.
        text: String,
        /// Parsed finite value (volts).
        value: Real,
    },
    /// A `{...}` expression, parsed but unevaluated until
    /// [`crate::netlist::elaborate::literalize`] resolves it against `.param` cards.
    Expression(Box<crate::netlist::expr::ParameterExpression>),
}

impl NodeHintValue {
    /// The finite value, if this is (or was literalized to) a literal.
    #[must_use]
    pub fn literal(&self) -> Option<Real> {
        match self {
            Self::Literal { value, .. } => Some(*value),
            Self::Expression(_) => None,
        }
    }
}

impl NodeHint {
    /// The finite value, unless the entry still holds an unevaluated expression.
    #[must_use]
    pub fn literal(&self) -> Option<Real> {
        self.value.literal()
    }
}

/// A `.save` card: deck-wide output-vector requests, in source order.
///
/// C: `ft_dotsaves()` in `src/frontend/dotcards.c` selects the deck's `.save`
/// lines and hands them to `com_save()` (`src/frontend/breakp2.c`), which stores
/// them in the `dbs` save list. A `.save` set applies to **every** analysis of
/// the deck; `.print` is analysis-specific ([`PrintCard`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SaveCard {
    /// Requests in source order. Duplicates stay visible here; the consumer
    /// collapses them (see `crate::analysis::selection`).
    pub requests: Vec<VectorRequest>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// A `.print` card: output-vector requests for **one** analysis, in source order.
///
/// C: `ft_savedotargs()` in `src/frontend/dotcards.c` reads the analysis name
/// after `.print` and registers the rest of the line for that analysis through
/// `com_save2()`.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintCard {
    /// Render a line-printer graph for `.plot`, rather than a table.
    pub ascii_plot: bool,
    /// The analysis the card names, e.g. `.print ac v(out)`.
    pub analysis: AnalysisKind,
    /// Where the analysis name was written.
    pub analysis_location: SourceLoc,
    /// Requests in source order. Duplicates stay visible here.
    pub requests: Vec<VectorRequest>,
    /// Where the `.print` card was written.
    pub location: SourceLoc,
}

/// One vector requested by a `.save` or `.print` card.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorRequest {
    /// What to write.
    pub vector: RequestedVector,
    /// Where the request starts: the function word of `v(out)`/`i(v1)`, or the
    /// `all` keyword.
    pub location: SourceLoc,
}

/// The bounded request grammar of `.save`/`.print`.
///
/// C: `com_save()`/`settrace()` in `src/frontend/breakp2.c` (`copynode()`
/// normalises `v(2)` to node `2` and `i(vds)` to the `vds#branch` vector) and
/// `fixem()` in `src/frontend/dotcards.c` (the `vm`/`vp`/`vr`/`vi`/`vdb` AC
/// component spellings). Device instance currents other than a source or
/// inductor branch current are **not** representable: see
/// `docs/port/OUTPUT_SELECTION.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestedVector {
    /// A named analysis vector (noise spectra or S/Y/Z parameters), optionally
    /// transformed with mag/ph/real/imag/db.
    Named {
        /// Lowercase vector name.
        name: String,
        /// Scalar component, when requested.
        component: Option<VectorComponent>,
    },
    /// `all`: keep the driver's whole vector set. C: `.save all`.
    All,
    /// `v(node)` or `v(first,second)`: a node voltage or a voltage difference.
    Voltage {
        /// The positive node.
        positive: NodeName,
        /// The negative node of a difference; `None` for a single node.
        negative: Option<NodeName>,
    },
    /// `i(device)`: the branch current of a voltage source or an inductor.
    Current {
        /// The instance name, lowercased.
        device: String,
    },
    /// `vm`/`vp`/`vr`/`vi`/`vdb` of `v(node)` or `v(first,second)`.
    Component {
        /// Which component.
        component: VectorComponent,
        /// The positive node.
        positive: NodeName,
        /// The negative node of a difference; `None` for a single node.
        negative: Option<NodeName>,
    },
}

/// An AC component spelling. C: `fixem()` in `src/frontend/dotcards.c`
/// (`vm(a,b)` becomes `mag(v(a)-v(b))`, `vp` `ph()`, `vr` `real()`, `vi`
/// `imag()`, `vdb` `db()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorComponent {
    /// `vm`: the magnitude. C's `mag()`.
    Magnitude,
    /// `vp`: the phase in radians in `(-pi, pi]`. C's `ph()`.
    Phase,
    /// `vr`: the real part. C's `real()`.
    Real,
    /// `vi`: the imaginary part. C's `imag()`.
    Imaginary,
    /// `vdb`: `20 log10` of the magnitude. C's `db()`.
    Decibels,
}

impl VectorComponent {
    /// The spelling as written on the card.
    #[must_use]
    pub const fn function(self) -> &'static str {
        match self {
            Self::Magnitude => "vm",
            Self::Phase => "vp",
            Self::Real => "vr",
            Self::Imaginary => "vi",
            Self::Decibels => "vdb",
        }
    }
}

impl RequestedVector {
    /// The canonical spelling of the request, for diagnostics and for the name
    /// of a computed column: `all`, `v(out)`, `v(in,out)`, `i(v1)`, `vm(out)`.
    #[must_use]
    pub fn name(&self) -> String {
        fn terminals(positive: &str, negative: Option<&str>) -> String {
            match negative {
                Some(negative) => format!("{positive},{negative}"),
                None => positive.to_owned(),
            }
        }
        match self {
            Self::Named { name, component } => match component {
                None => name.clone(),
                Some(c) => format!(
                    "{}({name})",
                    match c {
                        VectorComponent::Magnitude => "mag",
                        VectorComponent::Phase => "ph",
                        VectorComponent::Real => "real",
                        VectorComponent::Imaginary => "imag",
                        VectorComponent::Decibels => "db",
                    }
                ),
            },
            Self::All => "all".to_owned(),
            Self::Voltage { positive, negative } => {
                format!("v({})", terminals(positive, negative.as_deref()))
            }
            Self::Current { device } => format!("i({device})"),
            Self::Component {
                component,
                positive,
                negative,
            } => format!(
                "{}({})",
                component.function(),
                terminals(positive, negative.as_deref())
            ),
        }
    }

    /// True when the request is a node voltage difference. The single-node form
    /// has `negative == None`.
    #[must_use]
    pub const fn is_difference(&self) -> bool {
        match self {
            Self::Voltage { negative, .. } | Self::Component { negative, .. } => negative.is_some(),
            Self::All | Self::Current { .. } | Self::Named { .. } => false,
        }
    }
}

/// The `.save` and `.print` cards a deck contains, in deck order.
///
/// The parser returns these beside the [`Netlist`] rather than inside it: they
/// describe the *output* of an analysis and never reach the device elaboration,
/// so the netlist stays a description of the circuit. See
/// [`crate::netlist::Parser::parse_file_with_output`] and `docs/port/OUTPUT_SELECTION.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputCards {
    /// `.save` cards in deck order; they apply to every analysis.
    pub saves: Vec<SaveCard>,
    /// `.print` cards in deck order; each names one analysis.
    pub prints: Vec<PrintCard>,
}

impl OutputCards {
    /// True when the deck has no `.save` and no `.print` card.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.saves.is_empty() && self.prints.is_empty()
    }

    /// Number of output cards.
    #[must_use]
    pub fn len(&self) -> usize {
        self.saves.len() + self.prints.len()
    }
}

/// A `.measure`/`.meas` card: one named post-processing request over the plot.
///
/// C: `inp_spsource()` in `src/frontend/inp.c` removes the deck's `.measure`
/// lines and stores them in `ft_curckt->ci_meas`; after the run,
/// `do_measure()` (`src/frontend/measure.c`) hands each line to
/// `get_measure2()` (`src/frontend/com_measure2.c`). Measuring happens on the
/// **full** plot, before any `.save`/`.print` selection narrows what is
/// written: an operand the output selection dropped is still measurable. See
/// `docs/port/MEASURE.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureCard {
    /// The analysis the measurement applies to.
    pub analysis: AnalysisKind,
    /// Where the analysis name was written.
    pub analysis_location: SourceLoc,
    /// The result name, spelled as written (C prints it verbatim).
    pub name: String,
    /// Where the result name was written.
    pub name_location: SourceLoc,
    /// What to measure.
    pub request: MeasureRequest,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// The bounded operation of a [`MeasureCard`].
#[derive(Debug, Clone, PartialEq)]
pub enum MeasureRequest {
    /// A structurally validated card whose numeric setters need parameter evaluation.
    Deferred {
        /// Original positioned tokens; the same winnow grammar parses evaluated values.
        card: Box<crate::netlist::card::RawCard>,
        /// Ground-alias policy used when the card was parsed.
        auto_gnd: bool,
        /// Structurally checked request, for discovering observation operands.
        template: Box<MeasureRequest>,
    },
    /// Scalar numparam expression, evaluated after vector measurements.
    Parameter(Box<crate::netlist::expr::ParameterExpression>),
    /// FIND or DERIV at a threshold event or explicit axis position.
    AtEvent {
        /// Vector to evaluate.
        operand: VectorRequest,
        /// Locate the query.
        event: MeasureEvent,
        /// Differentiate a local quadratic instead of reading the vector.
        derivative: bool,
        /// Query bounds.
        window: MeasureWindow,
    },
    /// Axis position of a threshold crossing (`WHEN`).
    When {
        /// Threshold event.
        event: MeasureEvent,
        /// Search bounds.
        window: MeasureWindow,
    },
    /// `FIND <operand> AT=<value>`: the operand's value at one axis value.
    Find {
        /// The vector to read.
        operand: VectorRequest,
        /// The axis value to read it at.
        at: Real,
        /// Where the `AT=` setter was written.
        at_location: SourceLoc,
        /// The axis window the query must fall in.
        window: MeasureWindow,
    },
    /// `MIN`/`MAX`/`AVG`/`RMS`/`INTEG <operand> [FROM=…] [TO=…]`.
    Statistic {
        /// Which statistic.
        statistic: MeasureStatistic,
        /// The vector to reduce.
        operand: VectorRequest,
        /// The axis window to reduce over.
        window: MeasureWindow,
    },
    /// `TRIG <event> TARG <event>`: the axis distance between two events, as
    /// `targ - trig` (C's `AT_DELAY`; C also spells the operation `DELAY` and
    /// `TARG`, which the port does not accept).
    TrigTarg {
        /// The trigger event.
        trig: MeasureEvent,
        /// The target event.
        targ: MeasureEvent,
        /// The axis window the events are searched in.
        window: MeasureWindow,
    },
}

/// A whole-window reduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureStatistic {
    /// Phase margin in degrees at the first unity-gain crossing.
    PhaseMargin,
    /// Gain margin in dB at the first unwrapped -180 degree crossing.
    GainMargin,
    /// Axis value at the minimum.
    MinAt,
    /// Axis value at the maximum.
    MaxAt,
    /// Peak-to-peak range.
    PeakToPeak,
    /// `MIN`: the smallest operand value in the window.
    Min,
    /// `MAX`: the largest operand value in the window.
    Max,
    /// `AVG`: the operand averaged over the window.
    Avg,
    /// `RMS`: the root mean square of the operand over the window.
    Rms,
    /// `INTEG`/`INTEGRAL`: the operand integrated over the window.
    Integ,
}

impl MeasureStatistic {
    /// The spelling as written on the card.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PhaseMargin => "PHASE_MARGIN",
            Self::GainMargin => "GAIN_MARGIN",
            Self::MinAt => "MIN_AT",
            Self::MaxAt => "MAX_AT",
            Self::PeakToPeak => "PP",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Avg => "AVG",
            Self::Rms => "RMS",
            Self::Integ => "INTEG",
        }
    }
}

/// One `TRIG`/`TARG` event clause.
#[derive(Debug, Clone, PartialEq)]
pub enum MeasureEvent {
    /// Begin a threshold search at its own TD, independently of another event.
    Delayed {
        /// Event whose crossing count begins at TD.
        event: Box<MeasureEvent>,
        /// Lower search bound.
        td: Real,
    },
    /// `AT=<value>`: one axis value, with no operand.
    At {
        /// The axis value.
        at: Real,
        /// Where the `AT=` setter was written.
        location: SourceLoc,
    },
    /// `WHEN <operand>=<reference>`: crossing of two sampled vectors.
    VectorCrossing {
        /// Left side of the equality.
        operand: VectorRequest,
        /// Moving reference on the right side.
        reference: VectorRequest,
        /// Which crossing of left minus right through zero to take.
        transition: MeasureTransition,
    },
    /// `<operand> VAL=<value> [RISE=n|FALL=n|CROSS=n|LAST]`.
    Crossing {
        /// The vector whose threshold crossing is sought.
        operand: VectorRequest,
        /// The threshold value.
        value: Real,
        /// Where the `VAL=` setter was written.
        value_location: SourceLoc,
        /// Which crossing to take.
        transition: MeasureTransition,
    },
}

/// Which threshold crossing of an operand a measurement takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureTransition {
    /// No selector given: the first crossing, whatever its direction.
    First,
    /// `RISE=<n>`: the `n`-th rising crossing, `n >= 1`.
    Rise(u32),
    /// `FALL=<n>`: the `n`-th falling crossing, `n >= 1`.
    Fall(u32),
    /// `CROSS=<n>`: the `n`-th crossing in either direction, `n >= 1`.
    Cross(u32),
    /// `LAST` (or `RISE=LAST`/`FALL=LAST`/`CROSS=LAST`): the last crossing in
    /// either direction, as C's `MEASURE_LAST_TRANSITION` does.
    Last,
}

impl MeasureTransition {
    /// The selector's spelling, for diagnostics and for the `TRIG`/`TARG`
    /// crossings a measurement looks for.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::First => "first".to_owned(),
            Self::Last => "last".to_owned(),
            Self::Rise(n) => format!("RISE={n}"),
            Self::Fall(n) => format!("FALL={n}"),
            Self::Cross(n) => format!("CROSS={n}"),
        }
    }
}

/// The axis window a `.measure` request covers.
///
/// `None` bounds default to the ends of the plot's axis. The port requires
/// `from <= to` and treats a zero bound literally; C instead treats an upper
/// bound of `0` as "no upper bound" and swaps an inverted window for a `.dc`
/// measurement (`measure_parse_stdParams()` in `com_measure2.c`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeasureWindow {
    /// `FROM=<value>`: the lower bound, when written.
    pub from: Option<Real>,
    /// `TO=<value>`: the upper bound, when written.
    pub to: Option<Real>,
}

impl MeasureWindow {
    /// True when neither bound was written.
    #[must_use]
    pub const fn is_unbounded(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }
}

/// The highest harmonic index a [`FourierCard`] tabulates when the card writes
/// no `HARMONICS=`. C's `nfreqs` default of 10 rows is one DC row plus harmonics
/// `1..=9` (`fourier()` in `src/frontend/fourier.c`).
pub const DEFAULT_HARMONICS: u32 = 9;

/// The port's bounded harmonic count, and with it the resampling work budget:
/// harmonic `n` resamples the period onto `4 * max(n, 50)` subintervals, so the
/// widest grid this port builds is `4 * MAX_HARMONICS` subintervals per vector
/// (`400` subintervals, `401` samples). A larger `HARMONICS=` is
/// [`SpiceError::Unsupported`](crate::primitives::SpiceError::Unsupported) rather than a
/// silent clamp; C has no such bound (`set nfreqs=…`).
pub const MAX_HARMONICS: u32 = 100;

/// A `.four` card: Fourier amplitude/phase and THD of the **final complete
/// period** of a transient run.
///
/// C: `ft_dotsaves()` (`src/frontend/dotcards.c`) removes the deck's `.four`
/// lines (registering the named vectors for the `TRAN` plot so the transient
/// keeps them) and later hands each line to `fourier()`
/// (`src/frontend/fourier.c`), which transforms the last `nperiods / fundamental`
/// seconds of the `tran` plot it selects with `setcplot("tran")`. The port keeps
/// the typed request beside the netlist (`ParsedDeck::fourier`) and evaluates it
/// over the **full** plot, exactly as `.measure` does, so a `.save`/`.print`
/// selection never hides a transformed vector and a `.four` card never changes
/// the written rawfile. See `docs/port/FOURIER.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct FourierCard {
    /// A front-end `fourier` command uses C's configured half-open grid, even without `set`.
    pub frontend_command: bool,
    /// Unevaluated fundamental; resolve against the deck parameter scope before analysis.
    pub fundamental_expression: Option<Box<crate::netlist::expr::ParameterExpression>>,
    /// The fundamental frequency in hertz: a finite, strictly positive value.
    pub fundamental: Real,
    /// Where the fundamental frequency was written.
    pub fundamental_location: SourceLoc,
    /// The highest harmonic index tabulated: harmonics `1..=harmonics` are
    /// reported beside one DC row, i.e. `harmonics + 1` rows. C's `nfreqs` is
    /// `harmonics + 1`, C's row `0` is the DC component.
    pub harmonics: u32,
    /// Where the `HARMONICS=` value was written; `None` when the card wrote
    /// none and [`DEFAULT_HARMONICS`] was used.
    pub harmonics_location: Option<SourceLoc>,
    /// The vectors to transform, in source order: the `.save` spelling of one
    /// node voltage, voltage difference or source/inductor branch current each.
    pub vectors: Vec<VectorRequest>,
    /// Where the `.four` card was written.
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
    pub source: crate::netlist::card::RawCard,
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
    /// Index into this scope's options (`Netlist::options` or `Subcircuit::options`).
    Options(usize),
    /// Index into this scope's global declarations.
    Global(usize),
    /// Index into this scope's `.param` cards (`Netlist::params` at the root,
    /// `Subcircuit::params` in a body).
    Param(usize),
    /// Index into this scope's `.func` cards (`Netlist::functions` at the
    /// root, `Subcircuit::functions` in a body).
    Func(usize),
    /// Index into this scope's initial-condition cards.
    InitialCondition(usize),
    /// Index into this scope's nodeset cards.
    Nodeset(usize),
    /// A `.save` or root `.print`/`.plot` card. Body requests live in `Subcircuit::output`; root requests live in
    /// the [`OutputCards`] the parser returns beside the [`Netlist`]
    /// ([`crate::netlist::Parser::parse_file_with_output`]), so this card carries no
    /// scope-local index.
    Output,
    /// A `.measure`/`.meas` card. Body requests live in `Subcircuit::measurements`; root requests live in
    /// the [`MeasureCard`] list returned beside the [`Netlist`]
    /// ([`ParsedDeck::measurements`](crate::netlist::ParsedDeck::measurements)), so this
    /// card carries no scope-local index.
    Measure,
    /// A `.four` card. Body requests live in `Subcircuit::fourier`; root requests live in the
    /// [`FourierCard`] list returned beside the [`Netlist`]
    /// ([`ParsedDeck::fourier`](crate::netlist::ParsedDeck::fourier)), so this card
    /// carries no scope-local index.
    Fourier,
    /// End of a subcircuit body.
    Ends,
    /// End of a deck.
    End,
}

/// A semantic deck container. Scoped syntax and file resolution do not imply
/// flattening, parameter evaluation or simulation. `.param` cards are parsed
/// but unevaluated; `.option` cards are applied by `crate::analysis::RunConfig`,
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
    /// Top-level `.func` definitions in deck order, unevaluated.
    pub functions: Vec<FuncCard>,
    /// `.option` cards in deck order (root scope only; inside `.subckt` bodies
    /// they are rejected as not yet ported).
    pub options: Vec<OptionCard>,
    /// `.global` cards in deck order (root scope only).
    pub globals: Vec<GlobalCard>,
    /// `.ic` cards in deck order (root scope only; inside `.subckt` bodies
    /// they are rejected as not yet ported). Syntax only: not applied.
    pub initial_conditions: Vec<NodeHintCard>,
    /// `.nodeset` cards in deck order (root scope only). Syntax only.
    pub nodesets: Vec<NodeHintCard>,
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
                .global_node_names()
                .iter()
                .any(|node| node.eq_ignore_ascii_case(name))
    }

    /// Declared global nodes in first-declaration order without duplicates.
    /// Ground `0` is implicit and listed only if written (or aliased) explicitly.
    #[must_use]
    pub fn global_node_names(&self) -> Vec<&str> {
        let active = self.active_subcircuit_indices();
        let mut names: Vec<&str> = Vec::new();
        let cards = self.globals.iter().chain(
            self.subcircuits
                .iter()
                .enumerate()
                .filter(|(index, _)| active.contains(index))
                .flat_map(|(_, sub)| &sub.globals),
        );
        for node in cards.flat_map(|card| &card.nodes) {
            if !names.contains(&node.name.as_str()) {
                names.push(&node.name);
            }
        }
        names
    }

    /// Definitions reachable from root X instances, in declaration-index order.
    /// C removes unused bodies before collecting front-end declarations.
    pub(crate) fn active_subcircuit_indices(&self) -> std::collections::BTreeSet<usize> {
        // C inpcom removes unused definitions before collect_global_nodes.
        // A declaration in a reachable body applies throughout the deck.
        let mut graph = petgraph::graph::DiGraph::<Option<usize>, ()>::new();
        let root = graph.add_node(None);
        let mut definitions = std::collections::BTreeMap::new();
        for (index, sub) in self.subcircuits.iter().enumerate() {
            let node = graph.add_node(Some(index));
            definitions
                .entry(sub.name.to_ascii_lowercase())
                .or_insert(node);
        }
        for node in graph.node_indices().collect::<Vec<_>>() {
            let devices = graph[node].map_or(self.devices.as_slice(), |index| {
                &self.subcircuits[index].devices
            });
            for device in devices.iter().filter(|device| device.designator == 'x') {
                if let Some(target) = device
                    .model
                    .as_ref()
                    .and_then(|name| definitions.get(&name.to_ascii_lowercase()))
                {
                    graph.add_edge(node, *target, ());
                }
            }
        }
        let mut active = std::collections::BTreeSet::new();
        let mut walk = petgraph::visit::Dfs::new(&graph, root);
        while let Some(node) = walk.next(&graph) {
            if let Some(index) = graph[node] {
                active.insert(index);
            }
        }
        active
    }

    /// Every `.ic` entry in deck order (card order, then entry order),
    /// duplicates preserved: `(node, value, location)` are `entry.node`,
    /// `entry.value` and `entry.location`. Later entries for a node follow
    /// earlier ones; precedence is the consumer's decision. Values may still
    /// be unevaluated expressions; see
    /// [`crate::netlist::elaborate::ElaboratedNetlist::initial_conditions`] for finite
    /// values.
    pub fn initial_conditions(&self) -> impl Iterator<Item = &NodeHint> + '_ {
        self.initial_conditions
            .iter()
            .flat_map(|card| card.entries.iter())
    }

    /// Every `.nodeset` entry in deck order, duplicates preserved. A nodeset
    /// is a convergence hint, not a persistent constraint (C: `inppas3.c`
    /// stores it as the node's `nodeset`). See [`Self::initial_conditions`].
    pub fn nodesets(&self) -> impl Iterator<Item = &NodeHint> + '_ {
        self.nodesets.iter().flat_map(|card| card.entries.iter())
    }

    /// The first transient card that requested `uic`, if any.
    #[must_use]
    pub fn transient_uic(&self) -> Option<&AnalysisCard> {
        self.analyses
            .iter()
            .find(|card| card.kind == AnalysisKind::Transient && card.uic)
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
