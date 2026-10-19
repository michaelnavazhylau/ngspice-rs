//! `b4ld.c` (`BSIM4load`): the DC/transient load of one instance.
use super::fields::{Instance, Model, SizeParam};
use super::host::LoadCkt;
use crate::primitives::{SpiceError, SpiceResult};

/// `BSIM4load` for one instance: evaluates the device at the voltages C's
/// mode selects, writes `CKTstate0`, integrates charges and adds the
/// matrix and right-hand-side terms, as C does.
///
/// # Errors
/// `NIintegrate` failures, and C's error returns.
pub(crate) fn load(
    _m: &Model,
    _h: &mut Instance,
    _p: &SizeParam,
    _ckt: &mut dyn LoadCkt,
) -> SpiceResult<()> {
    Err(SpiceError::not_yet_ported(
        "BSIM4 load",
        "src/spicelib/devices/bsim4/b4ld.c",
    ))
}
