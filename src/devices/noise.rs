//! Device noise generators for small-signal `.noise` analysis.
//!
//! C reference: every noisy device has a `DEVnoise` entry (`resnoise.c`,
//! `dionoise.c`, `bjtnoise.c`, `mos1noi.c`, `swnoise.c`, `cswnoise.c`) that
//! names its generators and evaluates them through `NevalSrc`/
//! `NevalSrcInstanceTemp` (`src/spicelib/analysis/nevalsrc.c`): every generator
//! is a current source between two nodes whose power spectral density is
//!
//! | kind | density (A²/Hz) | C |
//! | --- | --- | --- |
//! | thermal | `4 k T g` | `THERMNOISE` |
//! | shot | `2 q abs(I)` | `SHOTNOISE` |
//! | flicker | `coefficient / f^exponent` | `N_GAIN` times the device's own law |
//!
//! and the output noise it causes is that density times the squared magnitude
//! of the transfer impedance from the generator's node pair to the output, which
//! the analysis obtains from one adjoint solve per frequency
//! (`crate::analysis` `.noise`, C `NInzIter`).
//!
//! The port splits C's single `DEVnoise` callback in two:
//!
//! * a device describes its generators **at the bias point** through
//!   [`crate::devices::Device::noise`], returning a [`DeviceNoise`]: no
//!   frequency, adjoint vector or integration state is visible to it. A device
//!   that is noiseless in C (`DEVnoise = NULL`: C, L, V, I, E/F/G/H, K, B)
//!   returns [`DeviceNoise::Noiseless`]; the trait default is an explicit error,
//!   so a device whose noise is not ported can never be silently omitted;
//! * the analysis owns gains, densities, the `Nintegrate` integration history
//!   and the output naming (`onoise_<inst><suffix>`), and orders the instances
//!   as C's `CKTnoise` visits them ([`circuit_noise`]).
//!
//! The same pattern (a pure bias-point description returned by a default
//! erroring trait method, evaluated by the analysis) is meant for later
//! small-signal analyses with per-device hooks.

use crate::devices::{Circuit, MnaUnknowns, ModelContext, SourceKind};
use crate::maths::Vector;
use crate::primitives::{NodeId, Real, SpiceError, SpiceResult};

/// ngspice `CONSTboltz` (J/K).
pub const BOLTZMANN: Real = 1.38064852e-23;
/// ngspice `CHARGE` (C).
pub const CHARGE: Real = 1.6021766208e-19;
/// ngspice `CONSTCtoK`.
pub const CELSIUS_TO_KELVIN: Real = 273.15;

/// The noisy C device types, in the order of C's `DEVices[]` table
/// (`src/spicelib/devices/dev.c`), which is the order `CKTnoise` visits them
/// and therefore the order of the per-device output columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoiseFamily {
    /// `bjt` (`bjtnoise.c`).
    Bjt,
    /// `csw`, the W switch (`cswnoise.c`).
    CurrentSwitch,
    /// `dio` (`dionoise.c`).
    Diode,
    /// `mos1` (`mos1noi.c`).
    Mos1,
    /// `mos3` (`mos3noi.c`).
    Mos3,
    /// `res` (`resnoise.c`).
    Resistor,
    /// `sw`, the S switch (`swnoise.c`).
    VoltageSwitch,
}

/// The physical law of one noise generator, evaluated at the bias point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoiseKind {
    /// Thermal noise of a conductance: `4 k T g` (C `THERMNOISE`).
    Thermal {
        /// Conductance in siemens.
        conductance: Real,
        /// Noise temperature in kelvin: the instance temperature for
        /// `NevalSrcInstanceTemp`, the circuit temperature for `NevalSrc`.
        temperature: Real,
    },
    /// Shot noise of a DC current: `2 q abs(I)` (C `SHOTNOISE`).
    Shot {
        /// DC current in amperes.
        current: Real,
    },
    /// Flicker noise: `coefficient / f^exponent` (C `N_GAIN` times the
    /// device's own 1/f law).
    Flicker {
        /// The density at 1 Hz, A²/Hz.
        coefficient: Real,
        /// The frequency exponent (`1`, or a model's `AF`/`EF`).
        exponent: Real,
    },
}

impl NoiseKind {
    /// The output-referred density `gain * density(f)`, in C's evaluation
    /// order (`nevalsrc.c`), for a squared transfer magnitude `gain`.
    #[must_use]
    pub fn output_density(&self, gain: Real, frequency: Real) -> Real {
        match *self {
            Self::Thermal {
                conductance,
                temperature,
            } => gain * 4.0 * BOLTZMANN * temperature * conductance,
            Self::Shot { current } => gain * 2.0 * CHARGE * current.abs(),
            Self::Flicker {
                coefficient,
                exponent,
            } => {
                let power = if exponent == 1.0 {
                    frequency
                } else {
                    frequency.powf(exponent)
                };
                gain * coefficient / power
            }
        }
    }

    fn validate(&self) -> bool {
        match *self {
            Self::Thermal {
                conductance,
                temperature,
            } => conductance.is_finite() && temperature.is_finite() && temperature > 0.0,
            Self::Shot { current } => current.is_finite(),
            Self::Flicker {
                coefficient,
                exponent,
            } => coefficient.is_finite() && exponent.is_finite(),
        }
    }
}

/// One noise generator: a current source between `nodes` (C's `node1`,
/// `node2` of `NevalSrc`), named by `suffix` after the instance name
/// (`onoise_<instance><suffix>`, e.g. `_thermal`, `_rb`, `_1overf`).
#[derive(Debug, Clone, PartialEq)]
pub struct NoiseSource {
    /// C's generator name suffix, including the leading underscore.
    pub suffix: &'static str,
    /// The node pair the generator connects (the order does not matter).
    pub nodes: [NodeId; 2],
    /// The generator's law at the bias point.
    pub kind: NoiseKind,
}

impl NoiseSource {
    /// A generator.
    #[must_use]
    pub const fn new(suffix: &'static str, nodes: [NodeId; 2], kind: NoiseKind) -> Self {
        Self {
            suffix,
            nodes,
            kind,
        }
    }
}

/// What a device contributes to `.noise`.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceNoise {
    /// No generators and no output columns: C's `DEVnoise = NULL` devices, and
    /// a resistor with `noisy=0` (`resnoise.c` skips it entirely).
    Noiseless,
    /// Generators in C's output order, optionally followed by the instance
    /// total the analysis computes (C's empty suffix).
    Sources {
        /// The C device type, for C's visiting order.
        family: NoiseFamily,
        /// The instance's model name, `None` for C's default model of the
        /// type (a literal resistor). C visits the models of one type in
        /// reverse order of their creation and the instances of one model in
        /// reverse order of theirs ([`circuit_noise`]).
        model: Option<String>,
        /// Whether C reports an instance total (empty suffix) after the
        /// generators: every multi-generator device does; the single-generator
        /// S/W switches name their generator with the empty suffix instead.
        total: bool,
        /// The generators, at least one.
        sources: Vec<NoiseSource>,
    },
}

/// What [`crate::devices::Device::noise`] sees: the converged operating point
/// the small-signal analysis linearizes around.
#[derive(Debug, Clone, Copy)]
pub struct NoiseContext<'a> {
    /// Circuit/nominal temperature and junction `gmin`.
    pub model_context: &'a ModelContext,
    /// Node numbering, with ground eliminated.
    pub unknowns: &'a MnaUnknowns,
    /// The operating-point solution.
    pub bias: &'a Vector,
    /// This device's state slots at the small-signal load (C `CKTstate0` at
    /// `MODEINITSMSIG`), when the analysis supplies them.
    pub states: Option<&'a [Real]>,
    /// Branch rows of the devices named by
    /// [`crate::devices::Device::controlling_sources`].
    pub controls: &'a [usize],
}

impl NoiseContext<'_> {
    /// The operating-point voltage of `node`, zero at ground.
    #[must_use]
    pub fn voltage(&self, node: NodeId) -> Real {
        self.unknowns
            .node_row(node)
            .and_then(|row| self.bias.get(row))
            .unwrap_or(0.0)
    }

    /// The circuit temperature in kelvin (C `CKTtemp`).
    #[must_use]
    pub fn circuit_kelvin(&self) -> Real {
        self.model_context.temperature + CELSIUS_TO_KELVIN
    }
}

/// An independent source as a small-signal input reference (the `.noise`
/// input source; C checks `VSRCacGiven`/`ISRCacGiven`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSource {
    /// Voltage or current source.
    pub kind: SourceKind,
    /// Whether the card gave an `ac` value (even `ac 0`).
    pub ac_given: bool,
}

/// One noisy instance, with its generators, in C's visiting order.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceNoise {
    /// The device ordinal in [`Circuit::devices`].
    pub device: usize,
    /// The instance name, as written in output names.
    pub name: String,
    /// The generators (without the instance total).
    pub sources: Vec<NoiseSource>,
    /// Whether the instance total follows the generators.
    pub total: bool,
}

/// Every noisy instance of `circuit` at the operating point `bias`, in the
/// order C's `CKTnoise` visits them: by device type in `DEVices[]` order
/// ([`NoiseFamily`]), then by model in reverse order of model creation (C
/// creates a model when the first instance referring to it is parsed and
/// prepends it to the type's list), then by instance in reverse deck order
/// (instances are prepended to their model's list as well).
///
/// `state` is the full small-signal state vector (see [`NoiseContext::states`]).
///
/// # Errors
///
/// Stale numbering, a bias or state of the wrong size, a device whose noise
/// is not ported (the [`crate::devices::Device::noise`] default), invalid
/// device physics at the bias, or a nonfinite generator.
pub fn circuit_noise(
    circuit: &Circuit,
    context: &ModelContext,
    bias: &Vector,
    state: Option<&[Real]>,
) -> SpiceResult<Vec<InstanceNoise>> {
    if bias.len() != circuit.unknown_count() || !bias.is_finite() {
        return Err(SpiceError::circuit("invalid noise bias dimensions/values"));
    }
    if state.is_some_and(|state| state.len() != circuit.state_len()) {
        return Err(SpiceError::circuit(
            "noise bias state does not match the circuit numbering",
        ));
    }
    // (family, model creation rank, instance rank, entry)
    let mut models: Vec<(NoiseFamily, Option<String>)> = Vec::new();
    let mut entries: Vec<(NoiseFamily, usize, usize, InstanceNoise)> = Vec::new();
    for (index, device) in circuit.devices().iter().enumerate() {
        let rows = circuit
            .state_rows(index)
            .ok_or_else(|| SpiceError::circuit("circuit numbering is stale; finalize first"))?;
        let controls = circuit.control_rows(index)?;
        let device_state = match state {
            Some(state) => Some(
                state
                    .get(rows)
                    .ok_or_else(|| SpiceError::circuit("noise bias state is too short"))?,
            ),
            None => None,
        };
        let noise = device.noise(&NoiseContext {
            model_context: context,
            unknowns: circuit.unknowns(),
            bias,
            states: device_state,
            controls: &controls,
        })?;
        let DeviceNoise::Sources {
            family,
            model,
            total,
            sources,
        } = noise
        else {
            continue;
        };
        if sources.is_empty() {
            return Err(SpiceError::circuit(format!(
                "{}: a noisy device must describe at least one generator",
                device.name()
            )));
        }
        if let Some(bad) = sources.iter().find(|source| !source.kind.validate()) {
            return Err(SpiceError::Numerical {
                context: format!("noise of {}", device.name()),
                message: format!("nonfinite or out-of-domain generator {}", bad.suffix),
            });
        }
        let key = (family, model);
        let rank = match models.iter().position(|known| *known == key) {
            Some(rank) => rank,
            None => {
                models.push(key);
                models.len() - 1
            }
        };
        entries.push((
            family,
            rank,
            index,
            InstanceNoise {
                device: index,
                name: device.name().to_owned(),
                sources,
                total,
            },
        ));
    }
    // Later-created models and later instances come first within a type.
    entries.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    Ok(entries.into_iter().map(|entry| entry.3).collect())
}

#[cfg(test)]
mod tests {
    use super::{BOLTZMANN, CHARGE, NoiseKind};

    #[test]
    fn densities_follow_nevalsrc() {
        let thermal = NoiseKind::Thermal {
            conductance: 1e-3,
            temperature: 300.15,
        };
        assert_eq!(
            thermal.output_density(2.0, 10.0),
            2.0 * 4.0 * BOLTZMANN * 300.15 * 1e-3
        );
        let shot = NoiseKind::Shot { current: -1e-3 };
        assert_eq!(shot.output_density(1.0, 1.0), 2.0 * CHARGE * 1e-3);
        let flicker = NoiseKind::Flicker {
            coefficient: 1e-12,
            exponent: 1.0,
        };
        assert_eq!(flicker.output_density(1.0, 100.0), 1e-14);
        let steep = NoiseKind::Flicker {
            coefficient: 1e-12,
            exponent: 2.0,
        };
        assert!((steep.output_density(1.0, 10.0) - 1e-14).abs() < 1e-28);
    }

    #[test]
    fn generators_are_validated() {
        assert!(
            !NoiseKind::Thermal {
                conductance: 1.0,
                temperature: 0.0
            }
            .validate()
        );
        assert!(!NoiseKind::Shot { current: f64::NAN }.validate());
        assert!(
            NoiseKind::Flicker {
                coefficient: 0.0,
                exponent: 1.0
            }
            .validate()
        );
    }
}
