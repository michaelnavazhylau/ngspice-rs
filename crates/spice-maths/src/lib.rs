//! Linear algebra and numerical integration for the port.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`dense`] | dense row-major matrix and vector storage | owned faer pivoted LU and checked solves |
//! | [`sparse`] | sparse triplet storage and petgraph coupling topology | owned faer sparse LU, symbolic reuse and checked solves |
//! | [`diffsol`] | bounded linear index-one DAE integration | adaptive BDF; diagonal mass and nonsingular algebraic block only |
//! | [`complex`] | complex sparse operators for AC | owned faer LU |
//! | [`integrator`] | trapezoidal and Gear companion coefficients, integration, prediction and truncation estimates | orders 1–2; trial coefficients separate from accepted step history |
//!
//! The C implementations are `src/maths/dense/`, `src/maths/sparse/`
//! (SPARSE 1.3, MIT licensed), `src/maths/KLU/` (LGPLv2 — see the licensing note
//! in `docs/port/MAPPING.md`) and `src/maths/ni/`. These are behavioral
//! references, not copied KLU algorithms: production LU/BDF uses MIT-licensed
//! faer/diffsol without native SuiteSparse/SUNDIALS. The locked graph requires
//! Rust 1.89. BDF is not ngspice trapezoidal or fixed Gear-2; see
//! `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and the central `TODO.md`.

#![warn(missing_docs)]

pub mod complex;
pub mod dense;
pub mod diffsol;
pub mod integrator;
pub mod linear;
pub use linear::{DenseLu, SparseLu, SparseSymbolic};
pub mod sparse;

pub use dense::{Matrix, Vector};
pub use integrator::{
    Coefficients, Companion, IntegrationMethod, StepHistory, TruncationTolerances,
};
pub use sparse::{SparseMatrix, Triplet};

/// The C reference for the dense solver, used in `NotYetPorted` errors.
pub const C_REFERENCE_DENSE: &str = "src/maths/dense/";

/// The C reference for the sparse solver, used in `NotYetPorted` errors.
pub const C_REFERENCE_SPARSE: &str = "src/maths/sparse/ (SPARSE 1.3), src/maths/KLU/";

/// The C reference for numerical integration, used in `NotYetPorted` errors.
pub const C_REFERENCE_INTEGRATION: &str = "src/maths/ni/niinteg.c, src/maths/ni/nicomcof.c";
