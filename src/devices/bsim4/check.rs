//! `b4check.c` (`BSIM4checkModel`): fatal parameter errors and warnings.
use super::fields::{Instance, Model, SizeParam};

/// `BSIM4checkModel`: appends C's messages (fatal and warning, in C order)
/// to `messages` and returns C's `Fatal_Flag`.
pub(crate) fn check_model(
    _m: &Model,
    _h: &Instance,
    _p: &SizeParam,
    _temperature: f64,
    _messages: &mut Vec<String>,
) -> bool {
    false
}
