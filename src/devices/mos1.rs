//! Shichman-Hodges MOS level 1 (`mos1`) on the shared [`crate::devices::mos`]
//! shell.
//!
//! C references, read as behaviour only:
//!
//! - `mos1/mos1set.c` (`MOS1setup`): model/instance defaults and the internal
//!   drain/source nodes created for series resistance;
//! - `mos1/mos1temp.c` (`MOS1temp`): oxide capacitance, process extraction
//!   (TOX/UO/NSUB/TPG/NSS), temperature scaling of KP/PHI/VBI/IS/JS/PB and the
//!   junction capacitances, area/perimeter junction geometry and RD/RS/RSH;
//! - `mos1/mos1load.c` (`MOS1load`): the square-law channel with drain/source
//!   reversal and body effect (forward body bias included), ported operation
//!   by operation; the rest of the load (junctions, Meyer charge, limiting,
//!   stamps) is the shell's;
//! - `mos1/mos1noi.c` (`.noise`, through the shell) and `mos1dset.c`/
//!   `mos1dist.c` (the `disto` submodule).
//!
//! See [`crate::devices::mos`] for the load's deliberate divergences and the
//! `off`/`IC` start behaviour. C's SPICE3-compatibility flicker form is not
//! selectable (no compatibility mode is ported).

mod disto;

use crate::devices::mos::{
    CHARGE, Common, DrainCurrent, EPSILON_0, Geometry, Junction, K_OVER_Q, MosLevel, Mosfet,
    NoiseShape, Operating, REFTEMP, ReverseLaw, aliased, band_gap, p, potential_factor, required,
};
use crate::devices::noise::NoiseFamily;
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::setup::CDeviceType;
use crate::devices::{ModelContext, ResolvedModel};
use crate::primitives::{Real, SpiceError, SpiceResult};

/// Intrinsic carrier density of silicon used by `mos1temp.c` (m^-3).
const INTRINSIC_DENSITY: Real = 1.45e16;

/// The level-1 MOSFET: the shared shell around [`Level1`].
pub(super) type Mos1 = Mosfet<Level1>;

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

/// Validated level-1 model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
pub(super) struct Level1 {
    pub(super) common: Common,
    vto: Option<Real>,
    kp: Option<Real>,
    gamma: Option<Real>,
    phi: Option<Real>,
    lambda: Real,
    tox: Option<Real>,
    u0: Option<Real>,
    nsub: Option<Real>,
    tpg: Option<Real>,
    nss: Option<Real>,
}

/// The level-1 channel parameters of one load.
#[derive(Debug, Clone, Copy)]
pub(super) struct Params {
    pub(super) lambda: Real,
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

impl Level1 {
    /// C `EffectiveLength = l - 2 * latDiff`.
    fn length(&self, geometry: &Geometry) -> Real {
        geometry.l - 2. * self.common.ld
    }

    fn process(&self, tnom: Real) -> SpiceResult<Process> {
        let pol = self.common.pol;
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
                process.gamma = (2. * 11.70 * EPSILON_0 * CHARGE * nsub * 1e6).sqrt() / cox;
            }
            if self.vto.is_none() {
                let vfb = wkfngs - self.nss.unwrap_or(0.) * 1e4 * CHARGE / cox;
                process.vto = vfb + pol * (process.gamma * process.phi.sqrt() + process.phi);
            }
        }
        Ok(process)
    }
}

impl MosLevel for Level1 {
    type Params = Params;
    const LEVEL: u8 = 1;
    const LABEL: &'static str = "MOS1";
    const DEVICE_TYPE: CDeviceType = CDeviceType::Mos1;
    const NOISE_FAMILY: NoiseFamily = NoiseFamily::Mos1;
    /// `mos1set.c`: `CKTmkVolt(..., "drain")` / `"source"`.
    const PRIME_SUFFIXES: [&'static str; 2] = ["drain", "source"];

    fn from_card(resolved: &ResolvedModel<'_>) -> SpiceResult<Self> {
        let m = resolved.parameters(&ScalarSchema { parameters: MODEL })?;
        let get = |name: &str| m.get(name).map(|v| v.value);
        let tpg = get("tpg");
        Ok(Self {
            common: Common::from_values(&m, resolved.family(), tpg, "MOS1", "mos1noi.c")?,
            vto: aliased(&m, ["vto", "vt0"]),
            kp: get("kp"),
            gamma: get("gamma"),
            phi: get("phi"),
            lambda: required(&m, "lambda", "MOS1")?,
            tox: get("tox"),
            u0: aliased(&m, ["u0", "uo"]),
            nsub: get("nsub"),
            tpg,
            nss: get("nss"),
        })
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()> {
        let length = self.length(geometry);
        if length <= 0. || !length.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS1 effective channel length L - 2*LD must be positive"
            )));
        }
        Ok(())
    }

    /// `mos1temp.c` and the start of `mos1load.c`, in C's arithmetic.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Params>> {
        let model = &self.common;
        let tnom = model.tnom.unwrap_or(context.nominal_temperature)
            + crate::devices::mos::CELSIUS_TO_KELVIN;
        // C: MOS1temp = CKTtemp + MOS1dtemp, with CKTtemp in kelvin.
        let temp = geometry.kelvin(context);
        if !(tnom.is_finite() && tnom > 0. && temp.is_finite() && temp > 0.) {
            return Err(SpiceError::circuit(format!(
                "{name}: invalid MOS1 temperature"
            )));
        }
        let process = self.process(tnom)?;
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
        let length = self.length(geometry);
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
        let operating = Operating {
            vt,
            beta: kp * m * geometry.w / length,
            vbi,
            phi,
            vto,
            gamma: process.gamma,
            oxide: process.cox * length * m * geometry.w,
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
                model.overlap[2] * m * length,
            ],
            params: Params {
                lambda: self.lambda,
            },
        };
        operating.check(
            name,
            "MOS1",
            temp,
            &[],
            &[bottom[0], bottom[1], side[0], side[1]],
        )?;
        Ok(operating)
    }

    /// `mos1load.c`'s Shichman-Hodges block, in C's operation order.
    fn drain_current(
        &self,
        op: &Operating<Params>,
        vgx: Real,
        vdsm: Real,
        vbx: Real,
    ) -> DrainCurrent {
        let lambda = op.params.lambda;
        let sarg = if vbx <= 0. {
            (op.phi - vbx).sqrt()
        } else {
            let root = op.phi.sqrt();
            let sarg = root - vbx / (root + root);
            // MAX(0, sarg)
            if 0. > sarg { 0. } else { sarg }
        };
        let von = (op.vbi * self.common.pol) + op.gamma * sarg;
        let vgst = vgx - von;
        let vdsat = if vgst > 0. { vgst } else { 0. };
        let arg = if sarg <= 0. {
            0.
        } else {
            op.gamma / (sarg + sarg)
        };
        let (cdrain, gm, gds, gmbs) = if vgst <= 0. {
            (0., 0., 0., 0.)
        } else {
            let betap = op.beta * (1. + lambda * vdsm);
            if vgst <= vdsm {
                let gm = betap * vgst;
                (
                    betap * vgst * vgst * 0.5,
                    gm,
                    lambda * op.beta * vgst * vgst * 0.5,
                    gm * arg,
                )
            } else {
                let gm = betap * vdsm;
                (
                    betap * vdsm * (vgst - 0.5 * vdsm),
                    gm,
                    betap * (vgst - vdsm) + lambda * op.beta * vdsm * (vgst - 0.5 * vdsm),
                    gm * arg,
                )
            }
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

    /// `mos1noi.c`: `beta = tKP m W / (L - 2 LD)` (the load's `Beta`), the
    /// drawn width, the effective length, and `Cox` for `TOX = 1e-7 m` when
    /// the model has no oxide capacitance (`sqrt(coxSquared)` in the NLEV
    /// laws).
    fn noise_shape(&self, geometry: &Geometry, op: &Operating<Params>) -> NoiseShape {
        let cox = match self.tox.filter(|tox| *tox != 0.) {
            Some(tox) => 3.9 * EPSILON_0 / tox,
            None => 3.9 * 8.854214871e-12 / 1e-7,
        };
        let cox_squared = cox * cox;
        NoiseShape {
            beta: op.beta,
            width: geometry.w,
            length: self.length(geometry),
            cox: cox_squared.sqrt(),
            cox_squared,
        }
    }

    /// `mos1dset.c`/`mos1dist.c` (the `disto` submodule).
    fn distortion(
        device: &Mos1,
        context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        device.distortion_terms(context)
    }
}
