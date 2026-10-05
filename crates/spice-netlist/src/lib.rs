//! The netlist front end: deck text in, [`ast::Netlist`] out.
//!
//! The front end is split so that it can be ported in stages:
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`source`] | physical lines → logical cards: title, `+` continuations, comments | **ported** |
//! | [`token`] | logical card → token stream | **ported** |
//! | [`card`] | first token → [`card::CardKind`] classification | **ported** |
//! | [`ast`] | the semantic netlist model | types only |
//! | [`parser`] | tokens → [`ast::Netlist`] | **not ported** |
//!
//! The C equivalent is spread over `src/frontend/inp.c`,
//! `src/frontend/inpcom.c` and `src/spicelib/parser/`. The Bison grammar in
//! `src/frontend/parse-bison.y` is only 180 lines because most of the work
//! happens in per-device C functions (`inp2r.c`, `inp2c.c`, …); the port plans a
//! hand-written recursive-descent parser instead.

#![warn(missing_docs)]

pub mod ast;
pub mod card;
pub mod parser;
pub mod source;
pub mod token;

pub use card::{CardKind, DEVICE_DESIGNATORS, DotCommand, RawCard};
pub use parser::{Parser, classify_deck, load_classified};
pub use source::{Deck, LogicalLine, PhysicalLine, load};
pub use token::{Token, TokenKind, tokenize};

/// The C reference for the whole front end, used in `NotYetPorted` errors.
pub const C_REFERENCE: &str =
    "src/frontend/inp.c, src/frontend/inpcom.c, src/spicelib/parser/inp*.c";
