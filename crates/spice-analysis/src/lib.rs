//! Analysis drivers and result handling.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`results`] | [`Plot`], [`Variable`] and the flags that describe them | ported |
//! | [`rawfile`] | ngspice ASCII rawfile reading **and** writing | ported for ASCII; binary not ported |
//! | [`analysis`] | the [`Analysis`] trait and the `.op`/`.dc`/`.ac`/`.tran` drivers | dispatch ported, drivers stubbed |//!
//! The C equivalent is `src/spicelib/analysis/` (21,993 lines: the `CKT*`
//! job-control, loading and iteration machinery) and `src/frontend/rawfile.c`.
//!
//! The rawfile layer is deliberately complete, because it is what makes the port
//! verifiable: `xtask` captures ASCII rawfiles from the C binary and the test
//! suite parses them, so the format the port must reproduce is pinned by data
//! rather than by prose. See `docs/port/VERIFICATION.md`.

#![warn(missing_docs)]

pub mod analysis;
pub mod rawfile;
pub mod results;

pub use analysis::{Analysis, AnalysisContext, AnalysisRequest, DRIVERS, has_driver, runner};
pub use rawfile::{RawFile, RawPlot};
pub use results::{Plot, PlotFlags, Variable};

/// The C reference for the analysis drivers, used in `NotYetPorted` errors.
pub const C_REFERENCE_ANALYSIS: &str = "src/spicelib/analysis/cktdojob.c, dctran.c, dcop.c, acan.c";
