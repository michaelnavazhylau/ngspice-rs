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

/// Largest number of instance parameters one [`crate::devices::ModelContext`]
/// can override at once (a two-axis `.dc` can sweep two `@inst[param]` targets).
pub const MAX_INSTANCE_OVERRIDES: usize = 2;

/// One per-point replacement of a device instance parameter, the immutable
/// form of C's `.dc @inst[param] ...` (`dctrcurv.c` `DCTsetInstParam`: the
/// device's `DEVparam` setter followed by `DEVtemperature`).
///
/// Created by [`crate::devices::Circuit::instance_override`], which resolves
/// the instance and the parameter through [`crate::devices::Device::instance_parameter`]
/// and validates the value by building the replacement once. Like
/// [`ResistorOverride`] it names the device by its ordinal in
/// [`crate::devices::Circuit::devices`] and is only meaningful for the circuit
/// that made it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstanceOverride {
    device: usize,
    parameter: &'static str,
    value: Real,
}

impl InstanceOverride {
    pub(crate) const fn new(device: usize, parameter: &'static str, value: Real) -> Self {
        Self {
            device,
            parameter,
            value,
        }
    }

    /// The overridden device's ordinal in [`crate::devices::Circuit::devices`].
    #[must_use]
    pub const fn device(&self) -> usize {
        self.device
    }

    /// The canonical (lowercase, alias-folded) parameter keyword.
    #[must_use]
    pub const fn parameter(&self) -> &'static str {
        self.parameter
    }

    /// The value given to the parameter, in the setter's own unit (Celsius for
    /// `temp`, as on the instance card).
    #[must_use]
    pub const fn value(&self) -> Real {
        self.value
    }
}

/// Settable real instance parameters of the C devices this port elaborates,
/// as `DCTfindInstParam` (`dctrcurv.c`) accepts them: `IF_SET | IF_REAL`
/// entries of each device's `*pTable` (`res.c`, `cap.c`, `ind.c`, `vsrc.c`,
/// `isrc.c`, `vcvs.c`, `vccs.c`, `cccs.c`, `ccvs.c`, `asrc.c`, `dio.c`,
/// `bjt.c`, `mos1.c`, `jfet.c`; K couplings live in `ind.c`'s `MUTpTable`; the S/W
/// switches have none). Aliases are listed separately.
///
/// Only used to tell a parameter C would sweep but this port does not yet
/// (`NotYetPorted`) from one C rejects as well (`Unsupported`).
#[must_use]
pub fn c_instance_parameter_known(designator: char, keyword: &str) -> bool {
    let keywords: &[&str] = match designator.to_ascii_lowercase() {
        'r' => &[
            "resistance",
            "r",
            "ac",
            "temp",
            "dtemp",
            "l",
            "w",
            "m",
            "tc",
            "tc1",
            "tc2",
            "tce",
            "bv_max",
            "scale",
        ],
        'c' => &[
            "capacitance",
            "cap",
            "c",
            "ic",
            "temp",
            "dtemp",
            "w",
            "l",
            "m",
            "tc1",
            "tc2",
            "bv_max",
            "scale",
        ],
        'l' => &[
            "inductance",
            "ic",
            "temp",
            "dtemp",
            "m",
            "tc1",
            "tc2",
            "scale",
            "nt",
        ],
        'k' => &["k", "coefficient"],
        'v' => &["dc", "acmag", "acphase", "z0", "pwr", "freq", "phase"],
        'i' => &["dc", "c", "m", "acmag", "acphase"],
        'e' | 'h' => &["gain"],
        'g' | 'f' => &["gain", "m"],
        'b' => &["temp", "dtemp", "tc1", "tc2", "m"],
        'd' => &[
            "temp", "dtemp", "ic", "area", "pj", "perim", "w", "l", "m", "lm", "lp", "wm", "wp",
        ],
        'q' => &[
            "icvbe", "icvce", "area", "areab", "areac", "m", "temp", "dtemp",
        ],
        'm' => &[
            "m", "l", "w", "ad", "as", "pd", "ps", "nrd", "nrs", "icvds", "icvgs", "icvbs", "temp",
            "dtemp",
        ],
        'j' => &["area", "m", "ic-vds", "ic-vgs", "temp", "dtemp"],
        _ => &[],
    };
    keywords.iter().any(|k| k.eq_ignore_ascii_case(keyword))
}

/// Domain check of a swept instance-parameter value: finite and `ok`,
/// otherwise a [`crate::primitives::SpiceError::Circuit`] naming the owner and
/// the requirement `what`.
pub(crate) fn check_swept(
    owner: &str,
    parameter: &str,
    value: Real,
    ok: bool,
    what: &str,
) -> crate::primitives::SpiceResult<()> {
    if ok && value.is_finite() {
        Ok(())
    } else {
        Err(crate::primitives::SpiceError::circuit(format!(
            "{owner}: swept {parameter}={value} must be {what}"
        )))
    }
}
