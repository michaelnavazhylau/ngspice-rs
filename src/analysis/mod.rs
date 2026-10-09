//! Analysis drivers and result handling.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`results`] | [`Plot`], [`Variable`] and the flags that describe them | ported |
//! | [`selection`] | `.save`/`.print` output selection and its text rendering | ported for the bounded request subset in `docs/port/OUTPUT_SELECTION.md` |
//! | [`measure`] | `.measure`/`.meas` evaluation over the full plot and its text rendering | ported for the bounded operation subset in `docs/port/MEASURE.md` |
//! | [`fourier`] | `.four` Fourier/THD evaluation over the final complete period of a transient plot and its text rendering | ported for the bounded card subset in `docs/port/FOURIER.md` |
//! | [`rawfile`] | ngspice rawfile reading **and** writing, ASCII and binary | ported for the bounded layouts in `docs/port/RAWFILES.md` |
//! | [`batch`] | ngspice batch order, plot names and per-plot `.save`/`.print`/`.measure`/`.four` targeting for multi-analysis decks | ported (`docs/port/CLI.md`) |
//! | [`driver`] | the [`Analysis`] trait and the `.op`/`.dc`/`.ac`/`.tran`/`.tf`/`.sp` drivers | linear DC/AC, adaptive trap/Gear-2 companion transient and explicitly selected bounded diffsol transient |
//! | `tf` | `.tf` DC small-signal gain, input and output resistance at the bias point | ported (`tfanal.c`; see [`driver::TransferFunction`]) |
//! | `sparam` | `.sp` S-parameter analysis over RF port sources (S, Y, Z) | ported without `donoise` (`docs/port/SPARAM.md`) |
//!
//! The C equivalent is `src/spicelib/analysis/` (21,993 lines: the `CKT*`
//! job-control, loading and iteration machinery) and `src/frontend/rawfile.c`.
//!
//! ASCII rawfile I/O makes the port verifiable: `xtask` captures ASCII rawfiles
//! from C and the test suite parses them, so the format the port must reproduce
//! is pinned by data
//! rather than by prose. `cargo xtask golden verify` runs supported Rust fixtures
//! against that data without C. Binary rawfiles are read and written through the
//! byte-level API in [`rawfile`], whose layout, provenance and rejected variants
//! are documented in `docs/port/RAWFILES.md`. See `docs/port/VERIFICATION.md` and
//! `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and `docs/port/TRANSIENT.md` for
//! current limits. Bounded nonlinear D/Q/M DC/AC and charge-companion support is
//! documented in `docs/port/M4_NONLINEAR.md`; general physics/DAEs remain rejected; `.ic`/`.nodeset`/instance `ic=`/`uic` are implemented by the companion
//! transient driver only (`docs/port/TRANSIENT.md`); the CLI still inspects/parses only.

mod ac;
pub mod batch;
pub mod bias;
mod companion;
pub mod config;
pub mod driver;
pub mod fourier;
mod initial;
mod linear;
pub mod measure;
pub mod newton;
pub mod rawfile;
pub mod results;
pub mod selection;
mod sparam;
pub mod sweep;
mod tf;
mod transient;

pub use companion::{TransientStats, companion_transient};
pub use config::{
    AppliedOption, DcOptions, IgnoredOption, RunConfig, RunOverrides, TransientSettings,
};
pub use driver::{
    Analysis, AnalysisContext, AnalysisRequest, AnalysisSupport, DRIVERS, NodeCondition,
    has_driver, runner, support,
};
pub use fourier::{FourierAnalysis, FourierWindow, Harmonic};
pub use measure::{MeasureEvents, MeasureSpan, Measurement};
pub use rawfile::{BinaryByteOrder, RawFile, RawFileReader, RawFormat, RawPlot};
pub use results::{Plot, PlotFlags, Variable};
pub use selection::{Selection, print_requests, write_requests};

/// The C reference for the analysis drivers, used in `NotYetPorted` errors.
pub const C_REFERENCE_ANALYSIS: &str = "src/spicelib/analysis/cktdojob.c, dctran.c, dcop.c, acan.c";
