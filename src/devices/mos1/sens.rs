//! MOS1 model sensitivity mapping (`mos1.c`, `mos1set.c`, `mos1temp.c`).
use super::Level1;
use crate::devices::ModelContext;
use crate::devices::mos::sens::{
    SensitivityLevel, optional, record_common, setup_process, update_common,
};
use crate::devices::sensitivity::{
    SensitivityFamily, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::primitives::SpiceResult;

impl SensitivityLevel for Level1 {
    const FAMILY: SensitivityFamily = SensitivityFamily::Mos1;
    const MODEL: &'static [P] = &[
        P::dc("vto"),
        P::dc("kp"),
        P::dc("gamma"),
        P::dc("phi"),
        P::dc("lambda"),
        P::dc("rd"),
        P::dc("rs"),
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
        P::dc("tnom"),
        P::dc("kf"),
        P::dc("af"),
        P::dc("gdsnoi"),
    ];
    const INSTANCE: &'static [P] = &[
        P::dc("m"),
        P::dc("l"),
        P::dc("w"),
        P::dc("ad"),
        P::dc("as"),
        P::dc("pd"),
        P::dc("ps"),
        P::dc("nrd"),
        P::dc("nrs"),
        P::dc("icvds"),
        P::dc("icvgs"),
        P::dc("icvbs"),
        P::dc("temp"),
        P::dc("dtemp"),
    ];
    fn record(&self, context: &ModelContext) -> Record {
        let mut r = record_common(&self.common, context);
        for (k, value, given) in [
            ("vto", self.vto.unwrap_or(0.), self.vto.is_some()),
            ("kp", self.kp.unwrap_or(2e-5), self.kp.is_some()),
            ("gamma", self.gamma.unwrap_or(0.), self.gamma.is_some()),
            ("phi", self.phi.unwrap_or(0.6), self.phi.is_some()),
            ("lambda", self.lambda, true),
            ("tox", self.tox.unwrap_or(0.), self.tox.is_some()),
            ("u0", self.u0.unwrap_or(0.), self.u0.is_some()),
            ("nsub", self.nsub.unwrap_or(0.), self.nsub.is_some()),
            ("nss", self.nss.unwrap_or(0.), self.nss.is_some()),
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
        m.tox = optional(r, "tox", false)?;
        m.u0 = optional(r, "u0", false)?;
        m.nsub = optional(r, "nsub", false)?;
        m.nss = optional(r, "nss", false)?;
        Ok(m)
    }
    fn setup(&self, r: &mut Record) -> SpiceResult<()> {
        setup_process(r, Some(("kp", 2e-5)))
    }
    fn temperature(&self, r: &mut Record) -> SpiceResult<()> {
        let process = self.with_record(r, false)?.process(r.value("tnom")?)?;
        for (k, value) in [
            ("vto", process.vto),
            ("kp", process.kp),
            ("gamma", process.gamma),
            ("phi", process.phi),
        ] {
            r.store(k, value)?;
        }
        if r.given("tox") && r.value("tox")? != 0. && !r.given("u0") {
            r.store("u0", 600.)?;
        }
        Ok(())
    }
}
