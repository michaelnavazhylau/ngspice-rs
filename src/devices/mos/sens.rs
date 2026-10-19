//! Shared classic-MOS sensitivity lifecycle. Each level supplies its own
//! `mosX.c` tables, model fields and `mosXtemp.c` process stores.
use super::{CELSIUS_TO_KELVIN, Common, MosLevel, Mosfet};
use crate::devices::ModelContext;
use crate::devices::sensitivity::{
    AcReset, DeviceSensitivity, ParameterScope, RecordPair, ResetStamp, ResetStamps,
    SensitivityFamily, SensitivityLoad, SensitivityMode, SensitivityParameter as P,
    SensitivityRecord as Record, StampAt, StampPart,
};
use crate::devices::{Device, LinearContext, StampContext};
use crate::primitives::{NodeId, Real, SpiceError, SpiceResult};
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
    P::ac("icvds"),
    P::ac("icvgs"),
    P::ac("icvbs"),
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
        // `mosXset.c` zeroes the ungiven overlap factors, CJ and CJSW (MOS2
        // derives its CJ in temperature); only the overlaps reach the AC
        // reset load.
        ("cgso", c.overlap[0], true),
        ("cgdo", c.overlap[1], true),
        ("cgbo", c.overlap[2], true),
        ("cj", c.cj.unwrap_or(0.), c.cj.is_some()),
        ("cjsw", c.cjsw.unwrap_or(0.), c.cjsw.is_some()),
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
    c.overlap = [r.value("cgso")?, r.value("cgdo")?, r.value("cgbo")?];
    c.cj = optional(r, "cj", false)?;
    c.cjsw = optional(r, "cjsw", false)?;
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
            return Err(SpiceError::circuit(format!(
                "{}'s AC sensitivity load is its reset load",
                L::LABEL
            )));
        }
        Ok(SensitivityLoad::Replacement(Box::new(self.mosfet(r)?)))
    }

    /// `capbd`/`capbs` (`mosXacld.c`).
    fn stale_ac_fields(&self) -> Option<&'static str> {
        Some(match L::LEVEL {
            1 => "src/spicelib/devices/mos1/mos1acld.c",
            2 => "src/spicelib/devices/mos2/mos2acld.c",
            3 => "src/spicelib/devices/mos3/mos3acld.c",
            9 => "src/spicelib/devices/mos9/mos9acld.c",
            _ => "src/spicelib/devices/mos6/mos6init.c",
        })
    }

    /// `mosXacld.c` after `cktsens.c`'s reset load: `gm`, `gds`, `gmbs`,
    /// `gbd`, `gbs` and the mode are what `mosXload.c` left in the instance
    /// at zero terminal voltages with the reset records; the Meyer halves in
    /// `CKTstate0` are zero; `capbd`/`capbs`, which only a small-signal or
    /// transient load writes, are what an earlier analysis's
    /// `MODEINITSMSIG` load left at the operating point
    /// ([`AcReset::small_signal`], with the circuit's own device, as the
    /// walk had not started) or C's zero-initialised fields; the series
    /// conductances and overlap capacitances come from `current`.
    fn reset_ac_load(
        &self,
        _setup: RecordPair<'_>,
        reset: AcReset<'_>,
        current: RecordPair<'_>,
    ) -> SpiceResult<SensitivityLoad> {
        L::small_signal(self.device, "AC")?;
        Ok(SensitivityLoad::Replacement(Box::new(ResetMosfet {
            state: self.mosfet(reset.state)?,
            current: self.mosfet(current)?,
            omega: reset.omega,
            stale: reset
                .small_signal
                .map(|bias| (self.pristine(), bias.clone())),
        })))
    }
}

impl<L: SensitivityLevel> MosSensitivity<'_, L> {
    /// A copy of the circuit's own device.
    fn pristine(&self) -> Mosfet<L> {
        let d = self.device;
        Mosfet {
            name: d.name.clone(),
            model_name: d.model_name.clone(),
            terminals: d.terminals.clone(),
            inner: d.inner,
            model: d.model,
            geometry: d.geometry,
            series: d.series,
            initial: d.initial.clone(),
            derived: OnceCell::new(),
        }
    }

    /// The port's MOSFET with the records' parameters.
    fn mosfet(&self, r: RecordPair<'_>) -> SpiceResult<Mosfet<L>> {
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
        Ok(Mosfet {
            name: self.device.name.clone(),
            model_name: self.device.model_name.clone(),
            terminals: self.device.terminals.clone(),
            inner: self.device.inner,
            model,
            geometry,
            series,
            initial: self.device.initial.clone(),
            derived: OnceCell::new(),
        })
    }
}

/// A classic MOSFET's AC load in `cktsens.c`'s sweep
/// ([`DeviceSensitivity::reset_ac_load`]).
#[derive(Debug)]
struct ResetMosfet<L: MosLevel> {
    /// Built from the reset load's records: the conductances and the mode.
    state: Mosfet<L>,
    /// Built from the current records: series conductances and overlaps.
    current: Mosfet<L>,
    /// `CKTomega`.
    omega: Real,
    /// The circuit's own device and the operating point at which an earlier
    /// analysis's small-signal load wrote `capbd`/`capbs`.
    stale: Option<(Mosfet<L>, crate::maths::Vector)>,
}

impl<L: MosLevel> Device for ResetMosfet<L> {
    fn name(&self) -> &str {
        &self.current.name
    }

    fn designator(&self) -> char {
        'm'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.current.terminals
    }

    fn stamp(&self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::circuit(format!(
            "{}: an AC sensitivity stand-in has no DC load",
            self.current.name
        )))
    }

    /// `mos1acld.c`'s stamps (`mos3acld.c`'s with the effective width) in
    /// its order: the imaginary parts `xg* = cap * CKTomega` and their sums,
    /// then the real parts.
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        let model = context.model_context;
        let state = &self.state;
        let op = state.operating(model)?;
        let e =
            state.small_signal_eval(&op, |_| 0., L::junction_gmin(model.gmin, &state.geometry))?;
        let overlap = self.current.operating(model)?.overlap;
        let omega = self.omega;
        // `state0 + state0 + overlap` with zero Meyer halves.
        let [xgs, xgd, xgb] = std::array::from_fn(|k| (0. + 0. + overlap[k]) * omega);
        let (capbd, capbs) = match &self.stale {
            Some((device, bias)) => {
                let unknowns = context.unknowns;
                let op = device.operating(model)?;
                let e = device.small_signal_eval(
                    &op,
                    |node| {
                        unknowns
                            .node_row(node)
                            .and_then(|r| bias.get(r))
                            .unwrap_or(0.)
                    },
                    L::junction_gmin(model.gmin, &device.geometry),
                )?;
                (e.capbd, e.capbs)
            }
            None => (0., 0.),
        };
        let (xbd, xbs) = (capbd * omega, capbs * omega);
        let (xnrm, xrev) = if e.mode < 0. { (0., 1.) } else { (1., 0.) };
        let [d, g, s, b] = [
            self.current.terminals[0],
            self.current.terminals[1],
            self.current.terminals[2],
            self.current.terminals[3],
        ];
        let [dp, _, sp, _] = self.current.inner;
        let [gdr, gsr] = self.current.series;
        let (gm, gds, gmbs, gbd, gbs) = (e.gm, e.gds, e.gmbs, e.gbd, e.gbs);
        let imaginary = [
            (g, g, xgd + xgs + xgb),
            (b, b, xgb + xbd + xbs),
            (dp, dp, xgd + xbd),
            (sp, sp, xgs + xbs),
            (g, b, -xgb),
            (g, dp, -xgd),
            (g, sp, -xgs),
            (b, g, -xgb),
            (b, dp, -xbd),
            (b, sp, -xbs),
            (dp, g, -xgd),
            (dp, b, -xbd),
            (sp, g, -xgs),
            (sp, b, -xbs),
        ];
        let real = [
            (d, d, gdr),
            (s, s, gsr),
            (b, b, gbd + gbs),
            (dp, dp, gdr + gds + gbd + xrev * (gm + gmbs)),
            (sp, sp, gsr + gds + gbs + xnrm * (gm + gmbs)),
            (d, dp, -gdr),
            (s, sp, -gsr),
            (b, dp, -gbd),
            (b, sp, -gbs),
            (dp, d, -gdr),
            (dp, g, (xnrm - xrev) * gm),
            (dp, b, -gbd + (xnrm - xrev) * gmbs),
            (dp, sp, -(gds + xnrm * (gm + gmbs))),
            (sp, g, -((xnrm - xrev) * gm)),
            (sp, s, -gsr),
            (sp, b, -(gbs + (xnrm - xrev) * gmbs)),
            (sp, dp, -(gds + xrev * (gm + gmbs))),
        ];
        let stamps = imaginary
            .into_iter()
            .map(|(a, b, value)| (a, b, value, StampPart::Imaginary))
            .chain(
                real.into_iter()
                    .map(|(a, b, value)| (a, b, value, StampPart::Real)),
            )
            .map(|(a, b, value, part)| ResetStamp {
                at: StampAt::Nodes(a, b),
                value,
                part,
            })
            .collect();
        ResetStamps::new(&self.current.name, 'm', &self.current.terminals, stamps)?
            .assemble_linear(context)
    }
}
