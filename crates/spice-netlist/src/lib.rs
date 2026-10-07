//! The netlist front end: deck text in, [`ast::Netlist`] out.
//!
//! The front end is split so that it can be ported in stages:
//!
//! | Module | Job | State |
//! | --- | --- | --- |
//! | [`source`] | physical lines → logical cards: title, `+` continuations, comments | **ported** |
//! | [`token`] | logical card → token stream | **ported** |
//! | [`card`] | first token → [`card::CardKind`] classification | **ported** |
//! | [`expr`] | unevaluated parameter-expression syntax tree (bounded numparam subset) | **#14** |
//! | [`eval`] | bounded `.param` scope resolution and expression evaluation | **#15** |
//! | [`elaborate`] | literalized netlist copy with evaluated numeric sites | **#15** |
//! | [`ast`] | the semantic netlist model | bounded devices/models, ordered subcircuit scopes and source provenance |
//! | [`writer`], [`semantic`] | normalized deck serialization and location-free comparison | **#20** |
//! | [`dump`], [`snapshot`] | versioned token/AST text dumps and snapshot generation | **#21** |
//! | [`parser`] | winnow token stream → [`ast::Netlist`] | **M1a/M1b + scoped subcircuits/X and bounded source resolution; no flattening** |
//!
//! The C equivalent is spread over `src/frontend/inp.c`,
//! `src/frontend/inpcom.c` and `src/spicelib/parser/`. Deck dispatch is in
//! `inppas2.c`/`inp2dot.c` and per-device functions (`inp2r.c`, `inp2c.c`, …).
//! `src/frontend/parse-bison.y` is a separate expression grammar, not a deck
//! grammar; `.param` evaluation lives in `src/frontend/numparam/`.

#![warn(missing_docs)]

pub mod ast;
pub mod card;
pub mod dump;
pub mod elaborate;
pub mod eval;
pub mod expr;
pub mod parser;
pub mod semantic;
pub mod snapshot;
pub mod source;
pub mod token;
pub mod writer;

pub use card::{CardKind, DEVICE_DESIGNATORS, DotCommand, RawCard};
pub use expr::{Expr, ExprKind, ParameterExpression, SourceSpan};
pub use parser::{Parser, SourceLimits, classify_deck, load_classified};
pub use semantic::{semantic_diff, semantic_eq, semantic_form};
pub use source::{Deck, LogicalLine, PhysicalLine, load};
pub use token::{Token, TokenKind, tokenize};
pub use writer::write_netlist;

/// The C reference for the whole front end, used in `NotYetPorted` errors.
pub const C_REFERENCE: &str =
    "src/frontend/inp.c, src/frontend/inpcom.c, src/spicelib/parser/inp*.c";
