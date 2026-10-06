//! Analysis drivers: `.op`, `.dc`, `.ac`, `.tran` and friends.
//!
//! Ported from `src/spicelib/analysis/`, which is where ngspice's job control
//! lives: `CKTdoJob()` in `cktdojob.c` dispatches on the analysis, `dctran.c`
//! drives DC sweeps and transient analysis, `dcop.c` the operating point,
//! `acan.c` the AC and noise analyses. Around them sit the loading
//! (`CKTload`), iteration (`CKTiter`) and convergence machinery.
//!
//! Drivers support linear R/C/L/V/I equations. Nonlinear analyses remain
//! unsupported; transient requires an explicit diffsol BDF selection. The split
//! between [`AnalysisRequest`] and the netlist AST is deliberate: the driver
//! layer does not need to know where a request came from, and the AST does not
//! need to know which analyses exist.

use std::fmt;

use spice_core::{AnalysisKind, Real, SpiceError, SpiceResult};
use spice_devices::Circuit;
use spice_netlist::ast::AnalysisCard;

use crate::C_REFERENCE_ANALYSIS;
use crate::results::Plot;

/// A request to run an analysis.
///
/// Arguments are kept as written. Their grammar differs per analysis — `.tran 1u
/// 10u 0 0.1u`, `.ac dec 10 1 1meg`, `.dc v1 0 5 0.1` — so each driver
/// interprets them, exactly as the per-analysis C code does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisRequest {
    /// Which analysis to run.
    pub kind: AnalysisKind,
    /// The card's arguments, as written.
    pub arguments: Vec<String>,
}

impl AnalysisRequest {
    /// A request with no arguments.
    #[must_use]
    pub fn new(kind: AnalysisKind) -> Self {
        Self {
            kind,
            arguments: Vec::new(),
        }
    }

    /// A request with arguments.
    #[must_use]
    pub fn with_arguments(
        kind: AnalysisKind,
        arguments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind,
            arguments: arguments.into_iter().map(Into::into).collect(),
        }
    }

    /// A positional argument.
    #[must_use]
    pub fn argument(&self, index: usize) -> Option<&str> {
        self.arguments.get(index).map(String::as_str)
    }

    /// A `name=value` argument, matched case-insensitively.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&str> {
        self.arguments.iter().find_map(|argument| {
            let (key, value) = argument.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then_some(value.trim())
        })
    }
}

impl From<&AnalysisCard> for AnalysisRequest {
    fn from(card: &AnalysisCard) -> Self {
        // The tokenizer separates '=' even without surrounding whitespace.
        // Keep positional arguments unchanged and normalize named triples for
        // the driver API. Malformed assignments remain visible for rejection.
        let mut arguments = Vec::new();
        let mut i = 0;
        while i < card.arguments.len() {
            if i + 2 < card.arguments.len() && card.arguments[i + 1] == "=" {
                arguments.push(format!("{}={}", card.arguments[i], card.arguments[i + 2]));
                i += 3;
            } else {
                arguments.push(card.arguments[i].clone());
                i += 1;
            }
        }
        Self {
            kind: card.kind,
            arguments,
        }
    }
}

/// Settings shared by every analysis.
///
/// Defaults match ngspice's, which prints `Doing analysis at TEMP = 27.000000
/// and TNOM = 27.000000` at the start of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalysisContext {
    /// Circuit temperature in degrees Celsius.
    pub temperature: Real,
    /// Nominal temperature at which model parameters were measured.
    pub nominal_temperature: Real,
}

impl Default for AnalysisContext {
    fn default() -> Self {
        Self {
            temperature: 27.0,
            nominal_temperature: 27.0,
        }
    }
}

/// An analysis driver.
pub trait Analysis: fmt::Debug {
    /// Which analysis this drives.
    fn kind(&self) -> AnalysisKind;

    /// A human-readable name, for diagnostics.
    fn name(&self) -> &'static str;

    /// Runs the analysis.
    ///
    /// # Errors
    ///
    /// Driver-specific failures, and [`SpiceError::NotYetPorted`] while the
    /// driver is a stub.
    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot>;
}

/// `.op` — the DC operating point.
///
/// C: `dcop.c`, reached through `CKTdoJob()` in `cktdojob.c`. The operating
/// point is also the starting point of every other analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatingPoint;

impl Analysis for OperatingPoint {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::OperatingPoint
    }

    fn name(&self) -> &'static str {
        "operating point"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        _context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::linear::op(circuit, request)
    }
}

/// `.dc` — a DC sweep of a source, a resistor or the temperature.
///
/// C: the DC transfer curve path in `dctran.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DcSweep;

impl Analysis for DcSweep {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::DcSweep
    }

    fn name(&self) -> &'static str {
        "DC sweep"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        _context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::linear::dc(circuit, request)
    }
}

/// `.ac` — the small-signal frequency sweep.
///
/// C: `acan.c`. The system is loaded at `omega = 2 pi f` and solved in the
/// complex domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcSmallSignal;

impl Analysis for AcSmallSignal {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::Ac
    }

    fn name(&self) -> &'static str {
        "AC small-signal"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        _context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::ac::run(circuit, request)
    }
}

/// `.tran` — transient analysis.
///
/// C: the transient path in `dctran.c`, plus the timestep control that lives
/// there and the integration in `src/maths/ni/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transient;

impl Analysis for Transient {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::Transient
    }

    fn name(&self) -> &'static str {
        "transient"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        _context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::transient::run(circuit, request)
    }
}

/// The analyses that have a driver, even if the driver is still a stub.
pub const DRIVERS: [AnalysisKind; 4] = [
    AnalysisKind::OperatingPoint,
    AnalysisKind::DcSweep,
    AnalysisKind::Ac,
    AnalysisKind::Transient,
];

/// Whether an analysis has a driver.
#[must_use]
pub fn has_driver(kind: AnalysisKind) -> bool {
    DRIVERS.contains(&kind)
}

/// The driver for an analysis.
///
/// # Errors
///
/// [`SpiceError::Unsupported`] for the analyses that are not on the roadmap yet
/// (`.noise`, `.disto`, `.pz`, `.sens`, `.tf`, `.four`). See
/// `docs/port/ROADMAP.md`.
pub fn runner(kind: AnalysisKind) -> SpiceResult<Box<dyn Analysis>> {
    let driver: Box<dyn Analysis> = match kind {
        AnalysisKind::OperatingPoint => Box::new(OperatingPoint),
        AnalysisKind::DcSweep => Box::new(DcSweep),
        AnalysisKind::Ac => Box::new(AcSmallSignal),
        AnalysisKind::Transient => Box::new(Transient),
        other => {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    ".{} analysis has no driver in the port yet (see {C_REFERENCE_ANALYSIS})",
                    other.as_str()
                ),
                location: None,
            });
        }
    };
    Ok(driver)
}

#[cfg(test)]
mod tests {
    use super::{Analysis, AnalysisContext, AnalysisRequest, DcSweep, runner};
    use spice_core::AnalysisKind;
    use spice_devices::Circuit;
    use spice_netlist::ast::AnalysisCard;

    #[test]
    fn every_planned_analysis_has_a_driver() {
        for kind in super::DRIVERS {
            let driver = runner(kind).expect("driver exists");
            assert_eq!(driver.kind(), kind);
            assert!(!driver.name().is_empty());
        }
    }

    #[test]
    fn analyses_off_the_roadmap_are_reported_as_unsupported() {
        for kind in [
            AnalysisKind::Noise,
            AnalysisKind::Distortion,
            AnalysisKind::PoleZero,
            AnalysisKind::Sensitivity,
            AnalysisKind::TransferFunction,
            AnalysisKind::Fourier,
        ] {
            let error = runner(kind).expect_err("no driver");
            assert!(
                !error.is_not_yet_ported(),
                "{kind:?} is out of scope, not pending"
            );
            assert!(error.to_string().contains(kind.as_str()));
        }
        assert!(!super::has_driver(AnalysisKind::Noise));
        assert!(super::has_driver(AnalysisKind::OperatingPoint));
    }

    #[test]
    fn invalid_analysis_inputs_are_reported() {
        let mut circuit = Circuit::new();
        let context = AnalysisContext::default();
        let error = super::OperatingPoint
            .run(
                &mut circuit,
                &AnalysisRequest::new(AnalysisKind::OperatingPoint),
                &context,
            )
            .expect_err("stub");
        assert!(error.to_string().contains("nonempty square"), "{error}");

        let error = DcSweep
            .run(
                &mut circuit,
                &AnalysisRequest::new(AnalysisKind::DcSweep),
                &context,
            )
            .expect_err("stub");
        assert!(error.to_string().contains(".dc requires"), "{error}");
    }

    #[test]
    fn context_defaults_match_ngspice() {
        let context = AnalysisContext::default();
        assert_eq!(context.temperature, 27.0);
        assert_eq!(context.nominal_temperature, 27.0);
    }

    #[test]
    fn requests_read_positional_and_named_arguments() {
        let request =
            AnalysisRequest::with_arguments(AnalysisKind::Transient, ["1u", "10u", "TSTOP=20u"]);
        assert_eq!(request.argument(0), Some("1u"));
        assert_eq!(request.argument(9), None);
        assert_eq!(request.named("tstop"), Some("20u"));
        assert_eq!(request.named("tstart"), None);
        assert_eq!(request.argument(1), Some("10u"));
    }

    #[test]
    fn requests_convert_from_netlist_cards() {
        use spice_core::SourceLoc;
        use std::path::PathBuf;

        let card = AnalysisCard {
            kind: AnalysisKind::Ac,
            arguments: vec![
                "dec".to_owned(),
                "10".to_owned(),
                "1".to_owned(),
                "1meg".to_owned(),
            ],
            location: SourceLoc::new(PathBuf::from("deck.cir"), 7, 1),
        };
        let request = AnalysisRequest::from(&card);
        assert_eq!(request.kind, AnalysisKind::Ac);
        assert_eq!(request.arguments.len(), 4);
        assert_eq!(request.argument(3), Some("1meg"));
    }
}
