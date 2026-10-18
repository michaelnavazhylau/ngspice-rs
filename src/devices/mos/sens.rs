//! Shared classic-MOS sensitivity lifecycle. Each level supplies its own
//! `mosX.c` tables, model fields and `mosXtemp.c` process stores.
use super::{CELSIUS_TO_KELVIN, Common, MosLevel, Mosfet};
use crate::devices::ModelContext;
use crate::devices::sensitivity::{
    DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::primitives::{Real, SpiceError, SpiceResult};
use std::cell::OnceCell;

/// Level-owned model record mapping. `derived` selects the already-stored
/// process fields rather than recomputing them in the replacement's load.
pub(crate) trait SensitivityLevel: MosLevel {
    const FAMILY: SensitivityFamily;
    const MODEL: &'static [P];
    const INSTANCE: &'static [P];
    const TNOM_ASK_KELVIN: bool = false;
    fn ask_instance(&self, r: &Record, k: &str) -> SpiceResult<Real> {
        r.value(k)
    }
    fn record(&self, context: &ModelContext) -> Record;
    fn with_record(&self, record: &Record, derived: bool) -> SpiceResult<Self>;
    fn temperature(&self, record: &mut Record) -> SpiceResult<()>;
    fn setup(&self, record: &mut Record) -> SpiceResult<()>;
    fn ask_model(&self, r: &Record, k: &str) -> SpiceResult<Real> {
        r.value(k)
    }
    fn set_model(&self, r: &mut Record, k: &str, value: Real) -> SpiceResult<()> {
        r.set(k, value)
    }
}

/// `mosX.c`: levels 2/3/9 make IC voltages AC-only; level 1 does not.
pub(crate) const INSTANCE: &[P] = &[
    P::dc("m"),
    P::dc("l"),
    P::dc("w"),
    P::dc("ad"),
    P::dc("as"),
    P::dc("pd"),
    P::dc("ps"),
    P::dc("nrd"),
    P::dc("nrs"),
    P::dc("temp"),
    P::dc("dtemp"),
];

/// Common `mosXset.c` fields, retaining optional resistance flags.
pub(crate) fn record_common(c: &Common, context: &ModelContext) -> Record {
    let mut record = Record::new();
    for (k, value, given) in [
        ("rd", c.rd.unwrap_or(0.), c.rd.is_some()),
        ("rs", c.rs.unwrap_or(0.), c.rs.is_some()),
        ("rsh", c.rsh.unwrap_or(0.), c.rsh.is_some()),
        ("is", c.is, true),
        ("js", c.js, true),
        ("pb", c.pb, true),
        ("mj", c.mj, true),
        ("mjsw", c.mjsw, true),
        ("fc", c.fc, true),
        ("ld", c.ld, true),
        (
            "tnom",
            c.tnom.unwrap_or(context.nominal_temperature) + CELSIUS_TO_KELVIN,
            c.tnom.is_some(),
        ),
        ("kf", c.kf, true),
        ("af", c.af, true),
        ("gdsnoi", c.gdsnoi, true),
        ("cbd", c.cbd.unwrap_or(0.), c.cbd.is_some()),
        ("cbs", c.cbs.unwrap_or(0.), c.cbs.is_some()),
    ] {
        record = record.with(k, value, given);
    }
    record
}

/// The common fields read by `mosXtemp.c` and `mosXload.c`.
pub(crate) fn update_common(c: &mut Common, r: &Record) -> SpiceResult<()> {
    c.rd = optional(r, "rd", false)?;
    c.rs = optional(r, "rs", false)?;
    c.rsh = optional(r, "rsh", false)?;
    c.is = r.value("is")?;
    c.js = r.value("js")?;
    c.pb = r.value("pb")?;
    c.mj = r.value("mj")?;
    c.mjsw = r.value("mjsw")?;
    c.fc = r.value("fc")?;
    c.ld = r.value("ld")?;
    c.tnom = Some(r.value("tnom")? - CELSIUS_TO_KELVIN);
    c.kf = r.value("kf")?;
    c.af = r.value("af")?;
    c.gdsnoi = r.value("gdsnoi")?;
    c.cbd = optional(r, "cbd", false)?;
    c.cbs = optional(r, "cbs", false)?;
    Ok(())
}

/// C's field value with the setter flag, or an already-derived field.
pub(crate) fn optional(r: &Record, k: &str, derived: bool) -> SpiceResult<Option<Real>> {
    (derived || r.given(k)).then(|| r.value(k)).transpose()
}

/// `mosXset.c` resets process defaults on every visit until restored/given.
/// `gain` names the level's conductance setter default; `mos2set.c`, unlike
/// levels 1/3/9 (`transconductance`) and 6 (`kc`), has none of its own.
pub(crate) fn setup_process(r: &mut Record, gain: Option<(&str, Real)>) -> SpiceResult<()> {
    let fields = [
        ("vto", 0.),
        (gain.map_or("", |(k, _)| k), gain.map_or(0., |(_, v)| v)),
        ("gamma", 0.),
        ("phi", 0.6),
    ];
    for (k, value) in fields {
        if k.is_empty() || r.given(k) {
            continue;
        }
        r.store(k, value)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct MosSensitivity<'a, L: SensitivityLevel> {
    pub(crate) device: &'a Mosfet<L>,
    pub(crate) context: ModelContext,
}

impl<L: SensitivityLevel> DeviceSensitivity for MosSensitivity<'_, L> {
    fn family(&self) -> SensitivityFamily {
        L::FAMILY
    }
    fn model(&self) -> Option<&str> {
        Some(&self.device.model_name)
    }
    fn model_parameters(&self) -> &'static [P] {
        L::MODEL
    }
    fn instance_parameters(&self) -> &'static [P] {
        L::INSTANCE
    }

    fn records(&self) -> SpiceResult<(Record, Record)> {
        let g = &self.device.geometry;
        let mut model = self.device.model.record(&self.context);
        let mut instance = Record::new();
        for (k, value) in [
            ("m", g.m),
            ("l", g.l),
            ("w", g.w),
            ("ad", g.ad),
            ("as", g.as_),
            ("pd", g.pd),
            ("ps", g.ps),
            ("nrd", g.nrd),
            ("nrs", g.nrs),
            ("dtemp", g.dtemp),
        ] {
            instance = instance.with(k, value, true);
        }
        instance = instance.with("temp", g.kelvin(&self.context), g.temp.is_some());
        for (k, v) in ["icvds", "icvgs", "icvbs"]
            .into_iter()
            .zip(self.device.initial.values.iter().copied())
        {
            instance = instance.with(k, v.unwrap_or(0.), v.is_some());
        }
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }
    fn ask(&self, scope: ParameterScope, k: &str, r: RecordPair<'_>) -> SpiceResult<Real> {
        let value = match scope {
            ParameterScope::Model => self.device.model.ask_model(r.model, k)?,
            ParameterScope::Instance => self.device.model.ask_instance(r.instance, k)?,
        };
        Ok(if k == "temp" || (k == "tnom" && !L::TNOM_ASK_KELVIN) {
            value - CELSIUS_TO_KELVIN
        } else {
            value
        })
    }
    fn set(
        &self,
        scope: ParameterScope,
        k: &str,
        value: Real,
        m: &mut Record,
        i: &mut Record,
    ) -> SpiceResult<()> {
        let value = if matches!(k, "tnom" | "temp") {
            value + CELSIUS_TO_KELVIN
        } else {
            value
        };
        match scope {
            ParameterScope::Model => self.device.model.set_model(m, k, value),
            ParameterScope::Instance => i.set(k, value),
        }
    }
    fn setup(&self, m: &mut Record, _i: &mut Record) -> SpiceResult<()> {
        self.device.model.setup(m)
    }
    fn temperature(&self, m: &mut Record, i: &mut Record) -> SpiceResult<()> {
        if !m.given("tnom") {
            m.store("tnom", self.context.nominal_temperature + CELSIUS_TO_KELVIN)?;
        }
        self.device.model.temperature(m)?;
        if !i.given("temp") {
            i.store(
                "temp",
                (self.context.temperature + CELSIUS_TO_KELVIN) + i.value("dtemp")?,
            )?;
        }
        Ok(())
    }
    fn load(
        &self,
        _setup: RecordPair<'_>,
        r: RecordPair<'_>,
        mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        if mode == SensitivityMode::Ac {
            return Err(SpiceError::not_yet_ported(
                format!("{} AC sensitivity", L::LABEL),
                format!("mos{0}/mos{0}acld.c", L::LEVEL),
            ));
        }
        let model = self.device.model.with_record(r.model, true)?;
        let mut geometry = self.device.geometry;
        let v = |k| r.instance.value(k);
        geometry.m = v("m")?;
        geometry.l = v("l")?;
        geometry.w = v("w")?;
        geometry.ad = v("ad")?;
        geometry.as_ = v("as")?;
        geometry.pd = v("pd")?;
        geometry.ps = v("ps")?;
        geometry.nrd = v("nrd")?;
        geometry.nrs = v("nrs")?;
        geometry.temp = Some(v("temp")? - CELSIUS_TO_KELVIN);
        geometry.dtemp = v("dtemp")?;
        model.check_geometry(&self.device.name, &geometry)?;
        // MOSXtemp, not the card validator: restored RD=0/RS=0 are
        // now given and suppress RSH, without changing the setup-time nodes.
        let c = model.common();
        let conductance = |resistance: Option<Real>, squares: Real| match resistance {
            Some(0.) => 0.,
            Some(r) => geometry.m / r,
            None => c
                .rsh
                .filter(|r| *r != 0.)
                .map_or(0., |r| geometry.m / (r * squares)),
        };
        let series = [
            conductance(c.rd, geometry.nrd),
            conductance(c.rs, geometry.nrs),
        ];
        Ok(SensitivityLoad::Replacement(Box::new(Mosfet {
            name: self.device.name.clone(),
            model_name: self.device.model_name.clone(),
            terminals: self.device.terminals.clone(),
            inner: self.device.inner,
            model,
            geometry,
            series,
            initial: self.device.initial.clone(),
            derived: OnceCell::new(),
        })))
    }
}
