//! Behavioural sources, switches and mutual inductance under `.sens`.
//!
//! * `asrc.c`: `temp`, `dtemp`, `tc1`, `tc2`, `m` (`asrcpar.c`,
//!   `asrcset.c`'s defaults, `asrctemp.c`, the `factor` of `asrcload.c`);
//! * `sw.c`/`csw.c`: the model's threshold, hysteresis and on/off
//!   resistances (`swmparam.c`/`cswmpar.c` store the conductances,
//!   `swsetup.c`/`cswsetup.c` default them; the switches have no
//!   temperature routine);
//! * `ind.c` (`MUTpTable`): the coupling `k` is `IF_AC`, so a K device has
//!   nothing to perturb in DC. AC `.sens` with K is refused by the analysis.

use super::{
    CELSIUS_TO_KELVIN, DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily,
    SensitivityLoad, SensitivityMode, SensitivityParameter as P, SensitivityRecord,
};
use crate::devices::{Behavioural, BehaviouralScale, Switch, SwitchKind};
use crate::primitives::{Real, SpiceResult};

/// `ASRCpTable` entries `set_param()` accepts.
const ASRC: &[P] = &[
    P::dc("temp"),
    P::dc("dtemp"),
    P::dc("tc1"),
    P::dc("tc2"),
    P::dc("m"),
];

/// `SWmPTable` entries `set_param()` accepts.
const SW: &[P] = &[P::dc("vt"), P::dc("vh"), P::dc("ron"), P::dc("roff")];
/// `CSWmPTable` entries `set_param()` accepts.
const CSW: &[P] = &[P::dc("it"), P::dc("ih"), P::dc("ron"), P::dc("roff")];

/// A B source's `.sens` description.
#[derive(Debug)]
pub(crate) struct BehaviouralSensitivity<'a> {
    source: &'a Behavioural,
    circuit_kelvin: Real,
}

impl<'a> BehaviouralSensitivity<'a> {
    pub(crate) fn new(source: &'a Behavioural, context: &crate::devices::ModelContext) -> Self {
        Self {
            source,
            circuit_kelvin: context.temperature + CELSIUS_TO_KELVIN,
        }
    }
}

impl DeviceSensitivity for BehaviouralSensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        SensitivityFamily::Asrc
    }

    fn model(&self) -> Option<&str> {
        None
    }

    fn model_parameters(&self) -> &'static [P] {
        &[]
    }

    fn instance_parameters(&self) -> &'static [P] {
        ASRC
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let scale = self.source.scale();
        let (temp, temp_given) = scale
            .temperature
            .map_or((0., false), |t| (t + CELSIUS_TO_KELVIN, true));
        let mut instance = SensitivityRecord::new()
            .with("temp", temp, temp_given)
            .with("dtemp", scale.dtemp, scale.dtemp != 0.)
            .with("tc1", scale.tc1, scale.tc1 != 0.)
            .with("tc2", scale.tc2, scale.tc2 != 0.)
            .with("m", scale.m, scale.m != 1.);
        let mut model = SensitivityRecord::new();
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }

    fn ask(
        &self,
        scope: ParameterScope,
        keyword: &str,
        records: RecordPair<'_>,
    ) -> SpiceResult<Real> {
        let value = records.instance.value(keyword)?;
        Ok(match (scope, keyword) {
            (ParameterScope::Instance, "temp") => value - CELSIUS_TO_KELVIN,
            _ => value,
        })
    }

    fn set(
        &self,
        _scope: ParameterScope,
        keyword: &str,
        value: Real,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if keyword == "temp" {
            instance.set("temp", value + CELSIUS_TO_KELVIN)
        } else {
            instance.set(keyword, value)
        }
    }

    /// `ASRCsetup`'s defaults.
    fn setup(
        &self,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        for (keyword, value) in [("tc1", 0.), ("tc2", 0.), ("m", 1.)] {
            if !instance.given(keyword) {
                instance.store(keyword, value)?;
            }
        }
        Ok(())
    }

    /// `ASRCtemp`.
    fn temperature(
        &self,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if instance.given("temp") {
            instance.store("dtemp", 0.)
        } else {
            instance.store("temp", self.circuit_kelvin)?;
            if instance.given("dtemp") {
                Ok(())
            } else {
                instance.store("dtemp", 0.)
            }
        }
    }

    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        _mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        let i = current.instance;
        let scale = BehaviouralScale {
            m: i.value("m")?,
            tc1: i.value("tc1")?,
            tc2: i.value("tc2")?,
            temperature: Some(i.value("temp")? - CELSIUS_TO_KELVIN),
            dtemp: i.value("dtemp")?,
            ..*self.source.scale()
        };
        Ok(SensitivityLoad::Replacement(Box::new(
            self.source.with_scale(scale),
        )))
    }
}

/// An S or W switch's `.sens` description.
#[derive(Debug)]
pub(crate) struct SwitchSensitivity<'a> {
    switch: &'a Switch,
    gmin: Real,
}

impl<'a> SwitchSensitivity<'a> {
    pub(crate) fn new(switch: &'a Switch, context: &crate::devices::ModelContext) -> Self {
        Self {
            switch,
            gmin: context.gmin,
        }
    }

    fn keys(&self) -> (&'static str, &'static str) {
        match self.switch.kind() {
            SwitchKind::Voltage => ("vt", "vh"),
            SwitchKind::Current => ("it", "ih"),
        }
    }
}

impl DeviceSensitivity for SwitchSensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        match self.switch.kind() {
            SwitchKind::Voltage => SensitivityFamily::VoltageSwitch,
            SwitchKind::Current => SensitivityFamily::CurrentSwitch,
        }
    }

    fn model(&self) -> Option<&str> {
        Some(self.switch.model_name())
    }

    fn model_parameters(&self) -> &'static [P] {
        match self.switch.kind() {
            SwitchKind::Voltage => SW,
            SwitchKind::Current => CSW,
        }
    }

    fn instance_parameters(&self) -> &'static [P] {
        &[]
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let (threshold, hysteresis) = self.keys();
        let values = self.switch.model_values();
        let mut model = SensitivityRecord::new()
            .with(
                threshold,
                values.threshold.unwrap_or(0.),
                values.threshold.is_some(),
            )
            .with(
                hysteresis,
                values.hysteresis.unwrap_or(0.),
                values.hysteresis.is_some(),
            )
            .with("ron", values.on.unwrap_or(0.), values.on.is_some())
            .with("roff", values.off.unwrap_or(0.), values.off.is_some())
            .with("gon", values.on.map_or(0., |r| 1. / r), false)
            .with("goff", values.off.map_or(0., |r| 1. / r), false);
        self.setup(&mut model, &mut SensitivityRecord::new())?;
        Ok((model, SensitivityRecord::new()))
    }

    /// `SWmParam`/`CSWmParam`: a resistance also stores its conductance.
    fn set(
        &self,
        _scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut SensitivityRecord,
        _instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        model.set(keyword, value)?;
        match keyword {
            "ron" => model.store("gon", 1. / value),
            "roff" => model.store("goff", 1. / value),
            _ => Ok(()),
        }
    }

    /// `SWsetup`/`CSWsetup`: threshold and hysteresis default to 0, the on
    /// conductance to 1 S and the off conductance to `gmin`.
    fn setup(
        &self,
        model: &mut SensitivityRecord,
        _instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        let (threshold, hysteresis) = self.keys();
        for keyword in [threshold, hysteresis] {
            if !model.given(keyword) {
                model.store(keyword, 0.)?;
            }
        }
        if !model.given("ron") {
            model.store("gon", 1.)?;
            model.store("ron", 1.)?;
        }
        if !model.given("roff") {
            model.store("goff", self.gmin)?;
            model.store("roff", 1. / self.gmin)?;
        }
        Ok(())
    }

    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        _mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        let (threshold, hysteresis) = self.keys();
        let m = current.model;
        Ok(SensitivityLoad::Replacement(Box::new(
            self.switch.with_model(
                m.value(threshold)?,
                m.value(hysteresis)?,
                m.value("gon")?,
                m.value("goff")?,
            )?,
        )))
    }
}

/// A device with nothing `.sens` can perturb in DC (K: its `k` is `IF_AC`).
#[derive(Debug)]
pub(crate) struct MutualSensitivity;

impl DeviceSensitivity for MutualSensitivity {
    fn family(&self) -> SensitivityFamily {
        SensitivityFamily::Mutual
    }

    fn model(&self) -> Option<&str> {
        None
    }

    fn model_parameters(&self) -> &'static [P] {
        &[]
    }

    /// `k` (`IOPAP`): perturbed only by an AC analysis, which refuses K.
    fn instance_parameters(&self) -> &'static [P] {
        const MUT: &[P] = &[P::principal("k", true)];
        MUT
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        Ok((SensitivityRecord::new(), SensitivityRecord::new()))
    }

    fn load(
        &self,
        _setup: RecordPair<'_>,
        _current: RecordPair<'_>,
        _mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        Ok(SensitivityLoad::Original)
    }
}
