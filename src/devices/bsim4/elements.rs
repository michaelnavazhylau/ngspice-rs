//! `b4set.c`'s `TSTALLOC` sequence: the matrix elements one instance makes,
//! in C's creation order, which fixes SPARSE's numbering and pivot ties.
use super::fields::{Instance, Model};

/// The `(row, column)` local node pairs `BSIM4setup` allocates for `h`,
/// in order. Pairs involving [`super::host::node::GROUND`] are included, as
/// C makes (trash-can) elements for them too; the host skips them.
pub(crate) fn matrix_elements(m: &Model, h: &Instance) -> Vec<(i32, i32)> {
    let (d, g, s, b) = (h.dNode, h.gNodeExt, h.sNode, h.bNode);
    let (dp, gp, gm, sp, bp) = (
        h.dNodePrime,
        h.gNodePrime,
        h.gNodeMid,
        h.sNodePrime,
        h.bNodePrime,
    );
    let (db, sb, q) = (h.dbNode, h.sbNode, h.qNode);
    let mut list = vec![
        (dp, bp),
        (gp, bp),
        (sp, bp),
        (bp, dp),
        (bp, gp),
        (bp, sp),
        (bp, bp),
        (d, d),
        (gp, gp),
        (s, s),
        (dp, dp),
        (sp, sp),
        (d, dp),
        (gp, dp),
        (gp, sp),
        (s, sp),
        (dp, sp),
        (dp, d),
        (dp, gp),
        (sp, gp),
        (sp, s),
        (sp, dp),
        (q, q),
        (q, bp),
        (q, dp),
        (q, sp),
        (q, gp),
        (dp, q),
        (sp, q),
        (gp, q),
    ];
    if h.rgateMod != 0 {
        list.extend([
            (g, g),
            (g, gp),
            (gp, g),
            (g, dp),
            (g, sp),
            (g, bp),
            (gm, dp),
            (gm, gp),
            (gm, gm),
            (gm, g),
            (gm, sp),
            (gm, bp),
            (dp, gm),
            (gp, gm),
            (g, gm),
            (sp, gm),
            (bp, gm),
        ]);
    }
    if h.rbodyMod == 1 || h.rbodyMod == 2 {
        list.extend([
            (dp, db),
            (sp, sb),
            (db, dp),
            (db, db),
            (db, bp),
            (db, b),
            (bp, db),
            (bp, b),
            (bp, sb),
            (sb, sp),
            (sb, bp),
            (sb, b),
            (sb, sb),
            (b, db),
            (b, bp),
            (b, sb),
            (b, b),
        ]);
    }
    if m.rdsMod != 0 {
        list.extend([(d, gp), (d, sp), (d, bp), (s, dp), (s, gp), (s, bp)]);
    }
    list
}
