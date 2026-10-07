//! Device models and the MNA stamping contract.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`traits`] | the [`Device`] trait, [`StampContext`] and the unknown map | real stamping and immutable equation-assembly contracts |
//! | [`state`] | trial versus accepted device state ([`StateHistory`], [`TrialState`]) | rotating accepted history, atomic commits |
//! | [`circuit`] | node/device container, petgraph incidence topology and unknown numbering | ported |
//! | [`registry`] | designator letter → device factory | scalar R/C/L/V/I factories |
//! | [`sources`] | independent DC/AC/transient sources | Constant/Step/PWL/PULSE waveforms |
//! | [`pulse`] | analytic periodic PULSE, C defaults, lazy corners | left/right limits |
//! | [`linear`] | immutable E x' + A x = b(t) assembly | linear devices only |
//! | [`rlc`] | resistor, capacitor, inductor | linear static/dynamic equations; trap/Gear-2 C/L companion stamps (no driver yet) |
//! | [`passive`] | bounded model-backed R/C/L | schemas, geometry and contextual temperature/scale/multiplicity |
//!
//! The C equivalent is `src/spicelib/devices/`: `ckt*.c` for the framework
//! (`CKTcrte`, `CKTbindNode`, the `CKTdevice` vtable) and one directory per
//! device, each with `<dev>load.c` doing the stamping. `src/spicelib/devices/`
//! is 464k lines of the C tree's 723k, so the registry is designed to be the
//! extension point that keeps the core small — see `docs/port/ROADMAP.md`.
//!
//! `Circuit::from_netlist` accepts literal R/C/L/V/I and bounded model-backed
//! R/C/L. Nonlinear D/Q/M backends are not enabled by syntax or schema validation.
//! Constant/Step/Pwl/Pulse forcing is available both through the device API and
//! from numeric `PULSE(...)`/`PWL(...)` source setters. See
//! `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and the central `TODO.md`.

#![warn(missing_docs)]

pub mod circuit;
mod factory;
pub mod linear;
pub mod models;
pub mod passive;
pub mod pulse;
pub use passive::PassiveParameters;
pub mod schema;
pub use models::{
    DiodeInstanceParameters, DiodeModelParameters, LevelSelection, ModelContext, ModelFamily,
    ModelResolver, ResolvedModel,
};
pub mod registry;
pub mod sources;
pub use linear::{
    Forcing, Limit, LinearContext, LinearSource, LinearSystem, SystemBreakpoints, Waveform,
    WaveformBreakpoints,
};
pub use pulse::{Pulse, PulseBreakpoints, PulseSpec, TransientTiming};
pub use sources::IndependentSource;
pub mod rlc;
pub mod state;
pub mod traits;

pub use circuit::LoadRequest;
pub use circuit::{Circuit, CircuitGraph, CircuitVertex};
pub use registry::{DeviceEntry, Registry};
pub use rlc::{Capacitor, Inductor, Resistor};
pub use state::{ACCEPTED_DEPTH, DeviceState, StateHistory, TrialState};
pub use traits::{
    AcceptContext, AnalysisMode, Device, MnaUnknowns, StampContext, StorageElement, StorageKind,
};

/// The C reference for the device framework, used in `NotYetPorted` errors.
pub const C_REFERENCE_FRAMEWORK: &str = "src/spicelib/devices/ (ckt*.c)";
