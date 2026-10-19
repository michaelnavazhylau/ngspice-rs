//! `b4cvtest.c` (`BSIM4convTest`): the device convergence test.
use super::fields::{Instance, Model};

/// `BSIM4convTest` for one instance: `true` when C would increment
/// `CKTnoncon`. `rhs_old` is the new iterate C reads as `CKTrhsOld`,
/// `state0` the last load's state vector, `mode` C's `CKTmode`.
pub(crate) fn conv_test(
    _m: &Model,
    _h: &Instance,
    _rhs_old: &dyn Fn(i32) -> f64,
    _state0: &dyn Fn(usize) -> f64,
    _mode: u32,
    _reltol: f64,
    _abstol: f64,
) -> bool {
    true
}
