//! A from-scratch Rust implementation of [ngspice], the SPICE circuit
//! simulator.
//!
//! The port reads ngspice decks, runs DC, AC and transient analyses on a
//! bounded set of linear and nonlinear devices, and writes ngspice-compatible
//! rawfiles. It is **not** a wrapper around the ngspice C library: the C tree is
//! a read-only specification and a source of comparison data, reached only out
//! of process. `unsafe_code` is forbidden at the crate level, so there is no FFI.
//!
//! Coverage is deliberately bounded. Anything outside it returns
//! [`SpiceError::NotYetPorted`] with a C reference naming the upstream file and
//! function that would have to be ported, rather than a partial or approximate
//! result.
//!
//! # Module map
//!
//! The port was developed as six crates and is now one package, so the old crate
//! boundaries are the module tree. They are layered: a module may use the
//! modules above it in this table and nothing below it.
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`primitives`] | `Real`, `Complex`, SPICE numeric literals with scale factors, the node table and ground aliasing, the error type and the analysis taxonomy. Dependency-free vocabulary for every other module. |
//! | [`netlist`] | Deck loading (title line, `+` continuations, comments), tokenizer, card classification, semantic AST, the winnow parser, `.param` evaluation, subcircuit scopes, deck writing and snapshots. |
//! | [`maths`] | Dense/sparse/complex storage, petgraph row-coupling topology, faer pivoted LU, bounded diffsol BDF integration and trapezoidal/Gear companion coefficients. |
//! | [`devices`] | The [`devices::Device`] stamping contract, device state history, scalar R/C/L/V/I and model-backed passive/diode/BJT/MOS1 stamps, [`devices::Circuit`] and subcircuit expansion. |
//! | [`analysis`] | The `.op`/`.dc`/`.ac`/`.tran` drivers, convergence policy, accepted points, `.measure`/`.four` post-processing and ASCII/binary rawfile I/O. |
//! | [`cli`] | The `spice-rs` command-line front end: argument parsing, command dispatch and the exit-status contract. |
//!
//! `xtask` is a separate unpublished workspace member holding development
//! automation (C golden capture and drift checks, Rust-engine verification, CI).
//!
//! # Example
//!
//! ```
//! use std::path::Path;
//!
//! use ngspice_rs::devices::Circuit;
//! use ngspice_rs::netlist::Parser;
//! use ngspice_rs::netlist::source::parse_deck_text;
//!
//! let deck = "rc\nV1 in 0 1\nR1 in out 1k\nC1 out 0 1u\n.op\n.end\n";
//! let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("rc.cir"), deck))?;
//! let circuit = Circuit::from_netlist(&netlist)?;
//! # let _ = circuit;
//! # Ok::<(), ngspice_rs::SpiceError>(())
//! ```
//!
//! The convenience vocabulary of [`primitives`] is re-exported at the crate
//! root, so [`SpiceError`], [`Real`] and [`NodeTable`] are available without the
//! module prefix.
//!
//! [ngspice]: https://ngspice.sourceforge.io/

#![warn(missing_docs)]

pub mod analysis;
pub mod cli;
pub mod devices;
pub mod maths;
pub mod netlist;
pub mod primitives;

pub use primitives::{
    AnalysisKind, Complex, GROUND_ALIAS, GROUND_NAME, Node, NodeId, NodeKind, NodeTable,
    ParsedNumber, Real, SourceLoc, SpiceError, SpiceResult, approx_eq, format_spice_number,
    parse_spice_number, parse_spice_number_prefix,
};
