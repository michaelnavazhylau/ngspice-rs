//! Linear controlled sources under `.sens`: the `vcvs.c`, `vccs.c`,
//! `cccs.c` and `ccvs.c` tables and setters. The loads stamp the stored
//! coefficient (`VCVScoeff`, ...) in DC and AC alike.
//!
//! `VCCSparam`/`CCCSparam` multiply a gain by `m` **if `m` was given
//! before it**; `sgen` asks the stored coefficient and sets it back, so once
//! `m` is marked given every later gain perturbation and restoration
//! multiplies by it again (a never-given `m` reads 0, so restoring the gain
//! after `m` was perturbed zeroes the coefficient). The port replays this.

use super::{
    DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityParameter as P, SensitivityRecord,
};
use crate::devices::{ControlledKind, ControlledSource};
use crate::primitives::{Real, SpiceResult};

/// `VCVSpTable`/`CCVSpTable` entries `set_param()` accepts.
const GAIN: &[P] = &[P::dc("gain")];
/// `VCCSpTable`/`CCCSpTable` entries `set_param()` accepts.
const GAIN_M: &[P] = &[P::dc("gain"), P::dc("m")];

/// A controlled source's `.sens` description.
#[derive(Debug)]
pub(crate) struct ControlledSensitivity<'a> {
    source: &'a ControlledSource,
    coefficient: Real,
    multiplier: Option<Real>,
}

impl<'a> ControlledSensitivity<'a> {
    pub(crate) fn new(
        source: &'a ControlledSource,
        coefficient: Real,
        multiplier: Option<Real>,
    ) -> Self {
        Self {
            source,
            coefficient,
            multiplier,
        }
    }

    fn scaled(&self) -> bool {
        matches!(
            self.source.kind(),
            ControlledKind::Vccs | ControlledKind::Cccs
        )
    }
}

impl DeviceSensitivity for ControlledSensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        match self.source.kind() {
            ControlledKind::Vcvs => SensitivityFamily::Vcvs,
            ControlledKind::Vccs => SensitivityFamily::Vccs,
            ControlledKind::Cccs => SensitivityFamily::Cccs,
            ControlledKind::Ccvs => SensitivityFamily::Ccvs,
        }
    }

    fn model(&self) -> Option<&str> {
        None
    }

    fn model_parameters(&self) -> &'static [P] {
        &[]
    }

    fn instance_parameters(&self) -> &'static [P] {
        if self.scaled() { GAIN_M } else { GAIN }
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let (m, m_given) = self.multiplier.map_or((0., false), |m| (m, true));
        Ok((
            SensitivityRecord::new(),
            SensitivityRecord::new()
                .with("gain", self.coefficient, true)
                .with("m", m, m_given),
        ))
    }

    /// `VCCSparam`/`CCCSparam`: the gain is multiplied by an `m` given
    /// before it.
    fn set(
        &self,
        _scope: ParameterScope,
        keyword: &str,
        value: Real,
        _model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if keyword == "gain" && self.scaled() && instance.given("m") {
            instance.set("gain", value * instance.value("m")?)
        } else {
            instance.set(keyword, value)
        }
    }

    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        _mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        Ok(SensitivityLoad::Replacement(Box::new(
            self.source
                .with_coefficient(current.instance.value("gain")?)?,
        )))
    }
}
