//! The semantic netlist model produced by the incremental parser.
//!
//! The parser, device registry and analyses share these types. The parser
//! currently constructs linear-device netlists plus scalar model cards and
//! D/Q/M instances; subcircuits remain future work. ngspice's parsing quirks
//! are encoded at that boundary:
//!
//! - Parameter values are kept as **text**, not numbers. ngspice evaluates them
//!   with `INPevaluate()`/numparam and lets them depend on `.param` values and
//!   on `temp`, so evaluation is a separate pass. Current parser values are
//!   finite scalar literals; expression/parameter-reference syntax is pending.
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
/// order: C applies leading source DC and D/Q area after named assignments.
/// Duplicate assignments remain visible; consumers must apply them in order
/// rather than treating this vector as a map.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterAssignment {
    /// Parameter name, lowercased; ngspice matches parameter names
    /// case-insensitively.
    pub name: String,
    /// The value as written. The type can hold expressions/references for
    /// future elaboration, but the parser currently accepts scalar literals only.
    pub value: String,
    /// Where the assignment was found.
    pub location: SourceLoc,
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
    /// Formal parameters declared after `params:`.
    pub parameters: Vec<ParameterAssignment>,
    /// Devices in the body.
    pub devices: Vec<DeviceInstance>,
    /// Models declared in the body.
    pub models: Vec<ModelCard>,
    /// Where the `.subckt` card was written.
    pub location: SourceLoc,
}

/// An `.include` or `.lib` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeDirective {
    /// The file to read.
    pub path: String,
    /// For `.lib`, the section to take from the file.
    pub section: Option<String>,
    /// Where the directive was written.
    pub location: SourceLoc,
}

/// A `.param` card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamCard {
    /// Parameter name.
    pub name: String,
    /// The expression that defines it.
    pub expression: String,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// An analysis request: which analysis, and its unparsed arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisCard {
    /// Which analysis.
    pub kind: AnalysisKind,
    /// The card's arguments, as written.
    pub arguments: Vec<String>,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// A `.option` card, or a single option from one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionCard {
    /// The card's text, minus the leading `.option(s)`.
    pub raw: String,
    /// Where the card was written.
    pub location: SourceLoc,
}

/// A semantic deck container. The parser currently fills only its bounded
/// scalar device/model/analysis subset; scope/include/parameter/option/global
/// fields do not imply implemented parsing or elaboration. See
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
    /// `.param` values.
    pub params: Vec<ParamCard>,
    /// `.option` cards.
    pub options: Vec<OptionCard>,
    /// `.global` node names.
    pub globals: Vec<NodeName>,
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
