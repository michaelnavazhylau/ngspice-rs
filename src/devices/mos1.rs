//! Shichman-Hodges MOS level 1 (`mos1`).
//!
//! C references, read as behaviour only:
//!
//! - `mos1/mos1set.c` (`MOS1setup`): model/instance defaults and the internal
//!   drain/source nodes created for series resistance;
//! - `mos1/mos1temp.c` (`MOS1temp`): oxide capacitance, process extraction
//!   (TOX/UO/NSUB/TPG/NSS), temperature scaling of KP/PHI/VBI/IS/JS/PB and the
//!   junction capacitances, area/perimeter junction geometry and RD/RS/RSH;
//! - `mos1/mos1load.c` (`MOS1load`): square-law channel with drain/source
//!   reversal and body effect (forward body bias included), bulk junctions with
//!   bottom and sidewall depletion charge, and Meyer gate charge in C's
//!   state-averaging formulation;
//! - `devices/devsup.c` (`DEVqmeyer`): Meyer's half capacitances;
//! - `mos1/mos1acld.c` (`MOS1acLoad`) and `mos1/mos1trun.c` (`MOS1trunc`).
//!
//! The load reproduces `mos1load.c`'s arithmetic operation by operation
//! (voltage selection per `MODEINIT*` phase with `DEVpred`, `DEVfetlim`/
//! `DEVlimvds`/`DEVpnjlim` limiting and `Check`, junction currents, the
//! channel, depletion and Meyer charges with `NIintegrate`, and the stamps
//! in C's order); `MOS1convTest` and `MOS1acLoad` likewise. The state vector
//! holds C's `MOS1numStates` slots plus the instance values C keeps between
//! loads.
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
//! - C warns and continues for `L - 2 LD <= 0`, a nonpositive temperature-
//!   adjusted PHI/PB, or RSH with zero drain squares (an infinite conductance).
//!   Here they are explicit errors.
//!
//! Instance `off` and initial conditions follow `mos1load.c`: `MODEINITJCT`
//! starts at the `IC` vector (`type * ICVDS/ICVGS/ICVBS`) whenever one
//! component is nonzero, also without `uic`, and at the default start
//! otherwise (except in the `uic` initial load, which keeps all-zero
//! conditions); unset components are taken from the external terminals
//! under `uic` (`mos1ic.c`) and are zero otherwise. An `off` instance starts
//! at zero and is held there through `MODEINITFIX`. The `.noise` generators
//! of `mos1noi.c` (RD/RS thermal noise, the NLEV channel thermal noise and
//! the NLEV flicker laws with KF/AF/GDSNOI) are ported through
//! [`Device::noise`]; C's SPICE3-compatibility flicker form is not
//! selectable (no compatibility mode is ported).

mod disto;

use crate::devices::limiting;
use crate::devices::noise::{DeviceNoise, NoiseContext, NoiseFamily, NoiseKind, NoiseSource};
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::devices::{
    Device, DeviceState, IterationPhase, LinearContext, ModelContext, ModelFamily, ResolvedModel,
    StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// C `CONSTboltz` (J/K).
const BOLTZMANN: Real = 1.38064852e-23;
/// C `CHARGE` (C).
const CHARGE: Real = 1.6021766208e-19;
/// C `CONSTKoverQ`.
const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
const CELSIUS_TO_KELVIN: Real = 273.15;
/// C `REFTEMP` (27 degrees Celsius).
const REFTEMP: Real = 27. + CELSIUS_TO_KELVIN;
/// Vacuum permittivity as written in `mos1temp.c` (F/m).
const EPSILON_0: Real = 8.854214871e-12;
/// Intrinsic carrier density of silicon used by `mos1temp.c` (m^-3).
const INTRINSIC_DENSITY: Real = 1.45e16;
/// C `MAX_EXP_ARG`: junction exponent clamp of `mos1load.c`.
const MAX_EXP_ARG: Real = 709.;
/// `DEVqmeyer`'s lower bound on the saturation voltage.
const MEYER_MIN_VDSAT: Real = 0.025;

/// Slots of the state vector: C's `MOS1numStates` layout (reordered so that
/// every charge is directly followed by its derivative, as
/// [`junction_companion`] expects), followed by the instance quantities C
/// keeps in `MOS1instance` between loads. All values are device-frame
/// (multiplied by the type), as in C.
mod slot {
    /// C `MOS1qbd` (`MOS1cqbd` follows).
    pub(super) const QBD: usize = 0;
    /// C `MOS1qbs` (`MOS1cqbs` follows).
    pub(super) const QBS: usize = 2;
    /// C `MOS1qgs`, `MOS1qgd`, `MOS1qgb` (each followed by its current).
    pub(super) const QG: [usize; 3] = [4, 6, 8];
    /// C `MOS1vbs`, `MOS1vbd`, `MOS1vgs`, `MOS1vds`.
    pub(super) const VBS: usize = 10;
    pub(super) const VBD: usize = 11;
    pub(super) const VGS: usize = 12;
    pub(super) const VDS: usize = 13;
    /// C `MOS1capgs`, `MOS1capgd`, `MOS1capgb` (`DEVqmeyer` halves).
    pub(super) const CAPG: [usize; 3] = [14, 15, 16];
    /// C instance values: `MOS1von`, `MOS1vdsat`, `MOS1mode`, `MOS1cd`,
    /// `MOS1cbs`, `MOS1cbd`, `MOS1gm`, `MOS1gds`, `MOS1gmbs`, `MOS1gbd`,
    /// `MOS1gbs`, `MOS1capbd`, `MOS1capbs`.
    pub(super) const VON: usize = 17;
    pub(super) const VDSAT: usize = 18;
    pub(super) const MODE: usize = 19;
    pub(super) const CD: usize = 20;
    pub(super) const CBS: usize = 21;
    pub(super) const CBD: usize = 22;
    pub(super) const GM: usize = 23;
    pub(super) const GDS: usize = 24;
    pub(super) const GMBS: usize = 25;
    pub(super) const GBD: usize = 26;
    pub(super) const GBS: usize = 27;
    pub(super) const CAPBD: usize = 28;
    pub(super) const CAPBS: usize = 29;
    pub(super) const COUNT: usize = 30;
}

const fn p(name: &'static str, unit: U, domain: D, default: Option<Real>) -> P {
    P {
        name,
        unit,
        domain,
        default,
    }
}

/// Model setters. Parameters whose *presence* changes C's derivations
/// (`MOS1...Given`) have no schema default.
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("kp", U::AmperePerVoltSquared, D::Positive, None),
    p("gamma", U::SquareRootVolt, D::NonNegative, None),
    p("phi", U::Volt, D::Positive, None),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.)),
    p("rd", U::Ohm, D::NonNegative, None),
    p("rs", U::Ohm, D::NonNegative, None),
    p("rsh", U::Ohm, D::NonNegative, None),
    p("cbd", U::Farad, D::NonNegative, None),
    p("cbs", U::Farad, D::NonNegative, None),
    p("is", U::Ampere, D::Positive, Some(1e-14)),
    p("js", U::AmperePerSquareMetre, D::NonNegative, Some(0.)),
    p("pb", U::Volt, D::Positive, Some(0.8)),
    p("cgso", U::FaradPerMetre, D::NonNegative, Some(0.)),
    p("cgdo", U::FaradPerMetre, D::NonNegative, Some(0.)),
    p("cgbo", U::FaradPerMetre, D::NonNegative, Some(0.)),
    p("cj", U::FaradPerSquareMetre, D::NonNegative, None),
    p("mj", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("cjsw", U::FaradPerMetre, D::NonNegative, None),
    p("mjsw", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("tox", U::Metre, D::NonNegative, None),
    p("ld", U::Metre, D::NonNegative, Some(0.)),
    p("u0", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("uo", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("nsub", U::PerCubicCentimetre, D::Positive, None),
    p("tpg", U::Dimensionless, D::Finite, None),
    p("nss", U::PerSquareCentimetre, D::Finite, None),
    p("tnom", U::Celsius, D::Temperature, None),
    // mos1noi.c; mos1set.c defaults KF = 0, AF = 1, NLEV = 2, GDSNOI = 1.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
    p("nlev", U::Dimensionless, D::Finite, Some(2.)),
    p("gdsnoi", U::Dimensionless, D::Finite, Some(1.)),
];

/// Instance setters (`mos1par.c`). Defaults are C's `CKTdefaultMos*` values,
/// whose `.options defl/defw/defad/defas/defm` are rejected by the options
/// layer.
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

fn required(values: &ScalarValues, name: &str) -> SpiceResult<Real> {
    values
        .get(name)
        .map(|v| v.value)
        .ok_or_else(|| SpiceError::circuit(format!("missing MOS1 schema default {name}")))
}

/// The last-set value of a C parameter with two spellings (`IOPR` aliases
/// such as `vto`/`vt0`, `u0`/`uo`): setters apply in card order.
fn aliased(values: &ScalarValues, names: [&str; 2]) -> Option<Real> {
    names
        .iter()
        .filter_map(|name| values.get(name))
        .max_by_key(|v| v.location.as_ref().map(|l| (l.line, l.column)))
        .map(|v| v.value)
}

/// Validated model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
struct Model {
    pol: Real,
    vto: Option<Real>,
    kp: Option<Real>,
    gamma: Option<Real>,
    phi: Option<Real>,
    lambda: Real,
    rd: Option<Real>,
    rs: Option<Real>,
    rsh: Option<Real>,
    cbd: Option<Real>,
    cbs: Option<Real>,
    is: Real,
    js: Real,
    pb: Real,
    overlap: [Real; 3],
    cj: Option<Real>,
    mj: Real,
    cjsw: Option<Real>,
    mjsw: Real,
    fc: Real,
    tox: Option<Real>,
    ld: Real,
    u0: Option<Real>,
    nsub: Option<Real>,
    tpg: Option<Real>,
    nss: Option<Real>,
    tnom: Option<Real>,
    /// Flicker coefficient/exponent, noise model selector (0..=3) and channel
    /// thermal-noise coefficient (`mos1noi.c`).
    kf: Real,
    af: Real,
    nlev: u8,
    gdsnoi: Real,
}

/// Validated instance geometry.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    m: Real,
    w: Real,
    /// Effective channel length `L - 2 LD`.
    length: Real,
    ad: Real,
    as_: Real,
    pd: Real,
    ps: Real,
    nrd: Real,
    nrs: Real,
    temp: Option<Real>,
    dtemp: Real,
}

/// Band gap of silicon at `kelvin` (`mos1temp.c`).
fn band_gap(kelvin: Real) -> Real {
    1.16 - (7.02e-4 * kelvin * kelvin) / (kelvin + 1108.)
}

/// The `pbfact` potential correction of `mos1temp.c` at `kelvin`.
fn potential_factor(kelvin: Real) -> Real {
    let vt = kelvin * K_OVER_Q;
    let kt = BOLTZMANN * kelvin;
    let arg = -band_gap(kelvin) / (kt + kt) + 1.1150877 / (BOLTZMANN * (REFTEMP + REFTEMP));
    -2. * vt * (1.5 * (kelvin / REFTEMP).ln() + CHARGE * arg)
}

/// Process-derived model values (`mos1temp.c`, model loop).
#[derive(Debug, Clone, Copy)]
struct Process {
    /// Oxide capacitance per area, zero when TOX is absent or zero.
    cox: Real,
    kp: Real,
    phi: Real,
    gamma: Real,
    vto: Real,
}

impl Model {
    fn process(&self, tnom: Real) -> SpiceResult<Process> {
        let mut process = Process {
            cox: 0.,
            kp: self.kp.unwrap_or(2e-5),
            phi: self.phi.unwrap_or(0.6),
            gamma: self.gamma.unwrap_or(0.),
            vto: self.vto.unwrap_or(0.),
        };
        let Some(tox) = self.tox.filter(|tox| *tox != 0.) else {
            return Ok(process);
        };
        let cox = 3.9 * EPSILON_0 / tox;
        process.cox = cox;
        if self.kp.is_none() {
            process.kp = self.u0.unwrap_or(600.) * cox * 1e-4;
        }
        if let Some(nsub) = self.nsub {
            let density = nsub * 1e6;
            if density <= INTRINSIC_DENSITY {
                return Err(SpiceError::circuit(format!(
                    "MOS1 NSUB={nsub} is below the intrinsic density (mos1temp.c: Nsub < Ni)"
                )));
            }
            let vtnom = tnom * K_OVER_Q;
            let egfet1 = band_gap(tnom);
            if self.phi.is_none() {
                process.phi = (2. * vtnom * (density / INTRINSIC_DENSITY).ln()).max(0.1);
            }
            let fermis = self.pol * 0.5 * process.phi;
            let tpg = self.tpg.unwrap_or(1.);
            let wkfng = if tpg == 0. {
                3.2
            } else {
                let fermig = self.pol * tpg * 0.5 * egfet1;
                3.25 + 0.5 * egfet1 - fermig
            };
            let wkfngs = wkfng - (3.25 + 0.5 * egfet1 + fermis);
            if self.gamma.is_none() {
                // C's left-to-right product, `nsub * 1e6` last.
                process.gamma = (2. * 11.70 * EPSILON_0 * CHARGE * nsub * 1e6).sqrt() / cox;
            }
            if self.vto.is_none() {
                let vfb = wkfngs - self.nss.unwrap_or(0.) * 1e4 * CHARGE / cox;
                process.vto = vfb + self.pol * (process.gamma * process.phi.sqrt() + process.phi);
            }
        }
        Ok(process)
    }
}

/// One side's bulk junction at the device temperature (`mos1temp.c`), in
/// C's instance quantities.
#[derive(Debug, Clone, Copy)]
struct Junction {
    /// C `DrainSatCur`/`SourceSatCur` of `mos1load.c` (M included).
    saturation: Real,
    /// C `MOS1drainVcrit`/`MOS1sourceVcrit`.
    vcrit: Real,
    /// C `MOS1Cbd`/`MOS1Cbs` (bottom) and `MOS1Cbdsw`/`MOS1Cbssw`.
    bottom: Real,
    sidewall: Real,
    mj: Real,
    mjsw: Real,
    /// C `MOS1tBulkPot`.
    potential: Real,
    fc: Real,
    /// C `MOS1tDepCap`.
    depletion: Real,
    /// C `MOS1f2d/f3d/f4d` (or the `s` set).
    f2: Real,
    f3: Real,
    f4: Real,
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
    /// `mos1temp.c`: the junction of one side from its zero-bias bottom and
    /// sidewall capacitances, with the `f2/f3/f4` linear continuation above
    /// `FC * PB` in C's arithmetic.
    #[allow(clippy::too_many_arguments)]
    fn new(
        saturation: Real,
        vcrit: Real,
        bottom: Real,
        sidewall: Real,
        mj: Real,
        mjsw: Real,
        potential: Real,
        fc: Real,
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
        }
    }

    /// Depletion charge and capacitance at the device-frame bulk voltage `v`
    /// (`mos1load.c`). Above `FC * PB` C writes the bulk-drain and
    /// bulk-source polynomials with different association, kept here.
    fn charge(&self, v: Real, drain: bool) -> (Real, Real) {
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

    /// The ideal junction current and conductance with `gmin` at the
    /// device-frame voltage `v` (`mos1load.c`).
    fn current(&self, v: Real, vt: Real, gmin: Real) -> (Real, Real) {
        if v <= -3. * vt {
            (gmin * v - self.saturation, gmin)
        } else {
            let e = (v / vt).min(MAX_EXP_ARG).exp();
            (
                self.saturation * (e - 1.) + gmin * v,
                self.saturation * e / vt + gmin,
            )
        }
    }
}

/// Every temperature- and geometry-dependent quantity of one load.
#[derive(Debug, Clone, Copy)]
struct Operating {
    vt: Real,
    /// C `Beta`.
    beta: Real,
    /// C `MOS1tVbi`.
    vbi: Real,
    /// C `MOS1tPhi`.
    phi: Real,
    /// C `MOS1tVto`.
    vto: Real,
    gamma: Real,
    lambda: Real,
    /// C `OxideCap`: `cox * Leff * m * w`.
    oxide: Real,
    drain: Junction,
    source: Junction,
    /// Gate-source, gate-drain and gate-bulk overlap capacitances.
    overlap: [Real; 3],
}

/// `DEVqmeyer` (`devsup.c`): half of the bias-dependent gate-source,
/// gate-drain and gate-bulk capacitances `[capgs, capgd, capgb]`, from
/// device-frame voltages, in C's arithmetic.
fn meyer(vgs: Real, vgd: Real, von: Real, vdsat: Real, phi: Real, cox: Real) -> [Real; 3] {
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
struct Eval {
    vbs: Real,
    vbd: Real,
    vgs: Real,
    vds: Real,
    vgd: Real,
    vgb: Real,
    /// C `MOS1mode` (1 normal, -1 inverse).
    mode: Real,
    cdrain: Real,
    gm: Real,
    gds: Real,
    gmbs: Real,
    gbs: Real,
    cbs: Real,
    gbd: Real,
    cbd: Real,
    /// C `MOS1cd`: `mode * cdrain - cbd`.
    cd: Real,
    /// `mos1load.c`'s local (device-frame) `von` and `vdsat`.
    von: Real,
    vdsat: Real,
    qbs: Real,
    capbs: Real,
    qbd: Real,
    capbd: Real,
    /// `DEVqmeyer`'s `[capgs, capgd, capgb]` (C `CKTstate0` `MOS1capgs`...).
    half: [Real; 3],
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

/// MOS level 1 with series resistance, junction geometry, Meyer gate charge
/// and temperature scaling.
#[derive(Debug)]
pub struct Mos1 {
    name: String,
    /// The model card's name, for C's `.noise` visiting order.
    model_name: String,
    /// External d, g, s, b followed by any internal drain/source nodes.
    terminals: Vec<NodeId>,
    /// d', g, s', b: the nodes the intrinsic device sees.
    inner: [NodeId; 4],
    model: Model,
    geometry: Geometry,
    /// Drain/source series conductances (zero without an internal node).
    series: [Real; 2],
    /// `OFF` and `ICVDS`/`ICVGS`/`ICVBS` (C `MOS1off`, `MOS1icV*`).
    initial: crate::devices::initial::InstanceInitial,
}

impl Mos1 {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 1 || i.nodes.len() != 4 {
            return Err(SpiceError::circuit("MOS1 needs level 1 and four terminals"));
        }
        let (initial, setters) = crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)?;
        let m = resolved.parameters(&ScalarSchema { parameters: MODEL })?;
        let v = ScalarSchema {
            parameters: INSTANCE,
        }
        .validate(&setters, &i.location)?;
        let pol = if resolved.family() == ModelFamily::Pmos {
            -1.
        } else {
            1.
        };
        let get = |name: &str| m.get(name).map(|v| v.value);
        let model = Model {
            pol,
            vto: aliased(&m, ["vto", "vt0"]),
            kp: get("kp"),
            gamma: get("gamma"),
            phi: get("phi"),
            lambda: required(&m, "lambda")?,
            rd: get("rd"),
            rs: get("rs"),
            rsh: get("rsh"),
            cbd: get("cbd"),
            cbs: get("cbs"),
            is: required(&m, "is")?,
            js: required(&m, "js")?,
            pb: required(&m, "pb")?,
            overlap: [
                required(&m, "cgso")?,
                required(&m, "cgdo")?,
                required(&m, "cgbo")?,
            ],
            cj: get("cj"),
            mj: required(&m, "mj")?,
            cjsw: get("cjsw"),
            mjsw: required(&m, "mjsw")?,
            fc: required(&m, "fc")?,
            tox: get("tox"),
            ld: required(&m, "ld")?,
            u0: aliased(&m, ["u0", "uo"]),
            nsub: get("nsub"),
            tpg: get("tpg"),
            nss: get("nss"),
            tnom: get("tnom"),
            kf: required(&m, "kf")?,
            af: required(&m, "af")?,
            nlev: {
                // IF_INTEGER setter: floor(value + 0.5). C's switch has no
                // case outside 0..=3 (the flicker density is then the bare
                // gain), which the port rejects.
                let raw = required(&m, "nlev")?;
                let rounded = (raw + 0.5).floor();
                if !(0. ..=3.).contains(&rounded) {
                    return Err(SpiceError::circuit(format!(
                        "MOS1 NLEV must round to 0..=3 (mos1noi.c), got {raw}"
                    )));
                }
                rounded as u8
            },
            gdsnoi: required(&m, "gdsnoi")?,
        };
        if model.tpg.is_some_and(|tpg| tpg.fract() != 0.) {
            return Err(SpiceError::circuit("MOS1 TPG must be an integer"));
        }
        if model.mj >= 1. || model.mjsw >= 1. || model.fc >= 1. {
            return Err(SpiceError::circuit("MOS1 requires MJ, MJSW and FC below 1"));
        }
        let geometry = Geometry {
            m: required(&v, "m")?,
            w: required(&v, "w")?,
            length: required(&v, "l")? - 2. * model.ld,
            ad: required(&v, "ad")?,
            as_: required(&v, "as")?,
            pd: required(&v, "pd")?,
            ps: required(&v, "ps")?,
            nrd: required(&v, "nrd")?,
            nrs: required(&v, "nrs")?,
            temp: v.get("temp").map(|v| v.value),
            dtemp: required(&v, "dtemp")?,
        };
        if geometry.length <= 0. || !geometry.length.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{}: MOS1 effective channel length L - 2*LD must be positive",
                i.name
            )));
        }
        let series = [
            series_conductance(model.rd, model.rsh, geometry.nrd, geometry.m, "drain")?,
            series_conductance(model.rs, model.rsh, geometry.nrs, geometry.m, "source")?,
        ];
        let mut device = Self {
            name: i.name.clone(),
            model_name: resolved.card().name.clone(),
            terminals: vec![],
            inner: [NodeId::GROUND; 4],
            model,
            geometry,
            series,
            initial,
        };
        // Validate every derivation before interning nodes.
        device.operating(context)?;
        let mut staged = nodes.clone();
        let external: Vec<NodeId> = i.nodes.iter().map(|n| staged.intern(n)).collect();
        device.terminals.clone_from(&external);
        device.inner = [external[0], external[1], external[2], external[3]];
        for (index, conductance, suffix) in [(0, series[0], "drain"), (2, series[1], "source")] {
            if conductance > 0. {
                let name = format!("{}#{suffix}", i.name);
                if staged.get(&name).is_some() {
                    return Err(SpiceError::circuit(format!(
                        "MOS1 internal-node name collision: {name}"
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

    /// The temperature-, process- and geometry-dependent values of one load
    /// (`mos1temp.c` and the start of `mos1load.c`), in C's arithmetic.
    fn operating(&self, context: &ModelContext) -> SpiceResult<Operating> {
        let (model, geometry) = (&self.model, &self.geometry);
        let tnom = model.tnom.unwrap_or(context.nominal_temperature) + CELSIUS_TO_KELVIN;
        // C: MOS1temp = CKTtemp + MOS1dtemp, with CKTtemp in kelvin.
        let temp = match geometry.temp {
            Some(t) => t + CELSIUS_TO_KELVIN,
            None => (context.temperature + CELSIUS_TO_KELVIN) + geometry.dtemp,
        };
        if !(tnom.is_finite() && tnom > 0. && temp.is_finite() && temp > 0.) {
            return Err(SpiceError::circuit(format!(
                "{}: invalid MOS1 temperature",
                self.name
            )));
        }
        let process = model.process(tnom)?;
        let vt = temp * K_OVER_Q;
        let vtnom = tnom * K_OVER_Q;
        let (egfet, egfet1) = (band_gap(temp), band_gap(tnom));
        let (fact1, fact2) = (tnom / REFTEMP, temp / REFTEMP);
        let (pbfact, pbfact1) = (potential_factor(temp), potential_factor(tnom));
        let ratio = temp / tnom;
        let kp = process.kp / (ratio * ratio.sqrt());
        let phio = (process.phi - pbfact1) / fact1;
        let phi = fact2 * phio + pbfact;
        let vbi = process.vto - model.pol * (process.gamma * process.phi.sqrt())
            + 0.5 * (egfet1 - egfet)
            + model.pol * 0.5 * (phi - process.phi);
        let vto = vbi + model.pol * process.gamma * phi.sqrt();
        let scale = (-egfet / vt + egfet1 / vtnom).exp();
        let (saturation, density) = (model.is * scale, model.js * scale);
        let pbo = (model.pb - pbfact1) / fact1;
        let gmaold = (model.pb - pbo) / pbo;
        let potential = fact2 * pbo + pbfact;
        let gmanew = (potential - pbo) / pbo;
        // mos1temp.c: divide out the nominal-temperature factor, then
        // multiply by the device-temperature one, as two steps.
        let nominal = |grading: Real| 1. / (1. + grading * (4e-4 * (tnom - REFTEMP) - gmaold));
        let actual = |grading: Real| 1. + grading * (4e-4 * (temp - REFTEMP) - gmanew);
        let (bottom, side) = (
            [nominal(model.mj), actual(model.mj)],
            [nominal(model.mjsw), actual(model.mjsw)],
        );
        let m = geometry.m;
        // MOS1tCbd/MOS1tCbs/MOS1tCj and MOS1tCjsw.
        let scaled = |c: Real, factor: [Real; 2]| c * factor[0] * factor[1];
        let bottom_cap = |given: Option<Real>, area: Real| match (given, model.cj) {
            (Some(c), _) => scaled(c, bottom) * m,
            (None, Some(cj)) => scaled(cj, bottom) * m * area,
            (None, None) => 0.,
        };
        let side_cap = |perimeter: Real| model.cjsw.map_or(0., |c| scaled(c, side) * perimeter * m);
        let root2 = std::f64::consts::SQRT_2;
        let (sat_d, sat_s, vcrit_d, vcrit_s) =
            if density == 0. || geometry.ad == 0. || geometry.as_ == 0. {
                let vcrit = vt * (vt / (root2 * m * saturation)).ln();
                (m * saturation, m * saturation, vcrit, vcrit)
            } else {
                (
                    density * m * geometry.ad,
                    density * m * geometry.as_,
                    vt * (vt / (root2 * m * density * geometry.ad)).ln(),
                    vt * (vt / (root2 * m * density * geometry.as_)).ln(),
                )
            };
        let junction = |saturation, vcrit, bottom, sidewall| {
            Junction::new(
                saturation, vcrit, bottom, sidewall, model.mj, model.mjsw, potential, model.fc,
            )
        };
        let operating = Operating {
            vt,
            beta: kp * m * geometry.w / geometry.length,
            vbi,
            phi,
            vto,
            gamma: process.gamma,
            lambda: model.lambda,
            oxide: process.cox * geometry.length * m * geometry.w,
            drain: junction(
                sat_d,
                vcrit_d,
                bottom_cap(model.cbd, geometry.ad),
                side_cap(geometry.pd),
            ),
            source: junction(
                sat_s,
                vcrit_s,
                bottom_cap(model.cbs, geometry.as_),
                side_cap(geometry.ps),
            ),
            overlap: [
                model.overlap[0] * m * geometry.w,
                model.overlap[1] * m * geometry.w,
                model.overlap[2] * m * geometry.length,
            ],
        };
        let finite = [
            operating.vt,
            operating.beta,
            operating.vbi,
            operating.vto,
            operating.gamma,
            operating.oxide,
            operating.drain.saturation,
            operating.source.saturation,
            operating.drain.vcrit,
            operating.source.vcrit,
            operating.drain.bottom,
            operating.drain.sidewall,
            operating.source.bottom,
            operating.source.sidewall,
            operating.drain.f4,
            operating.source.f4,
            operating.overlap[0],
            operating.overlap[1],
            operating.overlap[2],
        ]
        .iter()
        .all(|v| v.is_finite());
        if !(finite && phi.is_finite() && phi > 0. && potential.is_finite() && potential > 0.)
            || operating.beta <= 0.
            || operating.drain.saturation <= 0.
            || operating.source.saturation <= 0.
            || bottom
                .iter()
                .chain(&side)
                .any(|f| !f.is_finite() || *f < 0.)
        {
            return Err(SpiceError::circuit(format!(
                "{}: MOS1 parameters are out of range at {:.2} K \
                 (nonfinite value, or nonpositive PHI/PB/KP/IS after temperature scaling)",
                self.name, temp
            )));
        }
        Ok(operating)
    }

    /// `mos1load.c` from `vbd = vbs - vds` on, at the device-frame
    /// `[vbs, vgs, vds]`: junction currents with `gmin`, the Shichman-Hodges
    /// channel, the depletion charges and `DEVqmeyer`, before any charge
    /// integration. Every expression keeps C's operation order.
    fn evaluate(
        &self,
        op: &Operating,
        [vbs, vgs, vds]: [Real; 3],
        gmin: Real,
    ) -> SpiceResult<Eval> {
        let pol = self.model.pol;
        let vt = op.vt;
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgb = vgs - vbs;
        let (cbs, gbs) = op.source.current(vbs, vt, gmin);
        let (cbd, gbd) = op.drain.current(vbd, vt, gmin);
        let mode: Real = if vds >= 0. { 1. } else { -1. };
        let normal = mode == 1.;
        let vbx = if normal { vbs } else { vbd };
        let sarg = if vbx <= 0. {
            (op.phi - vbx).sqrt()
        } else {
            let root = op.phi.sqrt();
            let sarg = root - vbx / (root + root);
            // MAX(0, sarg)
            if 0. > sarg { 0. } else { sarg }
        };
        let von = (op.vbi * pol) + op.gamma * sarg;
        let vgst = (if normal { vgs } else { vgd }) - von;
        let vdsat = if vgst > 0. { vgst } else { 0. };
        let arg = if sarg <= 0. {
            0.
        } else {
            op.gamma / (sarg + sarg)
        };
        let vdsm = vds * mode;
        let (cdrain, gm, gds, gmbs) = if vgst <= 0. {
            (0., 0., 0., 0.)
        } else {
            let betap = op.beta * (1. + op.lambda * vdsm);
            if vgst <= vdsm {
                let gm = betap * vgst;
                (
                    betap * vgst * vgst * 0.5,
                    gm,
                    op.lambda * op.beta * vgst * vgst * 0.5,
                    gm * arg,
                )
            } else {
                let gm = betap * vdsm;
                (
                    betap * vdsm * (vgst - 0.5 * vdsm),
                    gm,
                    betap * (vgst - vdsm) + op.lambda * op.beta * vdsm * (vgst - 0.5 * vdsm),
                    gm * arg,
                )
            }
        };
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
                context: format!("MOS1 {}", self.name),
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
        op: &Operating,
        raw: [Real; 3],
        old: [Real; 4],
        here_von: Real,
    ) -> ([Real; 3], bool) {
        let [mut vbs, mut vgs, mut vds] = raw;
        let [old_vbs, old_vbd, old_vgs, old_vds] = old;
        let mut vbd = vbs - vds;
        let mut vgd = vgs - vds;
        let vgdo = old_vgs - old_vds;
        let von = self.model.pol * here_von;
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
        op: &Operating,
        states: &DeviceState<'_>,
        raw: [Real; 3],
        ic: [Real; 3],
        xfact: Real,
    ) -> ([Real; 3], Start, bool) {
        let pol = self.model.pol;
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
    /// load's `CKTstate0` there; the small-signal contexts supply the
    /// accepted `CKTstate1` instead, so the bias is evaluated unlimited,
    /// which equals C whenever the converged load itself limited nothing.
    fn small_signal_eval(
        &self,
        op: &Operating,
        voltage: impl Fn(NodeId) -> Real,
        gmin: Real,
    ) -> SpiceResult<Eval> {
        let pol = self.model.pol;
        let [dp, g, sp, b] = self.inner.map(&voltage);
        self.evaluate(op, [pol * (b - sp), pol * (g - sp), pol * (dp - sp)], gmin)
    }
}

/// `mos1temp.c`: RD/RS take precedence over RSH times the number of squares.
/// `mos1set.c` creates the internal node exactly when the conductance is
/// nonzero, except for combinations C leaves singular or infinite, which are
/// rejected here.
fn series_conductance(
    resistance: Option<Real>,
    sheet: Option<Real>,
    squares: Real,
    m: Real,
    side: &str,
) -> SpiceResult<Real> {
    let sheet_node = sheet.is_some_and(|rsh| rsh != 0.) && squares != 0.;
    let conductance = match (resistance, sheet) {
        (Some(r), _) if r != 0. => m / r,
        (Some(_), _) => {
            if sheet_node {
                return Err(SpiceError::circuit(format!(
                    "MOS1 RSH creates an internal {side} node that an explicit zero \
                     resistance leaves floating"
                )));
            }
            0.
        }
        (None, Some(rsh)) if rsh != 0. => {
            if squares == 0. {
                return Err(SpiceError::circuit(format!(
                    "MOS1 RSH with zero {side} squares gives C an infinite conductance"
                )));
            }
            m / (rsh * squares)
        }
        _ => 0.,
    };
    if !conductance.is_finite() {
        return Err(SpiceError::circuit(format!(
            "MOS1 {side} series conductance overflow"
        )));
    }
    Ok(conductance)
}

impl Device for Mos1 {
    /// `mos1set.c`.
    fn c_device_type(&self) -> crate::devices::setup::CDeviceType {
        crate::devices::setup::CDeviceType::Mos1
    }

    /// `mos1set.c`.
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
    /// `mos1trun.c`: only the three Meyer gate charges control the timestep;
    /// the bulk junction charges do not.
    fn truncation_slots(&self) -> Vec<usize> {
        slot::QG.to_vec()
    }
    /// `mos1.c` `MOS1pTable`: M, L, W, AD, AS, PD, PS, NRD, NRS, TEMP and
    /// DTEMP, which `mos1temp.c`/`mos1load.c` re-derive (`dctrcurv.c`
    /// `DCTsetInstParam`).
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
                geometry.length = value - 2. * self.model.ld;
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
                    "{}: MOS1 parameter {parameter} cannot be swept",
                    self.name
                )));
            }
        }
        if geometry.length <= 0. || !geometry.length.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{}: MOS1 effective channel length L - 2*LD must be positive",
                self.name
            )));
        }
        let series = [
            series_conductance(
                self.model.rd,
                self.model.rsh,
                geometry.nrd,
                geometry.m,
                "drain",
            )?,
            series_conductance(
                self.model.rs,
                self.model.rsh,
                geometry.nrs,
                geometry.m,
                "source",
            )?,
        ];
        if series
            .iter()
            .zip(self.series)
            .any(|(new, old)| (*new > 0.) != (old > 0.))
        {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    "{}: swept {parameter}={value} would add or remove a MOS1 internal \
                     drain/source node",
                    self.name
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
        };
        device.operating(context)?;
        Ok(Box::new(device))
    }
    /// `MOS1load` (`mos1load.c`, without bypass, which ngspice disables by
    /// default, and without `.sens` perturbation).
    #[allow(clippy::needless_range_loop)] // C's three gate charges, side by side.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("MOS1 AC needs small-signal assembly"));
        }
        let op = self.operating(&context.model_context())?;
        let pol = self.model.pol;
        let [dpv, gv, spv, bv] = self.inner.map(|n| context.node_voltage(n));
        let raw = [pol * (bv - spv), pol * (gv - spv), pol * (dpv - spv)];
        let ic = self.start_conditions(context.states.initial_conditions(), |n| {
            context.node_voltage(n)
        });
        let integration = context.integration.cloned();
        let transient = integration.is_some();
        let phase = context.states.phase();
        // C `xfact = CKTdelta / CKTdeltaOld[1]`; one in a `.dc` sweep.
        let xfact = integration
            .as_ref()
            .map_or(1., crate::maths::integrator::Coefficients::predictor_ratio);
        let (v, start, check) = self.select(&op, &context.states, raw, ic, xfact);
        let mut e = self.evaluate(&op, v, context.gmin)?;
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
                SpiceError::circuit(format!("{}: missing accepted MOS1 state", self.name))
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
                    SpiceError::circuit(format!("{}: missing accepted MOS1 charge", self.name))
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
        for (row, col, value) in [
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
        ] {
            context.stamp(row, col, value)?;
        }
        Ok(())
    }
    /// `MOS1convTest` (`mos1conv.c`): the drain and bulk currents predicted
    /// linearly from the last load at the new iterate (`CKTrhs`) against
    /// their loaded values, in C's arithmetic.
    fn convergence_test(
        &self,
        context: &crate::devices::ConvergenceContext<'_>,
    ) -> SpiceResult<bool> {
        let at = |k: usize| context.states.get(k).copied().unwrap_or(0.);
        let pol = self.model.pol;
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

    /// `mos1acld.c`: the conductances of the `MODEINITSMSIG` load plus
    /// `j omega` times its junction capacitances and the Meyer capacitances
    /// `half + half + overlap`, stamped in C's order; each element's dynamic
    /// terms are one [`LinearContext::dynamic_sum`], as C adds
    /// `xgd + xgs + xgb` in one step.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let op = self.operating(context.model_context)?;
        let unknowns = context.unknowns;
        let voltage = |n: NodeId| unknowns.node_row(n).and_then(|r| bias.get(r)).unwrap_or(0.);
        let e = self.small_signal_eval(&op, voltage, context.model_context.gmin)?;
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

    /// Pole-zero load: C `mos1pzld.c` equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }

    /// `mos1dset.c`/`mos1dist.c` at the operating point (see the `disto`
    /// submodule for C's distortion model).
    fn distortion(
        &self,
        context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        self.distortion_terms(context)
    }

    /// `mos1noi.c` at the operating point: RD/RS thermal noise and the
    /// channel thermal noise `Sid` at the instance temperature, and the
    /// flicker law of NLEV between the internal drain and source. NLEV < 3
    /// uses `Sid = 2/3 abs(gm)`; NLEV 3 the `GDSNOI`-scaled region formula.
    /// Flicker: NLEV 0 `m KF abs(cd/m)^AF / (f Leff^2 Cox)`, NLEV 1
    /// `m KF abs(cd/m)^AF / (f W Leff Cox)`, NLEV 2/3
    /// `KF gm^2 / m / (f^AF W Leff Cox)`, with `cd` C's `MOS1cd` (channel
    /// current in the device frame minus the bulk-drain junction current) and
    /// `Cox` taken for `TOX = 1e-7 m` when the model has no oxide capacitance.
    fn noise(&self, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let (model, geometry) = (&self.model, &self.geometry);
        let op = self.operating(context.model_context)?;
        let e = self.small_signal_eval(
            &op,
            |node| context.voltage(node),
            context.model_context.gmin,
        )?;
        let gm = e.gm;
        let sid = if model.nlev < 3 {
            2.0 / 3.0 * gm.abs()
        } else {
            let vgst = (if e.mode == 1. { e.vgs } else { e.vgd }) - e.von;
            if vgst > 0. {
                let alpha = if vgst <= e.vds * e.mode {
                    0.
                } else {
                    1.0 - (e.vds * e.mode / e.vdsat)
                };
                2.0 / 3.0 * op.beta * vgst * (1.0 + alpha + alpha * alpha) / (1.0 + alpha)
                    * model.gdsnoi
            } else {
                0.
            }
        };
        let cd = e.cd;
        let cox = match model.tox.filter(|tox| *tox != 0.) {
            Some(tox) => 3.9 * EPSILON_0 / tox,
            None => 3.9 * 8.854214871e-12 / 1e-7,
        };
        let m = geometry.m;
        let length = geometry.length;
        let current_law = || m * model.kf * (model.af * (cd / m).abs().max(1e-38).ln()).exp();
        let (coefficient, exponent) = if context.model_context.spice3_noise {
            (current_law() / (geometry.w * length * cox * cox), 1.)
        } else {
            match model.nlev {
                0 => (current_law() / (length * length * cox), 1.),
                1 => (current_law() / (geometry.w * length * cox), 1.),
                _ => (
                    model.kf * gm * gm / m / (geometry.w * length * cox),
                    model.af,
                ),
            }
        };
        let temperature = geometry
            .temp
            .unwrap_or(context.model_context.temperature + geometry.dtemp)
            + CELSIUS_TO_KELVIN;
        let [d, _, s, _] = self.inner;
        let thermal = |conductance: Real| NoiseKind::Thermal {
            conductance,
            temperature,
        };
        Ok(DeviceNoise::Sources {
            family: NoiseFamily::Mos1,
            model: Some(self.model_name.clone()),
            total: true,
            sources: vec![
                NoiseSource::new("_rd", [d, self.terminals[0]], thermal(self.series[0])),
                NoiseSource::new("_rs", [s, self.terminals[2]], thermal(self.series[1])),
                NoiseSource::new("_id", [d, s], thermal(sid)),
                NoiseSource::new(
                    "_1overf",
                    [d, s],
                    NoiseKind::Flicker {
                        coefficient,
                        exponent,
                    },
                ),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn junction(bottom: Real, sidewall: Real, mj: Real, mjsw: Real) -> Junction {
        Junction::new(1e-14, 0.7, bottom, sidewall, mj, mjsw, 0.8, 0.5)
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
