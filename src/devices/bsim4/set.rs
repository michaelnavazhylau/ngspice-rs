//! `b4set.c` (`BSIM4setup`): model defaults, then instance defaults and the
//! internal nodes C creates. Matrix elements are listed in `elements.rs`.
use super::fields::{Instance, Model};
use crate::primitives::{SpiceError, SpiceResult};

/// `BSIM4setup`'s model pass: every given-guarded default, the dependent
/// defaults and the model selector checks, in C order. `nominal_temperature`
/// is `CKTnomTemp` in kelvin. C's `printf` warnings are appended to
/// `warnings` verbatim.
///
/// # Errors
/// Conditions under which C's setup returns an error.
pub(crate) fn setup_model(
    _m: &mut Model,
    _nominal_temperature: f64,
    _warnings: &mut Vec<String>,
) -> SpiceResult<()> {
    Err(SpiceError::not_yet_ported(
        "BSIM4 model setup",
        "src/spicelib/devices/bsim4/b4set.c",
    ))
}

/// `BSIM4setup`'s instance pass: instance defaults and selector overrides,
/// then the internal nodes to create, as `(local node index, CKTmkVolt
/// suffix)` in C's creation order. The caller has set the four external
/// node fields; this sets every internal node field (an uncreated node
/// aliases its external node, an uncreated `qNode` is
/// [`super::host::node::GROUND`]). `noise_analysis` is C's
/// `noiseAnalGiven`.
///
/// # Errors
/// Conditions under which C's setup returns an error.
pub(crate) fn setup_instance(
    _m: &Model,
    _h: &mut Instance,
    _noise_analysis: bool,
    _warnings: &mut Vec<String>,
) -> SpiceResult<Vec<(i32, &'static str)>> {
    Err(SpiceError::not_yet_ported(
        "BSIM4 instance setup",
        "src/spicelib/devices/bsim4/b4set.c",
    ))
}
