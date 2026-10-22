//! `b3ld.c` evaluated at one bias: C's load run as a `MODEINITSMSIG`
//! evaluation (no limiting, no stamps) on an [`InstanceSetup`]'s structs,
//! reporting the instance members it leaves, as `b3ask.c` reads them.
use super::defaults::ModelDefaults;
use super::fields::Instance;
use super::host::{LoadCkt, mode, state};
use super::load;
use super::setup::InstanceSetup;
use super::temperature::TemperatureModel;
use crate::primitives::{Real, SpiceResult};

/// The internal terminal voltages C's load reads from `CKTstate0` in
/// `MODEINITSMSIG`, already multiplied by the model type (`vbs`, `vgs`, `vds`
/// of an NMOS-equivalent device).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bias {
    /// Bulk-source voltage.
    pub vbs: Real,
    /// Gate-source voltage.
    pub vgs: Real,
    /// Drain-source voltage.
    pub vds: Real,
}

/// What C's load leaves on the instance at a bias.
#[derive(Debug, Clone, PartialEq)]
pub struct DcEvaluation {
    /// Drain current (C `here->BSIM3cd`).
    pub ids: Real,
    /// Transconductance.
    pub gm: Real,
    /// Output conductance.
    pub gds: Real,
    /// Body transconductance.
    pub gmbs: Real,
    /// Substrate current (C `here->BSIM3csub`).
    pub isub: Real,
    /// Substrate current derivative in Vgs.
    pub gbgs: Real,
    /// Substrate current derivative in Vds.
    pub gbds: Real,
    /// Substrate current derivative in Vbs.
    pub gbbs: Real,
    /// Bulk-source junction current.
    pub cbs: Real,
    /// Bulk-drain junction current.
    pub cbd: Real,
    /// Bulk-source junction conductance.
    pub gbs: Real,
    /// Bulk-drain junction conductance.
    pub gbd: Real,
    /// Source-body voltage used, after mode selection (C's local `Vbs`).
    pub vbs: Real,
    /// Drain-body voltage computed as `vbs - vds` of the selected values.
    pub vbd: Real,
    /// Gate-source voltage used, after mode selection (C's local `Vgs`).
    pub vgs: Real,
    /// Drain-source voltage used, after mode selection (C's local `Vds`).
    pub vds: Real,
    /// Operating mode: 1 normal, -1 drain/source swapped.
    pub mode: i8,
    /// Threshold voltage (C `here->BSIM3von`).
    pub von: Real,
    /// Effective gate overdrive.
    pub vgsteff: Real,
    /// Effective drain voltage.
    pub vdseff: Real,
    /// Saturation voltage.
    pub vdsat: Real,
    /// Bulk-charge factor.
    pub abulk: Real,
    /// Effective mobility.
    pub ueff: Real,
    /// Source/drain series resistance.
    pub rds: Real,
    /// DIBL threshold shift.
    pub thetavth: Real,
}

/// The inputs of one evaluation. `model` and `temperature` are kept for the
/// callers' construction; the structs `b3temp.c` and `b3check.c` left are
/// `setup`'s.
#[derive(Debug, Clone, Copy)]
pub struct DcModel<'a> {
    /// The defaulted model.
    pub model: &'a ModelDefaults,
    /// The model's temperature pass.
    pub temperature: &'a TemperatureModel,
    /// The instance setup the load runs on.
    pub setup: &'a InstanceSetup,
}

/// A [`LoadCkt`] that reads the bias from `CKTstate0` (`MODEINITSMSIG`) and
/// discards stamps.
struct BiasProbe {
    gmin: Real,
    temperature: Real,
    s0: [Real; state::COUNT],
}

impl LoadCkt for BiasProbe {
    fn mode(&self) -> u32 {
        mode::MODEDCOP | mode::MODEINITSMSIG
    }
    fn limiting(&self) -> bool {
        false
    }
    fn gmin(&self) -> f64 {
        self.gmin
    }
    fn temp(&self) -> f64 {
        self.temperature
    }
    fn ag(&self, _i: usize) -> f64 {
        0.
    }
    fn delta(&self) -> f64 {
        1.
    }
    fn delta_old(&self, _i: usize) -> f64 {
        1.
    }
    fn rhs_old(&self, _node: i32) -> f64 {
        0.
    }
    fn state0(&self, slot: usize) -> f64 {
        self.s0.get(slot).copied().unwrap_or(0.)
    }
    fn set_state0(&mut self, slot: usize, value: f64) {
        if let Some(entry) = self.s0.get_mut(slot) {
            *entry = value;
        }
    }
    fn state1(&self, slot: usize) -> f64 {
        self.state0(slot)
    }
    fn set_state1(&mut self, _slot: usize, _value: f64) {}
    fn state2(&self, slot: usize) -> f64 {
        self.state0(slot)
    }
    fn integrate(&mut self, _cap: f64, slot: usize) -> SpiceResult<(f64, f64)> {
        // `MODEINITSMSIG` leaves the load before `NIintegrate`.
        self.set_state0(slot + 1, 0.);
        Ok((0., 0.))
    }
    fn noncon(&mut self) {}
    fn matrix(&mut self, _row: i32, _col: i32, _value: f64) {}
    fn rhs(&mut self, _node: i32, _value: f64) {}
}

/// C's load at `bias` on `setup`'s structs: the instance it leaves and
/// `CKTstate0`.
pub(super) fn evaluate_point(
    setup: &InstanceSetup,
    temperature: &TemperatureModel,
    bias: Bias,
    gmin: Real,
) -> SpiceResult<(Instance, [Real; state::COUNT])> {
    let mut s0 = [0.; state::COUNT];
    s0[state::VBS] = bias.vbs;
    s0[state::VGS] = bias.vgs;
    s0[state::VDS] = bias.vds;
    s0[state::VBD] = bias.vbs - bias.vds;
    let mut probe = BiasProbe {
        gmin,
        temperature: temperature.kelvin,
        s0,
    };
    let mut here = setup.instance.clone();
    load::load(&setup.model, &mut here, &setup.size, &mut probe)?;
    Ok((here, probe.s0))
}

impl DcModel<'_> {
    /// Evaluate the DC currents and conductances at `bias`.
    ///
    /// `gmin` is C's `CKTgmin`, added to both junction conductances.
    ///
    /// # Errors
    /// The load's own errors (`acmMod > 0`).
    #[allow(non_snake_case)] // C's local names.
    pub fn evaluate(&self, bias: Bias, gmin: Real) -> SpiceResult<DcEvaluation> {
        let (h, s0) = evaluate_point(self.setup, self.temperature, bias, gmin)?;
        // `b3ld.c`'s mode selection: inverse mode evaluates Vds = -vds,
        // Vgs = vgd, Vbs = vbd.
        let (vbs, vgs, vds) = (s0[state::VBS], s0[state::VGS], s0[state::VDS]);
        let (Vbs, Vgs, Vds) = if h.mode >= 0 {
            (vbs, vgs, vds)
        } else {
            (vbs - vds, vgs - vds, -vds)
        };
        Ok(DcEvaluation {
            ids: h.cd,
            gm: h.gm,
            gds: h.gds,
            gmbs: h.gmbs,
            isub: h.csub,
            gbgs: h.gbgs,
            gbds: h.gbds,
            gbbs: h.gbbs,
            cbs: h.cbs,
            cbd: h.cbd,
            gbs: h.gbs,
            gbd: h.gbd,
            vbs: Vbs,
            vbd: Vbs - Vds,
            vgs: Vgs,
            vds: Vds,
            mode: if h.mode >= 0 { 1 } else { -1 },
            von: h.von,
            vgsteff: h.Vgsteff,
            vdseff: h.Vdseff,
            vdsat: h.vdsat,
            abulk: h.Abulk,
            ueff: h.ueff,
            rds: h.rds,
            thetavth: h.thetavth,
        })
    }
}

/// `@inst[param]` values C's `b3ask.c` reads from the instance after a load.
impl DcEvaluation {
    /// The value of a DC operating-point ask keyword, before multiplicity.
    /// These are C's IFparm keywords; `cbs`/`cbd` are capacitances in C and are
    /// therefore absent, while `ibs`/`ibd` are the junction currents. Charge,
    /// NQS and noise asks are not part of this projection.
    #[must_use]
    pub fn ask(&self, keyword: &str) -> Option<Real> {
        match keyword.to_ascii_lowercase().as_str() {
            "id" => Some(self.ids),
            "gm" => Some(self.gm),
            "gds" => Some(self.gds),
            "gmbs" => Some(self.gmbs),
            "gbd" => Some(self.gbd),
            "gbs" => Some(self.gbs),
            "vth" => Some(self.von),
            "vbs" => Some(self.vbs),
            "vgs" => Some(self.vgs),
            "vds" => Some(self.vds),
            "vdsat" => Some(self.vdsat),
            "ibs" => Some(self.cbs),
            "ibd" => Some(self.cbd),
            _ => None,
        }
    }
}
