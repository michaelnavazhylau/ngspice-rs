//! MOS level 6 (`mos6`): Sakurai and Newton's n-th power law MOSFET on the
//! shared [`crate::devices::mos`] shell.
//!
//! C references, read as behaviour only:
//!
//! - `mos6/mos6.c` (`MOS6mPTable`, `MOS6pTable`), `mos6mpar.c`
//!   (`MOS6mParam`), `mos6par.c` and `mos6set.c` (`MOS6setup`): the setters,
//!   their defaults and the internal drain/source nodes;
//! - `mos6/mos6temp.c` (`MOS6temp`): oxide capacitance, KC derived from U0
//!   when TOX is given, process extraction from NSUB/TPG/NSS (as level 1),
//!   and the temperature scaling of KC, PHI, VBI, IS/JS, PB and the junction
//!   capacitances;
//! - `mos6/mos6load.c` (`MOS6load`): the n-th power law drain current
//!   `Id = KC W/Leff (vgs - von)^NC (1 + lambda vds)` in saturation, the
//!   `(2 - vds/vdsat) vds/vdsat` linear-region factor with
//!   `vdsat = KV (vgs - von)^NV`, the threshold
//!   `von = vbi + GAMMA sarg - GAMMA1 vbs - SIGMA vds`, the bulk-dependent
//!   `lambda = LAMBDA0 - LAMBDA1 vbs`, C's derivatives, the constant reverse
//!   junction law and Meyer's gate capacitances;
//! - `mos6/mos6trun.c` (the three Meyer gate charges) and `mos6ic.c`.
//!
//! The shell supplies series resistance, junction charges, Meyer gate charge,
//! limiting, the instance sweeps and observations.
//!
//! # What C does not have
//!
//! `mos6init.c` registers no `DEVacLoad`, `DEVpzLoad`, `DEVnoise`,
//! `DEVdisto` or sensitivity routines. In C a level-6 instance contributes
//! *nothing* to `.ac` (and the analyses built on it), which is not a model of
//! the device. Here `.ac`, `.noise`, `.disto`, `.sp` and `.pz` fail
//! explicitly ([`SpiceError::Unsupported`] for the small-signal and
//! pole-zero loads, [`SpiceError::NotYetPorted`] naming `mos6init.c` for
//! `.noise`/`.disto`, and the generic refusal for `.sens`).
//!
//! # Deliberate divergences
//!
//! - `mos6load.c` multiplies its whole stamp by `M`; the shell scales the
//!   currents, charges and conductances instead, which is the same except for
//!   `gmin` and the junction `vcrit`, which the level hooks keep per unit
//!   device as C does. C's convergence test compares per-unit currents
//!   (`MOS6convTest`); the port's Newton test is circuit-wide.
//! - `mos6set.c` gives NRD/NRS no default, so RSH without an explicit nonzero
//!   NRD (NRS) creates no internal node and, in C, no resistance at all; the
//!   port refuses that combination rather than silently dropping RSH. An
//!   explicit zero NRD/NRS with RSH is refused as for level 1.
//! - In forward body bias with `sarg` clamped to zero C stamps a body
//!   transconductance without the `GAMMA1` term; the port uses the exact
//!   derivative (the converged point is the same).
//! - NVTH and PS are settable in C but unused by `mos6load.c`; they are
//!   accepted and, as in C, have no effect. C warns and continues for
//!   `L - 2 LD <= 0` or a nonpositive temperature-adjusted PHI/PB; here those
//!   are errors.

use crate::devices::ModelContext;
use crate::devices::ResolvedModel;
use crate::devices::mos::{
    CHARGE, Common, DrainCurrent, EPSILON_0, Geometry, K_OVER_Q, MosLevel, Mosfet, Operating,
    ReverseLaw, Temperatures, aliased, band_gap, p, required,
};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::primitives::{Real, SpiceError, SpiceResult};

/// Intrinsic carrier density of silicon used by `mos6temp.c` (m^-3).
const INTRINSIC_DENSITY: Real = 1.45e16;

/// The level-6 MOSFET: the shared shell around [`Level6`].
pub(super) type Mos6 = Mosfet<Level6>;

/// Model setters (`MOS6mPTable` as `MOS6mParam` accepts them). Parameters
/// whose *presence* changes C's derivations (`MOS6...Given`) have no schema
/// default; the rest carry `mos6set.c`'s defaults. KC and KV are in
/// `A/V^NC` and `V^(1-NV)`, so they are recorded as dimensionless.
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("kv", U::Dimensionless, D::Finite, Some(2.)),
    p("nv", U::Dimensionless, D::Finite, Some(0.5)),
    p("kc", U::Dimensionless, D::NonNegative, None),
    p("nc", U::Dimensionless, D::Finite, Some(1.)),
    // Settable in C, unused by mos6load.c.
    p("nvth", U::Dimensionless, D::Finite, Some(0.5)),
    p("ps", U::Dimensionless, D::Finite, Some(0.)),
    p("gamma", U::SquareRootVolt, D::NonNegative, None),
    p("gamma1", U::Dimensionless, D::Finite, Some(0.)),
    p("sigma", U::Dimensionless, D::Finite, Some(0.)),
    p("phi", U::Volt, D::Positive, None),
    p("lambda", U::InverseVolt, D::Finite, None),
    p("lambda0", U::InverseVolt, D::Finite, None),
    p("lambda1", U::InverseVolt, D::Finite, Some(0.)),
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
    p("mjsw", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("js", U::AmperePerSquareMetre, D::NonNegative, Some(0.)),
    p("ld", U::Metre, D::NonNegative, Some(0.)),
    p("tox", U::Metre, D::NonNegative, None),
    p("u0", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("uo", U::SquareCentimetrePerVoltSecond, D::Positive, None),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("tpg", U::Dimensionless, D::Finite, None),
    p("nsub", U::PerCubicCentimetre, D::Positive, None),
    p("nss", U::PerSquareCentimetre, D::Finite, None),
    p("tnom", U::Celsius, D::Temperature, None),
];

/// Validated level-6 model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
pub(super) struct Level6 {
    common: Common,
    vto: Option<Real>,
    kv: Real,
    nv: Real,
    kc: Option<Real>,
    nc: Real,
    gamma: Option<Real>,
    gamma1: Real,
    sigma: Real,
    phi: Option<Real>,
    /// C `MOS6lamda0`: LAMBDA0, else LAMBDA, else zero (`mos6set.c`).
    lambda0: Real,
    lambda1: Real,
    tox: Option<Real>,
    u0: Option<Real>,
    nsub: Option<Real>,
    tpg: Option<Real>,
    nss: Option<Real>,
}

/// Process-derived model values (`mos6temp.c`, model loop).
#[derive(Debug, Clone, Copy)]
struct Process {
    /// Oxide capacitance per area, zero when TOX is absent or zero.
    cox: Real,
    kc: Real,
    phi: Real,
    gamma: Real,
    vto: Real,
}

/// The level-6 channel parameters of one load.
#[derive(Debug, Clone, Copy)]
pub(super) struct Params {
    /// C `betac = tKc W / Leff`, times `M`.
    pub(super) betac: Real,
    pub(super) kv: Real,
    pub(super) nv: Real,
    pub(super) nc: Real,
    pub(super) gamma1: Real,
    pub(super) sigma: Real,
    pub(super) lambda0: Real,
    pub(super) lambda1: Real,
}

impl Level6 {
    /// Effective channel length `L - 2 LD`.
    fn length(&self, geometry: &Geometry) -> Real {
        geometry.l - 2. * self.common.ld
    }

    /// `mos6temp.c`'s model loop at the nominal temperature `tnom` (K).
    fn process(&self, tnom: Real) -> SpiceResult<Process> {
        let pol = self.common.pol;
        let mut process = Process {
            cox: 0.,
            kc: self.kc.unwrap_or(5e-5),
            phi: self.phi.unwrap_or(0.6),
            gamma: self.gamma.unwrap_or(0.),
            vto: self.vto.unwrap_or(0.),
        };
        let Some(tox) = self.tox.filter(|tox| *tox != 0.) else {
            return Ok(process);
        };
        let cox = 3.9 * EPSILON_0 / tox;
        process.cox = cox;
        if self.kc.is_none() {
            process.kc = 0.5 * self.u0.unwrap_or(600.) * cox * 1e-4;
        }
        if let Some(nsub) = self.nsub {
            let density = nsub * 1e6;
            if density <= INTRINSIC_DENSITY {
                return Err(SpiceError::circuit(format!(
                    "MOS6 NSUB={nsub} is below the intrinsic density (mos6temp.c: Nsub < Ni)"
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
        }
        Ok(process)
    }
}

impl MosLevel for Level6 {
    type Params = Params;
    const LEVEL: u8 = 6;
    const LABEL: &'static str = "MOS6";
    const PRIME_SUFFIXES: [&'static str; 2] = ["drain", "source"];

    fn from_card(resolved: &ResolvedModel<'_>) -> SpiceResult<Self> {
        let m = resolved.parameters(&ScalarSchema { parameters: MODEL })?;
        let get = |name: &str| m.get(name).map(|v| v.value);
        let required = |name: &str| required(&m, name, "MOS6");
        let common = Common::from_values(&m, resolved.family(), "MOS6", "mos6init.c")?;
        let model = Self {
            common,
            vto: aliased(&m, ["vto", "vt0"]),
            kv: required("kv")?,
            nv: required("nv")?,
            kc: get("kc"),
            nc: required("nc")?,
            gamma: get("gamma"),
            gamma1: required("gamma1")?,
            sigma: required("sigma")?,
            phi: get("phi"),
            lambda0: get("lambda0").or_else(|| get("lambda")).unwrap_or(0.),
            lambda1: required("lambda1")?,
            tox: get("tox"),
            u0: aliased(&m, ["u0", "uo"]),
            nsub: get("nsub"),
            tpg: get("tpg"),
            nss: get("nss"),
        };
        model.common.check(model.tpg, "MOS6")?;
        Ok(model)
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()> {
        let length = self.length(geometry);
        if length <= 0. || !length.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS6 effective channel length L - 2*LD must be positive"
            )));
        }
        Ok(())
    }

    /// `mos6set.c` creates the internal drain (source) node only for a
    /// nonzero RD (RS), or a nonzero RSH with a nonzero *given* NRD (NRS):
    /// unlike levels 1 and 3 it has no default of one square, so C applies no
    /// sheet resistance at all to an instance without NRD (NRS). That silent
    /// drop is refused.
    fn check_instance(&self, name: &str, setters: &ScalarValues) -> SpiceResult<()> {
        let common = &self.common;
        if !common.rsh.is_some_and(|rsh| rsh != 0.) {
            return Ok(());
        }
        for (resistance, squares, side) in
            [(common.rd, "nrd", "drain"), (common.rs, "nrs", "source")]
        {
            let given = setters.get(squares).is_some_and(|v| v.location.is_some());
            if resistance.is_none() && !given {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "{name}: MOS6 RSH without an explicit {} (mos6set.c has no default \
                         number of {side} squares, so C silently applies no {side} sheet \
                         resistance); give {} or {}",
                        squares.to_uppercase(),
                        squares.to_uppercase(),
                        if side == "drain" { "RD" } else { "RS" },
                    ),
                    location: None,
                });
            }
        }
        Ok(())
    }

    /// `mos6temp.c` and the start of `mos6load.c`.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Params>> {
        let model = &self.common;
        let t = Temperatures::new(model.tnom, geometry, context, name, "MOS6")?;
        let process = self.process(t.tnom)?;
        let ratio = t.ratio;
        let kc = process.kc / (ratio * ratio.sqrt());
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
                betac: kc * m * geometry.w / length,
                kv: self.kv,
                nv: self.nv,
                nc: self.nc,
                gamma1: self.gamma1,
                sigma: self.sigma,
                lambda0: self.lambda0,
                lambda1: self.lambda1,
            },
        };
        let finite = [
            operating.vt,
            operating.params.betac,
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
            || operating.params.betac < 0.
            || operating.drain.saturation <= 0.
            || operating.source.saturation <= 0.
            || [bottom, side].iter().any(|f| !f.is_finite() || *f < 0.)
        {
            return Err(SpiceError::circuit(format!(
                "{}: MOS6 parameters are out of range at {:.2} K \
                 (nonfinite value, or nonpositive PHI/PB/IS after temperature scaling)",
                name, t.temp
            )));
        }
        Ok(operating)
    }

    /// `mos6load.c`'s n-th power law block in the normalized device frame
    /// (`vbs` is C's `vbsvbd`, `vds` its `vdshere`), with C's derivatives.
    fn drain_current(
        &self,
        op: &Operating<Params>,
        vgs: Real,
        vds: Real,
        vbs: Real,
    ) -> DrainCurrent {
        let pol = self.common.pol;
        let p = &op.params;
        let gamma = op.gamma;
        let root = op.phi.sqrt();
        let sarg = if vbs <= 0. {
            (op.phi - vbs).sqrt()
        } else {
            (root - vbs / (root + root)).max(0.)
        };
        let von = op.vbi * pol + gamma * sarg - p.gamma1 * vbs - p.sigma * vds;
        let vgon = vgs - von;
        if vgon <= 0. {
            return DrainCurrent {
                current: 0.,
                gm: 0.,
                gds: 0.,
                gmbs: 0.,
                von,
                vdsat: 0.,
            };
        }
        // `-d von / d vbs`. C drops the GAMMA1 term when `sarg` is clamped
        // to zero; this is the exact derivative.
        let vonbm = if sarg <= 0. {
            p.gamma1
        } else if vbs <= 0. {
            p.gamma1 + gamma / (sarg + sarg)
        } else {
            p.gamma1 + gamma / 2. / root
        };
        let log = vgon.ln();
        let vdsat = p.kv * (log * p.nv).exp();
        let idsat = p.betac * (log * p.nc).exp();
        let lambda = p.lambda0 - p.lambda1 * vbs;
        // Saturation.
        let mut current = idsat * (1. + lambda * vds);
        let mut gm = current * p.nc / vgon;
        let mut gds = gm * p.sigma + idsat * lambda;
        let mut gmbs = gm * vonbm - idsat * p.lambda1 * vds;
        if vdsat > vds {
            // Linear region.
            let vdst = vds / vdsat;
            let vdst2 = (2. - vdst) * vdst;
            let vdstg = -vdst * p.nv / vgon;
            let ivdst1 = current * (2. - vdst - vdst);
            current *= vdst2;
            gm = gm * vdst2 + ivdst1 * vdstg;
            gds = gds * vdst2 + ivdst1 * (1. / vdsat + vdstg * p.sigma);
            gmbs = gmbs * vdst2 + ivdst1 * vdstg * vonbm;
        }
        DrainCurrent {
            current,
            gm,
            gds,
            gmbs,
            von,
            vdsat,
        }
    }

    /// `mos6init.c` registers neither `DEVacLoad` nor `DEVpzLoad`: C's level
    /// 6 contributes nothing to a small-signal or pole-zero matrix, which is
    /// refused rather than reproduced.
    fn small_signal(device: &Mos6, analysis: &str) -> SpiceResult<()> {
        Err(SpiceError::Unsupported {
            feature: format!(
                "{analysis} analysis of MOS6 instance {}: C's level 6 registers no {} \
                 (src/spicelib/devices/mos6/mos6init.c), so ngspice drops the device from the \
                 small-signal matrix",
                device.name,
                if analysis == "AC" {
                    "DEVacLoad"
                } else {
                    "DEVpzLoad"
                },
            ),
            location: None,
        })
    }

    /// C's level 6 has no noise routine (`mos6init.c`: `DEVnoise = NULL`).
    fn noise(
        device: &Mos6,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(SpiceError::not_yet_ported(
            format!("MOS6 noise of {}", device.name),
            "src/spicelib/devices/mos6/mos6init.c (DEVnoise = NULL: C has no level-6 noise)",
        ))
    }

    /// C's level 6 has no distortion routine (`mos6init.c`: `DEVdisto =
    /// NULL`).
    fn distortion(
        device: &Mos6,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(SpiceError::not_yet_ported(
            format!("MOS6 distortion of {}", device.name),
            "src/spicelib/devices/mos6/mos6init.c (DEVdisto = NULL: C has no level-6 distortion)",
        ))
    }

    /// `mos6load.c` multiplies the junction `gmin` by `M` with the rest of
    /// the stamp.
    fn junction_gmin(gmin: Real, geometry: &Geometry) -> Real {
        gmin * geometry.m
    }

    /// `mos6temp.c` derives `vcrit` from the per-unit saturation current.
    fn limiting_saturation(saturation: Real, geometry: &Geometry) -> Real {
        saturation / geometry.m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::mos::Junction;

    fn junction() -> Junction {
        Junction {
            saturation: 1e-14,
            bottom: 0.,
            sidewall: 0.,
            mj: 0.5,
            mjsw: 0.5,
            potential: 0.8,
            fc: 0.5,
            reverse: ReverseLaw::Constant,
        }
    }

    fn level(pol: Real) -> Level6 {
        Level6 {
            common: Common {
                pol,
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
                mjsw: 0.5,
                fc: 0.5,
                ld: 0.,
                tnom: None,
                kf: 0.,
                af: 1.,
                nlev: 2,
                gdsnoi: 1.,
            },
            vto: None,
            kv: 1.6,
            nv: 0.8,
            kc: None,
            nc: 1.3,
            gamma: None,
            gamma1: 0.05,
            sigma: 0.02,
            phi: None,
            lambda0: 0.03,
            lambda1: 0.01,
            tox: None,
            u0: None,
            nsub: None,
            tpg: None,
            nss: None,
        }
    }

    fn operating(gamma: Real) -> Operating<Params> {
        Operating {
            vt: 0.025864,
            vbi: 0.4,
            phi: 0.7,
            gamma,
            oxide: 0.,
            drain: junction(),
            source: junction(),
            overlap: [0.; 3],
            params: Params {
                betac: 2e-4,
                kv: 1.6,
                nv: 0.8,
                nc: 1.3,
                gamma1: 0.05,
                sigma: 0.02,
                lambda0: 0.03,
                lambda1: 0.01,
            },
        }
    }

    #[test]
    fn regions_follow_the_n_th_power_law() {
        let model = level(1.);
        let op = operating(0.5);
        // vbs = 0: von = vbi + gamma sqrt(phi) - sigma vds.
        let von = |vds: Real| 0.4 + 0.5 * (0.7_f64).sqrt() - 0.02 * vds;
        // Cutoff.
        let off = model.drain_current(&op, 0.5, 1., 0.);
        assert_eq!([off.current, off.gm, off.gds, off.gmbs], [0.; 4]);
        assert_eq!(off.vdsat, 0.);
        // Saturation: KC W/L vgon^NC (1 + lambda vds).
        let vgon: Real = 2. - von(3.);
        let sat = model.drain_current(&op, 2., 3., 0.);
        let idsat = 2e-4 * vgon.powf(1.3);
        assert!((sat.current - idsat * (1. + 0.03 * 3.)).abs() <= 1e-15 * idsat);
        assert!((sat.vdsat - 1.6 * vgon.powf(0.8)).abs() <= 1e-14);
        // Linear: the saturation current times (2 - x) x, x = vds / vdsat.
        let vgon: Real = 2. - von(0.3);
        let lin = model.drain_current(&op, 2., 0.3, 0.);
        let x = 0.3 / (1.6 * vgon.powf(0.8));
        let expected = 2e-4 * vgon.powf(1.3) * (1. + 0.03 * 0.3) * (2. - x) * x;
        assert!((lin.current - expected).abs() <= 1e-15 * expected);
        // Continuity at vds = vdsat.
        let at = model.drain_current(&op, 2., 1.5, 0.);
        let edge = at.vdsat;
        let below = model.drain_current(&op, 2., edge * (1. - 1e-12), 0.);
        let above = model.drain_current(&op, 2., edge * (1. + 1e-12), 0.);
        assert!((below.current - above.current).abs() <= 1e-9 * above.current);
    }

    #[test]
    fn derivatives_match_finite_differences_in_every_region() {
        let model = level(1.);
        for gamma in [0., 0.5] {
            let op = operating(gamma);
            // (vgs, vds, vbs): saturation and linear region in reverse,
            // zero and forward body bias, and forward bias beyond 2 PHI
            // (sarg clamped, where the port's derivative includes GAMMA1).
            for (vgs, vds, vbs) in [
                (2., 3., -1.),
                (2., 0.3, -1.),
                (1.5, 0.8, 0.),
                (2.5, 0.2, 0.3),
                (2.5, 4., 0.3),
                (3., 0.5, 1.6),
                (3., 4., 1.6),
            ] {
                let f = |vgs: Real, vds: Real, vbs: Real| {
                    model.drain_current(&op, vgs, vds, vbs).current
                };
                let d = model.drain_current(&op, vgs, vds, vbs);
                assert!(d.current > 0., "{vgs} {vds} {vbs}");
                let h = 1e-6;
                let gm = (f(vgs + h, vds, vbs) - f(vgs - h, vds, vbs)) / (2. * h);
                let gds = (f(vgs, vds + h, vbs) - f(vgs, vds - h, vbs)) / (2. * h);
                let gmbs = (f(vgs, vds, vbs + h) - f(vgs, vds, vbs - h)) / (2. * h);
                for (name, numeric, analytic) in [
                    ("gm", gm, d.gm),
                    ("gds", gds, d.gds),
                    ("gmbs", gmbs, d.gmbs),
                ] {
                    assert!(
                        (numeric - analytic).abs() <= 1e-6 * d.current.abs() + 1e-12,
                        "{name} at ({vgs}, {vds}, {vbs}), gamma {gamma}: \
                         {numeric:e} vs {analytic:e}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_threshold_uses_the_polarity_normalized_built_in_potential() {
        // PMOS: von = type * tVbi + ..., so a negative tVbi turns on at
        // positive normalized vgs.
        let model = level(-1.);
        let mut op = operating(0.);
        op.vbi = -0.6;
        let d = model.drain_current(&op, 1., 2., 0.);
        assert!((d.von - (0.6 - 0.02 * 2.)).abs() < 1e-15);
        assert!(d.current > 0.);
    }

    #[test]
    fn multiplier_hooks_keep_gmin_and_vcrit_per_unit_device() {
        let geometry = Geometry {
            m: 3.,
            l: 1e-6,
            w: 1e-6,
            ad: 0.,
            as_: 0.,
            pd: 0.,
            ps: 0.,
            nrd: 1.,
            nrs: 1.,
            temp: None,
            dtemp: 0.,
        };
        assert_eq!(Level6::junction_gmin(1e-12, &geometry), 3e-12);
        assert_eq!(Level6::limiting_saturation(3e-14, &geometry), 1e-14);
    }
}
