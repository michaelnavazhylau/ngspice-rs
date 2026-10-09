//! Diodes under `.sens`: the `dio.c` tables, `diompar.c`/`dioparam.c`
//! setters, `diomask.c`/`dioask.c`, the defaults and setup-only quantities of
//! `diosetup.c` and `DIOtemp` (`diotemp.c`). The load itself is the port's
//! diode ([`Diode`]) built from C's records.
//!
//! `DIOsetup` runs once per parameter **before** the perturbation, so what it
//! alone derives does not follow the perturbed value: the series conductance
//! `1/RS` (an `rs` perturbation changes nothing) and the instance knee
//! currents `IKF*AREA*M`, `IKR*AREA*M`, `IKP*PJ*M`. A knee never given has an
//! instance knee of zero; perturbing it marks it given and `dioload.c`
//! divides by that zero, so C reports NaN, which the port reproduces. Setup
//! also defaults `NBV` to the `N` it sees and clears a knee's *given* flag
//! below `CKTepsmin`, and fields never given read C's defaults (`ISR` reads
//! `1e-14`: perturbing it turns the recombination current on, and restoring
//! it leaves it on for every later parameter of the model).

use super::{Diode, DiodeParameters, EPSMIN};
use crate::devices::sensitivity::{
    CELSIUS_TO_KELVIN, DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily,
    SensitivityLoad, SensitivityMode, SensitivityParameter as P, SensitivityRecord, written,
};
use crate::primitives::{Real, SpiceError, SpiceResult};

/// `DIOmPTable` entries `set_param()` accepts in a DC analysis (the `IF_AC`
/// `tt`, `ttt1`, `ttt2`, `cjo` follow in AC; `tnom`, `rsw` and the SOA and
/// self-heating limits are `IF_NONSENSE`, `level`/`tlev`/`tlevc` integers).
const MODEL: &[P] = &[
    P::dc("is"),
    P::dc("jsw"),
    P::dc("rs"),
    P::dc("trs"),
    P::dc("trs2"),
    P::dc("n"),
    P::dc("ns"),
    P::ac("tt"),
    P::ac("ttt1"),
    P::ac("ttt2"),
    P::ac("cjo"),
    P::dc("vj"),
    P::dc("m"),
    P::dc("tm1"),
    P::dc("tm2"),
    P::dc("cjp"),
    P::dc("php"),
    P::dc("mjsw"),
    P::dc("ikf"),
    P::dc("ikr"),
    P::dc("ikp"),
    P::dc("nbv"),
    P::dc("area"),
    P::dc("pj"),
    P::dc("eg"),
    P::dc("gap1"),
    P::dc("gap2"),
    P::dc("xti"),
    P::dc("cta"),
    P::dc("ctp"),
    P::dc("tpb"),
    P::dc("tphp"),
    P::dc("jtun"),
    P::dc("jtunsw"),
    P::dc("ntun"),
    P::dc("xtitun"),
    P::dc("keg"),
    P::dc("kf"),
    P::dc("af"),
    P::dc("fc"),
    P::dc("fcs"),
    P::dc("bv"),
    P::dc("ibv"),
    P::dc("tcv"),
    P::dc("isr"),
    P::dc("nr"),
    P::dc("vp"),
    P::dc("qpscale"),
    P::dc("lm"),
    P::dc("lp"),
    P::dc("wm"),
    P::dc("wp"),
    P::dc("xom"),
    P::dc("xoi"),
    P::dc("xm"),
    P::dc("xp"),
    P::dc("xw"),
];

/// `DIOpTable` entries `set_param()` accepts (`ic` is `IF_AC`).
const INSTANCE: &[P] = &[
    P::dc("temp"),
    P::dc("dtemp"),
    P::ac("ic"),
    P::dc("area"),
    P::dc("pj"),
    P::dc("w"),
    P::dc("l"),
    P::dc("m"),
    P::dc("lm"),
    P::dc("lp"),
    P::dc("wm"),
    P::dc("wp"),
];

/// Model fields beyond the table: `(keyword, port setter name)`.
const MODEL_FIELDS: &[&str] = &[
    "is",
    "jsw",
    "rs",
    "trs",
    "trs2",
    "n",
    "ns",
    "tt",
    "ttt1",
    "ttt2",
    "cjo",
    "vj",
    "m",
    "tm1",
    "tm2",
    "cjp",
    "php",
    "mjsw",
    "ikf",
    "ikr",
    "ikp",
    "nbv",
    "area",
    "pj",
    "eg",
    "gap1",
    "gap2",
    "xti",
    "cta",
    "ctp",
    "tpb",
    "tphp",
    "jtun",
    "jtunsw",
    "ntun",
    "xtitun",
    "keg",
    "kf",
    "af",
    "fc",
    "fcs",
    "bv",
    "ibv",
    "tcv",
    "isr",
    "nr",
    "vp",
    "qpscale",
    "lm",
    "lp",
    "wm",
    "wp",
    "xom",
    "xoi",
    "xm",
    "xp",
    "xw",
    "tnom",
    "tlev",
    "tlevc",
    "conductance",
];
const INSTANCE_FIELDS: &[&str] = &[
    "temp", "dtemp", "ic", "area", "pj", "w", "l", "m", "lm", "lp", "wm", "wp", "ikf", "ikr", "ikp",
];

/// The port's canonical setter name of a C model field (`cjp`/`php` are the
/// port's `cjsw`/`vjsw`, `trs` its `trs`).
fn port_name(keyword: &str) -> &str {
    match keyword {
        "cjp" => "cjsw",
        "php" => "vjsw",
        other => other,
    }
}

/// A diode's `.sens` description.
#[derive(Debug)]
pub(crate) struct DiodeSensitivity<'a> {
    diode: &'a Diode,
    circuit_kelvin: Real,
    nominal_kelvin: Real,
}

impl<'a> DiodeSensitivity<'a> {
    pub(crate) fn new(diode: &'a Diode, context: &crate::devices::ModelContext) -> Self {
        Self {
            diode,
            circuit_kelvin: context.temperature + CELSIUS_TO_KELVIN,
            nominal_kelvin: context.nominal_temperature + CELSIUS_TO_KELVIN,
        }
    }
}

fn default(record: &mut SensitivityRecord, keyword: &str, value: Real) -> SpiceResult<()> {
    if record.given(keyword) {
        Ok(())
    } else {
        record.store(keyword, value)
    }
}

impl DeviceSensitivity for DiodeSensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        SensitivityFamily::Diode
    }

    fn model(&self) -> Option<&str> {
        Some(&self.diode.model)
    }

    fn model_parameters(&self) -> &'static [P] {
        MODEL
    }

    fn instance_parameters(&self) -> &'static [P] {
        INSTANCE
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let (m, i) = &*self.diode.written;
        let mut model = SensitivityRecord::new();
        for keyword in MODEL_FIELDS {
            let value = written(m, port_name(keyword));
            let value = match (*keyword, value) {
                ("tnom", Some(celsius)) => Some(celsius + CELSIUS_TO_KELVIN),
                (_, value) => value,
            };
            model = model.with(keyword, value.unwrap_or(0.), value.is_some());
        }
        let mut instance = SensitivityRecord::new();
        for keyword in INSTANCE_FIELDS {
            let value = match *keyword {
                "ikf" | "ikr" | "ikp" => None,
                "ic" => self.diode.initial.values.first().copied().flatten(),
                "temp" => written(i, "temp").map(|t| t + CELSIUS_TO_KELVIN),
                other => written(i, other),
            };
            instance = instance.with(keyword, value.unwrap_or(0.), value.is_some());
        }
        self.setup(&mut model, &mut instance)?;
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }

    /// `DIOmAsk` reads `is` no lower than `CKTepsmin`; `DIOask` the
    /// temperature in Celsius.
    fn ask(
        &self,
        scope: ParameterScope,
        keyword: &str,
        records: RecordPair<'_>,
    ) -> SpiceResult<Real> {
        Ok(match (scope, keyword) {
            (ParameterScope::Model, "is") => records.model.value("is")?.max(EPSMIN),
            (ParameterScope::Model, _) => records.model.value(keyword)?,
            (ParameterScope::Instance, "temp") => {
                records.instance.value("temp")? - CELSIUS_TO_KELVIN
            }
            (ParameterScope::Instance, _) => records.instance.value(keyword)?,
        })
    }

    /// `DIOmParam`/`DIOparam`: `xom`/`xoi` are Angstrom, `temp` Celsius.
    fn set(
        &self,
        scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        match (scope, keyword) {
            (ParameterScope::Model, "xom" | "xoi") => model.set(keyword, value * 1e-10),
            (ParameterScope::Model, _) => model.set(keyword, value),
            (ParameterScope::Instance, "temp") => instance.set("temp", value + CELSIUS_TO_KELVIN),
            (ParameterScope::Instance, _) => instance.set(keyword, value),
        }
    }

    /// `DIOsetup` (level 1, no `scale` option).
    fn setup(
        &self,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        for (keyword, value) in [
            ("n", 1.),
            ("is", 1e-14),
            ("jsw", 0.),
            ("ns", 1.),
            ("ibv", 1e-3),
            ("vj", 1.),
            ("m", 0.5),
            ("tm1", 0.),
            ("tm2", 0.),
            ("fc", 0.5),
            ("fcs", 0.5),
            ("tt", 0.),
            ("ttt1", 0.),
            ("ttt2", 0.),
            ("cjo", 0.),
            ("cjp", 0.),
            ("php", 1.),
            ("mjsw", 0.33),
        ] {
            default(model, keyword, value)?;
        }
        for knee in ["ikf", "ikr", "ikp"] {
            if model.given(knee) && model.value(knee)? < EPSMIN {
                model.set_given(knee, false)?;
            }
        }
        let n = model.value("n")?;
        default(model, "nbv", n)?;
        default(model, "tlev", 0.)?;
        default(model, "tlevc", 0.)?;
        let eg = if model.value("tlev")? == 2. {
            1.16
        } else {
            1.11
        };
        for (keyword, value) in [
            ("eg", eg),
            ("gap1", 7.02e-4),
            ("gap2", 1108.),
            ("xti", 3.),
            ("cta", 0.),
            ("ctp", 0.),
            ("tpb", 0.),
            ("tphp", 0.),
            ("kf", 0.),
            ("af", 1.),
            ("trs", 0.),
            ("trs2", 0.),
            ("tcv", 0.),
            ("area", 1.),
            ("pj", 0.),
            ("jtun", 0.),
            ("jtunsw", 0.),
            ("ntun", 30.),
            ("xtitun", 3.),
            ("keg", 1.),
            ("nr", 2.),
            ("isr", 1e-14),
            ("vp", 0.),
            ("lm", 0.),
            ("lp", 0.),
            ("wm", 0.),
            ("wp", 0.),
            ("xom", 1e4),
            ("xoi", 1e4),
            ("xm", 0.),
            ("xp", 0.),
            ("xw", 0.),
        ] {
            default(model, keyword, value)?;
        }
        if !model.given("qpscale") || model.value("qpscale")? <= 0. {
            model.store("qpscale", 1e6)?;
        }
        if model.value("is")? < EPSMIN {
            model.store("is", EPSMIN)?;
        }
        default(model, "tnom", self.nominal_kelvin)?;
        let rs = model.value("rs")?;
        let conductance = if model.given("rs") && rs != 0. {
            1. / rs
        } else {
            0.
        };
        model.store("conductance", conductance)?;
        let geometry = instance.given("w") || instance.given("l");
        if !instance.given("area") {
            let area = if geometry { 1. } else { model.value("area")? };
            instance.store("area", area)?;
        }
        if !instance.given("pj") {
            let pj = if geometry { 0. } else { model.value("pj")? };
            instance.store("pj", pj)?;
        }
        if !instance.given("m") || instance.value("m")? <= 0. {
            instance.store("m", 1.)?;
        }
        if instance.value("area")? <= 0. {
            instance.store("area", 1.)?;
        }
        let (area, pj, m) = (
            instance.value("area")?,
            instance.value("pj")?,
            instance.value("m")?,
        );
        instance.store("ikf", model.value("ikf")? * area * m)?;
        instance.store("ikr", model.value("ikr")? * area * m)?;
        instance.store("ikp", model.value("ikp")? * pj * m)
    }

    /// `DIOtemp`: the instance temperature (`CKTtemp + dtemp` unless given).
    fn temperature(
        &self,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if !instance.given("dtemp") {
            instance.store("dtemp", 0.)?;
        }
        if !instance.given("temp") {
            let dtemp = instance.value("dtemp")?;
            instance.store("temp", self.circuit_kelvin + dtemp)?;
        }
        Ok(())
    }

    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        if mode == SensitivityMode::Ac {
            return Err(SpiceError::circuit("diode AC sensitivity is refused"));
        }
        let (m, i) = (current.model, current.instance);
        let given = |keyword: &str| -> SpiceResult<Option<Real>> {
            Ok(model_given(m, keyword)
                .then(|| m.value(keyword))
                .transpose()?)
        };
        let selector = |keyword: &str| -> SpiceResult<u8> {
            let value = m.value(keyword)?;
            match value {
                0. => Ok(0),
                1. => Ok(1),
                2. => Ok(2),
                _ => Err(SpiceError::circuit(format!(
                    "diode {keyword}={value} in .sens"
                ))),
            }
        };
        let (area, pj, multiplier) = (i.value("area")?, i.value("pj")?, i.value("m")?);
        let conductance = m.value("conductance")?;
        // The instance knee currents of the last setup, scaled back for the
        // port's `knee * area * m` (`m` and `area` may have moved since).
        let knee = |keyword: &str, scale: Real| -> SpiceResult<Option<Real>> {
            Ok(m.given(keyword)
                .then(|| i.value(keyword))
                .transpose()?
                .map(|k| k / scale))
        };
        let parameters = DiodeParameters {
            is: m.value("is")?,
            jsw: given("jsw")?,
            n: m.value("n")?,
            ns: m.value("ns")?,
            ns_given: m.given("ns"),
            // diosetup.c alone derives 1/RS; diotemp.c divides by the TRS
            // factor only when RS is (still) given and non-zero.
            rs: if conductance != 0. && m.given("rs") && m.value("rs")? != 0. {
                1. / conductance
            } else {
                0.
            },
            trs1: m.value("trs")?,
            trs2: m.value("trs2")?,
            tt: m.value("tt")?,
            ttt1: m.value("ttt1")?,
            ttt2: m.value("ttt2")?,
            cjo: m.value("cjo")?,
            vj: m.value("vj")?,
            grading: m.value("m")?,
            tm1: m.value("tm1")?,
            tm2: m.value("tm2")?,
            fc: m.value("fc")?,
            cjsw: m.value("cjp")?,
            vjsw: m.value("php")?,
            mjsw: m.value("mjsw")?,
            fcs: m.value("fcs")?,
            bv: given("bv")?,
            ibv: m.value("ibv")?,
            nbv: m.value("nbv")?,
            tcv: m.value("tcv")?,
            tlev: selector("tlev")?,
            tlevc: selector("tlevc")?,
            eg: m.value("eg")?,
            gap1: m.value("gap1")?,
            gap2: m.value("gap2")?,
            xti: m.value("xti")?,
            cta: m.value("cta")?,
            ctp: m.value("ctp")?,
            tpb: m.value("tpb")?,
            tphp: m.value("tphp")?,
            isr: given("isr")?,
            nr: m.value("nr")?,
            ikf: knee("ikf", area * multiplier)?,
            ikr: knee("ikr", area * multiplier)?,
            ikp: knee("ikp", pj * multiplier)?,
            jtun: given("jtun")?,
            jtunsw: given("jtunsw")?,
            ntun: m.value("ntun")?,
            xtitun: m.value("xtitun")?,
            keg: m.value("keg")?,
            area,
            perimeter: pj,
            multiplier,
            temperature: Some(i.value("temp")? - CELSIUS_TO_KELVIN),
            dtemp: 0.,
            nominal: Some(m.value("tnom")? - CELSIUS_TO_KELVIN),
            kf: m.value("kf")?,
            af: m.value("af")?,
        };
        parameters.check(&self.diode.location)?;
        Ok(SensitivityLoad::Replacement(Box::new(Diode {
            name: self.diode.name.clone(),
            model: self.diode.model.clone(),
            terminals: self.diode.terminals.clone(),
            junction: self.diode.junction,
            parameters,
            initial: self.diode.initial.clone(),
            location: self.diode.location.clone(),
            written: self.diode.written.clone(),
            lenient: true,
        })))
    }
}

/// A model field's *given* flag, with the knees' flag as `DIOsetup` left it.
fn model_given(model: &SensitivityRecord, keyword: &str) -> bool {
    model.given(keyword)
}
