//! The level-independent MOSFET shell shared by the classic SPICE levels.
//!
//! ngspice implements every classic MOS level (`mos1/`, `mos2/`, `mos3/`,
//! `mos6/`, `mos9/`) as a copy of one device frame around a level-specific
//! drain-current routine. This module holds that frame once:
//!
//! - instance geometry and its schema (`mos1par.c`/`mos3par.c` share the same
//!   setters), `off` and the `IC` vector;
//! - the drain/source series resistances and the internal prime nodes
//!   (`mos1set.c`/`mos1temp.c`);
//! - the shared temperature quantities and the bulk junctions: saturation
//!   currents, the two-step capacitance temperature factor and the depletion
//!   charge with its `FC` continuation (`mos1temp.c`, `mos1load.c`);
//! - Meyer's gate capacitances (`devsup.c` `DEVqmeyer`) and C's
//!   state-averaged gate charge;
//! - Newton limiting and start voltages (`DEVfetlim`/`DEVlimvds`/`DEVpnjlim`,
//!   `MODEINITJCT`, `MODEINITFIX`), the `MOSconvTest` held-`off` check;
//! - the real, small-signal (`mos1acld.c`) and pole-zero (`mos1pzld.c`) loads,
//!   the truncation slots (`mos1trun.c`), `.dc @m[...]` instance sweeps and
//!   instance observations.
//!
//! A level plugs in through [`MosLevel`]: it owns its model schema and
//! validated card, the temperature/process derivation that fills
//! [`Operating`] (using [`Temperatures`] and [`Common::junctions`]), and the
//! drain current with its derivatives in C's normalized device frame
//! ([`MosLevel::drain_current`]). Levels choose the bulk-junction reverse law
//! ([`ReverseLaw`]) and the internal node suffixes, and may provide `.noise`
//! and `.disto` generators; the defaults are explicit `NotYetPorted` errors,
//! never zeros. Levels whose frame differs (BSIM charge models, MOS6's
//! separate channel model) can reuse the helpers without the generic device.

use crate::devices::initial::InstanceInitial;
use crate::devices::limiting::{self, Limiter, Linearization};
use crate::devices::linear::nodal_stamp;
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

/// Slots of the state vector (C `MOS1numStates` layout, reordered so that
/// every charge is directly followed by its derivative).
pub(super) mod slot {
    pub(in crate::devices) const QBD: usize = 0;
    pub(in crate::devices) const QBS: usize = 2;
    /// Gate-source, gate-drain and gate-bulk Meyer charges (and derivatives).
    pub(in crate::devices) const QG: [usize; 3] = [4, 6, 8];
    /// Physical gate-source/gate-drain/gate-bulk voltages of the load.
    pub(in crate::devices) const VG: [usize; 3] = [10, 11, 12];
    /// Meyer half capacitances of the load (C `MOS1capgs`, ...).
    pub(in crate::devices) const HALF: [usize; 3] = [13, 14, 15];
    /// Limited `vbs`, `vgs`, `vds` and the load's `von` (C `MOS1vbs`,
    /// `MOS1vgs`, `MOS1vds`, `MOS1von`), for the next load's limiting.
    pub(in crate::devices) const LIMITED: usize = 16;
    pub(in crate::devices) const COUNT: usize = 20;
}

/// A schema entry.
pub(super) const fn p(name: &'static str, unit: U, domain: D, default: Option<Real>) -> P {
    P {
        name,
        unit,
        domain,
        default,
    }
}

/// Instance setters (`mos1par.c`, identical in `mos3par.c`). Defaults are
/// C's `CKTdefaultMos*` values, whose `.options defl/defw/defad/defas/defm`
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

/// A schema value that has a default, so it is always present.
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

/// Band gap of silicon at `kelvin` (`mos1temp.c`).
pub(super) fn band_gap(kelvin: Real) -> Real {
    1.16 - (7.02e-4 * kelvin * kelvin) / (kelvin + 1108.)
}

/// The `pbfact` potential correction of `mos1temp.c` at `kelvin`.
fn potential_factor(kelvin: Real) -> Real {
    let vt = kelvin * K_OVER_Q;
    let kt = BOLTZMANN * kelvin;
    let arg = -band_gap(kelvin) / (kt + kt) + 1.1150877 / (BOLTZMANN * (REFTEMP + REFTEMP));
    -2. * vt * (1.5 * (kelvin / REFTEMP).ln() + CHARGE * arg)
}

/// Model setters every classic level shares, validated from the level's own
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
    /// The shared setters of a validated model card. `noise_file` names the
    /// level's C noise routine in the NLEV diagnostic.
    pub(super) fn from_values(
        m: &ScalarValues,
        family: ModelFamily,
        label: &str,
        noise_file: &str,
    ) -> SpiceResult<Self> {
        let pol = if family == ModelFamily::Pmos { -1. } else { 1. };
        let get = |name: &str| m.get(name).map(|v| v.value);
        let required = |name: &str| required(m, name, label);
        Ok(Self {
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
            kf: required("kf")?,
            af: required("af")?,
            nlev: {
                // IF_INTEGER setter: floor(value + 0.5). C's switch has no
                // case outside 0..=3 (the flicker density is then the bare
                // gain), which the port rejects.
                let raw = required("nlev")?;
                let rounded = (raw + 0.5).floor();
                if !(0. ..=3.).contains(&rounded) {
                    return Err(SpiceError::circuit(format!(
                        "{label} NLEV must round to 0..=3 ({noise_file}), got {raw}"
                    )));
                }
                rounded as u8
            },
            gdsnoi: required("gdsnoi")?,
        })
    }

    /// C's checks every level shares after the card is read: TPG is an
    /// integer setter and the junction gradings stay below one.
    pub(super) fn check(&self, tpg: Option<Real>, label: &str) -> SpiceResult<()> {
        if tpg.is_some_and(|tpg| tpg.fract() != 0.) {
            return Err(SpiceError::circuit(format!(
                "{label} TPG must be an integer"
            )));
        }
        if self.mj >= 1. || self.mjsw >= 1. || self.fc >= 1. {
            return Err(SpiceError::circuit(format!(
                "{label} requires MJ, MJSW and FC below 1"
            )));
        }
        Ok(())
    }

    /// The bulk junctions of one instance at the temperatures `t`
    /// (`mos1temp.c`/`mos3temp.c`, identical in both levels): temperature-
    /// scaled saturation currents (area-scaled through JS when both areas are
    /// given), the junction potential and the two-step capacitance factor.
    pub(super) fn junctions(
        &self,
        t: &Temperatures,
        geometry: &Geometry,
        reverse: ReverseLaw,
    ) -> Junctions {
        let scale = (-t.egfet / t.vt + t.egfet1 / t.vtnom).exp();
        let (saturation, density) = (self.is * scale, self.js * scale);
        let pbo = (self.pb - t.pbfact1) / t.fact1;
        let gmaold = (self.pb - pbo) / pbo;
        let potential = t.fact2 * pbo + t.pbfact;
        let gmanew = (potential - pbo) / pbo;
        let (temp, tnom) = (t.temp, t.tnom);
        // Two-step capacitance temperature factor of `mos1temp.c`.
        let factor = |grading: Real| {
            (1. + grading * (4e-4 * (temp - REFTEMP) - gmanew))
                / (1. + grading * (4e-4 * (tnom - REFTEMP) - gmaold))
        };
        let (bottom, side) = (factor(self.mj), factor(self.mjsw));
        let m = geometry.m;
        let bottom_cap = |given: Option<Real>, area: Real| match (given, self.cj) {
            (Some(c), _) => c * bottom * m,
            (None, Some(cj)) => cj * bottom * m * area,
            (None, None) => 0.,
        };
        let side_cap = |perimeter: Real| self.cjsw.map_or(0., |c| c * side * perimeter * m);
        let (sat_d, sat_s) = if density == 0. || geometry.ad == 0. || geometry.as_ == 0. {
            (m * saturation, m * saturation)
        } else {
            (density * m * geometry.ad, density * m * geometry.as_)
        };
        let junction = |saturation, bottom, sidewall| Junction {
            saturation,
            bottom,
            sidewall,
            mj: self.mj,
            mjsw: self.mjsw,
            potential,
            fc: self.fc,
            reverse,
        };
        Junctions {
            drain: junction(
                sat_d,
                bottom_cap(self.cbd, geometry.ad),
                side_cap(geometry.pd),
            ),
            source: junction(
                sat_s,
                bottom_cap(self.cbs, geometry.as_),
                side_cap(geometry.ps),
            ),
            potential,
            factors: [bottom, side],
        }
    }
}

/// The two bulk junctions of an instance.
#[derive(Debug, Clone, Copy)]
pub(super) struct Junctions {
    pub(super) drain: Junction,
    pub(super) source: Junction,
    /// C `MOSxtBulkPot`.
    pub(super) potential: Real,
    /// The bottom and sidewall capacitance temperature factors.
    pub(super) factors: [Real; 2],
}

/// Validated instance geometry.
#[derive(Debug, Clone, Copy)]
pub(super) struct Geometry {
    pub(super) m: Real,
    /// Drawn length and width (the level derives the effective ones).
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

/// The nominal and device temperatures of one load and the `mos1temp.c`
/// quantities derived from them.
#[derive(Debug, Clone, Copy)]
pub(super) struct Temperatures {
    /// Nominal (TNOM) and device temperature in kelvin.
    pub(super) tnom: Real,
    pub(super) temp: Real,
    pub(super) vt: Real,
    pub(super) vtnom: Real,
    pub(super) egfet: Real,
    pub(super) egfet1: Real,
    pub(super) fact1: Real,
    pub(super) fact2: Real,
    pub(super) pbfact: Real,
    pub(super) pbfact1: Real,
    /// `temp / tnom`.
    pub(super) ratio: Real,
}

impl Temperatures {
    /// TNOM defaults to `.options tnom`; the instance runs at TEMP, or the
    /// circuit temperature plus DTEMP.
    pub(super) fn new(
        tnom: Option<Real>,
        geometry: &Geometry,
        context: &ModelContext,
        name: &str,
        label: &str,
    ) -> SpiceResult<Self> {
        let tnom = tnom.unwrap_or(context.nominal_temperature) + CELSIUS_TO_KELVIN;
        let temp = geometry
            .temp
            .unwrap_or(context.temperature + geometry.dtemp)
            + CELSIUS_TO_KELVIN;
        if !(tnom.is_finite() && tnom > 0. && temp.is_finite() && temp > 0.) {
            return Err(SpiceError::circuit(format!(
                "{name}: invalid {label} temperature"
            )));
        }
        Ok(Self {
            tnom,
            temp,
            vt: temp * K_OVER_Q,
            vtnom: tnom * K_OVER_Q,
            egfet: band_gap(temp),
            egfet1: band_gap(tnom),
            fact1: tnom / REFTEMP,
            fact2: temp / REFTEMP,
            pbfact: potential_factor(temp),
            pbfact1: potential_factor(tnom),
            ratio: temp / tnom,
        })
    }
}

/// The bulk-junction current below `-3 Vt` in reverse bias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReverseLaw {
    /// `mos1load.c`: the constant reverse saturation current `-Is`.
    Constant,
}

/// One side's bulk junction: zero-bias bottom/sidewall capacitances at the
/// device temperature and the temperature-adjusted potential.
#[derive(Debug, Clone, Copy)]
pub(super) struct Junction {
    pub(super) saturation: Real,
    pub(super) bottom: Real,
    pub(super) sidewall: Real,
    pub(super) mj: Real,
    pub(super) mjsw: Real,
    pub(super) potential: Real,
    pub(super) fc: Real,
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
    /// Depletion charge and capacitance at the polarity-normalized bulk
    /// voltage `v` (`mos1load.c`, linear continuation above `FC * PB` with the
    /// `f2/f3/f4` coefficients of `mos1temp.c`).
    pub(super) fn charge(&self, v: Real) -> (Real, Real) {
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
    /// terms for the bulk-to-node voltage `v` ([`ReverseLaw`] below `-3 Vt`).
    pub(super) fn point(
        &self,
        v: Real,
        pol: Real,
        vt: Real,
        gmin: Real,
    ) -> SpiceResult<JunctionPoint> {
        let normalized = pol * v;
        let (current, conductance) = if normalized <= -3. * vt {
            match self.reverse {
                ReverseLaw::Constant => (-self.saturation, 0.),
            }
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

/// Every temperature- and geometry-dependent quantity of one load. The
/// fields are what the shell needs; `params` is the level's own part.
#[derive(Debug, Clone, Copy)]
pub(super) struct Operating<C> {
    pub(super) vt: Real,
    /// C `MOSxtVbi`.
    pub(super) vbi: Real,
    /// C `MOSxtPhi`.
    pub(super) phi: Real,
    /// The model GAMMA (given or extracted), for `tVto = tVbi + type gamma
    /// sqrt(tPhi)`.
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

/// A level's drain current in C's normalized device frame (`vds >= 0` after
/// the drain/source interchange, polarity removed), with C's derivatives
/// (`MOSxgm`, `MOSxgds`, `MOSxgmbs`) and the `von`/`vdsat` its Meyer
/// capacitances and limiting use.
#[derive(Debug, Clone, Copy)]
pub(super) struct DrainCurrent {
    pub(super) current: Real,
    pub(super) gm: Real,
    pub(super) gds: Real,
    pub(super) gmbs: Real,
    pub(super) von: Real,
    pub(super) vdsat: Real,
}

/// What a MOS level supplies to the shared shell. See the module
/// documentation.
pub(super) trait MosLevel: std::fmt::Debug + Clone + Sized + 'static {
    /// The level's temperature-dependent channel parameters.
    type Params: std::fmt::Debug + Clone + Copy;
    /// The model `level` selector this implementation serves.
    const LEVEL: u8;
    /// Diagnostic label, e.g. `MOS1`.
    const LABEL: &'static str;
    /// `CKTmkVolt` suffixes of the internal drain and source nodes.
    const PRIME_SUFFIXES: [&'static str; 2];

    /// The validated model card (schema, defaults and C's card checks).
    ///
    /// # Errors
    /// Unknown or invalid setters, unsupported values.
    fn from_card(resolved: &ResolvedModel<'_>) -> SpiceResult<Self>;

    /// The setters shared by every level.
    fn common(&self) -> &Common;

    /// Rejects geometry whose effective channel C would refuse or leave
    /// nonpositive.
    ///
    /// # Errors
    /// A nonpositive effective length or width.
    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()>;

    /// The temperature-, process- and geometry-dependent values of one load.
    ///
    /// # Errors
    /// Parameters out of range at the device temperature.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Self::Params>>;

    /// The drain current at normalized device-frame `vgs`, `vds >= 0`,
    /// `vbs` (`vgd`/`vbd` of the instance in inverse mode).
    fn drain_current(
        &self,
        op: &Operating<Self::Params>,
        vgs: Real,
        vds: Real,
        vbs: Real,
    ) -> DrainCurrent;

    /// `.noise` generators (`mosXnoi.c`).
    ///
    /// # Errors
    /// [`SpiceError::NotYetPorted`] unless the level ports its noise routine.
    fn noise(
        device: &Mosfet<Self>,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(SpiceError::not_yet_ported(
            format!("{} noise of {}", Self::LABEL, device.name),
            format!(
                "src/spicelib/devices/mos{}/mos{}noi.c",
                Self::LEVEL,
                Self::LEVEL
            ),
        ))
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

/// The channel at one bias, in physical terms.
#[derive(Debug, Clone, Copy)]
pub(super) struct Channel {
    /// Effective drain and source (swapped in inverse mode).
    pub(super) ports: [NodeId; 2],
    /// Current from `ports[0]` through the channel to `ports[1]`.
    pub(super) current: Real,
    /// `[(drain, gds), (gate, gm), (source, -gds - gm - gmbs), (bulk, gmbs)]`.
    pub(super) partials: [(NodeId, Real); 4],
    /// Polarity-normalized `von` and `vdsat` (`mos1load.c` locals).
    pub(super) von: Real,
    pub(super) vdsat: Real,
    pub(super) normal: bool,
    /// `sum(partial * voltage)` at the evaluated voltages (the Newton
    /// linearization point, which limiting may move off the iterate).
    pub(super) linearized: Real,
}

/// `DEVqmeyer`: half of the bias-dependent gate-source, gate-drain and
/// gate-bulk capacitances, from polarity-normalized voltages.
pub(super) fn meyer(
    vgs: Real,
    vgd: Real,
    von: Real,
    vdsat: Real,
    phi: Real,
    cox: Real,
) -> [Real; 3] {
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
pub(super) struct Point {
    pub(super) channel: Channel,
    pub(super) bd: JunctionPoint,
    pub(super) bs: JunctionPoint,
    /// Physical `vgs`, `vgd`, `vgb` (internal drain/source nodes).
    pub(super) gate: [Real; 3],
    /// Meyer half capacitances for gate-source, gate-drain, gate-bulk.
    pub(super) half: [Real; 3],
}

/// A classic MOSFET: the shared frame around level `L`'s drain current.
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
    /// `OFF` and `ICVDS`/`ICVGS`/`ICVBS` (C `MOSxoff`, `MOSxicV*`).
    pub(super) initial: InstanceInitial,
}

impl<L: MosLevel> Mosfet<L> {
    pub(super) fn instantiate(
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
        let model = L::from_card(resolved)?;
        let v = ScalarSchema {
            parameters: INSTANCE,
        }
        .validate(&setters, &i.location)?;
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
        self.model.operating(&self.name, &self.geometry, context)
    }

    /// The channel evaluation at physical voltages `v` of d', g, s', b:
    /// C's drain/source interchange around the level's drain current.
    pub(super) fn channel(&self, op: &Operating<L::Params>, v: [Real; 4]) -> SpiceResult<Channel> {
        let pol = self.model.common().pol;
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
        let DrainCurrent {
            current,
            gm,
            gds,
            gmbs: gmb,
            von,
            vdsat,
        } = self.model.drain_current(op, vgs, vds, vbs);
        if [current, gm, gds, gmb, von].iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: format!("{} {}", L::LABEL, self.name),
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
    pub(super) fn point(
        &self,
        op: &Operating<L::Params>,
        v: [Real; 4],
        gmin: Real,
    ) -> SpiceResult<Point> {
        let pol = self.model.common().pol;
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
        op: &Operating<L::Params>,
        limiter: &mut Limiter,
        states: &crate::devices::DeviceState<'_>,
        raw: [Real; 3],
        ic: [Real; 3],
    ) -> [Real; 3] {
        let pol = self.model.common().pol;
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

impl<L: MosLevel> Device for Mosfet<L> {
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
    /// NRD, NRS, TEMP and DTEMP, which the temperature/load routines re-derive
    /// (`dctrcurv.c` `DCTsetInstParam`).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        [
            "icvds", "icvgs", "icvbs", "m", "l", "w", "ad", "as", "pd", "ps", "nrd", "nrs", "temp",
            "dtemp",
        ]
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(keyword))
    }
    /// `MOSxparam` then `MOSxtemp`, with the instance schema domains. M, NRD
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
            model: self.model.clone(),
            geometry,
            series,
            initial,
        };
        device.operating(context)?;
        Ok(Box::new(device))
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit(format!(
                "{} AC needs small-signal assembly",
                L::LABEL
            )));
        }
        let op = self.operating(&context.model_context())?;
        let node = self.inner.map(|n| context.node_voltage(n));
        let pol = self.model.common().pol;
        let raw = [node[3] - node[2], node[1] - node[2], node[0] - node[2]].map(|x| pol * x);
        let mut limiter = Limiter::new(&context.states);
        let ic = self.start_conditions(&limiter, |n| context.node_voltage(n));
        let [vbs, vgs, vds] = self.limit(&op, &mut limiter, &context.states, raw, ic);
        // The intrinsic d', g, s', b at the (limited) voltages, relative to
        // s'; every equation below depends on voltage differences only.
        let v = [vds, vgs, 0., vbs].map(|x| pol * x);
        let point = self.point(&op, v, context.gmin)?;
        // MOSconvTest for an `off` instance held in MODEINITFIX: the drain
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
                        SpiceError::circuit(format!(
                            "{}: missing accepted {} state",
                            self.name,
                            L::LABEL
                        ))
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
    /// `mos1acld.c`/`mos3acld.c`: conductances at the bias plus `jω` times the
    /// junction capacitances and the Meyer capacitances `2 half + overlap`.
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

    /// Pole-zero load: C `mos1pzld.c`/`mos3pzld.c` equal the AC load with
    /// `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }

    fn distortion(
        &self,
        context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        L::distortion(self, context)
    }

    fn noise(
        &self,
        context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        L::noise(self, context)
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
            reverse: ReverseLaw::Constant,
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
