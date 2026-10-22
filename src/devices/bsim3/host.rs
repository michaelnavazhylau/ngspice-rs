//! BSIM3's view of `CKTcircuit`: local node indices and state offsets.
//!
//! The load routines see the circuit through the BSIM4 port's host traits
//! ([`crate::devices::bsim4::LoadCkt`] and
//! [`crate::devices::bsim4::SmallSignalCkt`]), which model the same
//! `CKTcircuit` members. Nodes are BSIM3-local indices ([`node`]) that the
//! instance's node fields hold exactly as C's hold circuit node numbers: an
//! internal node `b3set.c` does not create aliases its external node, and an
//! uncreated `qNode` is ground, as in C.
pub(crate) use crate::devices::bsim4::{LoadCkt, SmallSignalCkt, mode};

/// BSIM3-local node indices.
pub(crate) mod node {
    /// External drain (`BSIM3dNode`).
    pub(crate) const D: i32 = 0;
    /// External gate (`BSIM3gNode`).
    pub(crate) const G: i32 = 1;
    /// External source (`BSIM3sNode`).
    pub(crate) const S: i32 = 2;
    /// External bulk (`BSIM3bNode`).
    pub(crate) const B: i32 = 3;
    /// Internal drain (`BSIM3dNodePrime`).
    pub(crate) const DP: i32 = 4;
    /// Internal source (`BSIM3sNodePrime`).
    pub(crate) const SP: i32 = 5;
    /// NQS charge node, `nqsMod` (`BSIM3qNode`).
    pub(crate) const Q: i32 = 6;
    /// Number of local node indices.
    pub(crate) const COUNT: usize = 7;
    /// Ground: C's node 0, which an uncreated `qNode` holds. Stamps and
    /// right-hand-side terms at ground are discarded, as SPARSE's trash-can
    /// element discards them in C, and its voltage reads as zero.
    pub(crate) const GROUND: i32 = -1;
}

/// State-vector offsets (`bsim3def.h`'s `BSIM3states + n`).
#[allow(dead_code)] // The full C layout.
pub(crate) mod state {
    /// `BSIM3vbd`.
    pub(crate) const VBD: usize = 0;
    /// `BSIM3vbs`.
    pub(crate) const VBS: usize = 1;
    /// `BSIM3vgs`.
    pub(crate) const VGS: usize = 2;
    /// `BSIM3vds`.
    pub(crate) const VDS: usize = 3;
    /// `BSIM3qb`.
    pub(crate) const QB: usize = 4;
    /// `BSIM3cqb`.
    pub(crate) const CQB: usize = 5;
    /// `BSIM3qg`.
    pub(crate) const QG: usize = 6;
    /// `BSIM3cqg`.
    pub(crate) const CQG: usize = 7;
    /// `BSIM3qd`.
    pub(crate) const QD: usize = 8;
    /// `BSIM3cqd`.
    pub(crate) const CQD: usize = 9;
    /// `BSIM3qbs`.
    pub(crate) const QBS: usize = 10;
    /// `BSIM3qbd`.
    pub(crate) const QBD: usize = 11;
    /// `BSIM3qcheq`.
    pub(crate) const QCHEQ: usize = 12;
    /// `BSIM3cqcheq`.
    pub(crate) const CQCHEQ: usize = 13;
    /// `BSIM3qcdump`.
    pub(crate) const QCDUMP: usize = 14;
    /// `BSIM3cqcdump`.
    pub(crate) const CQCDUMP: usize = 15;
    /// `BSIM3qdef`.
    pub(crate) const QDEF: usize = 16;
    /// `BSIM3numStates`.
    pub(crate) const COUNT: usize = 17;
}
