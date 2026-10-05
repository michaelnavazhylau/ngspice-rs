//! Linear algebra and numerical integration for the port.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`dense`] | dense row-major matrix and vector storage | storage and multiply ported, solve stubbed |
//! | [`sparse`] | triplet storage for the sparse MNA matrix | storage ported, factor/solve stubbed |
//! | [`integrator`] | trapezoidal and Gear integration of charge-storage elements | types only |
//!
//! The C implementations are `src/maths/dense/`, `src/maths/sparse/`
//! (SPARSE 1.3, MIT licensed), `src/maths/KLU/` (LGPLv2 — see the licensing note
//! in `docs/port/MAPPING.md`) and `src/maths/ni/`.

#![warn(missing_docs)]

pub mod dense;
pub mod integrator;
pub mod sparse;

pub use dense::{Matrix, Vector};
pub use integrator::{IntegrationMethod, Integrator, Timestep};
pub use sparse::{SparseMatrix, Triplet};

/// The C reference for the dense solver, used in `NotYetPorted` errors.
pub const C_REFERENCE_DENSE: &str = "src/maths/dense/";

/// The C reference for the sparse solver, used in `NotYetPorted` errors.
pub const C_REFERENCE_SPARSE: &str = "src/maths/sparse/ (SPARSE 1.3), src/maths/KLU/";

/// The C reference for numerical integration, used in `NotYetPorted` errors.
pub const C_REFERENCE_INTEGRATION: &str = "src/maths/ni/niinteg.c, src/maths/ni/nicomcof.c";
