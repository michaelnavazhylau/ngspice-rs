//! The slice of C's `CKTcircuit` the BSIM4 load routines read and write.
//!
//! `b4ld.c`, `b4acld.c` and `b4pzld.c` are ported statement by statement, so
//! they keep C's view of the circuit: a `CKTmode` bitmask, the previous
//! solution (`CKTrhsOld`), the state vectors `CKTstate0..2`, `NIintegrate`
//! and additions into matrix elements and the right-hand side. The device
//! (`device.rs`) implements these traits over the Rust solver's
//! [`crate::devices::StampContext`] and [`crate::devices::LinearContext`].
//!
//! Nodes are BSIM4-local indices ([`node`]), which the instance's node fields
//! (`dNodePrime`, `gNodeMid`, ...) hold exactly as C's hold circuit node
//! numbers: an internal node that `b4set.c` does not create aliases its
//! external node, as in C. Matrix stamps involving ground are dropped by the
//! host, as SPARSE's trash-can element absorbs them in C.
use crate::primitives::SpiceResult;

/// BSIM4-local node indices. `b4set.c` sets an instance's internal node
/// fields either to their own index or to the external node they alias.
pub(crate) mod node {
    /// External drain (`BSIM4dNode`).
    pub(crate) const D: i32 = 0;
    /// External gate (`BSIM4gNodeExt`).
    pub(crate) const G_EXT: i32 = 1;
    /// External source (`BSIM4sNode`).
    pub(crate) const S: i32 = 2;
    /// External bulk (`BSIM4bNode`).
    pub(crate) const B: i32 = 3;
    /// Internal drain (`BSIM4dNodePrime`).
    pub(crate) const DP: i32 = 4;
    /// Internal gate (`BSIM4gNodePrime`).
    pub(crate) const GP: i32 = 5;
    /// Mid gate, `rgateMod == 3` (`BSIM4gNodeMid`).
    pub(crate) const GM: i32 = 6;
    /// Internal source (`BSIM4sNodePrime`).
    pub(crate) const SP: i32 = 7;
    /// Internal bulk (`BSIM4bNodePrime`).
    pub(crate) const BP: i32 = 8;
    /// Drain-side bulk, `rbodyMod` (`BSIM4dbNode`).
    pub(crate) const DB: i32 = 9;
    /// Source-side bulk, `rbodyMod` (`BSIM4sbNode`).
    pub(crate) const SB: i32 = 10;
    /// NQS charge node, `trnqsMod` (`BSIM4qNode`).
    pub(crate) const Q: i32 = 11;
    /// Number of local node indices.
    pub(crate) const COUNT: usize = 12;
    /// Ground: C's node 0, which an uncreated `qNode` holds. Stamps and
    /// right-hand-side terms at ground are discarded, as SPARSE's trash-can
    /// element discards them in C, and its voltage reads as zero.
    pub(crate) const GROUND: i32 = -1;
}

/// `CKTmode` bits (`cktdefs.h`), with C's values.
#[allow(dead_code)] // The full C set, so ported conditions read as in C.
pub(crate) mod mode {
    /// `MODETRAN`.
    pub(crate) const MODETRAN: u32 = 0x1;
    /// `MODEAC`.
    pub(crate) const MODEAC: u32 = 0x2;
    /// `MODEDCOP`.
    pub(crate) const MODEDCOP: u32 = 0x10;
    /// `MODETRANOP`.
    pub(crate) const MODETRANOP: u32 = 0x20;
    /// `MODEDCTRANCURVE`.
    pub(crate) const MODEDCTRANCURVE: u32 = 0x40;
    /// `MODEINITFLOAT`.
    pub(crate) const MODEINITFLOAT: u32 = 0x100;
    /// `MODEINITJCT`.
    pub(crate) const MODEINITJCT: u32 = 0x200;
    /// `MODEINITFIX`.
    pub(crate) const MODEINITFIX: u32 = 0x400;
    /// `MODEINITSMSIG`.
    pub(crate) const MODEINITSMSIG: u32 = 0x800;
    /// `MODEINITTRAN`.
    pub(crate) const MODEINITTRAN: u32 = 0x1000;
    /// `MODEINITPRED`.
    pub(crate) const MODEINITPRED: u32 = 0x2000;
    /// `MODEUIC`.
    pub(crate) const MODEUIC: u32 = 0x10000;
}

/// State-vector offsets (`bsim4def.h`'s `BSIM4states + n`).
#[allow(dead_code)] // The full C layout.
pub(crate) mod state {
    /// `BSIM4vbd`.
    pub(crate) const VBD: usize = 0;
    /// `BSIM4vbs`.
    pub(crate) const VBS: usize = 1;
    /// `BSIM4vgs`.
    pub(crate) const VGS: usize = 2;
    /// `BSIM4vds`.
    pub(crate) const VDS: usize = 3;
    /// `BSIM4vdbs`.
    pub(crate) const VDBS: usize = 4;
    /// `BSIM4vdbd`.
    pub(crate) const VDBD: usize = 5;
    /// `BSIM4vsbs`.
    pub(crate) const VSBS: usize = 6;
    /// `BSIM4vges`.
    pub(crate) const VGES: usize = 7;
    /// `BSIM4vgms`.
    pub(crate) const VGMS: usize = 8;
    /// `BSIM4vses`.
    pub(crate) const VSES: usize = 9;
    /// `BSIM4vdes`.
    pub(crate) const VDES: usize = 10;
    /// `BSIM4qb`.
    pub(crate) const QB: usize = 11;
    /// `BSIM4cqb`.
    pub(crate) const CQB: usize = 12;
    /// `BSIM4qg`.
    pub(crate) const QG: usize = 13;
    /// `BSIM4cqg`.
    pub(crate) const CQG: usize = 14;
    /// `BSIM4qd`.
    pub(crate) const QD: usize = 15;
    /// `BSIM4cqd`.
    pub(crate) const CQD: usize = 16;
    /// `BSIM4qgmid`.
    pub(crate) const QGMID: usize = 17;
    /// `BSIM4cqgmid`.
    pub(crate) const CQGMID: usize = 18;
    /// `BSIM4qbs`.
    pub(crate) const QBS: usize = 19;
    /// `BSIM4cqbs`.
    pub(crate) const CQBS: usize = 20;
    /// `BSIM4qbd`.
    pub(crate) const QBD: usize = 21;
    /// `BSIM4cqbd`.
    pub(crate) const CQBD: usize = 22;
    /// `BSIM4qcheq`.
    pub(crate) const QCHEQ: usize = 23;
    /// `BSIM4cqcheq`.
    pub(crate) const CQCHEQ: usize = 24;
    /// `BSIM4qcdump`.
    pub(crate) const QCDUMP: usize = 25;
    /// `BSIM4cqcdump`.
    pub(crate) const CQCDUMP: usize = 26;
    /// `BSIM4qdef`.
    pub(crate) const QDEF: usize = 27;
    /// `BSIM4qs`.
    pub(crate) const QS: usize = 28;
    /// `BSIM4numStates`.
    pub(crate) const COUNT: usize = 29;
}

/// What `b4ld.c` reads from and writes to `CKTcircuit`.
pub(crate) trait LoadCkt {
    /// `CKTmode`, as [`mode`] bits.
    fn mode(&self) -> u32;
    /// Whether the load may limit its junction voltages (`DEVfetlim`,
    /// `DEVlimvds`, `DEVpnjlim`). Always true for C; false only for the
    /// port's exact (unlimited) re-evaluations, where the load evaluates the
    /// supplied voltages unchanged.
    fn limiting(&self) -> bool;
    /// `CKTgmin`.
    fn gmin(&self) -> f64;
    /// `CKTtemp` in kelvin.
    fn temp(&self) -> f64;
    /// `CKTag[i]` (zero outside transient).
    fn ag(&self, i: usize) -> f64;
    /// `CKTdelta` (zero outside transient).
    fn delta(&self) -> f64;
    /// `CKTdeltaOld[i]` (zero outside transient).
    fn delta_old(&self, i: usize) -> f64;
    /// `CKTrhsOld[node]`: the solution the load linearises around.
    fn rhs_old(&self, node: i32) -> f64;
    /// `CKTstate0[slot]`: the previous iterate's value until this load
    /// writes it, then the written value.
    fn state0(&self, slot: usize) -> f64;
    /// `*(CKTstate0 + slot) = value`.
    fn set_state0(&mut self, slot: usize, value: f64);
    /// `CKTstate1[slot]`, the last accepted point.
    fn state1(&self, slot: usize) -> f64;
    /// `*(CKTstate1 + slot) = value` (`MODEINITTRAN` only).
    fn set_state1(&mut self, slot: usize, value: f64);
    /// `CKTstate2[slot]`.
    fn state2(&self, slot: usize) -> f64;
    /// `NIintegrate(ckt, &geq, &ceq, cap, slot)`: integrates the charge in
    /// `state0[slot]` into `state0[slot + 1]` and returns `(geq, ceq)`.
    ///
    /// # Errors
    /// A short accepted history or nonfinite values (C's `E_ORDER` and
    /// `E_METHOD` cannot occur).
    fn integrate(&mut self, cap: f64, slot: usize) -> SpiceResult<(f64, f64)>;
    /// `ckt->CKTnoncon++`.
    fn noncon(&mut self);
    /// `*(here->XYPtr) += value` for the element at (`row`, `col`).
    fn matrix(&mut self, row: i32, col: i32, value: f64);
    /// `*(ckt->CKTrhs + node) += value`.
    fn rhs(&mut self, node: i32, value: f64);
}

/// What `b4acld.c` and `b4pzld.c` read and write. Both run after the
/// `MODEINITSMSIG` load has left the operating point's values on the
/// instance.
pub(crate) trait SmallSignalCkt {
    /// `CKTomega` (`b4acld.c` only).
    fn omega(&self) -> f64;
    /// `CKTstates[0][slot]` / `CKTstate0[slot]` at the operating point.
    fn state0(&self, slot: usize) -> f64;
    /// `*(here->XYPtr) += value`: the real part.
    fn real(&mut self, row: i32, col: i32, value: f64);
    /// `*(here->XYPtr + 1) += value`: the imaginary part, already including
    /// `CKTomega` as `b4acld.c` computes it.
    fn imag(&mut self, row: i32, col: i32, value: f64);
    /// `b4pzld.c`'s `*(ptr) += value * s->real; *(ptr + 1) += value *
    /// s->imag`: `value` is the coefficient of `s` in the pencil `A + s E`.
    fn cap(&mut self, row: i32, col: i32, value: f64);
}
