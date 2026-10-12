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
//! The device frame (series resistance, bulk junctions, Meyer charge,
//! limiting, loads and sweeps) is the shared [`crate::devices::mos`] shell;
//! this module supplies the level-1 schema, process extraction and
//! Shichman-Hodges drain current through [`MosLevel`].
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

use crate::devices::ResolvedModel;
use crate::devices::mos::{
    Common, DrainCurrent, EPSILON_0, Geometry, MosLevel, Mosfet, NoiseShape, Operating, ReverseLaw,
    Temperatures, aliased, p, required,
};
use crate::devices::noise::{DeviceNoise, NoiseContext, NoiseFamily};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::{ModelContext, mos::CHARGE, mos::K_OVER_Q};
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
    common: Common,
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

/// The level-1 channel parameters of one load.
#[derive(Debug, Clone, Copy)]
pub(super) struct Params {
    pub(super) beta: Real,
    pub(super) lambda: Real,
}

impl Level1 {
    /// Effective channel length `L - 2 LD`.
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
            let egfet1 = crate::devices::mos::band_gap(tnom);
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
        }
        Ok(process)
    }
}

impl MosLevel for Level1 {
    type Params = Params;
    const LEVEL: u8 = 1;
    const LABEL: &'static str = "MOS1";
    const PRIME_SUFFIXES: [&'static str; 2] = ["drain", "source"];

    fn from_card(resolved: &ResolvedModel<'_>) -> SpiceResult<Self> {
        let m = resolved.parameters(&ScalarSchema { parameters: MODEL })?;
        let get = |name: &str| m.get(name).map(|v| v.value);
        let common = Common::from_values(&m, resolved.family(), "MOS1", "mos1noi.c")?;
        let model = Self {
            common,
            vto: aliased(&m, ["vto", "vt0"]),
            kp: get("kp"),
            gamma: get("gamma"),
            phi: get("phi"),
            lambda: required(&m, "lambda", "MOS1")?,
            tox: get("tox"),
            u0: aliased(&m, ["u0", "uo"]),
            nsub: get("nsub"),
            tpg: get("tpg"),
            nss: get("nss"),
        };
        model.common.check(model.tpg, "MOS1")?;
        Ok(model)
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

    /// `mos1temp.c` and the start of `mos1load.c`.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Params>> {
        let model = &self.common;
        let t = Temperatures::new(model.tnom, geometry, context, name, "MOS1")?;
        let process = self.process(t.tnom)?;
        let ratio = t.ratio;
        let kp = process.kp / (ratio * ratio.sqrt());
        let phio = (process.phi - t.pbfact1) / t.fact1;
        let phi = t.fact2 * phio + t.pbfact;
        let vbi = process.vto - model.pol * (process.gamma * process.phi.sqrt())
            + 0.5 * (t.egfet1 - t.egfet)
            + model.pol * 0.5 * (phi - process.phi);
        let junctions = model.junctions(&t, geometry, ReverseLaw::Constant);
        let potential = junctions.potential;
        let [bottom, side] = junctions.factors;
        let m = geometry.m;
        let length = self.length(geometry);
        let operating = Operating {
            vt: t.vt,
            vbi,
            phi,
            gamma: process.gamma,
            oxide: process.cox * length * m * geometry.w,
            drain: junctions.drain,
            source: junctions.source,
            overlap: [
                model.overlap[0] * m * geometry.w,
                model.overlap[1] * m * geometry.w,
                model.overlap[2] * m * length,
            ],
            params: Params {
                beta: kp * m * geometry.w / length,
                lambda: self.lambda,
            },
        };
        let finite = [
            operating.vt,
            operating.params.beta,
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
            || operating.params.beta <= 0.
            || operating.drain.saturation <= 0.
            || operating.source.saturation <= 0.
            || [bottom, side].iter().any(|f| !f.is_finite() || *f < 0.)
        {
            return Err(SpiceError::circuit(format!(
                "{}: MOS1 parameters are out of range at {:.2} K \
                 (nonfinite value, or nonpositive PHI/PB/KP/IS after temperature scaling)",
                name, t.temp
            )));
        }
        Ok(operating)
    }

    /// `mos1load.c`'s Shichman-Hodges channel with body effect (forward body
    /// bias included).
    fn drain_current(
        &self,
        op: &Operating<Params>,
        vgs: Real,
        vds: Real,
        vbs: Real,
    ) -> DrainCurrent {
        let pol = self.common.pol;
        let Params { beta, lambda } = op.params;
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
            let betap = beta * (1. + lambda * vds);
            if vgst <= vds {
                (
                    betap * vgst * vgst * 0.5,
                    betap * vgst,
                    lambda * beta * vgst * vgst * 0.5,
                )
            } else {
                (
                    betap * vds * (vgst - 0.5 * vds),
                    betap * vds,
                    betap * (vgst - vds) + lambda * beta * vds * (vgst - 0.5 * vds),
                )
            }
        };
        DrainCurrent {
            current,
            gm,
            gds,
            gmbs: -gm * op.gamma * dsarg,
            von,
            vdsat,
        }
    }

    /// `mos1dset.c`/`mos1dist.c` at the operating point (see the `disto`
    /// submodule for C's distortion model).
    fn distortion(
        device: &Mos1,
        context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        device.distortion_terms(context)
    }

    /// `mos1noi.c` at the operating point ([`Mosfet::classic_noise`]): the
    /// NLEV 3 `beta` is the load's, the flicker laws use `W`, `L - 2 LD` and
    /// `Cox` taken for `TOX = 1e-7 m` when the model has no oxide capacitance.
    fn noise(device: &Mos1, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let model = &device.model;
        let geometry = &device.geometry;
        device.classic_noise(context, |op| NoiseShape {
            family: NoiseFamily::Mos1,
            beta: op.params.beta,
            width: geometry.w,
            length: model.length(geometry),
            cox: match model.tox.filter(|tox| *tox != 0.) {
                Some(tox) => 3.9 * EPSILON_0 / tox,
                None => 3.9 * 8.854214871e-12 / 1e-7,
            },
        })
    }
}
