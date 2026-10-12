//! Linear algebra and numerical integration for the port.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`dense`] | dense row-major matrix and vector storage | LU through the SPARSE port; Jacobi symmetric eigenvalues |
//! | [`sparse`] | sparse triplet storage and petgraph coupling topology | SPARSE 1.3 LU with pivot-order reuse |
//! | [`complex`] | complex sparse operators for AC | SPARSE 1.3 complex LU, pivot reuse across frequencies, determinant |
//! | [`dense_complex`] | small dense complex port matrices for `.sp` S/Y/Z conversion | Gauss-Jordan inverse with a rounding-aware singularity test |
//! | [`equilibration`] | explicit bounded row/column scaling | owned LU wrappers with original-unit residuals |
//! | [`pencil`] | roots of `det(A + s E)` for pole-zero analysis | port of ngspice's deflated Muller search (`cktpzstr.c`, `nipzmeth.c`) |
//! | [`integrator`] | trapezoidal and Gear companion coefficients, integration, prediction and truncation estimates | trapezoidal orders 1–2, variable-step Gear orders 1–6; trial coefficients separate from accepted step history |
//!
//! The C implementations are `src/maths/dense/`, `src/maths/sparse/`
//! (SPARSE 1.3, Berkeley permissive notice), `src/maths/KLU/` (LGPLv2 — see the
//! licensing note in `docs/port/MAPPING.md`) and `src/maths/ni/`. The sparse LU
//! is a Rust port of SPARSE 1.3 (`sparse13`); nothing is taken from KLU and no
//! external solver crate is used. See `docs/port/NATIVE_SOLVER_PROTOTYPE.md`.

pub mod complex;
pub use complex::DynamicGroup;
pub mod dense;
pub mod dense_complex;
pub mod equilibration;
pub mod integrator;
pub mod linear;
pub use linear::{
    DenseLu, NiStep, PivotTolerances, SparseLu, SparseSymbolic, solve_last_factors, solve_ni,
};
pub mod pencil;
pub mod sparse;
pub(crate) mod sparse13;

pub use dense::{Matrix, Vector};
pub use equilibration::{
    BackwardError, EquilibratedComplexLu, EquilibratedDenseLu, EquilibratedSparseLu, Equilibration,
};
pub use integrator::{
    Coefficients, Companion, IntegrationMethod, StepHistory, TruncationTolerances,
};
pub use sparse::{ElementId, Pattern, SparseMatrix, Triplet};

/// The C reference for the dense solver, used in `NotYetPorted` errors.
pub const C_REFERENCE_DENSE: &str = "src/maths/dense/";

/// The C reference for the sparse solver, used in `NotYetPorted` errors.
pub const C_REFERENCE_SPARSE: &str = "src/maths/sparse/ (SPARSE 1.3), src/maths/KLU/";

/// The C reference for numerical integration, used in `NotYetPorted` errors.
pub const C_REFERENCE_INTEGRATION: &str = "src/maths/ni/niinteg.c, src/maths/ni/nicomcof.c";

pub mod polynomial;
