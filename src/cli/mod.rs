//! The `spice-rs` command-line front end.
//!
//! The C equivalent is `src/frontend/main.c` and the batch-mode path of
//! `src/ngspice.c`. The default command reports what a deck contains and what
//! the port cannot do with it; [`simulate`] runs every analysis of the deck,
//! in ngspice batch order, with the production engine and writes one ASCII
//! rawfile with a plot per analysis. See `docs/port/CLI.md`.
//!
//! Exit status is part of the interface:
//!
//! | Status | Meaning |
//! | --- | --- |
//! | 0 | success |
//! | 1 | bad command line |
//! | 2 | the deck could not be read or understood, or the run/output failed |
//! | 3 | the operation is a documented gap in the port ([`NotYetPorted`]) |
//!
//! [`NotYetPorted`]: crate::primitives::SpiceError::NotYetPorted

pub mod args;
pub mod simulate;

pub use args::{Args, Command, exit_code, run, usage};
