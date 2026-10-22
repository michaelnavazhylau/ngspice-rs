//! `mos9` shares level 3's model replay, except for its noise selectors.
use super::Level9;
use crate::devices::ModelContext;
use crate::devices::mos::sens::{INSTANCE, SensitivityLevel};
use crate::devices::mos3::Level3;
use crate::devices::sensitivity::{
    SensitivityFamily, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::primitives::{Real, SpiceResult};

impl SensitivityLevel for Level9 {
    const FAMILY: SensitivityFamily = SensitivityFamily::Mos9;
    const MODEL: &'static [P] = Level3::MODEL.split_at(Level3::MODEL.len() - 1).0;
    // mos9ask.c has no MOS9_DTEMP case: C drops this GET ERROR column.
    const INSTANCE: &'static [P] = INSTANCE.split_at(INSTANCE.len() - 1).0;
    fn record(&self, c: &ModelContext) -> Record {
        self.0.record(c)
    }
    fn with_record(&self, r: &Record, derived: bool) -> SpiceResult<Self> {
        self.0.with_record(r, derived).map(Self)
    }
    fn setup(&self, r: &mut Record) -> SpiceResult<()> {
        self.0.setup(r)
    }
    fn temperature(&self, r: &mut Record) -> SpiceResult<()> {
        self.0.temperature(r)
    }
    fn ask_model(&self, r: &Record, k: &str) -> SpiceResult<Real> {
        self.0.ask_model(r, k)
    }
    fn set_model(&self, r: &mut Record, k: &str, value: Real) -> SpiceResult<()> {
        self.0.set_model(r, k, value)
    }
}
