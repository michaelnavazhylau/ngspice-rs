//! Device models and the MNA stamping contract.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`traits`] | the [`Device`] trait, [`StampContext`] and the unknown map | contract ported, stamping stubbed |
//! | [`circuit`] | node/device container, petgraph incidence topology and unknown numbering | ported |
//! | [`registry`] | designator letter → device factory | ported |
//! | [`rlc`] | resistor, capacitor, inductor | types only; the first porting targets |
//!
//! The C equivalent is `src/spicelib/devices/`: `ckt*.c` for the framework
//! (`CKTcrte`, `CKTbindNode`, the `CKTdevice` vtable) and one directory per
//! device, each with `<dev>load.c` doing the stamping. `src/spicelib/devices/`
//! is 464k lines of the C tree's 723k, so the registry is designed to be the
//! extension point that keeps the core small — see `docs/port/ROADMAP.md`.

#![warn(missing_docs)]

pub mod circuit;
pub mod registry;
pub mod rlc;
pub mod traits;

pub use circuit::{Circuit, CircuitGraph, CircuitVertex};
pub use registry::{DeviceEntry, Registry};
pub use rlc::{Capacitor, Inductor, Resistor};
pub use traits::{AnalysisMode, Device, MnaUnknowns, StampContext};

/// The C reference for the device framework, used in `NotYetPorted` errors.
pub const C_REFERENCE_FRAMEWORK: &str = "src/spicelib/devices/ (ckt*.c)";
