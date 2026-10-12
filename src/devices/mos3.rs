//! MOS level 3 (`mos3`): the semi-empirical short-channel model on the shared
//! [`crate::devices::mos`] shell.
//!
//! C references, read as behaviour only:
//!
//! - `mos3/mos3.c` (`MOS3mPTable`), `mos3mpar.c` (`MOS3mParam`) and
//!   `mos3set.c` (`MOS3setup`): the model setters and defaults;
//! - `mos3/mos3temp.c` (`MOS3temp`): oxide capacitance (TOX defaults to
//!   1e-7 m, so KP is always derived from U0 when not given), process
//!   extraction from NSUB with the TNOM-adjusted intrinsic density, the
//!   depletion coefficient `alpha` and the narrow-width factor, DELVTO, and
//!   the temperature scaling of KP, U0, PHI, VBI, IS/JS, PB and the junction
//!   capacitances;
//! - `mos3/mos3load.c` (`MOS3load`, the `moseq3` block): the drain current
//!   with the short-channel (XJ), narrow-width (DELTA) and static-feedback
//!   (ETA) threshold, weak inversion (NFS), mobility modulation (THETA),
//!   velocity saturation (VMAX) and channel-length modulation (KAPPA), with
//!   C's derivatives; bulk junctions with the cubic reverse law;
//! - `mos3/mos3acld.c`, `mos3pzld.c`, `mos3trun.c` and `mos3noi.c`, which
//!   equal their level-1 counterparts up to the effective width
//!   `W - 2 WD + XW` and length `L - 2 LD + XL`.
//!
//! The shell supplies the rest of `mos3load.c` (identical to `mos1load.c`
//! up to the effective geometry and the cubic reverse junction law): series
//! resistance, junction charges, Meyer gate charge (with `Cox` over the
//! effective channel area), limiting, the stamps, the small-signal and
//! pole-zero loads and the instance sweeps, operation by operation.
//!
//! # Deliberate divergences
//!
//! - C warns and continues where a value is nonfinite or nonpositive after
//!   temperature scaling; here those are explicit errors, as are `TOX = 0`
//!   (an infinite oxide capacitance in C) and an effective length or width
//!   that C rejects with `E_PARMVAL`.
//! - `MOS3mPTable` lists `XD`, `ALPHA` and `INPUT_DELTA` as settable, but
//!   `MOS3mParam` has no case for them (`E_BADPARM`); they are rejected as
//!   unknown setters. `.options badmos3` is rejected by the options layer, so
//!   C's default (`CKTbadMos3 = 0`) formulation is the only one.
//! - `.disto` (`mos3dset.c`/`mos3dist.c`) and `.sens` are not ported and fail
//!   explicitly.

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
use crate::devices::{ModelContext, ResolvedModel};
use crate::primitives::{Real, SpiceError, SpiceResult};

/// `EPSSIL` of `mos3temp.c`: the permittivity of silicon (F/m), as C's
/// macro `(11.7 * 8.854214871e-12)`.
const EPSILON_SI: Real = 11.7 * EPSILON_0;

/// The level-3 MOSFET: the shared shell around [`Level3`].
pub(super) type Mos3 = Mosfet<Level3>;

/// Model setters (`MOS3mPTable` as `MOS3mParam` accepts them). Parameters
/// whose *presence* changes C's derivations (`MOS3...Given`) have no schema
/// default; the rest carry `mos3set.c`'s defaults.
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("kp", U::AmperePerVoltSquared, D::Positive, None),
    p("gamma", U::SquareRootVolt, D::NonNegative, None),
    p("phi", U::Volt, D::Positive, None),
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
    p("xl", U::Metre, D::Finite, Some(0.)),
    p("wd", U::Metre, D::Finite, Some(0.)),
    p("xw", U::Metre, D::Finite, Some(0.)),
    p("delvto", U::Volt, D::Finite, None),
    p("delvt0", U::Volt, D::Finite, None),
    p("u0", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("uo", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("nsub", U::PerCubicCentimetre, D::Positive, None),
    p("tpg", U::Dimensionless, D::Finite, None),
    p("nss", U::PerSquareCentimetre, D::Finite, None),
    p("vmax", U::MetrePerSecond, D::NonNegative, Some(0.)),
    p("xj", U::Metre, D::NonNegative, Some(0.)),
    p("nfs", U::PerSquareCentimetre, D::NonNegative, Some(0.)),
    p("eta", U::Dimensionless, D::Finite, Some(0.)),
    p("delta", U::Dimensionless, D::Finite, Some(0.)),
    p("theta", U::InverseVolt, D::Finite, Some(0.)),
    p("kappa", U::Dimensionless, D::NonNegative, Some(0.2)),
    p("tnom", U::Celsius, D::Temperature, None),
    // mos3noi.c; mos3set.c defaults KF = 0, AF = 1, NLEV = 2, GDSNOI = 1.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
    p("nlev", U::Dimensionless, D::Finite, Some(2.)),
    p("gdsnoi", U::Dimensionless, D::Finite, Some(1.)),
];

/// Validated level-3 model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
pub(super) struct Level3 {
    common: Common,
    vto: Option<Real>,
    kp: Option<Real>,
    gamma: Option<Real>,
    phi: Option<Real>,
    tox: Real,
    xl: Real,
    wd: Real,
    xw: Real,
    delvto: Real,
    u0: Option<Real>,
    nsub: Option<Real>,
    tpg: Option<Real>,
    nss: Option<Real>,
    vmax: Real,
    xj: Real,
    nfs: Real,
    eta: Real,
    delta: Real,
    theta: Real,
    kappa: Real,
}

/// Process-derived model values (`mos3temp.c`, model loop).
#[derive(Debug, Clone, Copy)]
struct Process {
    cox: Real,
    kp: Real,
    u0: Real,
    phi: Real,
    gamma: Real,
    vto: Real,
    /// C `MOS3alpha` and `MOS3coeffDepLayWidth` (zero without NSUB).
    alpha: Real,
    depletion: Real,
    /// C `MOS3narrowFactor`.
    narrow: Real,
}

/// The level-3 channel parameters of one load (`mos3temp.c` and the start
/// of `mos3load.c`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Params {
    /// C `EffectiveLength = l - 2 LD + XL` and `EffectiveWidth = w - 2 WD + XW`.
    length: Real,
    width: Real,
    /// C `MOS3tTransconductance`.
    kp: Real,
    /// C `MOS3tSurfMob` (cm^2/Vs).
    mobility: Real,
    /// C's length-scaled `eta`: `ETA * 8.15e-22 / (Cox Leff^3)`.
    eta: Real,
    narrow: Real,
    alpha: Real,
    depletion: Real,
    /// C `csonco`: the NFS fast-surface-state term (zero without NFS).
    csonco: Real,
}

impl Level3 {
    /// C `EffectiveLength` and `EffectiveWidth` (`mos3load.c`).
    fn effective(&self, geometry: &Geometry) -> (Real, Real) {
        (
            geometry.l - 2. * self.common.ld + self.xl,
            geometry.w - 2. * self.wd + self.xw,
        )
    }

    /// `mos3temp.c`'s model loop, in C's arithmetic.
    fn process(&self, tnom: Real) -> SpiceResult<Process> {
        let pol = self.common.pol;
        let vtnom = tnom * K_OVER_Q;
        let egfet1 = band_gap(tnom);
        let nifact = (tnom / 300.) * (tnom / 300.).sqrt();
        let nifact = nifact * (0.5 * egfet1 * ((1. / 300.) - (1. / tnom)) / K_OVER_Q).exp();
        let intrinsic = 1.45e16 * nifact;
        let cox = 3.9 * EPSILON_0 / self.tox;
        let u0 = self.u0.unwrap_or(600.);
        let mut process = Process {
            cox,
            kp: self.kp.unwrap_or(u0 * cox * 1e-4),
            u0,
            phi: self.phi.unwrap_or(0.6),
            gamma: self.gamma.unwrap_or(0.),
            vto: self.vto.unwrap_or(0.),
            alpha: 0.,
            depletion: 0.,
            narrow: 0.,
        };
        if let Some(nsub) = self.nsub {
            if nsub * 1e6 <= intrinsic {
                return Err(SpiceError::circuit(format!(
                    "MOS3 NSUB={nsub} is below the intrinsic density (mos3temp.c: Nsub < Ni)"
                )));
            }
            if self.phi.is_none() {
                process.phi = (2. * vtnom * (nsub * 1e6 / intrinsic).ln()).max(0.1);
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
                // C's left-to-right product, `nsub * 1e6` last.
                process.gamma = (2. * EPSILON_SI * CHARGE * nsub * 1e6).sqrt() / cox;
            }
            if self.vto.is_none() {
                let vfb = wkfngs - self.nss.unwrap_or(0.) * 1e4 * CHARGE / cox;
                process.vto = vfb + pol * (process.gamma * process.phi.sqrt() + process.phi);
            }
            process.alpha = (EPSILON_SI + EPSILON_SI) / (CHARGE * nsub * 1e6);
            process.depletion = process.alpha.sqrt();
        }
        process.narrow = self.delta * 0.5 * std::f64::consts::PI * EPSILON_SI / cox;
        Ok(process)
    }
}

impl MosLevel for Level3 {
    type Params = Params;
    const LEVEL: u8 = 3;
    const LABEL: &'static str = "MOS3";
    const DEVICE_TYPE: CDeviceType = CDeviceType::Mos3;
    const NOISE_FAMILY: NoiseFamily = NoiseFamily::Mos3;
    /// `mos3set.c`: `CKTmkVolt(..., "internal#drain")` / `"internal#source"`.
    const PRIME_SUFFIXES: [&'static str; 2] = ["internal#drain", "internal#source"];

    fn from_card(resolved: &ResolvedModel<'_>) -> SpiceResult<Self> {
        let m = resolved.parameters(&ScalarSchema { parameters: MODEL })?;
        let get = |name: &str| m.get(name).map(|v| v.value);
        let required = |name: &str| required(&m, name, "MOS3");
        let tpg = get("tpg");
        Ok(Self {
            common: Common::from_values(&m, resolved.family(), tpg, "MOS3", "mos3noi.c")?,
            vto: aliased(&m, ["vto", "vt0"]),
            kp: get("kp"),
            gamma: get("gamma"),
            phi: get("phi"),
            tox: required("tox")?,
            xl: required("xl")?,
            wd: required("wd")?,
            xw: required("xw")?,
            delvto: aliased(&m, ["delvto", "delvt0"]).unwrap_or(0.),
            u0: aliased(&m, ["u0", "uo"]),
            nsub: get("nsub"),
            tpg,
            nss: get("nss"),
            vmax: required("vmax")?,
            xj: required("xj")?,
            nfs: required("nfs")?,
            eta: required("eta")?,
            delta: required("delta")?,
            theta: required("theta")?,
            kappa: required("kappa")?,
        })
    }

    fn common(&self) -> &Common {
        &self.common
    }

    /// `mos3temp.c` refuses (`E_PARMVAL`) a nonpositive effective length or
    /// width.
    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()> {
        let (length, width) = self.effective(geometry);
        if length <= 0. || !length.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS3 effective channel length L - 2*LD + XL must be positive"
            )));
        }
        if width <= 0. || !width.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS3 effective channel width W - 2*WD + XW must be positive"
            )));
        }
        Ok(())
    }

    /// `mos3temp.c`'s instance loop and the start of `mos3load.c`, in C's
    /// arithmetic.
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
                "{name}: invalid MOS3 temperature"
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
        let mobility = process.u0 / ratio4;
        let phio = (process.phi - pbfact1) / fact1;
        let phi = fact2 * phio + pbfact;
        let vbi = self.delvto + process.vto - model.pol * (process.gamma * process.phi.sqrt())
            + 0.5 * (egfet1 - egfet)
            + model.pol * 0.5 * (phi - process.phi);
        let vto = vbi + model.pol * process.gamma * phi.sqrt();
        let scale = (-egfet / vt + egfet1 / vtnom).exp();
        let (saturation, density) = (model.is * scale, model.js * scale);
        let pbo = (model.pb - pbfact1) / fact1;
        let gmaold = (model.pb - pbo) / pbo;
        let potential = fact2 * pbo + pbfact;
        let gmanew = (potential - pbo) / pbo;
        // mos3temp.c: divide out the nominal-temperature factor, then
        // multiply by the device-temperature one, as two steps.
        let nominal = |grading: Real| 1. / (1. + grading * (4e-4 * (tnom - REFTEMP) - gmaold));
        let actual = |grading: Real| 1. + grading * (4e-4 * (temp - REFTEMP) - gmanew);
        let (bottom, side) = (
            [nominal(model.mj), actual(model.mj)],
            [nominal(model.mjsw), actual(model.mjsw)],
        );
        let m = geometry.m;
        let (length, width) = self.effective(geometry);
        // MOS3tCbd/MOS3tCbs/MOS3tCj and MOS3tCjsw; mos3temp.c multiplies
        // `tCj * area * m` (mos1temp.c: `tCj * m * area`).
        let scaled = |c: Real, factor: [Real; 2]| c * factor[0] * factor[1];
        let bottom_cap = |given: Option<Real>, area: Real| match (given, model.cj) {
            (Some(c), _) => scaled(c, bottom) * m,
            (None, Some(cj)) => scaled(cj, bottom) * area * m,
            (None, None) => 0.,
        };
        let side_cap = |perimeter: Real| model.cjsw.map_or(0., |c| scaled(c, side) * perimeter * m);
        let root2 = std::f64::consts::SQRT_2;
        // mos3temp.c tests the model's JS for the critical voltages,
        // mos3load.c the scaled density for the saturation currents.
        let (vcrit_d, vcrit_s) = if model.js == 0. || geometry.ad == 0. || geometry.as_ == 0. {
            let vcrit = vt * (vt / (root2 * m * saturation)).ln();
            (vcrit, vcrit)
        } else {
            (
                vt * (vt / (root2 * m * density * geometry.ad)).ln(),
                vt * (vt / (root2 * m * density * geometry.as_)).ln(),
            )
        };
        let (sat_d, sat_s) = if density == 0. || geometry.ad == 0. || geometry.as_ == 0. {
            (m * saturation, m * saturation)
        } else {
            (m * density * geometry.ad, m * density * geometry.as_)
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
                ReverseLaw::Cubic,
            )
        };
        let oxide = process.cox * length * m * width;
        let csonco = if self.nfs == 0. {
            0.
        } else {
            CHARGE * self.nfs * 1e4 * length * width * m / oxide
        };
        let operating = Operating {
            vt,
            beta: kp * m * width / length,
            vbi,
            phi,
            vto,
            gamma: process.gamma,
            oxide,
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
                kp,
                mobility,
                eta: self.eta * 8.15e-22 / (process.cox * length * length * length),
                narrow: process.narrow,
                alpha: process.alpha,
                depletion: process.depletion,
                csonco,
            },
        };
        let p = &operating.params;
        operating.check(
            name,
            "MOS3",
            temp,
            &[p.mobility, p.eta, p.narrow, p.alpha, p.depletion, p.csonco],
            &[bottom[0], bottom[1], side[0], side[1]],
        )?;
        if p.mobility <= 0. {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS3 U0 is nonpositive after temperature scaling"
            )));
        }
        Ok(operating)
    }

    /// `mos3load.c`'s `moseq3` block, line for line, in the normalized device
    /// frame. The derivatives are C's: exact except where C's own expressions
    /// are approximate (see `docs/port/M4_NONLINEAR.md`).
    #[allow(clippy::too_many_lines, clippy::similar_names)]
    fn drain_current(
        &self,
        op: &Operating<Params>,
        vgs: Real,
        vds: Real,
        vbs: Real,
    ) -> DrainCurrent {
        // C's `(mode == 1 ? vgs : vgd)`, `mode * vds` and `(mode == 1 ? vbs :
        // vbd)`, named as in the normal mode.
        const COEFF0: Real = 0.0631353e0;
        const COEFF1: Real = 0.8013292e0;
        const COEFF2: Real = -0.01110777e0;
        let p = &op.params;
        let (length, width) = (p.length, p.width);
        let vt = op.vt;
        let phi = op.phi;
        let mut vdsat = 0.0;
        let oneoverxl = 1.0 / length;
        let eta = p.eta;
        // Square root term.
        let (phibs, sqphbs, dsqdvb);
        if vbs <= 0.0 {
            phibs = phi - vbs;
            sqphbs = phibs.sqrt();
            dsqdvb = -0.5 / sqphbs;
        } else {
            let sqphis = phi.sqrt();
            let sqphs3 = phi * sqphis;
            sqphbs = sqphis / (1.0 + vbs / (phi + phi));
            phibs = sqphbs * sqphbs;
            dsqdvb = -phibs / (sqphs3 + sqphs3);
        }
        // Short channel effect factor.
        let (fshort, dfsdvb) = if self.xj != 0.0 && p.depletion != 0.0 {
            let wps = p.depletion * sqphbs;
            let oneoverxj = 1.0 / self.xj;
            let xjonxl = self.xj * oneoverxl;
            let djonxj = self.common.ld * oneoverxj;
            let wponxj = wps * oneoverxj;
            let wconxj = COEFF0 + COEFF1 * wponxj + COEFF2 * wponxj * wponxj;
            let arga = wconxj + djonxj;
            let argc = wponxj / (1.0 + wponxj);
            let argb = (1.0 - argc * argc).sqrt();
            let fshort = 1.0 - xjonxl * (arga * argb - djonxj);
            let dwpdvb = p.depletion * dsqdvb;
            let dadvb = (COEFF1 + COEFF2 * (wponxj + wponxj)) * dwpdvb * oneoverxj;
            let dbdvb = -argc * argc * (1.0 - argc) * dwpdvb / (argb * wps);
            (fshort, -xjonxl * (dadvb * argb + arga * dbdvb))
        } else {
            (1.0, 0.0)
        };
        // Body effect.
        let gamma = op.gamma;
        let gammas = gamma * fshort;
        let fbodys = 0.5 * gammas / (sqphbs + sqphbs);
        let fbody = fbodys + p.narrow / width;
        let onfbdy = 1.0 / (1.0 + fbody);
        let dfbdvb = -fbodys * dsqdvb / sqphbs + fbodys * dfsdvb / fshort;
        let qbonco = gammas * sqphbs + p.narrow * phibs / width;
        let dqbdvb = gammas * dsqdvb + gamma * dfsdvb * sqphbs - p.narrow / width;
        // Static feedback effect.
        let vbix = op.vbi * self.common.pol - eta * vds;
        // Threshold voltage.
        let vth = vbix + qbonco;
        let dvtdvd = -eta;
        let dvtdvb = dqbdvb;
        // Joint weak inversion and strong inversion.
        let mut von = vth;
        let (mut xn, mut dxndvb, mut dvodvd, mut dvodvb) = (0.0, 0.0, 0.0, 0.0);
        if self.nfs != 0.0 {
            let cdonco = qbonco / (phibs + phibs);
            xn = 1.0 + p.csonco + cdonco;
            von = vth + vt * xn;
            dxndvb = dqbdvb / (phibs + phibs) - qbonco * dsqdvb / (phibs * sqphbs);
            dvodvd = dvtdvd;
            dvodvb = dvtdvb + vt * dxndvb;
        } else if vgs <= von {
            // Cutoff region.
            return DrainCurrent {
                cdrain: 0.0,
                gm: 0.0,
                gds: 0.0,
                gmbs: 0.0,
                von,
                vdsat,
            };
        }
        // Device is on.
        let vgsx = vgs.max(von);
        // Mobility modulation by gate voltage.
        let onfg = 1.0 + self.theta * (vgsx - vth);
        let fgate = 1.0 / onfg;
        let us = p.mobility * 1e-4 * fgate;
        let dfgdvg = -self.theta * fgate * fgate;
        let dfgdvd = -dfgdvg * dvtdvd;
        let dfgdvb = -dfgdvg * dvtdvb;
        // Saturation voltage.
        vdsat = (vgsx - vth) * onfbdy;
        let (dvsdvg, dvsdvd, dvsdvb, onvdsc);
        if self.vmax <= 0.0 {
            dvsdvg = onfbdy;
            dvsdvd = -dvsdvg * dvtdvd;
            dvsdvb = -dvsdvg * dvtdvb - vdsat * dfbdvb * onfbdy;
            onvdsc = 0.0;
        } else {
            let vdsc = length * self.vmax / us;
            onvdsc = 1.0 / vdsc;
            let arga = (vgsx - vth) * onfbdy;
            let argb = (arga * arga + vdsc * vdsc).sqrt();
            vdsat = arga + vdsc - argb;
            let dvsdga = (1.0 - arga / argb) * onfbdy;
            dvsdvg = dvsdga - (1.0 - vdsc / argb) * vdsc * dfgdvg * onfg;
            dvsdvd = -dvsdvg * dvtdvd;
            dvsdvb = -dvsdvg * dvtdvb - arga * dvsdga * dfbdvb;
        }
        // Current factors in the linear region.
        let vdsx = vds.min(vdsat);
        let mut beta = op.beta;
        if vdsx == 0.0 {
            // Special case of vds = 0.
            beta *= fgate;
            let mut gds = beta * (vgsx - vth);
            if self.nfs != 0.0 && vgs < von {
                gds *= ((vgs - von) / (vt * xn)).exp();
            }
            return DrainCurrent {
                cdrain: 0.0,
                gm: 0.0,
                gds,
                gmbs: 0.0,
                von,
                vdsat,
            };
        }
        let cdo = vgsx - vth - 0.5 * (1.0 + fbody) * vdsx;
        let dcodvb = -dvtdvb - 0.5 * dfbdvb * vdsx;
        // Normalized drain current.
        let cdnorm = cdo * vdsx;
        let mut gm = vdsx;
        let mut gds = if vds > vdsat {
            -dvtdvd * vdsx
        } else {
            vgsx - vth - (1.0 + fbody + dvtdvd) * vdsx
        };
        let mut gmbs = dcodvb * vdsx;
        // Drain current without velocity saturation effect.
        let cd1 = beta * cdnorm;
        beta *= fgate;
        let mut cdrain = beta * cdnorm;
        gm = beta * gm + dfgdvg * cd1;
        gds = beta * gds + dfgdvd * cd1;
        gmbs = beta * gmbs + dfgdvb * cd1;
        // Velocity saturation factor.
        let (mut fdrain, mut dfddvg, mut dfddvd, mut dfddvb) = (0.0, 0.0, 0.0, 0.0);
        if self.vmax > 0.0 {
            fdrain = 1.0 / (1.0 + vdsx * onvdsc);
            let fd2 = fdrain * fdrain;
            let arga = fd2 * vdsx * onvdsc * onfg;
            dfddvg = -dfgdvg * arga;
            dfddvd = if vds > vdsat {
                -dfgdvd * arga
            } else {
                -dfgdvd * arga - fd2 * onvdsc
            };
            dfddvb = -dfgdvb * arga;
            // Drain current.
            gm = fdrain * gm + dfddvg * cdrain;
            gds = fdrain * gds + dfddvd * cdrain;
            gmbs = fdrain * gmbs + dfddvb * cdrain;
            cdrain *= fdrain;
        }
        // Channel length modulation: `delxl` and its derivatives (C's
        // `line520` inputs), or `None` for C's `goto line700`.
        let (kappa, alpha) = (self.kappa, p.alpha);
        let modulation = if vds <= vdsat {
            if self.vmax > 0.0 || alpha == 0.0 {
                None
            } else {
                let mut arga = vds / vdsat;
                let mut delxl = (kappa * alpha * vdsat / 8.).sqrt();
                let dldvd = 4. * delxl * arga * arga * arga / vdsat;
                arga *= arga;
                arga *= arga;
                delxl *= arga;
                Some((delxl, dldvd, 0.0, -dldvd, 0.0))
            }
        } else if self.vmax <= 0.0 {
            // C `line510`.
            let delxl = (kappa * alpha * (vds - vdsat + (vdsat / 8.))).sqrt();
            let dldvd = 0.5 * delxl / (vds - vdsat + (vdsat / 8.));
            Some((delxl, dldvd, 0.0, -dldvd, 0.0))
        } else if alpha == 0.0 {
            None
        } else {
            let cdsat = cdrain;
            let gdsat = (cdsat * (1.0 - fdrain) * onvdsc).max(1.0e-12);
            let gdoncd = gdsat / cdsat;
            let gdonfd = gdsat / (1.0 - fdrain);
            let gdonfg = gdsat * onfg;
            let dgdvg = gdoncd * gm - gdonfd * dfddvg + gdonfg * dfgdvg;
            let dgdvd = gdoncd * gds - gdonfd * dfddvd + gdonfg * dfgdvd;
            let dgdvb = gdoncd * gmbs - gdonfd * dfddvb + gdonfg * dfgdvb;
            let emax = kappa * cdsat * oneoverxl / gdsat;
            let emoncd = emax / cdsat;
            let emongd = emax / gdsat;
            let demdvg = emoncd * gm - emongd * dgdvg;
            let demdvd = emoncd * gds - emongd * dgdvd;
            let demdvb = emoncd * gmbs - emongd * dgdvb;
            let arga = 0.5 * emax * alpha;
            let argc = kappa * alpha;
            let argb = (arga * arga + argc * (vds - vdsat)).sqrt();
            let delxl = argb - arga;
            let (dldvd, dldem) = if argb != 0.0 {
                (argc / (argb + argb), 0.5 * (arga / argb - 1.0) * alpha)
            } else {
                (0.0, 0.0)
            };
            Some((
                delxl,
                dldvd,
                dldem * demdvg,
                dldem * demdvd - dldvd,
                dldem * demdvb,
            ))
        };
        let mut gds0 = 0.0;
        if let Some((mut delxl, mut dldvd, mut ddldvg, mut ddldvd, mut ddldvb)) = modulation {
            // Punch through approximation (C `line520`).
            if delxl > (0.5 * length) {
                delxl = length - (length * length / (4.0 * delxl));
                let arga = 4.0 * (length - delxl) * (length - delxl) / (length * length);
                ddldvg *= arga;
                ddldvd *= arga;
                ddldvb *= arga;
                dldvd *= arga;
            }
            // Saturation region.
            let dlonxl = delxl * oneoverxl;
            let xlfact = 1.0 / (1.0 - dlonxl);
            cdrain *= xlfact;
            let diddl = cdrain / (length - delxl);
            gm = gm * xlfact + diddl * ddldvg;
            gmbs = gmbs * xlfact + diddl * ddldvb;
            gds0 = diddl * ddldvd;
            gm += gds0 * dvsdvg;
            gmbs += gds0 * dvsdvb;
            gds = gds * xlfact + diddl * dldvd + gds0 * dvsdvd;
        }
        // Finish the strong inversion case (C `line700`).
        if vgs < von {
            // Weak inversion.
            let onxn = 1.0 / xn;
            let ondvt = onxn / vt;
            let wfact = ((vgs - von) * ondvt).exp();
            cdrain *= wfact;
            let gms = gm * wfact;
            let gmw = cdrain * ondvt;
            gm = gmw;
            if vds > vdsat {
                gm += gds0 * dvsdvg * wfact;
            }
            gds = gds * wfact + (gms - gmw) * dvodvd;
            gmbs = gmbs * wfact + (gms - gmw) * dvodvb - gmw * (vgs - von) * onxn * dxndvb;
        }
        DrainCurrent {
            cdrain,
            gm,
            gds,
            gmbs,
            von,
            vdsat,
        }
    }

    /// `mos3noi.c`: the NLEV 3 `beta` is `tKP m W / (L - 2 LD)` with the
    /// drawn width, and the flicker laws use `W - 2 WD`, `L - 2 LD` (C
    /// ignores XW/XL there) and the model's `Cox` (its square for the
    /// SPICE3-compatible law).
    fn noise_shape(&self, geometry: &Geometry, op: &Operating<Params>) -> NoiseShape {
        let cox = 3.9 * EPSILON_0 / self.tox;
        let length = geometry.l - 2. * self.common.ld;
        NoiseShape {
            beta: op.params.kp * geometry.m * geometry.w / length,
            width: geometry.w - 2. * self.wd,
            length,
            cox,
            cox_squared: cox * cox,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A level-3 card without the schema: every effect switched on.
    fn level(nfs: Real, vmax: Real, nsub: bool) -> Level3 {
        Level3 {
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
            kp: Some(60e-6),
            gamma: Some(0.5),
            phi: Some(0.7),
            tox: 20e-9,
            xl: 0.,
            wd: 0.,
            xw: 0.,
            delvto: 0.,
            u0: Some(500.),
            nsub: nsub.then_some(2e16),
            tpg: None,
            nss: None,
            vmax,
            xj: 0.25e-6,
            nfs,
            eta: 0.5,
            delta: 0.8,
            theta: 0.08,
            kappa: 0.4,
        }
    }

    fn geometry() -> Geometry {
        Geometry {
            m: 1.,
            l: 1.5e-6,
            w: 6e-6,
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

    fn op(level: &Level3) -> Operating<Params> {
        level
            .operating("m1", &geometry(), &ModelContext::default())
            .unwrap()
    }

    #[test]
    fn regions_follow_mos3load() {
        let level = level(0., 0., true);
        let op = op(&level);
        // Cutoff (no NFS): no current, the threshold as von.
        let cut = level.drain_current(&op, 0.2, 1., -0.5);
        assert_eq!([cut.cdrain, cut.gm, cut.gds, cut.gmbs], [0.; 4]);
        assert_eq!(cut.vdsat, 0.);
        assert!(cut.von > 0.2);
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
        // Body bias raises the threshold, ETA lowers it with vds.
        let biased = level.drain_current(&op, 2., 3., -2.);
        assert!(biased.von > saturated.von && biased.cdrain < saturated.cdrain);
        assert!(level.drain_current(&op, 2., 0.1, 0.).von > saturated.von);
        // Weak inversion with NFS: an exponential subthreshold tail below
        // von that joins the strong-inversion current at von.
        let weak = super::tests::level(1e11, 0., true);
        let op = super::tests::op(&weak);
        let below = weak.drain_current(&op, 0.5, 1., 0.);
        let further = weak.drain_current(&op, 0.4, 1., 0.);
        assert!(below.von > 0.5);
        assert!(further.cdrain > 0. && further.cdrain < below.cdrain);
        let von = below.von;
        let (left, right) = (
            weak.drain_current(&op, von - 1e-9, 1., 0.),
            weak.drain_current(&op, von + 1e-9, 1., 0.),
        );
        assert!((left.cdrain - right.cdrain).abs() <= 1e-6 * right.cdrain);
        // Velocity saturation lowers vdsat and the current.
        let fast = super::tests::level(0., 5e4, true);
        let op_fast = super::tests::op(&fast);
        let slow = level.drain_current(&super::tests::op(&level), 2., 3., 0.);
        let limited = fast.drain_current(&op_fast, 2., 3., 0.);
        assert!(limited.vdsat < slow.vdsat && limited.cdrain < slow.cdrain);
    }

    /// The largest finite-difference mismatch of (gm, gds, gmbs), relative
    /// to `|gm| + |gds| + |gmbs|`, over a bias grid; `skip` excludes points.
    fn worst_mismatch(level: &Level3, skip: impl Fn(Real, Real, &DrainCurrent) -> bool) -> Real {
        let op = op(level);
        let current = |vgs, vds, vbs| level.drain_current(&op, vgs, vds, vbs).cdrain;
        let mut worst: Real = 0.;
        for vgs in [0.3, 0.45, 0.8, 1.3, 2.5, 4.] {
            for vds in [0.05, 0.4, 1.5, 4.] {
                for vbs in [-2., -0.3, 0.2] {
                    let point = level.drain_current(&op, vgs, vds, vbs);
                    if skip(vds, vbs, &point) {
                        continue;
                    }
                    let h = 1e-6;
                    let fd = |f: &dyn Fn(Real) -> Real| (f(h) - f(-h)) / (2. * h);
                    let numerical = [
                        fd(&|d| current(vgs + d, vds, vbs)),
                        fd(&|d| current(vgs, vds + d, vbs)),
                        fd(&|d| current(vgs, vds, vbs + d)),
                    ];
                    let scale = point.gm.abs() + point.gds.abs() + point.gmbs.abs() + 1e-15;
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

    /// C's derivatives are the exact derivatives of C's current wherever its
    /// expressions are exact: subthreshold (NFS), linear and saturated
    /// strong inversion, the XJ short-channel factor, ETA, THETA, and DELTA
    /// in reverse body bias, and velocity saturation below `vdsat`.
    #[test]
    fn derivatives_match_finite_differences_where_c_is_exact() {
        for (nfs, vmax, nsub, kappa, delta) in [
            (0., 0., false, 0.4, 0.8),
            (1e11, 0., false, 0.4, 0.8),
            (0., 0., true, 0., 0.8),
            (1e11, 0., true, 0., 0.8),
            (1e11, 0., true, 0., 0.),
            (0., 5e4, true, 0.4, 0.8),
        ] {
            let mut level = level(nfs, vmax, nsub);
            (level.kappa, level.delta) = (kappa, delta);
            let worst = worst_mismatch(&level, |vds, vbs, point| {
                (vbs > 0. && delta != 0.) || (vmax > 0. && vds > point.vdsat)
            });
            assert!(
                worst <= 1e-6,
                "nfs={nfs} vmax={vmax} nsub={nsub} kappa={kappa} delta={delta}: {worst:e}"
            );
        }
    }

    /// Where `mos3load.c`'s own derivatives are approximate, the port keeps
    /// them (AC and pole-zero parity use C's `gm`/`gds`/`gmbs`; the Newton
    /// solution does not depend on them): channel-length modulation with
    /// NSUB (`ddldv*` treat `delxl` as a function of `vds - vdsat` only), the
    /// narrow-width term in forward body bias (`dqbdvb` uses `d phibs/d vbs =
    /// -1`) and velocity saturation beyond `vdsat`. The mismatch is bounded.
    #[test]
    fn c_approximate_derivatives_stay_bounded() {
        for (nfs, vmax, nsub) in [(0., 0., true), (1e11, 0., true), (0., 5e4, true)] {
            let worst = worst_mismatch(&level(nfs, vmax, nsub), |_, _, _| false);
            assert!(
                (1e-6..0.15).contains(&worst),
                "nfs={nfs} vmax={vmax} nsub={nsub}: {worst:e}"
            );
        }
    }
}
