//! `b4temp.c` (`BSIM4temp`): the model's temperature pass, the instance's
//! size-dependent parameters and its temperature-dependent instance values,
//! then `b4check.c`'s parameter check.
use super::fields::{Instance, Model, SizeParam};
use crate::primitives::{SpiceError, SpiceResult};

/// `BSIM4temp` for one instance of `m` at `temperature` (`CKTtemp`, K).
/// The model-level part updates `m` as C does (the caller passes a model
/// fresh from `setup_model`); the size-dependent parameters C shares
/// between instances of equal geometry are returned for this instance.
///
/// # Errors
/// C's fatal `BSIM4checkModel` result (`E_BADPARM`) and C's other error
/// returns, with their messages.
pub(crate) fn temperature(
    _m: &mut Model,
    _h: &mut Instance,
    _temperature: f64,
    _warnings: &mut Vec<String>,
) -> SpiceResult<SizeParam> {
    Err(SpiceError::not_yet_ported(
        "BSIM4 temperature pass",
        "src/spicelib/devices/bsim4/b4temp.c",
    ))
}
