//! The `spice-rs` command-line front end.
//!
//! The C equivalent is `src/frontend/main.c` and the batch-mode path of
//! `src/ngspice.c`. The CLI exists so that the port has an end-to-end entry
//! point from the first milestone: today it reports what a deck contains and
//! what the port cannot do yet, which makes progress observable without any of
//! the simulator being finished.
//!
//! Exit status is part of the interface:
//!
//! | Status | Meaning |
//! | --- | --- |
//! | 0 | success |
//! | 1 | bad command line |
//! | 2 | the deck could not be read or understood |
//! | 3 | the operation is a documented gap in the port ([`NotYetPorted`]) |
//!
//! [`NotYetPorted`]: spice_core::SpiceError::NotYetPorted

#![warn(missing_docs)]

pub mod cli;

pub use cli::{Args, Command, exit_code, run, usage};
