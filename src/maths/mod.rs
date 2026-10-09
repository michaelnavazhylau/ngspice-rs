//! Linear algebra and numerical integration for the port.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`dense`] | dense row-major matrix and vector storage | owned faer pivoted LU and checked solves |
//! | [`sparse`] | sparse triplet storage and petgraph coupling topology | owned faer sparse LU, symbolic reuse and checked solves |
//! | [`diffsol`] | bounded linear index-one DAE integration | adaptive BDF; floating/coupled mass blocks via block-SVD nullspaces; higher-index pencils rejected |
//! | [`complex`] | complex sparse operators for AC | owned faer LU |
//! | [`dense_complex`] | small dense complex port matrices for `.sp` S/Y/Z conversion | Gauss-Jordan inverse with a rounding-aware singularity test |
//! | [`equilibration`] | explicit bounded row/column scaling | owned LU wrappers with original-unit residuals |
//! | [`pencil`] | finite roots of a regular real pencil `det(A + s E)` for pole-zero analysis | SVD deflation of infinite eigenvalues, then faer QZ ([POLE_ZERO_ADR.md](../../docs/port/POLE_ZERO_ADR.md)) |
//! | [`integrator`] | trapezoidal and Gear companion coefficients, integration, prediction and truncation estimates | trapezoidal orders 1–2, variable-step Gear orders 1–6; trial coefficients separate from accepted step history |
//!
//! The C implementations are `src/maths/dense/`, `src/maths/sparse/`
//! (SPARSE 1.3, MIT licensed), `src/maths/KLU/` (LGPLv2 — see the licensing note
//! in `docs/port/MAPPING.md`) and `src/maths/ni/`. These are behavioral
//! references, not copied KLU algorithms: production LU/BDF uses MIT-licensed
//! faer/diffsol without native SuiteSparse/SUNDIALS. The locked graph requires
//! Rust 1.89. BDF is not ngspice trapezoidal or fixed Gear-2; see
//! `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and the central `TODO.md`.

pub mod complex;
pub mod dense;
pub mod dense_complex;
pub mod diffsol;
pub mod equilibration;
pub mod integrator;
pub mod linear;
pub use linear::{DenseLu, SparseLu, SparseSymbolic};
pub mod pencil;
pub mod sparse;

pub use dense::{Matrix, Vector};
pub use equilibration::{
    BackwardError, EquilibratedComplexLu, EquilibratedDenseLu, EquilibratedSparseLu, Equilibration,
};
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
