//! MOS level 2 (`mos2`): the Grove-Frohman analytical model on the shared
//! [`crate::devices::mos`] shell.
//!
//! C references, read as behaviour only:
//!
//! - `mos2/mos2.c` (`MOS2mPTable`), `mos2mpar.c` (`MOS2mParam`) and
//!   `mos2set.c` (`MOS2setup`): the model setters and defaults;
//! - `mos2/mos2temp.c` (`MOS2temp`): oxide capacitance (TOX defaults to
//!   1e-7 m, so KP is always derived from U0 when not given), process
//!   extraction from NSUB with the fixed intrinsic density `1.45e16 m^-3`,
//!   the depletion width coefficient `XD`, and the temperature scaling of KP,
//!   PHI, VBI, IS/JS, PB and the junction capacitances (as in `mos1temp.c`);
//! - `mos2/mos2load.c` (`MOS2load`, the `moseq2` block): the drain current
//!   with the narrow-width (DELTA) and short-channel (XJ) threshold, weak
//!   inversion (NFS), the UCRIT/UEXP mobility degradation, the Grove-Frohman
//!   saturation voltage, Baum's velocity-saturated saturation voltage (VMAX,
//!   NEFF, the quartic root search), channel-length modulation (LAMBDA, or
//!   derived from XD) and the punch-through limit, with C's derivatives; bulk
//!   junctions with the constant reverse law of `mos1load.c`;
//! - `mos2/mos2acld.c`, `mos2pzld.c`, `mos2trun.c` and `mos2noi.c`, which
//!   equal their level-1 counterparts (effective length `L - 2 LD`, drawn
//!   width `W`).
//!
//! The shell supplies series resistance, junction charges, Meyer gate charge,
//! limiting, the small-signal and pole-zero loads and the instance sweeps.
//!
//! # Deliberate divergences
//!
//! - C warns and continues where the effective length `L - 2 LD` is not
//!   positive and where a value is nonfinite or nonpositive after temperature
//!   scaling; here those are explicit errors, as is `TOX = 0` (an infinite
//!   oxide capacitance in C). UCRIT and NEFF must be positive (C takes a
//!   logarithm and a square root of them), LAMBDA, VMAX, XJ and NFS
//!   nonnegative.
//! - In weak inversion `moseq2` leaves the current, `gm` and `gmbs` of the
//!   previous evaluation in place when the saturation voltage is not positive
//!   while the gate is above the threshold (`goto doneval` without assigning
//!   them). That stale state cannot be reproduced; the port returns no
//!   current there (the case needs NFS with a negative DELTA or a
//!   nonpositive velocity-saturated `vdsat`).
//! - With `GAMMA = 0` and VMAX the velocity-saturation quartic is
//!   biquadratic and one radicand of C's root search is exactly zero; its
//!   computed sign depends on rounding (and on the C compiler's contraction),
//!   and C's `vdsat` becomes NaN at some biases. The port takes a radicand
//!   negative only by rounding as zero ([`rounded_sqrt`]).
//! - `MOS2mPTable`'s computed default of CJ (from NSUB and PB) never reaches
//!   a load, because `mos2temp.c` uses the bottom capacitance only when CJ
//!   was given; the port therefore has no CJ default either.
//! - `.disto` (`mos2dset.c`/`mos2dist.c`) is not ported. DC `.sens` replays
//!   the model records in `sens`; invalid perturbations fail explicitly rather
//!   than continuing with stale temperature fields as C does.

mod sens;

use crate::devices::ModelContext;
use crate::devices::ResolvedModel;
use crate::devices::mos::{
    CELSIUS_TO_KELVIN, CHARGE, Common, DrainCurrent, EPSILON_0, Geometry, Junction, K_OVER_Q,
    MosLevel, Mosfet, NoiseShape, Operating, REFTEMP, ReverseLaw, aliased, band_gap, p,
    potential_factor, required,
};
use crate::devices::noise::NoiseFamily;
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::setup::CDeviceType;
use crate::primitives::{Real, SpiceError, SpiceResult};

/// `EPSSIL` of `mos2temp.c`/`mos2load.c`: the permittivity of silicon (F/m).
const EPSILON_SI: Real = 11.7 * EPSILON_0;

/// Intrinsic carrier density of silicon used by `mos2temp.c` (m^-3).
const INTRINSIC_DENSITY: Real = 1.45e16;

/// The quartic root signs of `mos2load.c` (`sig1`, `sig2`).
const SIG1: [Real; 4] = [1.0, -1.0, 1.0, -1.0];
const SIG2: [Real; 4] = [1.0, 1.0, -1.0, -1.0];

/// The level-2 MOSFET: the shared shell around [`Level2`].
pub(super) type Mos2 = Mosfet<Level2>;

/// Model setters (`MOS2mPTable` as `MOS2mParam` accepts them). Parameters
/// whose *presence* changes C's derivations (`MOS2...Given`) have no schema
/// default; the rest carry `mos2set.c`'s defaults.
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("kp", U::AmperePerVoltSquared, D::Positive, None),
    p("gamma", U::SquareRootVolt, D::NonNegative, None),
    p("phi", U::Volt, D::Positive, None),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.)),
    p("rd", U::Ohm, D::NonNegative, None),
    p("rs", U::Ohm, D::NonNegative, None),
    p("cbd", U::Farad, D::NonNegative, None),
    p("cbs", U::Farad, D::NonNegative, None),
    p("is", U::Ampere, D::Positive, Some(1e-14)),
    p("pb", U::Volt, D::Positive, Some(0.8)),
    p("cgso", U::FaradPerMetre, D::NonNegative, Some(0.)),
    p("cgdo", U::FaradPerMetre, D::NonNegative, Some(0.)),
    p("cgbo", U::FaradPerMetre, D::NonNegative, Some(0.)),
    p("rsh", U::Ohm, D::NonNegative, None),
    p("cj", U::FaradPerSquareMetre, D::NonNegative, None),
    p("mj", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("cjsw", U::FaradPerMetre, D::NonNegative, None),
    p("mjsw", U::Dimensionless, D::NonNegative, Some(0.33)),
    p("js", U::AmperePerSquareMetre, D::NonNegative, Some(0.)),
    p("tox", U::Metre, D::Positive, Some(1e-7)),
    p("ld", U::Metre, D::NonNegative, Some(0.)),
    p("u0", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("uo", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("nsub", U::PerCubicCentimetre, D::Positive, None),
    p("tpg", U::Dimensionless, D::Finite, None),
    p("nss", U::PerSquareCentimetre, D::Finite, None),
    p("delta", U::Dimensionless, D::Finite, Some(0.)),
    p("uexp", U::Dimensionless, D::Finite, Some(0.)),
    p("ucrit", U::VoltPerCentimetre, D::Positive, Some(1e4)),
    p("vmax", U::MetrePerSecond, D::NonNegative, Some(0.)),
    p("xj", U::Metre, D::NonNegative, Some(0.)),
    p("neff", U::Dimensionless, D::Positive, Some(1.)),
    p("nfs", U::PerSquareCentimetre, D::NonNegative, Some(0.)),
    p("tnom", U::Celsius, D::Temperature, None),
    // mos2noi.c; mos2set.c defaults KF = 0, AF = 1, NLEV = 2, GDSNOI = 1.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
    p("nlev", U::Dimensionless, D::Finite, Some(2.)),
    p("gdsnoi", U::Dimensionless, D::Finite, Some(1.)),
];

/// Validated level-2 model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
pub(super) struct Level2 {
    common: Common,
    vto: Option<Real>,
    kp: Option<Real>,
    gamma: Option<Real>,
    phi: Option<Real>,
    lambda: Real,
    tox: Real,
    u0: Option<Real>,
    nsub: Option<Real>,
    tpg: Option<Real>,
    nss: Option<Real>,
    delta: Real,
    uexp: Real,
    ucrit: Real,
    vmax: Real,
    xj: Real,
    neff: Real,
    nfs: Real,
}

/// Process-derived model values (`mos2temp.c`, model loop).
#[derive(Debug, Clone, Copy)]
struct Process {
    cox: Real,
    kp: Real,
    u0: Real,
    phi: Real,
    gamma: Real,
    vto: Real,
    /// C `MOS2xd` (zero without NSUB).
    xd: Real,
}

/// The level-2 channel parameters of one load (`mos2temp.c` and the start
/// of `mos2load.c`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Params {
    /// Effective length `L - 2 LD` and the drawn width.
    length: Real,
    width: Real,
    /// C `Beta = tKP W m / Leff`.
    beta: Real,
    /// C `MOS2oxideCapFactor` and `unscaledOxideCap = Cox Leff W`.
    cox: Real,
    unscaled_oxide: Real,
    /// C `MOS2surfaceMobility` (cm^2/Vs): `mos2load.c` uses the model value,
    /// not the temperature-scaled `MOS2tSurfMob`.
    mobility: Real,
    /// C `MOS2xd`.
    xd: Real,
    /// Whether NSUB was given (C `MOS2substrateDoping != 0`).
    doped: bool,
    /// C `MOS2tBulkPot`, whose square root bounds the punch-through.
    bulk_potential: Real,
}

impl Level2 {
    /// Effective channel length `L - 2 LD`.
    fn length(&self, geometry: &Geometry) -> Real {
        geometry.l - 2. * self.common.ld
    }

    fn process(&self, tnom: Real) -> SpiceResult<Process> {
        let pol = self.common.pol;
        let cox = 3.9 * EPSILON_0 / self.tox;
        let u0 = self.u0.unwrap_or(600.);
        let mut process = Process {
            cox,
            kp: self.kp.unwrap_or(u0 * 1e-4 * cox),
            u0,
            phi: self.phi.unwrap_or(0.6),
            gamma: self.gamma.unwrap_or(0.),
            vto: self.vto.unwrap_or(0.),
            xd: 0.,
        };
        if let Some(nsub) = self.nsub {
            let density = nsub * 1e6;
            if density <= INTRINSIC_DENSITY {
                return Err(SpiceError::circuit(format!(
                    "MOS2 NSUB={nsub} is below the intrinsic density (mos2temp.c: Nsub < Ni)"
                )));
            }
            let vtnom = tnom * K_OVER_Q;
            let egfet1 = band_gap(tnom);
            if self.phi.is_none() {
                process.phi = (2. * vtnom * (density / INTRINSIC_DENSITY).ln()).max(0.1);
            }
            let fermis = pol * 0.5 * process.phi;
            let tpg = self.tpg.unwrap_or(1.);
            let wkfng = if tpg == 0. {
                3.2
            } else {
                let fermig = pol * tpg * 0.5 * egfet1;
                3.25 + 0.5 * egfet1 - fermig
            };
            let wkfngs = wkfng - (3.25 + 0.5 * egfet1 + fermis);
            if self.gamma.is_none() {
                process.gamma = (2. * 11.70 * EPSILON_0 * CHARGE * density).sqrt() / cox;
            }
            if self.vto.is_none() {
                let vfb = wkfngs - self.nss.unwrap_or(0.) * 1e4 * CHARGE / cox;
                process.vto = vfb + pol * (process.gamma * process.phi.sqrt() + process.phi);
            }
            process.xd = ((EPSILON_SI + EPSILON_SI) / (CHARGE * density)).sqrt();
        }
        Ok(process)
    }
}

impl MosLevel for Level2 {
    type Params = Params;
    const LEVEL: u8 = 2;
    const LABEL: &'static str = "MOS2";
    const DEVICE_TYPE: CDeviceType = CDeviceType::Mos2;
    const NOISE_FAMILY: Option<NoiseFamily> = Some(NoiseFamily::Mos2);
    const PRIME_SUFFIXES: [&'static str; 2] = ["internal#drain", "internal#source"];

    fn model_schema() -> ScalarSchema<'static> {
        ScalarSchema { parameters: MODEL }
    }

    fn from_card<'a>(
        resolved: &'a ResolvedModel<'_>,
    ) -> SpiceResult<(Self, Vec<&'a crate::netlist::ast::ParameterAssignment>)> {
        let (m, defaults) = Self::model_and_defaults(resolved)?;
        let get = |name: &str| m.get(name).map(|v| v.value);
        let required = |name: &str| required(&m, name, "MOS2");
        let common = Common::from_values(&m, resolved.family(), get("tpg"), "MOS2", "mos2noi.c")?;
        let model = Self {
            common,
            vto: aliased(&m, ["vto", "vt0"]),
            kp: get("kp"),
            gamma: get("gamma"),
            phi: get("phi"),
            lambda: required("lambda")?,
            tox: required("tox")?,
            u0: aliased(&m, ["u0", "uo"]),
            nsub: get("nsub"),
            tpg: get("tpg"),
            nss: get("nss"),
            delta: required("delta")?,
            uexp: required("uexp")?,
            ucrit: required("ucrit")?,
            vmax: required("vmax")?,
            xj: required("xj")?,
            neff: required("neff")?,
            nfs: required("nfs")?,
        };
        Ok((model, defaults))
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn sensitivity<'a>(
        device: &'a Mos2,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn crate::devices::sensitivity::DeviceSensitivity + 'a>> {
        Ok(Box::new(crate::devices::mos::sens::MosSensitivity {
            device,
            context: *context,
        }))
    }

    /// `mos2temp.c` only warns for a nonpositive effective length; the port
    /// refuses it.
    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()> {
        let length = self.length(geometry);
        if length <= 0. || !length.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS2 effective channel length L - 2*LD must be positive"
            )));
        }
        Ok(())
    }

    /// `mos2temp.c` and the start of `mos2load.c`.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Params>> {
        let model = &self.common;
        let tnom = model.tnom.unwrap_or(context.nominal_temperature) + CELSIUS_TO_KELVIN;
        let temp = geometry.kelvin(context);
        if !(tnom.is_finite() && tnom > 0. && temp.is_finite() && temp > 0.) {
            return Err(SpiceError::circuit(format!(
                "{name}: invalid MOS2 temperature"
            )));
        }
        let process = self.process(tnom)?;
        let vt = temp * K_OVER_Q;
        let vtnom = tnom * K_OVER_Q;
        let (egfet, egfet1) = (band_gap(temp), band_gap(tnom));
        let (fact1, fact2) = (tnom / REFTEMP, temp / REFTEMP);
        let (pbfact, pbfact1) = (potential_factor(temp), potential_factor(tnom));
        let ratio = temp / tnom;
        let ratio4 = ratio * ratio.sqrt();
        let kp = process.kp / ratio4;
        let phio = (process.phi - pbfact1) / fact1;
        let phi = fact2 * phio + pbfact;
        let vbi = process.vto - model.pol * (process.gamma * process.phi.sqrt())
            + 0.5 * (egfet1 - egfet)
            + model.pol * 0.5 * (phi - process.phi);
        let vto = vbi + model.pol * process.gamma * phi.sqrt();
        // mos2temp.c: divide out the nominal-temperature factor, then
        // multiply by the device-temperature one, as two steps.
        let scale = (-egfet / vt + egfet1 / vtnom).exp();
        let (saturation, density) = (model.is * scale, model.js * scale);
        let pbo = (model.pb - pbfact1) / fact1;
        let gmaold = (model.pb - pbo) / pbo;
        let potential = fact2 * pbo + pbfact;
        let gmanew = (potential - pbo) / pbo;
        let factor = |grading: Real| {
            (1. + grading * (4e-4 * (temp - REFTEMP) - gmanew))
                / (1. + grading * (4e-4 * (tnom - REFTEMP) - gmaold))
        };
        let (bottom, side) = (factor(model.mj), factor(model.mjsw));
        let m = geometry.m;
        let length = self.length(geometry);
        let width = geometry.w;
        let bottom_cap = |given: Option<Real>, area: Real| match (given, model.cj) {
            (Some(c), _) => c * bottom * m,
            (None, Some(cj)) => cj * bottom * m * area,
            (None, None) => 0.,
        };
        let side_cap = |perimeter: Real| model.cjsw.map_or(0., |c| c * side * perimeter * m);
        let root2 = std::f64::consts::SQRT_2;
        // mos2temp.c scales the critical voltage by `MOS2m`, as mos1temp.c
        // does.
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
                saturation,
                vcrit,
                bottom,
                sidewall,
                model.mj,
                model.mjsw,
                potential,
                model.fc,
                ReverseLaw::Constant,
            )
        };
        let beta = kp * width * m / length;
        let unscaled_oxide = process.cox * length * width;
        let operating = Operating {
            vt,
            beta,
            vbi,
            phi,
            vto,
            gamma: process.gamma,
            oxide: unscaled_oxide * m,
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
                model.overlap[0] * m * width,
                model.overlap[1] * m * width,
                model.overlap[2] * m * length,
            ],
            params: Params {
                length,
                width,
                beta,
                cox: process.cox,
                unscaled_oxide,
                mobility: process.u0,
                xd: process.xd,
                doped: self.nsub.is_some(),
                bulk_potential: potential,
            },
        };
        let p = &operating.params;
        let finite = [
            operating.vt,
            operating.beta,
            operating.vto,
            p.beta,
            p.unscaled_oxide,
            p.xd,
            operating.vbi,
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
            operating.overlap[0],
            operating.overlap[1],
            operating.overlap[2],
        ]
        .iter()
        .all(|v| v.is_finite());
        if !(finite && phi.is_finite() && phi > 0. && potential.is_finite() && potential > 0.)
            || p.beta <= 0.
            || operating.drain.saturation <= 0.
            || operating.source.saturation <= 0.
            || [bottom, side].iter().any(|f| !f.is_finite() || *f < 0.)
        {
            return Err(SpiceError::circuit(format!(
                "{}: MOS2 parameters are out of range at {:.2} K \
                 (nonfinite value, or nonpositive PHI/PB/KP/IS after temperature scaling)",
                name, temp
            )));
        }
        Ok(operating)
    }

    /// `mos2load.c`'s `moseq2` block, line for line, in the normalized device
    /// frame (`lvgs`, `lvds >= 0`, `lvbs`). The derivatives are C's: exact
    /// except where C's own expressions are approximate (see
    /// `docs/port/M4_NONLINEAR.md`).
    #[allow(clippy::too_many_lines, clippy::similar_names)]
    fn drain_current(
        &self,
        op: &Operating<Params>,
        lvgs: Real,
        lvds: Real,
        lvbs: Real,
    ) -> DrainCurrent {
        let p = &op.params;
        let vt = op.vt;
        let tphi = op.phi;
        let gamma = op.gamma;
        let length = p.length;
        let xd = p.xd;
        let xj = self.xj;
        // `MOS2fastSurfaceStateDensity != 0 && OxideCap != 0`: the oxide
        // capacitance is always positive here.
        let nfs = self.nfs != 0.0;
        let phi_min_vbs = tphi - lvbs;
        // Compute some useful quantities.
        let (sarg1, dsrgdb, d2sdb2) = if lvbs <= 0.0 {
            let sarg1 = phi_min_vbs.sqrt();
            let dsrgdb = -0.5 / sarg1;
            (sarg1, dsrgdb, 0.5 * dsrgdb / phi_min_vbs)
        } else {
            let sphi = tphi.sqrt();
            let sphi3 = tphi * sphi;
            let sarg1 = sphi / (1.0 + 0.5 * lvbs / tphi);
            let tmp = sarg1 / sphi3;
            let dsrgdb = -0.5 * sarg1 * tmp;
            (sarg1, dsrgdb, -dsrgdb * tmp)
        };
        let (mut barg, d2bdb2, dbrgdb) = if (lvbs - lvds) <= 0.0 {
            let barg = (phi_min_vbs + lvds).sqrt();
            let dbrgdb = -0.5 / barg;
            (barg, 0.5 * dbrgdb / (phi_min_vbs + lvds), dbrgdb)
        } else {
            let sphi = tphi.sqrt();
            let sphi3 = tphi * sphi;
            let barg = sphi / (1.0 + 0.5 * (lvbs - lvds) / tphi);
            let tmp = barg / sphi3;
            let dbrgdb = -0.5 * barg * tmp;
            (barg, -dbrgdb * tmp, dbrgdb)
        };
        // Threshold voltage: narrow-channel effect.
        let factor = 0.125 * self.delta * 2.0 * std::f64::consts::PI * EPSILON_SI
            / p.unscaled_oxide
            * length;
        let eta = 1.0 + factor;
        let vbin = op.vbi * self.common.pol + factor * phi_min_vbs;
        let (gamasd, dgddvb, dgdvds, dgddb2);
        if gamma > 0.0 || p.doped {
            let xwd = xd * barg;
            let xws = xd * sarg1;
            // Short-channel effect with vds != 0.
            let (mut argss, mut argsd) = (0.0, 0.0);
            let (mut dbargs, mut dbargd) = (0.0, 0.0);
            let (mut dgdvds_, mut dgddb2_) = (0.0, 0.0);
            let (mut args, mut argd, mut argxs, mut argxd) = (0.0, 0.0, 0.0, 0.0);
            if xj > 0.0 {
                let tmp = 2.0 / xj;
                argxs = 1.0 + xws * tmp;
                argxd = 1.0 + xwd * tmp;
                args = argxs.sqrt();
                argd = argxd.sqrt();
                let tmp = 0.5 * xj / length;
                argss = tmp * (args - 1.0);
                argsd = tmp * (argd - 1.0);
            }
            gamasd = gamma * (1.0 - argss - argsd);
            let dbxwd = xd * dbrgdb;
            let dbxws = xd * dsrgdb;
            if xj > 0.0 {
                let tmp = 0.5 / length;
                dbargs = tmp * dbxws / args;
                dbargd = tmp * dbxwd / argd;
                let dasdb2 = -xd * (d2sdb2 + dsrgdb * dsrgdb * xd / (xj * argxs)) / (length * args);
                let daddb2 = -xd * (d2bdb2 + dbrgdb * dbrgdb * xd / (xj * argxd)) / (length * argd);
                dgddb2_ = -0.5 * gamma * (dasdb2 + daddb2);
            }
            dgddvb = -gamma * (dbargs + dbargd);
            if xj > 0.0 {
                let ddxwd = -dbxwd;
                dgdvds_ = -gamma * 0.5 * ddxwd / (length * argd);
            }
            (dgdvds, dgddb2) = (dgdvds_, dgddb2_);
        } else {
            gamasd = gamma;
            dgddvb = 0.0;
            dgdvds = 0.0;
            dgddb2 = 0.0;
        }
        let mut von = vbin + gamasd * sarg1;
        let mut vdsat = 0.0;
        let off = |von: Real, vdsat: Real, gds: Real| DrainCurrent {
            cdrain: 0.0,
            gm: 0.0,
            gds,
            gmbs: 0.0,
            von,
            vdsat,
        };
        let (mut xn, mut argg) = (0.0, 0.0);
        if nfs {
            let cfs = CHARGE * self.nfs * 1e4;
            let cdonco = -(gamasd * dsrgdb + dgddvb * sarg1) + factor;
            xn = 1.0 + cfs / p.unscaled_oxide * p.width * length + cdonco;
            let tmp = vt * xn;
            von += tmp;
            argg = 1.0 / tmp;
        } else if lvgs <= vbin {
            // Cutoff region.
            return off(von, vdsat, 0.0);
        }
        let vgst = lvgs - von;
        // Compute some more useful quantities.
        let sarg3 = sarg1 * sarg1 * sarg1;
        let sbiarg = p.bulk_potential.sqrt();
        let mut gammad = gamasd;
        let mut dgdvbs = dgddvb;
        let mut body = barg * barg * barg - sarg3;
        let mut gdbdv = 2.0 * gammad * (barg * barg * dbrgdb - sarg1 * sarg1 * dsrgdb);
        let mut dodvbs = -factor + dgdvbs * sarg1 + gammad * dsrgdb;
        let (mut dxndvb, mut dxndvd, mut dodvds) = (0.0, 0.0, 0.0);
        if nfs {
            dxndvb = 2.0 * dgdvbs * dsrgdb + gammad * d2sdb2 + dgddb2 * sarg1;
            dodvbs += vt * dxndvb;
            dxndvd = dgdvds * dsrgdb;
            dodvds = dgdvds * sarg1 + vt * dxndvd;
        }
        // Effective mobility and its derivatives (C `line400`/`line410`).
        let udenom = vgst;
        let critical = self.ucrit * 100.0 * EPSILON_SI / p.cox;
        let (ufact, ueff, dudvgs, dudvds, dudvbs) = if udenom <= critical {
            (1.0, p.mobility * 1e-4, 0.0, 0.0, 0.0)
        } else {
            let ufact = (self.uexp * (critical / udenom).ln()).exp();
            (
                ufact,
                p.mobility * 1e-4 * ufact,
                -ufact * self.uexp / udenom,
                0.0,
                self.uexp * ufact * dodvbs / vgst,
            )
        };
        // Saturation voltage and its derivatives by the Grove-Frohman
        // equation (C `line500`).
        let mut vgsx = lvgs;
        gammad = gamasd / eta;
        dgdvbs = dgddvb;
        if nfs {
            vgsx = c_max(lvgs, von);
        }
        let (mut dsdvgs, mut dsdvbs);
        if gammad > 0.0 {
            let gammd2 = gammad * gammad;
            let argv = (vgsx - vbin) / eta + phi_min_vbs;
            if argv <= 0.0 {
                vdsat = 0.0;
                dsdvgs = 0.0;
                dsdvbs = 0.0;
            } else {
                let arg1 = (1.0 + 4.0 * argv / gammd2).sqrt();
                vdsat = (vgsx - vbin) / eta + gammd2 * (1.0 - arg1) / 2.0;
                vdsat = c_max(vdsat, 0.0);
                dsdvgs = (1.0 - 1.0 / arg1) / eta;
                dsdvbs = (gammad * (1.0 - arg1) + 2.0 * argv / (gammad * arg1)) / eta * dgdvbs
                    + 1.0 / arg1
                    + factor * dsdvgs;
            }
        } else {
            vdsat = (vgsx - vbin) / eta;
            vdsat = c_max(vdsat, 0.0);
            dsdvgs = 1.0;
            dsdvbs = 0.0;
        }
        if self.vmax > 0.0 {
            // Saturation voltage by Baum's theory of scattering-velocity
            // saturation: the smallest positive root of a quartic.
            if let Some(x) = baum_root(BaumInputs {
                v1: (vgsx - vbin) / eta + phi_min_vbs,
                v2: phi_min_vbs,
                xv: self.vmax * length / ueff,
                gammad,
                sarg3,
            }) {
                vdsat = x * x - phi_min_vbs;
            }
        }
        // Effective channel length and its derivatives.
        let mut xlamda = self.lambda;
        let (mut bsarg, mut bodys, mut gdbdvs) = (0.0, 0.0, 0.0);
        // `dldsat` when C reaches the end of the block (not `line610`).
        let mut dldsat = None;
        if lvds != 0.0 {
            gammad = gamasd;
            let dbsrdb = if (lvbs - vdsat) <= 0.0 {
                bsarg = (vdsat + phi_min_vbs).sqrt();
                -0.5 / bsarg
            } else {
                let sphi = tphi.sqrt();
                let sphi3 = tphi * sphi;
                bsarg = sphi / (1.0 + 0.5 * (lvbs - vdsat) / tphi);
                -0.5 * bsarg * bsarg / sphi3
            };
            bodys = bsarg * bsarg * bsarg - sarg3;
            gdbdvs = 2.0 * gammad * (bsarg * bsarg * dbsrdb - sarg1 * sarg1 * dsrgdb);
            // C: `if (substrateDoping == 0.0) goto line610; if (xlamda >
            // 0.0) goto line610;` (LAMBDA is finite and nonnegative).
            let derived = p.doped && xlamda <= 0.0;
            if self.vmax <= 0.0 {
                if derived {
                    let argv = (lvds - vdsat) / 4.0;
                    let sargv = (1.0 + argv * argv).sqrt();
                    let arg1 = (argv + sargv).sqrt();
                    let xlfact = xd / (length * lvds);
                    xlamda = xlfact * arg1;
                    dldsat = Some(lvds * xlamda / (8.0 * sargv));
                }
            } else {
                let argv = (vgsx - vbin) / eta - vdsat;
                let xdv = xd / self.neff.sqrt();
                let xlv = self.vmax * xdv / (2.0 * ueff);
                let vqchan = argv - gammad * bsarg;
                let dqdsat = -1.0 + gammad * dbsrdb;
                let vl = self.vmax * length;
                let dfunds = vl * dqdsat - ueff * vqchan;
                let dfundg = (vl - ueff * vdsat) / eta;
                let dfundb = -vl * (1.0 + dqdsat - factor / eta)
                    + ueff * (gdbdvs - dgdvbs * bodys / 1.5) / eta;
                dsdvgs = -dfundg / dfunds;
                dsdvbs = -dfundb / dfunds;
                if derived {
                    let argv = c_max(lvds - vdsat, 0.0);
                    let xls = (xlv * xlv + argv).sqrt();
                    let xlfact = xdv / (length * lvds);
                    xlamda = xlfact * (xls - xlv);
                    dldsat = Some(xdv / (2.0 * xls) / length);
                }
            }
        }
        let (mut dldvgs, mut dldvds, mut dldvbs) = match dldsat {
            Some(dldsat) => (dldsat * dsdvgs, -xlamda + dldsat, dldsat * dsdvbs),
            None => (0.0, 0.0, 0.0),
        };
        // Limit channel shortening at punch-through.
        let mut xwb = xd * sbiarg;
        let xld = length - xwb;
        let mut clfact = 1.0 - xlamda * lvds;
        dldvds = -xlamda - dldvds;
        let mut xleff = length * clfact;
        let deltal = xlamda * lvds * length;
        if !p.doped {
            xwb = 0.25e-6;
        }
        if xleff < xwb {
            xleff = xwb / (1.0 + (deltal - xld) / xwb);
            clfact = xleff / length;
            let dfact = xleff * xleff / (xwb * xwb);
            dldvgs *= dfact;
            dldvds *= dfact;
            dldvbs *= dfact;
        }
        // Effective beta (effective KP).
        let beta1 = p.beta * ufact / clfact;
        // Test for the mode of operation and branch appropriately.
        gammad = gamasd;
        dgdvbs = dgddvb;
        if lvds <= 1.0e-10 {
            let gds = if lvgs <= von {
                if nfs {
                    beta1 * (von - vbin - gammad * sarg1) * (argg * (lvgs - von)).exp()
                } else {
                    0.0
                }
            } else {
                beta1 * (lvgs - vbin - gammad * sarg1)
            };
            return off(von, vdsat, gds);
        }
        // Without NFS the gate is above `vbin` here (cutoff returned above),
        // which C sends to `line900`.
        if nfs && lvgs <= von {
            // Subthreshold region.
            if vdsat <= 0.0 {
                // C keeps the previous current, gm and gmbs when the gate is
                // above the strong-inversion threshold `vbin + gamasd sarg1`
                // (see the module documentation).
                return off(von, vdsat, 0.0);
            }
            let vdson = c_min(vdsat, lvds);
            if lvds > vdsat {
                barg = bsarg;
                body = bodys;
                gdbdv = gdbdvs;
            }
            let cdson = beta1 * ((von - vbin - eta * vdson * 0.5) * vdson - gammad * body / 1.5);
            let didvds = beta1 * (von - vbin - eta * vdson - gammad * barg);
            let mut gdson = -cdson * dldvds / clfact - beta1 * dgdvds * body / 1.5;
            if lvds < vdsat {
                gdson += didvds;
            }
            let mut gbson = -cdson * dldvbs / clfact
                + beta1 * (dodvbs * vdson + factor * vdson - dgdvbs * body / 1.5 - gdbdv);
            if lvds > vdsat {
                gbson += didvds * dsdvbs;
            }
            let expg = (argg * (lvgs - von)).exp();
            let cdrain = cdson * expg;
            let gmw = cdrain * argg;
            let gm = if lvds > vdsat {
                gmw + didvds * dsdvgs * expg
            } else {
                gmw
            };
            let tmp = gmw * (lvgs - von) / xn;
            return DrainCurrent {
                cdrain,
                gm,
                gds: gdson * expg - gm * dodvds - tmp * dxndvd,
                gmbs: gbson * expg - gm * dodvbs - tmp * dxndvb,
                von,
                vdsat,
            };
        }
        // C `line900`.
        let (cdrain, gm, gds, gmbs) = if lvds <= vdsat {
            // Linear region.
            let cdrain = beta1 * ((lvgs - vbin - eta * lvds / 2.0) * lvds - gammad * body / 1.5);
            let arg1 = cdrain * (dudvgs / ufact - dldvgs / clfact);
            let gm = arg1 + beta1 * lvds;
            let arg1 = cdrain * (dudvds / ufact - dldvds / clfact);
            let gds =
                arg1 + beta1 * (lvgs - vbin - eta * lvds - gammad * barg - dgdvds * body / 1.5);
            let arg1 = cdrain * (dudvbs / ufact - dldvbs / clfact);
            let gmbs = arg1 - beta1 * (gdbdv + dgdvbs * body / 1.5 - factor * lvds);
            (cdrain, gm, gds, gmbs)
        } else {
            // Saturation region.
            let cdrain = beta1 * ((lvgs - vbin - eta * vdsat / 2.0) * vdsat - gammad * bodys / 1.5);
            let arg1 = cdrain * (dudvgs / ufact - dldvgs / clfact);
            let gm = arg1
                + beta1 * vdsat
                + beta1 * (lvgs - vbin - eta * vdsat - gammad * bsarg) * dsdvgs;
            let gds = -cdrain * dldvds / clfact - beta1 * dgdvds * bodys / 1.5;
            let arg1 = cdrain * (dudvbs / ufact - dldvbs / clfact);
            let gmbs = arg1 - beta1 * (gdbdvs + dgdvbs * bodys / 1.5 - factor * vdsat)
                + beta1 * (lvgs - vbin - eta * vdsat - gammad * bsarg) * dsdvbs;
            (cdrain, gm, gds, gmbs)
        };
        DrainCurrent {
            cdrain,
            gm,
            gds,
            gmbs,
            von,
            vdsat,
        }
    }

    /// `mos2noi.c`: the NLEV 3 `beta` is `tKP m W / (L - 2 LD)` (the load's
    /// `Beta`), the flicker laws use `W`, `L - 2 LD` and the model's `Cox`.
    fn noise_shape(&self, geometry: &Geometry, op: &Operating<Params>) -> Option<NoiseShape> {
        let length = self.length(geometry);
        let cox = op.params.cox;
        Some(NoiseShape {
            beta: op.params.beta,
            width: geometry.w,
            length,
            cox,
            cox_squared: cox * cox,
        })
    }
}

/// C's `MAX(a, b)`: `a > b ? a : b`.
fn c_max(a: Real, b: Real) -> Real {
    if a > b { a } else { b }
}

/// C's `MIN(a, b)`: `a < b ? a : b`.
fn c_min(a: Real, b: Real) -> Real {
    if a < b { a } else { b }
}

/// `sqrt(radicand)`, with a radicand that is negative only by rounding
/// (within `1e-12` of the magnitude `scale` of its terms) taken as zero.
///
/// With `GAMMA = 0` the quartic is biquadratic, Ferrari's resolvent root is
/// `y3 = 2 sqrt(d1)` and `y3^2 / 4 - d1` is exactly zero: its computed sign is
/// decided by rounding (in C also by the compiler's floating-point
/// contraction), and a negative one makes C's `vdsat` NaN at some biases.
/// The port treats such a radicand as zero; a genuinely negative one stays a
/// NaN, which the shell reports as a nonfinite channel.
fn rounded_sqrt(radicand: Real, scale: Real) -> Real {
    if radicand < 0.0 && -radicand <= 1e-12 * scale {
        0.0
    } else {
        radicand.sqrt()
    }
}

/// The inputs of `mos2load.c`'s velocity-saturation quartic.
struct BaumInputs {
    v1: Real,
    v2: Real,
    xv: Real,
    gammad: Real,
    sarg3: Real,
}

/// `mos2load.c`'s Baum velocity-saturation root: Ferrari's resolvent cubic
/// (Cardano or the trigonometric form), the four candidate roots of
/// `x^4 + a1 x^3 + b1 x^2 + c1 x + d1`, and the smallest positive one whose
/// residual is at most `1e-6`, or `None` (C keeps the Grove-Frohman `vdsat`).
#[allow(clippy::many_single_char_names, clippy::similar_names)]
fn baum_root(inputs: BaumInputs) -> Option<Real> {
    let BaumInputs {
        v1,
        v2,
        xv,
        gammad,
        sarg3,
    } = inputs;
    let a1 = gammad / 0.75;
    let b1 = -2.0 * (v1 + xv);
    let c1 = -2.0 * gammad * xv;
    let d1 = 2.0 * v1 * (v2 + xv) - v2 * v2 - 4.0 / 3.0 * gammad * sarg3;
    let a = -b1;
    let b = a1 * c1 - 4.0 * d1;
    let c = -d1 * (a1 * a1 - 4.0 * b1) - c1 * c1;
    let r = -a * a / 3.0 + b;
    let s = 2.0 * a * a * a / 27.0 - a * b / 3.0 + c;
    let r3 = r * r * r;
    let s2 = s * s;
    let p = s2 / 4.0 + r3 / 27.0;
    let p0 = p.abs();
    let p2 = p0.sqrt();
    let y3 = if p < 0.0 {
        let ro = (s2 / 4.0 + p0).sqrt();
        let ro = (ro.ln() / 3.0).exp();
        let fi = (-2.0 * p2 / s).atan();
        2.0 * ro * (fi / 3.0).cos() - a / 3.0
    } else {
        let p3 = -s / 2.0 + p2;
        let p3 = (p3.abs().ln() / 3.0).exp();
        let p4 = -s / 2.0 - p2;
        let p4 = (p4.abs().ln() / 3.0).exp();
        p3 + p4 - a / 3.0
    };
    let a3 = rounded_sqrt(a1 * a1 / 4.0 - b1 + y3, a1 * a1 / 4.0 + b1.abs() + y3.abs());
    let b3 = rounded_sqrt(y3 * y3 / 4.0 - d1, y3 * y3 / 4.0 + d1.abs());
    let mut x4 = [0.0; 8];
    let mut iknt = 0;
    for i in 0..4 {
        let a4 = a1 / 2.0 + SIG1[i] * a3;
        let b4 = y3 / 2.0 + SIG2[i] * b3;
        let delta4 = a4 * a4 / 4.0 - b4;
        if delta4 < 0.0 {
            continue;
        }
        let tmp = delta4.sqrt();
        x4[iknt] = -a4 / 2.0 + tmp;
        x4[iknt + 1] = -a4 / 2.0 - tmp;
        iknt += 2;
    }
    let mut valid: Option<Real> = None;
    for &x in &x4[..iknt] {
        if x <= 0.0 {
            continue;
        }
        let poly = x * x * x * x + a1 * x * x * x;
        let poly = poly + b1 * x * x + c1 * x + d1;
        if poly.abs() > 1.0e-6 {
            continue;
        }
        // C: the first accepted root, then any smaller one (`x4 > xvalid`
        // skips, so a NaN root replaces the current choice as in C).
        valid = match valid {
            Some(current) if x > current => Some(current),
            _ => Some(x),
        };
    }
    valid
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A level-2 card without the schema.
    fn level(nfs: Real, vmax: Real, nsub: bool, lambda: Real) -> Level2 {
        Level2 {
            common: Common {
                pol: 1.,
                rd: None,
                rs: None,
                rsh: None,
                cbd: None,
                cbs: None,
                is: 1e-14,
                js: 0.,
                pb: 0.8,
                overlap: [0.; 3],
                cj: None,
                mj: 0.5,
                cjsw: None,
                mjsw: 0.33,
                fc: 0.5,
                ld: 0.1e-6,
                tnom: None,
                kf: 0.,
                af: 1.,
                nlev: 2,
                gdsnoi: 1.,
            },
            vto: Some(0.7),
            kp: Some(50e-6),
            gamma: Some(0.5),
            phi: Some(0.7),
            lambda,
            tox: 25e-9,
            u0: Some(500.),
            nsub: nsub.then_some(2e16),
            tpg: None,
            nss: None,
            delta: 0.5,
            uexp: 0.15,
            ucrit: 2e4,
            vmax,
            xj: 0.3e-6,
            neff: 2.,
            nfs,
        }
    }

    fn geometry() -> Geometry {
        Geometry {
            m: 1.,
            l: 2e-6,
            w: 10e-6,
            ad: 0.,
            as_: 0.,
            pd: 0.,
            ps: 0.,
            nrd: 1.,
            nrs: 1.,
            temp: None,
            dtemp: 0.,
        }
    }

    fn op(level: &Level2) -> Operating<Params> {
        level
            .operating("m1", &geometry(), &ModelContext::default())
            .unwrap()
    }

    #[test]
    fn regions_follow_mos2load() {
        let level = level(0., 0., true, 0.);
        let op = op(&level);
        // Cutoff (no NFS): no current; `von` is the threshold.
        let cut = level.drain_current(&op, 0.1, 1., -0.5);
        assert_eq!([cut.cdrain, cut.gm, cut.gds, cut.gmbs], [0.; 4]);
        assert!(cut.von > 0.1);
        // vds = 0: no current but C's channel conductance.
        let zero = level.drain_current(&op, 2., 0., 0.);
        assert_eq!(zero.cdrain, 0.);
        assert!(zero.gds > 0.);
        // Linear and saturated currents grow with vds; saturation flattens.
        let linear = level.drain_current(&op, 2., 0.1, 0.);
        let saturated = level.drain_current(&op, 2., 3., 0.);
        assert!(linear.cdrain > 0. && saturated.cdrain > linear.cdrain);
        assert!(0.1 < linear.vdsat && linear.vdsat < 3.);
        assert!(saturated.gds < linear.gds);
        // Body bias raises the threshold and lowers the current.
        let biased = level.drain_current(&op, 2., 3., -2.);
        assert!(biased.von > saturated.von && biased.cdrain < saturated.cdrain);
        // Weak inversion with NFS: an exponential tail below von that joins
        // the strong-inversion current at von.
        let weak = super::tests::level(1e11, 0., true, 0.);
        let op_weak = super::tests::op(&weak);
        let below = weak.drain_current(&op_weak, 0.6, 1., 0.);
        let further = weak.drain_current(&op_weak, 0.5, 1., 0.);
        assert!(below.von > 0.6);
        assert!(further.cdrain > 0. && further.cdrain < below.cdrain);
        let von = below.von;
        let (left, right) = (
            weak.drain_current(&op_weak, von - 1e-9, 1., 0.),
            weak.drain_current(&op_weak, von + 1e-9, 1., 0.),
        );
        assert!((left.cdrain - right.cdrain).abs() <= 1e-6 * right.cdrain);
        // Velocity saturation (Baum's quartic) lowers vdsat and the current.
        let fast = super::tests::level(0., 5e4, true, 0.);
        let op_fast = super::tests::op(&fast);
        let limited = fast.drain_current(&op_fast, 2., 3., 0.);
        assert!(limited.vdsat < saturated.vdsat && limited.cdrain < saturated.cdrain);
        // A given LAMBDA replaces the XD-derived channel-length modulation.
        let mut clm = super::tests::level(0., 0., true, 0.05);
        clm.xj = 0.;
        let op_clm = super::tests::op(&clm);
        let (low, high) = (
            clm.drain_current(&op_clm, 2., 3., 0.),
            clm.drain_current(&op_clm, 2., 4., 0.),
        );
        let expected = (1. - 0.05 * 3.) / (1. - 0.05 * 4.);
        assert!((high.cdrain / low.cdrain - expected).abs() < 1e-12);
    }

    #[test]
    fn baum_root_solves_the_velocity_saturation_quartic() {
        let inputs = BaumInputs {
            v1: 2.5,
            v2: 0.7,
            xv: 1.2,
            gammad: 0.4,
            sarg3: 0.7f64.powf(1.5),
        };
        let (a1, b1) = (0.4 / 0.75, -2.0 * (2.5 + 1.2));
        let c1 = -2.0 * 0.4 * 1.2;
        let d1 = 2.0 * 2.5 * (0.7 + 1.2) - 0.49 - 4.0 / 3.0 * 0.4 * 0.7f64.powf(1.5);
        let x = baum_root(inputs).unwrap();
        assert!(x > 0.);
        let poly = |x: Real| x.powi(4) + a1 * x.powi(3) + b1 * x * x + c1 * x + d1;
        assert!(poly(x).abs() < 1e-9);
        // It is the smallest positive root: no sign change below it.
        let mut previous = poly(1e-9);
        let mut t = 1e-9;
        while t < x * (1. - 1e-6) {
            let value = poly(t);
            assert!(value.signum() == previous.signum(), "{t}");
            previous = value;
            t += x / 1000.;
        }
    }

    /// The largest finite-difference mismatch of (gm, gds, gmbs), relative
    /// to `|gm| + |gds| + |gmbs|`, over a bias grid; `skip` excludes points.
    fn worst_mismatch(
        level: &Level2,
        skip: impl Fn(Real, Real, Real, &DrainCurrent) -> bool,
    ) -> Real {
        let op = op(level);
        let current = |vgs, vds, vbs| level.drain_current(&op, vgs, vds, vbs).cdrain;
        let mut worst: Real = 0.;
        for vgs in [0.45, 0.7, 1.0, 1.5, 2.5, 4.] {
            for vds in [0.01, 0.05, 0.4, 1.5, 4.] {
                for vbs in [-2., -0.3, 0.2] {
                    let point = level.drain_current(&op, vgs, vds, vbs);
                    if point.cdrain == 0. || skip(vgs, vds, vbs, &point) {
                        continue;
                    }
                    let h = 1e-6;
                    let fd = |f: &dyn Fn(Real) -> Real| (f(h) - f(-h)) / (2. * h);
                    let numerical = [
                        fd(&|d| current(vgs + d, vds, vbs)),
                        fd(&|d| current(vgs, vds + d, vbs)),
                        fd(&|d| current(vgs, vds, vbs + d)),
                    ];
                    let scale = point.gm.abs() + point.gds.abs() + point.gmbs.abs() + 1e-25;
                    for (analytic, numerical) in
                        [point.gm, point.gds, point.gmbs].into_iter().zip(numerical)
                    {
                        worst = worst.max((analytic - numerical).abs() / scale);
                    }
                }
            }
        }
        worst
    }

    /// A card with the given NFS, VMAX, NSUB, LAMBDA, XJ, DELTA and UEXP.
    #[allow(clippy::too_many_arguments)]
    fn card(
        nfs: Real,
        vmax: Real,
        nsub: bool,
        lambda: Real,
        xj: Real,
        delta: Real,
        uexp: Real,
    ) -> Level2 {
        let mut l = level(nfs, vmax, nsub, lambda);
        (l.xj, l.delta, l.uexp) = (xj, delta, uexp);
        l
    }

    /// C's derivatives are the exact derivatives of C's current wherever its
    /// expressions are exact (zero and reverse body bias, conducting
    /// points): linear and Grove-Frohman saturated strong inversion with the
    /// narrow-width DELTA, UCRIT/UEXP mobility, LAMBDA or the XD-derived
    /// channel-length modulation, XJ without NSUB, Baum's velocity-saturated
    /// `vdsat` (the derivative of the quartic root through `dfund*`), and
    /// strong inversion of an NFS device without mobility degradation.
    #[test]
    fn derivatives_match_finite_differences_where_c_is_exact() {
        for (l, linear_only, strong_only) in [
            (card(0., 0., false, 0.03, 0.3e-6, 0.5, 0.15), false, false),
            (card(0., 0., true, 0., 0., 0.5, 0.), false, false),
            (card(0., 0., true, 0., 0., 0., 0.15), false, false),
            (card(0., 5e4, false, 0., 0.3e-6, 0., 0.), false, false),
            (card(0., 5e4, true, 0.03, 0., 0., 0.), false, false),
            (card(0., 5e4, false, 0., 0., 0.5, 0.15), true, false),
            (card(1e11, 0., true, 0.03, 0., 0.5, 0.), false, true),
            (card(1e11, 5e4, false, 0., 0., 0.5, 0.), true, true),
        ] {
            let worst = worst_mismatch(&l, |vgs, vds, vbs, point| {
                vbs > 0. || (linear_only && vds > point.vdsat) || (strong_only && vgs <= point.von)
            });
            assert!(worst <= 1e-6, "{l:?}: {worst:e}");
        }
    }

    /// Where `mos2load.c`'s own derivatives are approximate, the port keeps
    /// them (AC and pole-zero parity use C's `gm`/`gds`/`gmbs`; the Newton
    /// solution does not depend on them): the XJ short-channel factor with
    /// NSUB (`dgdvds`/`dgddb2` drop terms), velocity saturation with DELTA
    /// or UEXP beyond `vdsat`, the XD-derived channel-length modulation with
    /// VMAX in the linear region (C differentiates `xls` although the
    /// clamped `vds - vdsat` is constant there), and weak inversion (NFS),
    /// whose `dxndvb` has the opposite sign of the derivative of `xn` and
    /// which ignores the dependence of `vdsat` on `vds` beyond it. The
    /// mismatch stays bounded on the reverse-bias grid.
    #[test]
    fn c_approximate_derivatives_stay_bounded() {
        for l in [
            card(0., 0., true, 0., 0.3e-6, 0., 0.),
            card(0., 5e4, false, 0., 0., 0.5, 0.15),
            card(0., 5e4, true, 0., 0., 0., 0.),
            card(1e11, 0., false, 0., 0., 0., 0.),
            card(1e11, 0., true, 0.03, 0., 0.5, 0.),
            card(1e11, 5e4, true, 0., 0.3e-6, 0.5, 0.15),
        ] {
            let worst = worst_mismatch(&l, |_, _, vbs, _| vbs > 0.);
            assert!((1e-6..0.5).contains(&worst), "{l:?}: {worst:e}");
        }
    }

    /// Below the threshold without NFS (`vbin < vgs <= von`) C runs the
    /// saturation formulas at `vdsat = 0`: no current, but a negative `gm`
    /// from the unclamped `dsdvgs`, which the port keeps.
    #[test]
    fn subthreshold_without_nfs_keeps_cs_negative_transconductance() {
        let l = level(0., 0., false, 0.);
        let op = op(&l);
        let point = l.drain_current(&op, 0.45, 1., -2.);
        assert_eq!(point.cdrain, 0.);
        assert_eq!(point.vdsat, 0.);
        assert!(point.gm < 0. && point.von > 0.45);
    }

    /// With GAMMA = 0 and VMAX the quartic is biquadratic and one radicand
    /// of the root search is zero up to rounding: the saturation voltage
    /// stays finite over the whole bias grid (C turns some of these points
    /// into NaN, depending on its rounding), and still solves the quartic.
    #[test]
    fn biquadratic_velocity_saturation_stays_finite() {
        let mut l = card(1e11, 5e4, false, 0., 0., 0.4, 0.);
        l.gamma = None;
        let op = op(&l);
        for i in 0..50 {
            for j in 1..50 {
                for vbs in [-1., -0.2, 0., 0.3] {
                    let (vgs, vds) = (Real::from(i) * 0.1, Real::from(j) * 0.1);
                    let point = l.drain_current(&op, vgs, vds, vbs);
                    assert!(
                        [point.cdrain, point.gm, point.gds, point.gmbs, point.vdsat]
                            .iter()
                            .all(|v| v.is_finite()),
                        "{vgs} {vds} {vbs}: {point:?}"
                    );
                }
            }
        }
        assert_eq!(rounded_sqrt(-1e-15, 5.), 0.);
        assert!(rounded_sqrt(-1e-3, 5.).is_nan());
        assert_eq!(rounded_sqrt(4., 5.), 2.);
    }
}
