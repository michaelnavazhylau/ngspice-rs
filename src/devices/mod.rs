//! Device models and the MNA stamping contract.
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`traits`] | the [`Device`] trait, [`StampContext`] and the unknown map | real stamping and immutable equation-assembly contracts |
//! | [`state`] | trial versus accepted device state ([`StateHistory`], [`TrialState`]) | rotating accepted history, atomic commits |
//! | [`limiting`] | Newton junction/FET voltage limiting (`DEVpnjlim`, `DEVfetlim`, `DEVlimvds`) and the [`limiting::Limiter`] device hook | diode, BJT and MOS1 |
//! | [`circuit`] | node/device container, petgraph incidence topology and unknown numbering | ported |
//! | [`registry`] | designator letter → device factory | scalar R/C/L/V/I and linear E/F/G/H factories |
//! | [`sources`] | independent DC/AC/transient sources | Constant/Step/PWL/PULSE/SIN/EXP/SFFM/AM waveforms |
//! | [`controlled`] | linear E/F/G/H controlled sources | VCVS/CCCS/VCCS/CCVS gain stamps; F/H controlling branches resolved by [`circuit`] |
//! | [`behavioural`] | B sources and the lowered E/G/F/H VALUE/TABLE/POLY forms | `inpptree.c` function set with C's derivative rules; Newton, AC and transient loads |
//! | [`mutual`] | K mutual inductance | coupled flux in DC/AC/companion/BDF; inductors and inductive-system checks resolved by [`circuit`] |
//! | [`switch`] | S/W voltage- and current-controlled switches | hysteresis, accepted switch state, Newton phases, `swtrunc.c` step control |
//! | [`pulse`] | analytic periodic PULSE, C defaults, lazy corners | left/right limits, pulse count |
//! | [`functions`] | analytic SIN/EXP/SFFM/AM and delayed/repeating PWL | C defaults, lazy corners |
//! | [`linear`] | immutable E x' + A x = b(t) assembly | linear devices only |
//! | [`rlc`] | resistor, capacitor, inductor | linear static/dynamic equations; trap/Gear-2 C/L companion stamps (no driver yet) |
//! | [`passive`] | bounded model-backed R/C/L | schemas, geometry and contextual temperature/scale/multiplicity |
//! | [`sweep`] | physical resistor metadata, immutable per-point resistor and instance-parameter overrides | typed `.dc` resistor and `@inst[param]` targets |
//! | [`noise`] | `.noise` generators: the [`Device::noise`] hook, thermal/shot/flicker laws and C's instance order | R, D, Q, MOS1, S/W; explicit `Noiseless` for C's noise-free devices |
//! | [`subckt`] | `X` instance expansion: port binding, hierarchical names, scoped parameters and models | top-level definitions, named overrides, `.global` nodes |
//!
//! The C equivalent is `src/spicelib/devices/`: `ckt*.c` for the framework
//! (`CKTcrte`, `CKTbindNode`, the `CKTdevice` vtable) and one directory per
//! device, each with `<dev>load.c` doing the stamping. `src/spicelib/devices/`
//! is 464k lines of the C tree's 723k, so the registry is designed to be the
//! extension point that keeps the core small — see `docs/port/ROADMAP.md`.
//!
//! `Circuit::from_netlist` accepts literal R/C/L/V/I, linear E/F/G/H controlled
//! sources, K mutual inductance, bounded model-backed R/C/L and the explicitly bounded M4
//! diode/Gummel-Poon BJT/MOS1 subset.
//! Parsing alone never enables unsupported physics; model-aware schemas reject it.
//! Constant/Step/Pwl/Pulse forcing is available both through the device API and
//! from numeric `PULSE(...)`/`PWL(...)` source setters; `SIN`/`EXP`/`SFFM`/`AM`
//! and PWL `td=`/`r=` elaborate to [`functions`] forcing. See
//! `docs/port/DIFFSOL_FAER_IMPLEMENTATION.md` and the central `TODO.md`.

pub mod behavioural;
pub mod bjt;
pub mod circuit;
pub mod controlled;
pub mod distortion;
mod factory;
pub mod functions;
mod initial;
pub mod limiting;
pub mod linear;
pub mod models;
pub mod mos;
pub mod mos1;
pub mod mos3;
pub mod mutual;
pub mod noise;
pub mod nonlinear;
pub mod passive;
pub mod pulse;
pub use passive::PassiveParameters;
pub mod schema;
pub mod sensitivity;
pub use models::{
    DEFAULT_GMIN, DiodeInstanceParameters, DiodeModelParameters, LevelSelection, ModelContext,
    ModelFamily, ModelResolver, ResolvedModel,
};
pub mod registry;
pub mod sources;
pub mod subckt;
pub mod sweep;
pub mod switch;
pub use functions::{
    AmSpec, ExpSpec, FunctionSpec, PwlBreakpoints, PwlSource, SffmSpec, SineSpec, SourceFunction,
};
pub use linear::{
    Forcing, Limit, LinearContext, LinearSource, LinearSystem, SourceKind, SystemBreakpoints,
    Waveform, WaveformBreakpoints,
};
pub use pulse::{Pulse, PulseBreakpoints, PulseSpec, TransientTiming};
pub use sources::{IndependentSource, RfPort};
pub use subckt::{ExpandedNetlist, SubcircuitLimits, expand_subcircuits};
pub mod rlc;
pub mod state;
pub mod traits;

pub use behavioural::{Behavioural, BehaviouralOutput, BehaviouralScale};
pub use circuit::LoadRequest;
pub use circuit::{Circuit, CircuitGraph, CircuitVertex};
pub use controlled::{ControlledKind, ControlledSource};
pub use mutual::MutualInductance;
pub use registry::{DeviceEntry, DeviceSupport, Registry};
pub use rlc::{Capacitor, Inductor, Resistor, ResistorNoise};
pub use state::{ACCEPTED_DEPTH, DeviceState, IterationPhase, StateHistory, TrialState};
pub use sweep::{
    InstanceOverride, MAX_INSTANCE_OVERRIDES, MAX_RESISTOR_OVERRIDES, ResistorMetadata,
    ResistorOrigin, ResistorOverride,
};
pub use switch::{Switch, SwitchKind, SwitchState};
pub use traits::{
    AcceptContext, AnalysisMode, ControlReference, Device, InductanceValue, MnaUnknowns,
    MutualCoupling, MutualTerm, StampContext, StorageElement, StorageKind, TruncationContext,
};

/// The C reference for the device framework, used in `NotYetPorted` errors.
pub const C_REFERENCE_FRAMEWORK: &str = "src/spicelib/devices/ (ckt*.c)";

pub mod observe;
