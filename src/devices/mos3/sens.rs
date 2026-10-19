//! Level-3 (and level-9) model records, `mos3.c`/`mos3temp.c`/`mos3mask.c`.
use super::Level3;
use crate::devices::ModelContext;
use crate::devices::mos::sens::{
    INSTANCE, SensitivityLevel, optional, record_common, setup_process, update_common,
};
use crate::devices::sensitivity::{
    SensitivityFamily, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::primitives::{Real, SpiceResult};

impl SensitivityLevel for Level3 {
    const FAMILY: SensitivityFamily = SensitivityFamily::Mos3;
    const MODEL: &'static [P] = &[
        P::dc("vto"),
        P::dc("kp"),
        P::dc("gamma"),
        P::dc("phi"),
        P::dc("rd"),
        P::dc("rs"),
        P::ac("cbd"),
        P::ac("cbs"),
        P::dc("is"),
        P::dc("pb"),
        P::ac("cgso"),
        P::ac("cgdo"),
        P::ac("cgbo"),
        P::dc("rsh"),
        P::ac("cj"),
        P::dc("mj"),
        P::ac("cjsw"),
        P::dc("mjsw"),
        P::dc("js"),
        P::dc("tox"),
        P::dc("ld"),
        P::dc("xl"),
        P::dc("wd"),
        P::dc("xw"),
        P::dc("delvto"),
        P::dc("u0"),
        P::dc("fc"),
        P::dc("nsub"),
        P::dc("nss"),
        P::dc("vmax"),
        P::dc("xj"),
        P::dc("nfs"),
        P::dc("xd"),
        P::dc("alpha"),
        P::dc("eta"),
        P::dc("delta"),
        P::dc("input_delta"),
        P::dc("theta"),
        P::dc("kappa"),
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
            ("tox", self.tox, true),
            ("u0", self.u0.unwrap_or(600.), self.u0.is_some()),
            ("nsub", self.nsub.unwrap_or(0.), self.nsub.is_some()),
            ("nss", self.nss.unwrap_or(0.), self.nss.is_some()),
            ("xl", self.xl, true),
            ("wd", self.wd, true),
            ("xw", self.xw, true),
            ("delvto", self.delvto, true),
            ("vmax", self.vmax, true),
            ("xj", self.xj, true),
            ("nfs", self.nfs, true),
            ("eta", self.eta, true),
            ("delta", self.delta, true),
            ("theta", self.theta, true),
            ("kappa", self.kappa, true),
            ("xd", 0., false),
            ("alpha", 0., false),
            ("narrow", 0., false),
            ("input_delta", self.delta, true),
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
        m.tox = r.value("tox")?;
        m.u0 = optional(r, "u0", derived)?;
        m.nsub = optional(r, "nsub", false)?;
        m.nss = optional(r, "nss", false)?;
        m.xl = r.value("xl")?;
        m.wd = r.value("wd")?;
        m.xw = r.value("xw")?;
        m.delvto = r.value("delvto")?;
        m.vmax = r.value("vmax")?;
        m.xj = r.value("xj")?;
        m.nfs = r.value("nfs")?;
        m.eta = r.value("eta")?;
        m.delta = r.value("delta")?;
        m.theta = r.value("theta")?;
        m.kappa = r.value("kappa")?;
        Ok(m)
    }
    fn setup(&self, r: &mut Record) -> SpiceResult<()> {
        setup_process(r, Some(("kp", 2e-5)))
    }
    fn temperature(&self, r: &mut Record) -> SpiceResult<()> {
        let process = self
            .with_record(r, false)?
            .process(r.value("tnom")?, "MOS3")?;
        for (k, value) in [
            ("vto", process.vto),
            ("kp", process.kp),
            ("gamma", process.gamma),
            ("phi", process.phi),
            ("u0", process.u0),
            ("xd", process.depletion),
            ("alpha", process.alpha),
            ("narrow", process.narrow),
        ] {
            r.store(k, value)?;
        }
        Ok(())
    }
    fn ask_model(&self, r: &Record, k: &str) -> SpiceResult<Real> {
        // MOS3_MOD_DELTA asks the scaled narrow factor, but its setter writes
        // raw DELTA. MOS3_DELTA (input_delta) asks raw DELTA instead.
        r.value(match k {
            "delta" => "narrow",
            "input_delta" => "delta",
            _ => k,
        })
    }
    fn set_model(&self, r: &mut Record, k: &str, value: Real) -> SpiceResult<()> {
        // These table entries have asks but no MOS3mParam cases. C prints
        // SET ERROR and continues with the unchanged record: zero columns.
        if matches!(k, "xd" | "alpha" | "input_delta") {
            return Ok(());
        }
        r.set(k, value)
    }
}
