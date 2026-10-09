//! Analysis drivers: `.op`, `.dc`, `.ac`, `.tran`, `.tf` and friends.
//!
//! `.noise` runs [`crate::analysis::noise`] (two plots, see
//! [`Analysis::run_plots`]); `.disto` runs [`crate::analysis::disto`] (two
//! harmonic or three intermodulation plots).
//!
//! Ported from `src/spicelib/analysis/`, which is where ngspice's job control
//! lives: `CKTdoJob()` in `cktdojob.c` dispatches on the analysis, `dctran.c`
//! drives DC sweeps and transient analysis, `dcop.c` the operating point,
//! `acan.c` the AC and noise analyses. Around them sit the loading
//! (`CKTload`), iteration (`CKTiter`) and convergence machinery.
//!
//! Drivers support linear R/C/L/V/I equations. Nonlinear analyses remain
//! unsupported; an ordinary `.tran` runs the trap/Gear companion driver and
//! `backend=diffsol method=bdf` selects the BDF backend. The split
//! between [`AnalysisRequest`] and the netlist AST is deliberate: the driver
//! layer does not need to know where a request came from, and the AST does not
//! need to know which analyses exist.

use std::fmt;

use crate::devices::Circuit;
use crate::netlist::ast::AnalysisCard;
use crate::primitives::{AnalysisKind, Real, SourceLoc, SpiceError, SpiceResult};

use crate::analysis::C_REFERENCE_ANALYSIS;
use crate::analysis::results::Plot;

/// One evaluated `.ic` or `.nodeset` entry, `V(node)=value`, as the analyses
/// receive it: the canonical node name, the finite value in volts and where the
/// entry was written. Duplicates are kept in deck order; the last entry for a
/// node wins (C: `INPpas3()` overwrites the node's `ic`/`nodeset` in turn).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCondition {
    /// Canonical (lowercased) node name.
    pub node: String,
    /// Value in volts.
    pub value: Real,
    /// Where the `V(node)=value` entry started.
    pub location: SourceLoc,
}

/// A request to run an analysis.
///
/// Arguments are kept as written. Their grammar differs per analysis — `.tran 1u
/// 10u 0 0.1u`, `.ac dec 10 1 1meg`, `.dc v1 0 5 0.1` — so each driver
/// interprets them, exactly as the per-analysis C code does.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisRequest {
    /// Which analysis to run.
    pub kind: AnalysisKind,
    /// The card's arguments, as written. A `.tran` `uic` flag is **not** among
    /// them; see [`Self::uic`].
    pub arguments: Vec<String>,
    /// The `.tran` `uic` flag (use initial conditions), kept apart from the
    /// positional time arguments. The companion transient driver implements it
    /// (`dctran.c` `MODEUIC`); `backend=diffsol` rejects it explicitly.
    pub uic: bool,
    /// The deck's `.ic` entries in deck order (see [`NodeCondition`]). Enforced
    /// by the companion transient initial operating point unless `uic`; ignored
    /// by `.op`/`.dc`/`.ac`, exactly as C (`cktload.c`), but always validated.
    pub initial_conditions: Vec<NodeCondition>,
    /// The deck's `.nodeset` entries in deck order: convergence hints for the
    /// DC operating points. They cannot change the unique solution of a linear
    /// circuit; under transient `uic` C reuses them as initial node voltages.
    pub nodesets: Vec<NodeCondition>,
}

impl AnalysisRequest {
    /// A request with no arguments.
    #[must_use]
    pub fn new(kind: AnalysisKind) -> Self {
        Self {
            kind,
            arguments: Vec::new(),
            uic: false,
            initial_conditions: Vec::new(),
            nodesets: Vec::new(),
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
            uic: false,
            initial_conditions: Vec::new(),
            nodesets: Vec::new(),
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
            uic: card.uic,
            initial_conditions: Vec::new(),
            nodesets: Vec::new(),
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
    /// Junction minimum conductance in siemens (`.option gmin`, C `CKTgmin`),
    /// default [`crate::devices::DEFAULT_GMIN`]. See
    /// [`crate::devices::ModelContext::gmin`].
    pub gmin: Real,
}

impl AnalysisContext {
    /// Copy temperature and junction-gmin settings into the lower-level device
    /// context. Validation occurs at circuit elaboration/equation assembly; no
    /// global state is used.
    #[must_use]
    pub const fn model_context(&self) -> crate::devices::ModelContext {
        crate::devices::ModelContext::new(self.temperature, self.nominal_temperature)
            .with_gmin(self.gmin)
    }
}

impl Default for AnalysisContext {
    fn default() -> Self {
        Self {
            temperature: 27.0,
            nominal_temperature: 27.0,
            gmin: crate::devices::DEFAULT_GMIN,
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
    /// Invalid/unsupported requests, device/elaboration failures or numerical
    /// failures. Pending device functionality can return
    /// [`SpiceError::NotYetPorted`]; driver presence is not universal support.
    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot>;

    /// Runs the analysis and returns **every** plot it produces, in C's
    /// order. Only `.noise` produces more than one (the spectrum and the
    /// integrated noise); the default wraps [`Self::run`].
    ///
    /// # Errors
    ///
    /// As [`Self::run`].
    fn run_plots(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Vec<Plot>> {
        Ok(vec![self.run(circuit, request, context)?])
    }
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
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::linear::op(circuit, request, context)
    }
}

/// `.dc` — a bounded typed DC sweep of independent V/I sources, resistors
/// (literal or model-backed) and circuit temperature, with one optional nested
/// axis. See `docs/port/DC_SWEEPS.md`.
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
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::linear::dc(circuit, request, context)
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
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::ac::run(circuit, request, context)
    }
}

/// `.noise` — small-signal noise spectra and integrated noise
/// ([`crate::analysis::noise`], `docs/port/NOISE.md`).
///
/// C: `noisean.c`. [`Analysis::run_plots`] returns both of C's plots, the
/// `Noise Spectral Density Curves` and (unless the sweep is one frequency)
/// the `Integrated Noise`; [`Analysis::run`] returns the spectrum alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Noise;

impl Analysis for Noise {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::Noise
    }

    fn name(&self) -> &'static str {
        "noise"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        let mut plots = crate::analysis::noise::run(circuit, request, context)?;
        Ok(plots.swap_remove(0))
    }

    fn run_plots(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Vec<Plot>> {
        crate::analysis::noise::run(circuit, request, context)
    }
}

/// `.disto` — small-signal harmonic and intermodulation distortion
/// ([`crate::analysis::disto`], `docs/port/DISTORTION.md`).
///
/// C: `distoan.c`. [`Analysis::run_plots`] returns every plot C writes (the
/// 2nd and 3rd harmonics, or the three IM products with `f2overf1`);
/// [`Analysis::run`] returns the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Distortion;

impl Analysis for Distortion {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::Distortion
    }

    fn name(&self) -> &'static str {
        "distortion"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        let mut plots = crate::analysis::disto::run(circuit, request, context)?;
        Ok(plots.swap_remove(0))
    }

    fn run_plots(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Vec<Plot>> {
        crate::analysis::disto::run(circuit, request, context)
    }
}

/// `.tran` — the adaptive trapezoidal / Gear-2 companion driver by default
/// ([`crate::analysis::companion_transient`]), or the explicitly selected bounded diffsol
/// adaptive BDF (`backend=diffsol method=bdf`, not ngspice trap/Gear).
/// `.ic`/`uic` are implemented by the companion driver only; general DAEs remain unsupported.
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
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::transient::run(circuit, request, context)
    }
}

/// `.pz` — poles and zeros of the small-signal transfer function at the
/// operating point, as the finite eigenvalues of the drive-modified pencil
/// `A + s E` (`docs/port/POLE_ZERO_ADR.md`).
///
/// C: `pzan.c`, `cktpzset.c`, `cktpzld.c` (C searches with Muller's method,
/// `cktpzstr.c`; the port does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoleZero;

impl Analysis for PoleZero {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::PoleZero
    }

    fn name(&self) -> &'static str {
        "pole-zero"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::pz::run(circuit, request, context)
    }
}

/// `.tf` — the DC small-signal transfer function, input and output resistance
/// at the operating point (`src/analysis/tf.rs`; see `docs/port/TRANSFER_FUNCTION.md`).
///
/// C: `tfanal.c` (`TFanal`), reached through `CKTdoJob()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferFunction;

impl Analysis for TransferFunction {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::TransferFunction
    }

    fn name(&self) -> &'static str {
        "transfer function"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::tf::run(circuit, request, context)
    }
}

/// `.sp` — S-parameter analysis over the RF port sources (V sources with
/// `portnum`), producing S, Y and Z matrices. `donoise` is not ported.
///
/// C: `span.c` (an `RFSPICE` build option); see `docs/port/SPARAM.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SParameter;

impl Analysis for SParameter {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::SParameter
    }

    fn name(&self) -> &'static str {
        "S-parameter"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::sparam::run(circuit, request, context)
    }
}

/// `.sens` — DC or AC sensitivity of one output to every perturbable device
/// parameter, by C's finite-difference perturbation
/// (`src/analysis/sens.rs`; see `docs/port/SENSITIVITY.md`).
///
/// C: `cktsens.c` (`sens_sens`), `cktsgen.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sensitivity;

impl Analysis for Sensitivity {
    fn kind(&self) -> AnalysisKind {
        AnalysisKind::Sensitivity
    }

    fn name(&self) -> &'static str {
        "sensitivity"
    }

    fn run(
        &self,
        circuit: &mut Circuit,
        request: &AnalysisRequest,
        context: &AnalysisContext,
    ) -> SpiceResult<Plot> {
        crate::analysis::sens::run(circuit, request, context)
    }
}

/// Analyses with production drivers, for the devices and options documented
/// under `docs/port/`.
pub const DRIVERS: [AnalysisKind; 10] = [
    AnalysisKind::OperatingPoint,
    AnalysisKind::DcSweep,
    AnalysisKind::Ac,
    AnalysisKind::Transient,
    AnalysisKind::PoleZero,
    AnalysisKind::TransferFunction,
    AnalysisKind::SParameter,
    AnalysisKind::Noise,
    AnalysisKind::Distortion,
    AnalysisKind::Sensitivity,
];

/// Whether an analysis has a driver.
#[must_use]
pub fn has_driver(kind: AnalysisKind) -> bool {
    DRIVERS.contains(&kind)
}

/// What the port does with an analysis card (`spice-rs analyses`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisSupport {
    /// A production driver ([`runner`]).
    Driver,
    /// Not an analysis of its own: evaluated over the plot of `of`, as
    /// `.four` is over the final `.tran` plot ([`crate::analysis::fourier`]).
    PostProcessor {
        /// The analysis whose plot is post-processed.
        of: AnalysisKind,
    },
    /// No driver: the card is refused.
    Missing,
}

/// The port's support for an analysis kind.
#[must_use]
pub fn support(kind: AnalysisKind) -> AnalysisSupport {
    if has_driver(kind) {
        AnalysisSupport::Driver
    } else if kind == AnalysisKind::Fourier {
        AnalysisSupport::PostProcessor {
            of: AnalysisKind::Transient,
        }
    } else {
        AnalysisSupport::Missing
    }
}

/// The driver for an analysis.
///
/// # Errors
///
/// [`SpiceError::Unsupported`] for `.four`, which is not a driver but
/// a post-processor of the transient plot ([`support`]). See
/// `docs/port/ROADMAP.md`.
pub fn runner(kind: AnalysisKind) -> SpiceResult<Box<dyn Analysis>> {
    let driver: Box<dyn Analysis> = match kind {
        AnalysisKind::OperatingPoint => Box::new(OperatingPoint),
        AnalysisKind::DcSweep => Box::new(DcSweep),
        AnalysisKind::Ac => Box::new(AcSmallSignal),
        AnalysisKind::Transient => Box::new(Transient),
        AnalysisKind::PoleZero => Box::new(PoleZero),
        AnalysisKind::TransferFunction => Box::new(TransferFunction),
        AnalysisKind::SParameter => Box::new(SParameter),
        AnalysisKind::Noise => Box::new(Noise),
        AnalysisKind::Distortion => Box::new(Distortion),
        AnalysisKind::Sensitivity => Box::new(Sensitivity),
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
    use crate::devices::Circuit;
    use crate::netlist::ast::AnalysisCard;
    use crate::primitives::AnalysisKind;

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
        let kind = AnalysisKind::Fourier;
        let error = runner(kind).expect_err("no driver");
        assert!(
            !error.is_not_yet_ported(),
            "{kind:?} is out of scope, not pending"
        );
        assert!(error.to_string().contains(kind.as_str()));
        assert!(super::has_driver(AnalysisKind::Noise));
        assert!(super::has_driver(AnalysisKind::Sensitivity));
        assert!(super::has_driver(AnalysisKind::Distortion));
        assert!(super::has_driver(AnalysisKind::OperatingPoint));
        assert!(super::has_driver(AnalysisKind::TransferFunction));
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
        use crate::primitives::SourceLoc;
        use std::path::PathBuf;

        let card = AnalysisCard {
            kind: AnalysisKind::Ac,
            arguments: vec![
                "dec".to_owned(),
                "10".to_owned(),
                "1".to_owned(),
                "1meg".to_owned(),
            ],
            expressions: Vec::new(),
            uic: false,
            uic_location: None,
            location: SourceLoc::new(PathBuf::from("deck.cir"), 7, 1),
        };
        let request = AnalysisRequest::from(&card);
        assert_eq!(request.kind, AnalysisKind::Ac);
        assert_eq!(request.arguments.len(), 4);
        assert_eq!(request.argument(3), Some("1meg"));
    }
}
