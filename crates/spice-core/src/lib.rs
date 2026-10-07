//! Core primitives shared by every layer of the ngspice Rust port.
//!
//! This crate deliberately has no dependencies, on crates.io or otherwise. It
//! holds the vocabulary the rest of the port is written in:
//!
//! - [`Real`] and [`Complex`] — the numeric types
//! - [`parse_spice_number`] / [`format_spice_number`] — SPICE literals and the
//!   scale factors that go with them
//! - [`NodeTable`] — node identity and the `gnd` → `0` aliasing rule
//! - [`SpiceError`] — the error vocabulary, including the
//!   [`SpiceError::NotYetPorted`] variant that every unimplemented entry point
//!   returns
//! - [`AnalysisKind`] — the taxonomy of analyses, shared between the netlist
//!   front-end and the analysis engine
//!
//! Behaviour documented here is ported from the C tree; the C file and function
//! that define it are named in each doc comment.

#![warn(missing_docs)]

pub mod error;
pub mod node;
pub mod value;

pub use error::{SourceLoc, SpiceError, SpiceResult};
pub use node::{GROUND_ALIAS, GROUND_NAME, Node, NodeId, NodeKind, NodeTable};
pub use value::{
    Complex, ParsedNumber, Real, approx_eq, format_spice_number, parse_spice_number,
    parse_spice_number_prefix,
};

/// The kinds of analysis ngspice can run.
///
/// Shared vocabulary: the netlist front-end records which analyses a deck asks
/// for, and the analysis engine dispatches on them. Mirrors the `.op`, `.dc`,
/// `.ac`, `.tran`, `.noise`, `.disto`, `.pz`, `.sens`, `.tf` and `.four` cards
/// handled throughout `src/spicelib/analysis/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnalysisKind {
    /// `.op` — DC operating point.
    OperatingPoint,
    /// `.dc` — DC sweep of a source, a resistor or the temperature.
    DcSweep,
    /// `.ac` — small-signal frequency sweep.
    Ac,
    /// `.tran` — transient analysis.
    Transient,
    /// `.noise` — noise analysis.
    Noise,
    /// `.disto` — distortion analysis.
    Distortion,
    /// `.pz` — pole-zero analysis.
    PoleZero,
    /// `.sens` — DC or AC sensitivity.
    Sensitivity,
    /// `.tf` — transfer function.
    TransferFunction,
    /// `.four` — Fourier analysis of a transient result.
    Fourier,
}

impl AnalysisKind {
    /// Every analysis kind, in the order ngspice documents them.
    pub const ALL: [Self; 10] = [
        Self::OperatingPoint,
        Self::DcSweep,
        Self::Ac,
        Self::Transient,
        Self::Noise,
        Self::Distortion,
        Self::PoleZero,
        Self::Sensitivity,
        Self::TransferFunction,
        Self::Fourier,
    ];

    /// The name without the leading dot, e.g. `"tran"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OperatingPoint => "op",
            Self::DcSweep => "dc",
            Self::Ac => "ac",
            Self::Transient => "tran",
            Self::Noise => "noise",
            Self::Distortion => "disto",
            Self::PoleZero => "pz",
            Self::Sensitivity => "sens",
            Self::TransferFunction => "tf",
            Self::Fourier => "four",
        }
    }

    /// Parses a card name, with or without its leading dot, case-insensitively.
    ///
    /// Returns `None` for a `.` command that is not an analysis.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.strip_prefix('.').unwrap_or(name);
        Self::ALL
            .into_iter()
            .find(|kind| name.eq_ignore_ascii_case(kind.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisKind;

    #[test]
    fn parses_card_names_case_insensitively() {
        assert_eq!(AnalysisKind::parse(".TRAN"), Some(AnalysisKind::Transient));
        assert_eq!(AnalysisKind::parse("tran"), Some(AnalysisKind::Transient));
        assert_eq!(
            AnalysisKind::parse(".op"),
            Some(AnalysisKind::OperatingPoint)
        );
        assert_eq!(AnalysisKind::parse(".model"), None);
    }

    #[test]
    fn as_str_round_trips() {
        for kind in AnalysisKind::ALL {
            assert_eq!(AnalysisKind::parse(kind.as_str()), Some(kind));
        }
    }
}
