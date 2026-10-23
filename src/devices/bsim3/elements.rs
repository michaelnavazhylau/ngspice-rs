//! `b3set.c`'s `TSTALLOC` sequence: the matrix elements one instance makes,
//! in C's creation order, which fixes SPARSE's numbering and pivot ties.
use super::fields::Instance;

/// The `(row, column)` local node pairs `BSIM3setup` allocates for `here`,
/// in order. Pairs involving [`super::host::node::GROUND`] (an uncreated
/// `qNode`) are included, as C makes (trash-can) elements for them too; the
/// host skips them.
pub(crate) fn matrix_elements(here: &Instance) -> Vec<(i32, i32)> {
    let (d, g, s, b) = (here.dNode, here.gNode, here.sNode, here.bNode);
    let (dp, sp, q) = (here.dNodePrime, here.sNodePrime, here.qNode);
    vec![
        (d, d),
        (g, g),
        (s, s),
        (b, b),
        (dp, dp),
        (sp, sp),
        (d, dp),
        (g, b),
        (g, dp),
        (g, sp),
        (s, sp),
        (b, dp),
        (b, sp),
        (dp, sp),
        (dp, d),
        (b, g),
        (dp, g),
        (sp, g),
        (sp, s),
        (dp, b),
        (sp, b),
        (sp, dp),
        (q, q),
        (q, dp),
        (q, sp),
        (q, g),
        (q, b),
        (dp, q),
        (sp, q),
        (g, q),
        (b, q),
    ]
}
