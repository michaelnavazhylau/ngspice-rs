//! Analysis drivers and result handling.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`results`] | [`Plot`], [`Variable`] and the flags that describe them | ported |
//! | [`rawfile`] | ngspice ASCII rawfile reading **and** writing | ported for ASCII; binary not ported |
//! | [`analysis`] | the [`Analysis`] trait and the `.op`/`.dc`/`.ac`/`.tran` drivers | linear DC/AC, adaptive trap/Gear-2 companion transient and explicitly selected bounded diffsol transient |
//!
//! The C equivalent is `src/spicelib/analysis/` (21,993 lines: the `CKT*`
//! job-control, loading and iteration machinery) and `src/frontend/rawfile.c`.
//!
//! ASCII rawfile I/O makes the port verifiable (binary I/O remains pending):
//! `xtask` captures ASCII rawfiles from C and the test suite parses them, so
//! the format the port must reproduce is pinned by data
//! rather than by prose. `cargo xtask golden verify` runs supported Rust fixtures
//! against that data without C. See `docs/port/VERIFICATION.md` and
//! `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and `docs/port/TRANSIENT.md` for
//! current limits. Bounded nonlinear D/Q/M DC/AC and charge-companion support is
//! documented in `docs/port/M4_NONLINEAR.md`; general physics/DAEs remain rejected; `.ic`/`.nodeset`/instance `ic=`/`uic` are implemented by the companion
//! transient driver only (`docs/port/TRANSIENT.md`); the CLI still inspects/parses only.

#![warn(missing_docs)]

mod ac;
pub mod analysis;
pub mod bias;
mod companion;
pub mod config;
mod initial;
mod linear;
pub mod newton;
pub mod rawfile;
pub mod results;
pub mod sweep;
mod transient;

pub use analysis::{
    Analysis, AnalysisContext, AnalysisRequest, DRIVERS, NodeCondition, has_driver, runner,
};
pub use companion::{TransientStats, companion_transient};
pub use config::{AppliedOption, DcOptions, RunConfig, RunOverrides, TransientSettings};
pub use rawfile::{RawFile, RawPlot};
pub use results::{Plot, PlotFlags, Variable};

/// The C reference for the analysis drivers, used in `NotYetPorted` errors.
pub const C_REFERENCE_ANALYSIS: &str = "src/spicelib/analysis/cktdojob.c, dctran.c, dcop.c, acan.c";
