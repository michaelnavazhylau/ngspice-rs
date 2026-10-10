//! Capacitors and inductors under `.sens`: `cap.c`/`ind.c` tables, the
//! setters of `capparam.c`/`capmpar.c`/`indparam.c`/`indmpar.c`, the defaults
//! of `capsetup.c`/`indsetup.c`, `captemp.c`/`indtemp.c` and the AC loads
//! `capacld.c`/`indacld.c`. Their DC loads do not depend on any parameter (a
//! capacitor stamps nothing at the operating point, an inductor a short), so
//! every DC sensitivity of theirs is exactly zero.

use super::{
    CELSIUS_TO_KELVIN, DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily,
    SensitivityLoad, SensitivityMode, SensitivityParameter as P, SensitivityRecord,
};
use crate::devices::{Device, LinearContext, StampContext};
use crate::primitives::{NodeId, Real, SpiceError, SpiceResult};

/// `CAPmPTable` entries `set_param()` accepts.
const CAP_MODEL: &[P] = &[
    P::ac("cap"),
    P::ac("cj"),
    P::ac("cjsw"),
    P::ac("narrow"),
    P::ac("short"),
    P::ac("del"),
    P::ac("tc1"),
    P::ac("tc2"),
    P::ac("di"),
    P::ac("thick"),
    P::dc("bv_max"),
];

/// `CAPpTable` entries `set_param()` accepts.
const CAP_INSTANCE: &[P] = &[
    P::principal("capacitance", true),
    P::ac("ic"),
    P::dc("temp"),
    P::dc("dtemp"),
    P::ac("w"),
    P::ac("l"),
    P::dc("m"),
    P::dc("tc1"),
    P::dc("tc2"),
    P::dc("bv_max"),
    P::dc("scale"),
];

/// `INDmPTable` entries `set_param()` accepts (`tnom` is `IF_NONSENSE`).
const IND_MODEL: &[P] = &[
    P::ac("ind"),
    P::ac("tc1"),
    P::ac("tc2"),
    P::ac("csect"),
    P::ac("dia"),
    P::ac("length"),
    P::ac("nt"),
    P::ac("mu"),
];

/// `INDpTable` entries `set_param()` accepts.
const IND_INSTANCE: &[P] = &[
    P::principal("inductance", true),
    P::ac("ic"),
    P::dc("temp"),
    P::dc("dtemp"),
    P::dc("m"),
    P::dc("tc1"),
    P::dc("tc2"),
    P::dc("scale"),
    P::dc("nt"),
];

/// `CONSTepsZero`, `CONSTepsSiO2` (`const.h`).
const EPS_ZERO: Real = 8.854214871e-12;
const EPS_SIO2: Real = 3.9 * EPS_ZERO;
/// `CONSTmuZero`.
const MU_ZERO: Real = 4.0 * std::f64::consts::PI * 1e-7;

/// Which reactive element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReactiveKind {
    /// A capacitor.
    Capacitor,
    /// An inductor.
    Inductor,
}

/// The values a capacitor or inductor card gave, `None` where it gave none
/// (keywords as in C's tables).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ReactiveInputs {
    /// Written instance setters: `(keyword, value)`; `temp` in Celsius.
    pub instance: Vec<(&'static str, Real)>,
    /// Written model setters: `(keyword, value)`; `tnom` in Celsius.
    pub model: Vec<(&'static str, Real)>,
}

/// A capacitor's or inductor's `.sens` description.
#[derive(Debug)]
pub(crate) struct ReactiveSensitivity {
    kind: ReactiveKind,
    name: String,
    model: Option<String>,
    terminals: [NodeId; 2],
    circuit_kelvin: Real,
    nominal_kelvin: Real,
    inputs: ReactiveInputs,
}

impl ReactiveSensitivity {
    pub(crate) fn new(
        kind: ReactiveKind,
        name: &str,
        model: Option<&str>,
        terminals: [NodeId; 2],
        inputs: ReactiveInputs,
        context: &crate::devices::ModelContext,
    ) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            model: model.map(str::to_owned),
            terminals,
            circuit_kelvin: context.temperature + CELSIUS_TO_KELVIN,
            nominal_kelvin: context.nominal_temperature + CELSIUS_TO_KELVIN,
            inputs,
        }
    }

    fn fields(&self) -> (&'static [&'static str], &'static [&'static str]) {
        match self.kind {
            ReactiveKind::Capacitor => (
                &[
                    "tnom", "cap", "cj", "cjsw", "defw", "defl", "narrow", "short", "del", "tc1",
                    "tc2", "di", "thick", "bv_max",
                ],
                &[
                    "capacitance",
                    "ic",
                    "temp",
                    "dtemp",
                    "w",
                    "l",
                    "m",
                    "tc1",
                    "tc2",
                    "bv_max",
                    "scale",
                    "capac",
                ],
            ),
            ReactiveKind::Inductor => (
                &[
                    "tnom", "ind", "tc1", "tc2", "csect", "dia", "length", "nt", "mu", "spec",
                ],
                &[
                    "inductance",
                    "ic",
                    "temp",
                    "dtemp",
                    "m",
                    "tc1",
                    "tc2",
                    "scale",
                    "nt",
                    "induct",
                ],
            ),
        }
    }
}

/// `Lundin()` of `indsetup.c`: Lundin's geometry correction of a solenoid.
fn lundin(length: Real, csect: Real) -> Real {
    if csect < 1e-12 || length < 1e-6 {
        return 1.;
    }
    let x = (csect / std::f64::consts::PI).sqrt() * 2. / length;
    let xx = x * x;
    let xxxx = xx * xx;
    if x < 1. {
        let num = 1. + 0.383901 * xx + 0.017108 * xxxx;
        let den = 1. + 0.258952 * xx;
        num / den - 4. * x / (3. * std::f64::consts::PI)
    } else {
        let num = ((4. * x).ln() - 0.5) * (1. + 0.383901 / xx + 0.017108 / xxxx);
        let den = 1. + 0.258952 / xx;
        let kk = 0.093842 / xx + 0.002029 / xxxx - 0.000801 / (xx * xxxx);
        2. * (num / den + kk) / (std::f64::consts::PI * x)
    }
}

impl DeviceSensitivity for ReactiveSensitivity {
    fn family(&self) -> SensitivityFamily {
        match self.kind {
            ReactiveKind::Capacitor => SensitivityFamily::Capacitor,
            ReactiveKind::Inductor => SensitivityFamily::Inductor,
        }
    }

    fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    fn model_parameters(&self) -> &'static [P] {
        match self.kind {
            ReactiveKind::Capacitor => CAP_MODEL,
            ReactiveKind::Inductor => IND_MODEL,
        }
    }

    fn instance_parameters(&self) -> &'static [P] {
        match self.kind {
            ReactiveKind::Capacitor => CAP_INSTANCE,
            ReactiveKind::Inductor => IND_INSTANCE,
        }
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let (model_fields, instance_fields) = self.fields();
        let build = |fields: &[&'static str], written: &[(&'static str, Real)]| {
            let mut record = SensitivityRecord::new();
            for keyword in fields {
                let value = written
                    .iter()
                    .rev()
                    .find(|(name, _)| name == keyword)
                    .map(|(_, value)| *value);
                let value = match (*keyword, value) {
                    ("temp" | "tnom", Some(celsius)) => Some(celsius + CELSIUS_TO_KELVIN),
                    (_, value) => value,
                };
                record = record.with(keyword, value.unwrap_or(0.), value.is_some());
            }
            record
        };
        let mut model = build(model_fields, &self.inputs.model);
        let mut instance = build(instance_fields, &self.inputs.instance);
        // The instance value is kept as C's `CAPcapacinst`/`INDinductinst`.
        self.setup(&mut model, &mut instance)?;
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }

    /// `CAPask`: the capacitance is the temperature-adjusted `CAPcapac`
    /// times `m`; `INDask`: the inductance is `INDinduct`; `temp` in Celsius.
    fn ask(
        &self,
        scope: ParameterScope,
        keyword: &str,
        records: RecordPair<'_>,
    ) -> SpiceResult<Real> {
        let i = records.instance;
        match (scope, keyword) {
            (ParameterScope::Model, _) => records.model.value(keyword),
            (ParameterScope::Instance, "capacitance") => Ok(i.value("capac")? * i.value("m")?),
            (ParameterScope::Instance, "inductance") => i.value("induct"),
            (ParameterScope::Instance, "temp") => Ok(i.value("temp")? - CELSIUS_TO_KELVIN),
            (ParameterScope::Instance, _) => i.value(keyword),
        }
    }

    /// The setters: `temp` from Celsius; the instance value sets
    /// `CAPcapacinst`/`INDinductinst` (and defaults `m`).
    fn set(
        &self,
        scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        match (scope, keyword) {
            (ParameterScope::Model, _) => model.set(keyword, value),
            (ParameterScope::Instance, "temp") => instance.set("temp", value + CELSIUS_TO_KELVIN),
            (ParameterScope::Instance, "capacitance" | "inductance") => {
                instance.set(keyword, value)?;
                if !instance.given("m") {
                    instance.store("m", 1.)?;
                }
                Ok(())
            }
            (ParameterScope::Instance, _) => instance.set(keyword, value),
        }
    }

    /// `CAPsetup`/`INDsetup`: model defaults and the quantities only setup
    /// derives (a capacitor's `cj` from `di`/`thick`, `narrow`/`short` from
    /// `del`; an inductor's specific inductance and default `ind`).
    fn setup(
        &self,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        let default = |record: &mut SensitivityRecord, keyword: &str, value: Real| {
            if record.given(keyword) {
                Ok(())
            } else {
                record.store(keyword, value)
            }
        };
        match self.kind {
            ReactiveKind::Capacitor => {
                for (keyword, value) in [
                    ("cap", 0.),
                    ("cjsw", 0.),
                    ("defw", 10e-6),
                    ("defl", 0.),
                    ("narrow", 0.),
                    ("short", 0.),
                    ("del", 0.),
                    ("tc1", 0.),
                    ("tc2", 0.),
                    ("tnom", self.nominal_kelvin),
                    ("di", 0.),
                    ("thick", 0.),
                    ("bv_max", 1e99),
                ] {
                    default(model, keyword, value)?;
                }
                if !model.given("cj") {
                    let thick = model.value("thick")?;
                    let cj = if model.given("thick") && thick > 0. {
                        if model.given("di") {
                            model.value("di")? * EPS_ZERO / thick
                        } else {
                            EPS_SIO2 / thick
                        }
                    } else {
                        0.
                    };
                    model.store("cj", cj)?;
                }
                if model.given("del") {
                    let del = model.value("del")?;
                    default(model, "narrow", 2. * del)?;
                    default(model, "short", 2. * del)?;
                }
                default(instance, "l", 0.)?;
                let bv_max = model.value("bv_max")?;
                default(instance, "bv_max", bv_max)
            }
            ReactiveKind::Inductor => {
                for (keyword, value) in [
                    ("ind", 0.),
                    ("tnom", self.nominal_kelvin),
                    ("tc1", 0.),
                    ("tc2", 0.),
                    ("csect", 0.),
                    ("dia", 0.),
                    ("length", 0.),
                    ("nt", 0.),
                    ("mu", 1.),
                ] {
                    default(model, keyword, value)?;
                }
                if model.given("dia") {
                    let dia = model.value("dia")?;
                    model.store("csect", std::f64::consts::PI * dia * dia / 4.)?;
                }
                let length = model.value("length")?;
                let mut spec = if model.given("length") && length > 0. {
                    model.value("mu")? * MU_ZERO * model.value("csect")? / length
                } else {
                    0.
                };
                if model.given("length") && (model.given("dia") || model.given("csect")) {
                    spec *= lundin(length, model.value("csect")?);
                }
                model.store("spec", spec)?;
                if !model.given("ind") {
                    let nt = model.value("nt")?;
                    model.store("ind", nt * nt * spec)?;
                }
                Ok(())
            }
        }
    }

    /// `CAPtemp`/`INDtemp`.
    fn temperature(
        &self,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if instance.given("temp") {
            instance.store("dtemp", 0.)?;
        } else {
            instance.store("temp", self.circuit_kelvin)?;
            if !instance.given("dtemp") {
                instance.store("dtemp", 0.)?;
            }
        }
        if !instance.given("scale") {
            instance.store("scale", 1.)?;
        }
        if !instance.given("m") {
            instance.store("m", 1.)?;
        }
        let base = match self.kind {
            ReactiveKind::Capacitor => {
                if !instance.given("w") {
                    instance.store("w", model.value("defw")?)?;
                }
                if instance.given("capacitance") {
                    instance.value("capacitance")?
                } else if model.given("cap") {
                    model.value("cap")?
                } else {
                    let (w, l) = (instance.value("w")?, instance.value("l")?);
                    let (narrow, short) = (model.value("narrow")?, model.value("short")?);
                    model.value("cj")? * (w - narrow) * (l - short)
                        + model.value("cjsw")? * 2. * ((l - short) + (w - narrow))
                }
            }
            ReactiveKind::Inductor => {
                if !instance.given("nt") {
                    instance.store("nt", 0.)?;
                }
                if instance.given("inductance") {
                    instance.value("inductance")?
                } else if instance.given("nt") {
                    let nt = instance.value("nt")?;
                    model.value("spec")? * nt * nt
                } else {
                    model.value("ind")?
                }
            }
        };
        let difference =
            (instance.value("temp")? + instance.value("dtemp")?) - model.value("tnom")?;
        let pick = |keyword: &str| -> SpiceResult<Real> {
            if instance.given(keyword) {
                instance.value(keyword)
            } else {
                model.value(keyword)
            }
        };
        let (tc1, tc2) = (pick("tc1")?, pick("tc2")?);
        let factor = 1.0 + tc1 * difference + tc2 * difference * difference;
        let value = base * factor * instance.value("scale")?;
        match self.kind {
            ReactiveKind::Capacitor => instance.store("capac", value),
            ReactiveKind::Inductor => instance.store("induct", value),
        }
    }

    /// DC: the device's own load (independent of every parameter). AC:
    /// `CAPacLoad` stamps `omega CAPcapac m`, `INDacLoad` `omega INDinduct /
    /// m` on the branch.
    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        if mode == SensitivityMode::Dc {
            return Ok(SensitivityLoad::Original);
        }
        let i = current.instance;
        let m = i.value("m")?;
        let value = match self.kind {
            ReactiveKind::Capacitor => i.value("capac")? * m,
            ReactiveKind::Inductor => i.value("induct")? / m,
        };
        if !value.is_finite() {
            return Err(SpiceError::Numerical {
                context: format!("sensitivity of {}", self.name),
                message: format!("nonfinite perturbed value {value}"),
            });
        }
        Ok(SensitivityLoad::Replacement(Box::new(ReactiveStandIn {
            name: self.name.clone(),
            kind: self.kind,
            terminals: self.terminals,
            value,
        })))
    }
}

/// The AC stand-in of a perturbed capacitor or inductor.
#[derive(Debug)]
struct ReactiveStandIn {
    name: String,
    kind: ReactiveKind,
    terminals: [NodeId; 2],
    value: Real,
}

impl Device for ReactiveStandIn {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        match self.kind {
            ReactiveKind::Capacitor => 'c',
            ReactiveKind::Inductor => 'l',
        }
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn branch_currents(&self) -> usize {
        usize::from(self.kind == ReactiveKind::Inductor)
    }

    fn stamp(&self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::circuit(format!(
            "{}: the sensitivity stand-in has no DC load",
            self.name
        )))
    }

    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        match self.kind {
            ReactiveKind::Capacitor => context.nodal(self.terminals, self.value, true),
            ReactiveKind::Inductor => {
                let branch = context.branch(self.terminals)?;
                context.system.e.add(branch, branch, -self.value)?;
                for term in context.mutual {
                    context.system.e.add(branch, term.row, -term.inductance)?;
                }
                Ok(())
            }
        }
    }
}
