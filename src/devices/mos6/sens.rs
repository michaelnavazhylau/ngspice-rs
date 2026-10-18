//! Level-6 DC record replay (`mos6.c`, `mos6set.c`, `mos6temp.c`).
use super::Level6;
use crate::devices::ModelContext;
use crate::devices::mos::sens::{
    SensitivityLevel, optional, record_common, setup_process, update_common,
};
use crate::devices::sensitivity::{
    SensitivityFamily, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::primitives::SpiceResult;

impl SensitivityLevel for Level6 {
    const FAMILY: SensitivityFamily = SensitivityFamily::Mos6;
    // mos6mask.c returns kelvin; mos6mpar.c nevertheless adds CONSTCtoK.
    const TNOM_ASK_KELVIN: bool = true;
    fn ask_instance(&self, r: &Record, k: &str) -> SpiceResult<crate::primitives::Real> {
        let scale = if matches!(k, "w" | "ad" | "as" | "pd" | "ps" | "nrd" | "nrs") {
            r.value("m")?
        } else {
            1.
        };
        Ok(r.value(k)? * scale)
    }
    const MODEL: &'static [P] = &[
        P::dc("vto"),
        P::dc("kv"),
        P::dc("nv"),
        P::dc("kc"),
        P::dc("nc"),
        P::dc("nvth"),
        P::dc("ps"),
        P::dc("gamma"),
        P::dc("gamma1"),
        P::dc("sigma"),
        P::dc("phi"),
        P::dc("lambda"),
        P::dc("lambda0"),
        P::dc("lambda1"),
        P::dc("rd"),
        P::dc("rs"),
        P::dc("is"),
        P::dc("pb"),
        P::dc("rsh"),
        P::dc("mj"),
        P::dc("mjsw"),
        P::dc("js"),
        P::dc("ld"),
        P::dc("tox"),
        P::dc("u0"),
        P::dc("fc"),
        P::dc("nsub"),
        P::dc("nss"),
        P::dc("tnom"),
    ];
    const INSTANCE: &'static [P] = &[
        P::dc("l"),
        P::dc("w"),
        P::dc("m"),
        P::dc("ad"),
        P::dc("as"),
        P::dc("pd"),
        P::dc("ps"),
        P::dc("nrd"),
        P::dc("nrs"),
        P::dc("temp"),
        P::dc("dtemp"),
    ];
    fn record(&self, context: &ModelContext) -> Record {
        let mut r = record_common(&self.common, context);
        for (k, value, given) in [
            ("vto", self.vto.unwrap_or(0.), self.vto.is_some()),
            ("kc", self.kc.unwrap_or(5e-5), self.kc.is_some()),
            ("gamma", self.gamma.unwrap_or(0.), self.gamma.is_some()),
            ("phi", self.phi.unwrap_or(0.6), self.phi.is_some()),
            ("tox", self.tox.unwrap_or(0.), self.tox.is_some()),
            ("u0", self.u0.unwrap_or(0.), self.u0.is_some()),
            ("nsub", self.nsub.unwrap_or(0.), self.nsub.is_some()),
            ("nss", self.nss.unwrap_or(0.), self.nss.is_some()),
            ("kv", self.kv, true),
            ("nv", self.nv, true),
            ("nc", self.nc, true),
            ("nvth", self.nvth, true),
            ("ps", self.ps, true),
            ("gamma1", self.gamma1, true),
            ("sigma", self.sigma, true),
            ("lambda", self.lambda.unwrap_or(0.), self.lambda.is_some()),
            ("lambda0", self.lambda0, self.lambda0_given),
            ("lambda1", self.lambda1, true),
        ] {
            r = r.with(k, value, given);
        }
        r
    }
    fn with_record(&self, r: &Record, derived: bool) -> SpiceResult<Self> {
        let mut m = *self;
        update_common(&mut m.common, r)?;
        m.vto = optional(r, "vto", derived)?;
        m.kc = optional(r, "kc", derived)?;
        m.gamma = optional(r, "gamma", derived)?;
        m.phi = optional(r, "phi", derived)?;
        m.tox = optional(r, "tox", false)?;
        m.u0 = optional(r, "u0", false)?;
        m.nsub = optional(r, "nsub", false)?;
        m.nss = optional(r, "nss", false)?;
        m.kv = r.value("kv")?;
        m.nv = r.value("nv")?;
        m.nc = r.value("nc")?;
        m.gamma1 = r.value("gamma1")?;
        m.sigma = r.value("sigma")?;
        m.lambda0 = r.value("lambda0")?;
        m.lambda1 = r.value("lambda1")?;
        m.lambda = optional(r, "lambda", false)?;
        m.lambda0_given = r.given("lambda0");
        m.nvth = r.value("nvth")?;
        m.ps = r.value("ps")?;
        Ok(m)
    }
    fn setup(&self, r: &mut Record) -> SpiceResult<()> {
        setup_process(r, Some(("kc", 5e-5)))?;
        // LAMBDA does not directly feed the channel. Only setup copies it
        // into ungiven LAMDA0; perturbing LAMBDA after setup has no effect.
        if !r.given("lambda0") {
            r.store("lambda0", r.value("lambda")?)?;
        }
        Ok(())
    }
    fn temperature(&self, r: &mut Record) -> SpiceResult<()> {
        let process = self.with_record(r, false)?.process(r.value("tnom")?)?;
        for (k, value) in [
            ("vto", process.vto),
            ("kc", process.kc),
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
