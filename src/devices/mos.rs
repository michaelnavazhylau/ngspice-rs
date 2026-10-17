//! The level-independent MOSFET shell shared by the classic SPICE levels.
//!
//! ngspice implements every classic MOS level (`mos1/`, `mos2/`, `mos3/`,
//! `mos6/`, `mos9/`) as a copy of one device frame around a level-specific
//! drain-current block: `mos3load.c` is `mos1load.c` with the `moseq3`
//! current, the effective width `W - 2 WD + XW` and length `L - 2 LD + XL`,
//! and the cubic reverse junction law; `mos3acld.c`, `mos3pzld.c`,
//! `mos3conv.c`, `mos3trun.c` and `mos3set.c`'s matrix allocation equal their
//! level-1 counterparts. This module holds that frame once, as an
//! operation-by-operation port of `mos1load.c`:
//!
//! - voltage selection per `MODEINIT*` phase with `DEVpred`, `DEVfetlim`/
//!   `DEVlimvds`/`DEVpnjlim` limiting and `Check`;
//! - the bulk junction currents ([`ReverseLaw`]) and depletion charges with
//!   their `f2/f3/f4` continuation above `FC * PB` (`mos1temp.c`);
//! - Meyer's gate capacitances (`devsup.c` `DEVqmeyer`) and C's
//!   state-averaged gate charge with `NIintegrate`;
//! - the stamps in C's order, `MOS1convTest`, `MOS1acLoad`, the pole-zero
//!   load, `MOS1trunc`'s slots, `.dc @m[...]` instance sweeps and the
//!   `mos1noi.c` generators.
//!
//! A level plugs in through [`MosLevel`]: its model schema and validated
//! card, its `mosXtemp.c` port that fills [`Operating`], the effective
//! channel geometry and the drain current with its derivatives in C's
//! normalized device frame ([`MosLevel::drain_current`]).
//!
//! The state vector holds C's `MOS1numStates` slots plus the instance values
//! C keeps between loads.
//!
//! # Deliberate divergences
//!
//! - Bypass (`.options bypass`, off by default in ngspice) and `.sens`
//!   perturbation loads are not ported; `.options fixlimit` is not ported
//!   (C's default `DEVlimvds` in reverse mode always applies).
//! - C's next-timepoint limiting reads `MOS1von` of the last load ever made,
//!   which after a rejected timestep belongs to the rejected attempt; here it
//!   is the last accepted point's.
//! - The small-signal loads evaluate the bias unlimited: C's
//!   `MODEINITSMSIG` load limits against the converged load's `CKTstate0`,
//!   which the small-signal contexts do not supply (identical whenever the
//!   converged load limited nothing).
//! - `mos3load.c` stores `MODEINITSMSIG` gate charges from the
//!   state-averaged capacitances where `mos1load.c` uses `2 * CKTstate0`;
//!   no load or stamp reads those charges, and the shell keeps the level-1
//!   form for both.
//! - C warns and continues for a nonpositive effective length (level 1),
//!   a nonpositive temperature-adjusted PHI/PB, or RSH with zero drain
//!   squares (an infinite conductance). Here they are explicit errors.
//!
//! Instance `off` and initial conditions follow `mos1load.c`: `MODEINITJCT`
//! starts at the `IC` vector (`type * ICVDS/ICVGS/ICVBS`) whenever one
//! component is nonzero, also without `uic`, and at the default start
//! otherwise (except in the `uic` initial load, which keeps all-zero
//! conditions); unset components are taken from the external terminals
//! under `uic` (`mos1ic.c`) and are zero otherwise. An `off` instance starts
//! at zero and is held there through `MODEINITFIX`.

use std::cell::OnceCell;

use crate::devices::initial::InstanceInitial;
use crate::devices::limiting;
use crate::devices::noise::{
    DeviceNoise, FlickerLaw, NoiseContext, NoiseFamily, NoiseKind, NoiseSource,
};
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::devices::setup::CDeviceType;
use crate::devices::{
    Device, DeviceState, IterationPhase, LinearContext, ModelContext, ModelFamily, ResolvedModel,
    StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// C `CONSTboltz` (J/K).
pub(super) const BOLTZMANN: Real = 1.38064852e-23;
/// C `CHARGE` (C).
pub(super) const CHARGE: Real = 1.6021766208e-19;
/// C `CONSTKoverQ`.
pub(super) const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
pub(super) const CELSIUS_TO_KELVIN: Real = 273.15;
/// C `REFTEMP` (27 degrees Celsius).
pub(super) const REFTEMP: Real = 27. + CELSIUS_TO_KELVIN;
/// Vacuum permittivity as written in `mos1temp.c` (F/m).
pub(super) const EPSILON_0: Real = 8.854214871e-12;
/// C `MAX_EXP_ARG`: junction exponent clamp of `mos1load.c`.
pub(super) const MAX_EXP_ARG: Real = 709.;
/// `DEVqmeyer`'s lower bound on the saturation voltage.
const MEYER_MIN_VDSAT: Real = 0.025;

/// Slots of the state vector: C's `MOS1numStates` layout (reordered so that
/// every charge is directly followed by its derivative, as
/// [`junction_companion`] expects), followed by the instance quantities C
/// keeps in `MOS1instance` between loads. All values are device-frame
/// (multiplied by the type), as in C.
pub(super) mod slot {
    /// C `MOS1qbd` (`MOS1cqbd` follows).
    pub(in crate::devices) const QBD: usize = 0;
    /// C `MOS1qbs` (`MOS1cqbs` follows).
    pub(in crate::devices) const QBS: usize = 2;
    /// C `MOS1qgs`, `MOS1qgd`, `MOS1qgb` (each followed by its current).
    pub(in crate::devices) const QG: [usize; 3] = [4, 6, 8];
    /// C `MOS1vbs`, `MOS1vbd`, `MOS1vgs`, `MOS1vds`.
    pub(in crate::devices) const VBS: usize = 10;
    pub(in crate::devices) const VBD: usize = 11;
    pub(in crate::devices) const VGS: usize = 12;
    pub(in crate::devices) const VDS: usize = 13;
    /// C `MOS1capgs`, `MOS1capgd`, `MOS1capgb` (`DEVqmeyer` halves).
    pub(in crate::devices) const CAPG: [usize; 3] = [14, 15, 16];
    /// C instance values: `MOS1von`, `MOS1vdsat`, `MOS1mode`, `MOS1cd`,
    /// `MOS1cbs`, `MOS1cbd`, `MOS1gm`, `MOS1gds`, `MOS1gmbs`, `MOS1gbd`,
    /// `MOS1gbs`, `MOS1capbd`, `MOS1capbs`.
    pub(in crate::devices) const VON: usize = 17;
    pub(in crate::devices) const VDSAT: usize = 18;
    pub(in crate::devices) const MODE: usize = 19;
    pub(in crate::devices) const CD: usize = 20;
    pub(in crate::devices) const CBS: usize = 21;
    pub(in crate::devices) const CBD: usize = 22;
    pub(in crate::devices) const GM: usize = 23;
    pub(in crate::devices) const GDS: usize = 24;
    pub(in crate::devices) const GMBS: usize = 25;
    pub(in crate::devices) const GBD: usize = 26;
    pub(in crate::devices) const GBS: usize = 27;
    pub(in crate::devices) const CAPBD: usize = 28;
    pub(in crate::devices) const CAPBS: usize = 29;
    pub(in crate::devices) const COUNT: usize = 30;
}

pub(super) const fn p(name: &'static str, unit: U, domain: D, default: Option<Real>) -> P {
    P {
        name,
        unit,
        domain,
        default,
    }
}

/// Instance setters (`mos1par.c`; `mos3par.c` has the same set). Defaults
/// are C's `CKTdefaultMos*` values, whose `.options defl/defw/defad/defas/defm`
/// are rejected by the options layer.
const INSTANCE: &[P] = &[
    p("m", U::Dimensionless, D::Positive, Some(1.)),
    p("l", U::Metre, D::Positive, Some(1e-4)),
    p("w", U::Metre, D::Positive, Some(1e-4)),
    p("ad", U::SquareMetre, D::NonNegative, Some(0.)),
    p("as", U::SquareMetre, D::NonNegative, Some(0.)),
    p("pd", U::Metre, D::NonNegative, Some(0.)),
    p("ps", U::Metre, D::NonNegative, Some(0.)),
    p("nrd", U::Dimensionless, D::NonNegative, Some(1.)),
    p("nrs", U::Dimensionless, D::NonNegative, Some(1.)),
    p("temp", U::Celsius, D::Temperature, None),
    p("dtemp", U::Celsius, D::Finite, Some(0.)),
];

/// The `IC` vector components (`mos1par.c`): `ICVDS`, `ICVGS`, `ICVBS`.
const IC_COMPONENTS: [&str; 3] = ["icvds", "icvgs", "icvbs"];

/// A schema-defaulted value; `label` names the level in the diagnostic.
pub(super) fn required(values: &ScalarValues, name: &str, label: &str) -> SpiceResult<Real> {
    values
        .get(name)
        .map(|v| v.value)
        .ok_or_else(|| SpiceError::circuit(format!("missing {label} schema default {name}")))
}

/// The last-set value of a C parameter with two spellings (`IOPR` aliases
/// such as `vto`/`vt0`, `u0`/`uo`): setters apply in card order.
pub(super) fn aliased(values: &ScalarValues, names: [&str; 2]) -> Option<Real> {
    names
        .iter()
        .filter_map(|name| values.get(name))
        .max_by_key(|v| v.location.as_ref().map(|l| (l.line, l.column)))
        .map(|v| v.value)
}

/// Band gap of silicon at `kelvin` (`mos1temp.c`, `mos3temp.c`).
pub(super) fn band_gap(kelvin: Real) -> Real {
    1.16 - (7.02e-4 * kelvin * kelvin) / (kelvin + 1108.)
}

/// The `pbfact` potential correction of `mos1temp.c`/`mos3temp.c` at
/// `kelvin`.
pub(super) fn potential_factor(kelvin: Real) -> Real {
    let vt = kelvin * K_OVER_Q;
    let kt = BOLTZMANN * kelvin;
    let arg = -band_gap(kelvin) / (kt + kt) + 1.1150877 / (BOLTZMANN * (REFTEMP + REFTEMP));
    -2. * vt * (1.5 * (kelvin / REFTEMP).ln() + CHARGE * arg)
}

/// Model setters every classic level shares, read from the level's own
/// schema (which must contain these names, with defaults where C defaults
/// them). `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
pub(super) struct Common {
    /// C `MOSxtype`: `1` for NMOS, `-1` for PMOS.
    pub(super) pol: Real,
    pub(super) rd: Option<Real>,
    pub(super) rs: Option<Real>,
    pub(super) rsh: Option<Real>,
    pub(super) cbd: Option<Real>,
    pub(super) cbs: Option<Real>,
    pub(super) is: Real,
    pub(super) js: Real,
    pub(super) pb: Real,
    /// CGSO, CGDO, CGBO.
    pub(super) overlap: [Real; 3],
    pub(super) cj: Option<Real>,
    pub(super) mj: Real,
    pub(super) cjsw: Option<Real>,
    pub(super) mjsw: Real,
    pub(super) fc: Real,
    pub(super) ld: Real,
    pub(super) tnom: Option<Real>,
    /// Flicker coefficient/exponent, noise model selector (0..=3) and channel
    /// thermal-noise coefficient (`mos1noi.c`).
    pub(super) kf: Real,
    pub(super) af: Real,
    pub(super) nlev: u8,
    pub(super) gdsnoi: Real,
}

impl Common {
    /// The shared setters of a validated model card, with C's shared checks
    /// (TPG is an integer setter, the junction gradings stay below one).
    /// `label` names the level and `noise_file` its C noise routine in
    /// diagnostics.
    pub(super) fn from_values(
        m: &ScalarValues,
        family: ModelFamily,
        tpg: Option<Real>,
        label: &str,
        noise_file: &str,
    ) -> SpiceResult<Self> {
        let pol = if family == ModelFamily::Pmos { -1. } else { 1. };
        let get = |name: &str| m.get(name).map(|v| v.value);
        let required = |name: &str| required(m, name, label);
        let common = Self {
            pol,
            rd: get("rd"),
            rs: get("rs"),
            rsh: get("rsh"),
            cbd: get("cbd"),
            cbs: get("cbs"),
            is: required("is")?,
            js: required("js")?,
            pb: required("pb")?,
            overlap: [required("cgso")?, required("cgdo")?, required("cgbo")?],
            cj: get("cj"),
            mj: required("mj")?,
            cjsw: get("cjsw"),
            mjsw: required("mjsw")?,
            fc: required("fc")?,
            ld: required("ld")?,
            tnom: get("tnom"),
            // Levels whose C device has no noise routine (MOS6) or whose card
            // lacks the noise selectors (MOS9) have no KF/AF/NLEV/GDSNOI
            // setters; those levels either refuse `.noise` or noise's own
            // C defaults apply, so the neutral values below are unused or
            // are C's defaults for the levels that do have the setters.
            kf: get("kf").unwrap_or(0.),
            af: get("af").unwrap_or(1.),
            nlev: {
                // IF_INTEGER setter: floor(value + 0.5). C's switch has no
                // case outside 0..=3 (the flicker density is then the bare
                // gain), which the port rejects.
                let raw = get("nlev").unwrap_or(2.);
                let rounded = (raw + 0.5).floor();
                if !(0. ..=3.).contains(&rounded) {
                    return Err(SpiceError::circuit(format!(
                        "{label} NLEV must round to 0..=3 ({noise_file}), got {raw}"
                    )));
                }
                rounded as u8
            },
            gdsnoi: get("gdsnoi").unwrap_or(1.),
        };
        if tpg.is_some_and(|tpg| tpg.fract() != 0.) {
            return Err(SpiceError::circuit(format!(
                "{label} TPG must be an integer"
            )));
        }
        if common.mj >= 1. || common.mjsw >= 1. || common.fc >= 1. {
            return Err(SpiceError::circuit(format!(
                "{label} requires MJ, MJSW and FC below 1"
            )));
        }
        Ok(common)
    }
}

/// Validated instance geometry (drawn sizes; the level derives the
/// effective channel).
#[derive(Debug, Clone, Copy)]
pub(super) struct Geometry {
    pub(super) m: Real,
    pub(super) l: Real,
    pub(super) w: Real,
    pub(super) ad: Real,
    pub(super) as_: Real,
    pub(super) pd: Real,
    pub(super) ps: Real,
    pub(super) nrd: Real,
    pub(super) nrs: Real,
    pub(super) temp: Option<Real>,
    pub(super) dtemp: Real,
}

impl Geometry {
    /// C's instance temperature in kelvin: `TEMP`, or `CKTtemp + DTEMP`
    /// with `CKTtemp` in kelvin.
    pub(super) fn kelvin(&self, context: &ModelContext) -> Real {
        match self.temp {
            Some(t) => t + CELSIUS_TO_KELVIN,
            None => (context.temperature + CELSIUS_TO_KELVIN) + self.dtemp,
        }
    }
}

/// The bulk-junction current below `-3 Vt` in reverse bias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReverseLaw {
    /// `mos1load.c`: `gbs = gmin`, `cbs = gbs * vbs - Is`.
    Constant,
    /// `mos3load.c`: `-Is (1 + (3 Vt / (v e))^3) + gmin v` with its
    /// derivative, which joins the forward law with continuous value and
    /// slope.
    Cubic,
}

/// One side's bulk junction at the device temperature (`mos1temp.c`), in
/// C's instance quantities.
#[derive(Debug, Clone, Copy)]
pub(super) struct Junction {
    /// C `DrainSatCur`/`SourceSatCur` of `mos1load.c` (M included).
    pub(super) saturation: Real,
    /// C `MOS1drainVcrit`/`MOS1sourceVcrit`.
    pub(super) vcrit: Real,
    /// C `MOS1Cbd`/`MOS1Cbs` (bottom) and `MOS1Cbdsw`/`MOS1Cbssw`.
    pub(super) bottom: Real,
    pub(super) sidewall: Real,
    pub(super) mj: Real,
    pub(super) mjsw: Real,
    /// C `MOS1tBulkPot`.
    pub(super) potential: Real,
    pub(super) fc: Real,
    /// C `MOS1tDepCap`.
    pub(super) depletion: Real,
    /// C `MOS1f2d/f3d/f4d` (or the `s` set).
    pub(super) f2: Real,
    pub(super) f3: Real,
    pub(super) f4: Real,
    pub(super) reverse: ReverseLaw,
}

/// `exp(-grading * ln(arg))`, with `mos1load.c`'s square-root shortcut.
fn grading_power(arg: Real, grading: Real) -> Real {
    if grading == 0.5 {
        1. / arg.sqrt()
    } else {
        (-grading * arg.ln()).exp()
    }
}

impl Junction {
    /// `mos1temp.c`/`mos3temp.c`: the junction of one side from its
    /// zero-bias bottom and sidewall capacitances, with the `f2/f3/f4` linear
    /// continuation above `FC * PB` in C's arithmetic.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        saturation: Real,
        vcrit: Real,
        bottom: Real,
        sidewall: Real,
        mj: Real,
        mjsw: Real,
        potential: Real,
        fc: Real,
        reverse: ReverseLaw,
    ) -> Self {
        let depletion = fc * potential;
        let arg = 1. - fc;
        let sarg = (-mj * arg.ln()).exp();
        let sargsw = (-mjsw * arg.ln()).exp();
        let f2 = bottom * (1. - fc * (1. + mj)) * sarg / arg
            + sidewall * (1. - fc * (1. + mjsw)) * sargsw / arg;
        let f3 = bottom * mj * sarg / arg / potential + sidewall * mjsw * sargsw / arg / potential;
        let f4 = bottom * potential * (1. - arg * sarg) / (1. - mj)
            + sidewall * potential * (1. - arg * sargsw) / (1. - mjsw)
            - f3 / 2. * (depletion * depletion)
            - depletion * f2;
        Self {
            saturation,
            vcrit,
            bottom,
            sidewall,
            mj,
            mjsw,
            potential,
            fc,
            depletion,
            f2,
            f3,
            f4,
            reverse,
        }
    }

    /// Depletion charge and capacitance at the device-frame bulk voltage `v`
    /// (`mos1load.c`). Above `FC * PB` C writes the bulk-drain and
    /// bulk-source polynomials with different association, kept here.
    pub(super) fn charge(&self, v: Real, drain: bool) -> (Real, Real) {
        if self.bottom == 0. && self.sidewall == 0. {
            return (0., 0.);
        }
        if v < self.depletion {
            let arg = 1. - v / self.potential;
            let sarg = grading_power(arg, self.mj);
            let sargsw = grading_power(arg, self.mjsw);
            (
                self.potential
                    * (self.bottom * (1. - arg * sarg) / (1. - self.mj)
                        + self.sidewall * (1. - arg * sargsw) / (1. - self.mjsw)),
                self.bottom * sarg + self.sidewall * sargsw,
            )
        } else if drain {
            (
                self.f4 + v * (self.f2 + v * self.f3 / 2.),
                self.f2 + v * self.f3,
            )
        } else {
            (
                self.f4 + v * (self.f2 + v * (self.f3 / 2.)),
                self.f2 + self.f3 * v,
            )
        }
    }

    /// The junction current and conductance with `gmin` at the device-frame
    /// voltage `v` (`mos1load.c`, `mos3load.c`).
    pub(super) fn current(&self, v: Real, vt: Real, gmin: Real) -> (Real, Real) {
        if v <= -3. * vt {
            match self.reverse {
                ReverseLaw::Constant => (gmin * v - self.saturation, gmin),
                ReverseLaw::Cubic => {
                    // C `CONSTe` is exp(1.0).
                    let arg = 3. * vt / (v * std::f64::consts::E);
                    let arg = arg * arg * arg;
                    (
                        -self.saturation * (1. + arg) + gmin * v,
                        self.saturation * 3. * arg / v + gmin,
                    )
                }
            }
        } else {
            let e = (v / vt).min(MAX_EXP_ARG).exp();
            (
                self.saturation * (e - 1.) + gmin * v,
                self.saturation * e / vt + gmin,
            )
        }
    }
}

/// Every temperature- and geometry-dependent quantity of one load (the
/// `mosXtemp.c` instance values and the start of `mosXload.c`); `params`
/// is the level's own part.
#[derive(Debug, Clone, Copy)]
pub(super) struct Operating<C> {
    pub(super) vt: Real,
    /// C `Beta`: `tKP * m * Weff / Leff`.
    pub(super) beta: Real,
    /// C `MOSxtVbi`.
    pub(super) vbi: Real,
    /// C `MOSxtPhi`.
    pub(super) phi: Real,
    /// C `MOSxtVto`.
    pub(super) vto: Real,
    /// The model GAMMA (given or extracted).
    pub(super) gamma: Real,
    /// C `OxideCap`: `cox * Leff * m * Weff`.
    pub(super) oxide: Real,
    pub(super) drain: Junction,
    pub(super) source: Junction,
    /// Gate-source, gate-drain and gate-bulk overlap capacitances.
    pub(super) overlap: [Real; 3],
    /// The level's channel parameters.
    pub(super) params: C,
}

impl<C> Operating<C> {
    /// The shell's range checks after a level's temperature derivation:
    /// `extra` are further level values that must be finite, `factors` the
    /// junction capacitance temperature factors that must be nonnegative.
    ///
    /// # Errors
    /// A nonfinite value, or a nonpositive PHI/PB/Beta/saturation current
    /// after temperature scaling.
    pub(super) fn check(
        &self,
        name: &str,
        label: &str,
        temp: Real,
        extra: &[Real],
        factors: &[Real],
    ) -> SpiceResult<()> {
        let finite = [
            self.vt,
            self.beta,
            self.vbi,
            self.vto,
            self.gamma,
            self.oxide,
            self.drain.saturation,
            self.source.saturation,
            self.drain.vcrit,
            self.source.vcrit,
            self.drain.bottom,
            self.drain.sidewall,
            self.source.bottom,
            self.source.sidewall,
            self.drain.f4,
            self.source.f4,
            self.overlap[0],
            self.overlap[1],
            self.overlap[2],
        ]
        .iter()
        .chain(extra)
        .all(|v| v.is_finite());
        let (phi, potential) = (self.phi, self.drain.potential);
        if !(finite && phi.is_finite() && phi > 0. && potential.is_finite() && potential > 0.)
            || self.beta <= 0.
            || self.drain.saturation <= 0.
            || self.source.saturation <= 0.
            || factors.iter().any(|f| !f.is_finite() || *f < 0.)
        {
            return Err(SpiceError::circuit(format!(
                "{name}: {label} parameters are out of range at {temp:.2} K \
                 (nonfinite value, or nonpositive PHI/PB/KP/IS after temperature scaling)"
            )));
        }
        Ok(())
    }
}

/// A level's drain current in C's normalized device frame (`vds >= 0` after
/// the drain/source interchange, polarity removed), with C's derivatives
/// (`MOSxgm`, `MOSxgds`, `MOSxgmbs`) and the local `von`/`vdsat` the Meyer
/// capacitances and limiting use.
#[derive(Debug, Clone, Copy)]
pub(super) struct DrainCurrent {
    pub(super) cdrain: Real,
    pub(super) gm: Real,
    pub(super) gds: Real,
    pub(super) gmbs: Real,
    pub(super) von: Real,
    pub(super) vdsat: Real,
}

/// The level-specific inputs of the `mosXnoi.c` generators.
#[derive(Debug, Clone, Copy)]
pub(super) struct NoiseShape {
    /// The NLEV 3 channel-noise `beta`.
    pub(super) beta: Real,
    /// Width and length of the flicker laws.
    pub(super) width: Real,
    pub(super) length: Real,
    /// The oxide capacitance factor the NLEV 0..3 laws divide by, and the
    /// square the SPICE3-compatible law divides by.
    pub(super) cox: Real,
    pub(super) cox_squared: Real,
}

/// What a MOS level supplies to the shared shell. See the module
/// documentation.
pub(super) trait MosLevel: std::fmt::Debug + Clone + Copy + Sized + 'static {
    /// The level's temperature-dependent channel parameters.
    type Params: std::fmt::Debug + Clone + Copy;
    /// The model `level` selector this implementation serves.
    const LEVEL: u8;
    /// Diagnostic label, e.g. `MOS1`.
    const LABEL: &'static str;
    /// The C device type (`mos1`, `mos3`) for the setup/load order.
    const DEVICE_TYPE: CDeviceType;
    /// C's device type for the `.noise` visiting order, or `None` when C's
    /// level has no `mosXnoi.c` routine at all (level 6): `.noise` then
    /// refuses instead of returning zeros.
    const NOISE_FAMILY: Option<NoiseFamily> = None;
    /// `CKTmkVolt` suffixes of the internal drain and source nodes.
    const PRIME_SUFFIXES: [&'static str; 2];

    /// The level's `mosXmparam.c` model table, for `inpgmod.c`'s instance
    /// fallback ([`crate::devices::schema::split_model_setters`]).
    fn model_schema() -> ScalarSchema<'static>;

    /// The level's validated model values plus the model-level **instance
    /// defaults** `inpgmod.c::create_model` collected from the same card.
    ///
    /// C looks a `.model` parameter up in the model table first and, when it is
    /// absent there, in the instance table; a settable instance parameter
    /// becomes a `GENmodel.defaults` entry that `inpdpar.c::INPdevParse`
    /// applies before the instance card's own setters. `l`, `w`, `ad`, `as`,
    /// `m`, `temp` and `dtemp` reach a MOS card that way - they are instance
    /// parameters, not model ones.
    ///
    /// # Errors
    /// Unknown or invalid setters, unsupported values.
    fn model_and_defaults<'a>(
        resolved: &'a ResolvedModel<'_>,
    ) -> SpiceResult<(
        ScalarValues,
        Vec<&'a crate::netlist::ast::ParameterAssignment>,
    )> {
        let instance = ScalarSchema {
            parameters: INSTANCE,
        };
        let (model_side, defaults) = crate::devices::schema::split_model_setters(
            resolved
                .card()
                .parameters
                .iter()
                .filter(|p| !p.name.eq_ignore_ascii_case("level")),
            &Self::model_schema(),
            &instance,
        );
        let values = Self::model_schema().validate(model_side, &resolved.card().location)?;
        Ok((values, defaults))
    }

    /// The validated model card (schema, defaults and C's card checks) and the
    /// model's instance defaults.
    ///
    /// # Errors
    /// Unknown or invalid setters, unsupported values.
    fn from_card<'a>(
        resolved: &'a ResolvedModel<'_>,
    ) -> SpiceResult<(Self, Vec<&'a crate::netlist::ast::ParameterAssignment>)>;

    /// The setters shared by every level.
    fn common(&self) -> &Common;

    /// Rejects geometry whose effective channel C would refuse or leave
    /// nonpositive.
    ///
    /// # Errors
    /// A nonpositive effective length or width.
    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()>;

    /// The temperature-, process- and geometry-dependent values of one load
    /// (`mosXtemp.c` and the start of `mosXload.c`).
    ///
    /// # Errors
    /// Parameters out of range at the device temperature.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Self::Params>>;

    /// The drain-current block of `mosXload.c` at the normalized
    /// device-frame gate voltage `vgx` (`vgs`, or `vgd` in inverse mode),
    /// `vdsm = vds * mode >= 0` and bulk voltage `vbx` (`vbs` or `vbd`).
    fn drain_current(
        &self,
        op: &Operating<Self::Params>,
        vgx: Real,
        vdsm: Real,
        vbx: Real,
    ) -> DrainCurrent;

    /// The `mosXnoi.c` beta, width, length and oxide capacitance factors, or
    /// `None` when C's level has no noise routine (level 6, see
    /// [`MosLevel::NOISE_FAMILY`]).
    fn noise_shape(
        &self,
        _geometry: &Geometry,
        _op: &Operating<Self::Params>,
    ) -> Option<NoiseShape> {
        None
    }

    /// The refusal `.noise` raises when the level has no generators to offer:
    /// C's level 6 registers `DEVnoise = NULL`, so C contributes nothing to a
    /// noise analysis rather than a model of the device.
    fn noise_refusal(device: &Mosfet<Self>) -> SpiceError {
        SpiceError::not_yet_ported(
            format!("{} noise of {}", Self::LABEL, device.name),
            format!(
                "src/spicelib/devices/mos{}/mos{}noi.c",
                Self::LEVEL,
                Self::LEVEL
            ),
        )
    }

    /// The model context and thermal-noise temperature of the level's
    /// `.noise` generators. The classic levels use their own flicker law and
    /// `NevalSrcInstanceTemp` (the instance temperature); `mos9noi.c` uses
    /// the SPICE3-compatible law and `NevalSrc` at the circuit temperature.
    fn noise_environment(context: &ModelContext, geometry: &Geometry) -> (ModelContext, Real) {
        let temperature = geometry
            .temp
            .unwrap_or(context.temperature + geometry.dtemp)
            + CELSIUS_TO_KELVIN;
        (*context, temperature)
    }

    /// Instance checks that need to know which setters the card gave (the
    /// geometry carries C's defaults). The default accepts every instance.
    ///
    /// # Errors
    /// Setter combinations the level refuses.
    fn check_instance(
        &self,
        _name: &str,
        _setters: &crate::devices::schema::ScalarValues,
    ) -> SpiceResult<()> {
        Ok(())
    }

    /// The `gmin` the bulk junctions add in a load. C's level 6
    /// (`mos6load.c`) multiplies its whole stamp, `gmin` included, by `M`;
    /// the other classic levels scale the currents and add `gmin` once (the
    /// default).
    fn junction_gmin(gmin: Real, _geometry: &Geometry) -> Real {
        gmin
    }

    /// Refuses a small-signal (`"AC"`) or pole-zero (`"pole-zero"`) load the
    /// level's C device does not have. The default accepts both.
    ///
    /// # Errors
    /// The level has no such C load.
    fn small_signal(_device: &Mosfet<Self>, _analysis: &str) -> SpiceResult<()> {
        Ok(())
    }

    /// `.disto` terms (`mosXdset.c`/`mosXdist.c`).
    ///
    /// # Errors
    /// [`SpiceError::NotYetPorted`] unless the level ports its distortion.
    fn distortion(
        device: &Mosfet<Self>,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(SpiceError::not_yet_ported(
            format!("{} distortion of {}", Self::LABEL, device.name),
            format!(
                "src/spicelib/devices/mos{}/mos{}dset.c",
                Self::LEVEL,
                Self::LEVEL
            ),
        ))
    }
}

/// `DEVqmeyer` (`devsup.c`): half of the bias-dependent gate-source,
/// gate-drain and gate-bulk capacitances `[capgs, capgd, capgb]`, from
/// device-frame voltages, in C's arithmetic.
pub(super) fn meyer(
    vgs: Real,
    vgd: Real,
    von: Real,
    vdsat: Real,
    phi: Real,
    cox: Real,
) -> [Real; 3] {
    let vgst = vgs - von;
    let vdsat = if vdsat > MEYER_MIN_VDSAT {
        vdsat
    } else {
        MEYER_MIN_VDSAT
    };
    if vgst <= -phi {
        [0., 0., cox / 2.]
    } else if vgst <= -phi / 2. {
        [0., 0., -vgst * cox / (2. * phi)]
    } else if vgst <= 0. {
        let capgb = -vgst * cox / (2. * phi);
        let capgs = vgst * cox / (1.5 * phi) + cox / 3.;
        let vds = vgs - vgd;
        if vds >= vdsat {
            [capgs, 0., capgb]
        } else {
            let vddif = 2.0 * vdsat - vds;
            let vddif1 = vdsat - vds;
            let vddif2 = vddif * vddif;
            [
                capgs * (1.0 - vddif1 * vddif1 / vddif2),
                capgs * (1.0 - vdsat * vdsat / vddif2),
                capgb,
            ]
        }
    } else {
        let vds = vgs - vgd;
        if vdsat <= vds {
            [cox / 3., 0., 0.]
        } else {
            let vddif = 2.0 * vdsat - vds;
            let vddif1 = vdsat - vds;
            let vddif2 = vddif * vddif;
            [
                cox * (1.0 - vddif1 * vddif1 / vddif2) / 3.,
                cox * (1.0 - vdsat * vdsat / vddif2) / 3.,
                0.,
            ]
        }
    }
}

/// What one `MOS1load` evaluation computes at device-frame voltages, before
/// any charge integration (C keeps most of it in the instance).
#[derive(Debug, Clone, Copy)]
pub(super) struct Eval {
    pub(super) vbs: Real,
    pub(super) vbd: Real,
    pub(super) vgs: Real,
    pub(super) vds: Real,
    pub(super) vgd: Real,
    pub(super) vgb: Real,
    /// C `MOS1mode` (1 normal, -1 inverse).
    pub(super) mode: Real,
    pub(super) cdrain: Real,
    pub(super) gm: Real,
    pub(super) gds: Real,
    pub(super) gmbs: Real,
    pub(super) gbs: Real,
    pub(super) cbs: Real,
    pub(super) gbd: Real,
    pub(super) cbd: Real,
    /// C `MOS1cd`: `mode * cdrain - cbd`.
    pub(super) cd: Real,
    /// `mos1load.c`'s local (device-frame) `von` and `vdsat`.
    pub(super) von: Real,
    pub(super) vdsat: Real,
    pub(super) qbs: Real,
    pub(super) capbs: Real,
    pub(super) qbd: Real,
    pub(super) capbd: Real,
    /// `DEVqmeyer`'s `[capgs, capgd, capgb]` (C `CKTstate0` `MOS1capgs`...).
    pub(super) half: [Real; 3],
}

/// How `mos1load.c` obtained the voltages of one load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    /// The `uic` initial load (`MODETRANOP | MODEUIC | MODEINITJCT`).
    Conditions,
    /// A load outside a device-limited Newton solve: the supplied solution.
    Exact,
    /// `MODEINITJCT`: start voltages, always nonconvergent.
    Junction,
    /// An `off` instance in `MODEINITFIX`: zero, exempt from `Check`.
    HeldOff,
    /// `MODEINITPRED`/`MODEINITTRAN` (`DEVpred` extrapolation) or
    /// `MODEINITFLOAT`/`MODEINITFIX` (the iterate), then limited.
    Limited,
}

/// A classic MOSFET: the shared frame around level `L`'s drain current, with
/// series resistance, junction geometry, Meyer gate charge and temperature
/// scaling.
#[derive(Debug)]
pub(super) struct Mosfet<L: MosLevel> {
    pub(super) name: String,
    /// The model card's name, for C's `.noise` visiting order.
    pub(super) model_name: String,
    /// External d, g, s, b followed by any internal drain/source nodes.
    pub(super) terminals: Vec<NodeId>,
    /// d', g, s', b: the nodes the intrinsic device sees.
    pub(super) inner: [NodeId; 4],
    pub(super) model: L,
    pub(super) geometry: Geometry,
    /// Drain/source series conductances (zero without an internal node).
    pub(super) series: [Real; 2],
    /// `OFF` and `ICVDS`/`ICVGS`/`ICVBS` (C `MOS1off`, `MOS1icV*`).
    pub(super) initial: InstanceInitial,
    /// The derivation at the first circuit and nominal temperature it was
    /// asked for, which C's `MOSxtemp` computes once; it depends on nothing
    /// else, so loads reuse it.
    pub(super) derived: OnceCell<(Real, Real, Operating<L::Params>)>,
}

impl<L: MosLevel> Mosfet<L> {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != L::LEVEL || i.nodes.len() != 4 {
            return Err(SpiceError::circuit(format!(
                "{} needs level {} and four terminals",
                L::LABEL,
                L::LEVEL
            )));
        }
        let (initial, setters) = crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)?;
        let (model, model_defaults) = L::from_card(resolved)?;
        let v = crate::devices::schema::with_defaults(
            &ScalarSchema {
                parameters: INSTANCE,
            },
            &model_defaults,
            setters.iter(),
            &i.location,
        )?;
        let required = |name: &str| required(&v, name, L::LABEL);
        let geometry = Geometry {
            m: required("m")?,
            l: required("l")?,
            w: required("w")?,
            ad: required("ad")?,
            as_: required("as")?,
            pd: required("pd")?,
            ps: required("ps")?,
            nrd: required("nrd")?,
            nrs: required("nrs")?,
            temp: v.get("temp").map(|v| v.value),
            dtemp: required("dtemp")?,
        };
        model.check_geometry(&i.name, &geometry)?;
        model.check_instance(&i.name, &v)?;
        let series = Self::series(&model, &geometry)?;
        let mut device = Self {
            name: i.name.clone(),
            model_name: resolved.card().name.clone(),
            terminals: vec![],
            inner: [NodeId::GROUND; 4],
            model,
            geometry,
            series,
            initial,
            derived: OnceCell::new(),
        };
        // Validate every derivation before interning nodes.
        device.operating(context)?;
        let mut staged = nodes.clone();
        let external: Vec<NodeId> = i.nodes.iter().map(|n| staged.intern(n)).collect();
        device.terminals.clone_from(&external);
        device.inner = [external[0], external[1], external[2], external[3]];
        for (index, conductance, suffix) in [
            (0, series[0], L::PRIME_SUFFIXES[0]),
            (2, series[1], L::PRIME_SUFFIXES[1]),
        ] {
            if conductance > 0. {
                let name = format!("{}#{suffix}", i.name);
                if staged.get(&name).is_some() {
                    return Err(SpiceError::circuit(format!(
                        "{} internal-node name collision: {name}",
                        L::LABEL
                    )));
                }
                let prime = staged.intern(&name);
                staged.set_kind(prime, NodeKind::Internal);
                device.terminals.push(prime);
                device.inner[index] = prime;
            }
        }
        *nodes = staged;
        Ok(Box::new(device))
    }

    /// The drain and source series conductances of `geometry`.
    fn series(model: &L, geometry: &Geometry) -> SpiceResult<[Real; 2]> {
        let common = model.common();
        Ok([
            series_conductance(
                common.rd,
                common.rsh,
                geometry.nrd,
                geometry.m,
                "drain",
                L::LABEL,
            )?,
            series_conductance(
                common.rs,
                common.rsh,
                geometry.nrs,
                geometry.m,
                "source",
                L::LABEL,
            )?,
        ])
    }

    /// The temperature-, process- and geometry-dependent values of one load.
    pub(super) fn operating(&self, context: &ModelContext) -> SpiceResult<Operating<L::Params>> {
        if let Some(op) = self.cached(context.temperature, context.nominal_temperature) {
            return Ok(*op);
        }
        let op = self.model.operating(&self.name, &self.geometry, context)?;
        let _ = self
            .derived
            .set((context.temperature, context.nominal_temperature, op));
        Ok(op)
    }

    /// The cached derivation when it was made at these circuit and nominal
    /// temperatures.
    #[inline]
    fn cached(&self, temperature: Real, nominal: Real) -> Option<&Operating<L::Params>> {
        self.derived
            .get()
            .filter(|(t, n, _)| {
                t.to_bits() == temperature.to_bits() && n.to_bits() == nominal.to_bits()
            })
            .map(|(_, _, op)| op)
    }

    /// `mos1load.c` from `vbd = vbs - vds` on, at the device-frame
    /// `[vbs, vgs, vds]`: junction currents with `gmin`, the level's
    /// channel, the depletion charges and `DEVqmeyer`, before any charge
    /// integration. Every expression keeps C's operation order.
    pub(super) fn evaluate(
        &self,
        op: &Operating<L::Params>,
        [vbs, vgs, vds]: [Real; 3],
        gmin: Real,
    ) -> SpiceResult<Eval> {
        let vt = op.vt;
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgb = vgs - vbs;
        let (cbs, gbs) = op.source.current(vbs, vt, gmin);
        let (cbd, gbd) = op.drain.current(vbd, vt, gmin);
        let mode: Real = if vds >= 0. { 1. } else { -1. };
        let normal = mode == 1.;
        let DrainCurrent {
            cdrain,
            gm,
            gds,
            gmbs,
            von,
            vdsat,
        } = self.model.drain_current(
            op,
            if normal { vgs } else { vgd },
            vds * mode,
            if normal { vbs } else { vbd },
        );
        let (qbs, capbs) = op.source.charge(vbs, false);
        let (qbd, capbd) = op.drain.charge(vbd, true);
        let half = if normal {
            meyer(vgs, vgd, von, vdsat, op.phi, op.oxide)
        } else {
            let [capgd, capgs, capgb] = meyer(vgd, vgs, von, vdsat, op.phi, op.oxide);
            [capgs, capgd, capgb]
        };
        let eval = Eval {
            vbs,
            vbd,
            vgs,
            vds,
            vgd,
            vgb,
            mode,
            cdrain,
            gm,
            gds,
            gmbs,
            gbs,
            cbs,
            gbd,
            cbd,
            cd: mode * cdrain - cbd,
            von,
            vdsat,
            qbs,
            capbs,
            qbd,
            capbd,
            half,
        };
        let values = [
            eval.cdrain,
            eval.gm,
            eval.gds,
            eval.gmbs,
            eval.gbs,
            eval.cbs,
            eval.gbd,
            eval.cbd,
            eval.von,
            eval.qbs,
            eval.capbs,
            eval.qbd,
            eval.capbd,
            half[0],
            half[1],
            half[2],
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: format!("{} {}", L::LABEL, self.name),
                message: "nonfinite channel/junction values".into(),
            });
        }
        Ok(eval)
    }

    /// `mos1load.c`'s limiting of the candidate `[vbs, vgs, vds]` against
    /// the previous load's (C `CKTstate0`) `old = [vbs, vbd, vgs, vds]` and
    /// `MOS1von`: `DEVfetlim` on the gate voltage (`vgs`, or `vgd` when the
    /// old `vds` is negative), `DEVlimvds` and `DEVpnjlim` on the forward
    /// junction. Returns the voltages and C's `Check` (from `DEVpnjlim`).
    fn limit(
        &self,
        op: &Operating<L::Params>,
        raw: [Real; 3],
        old: [Real; 4],
        here_von: Real,
    ) -> ([Real; 3], bool) {
        let [mut vbs, mut vgs, mut vds] = raw;
        let [old_vbs, old_vbd, old_vgs, old_vds] = old;
        let mut vbd = vbs - vds;
        let mut vgd = vgs - vds;
        let vgdo = old_vgs - old_vds;
        let von = self.model.common().pol * here_von;
        if old_vds >= 0. {
            vgs = limiting::fetlim(vgs, old_vgs, von);
            vds = vgs - vgd;
            vds = limiting::limvds(vds, old_vds);
        } else {
            vgd = limiting::fetlim(vgd, vgdo, von);
            vds = vgs - vgd;
            // C skips this under `.options fixlimit`, which is not ported.
            vds = -limiting::limvds(-vds, -old_vds);
            vgs = vgd + vds;
        }
        let check = if vds >= 0. {
            let limited = limiting::pnjlim(vbs, old_vbs, op.vt, op.source.vcrit);
            vbs = limited.value;
            limited.limited
        } else {
            let limited = limiting::pnjlim(vbd, old_vbd, op.vt, op.drain.vcrit);
            vbd = limited.value;
            vbs = vbd + vds;
            limited.limited
        };
        ([vbs, vgs, vds], check)
    }

    /// `mos1load.c`'s device-frame `[vbs, vgs, vds]` for this load and how
    /// they were obtained, with C's `Check`.
    ///
    /// `raw` is the iterate's (C `CKTrhsOld`) and `ic` the instance initial
    /// conditions of [`Self::start_conditions`]. `MODEINITJCT` starts an
    /// `off` instance at zero, otherwise at `type * IC` unless all three
    /// are zero, where it takes `vbs = -1`, `vgs = type * tVto`, `vds = 0`;
    /// the `uic` initial load keeps all-zero conditions. An `off` instance
    /// is held at zero through `MODEINITFIX`. `MODEINITPRED`/`MODEINITTRAN`
    /// extrapolate the accepted voltages with `DEVpred`'s
    /// `(1 + xfact) state1 - xfact state2` (`xfact = 1` in a `.dc` sweep,
    /// whose `CKTdelta` equals every `CKTdeltaOld`) and limit against
    /// `CKTstate1` (copied into `CKTstate0` there); other loads limit the
    /// iterate against the previous load.
    fn select(
        &self,
        op: &Operating<L::Params>,
        states: &DeviceState<'_>,
        raw: [Real; 3],
        ic: [Real; 3],
        xfact: Real,
    ) -> ([Real; 3], Start, bool) {
        let pol = self.model.common().pol;
        let off = self.initial.off;
        if states.initial_conditions() {
            let v = if off { [0.; 3] } else { ic.map(|v| pol * v) };
            return (v, Start::Conditions, true);
        }
        if !states.device_limiting() {
            return (raw, Start::Exact, false);
        }
        match states.phase() {
            IterationPhase::Junction => {
                let v = if off {
                    [0.; 3]
                } else {
                    let v = ic.map(|v| pol * v);
                    if v.iter().all(|v| *v == 0.) {
                        [-1., pol * op.vto, 0.]
                    } else {
                        v
                    }
                };
                (v, Start::Junction, true)
            }
            IterationPhase::Fix if off => ([0.; 3], Start::HeldOff, true),
            IterationPhase::Predict => {
                let accepted = |slot: usize| states.accepted(1, slot);
                let predicted = |slot: usize| states.predicted(slot, xfact);
                let (
                    Some(vbs),
                    Some(vgs),
                    Some(vds),
                    Some(old_vbs),
                    Some(old_vgs),
                    Some(old_vds),
                    Some(von),
                ) = (
                    predicted(slot::VBS),
                    predicted(slot::VGS),
                    predicted(slot::VDS),
                    accepted(slot::VBS),
                    accepted(slot::VGS),
                    accepted(slot::VDS),
                    accepted(slot::VON),
                )
                else {
                    return (raw, Start::Exact, false);
                };
                let old = [old_vbs, old_vbs - old_vds, old_vgs, old_vds];
                let (v, check) = self.limit(op, [vbs, vgs, vds], old, von);
                (v, Start::Limited, check)
            }
            IterationPhase::Fix | IterationPhase::Float => {
                let previous = |slot: usize| states.iterate(slot).filter(|v| v.is_finite());
                let (Some(old_vbs), Some(old_vbd), Some(old_vgs), Some(old_vds), Some(von)) = (
                    previous(slot::VBS),
                    previous(slot::VBD),
                    previous(slot::VGS),
                    previous(slot::VDS),
                    previous(slot::VON),
                ) else {
                    return (raw, Start::Exact, false);
                };
                let (v, check) = self.limit(op, raw, [old_vbs, old_vbd, old_vgs, old_vds], von);
                (v, Start::Limited, check)
            }
        }
    }

    /// The instance initial conditions `[ICVBS, ICVGS, ICVDS]` for this
    /// load: unset components are the external terminal voltages of the
    /// solution in the `uic` initial load (`mos1ic.c`, called by `CKTic`
    /// only under `MODEUIC`) and zero (C's unset default) otherwise.
    fn start_conditions(&self, conditions: bool, voltage: impl Fn(NodeId) -> Real) -> [Real; 3] {
        let [vds, vgs, vbs] = [0, 1, 2].map(|k| self.initial.values[k]);
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        if conditions {
            [
                vbs.unwrap_or_else(|| voltage(b) - voltage(s)),
                vgs.unwrap_or_else(|| voltage(g) - voltage(s)),
                vds.unwrap_or_else(|| voltage(d) - voltage(s)),
            ]
        } else {
            [vbs, vgs, vds].map(|v| v.unwrap_or(0.))
        }
    }

    /// The `MODEINITSMSIG` evaluation the small-signal loads read
    /// (`acan.c`, `noisean.c`, `pzan.c` load once in that mode after the
    /// operating point). C limits the bias voltages against the converged
    /// load's `CKTstate0` there (`mos1load.c`). The small-signal contexts
    /// supply the accepted `CKTstate1` instead, so the limiting is replayed
    /// against the bias itself: when the converged load limited nothing
    /// (it stored its own iterate up to the rounding of `vds = vgs - vgd`,
    /// and of `vgs = vgd + vds` for a reversed device), every limiter is the
    /// identity and only that rounding remains, which is what C evaluates.
    pub(super) fn small_signal_eval(
        &self,
        op: &Operating<L::Params>,
        voltage: impl Fn(NodeId) -> Real,
        gmin: Real,
    ) -> SpiceResult<Eval> {
        let pol = self.model.common().pol;
        let [dp, g, sp, b] = self.inner.map(&voltage);
        let raw = [pol * (b - sp), pol * (g - sp), pol * (dp - sp)];
        let [vbs, vgs, vds] = raw;
        // DEVfetlim/DEVpnjlim/DEVlimvds leave an unchanged voltage alone
        // whatever `von` is.
        let (limited, _) = self.limit(op, raw, [vbs, vbs - vds, vgs, vds], 0.);
        self.evaluate(op, limited, gmin)
    }
}

/// `mos1temp.c`/`mos3temp.c`: RD/RS take precedence over RSH times the
/// number of squares. `mos1set.c` creates the internal node exactly when the
/// conductance is nonzero, except for combinations C leaves singular or
/// infinite, which are rejected here.
fn series_conductance(
    resistance: Option<Real>,
    sheet: Option<Real>,
    squares: Real,
    m: Real,
    side: &str,
    label: &str,
) -> SpiceResult<Real> {
    let sheet_node = sheet.is_some_and(|rsh| rsh != 0.) && squares != 0.;
    let conductance = match (resistance, sheet) {
        (Some(r), _) if r != 0. => m / r,
        (Some(_), _) => {
            if sheet_node {
                return Err(SpiceError::circuit(format!(
                    "{label} RSH creates an internal {side} node that an explicit zero \
                     resistance leaves floating"
                )));
            }
            0.
        }
        (None, Some(rsh)) if rsh != 0. => {
            if squares == 0. {
                return Err(SpiceError::circuit(format!(
                    "{label} RSH with zero {side} squares gives C an infinite conductance"
                )));
            }
            m / (rsh * squares)
        }
        _ => 0.,
    };
    if !conductance.is_finite() {
        return Err(SpiceError::circuit(format!(
            "{label} {side} series conductance overflow"
        )));
    }
    Ok(conductance)
}

impl<L: MosLevel> Device for Mosfet<L> {
    /// `mos1set.c`, `mos3set.c`.
    fn c_device_type(&self) -> CDeviceType {
        L::DEVICE_TYPE
    }

    /// `mos1set.c` (`mos3set.c` allocates the same elements in the same
    /// order).
    fn setup_matrix(
        &self,
        setup: &mut crate::devices::setup::MatrixSetup<'_>,
    ) -> SpiceResult<bool> {
        let [d, g, s, b] = [
            self.terminals[0],
            self.terminals[1],
            self.terminals[2],
            self.terminals[3],
        ];
        let [dp, _, sp, _] = self.inner;
        for (row, col) in [
            (d, d),
            (g, g),
            (s, s),
            (b, b),
            (dp, dp),
            (sp, sp),
            (d, dp),
            (g, b),
            (g, dp),
            (g, sp),
            (s, sp),
            (b, dp),
            (b, sp),
            (dp, sp),
            (dp, d),
            (b, g),
            (dp, g),
            (sp, g),
            (sp, s),
            (dp, b),
            (sp, b),
            (sp, dp),
        ] {
            setup.nodes(row, col);
        }
        Ok(true)
    }
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        let p = &self.geometry;
        Ok(match key {
            "w" => Some(p.w),
            "m" => Some(p.m),
            "ad" => Some(p.ad),
            "as" => Some(p.as_),
            "pd" => Some(p.pd),
            "ps" => Some(p.ps),
            "nrd" => Some(p.nrd),
            "nrs" => Some(p.nrs),
            "temp" => Some(p.temp.map_or(context.temperature + p.dtemp, |t| t - 273.15)),
            "dtemp" => Some(p.dtemp),
            _ => None,
        })
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'm'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn has_start_settings(&self) -> bool {
        self.initial.off || self.initial.values.iter().flatten().any(|v| *v != 0.)
    }
    fn state_count(&self) -> usize {
        slot::COUNT
    }
    /// `mos1trun.c`/`mos3trun.c`: only the three Meyer gate charges control
    /// the timestep; the bulk junction charges do not.
    fn truncation_slots(&self) -> Vec<usize> {
        slot::QG.to_vec()
    }
    /// `mos1.c` `MOS1pTable` (and `MOS3pTable`): M, L, W, AD, AS, PD, PS,
    /// NRD, NRS, TEMP and DTEMP, which `mosXtemp.c`/`mosXload.c` re-derive
    /// (`dctrcurv.c` `DCTsetInstParam`).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        [
            "icvds", "icvgs", "icvbs", "m", "l", "w", "ad", "as", "pd", "ps", "nrd", "nrs", "temp",
            "dtemp",
        ]
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(keyword))
    }
    /// `MOS1param` then `MOS1temp`, with the instance schema domains. M, NRD
    /// and NRS re-derive the drain/source series conductances; a value that
    /// would create or remove an internal node (`mos1set.c` decides those once,
    /// at setup) is rejected rather than changing the topology mid-sweep.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let mut initial = self.initial.clone();
        if let Some(index) = ["icvds", "icvgs", "icvbs"]
            .iter()
            .position(|k| *k == parameter)
        {
            crate::devices::sweep::check_swept(&self.name, parameter, value, true, "finite")?;
            initial.values[index] = Some(value);
        }
        use crate::devices::sweep::check_swept;
        let mut geometry = self.geometry;
        let check = |ok: bool, what: &str| check_swept(&self.name, parameter, value, ok, what);
        match parameter {
            "icvds" | "icvgs" | "icvbs" => {}
            "m" => {
                check(value > 0., "positive")?;
                geometry.m = value;
            }
            "l" => {
                check(value > 0., "positive")?;
                geometry.l = value;
            }
            "w" => {
                check(value > 0., "positive")?;
                geometry.w = value;
            }
            "ad" | "as" | "pd" | "ps" | "nrd" | "nrs" => {
                check(value >= 0., "nonnegative")?;
                *match parameter {
                    "ad" => &mut geometry.ad,
                    "as" => &mut geometry.as_,
                    "pd" => &mut geometry.pd,
                    "ps" => &mut geometry.ps,
                    "nrd" => &mut geometry.nrd,
                    _ => &mut geometry.nrs,
                } = value;
            }
            "temp" => {
                check(value + CELSIUS_TO_KELVIN > 0., "above absolute zero")?;
                geometry.temp = Some(value);
            }
            "dtemp" => {
                check(true, "finite")?;
                geometry.dtemp = value;
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "{}: {} parameter {parameter} cannot be swept",
                    self.name,
                    L::LABEL
                )));
            }
        }
        self.model.check_geometry(&self.name, &geometry)?;
        let series = Self::series(&self.model, &geometry)?;
        if series
            .iter()
            .zip(self.series)
            .any(|(new, old)| (*new > 0.) != (old > 0.))
        {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "{}: swept {parameter}={value} would add or remove a {} internal \
                     drain/source node",
                    self.name,
                    L::LABEL
                ),
                location: None,
            });
        }
        let device = Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            terminals: self.terminals.clone(),
            inner: self.inner,
            model: self.model,
            geometry,
            series,
            initial,
            derived: OnceCell::new(),
        };
        device.operating(context)?;
        Ok(Box::new(device))
    }
    /// `MOS1load` (`mos1load.c`; `mos3load.c` for level 3), without bypass,
    /// which ngspice disables by default, and without `.sens` perturbation.
    #[allow(clippy::needless_range_loop)] // C's three gate charges, side by side.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit(format!(
                "{} AC needs small-signal assembly",
                L::LABEL
            )));
        }
        let fresh;
        let op = match self.cached(context.temperature, context.nominal_temperature) {
            Some(op) => op,
            None => {
                fresh = self.operating(&context.model_context())?;
                &fresh
            }
        };
        let pol = self.model.common().pol;
        let [dpv, gv, spv, bv] = self.inner.map(|n| context.node_voltage(n));
        let raw = [pol * (bv - spv), pol * (gv - spv), pol * (dpv - spv)];
        let ic = self.start_conditions(context.states.initial_conditions(), |n| {
            context.node_voltage(n)
        });
        let integration = context.integration;
        let transient = integration.is_some();
        let phase = context.states.phase();
        // C `xfact = CKTdelta / CKTdeltaOld[1]`; one in a `.dc` sweep.
        let xfact = integration.map_or(1., crate::maths::integrator::Coefficients::predictor_ratio);
        let (v, start, check) = self.select(op, &context.states, raw, ic, xfact);
        let mut e = self.evaluate(op, v, L::junction_gmin(context.gmin, &self.geometry))?;
        let states = &mut context.states;

        // Bulk junction charges (C computes them in MODETRAN, MODETRANOP and
        // MODEINITSMSIG; computing them in every load changes no stamp) and
        // their NIintegrate companions folded into gbd/cbd/cd and gbs/cbs.
        states.set(slot::QBD, e.qbd)?;
        states.set(slot::QBS, e.qbs)?;
        let mut ag0 = 0.;
        if let Some(coefficients) = &integration {
            ag0 = coefficients.ag()[0];
            let companion = |context: &mut StampContext<'_>, charge, capacitance, slot| {
                junction_companion(
                    context,
                    coefficients,
                    JunctionPoint {
                        current: 0.,
                        conductance: 0.,
                        charge,
                        capacitance,
                    },
                    slot,
                )
            };
            let (geq, ccap) = companion(context, e.qbd, e.capbd, slot::QBD)?;
            e.gbd += geq;
            e.cbd += ccap;
            e.cd -= ccap;
            let (geq, ccap) = companion(context, e.qbs, e.capbs, slot::QBS)?;
            e.gbs += geq;
            e.cbs += ccap;
        } else {
            context.states.set(slot::QBD + 1, 0.)?;
            context.states.set(slot::QBS + 1, 0.)?;
        }
        let states = &mut context.states;
        // C: `if (off == 0 || !(MODEINITFIX | MODEINITSMSIG)) if (Check)
        // CKTnoncon++`; loads outside a limited solve never report.
        let report = match start {
            Start::Junction => true,
            Start::Limited => check,
            Start::Conditions | Start::Exact | Start::HeldOff => false,
        };
        if report {
            states.report_nonconvergence()?;
        }
        for (slot, value) in [
            (slot::VBS, e.vbs),
            (slot::VBD, e.vbd),
            (slot::VGS, e.vgs),
            (slot::VDS, e.vds),
            (slot::VON, pol * e.von),
            (slot::VDSAT, pol * e.vdsat),
            (slot::MODE, e.mode),
        ] {
            states.set(slot, value)?;
        }

        // Meyer gate capacitances and charges.
        let initial_transient =
            transient && phase == IterationPhase::Predict && states.initial_transient();
        let s1 = |states: &DeviceState<'_>, slot: usize| {
            states.accepted(1, slot).ok_or_else(|| {
                SpiceError::circuit(format!(
                    "{}: missing accepted {} state",
                    self.name,
                    L::LABEL
                ))
            })
        };
        let gate = [e.vgs, e.vgd, e.vgb];
        let mut cap = [0.; 3];
        let mut charge = [0.; 3];
        for k in 0..3 {
            states.set(slot::CAPG[k], e.half[k])?;
            cap[k] = if transient {
                e.half[k] + s1(states, slot::CAPG[k])? + op.overlap[k]
            } else {
                2. * e.half[k] + op.overlap[k]
            };
        }
        if transient && phase == IterationPhase::Predict {
            for k in 0..3 {
                charge[k] = states.predicted(slot::QG[k], xfact).ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: missing accepted {} charge",
                        self.name,
                        L::LABEL
                    ))
                })?;
            }
        } else if transient {
            let vgs1 = s1(states, slot::VGS)?;
            let gate1 = [
                vgs1,
                vgs1 - s1(states, slot::VDS)?,
                vgs1 - s1(states, slot::VBS)?,
            ];
            for k in 0..3 {
                charge[k] = (gate[k] - gate1[k]) * cap[k] + s1(states, slot::QG[k])?;
            }
        } else {
            for k in 0..3 {
                charge[k] = gate[k] * cap[k];
            }
        }
        for k in 0..3 {
            states.set(slot::QG[k], charge[k])?;
        }
        let mut gcg = [0.; 3];
        let mut ceqg = [0.; 3];
        match &integration {
            Some(coefficients) if !initial_transient => {
                for k in 0..3 {
                    let (geq, ccap) = junction_companion(
                        context,
                        coefficients,
                        JunctionPoint {
                            current: 0.,
                            conductance: 0.,
                            charge: charge[k],
                            capacitance: cap[k],
                        },
                        slot::QG[k],
                    )?;
                    gcg[k] = geq;
                    ceqg[k] = ccap - ag0 * charge[k];
                }
                for k in 0..3 {
                    ceqg[k] = ceqg[k] - gcg[k] * gate[k] + ag0 * charge[k];
                }
            }
            _ => {
                for k in 0..3 {
                    context.states.set(slot::QG[k] + 1, 0.)?;
                }
            }
        }
        let [gcgs, gcgd, gcgb] = gcg;
        let [ceqgs, ceqgd, ceqgb] = ceqg;

        // What MOS1convTest and the small-signal loads read (C `here`).
        for (slot, value) in [
            (slot::CD, e.cd),
            (slot::CBS, e.cbs),
            (slot::CBD, e.cbd),
            (slot::GM, e.gm),
            (slot::GDS, e.gds),
            (slot::GMBS, e.gmbs),
            (slot::GBD, e.gbd),
            (slot::GBS, e.gbs),
            (slot::CAPBD, e.capbd),
            (slot::CAPBS, e.capbs),
        ] {
            context.states.set(slot, value)?;
        }

        // Current vector and Y matrix, in mos1load.c's order and arithmetic.
        let t = pol;
        let ceqbs = t * (e.cbs - e.gbs * e.vbs);
        let ceqbd = t * (e.cbd - e.gbd * e.vbd);
        let (xnrm, xrev, cdreq) = if e.mode >= 0. {
            (
                1.,
                0.,
                t * (e.cdrain - e.gds * e.vds - e.gm * e.vgs - e.gmbs * e.vbs),
            )
        } else {
            (
                0.,
                1.,
                -t * (e.cdrain - e.gds * (-e.vds) - e.gm * e.vgd - e.gmbs * e.vbd),
            )
        };
        let [d, g, s, b] = [
            self.terminals[0],
            self.terminals[1],
            self.terminals[2],
            self.terminals[3],
        ];
        let [dp, _, sp, _] = self.inner;
        let [gdr, gsr] = self.series;
        let (gm, gds, gmbs, gbd, gbs) = (e.gm, e.gds, e.gmbs, e.gbd, e.gbs);
        context.stamp_rhs(g, -(t * (ceqgs + ceqgb + ceqgd)))?;
        context.stamp_rhs(b, -(ceqbs + ceqbd - t * ceqgb))?;
        context.stamp_rhs(dp, ceqbd - cdreq + t * ceqgd)?;
        context.stamp_rhs(sp, cdreq + ceqbs + t * ceqgs)?;
        context.stamp_fixed([
            (d, d, gdr),
            (g, g, gcgd + gcgs + gcgb),
            (s, s, gsr),
            (b, b, gbd + gbs + gcgb),
            (dp, dp, gdr + gds + gbd + xrev * (gm + gmbs) + gcgd),
            (sp, sp, gsr + gds + gbs + xnrm * (gm + gmbs) + gcgs),
            (d, dp, -gdr),
            (g, b, -gcgb),
            (g, dp, -gcgd),
            (g, sp, -gcgs),
            (s, sp, -gsr),
            (b, g, -gcgb),
            (b, dp, -gbd),
            (b, sp, -gbs),
            (dp, d, -gdr),
            (dp, g, (xnrm - xrev) * gm - gcgd),
            (dp, b, -gbd + (xnrm - xrev) * gmbs),
            (dp, sp, -gds - xnrm * (gm + gmbs)),
            (sp, g, -(xnrm - xrev) * gm - gcgs),
            (sp, s, -gsr),
            (sp, b, -gbs - (xnrm - xrev) * gmbs),
            (sp, dp, -gds - xrev * (gm + gmbs)),
        ])?;
        Ok(())
    }
    /// `MOS1convTest` (`mos1conv.c`, equal to `mos3conv.c`): the drain and
    /// bulk currents predicted linearly from the last load at the new
    /// iterate (`CKTrhs`) against their loaded values, in C's arithmetic.
    fn convergence_test(
        &self,
        context: &crate::devices::ConvergenceContext<'_>,
    ) -> SpiceResult<bool> {
        let at = |k: usize| context.states.get(k).copied().unwrap_or(0.);
        let pol = self.model.common().pol;
        let [dp, g, sp, b] = self.inner.map(|n| context.voltage(n));
        let vbs = pol * (b - sp);
        let vgs = pol * (g - sp);
        let vds = pol * (dp - sp);
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgdo = at(slot::VGS) - at(slot::VDS);
        let delvbs = vbs - at(slot::VBS);
        let delvbd = vbd - at(slot::VBD);
        let delvgs = vgs - at(slot::VGS);
        let delvds = vds - at(slot::VDS);
        let delvgd = vgd - vgdo;
        let (cd, gbd, gmbs, gm, gds) = (
            at(slot::CD),
            at(slot::GBD),
            at(slot::GMBS),
            at(slot::GM),
            at(slot::GDS),
        );
        let (cbs, cbd, gbs) = (at(slot::CBS), at(slot::CBD), at(slot::GBS));
        let cdhat = if at(slot::MODE) >= 0. {
            cd - gbd * delvbd + gmbs * delvbs + gm * delvgs + gds * delvds
        } else {
            cd - (gbd - gmbs) * delvbd - gm * delvgd + gds * delvds
        };
        let cbhat = cbs + cbd + gbd * delvbd + gbs * delvbs;
        let tol = context.reltol * cdhat.abs().max(cd.abs()) + context.abstol;
        if (cdhat - cd).abs() >= tol {
            return Ok(true);
        }
        let tol = context.reltol * cbhat.abs().max((cbs + cbd).abs()) + context.abstol;
        Ok((cbhat - (cbs + cbd)).abs() > tol)
    }

    /// `mos1acld.c` (`mos3acld.c` with the effective width): the
    /// conductances of the `MODEINITSMSIG` load plus `j omega` times its
    /// junction capacitances and the Meyer capacitances
    /// `half + half + overlap`, stamped in C's order; each element's dynamic
    /// terms are one [`LinearContext::dynamic_sum`], as C adds
    /// `xgd + xgs + xgb` in one step.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        // Only a load at a solved bias point (AC and the analyses built on it)
        // carries the bias state; the zero-bias structural assemblies of the
        // DC and transient drivers do not, and stay level-independent.
        if context.states.is_some() {
            L::small_signal(self, "AC")?;
        }
        let op = self.operating(context.model_context)?;
        let unknowns = context.unknowns;
        let voltage = |n: NodeId| unknowns.node_row(n).and_then(|r| bias.get(r)).unwrap_or(0.);
        let e = self.small_signal_eval(
            &op,
            voltage,
            L::junction_gmin(context.model_context.gmin, &self.geometry),
        )?;
        let cap: [Real; 3] = std::array::from_fn(|k| e.half[k] + e.half[k] + op.overlap[k]);
        let [capgs, capgd, capgb] = cap;
        let (xnrm, xrev) = if e.mode < 0. { (0., 1.) } else { (1., 0.) };
        let [d, g, s, b] = [
            self.terminals[0],
            self.terminals[1],
            self.terminals[2],
            self.terminals[3],
        ];
        let [dp, _, sp, _] = self.inner;
        let [gdr, gsr] = self.series;
        let (gm, gds, gmbs, gbd, gbs) = (e.gm, e.gds, e.gmbs, e.gbd, e.gbs);
        let (capbd, capbs) = (e.capbd, e.capbs);
        for (row, col, terms) in [
            (g, g, &[capgd, capgs, capgb][..]),
            (b, b, &[capgb, capbd, capbs][..]),
            (dp, dp, &[capgd, capbd][..]),
            (sp, sp, &[capgs, capbs][..]),
            (g, b, &[-capgb][..]),
            (g, dp, &[-capgd][..]),
            (g, sp, &[-capgs][..]),
            (b, g, &[-capgb][..]),
            (b, dp, &[-capbd][..]),
            (b, sp, &[-capbs][..]),
            (dp, g, &[-capgd][..]),
            (dp, b, &[-capbd][..]),
            (sp, g, &[-capgs][..]),
            (sp, b, &[-capbs][..]),
        ] {
            context.dynamic_sum(row, col, terms)?;
        }
        for (row, col, value) in [
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
        ] {
            if let (Some(r), Some(c)) = (unknowns.node_row(row), unknowns.node_row(col)) {
                context.system.a.add(r, c, value)?;
            }
        }
        Ok(())
    }

    /// Pole-zero load: C `mos1pzld.c`/`mos3pzld.c` equal the AC load with
    /// `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        L::small_signal(self, "pole-zero")?;
        self.assemble_small_signal(context, bias)
    }

    /// The level's `.disto` routine ([`MosLevel::distortion`]).
    fn distortion(
        &self,
        context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        L::distortion(self, context)
    }

    /// `mos1noi.c`/`mos3noi.c` at the operating point: RD/RS thermal noise
    /// and the channel thermal noise `Sid` at the instance temperature, and
    /// the flicker law of NLEV between the internal drain and source. NLEV <
    /// 3 uses `Sid = 2/3 abs(gm)`; NLEV 3 the `GDSNOI`-scaled region formula
    /// with the level's `beta`. Flicker: NLEV 0 `m KF abs(cd/m)^AF / (f L^2
    /// Cox)`, NLEV 1 `m KF abs(cd/m)^AF / (f W L Cox)`, NLEV 2/3
    /// `KF gm^2 / m / (f^AF W L Cox)`, with `cd` C's `MOS1cd` (channel
    /// current in the device frame minus the bulk-drain junction current)
    /// and the level's `W`, `L` and `Cox` ([`NoiseShape`]).
    fn noise(&self, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let (common, geometry) = (self.model.common(), &self.geometry);
        let (model_context, temperature) = L::noise_environment(context.model_context, geometry);
        let op = self.operating(&model_context)?;
        let (Some(family), Some(shape)) = (L::NOISE_FAMILY, self.model.noise_shape(geometry, &op))
        else {
            return Err(L::noise_refusal(self));
        };
        let e = self.small_signal_eval(&op, |node| context.voltage(node), model_context.gmin)?;
        let gm = e.gm;
        let sid = if common.nlev < 3 {
            2.0 / 3.0 * gm.abs()
        } else {
            let vgst = (if e.mode == 1. { e.vgs } else { e.vgd }) - e.von;
            if vgst > 0. {
                let alpha = if vgst <= e.vds * e.mode {
                    0.
                } else {
                    1.0 - (e.vds * e.mode / e.vdsat)
                };
                2.0 / 3.0 * shape.beta * vgst * (1.0 + alpha + alpha * alpha) / (1.0 + alpha)
                    * common.gdsnoi
            } else {
                0.
            }
        };
        let cd = e.cd;
        let m = geometry.m;
        let length = shape.length;
        let current_law = || m * common.kf * (common.af * (cd / m).abs().max(1e-38).ln()).exp();
        let (numerator, law) = if model_context.spice3_noise {
            (
                current_law(),
                FlickerLaw::Channel {
                    exponent: None,
                    factors: [shape.width, length, shape.cox_squared],
                },
            )
        } else {
            match common.nlev {
                0 => (
                    current_law(),
                    FlickerLaw::Channel {
                        exponent: None,
                        factors: [length, length, shape.cox],
                    },
                ),
                1 => (
                    current_law(),
                    FlickerLaw::Channel {
                        exponent: None,
                        factors: [shape.width, length, shape.cox],
                    },
                ),
                _ => (
                    common.kf * gm * gm / m,
                    FlickerLaw::Channel {
                        exponent: Some(common.af),
                        factors: [shape.width, length, shape.cox],
                    },
                ),
            }
        };
        let [d, _, s, _] = self.inner;
        let thermal = |conductance: Real| NoiseKind::Thermal {
            conductance,
            temperature,
        };
        Ok(DeviceNoise::Sources {
            family,
            model: Some(self.model_name.clone()),
            total: true,
            sources: vec![
                NoiseSource::new("_rd", [d, self.terminals[0]], thermal(self.series[0])),
                NoiseSource::new("_rs", [s, self.terminals[2]], thermal(self.series[1])),
                NoiseSource::new("_id", [d, s], thermal(sid)),
                NoiseSource::new("_1overf", [d, s], NoiseKind::Flicker { numerator, law }),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn junction(bottom: Real, sidewall: Real, mj: Real, mjsw: Real) -> Junction {
        Junction::new(
            1e-14,
            0.7,
            bottom,
            sidewall,
            mj,
            mjsw,
            0.8,
            0.5,
            ReverseLaw::Constant,
        )
    }

    #[test]
    fn junction_capacitance_is_the_charge_derivative_across_the_fc_boundary() {
        for j in [
            junction(1e-12, 0., 0.5, 0.5),
            junction(1e-12, 2e-13, 0.5, 0.33),
            junction(0., 3e-13, 0.4, 0.33),
        ] {
            // Bulk-drain and bulk-source polynomials (C associates them
            // differently above FC*PB).
            for drain in [true, false] {
                for v in [-3., -0.5, 0., 0.39, 0.4, 0.41, 0.7] {
                    let h = 1e-6;
                    let (_, c) = j.charge(v, drain);
                    let numerical =
                        (j.charge(v + h, drain).0 - j.charge(v - h, drain).0) / (2. * h);
                    assert!((numerical - c).abs() <= 1e-7 * c, "{v}: {numerical} {c}");
                }
                // Charge and capacitance are continuous at FC*PB.
                let edge = 0.5 * 0.8;
                let (below, above) = (j.charge(edge - 1e-12, drain), j.charge(edge, drain));
                assert!((below.0 - above.0).abs() <= 1e-9 * above.0.abs());
                assert!((below.1 - above.1).abs() <= 1e-9 * above.1);
            }
        }
    }

    #[test]
    fn cubic_reverse_law_matches_mos3load_and_joins_the_forward_law() {
        let mut j = junction(0., 0., 0.5, 0.5);
        j.reverse = ReverseLaw::Cubic;
        let vt = 0.0258;
        // mos3load.c below -3 vt.
        let v = -0.5;
        let (current, conductance) = j.current(v, vt, 0.);
        let arg = (3. * vt / (v * std::f64::consts::E)).powi(3);
        assert!((current + 1e-14 * (1. + arg)).abs() < 1e-28);
        assert!((conductance - 1e-14 * 3. * arg / v).abs() < 1e-30);
        // Continuous value and slope at -3 vt.
        let edge = -3. * vt;
        let (below, g_below) = j.current(edge, vt, 0.);
        let (above, g_above) = j.current(edge + 1e-9, vt, 0.);
        assert!(
            (below - above).abs() <= 1e-8 * j.saturation,
            "{below} {above}"
        );
        assert!(
            (g_below - g_above).abs() <= 1e-6 * g_above.abs(),
            "{g_below} {g_above}"
        );
    }

    #[test]
    fn meyer_half_capacitances_follow_the_c_regions() {
        let (phi, cox) = (0.6, 3e-14);
        // Accumulation: all gate capacitance to bulk.
        assert_eq!(meyer(-2., -2., 0.5, 0., phi, cox), [0., 0., cox / 2.]);
        // Depletion: linear fall of the gate-bulk share.
        let [gs, gd, gb] = meyer(0.5 - 0.4, 0.1, 0.5, 0., phi, cox);
        assert_eq!([gs, gd], [0., 0.]);
        assert!((gb - 0.4 * cox / (2. * phi)).abs() < 1e-30);
        // Saturation: two thirds of Cox to the source (half stored).
        assert_eq!(meyer(2., 0.5, 0.5, 1.5, phi, cox), [cox / 3., 0., 0.]);
        // Zero vds in the linear region: the channel splits evenly,
        // Cox/2 each, so the halves are Cox/4.
        let [gs, gd, gb] = meyer(2., 2., 0.5, 1.5, phi, cox);
        assert!((gs - cox / 4.).abs() < 1e-28 && (gd - cox / 4.).abs() < 1e-28);
        assert_eq!(gb, 0.);
        // Continuity of the gate-source half at the depletion/inversion edge
        // (vgst = 0) in saturation.
        let below = meyer(0.5 - 1e-12, -1., 0.5, 0., phi, cox);
        let above = meyer(0.5 + 1e-12, -1., 0.5, 1e-12, phi, cox);
        assert!((below[0] - above[0]).abs() < 1e-24, "{below:?} {above:?}");
    }
}
