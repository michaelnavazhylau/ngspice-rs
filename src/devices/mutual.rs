//! K: mutual inductance between two or more inductors.
//!
//! C references: `src/spicelib/parser/inp2k.c` (card), `inp_compat()` in
//! `src/frontend/inpcom.c` (a card with more than two inductors becomes one
//! card per pair), and `src/spicelib/devices/ind/`: `mutsetup.c` (inductor
//! lookup), `muttemp.c` (`M = k sqrt(|L1 L2|)` and the inductive-system
//! check), `indload.c` (mutual flux), `mutacld.c` (AC) and `indtrunc.c`
//! (truncation error on the coupled flux).
//!
//! # Equations
//!
//! A K device stamps nothing itself. [`crate::devices::Circuit`] resolves the named
//! inductors, computes `M = k sqrt(|L1 L2|)` for every pair and hands each
//! inductor its [`MutualTerm`](crate::devices::MutualTerm)s, so that the inductor owns
//! its whole branch equation:
//!
//! ```text
//! v+ - v- = d/dt (L i + sum(M i_k))
//! ```
//!
//! - DC: coupled inductors remain shorts (no mutual term).
//! - AC: `-j omega M` between the two branch rows (`E` entries of the
//!   immutable `E x' + A x = b` assembly, so the explicit diffsol BDF backend
//!   integrates the same coupled mass matrix).
//! - Companion transient: the integrated quantity is the coupled flux, so the
//!   trap/Gear-2 history, the truncation-error estimate (`INDtrunc` on
//!   `INDflux`) and the `uic` starting flux `L ic + sum(M ic_k)` all include
//!   the mutual flux, exactly as `indload.c` accumulates it.
//!
//! # Validation (divergences from C are deliberate and listed)
//!
//! - A name that is not an instance in the circuit is an error, as in C
//!   (`MUTsetup`: "coupling to non-existent inductor"). A name that is an
//!   instance but not an inductor is an error too; C would read the other
//!   device's memory as an inductor (a crash or nonsense).
//! - Coupling an inductor to itself is rejected (C silently produces a zero
//!   solution).
//! - `MUTtemp` only *warns* when an inductive system (a connected group of
//!   coupled inductors) is not positive definite, exempting the case where
//!   every `|k| = 1`. The port rejects any system whose inductance matrix is
//!   not positive *semi*definite (a negative eigenvalue, beyond a rounding
//!   tolerance), which covers `|k| > 1`: such a system stores negative
//!   energy and its transient grows without bound. Semidefinite systems
//!   (ideal `|k| = 1` coupling) are accepted.
//! - Several K devices on the same pair are summed, as C's loads sum them; C
//!   warns ("has duplicate K instances") and checks only the last value, the
//!   port checks the summed matrix.
//! - A K card without a coupling value is a parse error (C silently uses 0).

use crate::netlist::ast::{DeviceInstance, ParameterKind};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number};

use crate::devices::linear::LinearContext;
use crate::devices::traits::{ControlReference, Device, MutualCoupling, StampContext};

/// The C references for diagnostics.
pub const C_REFERENCE: &str = "src/spicelib/parser/inp2k.c; src/spicelib/devices/ind/mutsetup.c, \
     muttemp.c, mutacld.c, indload.c";

/// A K device. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq)]
pub struct MutualInductance {
    name: String,
    inductors: Vec<ControlReference>,
    coefficient: Real,
    location: Option<SourceLoc>,
}

impl MutualInductance {
    /// A coupling of every pair of `inductors` with coefficient `k`.
    ///
    /// # Errors
    /// Fewer than two inductors, a nonfinite coefficient, or an inductor named
    /// twice (C would couple it to itself).
    pub fn new(
        name: impl Into<String>,
        inductors: Vec<ControlReference>,
        coefficient: Real,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if inductors.len() < 2 {
            return Err(SpiceError::circuit(format!(
                "{name}: a K device couples at least two inductors"
            )));
        }
        if !coefficient.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{name}: nonfinite coupling coefficient"
            )));
        }
        for (index, inductor) in inductors.iter().enumerate() {
            if inductors[..index]
                .iter()
                .any(|other| other.name.eq_ignore_ascii_case(&inductor.name))
            {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "{name} couples inductor {} to itself (C produces a zero solution)",
                        inductor.name
                    ),
                    location: inductor.location.clone(),
                });
            }
        }
        Ok(Self {
            name,
            inductors,
            coefficient,
            location: None,
        })
    }

    /// The coupled inductors' names, in card order.
    #[must_use]
    pub fn inductors(&self) -> &[ControlReference] {
        &self.inductors
    }

    /// The coupling coefficient `k`.
    #[must_use]
    pub const fn coefficient(&self) -> Real {
        self.coefficient
    }

    /// Where the card was written, if known.
    #[must_use]
    pub const fn location(&self) -> Option<&SourceLoc> {
        self.location.as_ref()
    }
}

impl Device for MutualInductance {
    /// Noiseless: C gives this device no noise routine (`DEVnoise = NULL`,
    /// `src/spicelib/devices/ind/indinit.c (mutual)`).
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> crate::primitives::SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        'k'
    }

    fn terminals(&self) -> &[crate::primitives::NodeId] {
        &[]
    }

    fn mutual_coupling(&self) -> Option<MutualCoupling<'_>> {
        Some(MutualCoupling {
            inductors: &self.inductors,
            coefficient: self.coefficient,
            location: self.location.as_ref(),
        })
    }

    /// The coupled inductors stamp the mutual terms in their own branch rows
    /// (they need the coupled flux for their companion history), so the K
    /// device itself contributes nothing in any analysis.
    fn stamp(&self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Ok(())
    }

    /// As [`Device::stamp`]: the inductors assemble the mutual `E` entries.
    fn assemble_linear(&self, _context: &mut LinearContext<'_>) -> SpiceResult<()> {
        Ok(())
    }

    /// Pole-zero load: C `mutpzld.c`, whose terms the coupled inductors stamp equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
}

/// Builds a K device from its parsed (and literalized) AST instance.
pub(crate) fn instantiate(instance: &DeviceInstance) -> SpiceResult<Box<dyn Device>> {
    if instance.model.is_some() || !instance.nodes.is_empty() {
        return Err(SpiceError::Unsupported {
            feature: format!(
                "{} takes inductor names, not nodes or a model",
                instance.name
            ),
            location: Some(instance.location.clone()),
        });
    }
    let mut inductors = Vec::new();
    let mut coefficient = None;
    for parameter in &instance.parameters {
        match (&parameter.kind, parameter.name.as_str()) {
            (ParameterKind::Instance, name) if name.starts_with("inductor") => {
                inductors.push(ControlReference {
                    name: parameter.value.to_ascii_lowercase(),
                    location: Some(parameter.location.clone()),
                });
            }
            (ParameterKind::Scalar, "coefficient" | "k") => {
                coefficient = Some(
                    parse_spice_number(&parameter.value)
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| SpiceError::Unsupported {
                            feature: format!(
                                "non-finite or nonliteral coupling coefficient {}",
                                parameter.value
                            ),
                            location: Some(parameter.location.clone()),
                        })?,
                );
            }
            (ParameterKind::Expression(_) | ParameterKind::Textual, _) => {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "non-literal {} parameter {}={} (expressions must be literalized first)",
                        instance.name, parameter.name, parameter.value
                    ),
                    location: Some(parameter.location.clone()),
                });
            }
            _ => {
                return Err(SpiceError::Unsupported {
                    feature: format!("{} parameter {}", instance.name, parameter.name),
                    location: Some(parameter.location.clone()),
                });
            }
        }
    }
    let Some(coefficient) = coefficient else {
        return Err(SpiceError::Unsupported {
            feature: format!(
                "{} has no coupling coefficient (C silently uses 0)",
                instance.name
            ),
            location: Some(instance.location.clone()),
        });
    };
    let mut device = MutualInductance::new(&instance.name, inductors, coefficient).map_err(
        |error| match error {
            SpiceError::Circuit { message } => SpiceError::Unsupported {
                feature: message,
                location: Some(instance.location.clone()),
            },
            other => other,
        },
    )?;
    device.location = Some(instance.location.clone());
    Ok(Box::new(device))
}
