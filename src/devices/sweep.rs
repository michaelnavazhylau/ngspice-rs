//! Physical resistor metadata and the immutable per-point resistor override
//! used by typed DC sweeps (`.dc r1 ...`, C `dctrcurv.c`).
//!
//! A sweep target is identified by what the device *is* ([`crate::devices::Device::resistor_metadata`]),
//! never by an instance-name prefix, so a programmatic resistor called `load`
//! is sweepable and a voltage source called `r9` is not.
//!
//! # Supplied scalar versus effective resistance
//!
//! The swept value replaces the resistor's **supplied scalar**: the primary
//! `r1 a b <value>` resistance, which for a model-backed resistor outranks the
//! model `r` and RSH/geometry (see `docs/port/PASSIVE_MODELS.md`). It is *not* the
//! final stamped resistance. The effective resistance is recomputed for every
//! point from the supplied scalar and the point's [`crate::devices::ModelContext`]:
//!
//! ```text
//! literal:       Reffective = supplied
//! model-backed:  Reffective = supplied * (1 + TC1*dT + TC2*dT^2) * scale / m
//! ```
//!
//! so temperature, TC1/TC2, `scale` and multiplicity `m` keep acting on a swept
//! model-backed resistor, exactly as they do on one written with that value in
//! the deck. Nothing is mutated: [`crate::devices::Circuit`] builds a disposable
//! [`crate::devices::Resistor`] with the effective value for the points that carry an
//! override, and every device, model recipe and AST value is left untouched.
//!
//! C reference: `dctrcurv.c` stores the sweep value as the instance resistance
//! (`RESresist`, marked given) and `restemp.c::RESupdate_conduct` then applies
//! temperature, TC, scale and `m`; the bounded Rust mapping is pinned by the
//! opt-in live comparisons in `tests/c_dc_sweep_reference.rs`
//! and by the analytic tests listed in `docs/port/DC_SWEEPS.md`.

use crate::primitives::Real;

/// Largest number of resistors one [`crate::devices::ModelContext`] can override at once
/// (a two-axis `.dc` can sweep two resistors).
pub const MAX_RESISTOR_OVERRIDES: usize = 2;

/// How a resistor was constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResistorOrigin {
    /// A literal `r1 a b <value>` resistor with no model.
    Literal,
    /// A model-backed resistor (`r1 a b rm ...`) with contextual temperature,
    /// TC, scale and multiplicity.
    ModelBacked,
}

/// Physical metadata of a sweepable resistor.
#[derive(Debug, Clone, PartialEq)]
pub struct ResistorMetadata {
    /// Literal or model-backed.
    pub origin: ResistorOrigin,
    /// The supplied scalar in ohms: the literal resistance, or the model-backed
    /// resistor's unadjusted base (no temperature, scale or multiplicity).
    pub supplied: Real,
    /// Positive parallel multiplier (1 for a literal resistor).
    pub multiplicity: Real,
}

/// One per-point replacement of a resistor's supplied scalar.
///
/// Created by [`crate::devices::Circuit::resistor_override`], which resolves the target
/// by physical metadata. It names the device by its position in
/// [`crate::devices::Circuit::devices`], so it is only meaningful for the circuit that
/// made it while that circuit's device list is unchanged; a stale or foreign
/// override is rejected when it is used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResistorOverride {
    device: usize,
    supplied: Real,
}

impl ResistorOverride {
    pub(crate) const fn new(device: usize, supplied: Real) -> Self {
        Self { device, supplied }
    }

    /// The overridden device's ordinal in [`crate::devices::Circuit::devices`].
    #[must_use]
    pub const fn device(&self) -> usize {
        self.device
    }

    /// The supplied scalar (ohms) used instead of the device's own.
    #[must_use]
    pub const fn supplied(&self) -> Real {
        self.supplied
    }
}
