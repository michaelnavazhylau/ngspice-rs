//! `b4pzld.c` (`BSIM4pzLoad`): the pole-zero load.
use super::fields::{Instance, Model, SizeParam};
use super::host::SmallSignalCkt;
use crate::primitives::{SpiceError, SpiceResult};

/// `BSIM4pzLoad` for one instance, from the values the `MODEINITSMSIG`
/// load left on `h`.
///
/// # Errors
/// C's error returns.
pub(crate) fn pz_load(
    _m: &Model,
    _h: &Instance,
    _p: &SizeParam,
    _ckt: &mut dyn SmallSignalCkt,
) -> SpiceResult<()> {
    Err(SpiceError::not_yet_ported(
        "BSIM4 pole-zero load",
        "src/spicelib/devices/bsim4/b4pzld.c",
    ))
}
