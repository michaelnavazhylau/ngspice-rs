//! The netlist front end: deck text in, [`ast::Netlist`] out.
//!
//! The front end is split so that it can be ported in stages:
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`source`] | physical lines → logical cards: title, `+` continuations, comments | **ported** |
//! | [`token`] | logical card → token stream | **ported** |
//! | [`card`] | first token → [`card::CardKind`] classification | **ported** |
//! | [`ast`] | the semantic netlist model | linear/model/D/Q/M subset constructed |
//! | [`parser`] | winnow token stream → [`ast::Netlist`] | **M1a + scalar models and bounded D/Q/M syntax** |
//!
//! The C equivalent is spread over `src/frontend/inp.c`,
//! `src/frontend/inpcom.c` and `src/spicelib/parser/`. Deck dispatch is in
//! `inppas2.c`/`inp2dot.c` and per-device functions (`inp2r.c`, `inp2c.c`, …).
//! `src/frontend/parse-bison.y` is a separate expression grammar, not a deck
//! grammar; `.param` evaluation lives in `src/frontend/numparam/`.

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
