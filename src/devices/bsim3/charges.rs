//! `b3ld.c`'s charge block at one bias: the capacitance derivatives it
//! leaves on the instance and the `CKTstate0` charges, from the same
//! `MODEINITSMSIG` evaluation as [`super::dc`].
use super::dc::{Bias, DcEvaluation, evaluate_point};
use super::defaults::ModelDefaults;
use super::host::state;
use super::setup::InstanceSetup;
use super::temperature::TemperatureModel;
use crate::primitives::{Real, SpiceError, SpiceResult};

/// The capacitance derivatives and charges C's load leaves at a bias.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeEvaluation {
    /// dQg/dVg.
    pub cggb: Real,
    /// dQg/dVd.
    pub cgdb: Real,
    /// dQg/dVs.
    pub cgsb: Real,
    /// dQd/dVg.
    pub cdgb: Real,
    /// dQd/dVd.
    pub cddb: Real,
    /// dQd/dVs.
    pub cdsb: Real,
    /// dQb/dVg.
    pub cbgb: Real,
    /// dQb/dVd.
    pub cbdb: Real,
    /// dQb/dVs.
    pub cbsb: Real,
    /// Gate-drain overlap capacitance C's load computed (`here->BSIM3cgdo`).
    pub cgdo: Real,
    /// Gate-source overlap capacitance (`here->BSIM3cgso`).
    pub cgso: Real,
    /// Source junction capacitance.
    pub capbs: Real,
    /// Drain junction capacitance.
    pub capbd: Real,
    /// `CKTstate0` source junction charge.
    pub qbs: Real,
    /// `CKTstate0` drain junction charge.
    pub qbd: Real,
    /// Inversion charge (`here->BSIM3qinv`).
    pub qinv: Real,
    qg: Real,
    qd: Real,
    qb: Real,
}

impl ChargeEvaluation {
    /// The `CKTstate0` charges `b3ask.c` reports, after C's overlap-charge
    /// adjustment and junction-charge split. `mode` and `vgb` are those of
    /// the evaluation (C applied them in the load).
    #[must_use]
    pub fn with_overlap(&self, mode: i8, vgb: Real) -> OverlapCharges {
        let _ = (mode, vgb);
        OverlapCharges {
            qg: self.qg,
            qd: self.qd,
            qb: self.qb,
        }
    }
}

/// The `CKTstate0` charge values `b3ask.c` reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlapCharges {
    /// Gate charge.
    pub qg: Real,
    /// Drain charge (without the drain junction charge).
    pub qd: Real,
    /// Bulk charge (with both junction charges).
    pub qb: Real,
}

/// The inputs of one charge evaluation, as for [`super::dc::DcModel`].
#[derive(Debug, Clone, Copy)]
pub struct ChargeModel<'a> {
    /// The defaulted model.
    pub model: &'a ModelDefaults,
    /// The model's temperature pass.
    pub temperature: &'a TemperatureModel,
    /// The instance setup the load runs on.
    pub setup: &'a InstanceSetup,
}

impl ChargeModel<'_> {
    /// Evaluate the charge model at `bias` (the bias `dc` was evaluated at).
    ///
    /// # Errors
    /// A `capMod` other than 3 or any NQS mode (`NotYetPorted`: translated
    /// but not yet verified against C, #190/#191), and the load's errors.
    pub fn evaluate(&self, dc: &DcEvaluation, bias: Bias) -> SpiceResult<ChargeEvaluation> {
        let model = &self.setup.model;
        if model.capMod != 3 {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "BSIM3 capMod={} charge model (only capMod=3 is verified; \
                     set capMod=3 or omit it)",
                    model.capMod
                ),
                "src/spicelib/devices/bsim3/b3ld.c (capMod 0/1/2, #190)",
            ));
        }
        if self.setup.nqs_mod != 0 || self.setup.acnqs_mod != 0 {
            return Err(SpiceError::not_yet_ported(
                "BSIM3 NQS charge model",
                "src/spicelib/devices/bsim3/b3ld.c (nqsMod/acnqsMod, #191)",
            ));
        }
        let _ = dc;
        let (h, s0) = evaluate_point(self.setup, self.temperature, bias, 0.)?;
        Ok(ChargeEvaluation {
            cggb: h.cggb,
            cgdb: h.cgdb,
            cgsb: h.cgsb,
            cdgb: h.cdgb,
            cddb: h.cddb,
            cdsb: h.cdsb,
            cbgb: h.cbgb,
            cbdb: h.cbdb,
            cbsb: h.cbsb,
            cgdo: h.cgdo,
            cgso: h.cgso,
            capbs: h.capbs,
            capbd: h.capbd,
            qbs: s0[state::QBS],
            qbd: s0[state::QBD],
            qinv: h.qinv,
            qg: s0[state::QG],
            qd: s0[state::QD],
            qb: s0[state::QB],
        })
    }
}
