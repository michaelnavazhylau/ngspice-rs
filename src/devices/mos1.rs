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
//! # Deliberate divergences
//!
//! - Newton limiting follows `mos1load.c` through [`crate::devices::limiting`]:
//!   `MODEINITJCT` starts at `vbs = -1`, `vgs = type * tVto`, `vds = 0`, and
//!   later loads apply `DEVfetlim`/`DEVlimvds`/`DEVpnjlim`. C's predictor
//!   extrapolation (`MODEINITPRED`/`MODEINITTRAN`) and bypass are not ported:
//!   a predicted load limits the iterate against the last accepted voltages.
//! - The body-effect transconductance is the exact derivative of the drain
//!   current. In forward body bias C stamps `gm * gamma / (2 sarg)`, which is
//!   not the derivative of its own `von`; the converged point is the same.
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

use crate::devices::limiting::{self, Limiter, Linearization};
use crate::devices::linear::nodal_stamp;
use crate::devices::noise::{DeviceNoise, NoiseContext, NoiseFamily, NoiseKind, NoiseSource};
use crate::devices::nonlinear::{JunctionPoint, stamp_junction};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::devices::{
    Device, LinearContext, ModelContext, ModelFamily, ResolvedModel, StampContext,
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

/// Slots of the state vector (C `MOS1numStates` layout, reordered so that
/// every charge is directly followed by its derivative).
mod slot {
    pub(super) const QBD: usize = 0;
    pub(super) const QBS: usize = 2;
    /// Gate-source, gate-drain and gate-bulk Meyer charges (and derivatives).
    pub(super) const QG: [usize; 3] = [4, 6, 8];
    /// Physical gate-source/gate-drain/gate-bulk voltages of the load.
    pub(super) const VG: [usize; 3] = [10, 11, 12];
    /// Meyer half capacitances of the load (C `MOS1capgs`, ...).
    pub(super) const HALF: [usize; 3] = [13, 14, 15];
    /// Limited `vbs`, `vgs`, `vds` and the load's `von` (C `MOS1vbs`,
    /// `MOS1vgs`, `MOS1vds`, `MOS1von`), for the next load's limiting.
    pub(super) const LIMITED: usize = 16;
    pub(super) const COUNT: usize = 20;
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
                process.gamma = (2. * 11.70 * EPSILON_0 * CHARGE * density).sqrt() / cox;
            }
            if self.vto.is_none() {
                let vfb = wkfngs - self.nss.unwrap_or(0.) * 1e4 * CHARGE / cox;
                process.vto = vfb + self.pol * (process.gamma * process.phi.sqrt() + process.phi);
            }
        }
        Ok(process)
    }
}

/// One side's bulk junction: zero-bias bottom/sidewall capacitances at the
/// device temperature and the temperature-adjusted potential.
#[derive(Debug, Clone, Copy)]
struct Junction {
    saturation: Real,
    bottom: Real,
    sidewall: Real,
    mj: Real,
    mjsw: Real,
    potential: Real,
    fc: Real,
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
    /// Depletion charge and capacitance at the polarity-normalized bulk
    /// voltage `v` (`mos1load.c`, linear continuation above `FC * PB` with the
    /// `f2/f3/f4` coefficients of `mos1temp.c`).
    fn charge(&self, v: Real) -> (Real, Real) {
        if self.bottom == 0. && self.sidewall == 0. {
            return (0., 0.);
        }
        let pb = self.potential;
        if v < self.fc * pb {
            let arg = 1. - v / pb;
            let sarg = grading_power(arg, self.mj);
            let sargsw = grading_power(arg, self.mjsw);
            (
                pb * (self.bottom * (1. - arg * sarg) / (1. - self.mj)
                    + self.sidewall * (1. - arg * sargsw) / (1. - self.mjsw)),
                self.bottom * sarg + self.sidewall * sargsw,
            )
        } else {
            let arg = 1. - self.fc;
            let sarg = (-self.mj * arg.ln()).exp();
            let sargsw = (-self.mjsw * arg.ln()).exp();
            let f2 = self.bottom * (1. - self.fc * (1. + self.mj)) * sarg / arg
                + self.sidewall * (1. - self.fc * (1. + self.mjsw)) * sargsw / arg;
            let f3 = self.bottom * self.mj * sarg / arg / pb
                + self.sidewall * self.mjsw * sargsw / arg / pb;
            let depletion = self.fc * pb;
            let f4 = self.bottom * pb * (1. - arg * sarg) / (1. - self.mj)
                + self.sidewall * pb * (1. - arg * sargsw) / (1. - self.mjsw)
                - f3 / 2. * depletion * depletion
                - depletion * f2;
            (f4 + v * (f2 + v * f3 / 2.), f2 + f3 * v)
        }
    }

    /// Current, conductance (with `gmin`), charge and capacitance in physical
    /// terms for the bulk-to-node voltage `v` (`mos1load.c`: constant reverse
    /// saturation current below `-3 Vt`).
    fn point(&self, v: Real, pol: Real, vt: Real, gmin: Real) -> SpiceResult<JunctionPoint> {
        let normalized = pol * v;
        let (current, conductance) = if normalized <= -3. * vt {
            (-self.saturation, 0.)
        } else {
            let e = (normalized / vt).min(MAX_EXP_ARG).exp();
            (self.saturation * (e - 1.), self.saturation * e / vt)
        };
        let (charge, capacitance) = self.charge(normalized);
        let point = JunctionPoint {
            current: pol * current + gmin * v,
            conductance: conductance + gmin,
            charge: pol * charge,
            capacitance,
        };
        point.validate()?;
        Ok(point)
    }
}

/// Every temperature- and geometry-dependent quantity of one load.
#[derive(Debug, Clone, Copy)]
struct Operating {
    vt: Real,
    beta: Real,
    /// C `MOS1tVbi`.
    vbi: Real,
    /// C `MOS1tPhi`.
    phi: Real,
    gamma: Real,
    lambda: Real,
    /// C `OxideCap`: `cox * Leff * m * w`.
    oxide: Real,
    drain: Junction,
    source: Junction,
    /// Gate-source, gate-drain and gate-bulk overlap capacitances.
    overlap: [Real; 3],
}

/// The channel at one bias, in physical terms.
#[derive(Debug, Clone, Copy)]
struct Channel {
    /// Effective drain and source (swapped in inverse mode).
    ports: [NodeId; 2],
    /// Current from `ports[0]` through the channel to `ports[1]`.
    current: Real,
    partials: [(NodeId, Real); 4],
    /// Polarity-normalized `von` and `vdsat` (`mos1load.c` locals).
    von: Real,
    vdsat: Real,
    normal: bool,
    /// `sum(partial * voltage)` at the evaluated voltages (the Newton
    /// linearization point, which limiting may move off the iterate).
    linearized: Real,
}

/// `DEVqmeyer`: half of the bias-dependent gate-source, gate-drain and
/// gate-bulk capacitances, from polarity-normalized voltages.
fn meyer(vgs: Real, vgd: Real, von: Real, vdsat: Real, phi: Real, cox: Real) -> [Real; 3] {
    let vgst = vgs - von;
    let vdsat = vdsat.max(MEYER_MIN_VDSAT);
    let vds = vgs - vgd;
    // The drain share of a capacitance `c` below saturation.
    let split = |c: Real| {
        let vddif = 2. * vdsat - vds;
        let vddif1 = vdsat - vds;
        let vddif2 = vddif * vddif;
        (
            c * (1. - vddif1 * vddif1 / vddif2),
            c * (1. - vdsat * vdsat / vddif2),
        )
    };
    if vgst <= -phi {
        [0., 0., cox / 2.]
    } else if vgst <= -phi / 2. {
        [0., 0., -vgst * cox / (2. * phi)]
    } else if vgst <= 0. {
        let gb = -vgst * cox / (2. * phi);
        let gs = vgst * cox / (1.5 * phi) + cox / 3.;
        if vds >= vdsat {
            [gs, 0., gb]
        } else {
            let (gs, gd) = split(gs);
            [gs, gd, gb]
        }
    } else if vdsat <= vds {
        [cox / 3., 0., 0.]
    } else {
        let (gs, gd) = split(cox / 3.);
        [gs, gd, 0.]
    }
}

/// Everything one bias point loads.
#[derive(Debug, Clone, Copy)]
struct Point {
    channel: Channel,
    bd: JunctionPoint,
    bs: JunctionPoint,
    /// Physical `vgs`, `vgd`, `vgb` (internal drain/source nodes).
    gate: [Real; 3],
    /// Meyer half capacitances for gate-source, gate-drain, gate-bulk.
    half: [Real; 3],
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
    /// (`mos1temp.c` and the start of `mos1load.c`).
    fn operating(&self, context: &ModelContext) -> SpiceResult<Operating> {
        let (model, geometry) = (&self.model, &self.geometry);
        let tnom = model.tnom.unwrap_or(context.nominal_temperature) + CELSIUS_TO_KELVIN;
        let temp = geometry
            .temp
            .unwrap_or(context.temperature + geometry.dtemp)
            + CELSIUS_TO_KELVIN;
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
        let scale = (-egfet / vt + egfet1 / vtnom).exp();
        let (saturation, density) = (model.is * scale, model.js * scale);
        let pbo = (model.pb - pbfact1) / fact1;
        let gmaold = (model.pb - pbo) / pbo;
        let potential = fact2 * pbo + pbfact;
        let gmanew = (potential - pbo) / pbo;
        // Two-step capacitance temperature factor of `mos1temp.c`.
        let factor = |grading: Real| {
            (1. + grading * (4e-4 * (temp - REFTEMP) - gmanew))
                / (1. + grading * (4e-4 * (tnom - REFTEMP) - gmaold))
        };
        let (bottom, side) = (factor(model.mj), factor(model.mjsw));
        let m = geometry.m;
        let bottom_cap = |given: Option<Real>, area: Real| match (given, model.cj) {
            (Some(c), _) => c * bottom * m,
            (None, Some(cj)) => cj * bottom * m * area,
            (None, None) => 0.,
        };
        let side_cap = |perimeter: Real| model.cjsw.map_or(0., |c| c * side * perimeter * m);
        let (sat_d, sat_s) = if density == 0. || geometry.ad == 0. || geometry.as_ == 0. {
            (m * saturation, m * saturation)
        } else {
            (density * m * geometry.ad, density * m * geometry.as_)
        };
        let junction = |saturation, bottom, sidewall| Junction {
            saturation,
            bottom,
            sidewall,
            mj: model.mj,
            mjsw: model.mjsw,
            potential,
            fc: model.fc,
        };
        let operating = Operating {
            vt,
            beta: kp * m * geometry.w / geometry.length,
            vbi,
            phi,
            gamma: process.gamma,
            lambda: model.lambda,
            oxide: process.cox * geometry.length * m * geometry.w,
            drain: junction(
                sat_d,
                bottom_cap(model.cbd, geometry.ad),
                side_cap(geometry.pd),
            ),
            source: junction(
                sat_s,
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
            operating.gamma,
            operating.oxide,
            operating.drain.saturation,
            operating.source.saturation,
            operating.drain.bottom,
            operating.drain.sidewall,
            operating.source.bottom,
            operating.source.sidewall,
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
            || [bottom, side].iter().any(|f| !f.is_finite() || *f < 0.)
        {
            return Err(SpiceError::circuit(format!(
                "{}: MOS1 parameters are out of range at {:.2} K \
                 (nonfinite value, or nonpositive PHI/PB/KP/IS after temperature scaling)",
                self.name, temp
            )));
        }
        Ok(operating)
    }

    /// `mos1load.c`'s channel evaluation at physical voltages `v` of
    /// d', g, s', b.
    fn channel(&self, op: &Operating, v: [Real; 4]) -> SpiceResult<Channel> {
        let pol = self.model.pol;
        let [d, g, s, b] = self.inner;
        let normal = pol * (v[0] - v[2]) >= 0.;
        let (drain, source, vd, vs) = if normal {
            (d, s, v[0], v[2])
        } else {
            (s, d, v[2], v[0])
        };
        let vds = pol * (vd - vs);
        let vgs = pol * (v[1] - vs);
        let vbs = pol * (v[3] - vs);
        let root = op.phi.sqrt();
        // `sarg` and its derivative with respect to vbs.
        let (sarg, dsarg) = if vbs <= 0. {
            let sarg = (op.phi - vbs).sqrt();
            (sarg, -0.5 / sarg)
        } else {
            let sarg = root - vbs / (root + root);
            if sarg > 0. {
                (sarg, -0.5 / root)
            } else {
                (0., 0.)
            }
        };
        let von = op.vbi * pol + op.gamma * sarg;
        let vgst = vgs - von;
        let vdsat = vgst.max(0.);
        let (current, gm, gds) = if vgst <= 0. {
            (0., 0., 0.)
        } else {
            let betap = op.beta * (1. + op.lambda * vds);
            if vgst <= vds {
                (
                    betap * vgst * vgst * 0.5,
                    betap * vgst,
                    op.lambda * op.beta * vgst * vgst * 0.5,
                )
            } else {
                (
                    betap * vds * (vgst - 0.5 * vds),
                    betap * vds,
                    betap * (vgst - vds) + op.lambda * op.beta * vds * (vgst - 0.5 * vds),
                )
            }
        };
        let gmb = -gm * op.gamma * dsarg;
        if [current, gm, gds, gmb, von].iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: format!("MOS1 {}", self.name),
                message: "nonfinite channel current/Jacobian".into(),
            });
        }
        Ok(Channel {
            ports: [drain, source],
            current: pol * current,
            partials: [(drain, gds), (g, gm), (source, -gds - gm - gmb), (b, gmb)],
            von,
            vdsat,
            normal,
            linearized: gds * vd + gm * v[1] - (gds + gm + gmb) * vs + gmb * v[3],
        })
    }

    /// Every bias-dependent quantity at physical node voltages `v` of the
    /// intrinsic d', g, s', b.
    fn point(&self, op: &Operating, v: [Real; 4], gmin: Real) -> SpiceResult<Point> {
        let pol = self.model.pol;
        let channel = self.channel(op, v)?;
        let bd = op.drain.point(v[3] - v[0], pol, op.vt, gmin)?;
        let bs = op.source.point(v[3] - v[2], pol, op.vt, gmin)?;
        let gate = [v[1] - v[2], v[1] - v[0], v[1] - v[3]];
        let (vgs, vgd) = (pol * gate[0], pol * gate[1]);
        let half = if channel.normal {
            meyer(vgs, vgd, channel.von, channel.vdsat, op.phi, op.oxide)
        } else {
            let [gd, gs, gb] = meyer(vgd, vgs, channel.von, channel.vdsat, op.phi, op.oxide);
            [gs, gd, gb]
        };
        Ok(Point {
            channel,
            bd,
            bs,
            gate,
            half,
        })
    }

    /// `mos1load.c`'s device-frame `[vbs, vgs, vds]` for this load:
    /// `MODEINITJCT` starts at `vbs = -1`, `vgs = type * tVto`, `vds = 0`;
    /// later loads limit the gate voltage with `DEVfetlim` against the
    /// previous `von` (through `vgs`, or `vgd` when the previous `vds` was
    /// negative), `vds` with `DEVlimvds` and the forward-biased bulk junction
    /// with `DEVpnjlim` (`vt = kT/q`, source/drain `vcrit` from their
    /// saturation currents).
    ///
    /// The start voltages follow `mos1load.c`'s `MODEINITJCT` branch: an
    /// `off` instance starts at zero (and is held there in `MODEINITFIX`);
    /// otherwise the `IC` vector `type * [ICVBS, ICVGS, ICVDS]` (`ic`,
    /// device frame `[vbs, vgs, vds]`), replaced by the default start when
    /// all three are zero, except in the `uic` initial load.
    fn limit(
        &self,
        op: &Operating,
        limiter: &mut Limiter,
        states: &crate::devices::DeviceState<'_>,
        raw: [Real; 3],
        ic: [Real; 3],
    ) -> [Real; 3] {
        let pol = self.model.pol;
        let mode = limiter.mode();
        if mode != Linearization::InitialConditions && limiter.holds_off(states, self.initial.off) {
            return [0.; 3];
        }
        match mode {
            Linearization::InitialConditions if self.initial.off => return [0.; 3],
            Linearization::InitialConditions => return ic.map(|v| pol * v),
            Linearization::Initial => {
                if ic.iter().any(|v| *v != 0.) {
                    return ic.map(|v| pol * v);
                }
                let tvto = op.vbi + pol * op.gamma * op.phi.sqrt();
                return [-1., pol * tvto, 0.];
            }
            _ => {}
        }
        let previous: Vec<_> = (0..4)
            .map(|k| limiter.previous(states, slot::LIMITED + k))
            .collect();
        let [Some(vbs_old), Some(vgs_old), Some(vds_old), Some(von)] = previous[..] else {
            return raw;
        };
        let [mut vbs, mut vgs, mut vds] = raw;
        let vgd = vgs - vds;
        if vds_old >= 0. {
            vgs = limiter.fet_gate(vgs, vgs_old, von);
            vds = limiter.drain_source(vgs - vgd, vds_old);
        } else {
            let vgd = limiter.fet_gate(vgd, vgs_old - vds_old, von);
            vds = -limiter.drain_source(-(vgs - vgd), -vds_old);
            vgs = vgd + vds;
        }
        if vds >= 0. {
            let vcrit = limiting::critical_voltage(op.vt, op.source.saturation);
            vbs = limiter.pn_junction(vbs, Some(vbs_old), op.vt, vcrit);
        } else {
            let vcrit = limiting::critical_voltage(op.vt, op.drain.saturation);
            let vbd = limiter.pn_junction(vbs - vds, Some(vbs_old - vds_old), op.vt, vcrit);
            vbs = vbd + vds;
        }
        [vbs, vgs, vds]
    }

    /// The instance initial conditions `[ICVBS, ICVGS, ICVDS]` for this
    /// load: unset components are the external terminal voltages of the
    /// solution in the `uic` initial load (`mos1ic.c`, called by `CKTic`
    /// only under `MODEUIC`) and zero (C's unset default) otherwise.
    fn start_conditions(&self, limiter: &Limiter, voltage: impl Fn(NodeId) -> Real) -> [Real; 3] {
        let [vds, vgs, vbs] = [0, 1, 2].map(|k| self.initial.values[k]);
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        if limiter.mode() == Linearization::InitialConditions {
            [
                vbs.unwrap_or_else(|| voltage(b) - voltage(s)),
                vgs.unwrap_or_else(|| voltage(g) - voltage(s)),
                vds.unwrap_or_else(|| voltage(d) - voltage(s)),
            ]
        } else {
            [vbs, vgs, vds].map(|v| v.unwrap_or(0.))
        }
    }

    /// Nodes of the drain/source series resistors and the gate charges.
    fn series_ports(&self) -> [[NodeId; 2]; 2] {
        [
            [self.terminals[0], self.inner[0]],
            [self.terminals[2], self.inner[2]],
        ]
    }
    fn gate_ports(&self) -> [[NodeId; 2]; 3] {
        let [d, g, s, b] = self.inner;
        [[g, s], [g, d], [g, b]]
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

fn stamp_current(
    context: &mut StampContext<'_>,
    output: [NodeId; 2],
    current: Real,
    partials: &[(NodeId, Real)],
    linearized: Real,
) -> SpiceResult<()> {
    let equivalent = current - linearized;
    for (row, sign) in [(output[0], 1.), (output[1], -1.)] {
        for (col, derivative) in partials {
            context.stamp(row, *col, sign * derivative)?;
        }
        context.stamp_rhs(row, -sign * equivalent)?;
    }
    Ok(())
}

fn linear_current(
    context: &mut LinearContext<'_>,
    output: [NodeId; 2],
    partials: &[(NodeId, Real)],
) -> SpiceResult<()> {
    for (row, sign) in [(output[0], 1.), (output[1], -1.)] {
        for (col, derivative) in partials {
            if let (Some(r), Some(c)) = (
                context.unknowns.node_row(row),
                context.unknowns.node_row(*col),
            ) {
                context.system.a.add(r, c, sign * derivative)?;
            }
        }
    }
    Ok(())
}

impl Device for Mos1 {
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
            "m", "l", "w", "ad", "as", "pd", "ps", "nrd", "nrs", "temp", "dtemp",
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
        use crate::devices::sweep::check_swept;
        let mut geometry = self.geometry;
        let check = |ok: bool, what: &str| check_swept(&self.name, parameter, value, ok, what);
        match parameter {
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
            initial: self.initial.clone(),
        };
        device.operating(context)?;
        Ok(Box::new(device))
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("MOS1 AC needs small-signal assembly"));
        }
        let op = self.operating(&context.model_context())?;
        let node = self.inner.map(|n| context.node_voltage(n));
        let pol = self.model.pol;
        let raw = [node[3] - node[2], node[1] - node[2], node[0] - node[2]].map(|x| pol * x);
        let mut limiter = Limiter::new(&context.states);
        let ic = self.start_conditions(&limiter, |n| context.node_voltage(n));
        let [vbs, vgs, vds] = self.limit(&op, &mut limiter, &context.states, raw, ic);
        // The intrinsic d', g, s', b at the (limited) voltages, relative to
        // s'; every equation below depends on voltage differences only.
        let v = [vds, vgs, 0., vbs].map(|x| pol * x);
        let point = self.point(&op, v, context.gmin)?;
        // MOS1convTest for an `off` instance held in MODEINITFIX: the drain
        // (channel minus bulk-drain) and bulk currents at the held voltages
        // against their linear prediction at the iterate's node voltages.
        {
            let node_voltage = |n: NodeId| context.node_voltage(n);
            let [d, _, s, b] = self.inner;
            let channel = &point.channel;
            let channel_change: Real = channel
                .partials
                .iter()
                .map(|(node, partial)| partial * node_voltage(*node))
                .sum::<Real>()
                - channel.linearized;
            let bd_change =
                point.bd.conductance * (node_voltage(b) - node_voltage(d) - (v[3] - v[0]));
            let bs_change =
                point.bs.conductance * (node_voltage(b) - node_voltage(s) - (v[3] - v[2]));
            let drain = channel.current - point.bd.current;
            let bulk = point.bd.current + point.bs.current;
            limiter.test_held(
                &context.states,
                &[
                    (drain, drain + channel_change - bd_change),
                    (bulk, bulk + bd_change + bs_change),
                ],
            );
        }
        for (ports, conductance) in self.series_ports().into_iter().zip(self.series) {
            if conductance > 0. {
                nodal_stamp(context.matrix, context.unknowns, ports, conductance)?;
            }
        }
        let channel = point.channel;
        stamp_current(
            context,
            channel.ports,
            channel.current,
            &channel.partials,
            channel.linearized,
        )?;
        let [d, _, s, b] = self.inner;
        stamp_junction(context, [b, d], v[3] - v[0], point.bd, slot::QBD)?;
        stamp_junction(context, [b, s], v[3] - v[2], point.bs, slot::QBS)?;
        // Meyer gate charge (`mos1load.c`): at an operating point the charge
        // is `v * (2 half + overlap)`; in transient it advances from the last
        // accepted point with the average of both points' half capacitances,
        // `q = q1 + (v - v1) * (half + half1 + overlap)`, whose Jacobian is
        // that averaged capacitance.
        for (k, ports) in self.gate_ports().into_iter().enumerate() {
            let voltage = point.gate[k];
            let half = point.half[k];
            context.states.set(slot::VG[k], voltage)?;
            context.states.set(slot::HALF[k], half)?;
            let (charge, capacitance) = if context.integration.is_some() {
                let accepted = |s: usize| {
                    context.states.accepted(1, s).ok_or_else(|| {
                        SpiceError::circuit(format!("{}: missing accepted MOS1 state", self.name))
                    })
                };
                let capacitance = half + accepted(slot::HALF[k])? + op.overlap[k];
                (
                    accepted(slot::QG[k])? + (voltage - accepted(slot::VG[k])?) * capacitance,
                    capacitance,
                )
            } else {
                let capacitance = 2. * half + op.overlap[k];
                (voltage * capacitance, capacitance)
            };
            stamp_junction(
                context,
                ports,
                voltage,
                JunctionPoint {
                    current: 0.,
                    conductance: 0.,
                    charge,
                    capacitance,
                },
                slot::QG[k],
            )?;
        }
        limiter.finish(
            &mut context.states,
            &[
                (slot::LIMITED, vbs),
                (slot::LIMITED + 1, vgs),
                (slot::LIMITED + 2, vds),
                (slot::LIMITED + 3, channel.von),
            ],
        )
    }
    /// `mos1acld.c`: conductances at the bias plus `jω` times the junction
    /// capacitances and the Meyer capacitances `2 half + overlap`.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let op = self.operating(context.model_context)?;
        let v = self.inner.map(|n| {
            context
                .unknowns
                .node_row(n)
                .and_then(|r| bias.get(r))
                .unwrap_or(0.)
        });
        let point = self.point(&op, v, context.model_context.gmin)?;
        for (ports, conductance) in self.series_ports().into_iter().zip(self.series) {
            if conductance > 0. {
                context.nodal(ports, conductance, false)?;
            }
        }
        linear_current(context, point.channel.ports, &point.channel.partials)?;
        let [d, _, s, b] = self.inner;
        for (ports, junction) in [([b, d], point.bd), ([b, s], point.bs)] {
            context.nodal(ports, junction.conductance, false)?;
            context.nodal(ports, junction.capacitance, true)?;
        }
        for (k, ports) in self.gate_ports().into_iter().enumerate() {
            context.nodal(ports, 2. * point.half[k] + op.overlap[k], true)?;
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
        let v = self.inner.map(|node| context.voltage(node));
        let point = self.point(&op, v, context.model_context.gmin)?;
        let channel = point.channel;
        let pol = model.pol;
        let gm = channel.partials[1].1;
        let sid = if model.nlev < 3 {
            2.0 / 3.0 * gm.abs()
        } else {
            let (vd, vs) = if channel.normal {
                (v[0], v[2])
            } else {
                (v[2], v[0])
            };
            let vds = pol * (vd - vs);
            let vgst = pol * (v[1] - vs) - channel.von;
            if vgst > 0. {
                let alpha = if vgst <= vds {
                    0.
                } else {
                    1. - vds / channel.vdsat
                };
                2.0 / 3.0 * op.beta * vgst * (1. + alpha + alpha * alpha) / (1. + alpha)
                    * model.gdsnoi
            } else {
                0.
            }
        };
        let mode = if channel.normal { 1. } else { -1. };
        let cd = mode * (pol * channel.current) - pol * point.bd.current;
        let cox = match model.tox.filter(|tox| *tox != 0.) {
            Some(tox) => 3.9 * EPSILON_0 / tox,
            None => 3.9 * 8.854214871e-12 / 1e-7,
        };
        let m = geometry.m;
        let length = geometry.length;
        let current_law = || m * model.kf * (model.af * (cd / m).abs().max(1e-38).ln()).exp();
        let (coefficient, exponent) = match model.nlev {
            0 => (current_law() / (length * length * cox), 1.),
            1 => (current_law() / (geometry.w * length * cox), 1.),
            _ => (
                model.kf * gm * gm / m / (geometry.w * length * cox),
                model.af,
            ),
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
        Junction {
            saturation: 1e-14,
            bottom,
            sidewall,
            mj,
            mjsw,
            potential: 0.8,
            fc: 0.5,
        }
    }

    #[test]
    fn junction_capacitance_is_the_charge_derivative_across_the_fc_boundary() {
        for j in [
            junction(1e-12, 0., 0.5, 0.5),
            junction(1e-12, 2e-13, 0.5, 0.33),
            junction(0., 3e-13, 0.4, 0.33),
        ] {
            for v in [-3., -0.5, 0., 0.39, 0.4, 0.41, 0.7] {
                let h = 1e-6;
                let (_, c) = j.charge(v);
                let numerical = (j.charge(v + h).0 - j.charge(v - h).0) / (2. * h);
                assert!((numerical - c).abs() <= 1e-7 * c, "{v}: {numerical} {c}");
            }
            // Charge and capacitance are continuous at FC*PB.
            let edge = 0.5 * 0.8;
            let (below, above) = (j.charge(edge - 1e-12), j.charge(edge));
            assert!((below.0 - above.0).abs() <= 1e-9 * above.0.abs());
            assert!((below.1 - above.1).abs() <= 1e-9 * above.1);
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
