//! Independent sources under `.sens`: `vsrc.c`/`isrc.c` tables,
//! `vsrcpar.c`/`isrcpar.c` setters, `vsrctemp.c`/`isrctemp.c` and the DC/AC
//! loads (`vsrcload.c`, `vsrcacld.c`, `isrcload.c`, `isrcacld.c`).

use super::{
    DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityParameter as P, SensitivityRecord,
};
use crate::devices::IndependentSource;
use crate::primitives::{Complex, Real, SpiceResult};

/// `VSRCpTable` entries `set_param()` accepts (`portnum` is an integer).
const VOLTAGE: &[P] = &[
    P::principal("dc", false),
    P::principal("acmag", true),
    P::ac("acphase"),
    P::dc("z0"),
    P::dc("pwr"),
    P::dc("freq"),
    P::dc("phase"),
];

/// `ISRCpTable` entries `set_param()` accepts.
const CURRENT: &[P] = &[
    P::principal("dc", false),
    P::dc("m"),
    P::principal("acmag", true),
    P::ac("acphase"),
];

/// What the source card gave.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SourceInputs {
    /// An explicit DC value (`VSRCdcGiven`).
    pub dc_given: bool,
    /// A transient function (`VSRCfuncTGiven`).
    pub function_given: bool,
    /// Any AC setter (`VSRCacGiven`), with the magnitude and phase in degrees.
    pub ac: Option<(Real, Real)>,
}

/// An independent source's `.sens` description.
#[derive(Debug)]
pub(crate) struct SourceSensitivity<'a> {
    source: &'a IndependentSource,
    voltage: bool,
    /// The card's DC value (the function's time-zero value when not given).
    level: Real,
    inputs: SourceInputs,
}

impl<'a> SourceSensitivity<'a> {
    pub(crate) fn new(
        source: &'a IndependentSource,
        voltage: bool,
        level: Real,
        inputs: SourceInputs,
    ) -> Self {
        Self {
            source,
            voltage,
            level,
            inputs,
        }
    }
}

impl DeviceSensitivity for SourceSensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        if self.voltage {
            SensitivityFamily::VoltageSource
        } else {
            SensitivityFamily::CurrentSource
        }
    }

    fn model(&self) -> Option<&str> {
        None
    }

    fn model_parameters(&self) -> &'static [P] {
        &[]
    }

    fn instance_parameters(&self) -> &'static [P] {
        if self.voltage { VOLTAGE } else { CURRENT }
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let i = &self.inputs;
        let (mag, phase) = i.ac.unwrap_or((0., 0.));
        let given = i.ac.is_some();
        let mut instance = SensitivityRecord::new()
            .with("dc", if i.dc_given { self.level } else { 0. }, i.dc_given)
            .with("acmag", mag, given)
            .with("acphase", phase, given)
            .with("ac", 0., given)
            .with("m", 1., false)
            .with("z0", 0., false)
            .with("pwr", 0., false)
            .with("freq", 0., false)
            .with("phase", 0., false)
            .with("acreal", 0., false)
            .with("acimag", 0., false);
        let mut model = SensitivityRecord::new();
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }

    /// `VSRCparam`/`ISRCparam`: an AC magnitude or phase also sets
    /// `VSRCacGiven`.
    fn set(
        &self,
        _scope: ParameterScope,
        keyword: &str,
        value: Real,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        instance.set(keyword, value)?;
        if matches!(keyword, "acmag" | "acphase") {
            instance.set_given("ac", true)?;
        }
        Ok(())
    }

    /// `VSRCtemp`/`ISRCtemp`: the AC defaults, the phasor and the default `m`.
    fn temperature(
        &self,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if instance.given("ac") && !instance.given("acmag") {
            instance.store("acmag", 1.)?;
        }
        if instance.given("ac") && !instance.given("acphase") {
            instance.store("acphase", 0.)?;
        }
        if !instance.given("m") {
            instance.store("m", 1.)?;
        }
        let radians = instance.value("acphase")? * std::f64::consts::PI / 180.0;
        let magnitude = instance.value("acmag")?;
        instance.store("acreal", magnitude * radians.cos())?;
        instance.store("acimag", magnitude * radians.sin())
    }

    /// The DC load drives the DC value when given, otherwise the transient
    /// function's time-zero value (`dcValue`, zero, without a function); the
    /// AC load drives the phasor; a current source scales both by `m`. RF
    /// port setters (`z0`, `pwr`, `freq`, `phase`) do nothing on a source
    /// that is not a port.
    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        _mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        let i = current.instance;
        let dc = if i.given("dc") || !self.inputs.function_given {
            i.value("dc")?
        } else {
            self.level
        };
        let ac = Complex::new(i.value("acreal")?, i.value("acimag")?);
        let m = if self.voltage { 1. } else { i.value("m")? };
        Ok(SensitivityLoad::Replacement(Box::new(
            self.source
                .sensitivity_copy(m * dc, Complex::new(m * ac.re, m * ac.im))?,
        )))
    }
}
