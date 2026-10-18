//! Level-2 record replay: `mos2.c`, `mos2set.c`, `mos2temp.c` (behaviour only).
use super::Level2;
use crate::devices::ModelContext;
use crate::devices::mos::sens::{
    INSTANCE, SensitivityLevel, optional, record_common, setup_process, update_common,
};
use crate::devices::sensitivity::{
    SensitivityFamily, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::primitives::SpiceResult;

impl SensitivityLevel for Level2 {
    const FAMILY: SensitivityFamily = SensitivityFamily::Mos2;
    const MODEL: &'static [P] = &[
        P::dc("vto"),
        P::dc("kp"),
        P::dc("gamma"),
        P::dc("phi"),
        P::dc("lambda"),
        P::dc("rd"),
        P::dc("rs"),
        P::dc("cbd"),
        P::dc("cbs"),
        P::dc("is"),
        P::dc("pb"),
        P::dc("rsh"),
        P::dc("mj"),
        P::dc("mjsw"),
        P::dc("js"),
        P::dc("tox"),
        P::dc("ld"),
        P::dc("u0"),
        P::dc("fc"),
        P::dc("nsub"),
        P::dc("nss"),
        P::dc("delta"),
        P::dc("uexp"),
        P::dc("ucrit"),
        P::dc("vmax"),
        P::dc("xj"),
        P::dc("neff"),
        P::dc("nfs"),
        P::dc("tnom"),
        P::dc("kf"),
        P::dc("af"),
        P::dc("gdsnoi"),
    ];
    const INSTANCE: &'static [P] = INSTANCE;
    fn record(&self, context: &ModelContext) -> Record {
        let mut r = record_common(&self.common, context);
        for (k, value, given) in [
            ("vto", self.vto.unwrap_or(0.), self.vto.is_some()),
            ("kp", self.kp.unwrap_or(2e-5), self.kp.is_some()),
            ("gamma", self.gamma.unwrap_or(0.), self.gamma.is_some()),
            ("phi", self.phi.unwrap_or(0.6), self.phi.is_some()),
            ("lambda", self.lambda, true),
            ("tox", self.tox, true),
            ("u0", self.u0.unwrap_or(600.), self.u0.is_some()),
            ("nsub", self.nsub.unwrap_or(0.), self.nsub.is_some()),
            ("nss", self.nss.unwrap_or(0.), self.nss.is_some()),
            ("delta", self.delta, true),
            ("uexp", self.uexp, true),
            ("ucrit", self.ucrit, true),
            ("vmax", self.vmax, true),
            ("xj", self.xj, true),
            ("neff", self.neff, true),
            ("nfs", self.nfs, true),
        ] {
            r = r.with(k, value, given);
        }
        r
    }
    fn with_record(&self, r: &Record, derived: bool) -> SpiceResult<Self> {
        let mut m = *self;
        update_common(&mut m.common, r)?;
        m.vto = optional(r, "vto", derived)?;
        m.kp = optional(r, "kp", derived)?;
        m.gamma = optional(r, "gamma", derived)?;
        m.phi = optional(r, "phi", derived)?;
        m.lambda = r.value("lambda")?;
        m.tox = r.value("tox")?;
        m.u0 = optional(r, "u0", derived)?;
        m.nsub = optional(r, "nsub", false)?;
        m.nss = optional(r, "nss", false)?;
        m.delta = r.value("delta")?;
        m.uexp = r.value("uexp")?;
        m.ucrit = r.value("ucrit")?;
        m.vmax = r.value("vmax")?;
        m.xj = r.value("xj")?;
        m.neff = r.value("neff")?;
        m.nfs = r.value("nfs")?;
        Ok(m)
    }
    fn setup(&self, r: &mut Record) -> SpiceResult<()> {
        // mos2set.c has no transconductance default: mos2temp.c derives KP.
        setup_process(r, None)
    }
    fn temperature(&self, r: &mut Record) -> SpiceResult<()> {
        let process = self.with_record(r, false)?.process(r.value("tnom")?)?;
        for (k, value) in [
            ("vto", process.vto),
            ("kp", process.kp),
            ("gamma", process.gamma),
            ("phi", process.phi),
            ("u0", process.u0),
        ] {
            r.store(k, value)?;
        }
        Ok(())
    }
}
