//! `b4temp.c` (`BSIM4temp`): the model's temperature pass, the instance's
//! size-dependent parameters and its temperature-dependent instance values,
//! then `b4check.c`'s parameter check.
//!
//! Ported statement by statement from BSIM4 4.8.3 (`b4temp.c`, Copyright
//! (c) 2025 University of California, Educational Community License 2.0);
//! C's member and local names are kept so the port can be read against it.
#![allow(non_snake_case, non_upper_case_globals)]

use super::check::check_model;
use super::fields::{Instance, Model, SizeParam};
use super::geo::{pa_eff_geo, rdseff_geo};
use crate::primitives::{SpiceError, SpiceResult};

const KboQ: f64 = 8.617087e-5;
const EPS0: f64 = 8.85418e-12;
const EPSSI: f64 = 1.03594e-10;
// b4temp.c's own rounded PI, not the platform's.
#[allow(clippy::approx_constant)]
const PI: f64 = 3.141592654;
const MAX_EXP: f64 = 5.834617425e14;
const MIN_EXP: f64 = 1.713908431e-15;
const EXP_THRESHOLD: f64 = 34.0;
const Charge_q: f64 = 1.60219e-19;
const DELTA: f64 = 1.0E-9;
/// `const.h` `CHARGE`.
const CHARGE: f64 = 1.6021766208e-19;
/// `main.c`: `CONSTroot2 = CONSTsqrt2`.
const CONSTroot2: f64 = std::f64::consts::SQRT_2;
/// `main.c`: `CONSTvt0 = CONSTboltz * REFTEMP / CHARGE`.
const CONSTvt0: f64 = 1.38064852e-23 * (27.0 + 273.15) / CHARGE;
/// `bsim4def.h` `NMOS`.
const NMOS: i32 = 1;
/// `bsim4def.h` `BSIM4v48V480`.
const BSIM4v48V480: i32 = 480;

// C's libm calls, under C's names so the port reads as the C does.
fn exp(x: f64) -> f64 {
    x.exp()
}
fn log(x: f64) -> f64 {
    x.ln()
}
fn sqrt(x: f64) -> f64 {
    x.sqrt()
}
fn pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}
fn fabs(x: f64) -> f64 {
    x.abs()
}
fn atan(x: f64) -> f64 {
    x.atan()
}
/// `ABS` from `ngspice.h`.
fn ABS(x: f64) -> f64 {
    if x >= 0.0 { x } else { -x }
}

/// b4temp.c's two-argument `DEXP(A, B)`: `B = dexp(A)`.
fn dexp(a: f64) -> f64 {
    if a > EXP_THRESHOLD {
        MAX_EXP * (1.0 + a - EXP_THRESHOLD)
    } else if a < -EXP_THRESHOLD {
        MIN_EXP
    } else {
        exp(a)
    }
}

/// `BSIM4DioIjthVjmEval`: the diode limiting voltage `Vjm`.
fn dio_ijth_vjm_eval(Nvtm: f64, Ijth: f64, Isb: f64, XExpBV: f64) -> f64 {
    let Tc = XExpBV;
    let Tb = 1.0 + Ijth / Isb - Tc;
    let EVjmovNv = 0.5 * (Tb + sqrt(Tb * Tb + 4.0 * Tc));
    Nvtm * log(EVjmovNv)
}

/// A C `printf("%g")` spelling for the ported messages.
pub(crate) fn c_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }
    if !value.is_finite() {
        return if value.is_nan() {
            "nan".into()
        } else if value > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if !(-4..6).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        )
    } else {
        let decimals = usize::try_from(5 - exponent).unwrap_or(0);
        let fixed = format!("{value:.decimals$}");
        let fixed = fixed.trim_end_matches('0').trim_end_matches('.');
        fixed.to_string()
    }
}

/// C's `E_BADPARM` with its `IFerrorf(ERR_FATAL, ...)` message.
fn bad_parameter(message: String) -> SpiceError {
    SpiceError::circuit(message)
}

/// The locals `BSIM4temp`'s model pass computes and its instance loop
/// reads (C keeps them as function locals across the loop).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ModelTemp {
    /// C local `Temp`.
    pub(crate) Temp: f64,
    /// C local `Tnom`.
    pub(crate) Tnom: f64,
    /// C local `TRatio`.
    pub(crate) TRatio: f64,
    /// C local `Vtm0`.
    pub(crate) Vtm0: f64,
    /// C local `Eg0`.
    pub(crate) Eg0: f64,
    /// C local `Eg`.
    pub(crate) Eg: f64,
    /// C local `ni`.
    pub(crate) ni: f64,
    /// C local `epssub`.
    pub(crate) epssub: f64,
    /// C local `epsrox`.
    pub(crate) epsrox: f64,
    /// C local `toxe`.
    pub(crate) toxe: f64,
    /// C local `delTemp`.
    pub(crate) delTemp: f64,
}

/// `BSIM4temp`'s model pass (everything before its instance loop) at
/// `temperature` (`CKTtemp`, K): updates `m` as C does and returns the
/// locals the instance loop reads. C runs it once per model per call.
#[allow(
    unused_assignments,
    unused_parens,
    unused_mut,
    unused_variables,
    clippy::too_many_lines,
    clippy::if_same_then_else,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if
)] // A statement-by-statement port of C.
pub(crate) fn temperature_model(
    m: &mut Model,
    temperature: f64,
    warnings: &mut Vec<String>,
) -> ModelTemp {
    let (mut tmp, mut tmp1, mut tmp2, mut tmp3, mut Eg, mut Eg0, mut ni, mut epssub) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut T0, mut T1, mut T2, mut T3, mut T4, mut T5, mut T6, mut T7, mut T8, mut T9) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut Lnew, mut Wnew) = (0.0_f64, 0.0_f64);
    let (mut delTemp, mut Temp, mut TRatio, mut Inv_L, mut Inv_W, mut Inv_LW, mut Vtm0, mut Tnom) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut dumPs, mut dumPd, mut dumAs, mut dumAd, mut PowWeffWr) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut DMCGeff, mut DMCIeff, mut DMDGeff) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut Nvtms, mut Nvtmd, mut SourceSatCurrent, mut DrainSatCurrent) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut T10, mut T11) = (0.0_f64, 0.0_f64);
    let (mut Inv_saref, mut Inv_sbref, mut Inv_sa, mut Inv_sb, mut rho, mut Ldrn, mut dvth0_lod) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut W_tmp, mut Inv_ODeff, mut OD_offset, mut dk2_lod, mut deta0_lod) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut lnl, mut lnw, mut lnnf, mut rbpbx, mut rbpby, mut rbsbx, mut rbsby) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut rbdbx, mut rbdby, mut bodymode) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut kvsat, mut wlod, mut sceff, mut Wdrn) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut V0, mut lt1, mut ltw, mut Theta0, mut Delt_vth, mut Vth_NarrowW, mut Lpe_Vb, mut Vth) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut n, mut n0, mut Vgsteff, mut Vgs_eff, mut niter, mut toxpf, mut toxpi, mut Tcen) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut toxe, mut epsrox, mut vddeot) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut vtfbphi2eot, mut phieot, mut TempRatioeot, mut Vtm0eot, mut Vtmeot, mut vbieot) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let mut Size_Not_Found: i32 = 0;
    let mut i: i32 = 0;
    let mut Fatal_Flag: i32 = 0;
    {
        Temp = temperature;
        if m.SbulkJctPotential < 0.1 {
            m.SbulkJctPotential = 0.1;
            warnings.push("Given pbs is less than 0.1. Pbs is set to 0.1.".to_string());
        }
        if m.SsidewallJctPotential < 0.1 {
            m.SsidewallJctPotential = 0.1;
            warnings.push("Given pbsws is less than 0.1. Pbsws is set to 0.1.".to_string());
        }
        if m.SGatesidewallJctPotential < 0.1 {
            m.SGatesidewallJctPotential = 0.1;
            warnings.push("Given pbswgs is less than 0.1. Pbswgs is set to 0.1.".to_string());
        }
        if m.DbulkJctPotential < 0.1 {
            m.DbulkJctPotential = 0.1;
            warnings.push("Given pbd is less than 0.1. Pbd is set to 0.1.".to_string());
        }
        if m.DsidewallJctPotential < 0.1 {
            m.DsidewallJctPotential = 0.1;
            warnings.push("Given pbswd is less than 0.1. Pbswd is set to 0.1.".to_string());
        }
        if m.DGatesidewallJctPotential < 0.1 {
            m.DGatesidewallJctPotential = 0.1;
            warnings.push("Given pbswgd is less than 0.1. Pbswgd is set to 0.1.".to_string());
        }
        if m.mtrlMod == 0 {
            let mut chktol = fabs(m.toxe);
            if fabs(m.toxp) > chktol {
                chktol = fabs(m.toxp);
            }
            if fabs(m.dtox) > chktol {
                chktol = fabs(m.dtox);
            }
            chktol = chktol * 1e-14;
            if (m.toxeGiven)
                && (m.toxpGiven)
                && (m.dtoxGiven)
                && (fabs(m.toxe - (m.toxp + m.dtox)) > chktol)
            {
                warnings.push(
                    "Warning: toxe, toxp and dtox all given and toxe != toxp + dtox; dtox ignored."
                        .to_string(),
                );
            } else if (m.toxeGiven) && (!m.toxpGiven) {
                m.toxp = m.toxe - m.dtox;
            } else if (!m.toxeGiven) && (m.toxpGiven) {
                m.toxe = m.toxp + m.dtox;
                if !m.toxmGiven {
                    m.toxm = m.toxe;
                }
            }
        } else if m.mtrlCompatMod != 0 {
            T0 = m.epsrox / 3.9;
            if (m.eotGiven)
                && (m.toxpGiven)
                && (m.dtoxGiven)
                && (ABS(m.eot * T0 - (m.toxp + m.dtox)) > 1.0e-20)
            {
                warnings.push("Warning: eot, toxp and dtox all given and eot * EPSROX / 3.9 != toxp + dtox; dtox ignored.".to_string());
            } else if (m.eotGiven) && (!m.toxpGiven) {
                m.toxp = T0 * m.eot - m.dtox;
            } else if (!m.eotGiven) && (m.toxpGiven) {
                m.eot = (m.toxp + m.dtox) / T0;
                if !m.toxmGiven {
                    m.toxm = m.eot;
                }
            }
        }
        if m.mtrlMod != 0 {
            epsrox = 3.9;
            toxe = m.eot;
            epssub = EPS0 * m.epsrsub;
        } else {
            epsrox = m.epsrox;
            toxe = m.toxe;
            epssub = EPSSI;
        }
        if !m.cfGiven {
            m.cf = 2.0 * epsrox * EPS0 / PI * log(1.0 + 0.4e-6 / toxe);
        }
        m.coxe = epsrox * EPS0 / toxe;
        if m.mtrlMod == 0 || m.mtrlCompatMod != 0 {
            m.coxp = m.epsrox * EPS0 / m.toxp;
        }
        if !m.cgdoGiven {
            if m.dlcGiven && (m.dlc > 0.0) {
                m.cgdo = m.dlc * m.coxe - m.cgdl;
            } else {
                m.cgdo = 0.6 * m.xj * m.coxe;
            }
        }
        if !m.cgsoGiven {
            if m.dlcGiven && (m.dlc > 0.0) {
                m.cgso = m.dlc * m.coxe - m.cgsl;
            } else {
                m.cgso = 0.6 * m.xj * m.coxe;
            }
        }
        if !m.cgboGiven {
            m.cgbo = 2.0 * m.dwc * m.coxe;
        }
        // C resets the model's size-dependent parameter cache here.
        Tnom = m.tnom;
        TRatio = Temp / Tnom;
        m.vcrit = CONSTvt0 * log(CONSTvt0 / (CONSTroot2 * 1.0e-14));
        m.factor1 = sqrt(epssub / (epsrox * EPS0) * toxe);
        m.vtm0 = KboQ * Tnom;
        Vtm0 = m.vtm0;
        if m.mtrlMod == 0 {
            Eg0 = 1.16 - 7.02e-4 * Tnom * Tnom / (Tnom + 1108.0);
            ni = 1.45e10
                * (Tnom / 300.15)
                * sqrt(Tnom / 300.15)
                * exp(21.5565981 - Eg0 / (2.0 * Vtm0));
        } else {
            Eg0 = m.bg0sub - m.tbgasub * Tnom * Tnom / (Tnom + m.tbgbsub);
            T0 = m.bg0sub - m.tbgasub * 90090.0225 / (300.15 + m.tbgbsub);
            ni = m.ni0sub * (Tnom / 300.15) * sqrt(Tnom / 300.15) * exp((T0 - Eg0) / (2.0 * Vtm0));
        }
        m.Eg0 = Eg0;
        m.vtm = KboQ * Temp;
        if m.mtrlMod == 0 {
            Eg = 1.16 - 7.02e-4 * Temp * Temp / (Temp + 1108.0);
        } else {
            Eg = m.bg0sub - m.tbgasub * Temp * Temp / (Temp + m.tbgbsub);
        }
        if Temp != Tnom {
            T0 = Eg0 / Vtm0 - Eg / m.vtm;
            T1 = log(Temp / Tnom);
            T2 = T0 + m.SjctTempExponent * T1;
            T3 = exp(T2 / m.SjctEmissionCoeff);
            m.SjctTempSatCurDensity = m.SjctSatCurDensity * T3;
            m.SjctSidewallTempSatCurDensity = m.SjctSidewallSatCurDensity * T3;
            m.SjctGateSidewallTempSatCurDensity = m.SjctGateSidewallSatCurDensity * T3;
            T2 = T0 + m.DjctTempExponent * T1;
            T3 = exp(T2 / m.DjctEmissionCoeff);
            m.DjctTempSatCurDensity = m.DjctSatCurDensity * T3;
            m.DjctSidewallTempSatCurDensity = m.DjctSidewallSatCurDensity * T3;
            m.DjctGateSidewallTempSatCurDensity = m.DjctGateSidewallSatCurDensity * T3;
        } else {
            m.SjctTempSatCurDensity = m.SjctSatCurDensity;
            m.SjctSidewallTempSatCurDensity = m.SjctSidewallSatCurDensity;
            m.SjctGateSidewallTempSatCurDensity = m.SjctGateSidewallSatCurDensity;
            m.DjctTempSatCurDensity = m.DjctSatCurDensity;
            m.DjctSidewallTempSatCurDensity = m.DjctSidewallSatCurDensity;
            m.DjctGateSidewallTempSatCurDensity = m.DjctGateSidewallSatCurDensity;
        }
        if m.SjctTempSatCurDensity < 0.0 {
            m.SjctTempSatCurDensity = 0.0;
        }
        if m.SjctSidewallTempSatCurDensity < 0.0 {
            m.SjctSidewallTempSatCurDensity = 0.0;
        }
        if m.SjctGateSidewallTempSatCurDensity < 0.0 {
            m.SjctGateSidewallTempSatCurDensity = 0.0;
        }
        if m.DjctTempSatCurDensity < 0.0 {
            m.DjctTempSatCurDensity = 0.0;
        }
        if m.DjctSidewallTempSatCurDensity < 0.0 {
            m.DjctSidewallTempSatCurDensity = 0.0;
        }
        if m.DjctGateSidewallTempSatCurDensity < 0.0 {
            m.DjctGateSidewallTempSatCurDensity = 0.0;
        }
        delTemp = temperature - m.tnom;
        T0 = m.tcj * delTemp;
        if T0 >= -1.0 {
            m.SunitAreaTempJctCap = m.SunitAreaJctCap * (1.0 + T0);
            m.DunitAreaTempJctCap = m.DunitAreaJctCap * (1.0 + T0);
        } else {
            if m.SunitAreaJctCap > 0.0 {
                m.SunitAreaTempJctCap = 0.0;
                warnings.push(
                    "Temperature effect has caused cjs to be negative. Cjs is clamped to zero."
                        .to_string(),
                );
            }
            if m.DunitAreaJctCap > 0.0 {
                m.DunitAreaTempJctCap = 0.0;
                warnings.push(
                    "Temperature effect has caused cjd to be negative. Cjd is clamped to zero."
                        .to_string(),
                );
            }
        }
        T0 = m.tcjsw * delTemp;
        if m.SunitLengthSidewallJctCap < 0.0 {
            m.SunitLengthSidewallJctCap = 0.0;
            warnings.push("CJSWS is negative. Cjsws is clamped to zero.".to_string());
        }
        if m.DunitLengthSidewallJctCap < 0.0 {
            m.DunitLengthSidewallJctCap = 0.0;
            warnings.push("CJSWD is negative. Cjswd is clamped to zero.".to_string());
        }
        if T0 >= -1.0 {
            m.SunitLengthSidewallTempJctCap = m.SunitLengthSidewallJctCap * (1.0 + T0);
            m.DunitLengthSidewallTempJctCap = m.DunitLengthSidewallJctCap * (1.0 + T0);
        } else {
            if m.SunitLengthSidewallJctCap > 0.0 {
                m.SunitLengthSidewallTempJctCap = 0.0;
                warnings.push(
                    "Temperature effect has caused cjsws to be negative. Cjsws is clamped to zero."
                        .to_string(),
                );
            }
            if m.DunitLengthSidewallJctCap > 0.0 {
                m.DunitLengthSidewallTempJctCap = 0.0;
                warnings.push(
                    "Temperature effect has caused cjswd to be negative. Cjswd is clamped to zero."
                        .to_string(),
                );
            }
        }
        T0 = m.tcjswg * delTemp;
        if T0 >= -1.0 {
            m.SunitLengthGateSidewallTempJctCap = m.SunitLengthGateSidewallJctCap * (1.0 + T0);
            m.DunitLengthGateSidewallTempJctCap = m.DunitLengthGateSidewallJctCap * (1.0 + T0);
        } else {
            if m.SunitLengthGateSidewallJctCap > 0.0 {
                m.SunitLengthGateSidewallTempJctCap = 0.0;
                warnings.push("Temperature effect has caused cjswgs to be negative. Cjswgs is clamped to zero.".to_string());
            }
            if m.DunitLengthGateSidewallJctCap > 0.0 {
                m.DunitLengthGateSidewallTempJctCap = 0.0;
                warnings.push("Temperature effect has caused cjswgd to be negative. Cjswgd is clamped to zero.".to_string());
            }
        }
        m.PhiBS = m.SbulkJctPotential - m.tpb * delTemp;
        if m.PhiBS < 0.01 {
            m.PhiBS = 0.01;
            warnings.push(
                "Temperature effect has caused pbs to be less than 0.01. Pbs is clamped to 0.01."
                    .to_string(),
            );
        }
        m.PhiBD = m.DbulkJctPotential - m.tpb * delTemp;
        if m.PhiBD < 0.01 {
            m.PhiBD = 0.01;
            warnings.push(
                "Temperature effect has caused pbd to be less than 0.01. Pbd is clamped to 0.01."
                    .to_string(),
            );
        }
        m.PhiBSWS = m.SsidewallJctPotential - m.tpbsw * delTemp;
        if m.PhiBSWS < 0.01 {
            m.PhiBSWS = 0.01;
            warnings.push("Temperature effect has caused pbsws to be less than 0.01. Pbsws is clamped to 0.01.".to_string());
        }
        m.PhiBSWD = m.DsidewallJctPotential - m.tpbsw * delTemp;
        if m.PhiBSWD < 0.01 {
            m.PhiBSWD = 0.01;
            warnings.push("Temperature effect has caused pbswd to be less than 0.01. Pbswd is clamped to 0.01.".to_string());
        }
        m.PhiBSWGS = m.SGatesidewallJctPotential - m.tpbswg * delTemp;
        if m.PhiBSWGS < 0.01 {
            m.PhiBSWGS = 0.01;
            warnings.push("Temperature effect has caused pbswgs to be less than 0.01. Pbswgs is clamped to 0.01.".to_string());
        }
        m.PhiBSWGD = m.DGatesidewallJctPotential - m.tpbswg * delTemp;
        if m.PhiBSWGD < 0.01 {
            m.PhiBSWGD = 0.01;
            warnings.push("Temperature effect has caused pbswgd to be less than 0.01. Pbswgd is clamped to 0.01.".to_string());
        }
        if m.ijthdfwd <= 0.0 {
            m.ijthdfwd = 0.0;
            warnings.push(format!("Ijthdfwd reset to {}.", c_g(m.ijthdfwd)));
        }
        if m.ijthsfwd <= 0.0 {
            m.ijthsfwd = 0.0;
            warnings.push(format!("Ijthsfwd reset to {}.", c_g(m.ijthsfwd)));
        }
        if m.ijthdrev <= 0.0 {
            m.ijthdrev = 0.0;
            warnings.push(format!("Ijthdrev reset to {}.", c_g(m.ijthdrev)));
        }
        if m.ijthsrev <= 0.0 {
            m.ijthsrev = 0.0;
            warnings.push(format!("Ijthsrev reset to {}.", c_g(m.ijthsrev)));
        }
        if (m.xjbvd <= 0.0) && (m.dioMod == 2) {
            m.xjbvd = 0.0;
            warnings.push(format!("Xjbvd reset to {}.", c_g(m.xjbvd)));
        } else if (m.xjbvd < 0.0) && (m.dioMod == 0) {
            m.xjbvd = 0.0;
            warnings.push(format!("Xjbvd reset to {}.", c_g(m.xjbvd)));
        }
        if m.bvd <= 0.0 {
            m.bvd = 0.0;
            warnings.push(format!("BVD reset to {}.", c_g(m.bvd)));
        }
        if (m.xjbvs <= 0.0) && (m.dioMod == 2) {
            m.xjbvs = 0.0;
            warnings.push(format!("Xjbvs reset to {}.", c_g(m.xjbvs)));
        } else if (m.xjbvs < 0.0) && (m.dioMod == 0) {
            m.xjbvs = 0.0;
            warnings.push(format!("Xjbvs reset to {}.", c_g(m.xjbvs)));
        }
        if m.bvs <= 0.0 {
            m.bvs = 0.0;
            warnings.push(format!("BVS reset to {}.", c_g(m.bvs)));
        }
    }
    ModelTemp {
        Temp,
        Tnom,
        TRatio,
        Vtm0,
        Eg0,
        Eg,
        ni,
        epssub,
        epsrox,
        toxe,
        delTemp,
    }
}

/// Geometry knots and the function-local Lnew retained across C's instance loop.
#[derive(Default)]
pub(crate) struct SizeCache {
    sizes: Vec<SizeParam>,
    last_length: f64,
}

impl SizeCache {
    pub(crate) fn find(&self, h: &Instance) -> Option<&SizeParam> {
        self.sizes
            .iter()
            .find(|p| p.Length == h.l && p.Width == h.w && p.NFinger == h.nf)
    }
}

/// [`temperature_instance_shared`] for an instance alone in its pass (no
/// earlier geometry knot to reuse), as verification and sensitivity replay
/// need.
///
/// # Errors
/// As [`temperature_instance_shared`].
pub(crate) fn temperature_instance(
    m: &mut Model,
    h: &mut Instance,
    locals: &ModelTemp,
    check: bool,
    model_name: &str,
    instance_name: &str,
    warnings: &mut Vec<String>,
) -> SpiceResult<SizeParam> {
    temperature_instance_shared(
        m,
        h,
        locals,
        check,
        model_name,
        instance_name,
        warnings,
        &mut SizeCache::default(),
    )
}

/// `BSIM4temp`'s instance loop body for `h`: its size-dependent parameters
/// (returned), its temperature-dependent instance values (into `h`) and,
/// when `check` is set, `BSIM4checkModel` (see [`check_model`] for which
/// instance C checks). `m` is the model after [`temperature_model`] and
/// after every instance C processed earlier in this pass: C's instance loop
/// updates the model (`njts*temp`, `dlcig`, `jtweff`, and the checked
/// instance's `b4check.c` resets), and later instances read those updates.
/// `model_name`/`instance_name` are C's `BSIM4modName`/`BSIM4name`, which
/// its messages print. Messages C prints to stdout/stderr are appended to
/// `warnings` in C order, followed by the check's nonfatal messages.
///
/// `cache` is the pass's `pSizeDependParamKnot` list and the function-local
/// `Lnew`: an instance whose geometry already has a knot reuses it without
/// running the size block, and `here->BSIM4grgeltd` then sees the `Lnew` of
/// the last knot C computed in this pass (possibly another geometry's).
/// C ignores instance `dtemp`. `Phi`/`phieot` "Fatal" messages are warnings:
/// `BSIM4temp` never reads the local `Fatal_Flag` they set.
///
/// # Errors
/// C's fatal `BSIM4checkModel` result and nonpositive effective sizes
/// (`E_BADPARM`), with C's messages.
#[allow(
    unused_assignments,
    unused_parens,
    unused_mut,
    unused_variables,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::neg_multiply,
    clippy::assign_op_pattern,
    clippy::excessive_precision,
    clippy::unnecessary_cast,
    clippy::if_same_then_else,
    clippy::manual_clamp,
    clippy::collapsible_if,
    clippy::collapsible_else_if
)] // A statement-by-statement port of C.
pub(crate) fn temperature_instance_shared(
    m: &mut Model,
    h: &mut Instance,
    locals: &ModelTemp,
    check: bool,
    model_name: &str,
    instance_name: &str,
    warnings: &mut Vec<String>,
    cache: &mut SizeCache,
) -> SpiceResult<SizeParam> {
    let cached = cache.find(h).cloned();
    let mut p = cached.clone().unwrap_or_default();
    let (mut tmp, mut tmp1, mut tmp2, mut tmp3, mut Eg, mut Eg0, mut ni, mut epssub) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut T0, mut T1, mut T2, mut T3, mut T4, mut T5, mut T6, mut T7, mut T8, mut T9) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut Lnew, mut Wnew) = (cache.last_length, 0.0_f64);
    let (mut delTemp, mut Temp, mut TRatio, mut Inv_L, mut Inv_W, mut Inv_LW, mut Vtm0, mut Tnom) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut dumPs, mut dumPd, mut dumAs, mut dumAd, mut PowWeffWr) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut DMCGeff, mut DMCIeff, mut DMDGeff) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut Nvtms, mut Nvtmd, mut SourceSatCurrent, mut DrainSatCurrent) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut T10, mut T11) = (0.0_f64, 0.0_f64);
    let (mut Inv_saref, mut Inv_sbref, mut Inv_sa, mut Inv_sb, mut rho, mut Ldrn, mut dvth0_lod) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut W_tmp, mut Inv_ODeff, mut OD_offset, mut dk2_lod, mut deta0_lod) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut lnl, mut lnw, mut lnnf, mut rbpbx, mut rbpby, mut rbsbx, mut rbsby) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut rbdbx, mut rbdby, mut bodymode) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut kvsat, mut wlod, mut sceff, mut Wdrn) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut V0, mut lt1, mut ltw, mut Theta0, mut Delt_vth, mut Vth_NarrowW, mut Lpe_Vb, mut Vth) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut n, mut n0, mut Vgsteff, mut Vgs_eff, mut niter, mut toxpf, mut toxpi, mut Tcen) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    let (mut toxe, mut epsrox, mut vddeot) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut vtfbphi2eot, mut phieot, mut TempRatioeot, mut Vtm0eot, mut Vtmeot, mut vbieot) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let mut Size_Not_Found: i32 = 0;
    let mut i: i32 = 0;
    let mut Fatal_Flag: i32 = 0;
    let ModelTemp {
        Temp,
        Tnom,
        TRatio,
        Vtm0,
        Eg0,
        Eg,
        ni,
        epssub,
        epsrox,
        toxe,
        delTemp,
    } = *locals;
    // b4temp.c pSizeDependParamKnot lookup: only new knots run the size block.
    {
        Size_Not_Found = i32::from(cached.is_none());
        Ldrn = h.l;
        Wdrn = h.w / h.nf;
        if Size_Not_Found != 0 {
            p.Length = h.l;
            p.Width = h.w;
            p.NFinger = h.nf;
            Lnew = h.l + m.xl;
            Wnew = h.w / h.nf + m.xw;
            T0 = pow(Lnew, m.Lln);
            T1 = pow(Wnew, m.Lwn);
            tmp1 = m.Ll / T0 + m.Lw / T1 + m.Lwl / (T0 * T1);
            p.dl = m.Lint + tmp1;
            tmp2 = m.Llc / T0 + m.Lwc / T1 + m.Lwlc / (T0 * T1);
            p.dlc = m.dlc + tmp2;
            T2 = pow(Lnew, m.Wln);
            T3 = pow(Wnew, m.Wwn);
            tmp1 = m.Wl / T2 + m.Ww / T3 + m.Wwl / (T2 * T3);
            p.dw = m.Wint + tmp1;
            tmp2 = m.Wlc / T2 + m.Wwc / T3 + m.Wwlc / (T2 * T3);
            p.dwc = m.dwc + tmp2;
            p.dwj = m.dwj + tmp2;
            p.leff = Lnew - 2.0 * p.dl;
            if p.leff <= 0.0 {
                return Err(bad_parameter(format!(
                    "BSIM4: mosfet {}, model {}: Effective channel length <= 0",
                    model_name, instance_name
                )));
            }
            p.weff = Wnew - 2.0 * p.dw;
            if p.weff <= 0.0 {
                return Err(bad_parameter(format!(
                    "BSIM4: mosfet {}, model {}: Effective channel width <= 0",
                    model_name, instance_name
                )));
            }
            p.leffCV = Lnew - 2.0 * p.dlc;
            if p.leffCV <= 0.0 {
                return Err(bad_parameter(format!(
                    "BSIM4: mosfet {}, model {}: Effective channel length for C-V <= 0",
                    model_name, instance_name
                )));
            }
            p.weffCV = Wnew - 2.0 * p.dwc;
            if p.weffCV <= 0.0 {
                return Err(bad_parameter(format!(
                    "BSIM4: mosfet {}, model {}: Effective channel width for C-V <= 0",
                    model_name, instance_name
                )));
            }
            p.weffCJ = Wnew - 2.0 * p.dwj;
            if p.weffCJ <= 0.0 {
                return Err(bad_parameter(format!(
                    "BSIM4: mosfet {}, model {}: Effective channel width for S/D junctions <= 0",
                    model_name, instance_name
                )));
            }
            if m.binUnit == 1 {
                Inv_L = 1.0e-6 / p.leff;
                Inv_W = 1.0e-6 / p.weff;
                Inv_LW = 1.0e-12 / (p.leff * p.weff);
            } else {
                Inv_L = 1.0 / p.leff;
                Inv_W = 1.0 / p.weff;
                Inv_LW = 1.0 / (p.leff * p.weff);
            }
            p.cdsc = m.cdsc + m.lcdsc * Inv_L + m.wcdsc * Inv_W + m.pcdsc * Inv_LW;
            p.cdscb = m.cdscb + m.lcdscb * Inv_L + m.wcdscb * Inv_W + m.pcdscb * Inv_LW;
            p.cdscd = m.cdscd + m.lcdscd * Inv_L + m.wcdscd * Inv_W + m.pcdscd * Inv_LW;
            p.cit = m.cit + m.lcit * Inv_L + m.wcit * Inv_W + m.pcit * Inv_LW;
            p.nfactor = m.nfactor + m.lnfactor * Inv_L + m.wnfactor * Inv_W + m.pnfactor * Inv_LW;
            p.tnfactor =
                m.tnfactor + m.ltnfactor * Inv_L + m.wtnfactor * Inv_W + m.ptnfactor * Inv_LW;
            p.xj = m.xj + m.lxj * Inv_L + m.wxj * Inv_W + m.pxj * Inv_LW;
            p.vsat = m.vsat + m.lvsat * Inv_L + m.wvsat * Inv_W + m.pvsat * Inv_LW;
            p.at = m.at + m.lat * Inv_L + m.wat * Inv_W + m.pat * Inv_LW;
            p.a0 = m.a0 + m.la0 * Inv_L + m.wa0 * Inv_W + m.pa0 * Inv_LW;
            p.ags = m.ags + m.lags * Inv_L + m.wags * Inv_W + m.pags * Inv_LW;
            p.a1 = m.a1 + m.la1 * Inv_L + m.wa1 * Inv_W + m.pa1 * Inv_LW;
            p.a2 = m.a2 + m.la2 * Inv_L + m.wa2 * Inv_W + m.pa2 * Inv_LW;
            p.keta = m.keta + m.lketa * Inv_L + m.wketa * Inv_W + m.pketa * Inv_LW;
            p.ketac = m.ketac + m.lketac * Inv_L + m.wketac * Inv_W + m.pketac * Inv_LW;
            p.nsub = m.nsub + m.lnsub * Inv_L + m.wnsub * Inv_W + m.pnsub * Inv_LW;
            p.ndep = m.ndep + m.lndep * Inv_L + m.wndep * Inv_W + m.pndep * Inv_LW;
            p.nsd = m.nsd + m.lnsd * Inv_L + m.wnsd * Inv_W + m.pnsd * Inv_LW;
            p.phin = m.phin + m.lphin * Inv_L + m.wphin * Inv_W + m.pphin * Inv_LW;
            p.ngate = m.ngate + m.lngate * Inv_L + m.wngate * Inv_W + m.pngate * Inv_LW;
            p.gamma1 = m.gamma1 + m.lgamma1 * Inv_L + m.wgamma1 * Inv_W + m.pgamma1 * Inv_LW;
            p.gamma2 = m.gamma2 + m.lgamma2 * Inv_L + m.wgamma2 * Inv_W + m.pgamma2 * Inv_LW;
            p.vbx = m.vbx + m.lvbx * Inv_L + m.wvbx * Inv_W + m.pvbx * Inv_LW;
            p.vbm = m.vbm + m.lvbm * Inv_L + m.wvbm * Inv_W + m.pvbm * Inv_LW;
            p.xt = m.xt + m.lxt * Inv_L + m.wxt * Inv_W + m.pxt * Inv_LW;
            p.vfb = m.vfb + m.lvfb * Inv_L + m.wvfb * Inv_W + m.pvfb * Inv_LW;
            p.k1 = m.k1 + m.lk1 * Inv_L + m.wk1 * Inv_W + m.pk1 * Inv_LW;
            p.kt1 = m.kt1 + m.lkt1 * Inv_L + m.wkt1 * Inv_W + m.pkt1 * Inv_LW;
            p.kt1l = m.kt1l + m.lkt1l * Inv_L + m.wkt1l * Inv_W + m.pkt1l * Inv_LW;
            p.k2 = m.k2 + m.lk2 * Inv_L + m.wk2 * Inv_W + m.pk2 * Inv_LW;
            p.kt2 = m.kt2 + m.lkt2 * Inv_L + m.wkt2 * Inv_W + m.pkt2 * Inv_LW;
            p.k3 = m.k3 + m.lk3 * Inv_L + m.wk3 * Inv_W + m.pk3 * Inv_LW;
            p.k3b = m.k3b + m.lk3b * Inv_L + m.wk3b * Inv_W + m.pk3b * Inv_LW;
            p.w0 = m.w0 + m.lw0 * Inv_L + m.ww0 * Inv_W + m.pw0 * Inv_LW;
            p.lpe0 = m.lpe0 + m.llpe0 * Inv_L + m.wlpe0 * Inv_W + m.plpe0 * Inv_LW;
            p.lpeb = m.lpeb + m.llpeb * Inv_L + m.wlpeb * Inv_W + m.plpeb * Inv_LW;
            p.dvtp0 = m.dvtp0 + m.ldvtp0 * Inv_L + m.wdvtp0 * Inv_W + m.pdvtp0 * Inv_LW;
            p.dvtp1 = m.dvtp1 + m.ldvtp1 * Inv_L + m.wdvtp1 * Inv_W + m.pdvtp1 * Inv_LW;
            p.dvtp2 = m.dvtp2 + m.ldvtp2 * Inv_L + m.wdvtp2 * Inv_W + m.pdvtp2 * Inv_LW;
            p.dvtp3 = m.dvtp3 + m.ldvtp3 * Inv_L + m.wdvtp3 * Inv_W + m.pdvtp3 * Inv_LW;
            p.dvtp4 = m.dvtp4 + m.ldvtp4 * Inv_L + m.wdvtp4 * Inv_W + m.pdvtp4 * Inv_LW;
            p.dvtp5 = m.dvtp5 + m.ldvtp5 * Inv_L + m.wdvtp5 * Inv_W + m.pdvtp5 * Inv_LW;
            p.dvt0 = m.dvt0 + m.ldvt0 * Inv_L + m.wdvt0 * Inv_W + m.pdvt0 * Inv_LW;
            p.dvt1 = m.dvt1 + m.ldvt1 * Inv_L + m.wdvt1 * Inv_W + m.pdvt1 * Inv_LW;
            p.dvt2 = m.dvt2 + m.ldvt2 * Inv_L + m.wdvt2 * Inv_W + m.pdvt2 * Inv_LW;
            p.dvt0w = m.dvt0w + m.ldvt0w * Inv_L + m.wdvt0w * Inv_W + m.pdvt0w * Inv_LW;
            p.dvt1w = m.dvt1w + m.ldvt1w * Inv_L + m.wdvt1w * Inv_W + m.pdvt1w * Inv_LW;
            p.dvt2w = m.dvt2w + m.ldvt2w * Inv_L + m.wdvt2w * Inv_W + m.pdvt2w * Inv_LW;
            p.drout = m.drout + m.ldrout * Inv_L + m.wdrout * Inv_W + m.pdrout * Inv_LW;
            p.dsub = m.dsub + m.ldsub * Inv_L + m.wdsub * Inv_W + m.pdsub * Inv_LW;
            p.vth0 = m.vth0 + m.lvth0 * Inv_L + m.wvth0 * Inv_W + m.pvth0 * Inv_LW;
            p.ua = m.ua + m.lua * Inv_L + m.wua * Inv_W + m.pua * Inv_LW;
            p.ua1 = m.ua1 + m.lua1 * Inv_L + m.wua1 * Inv_W + m.pua1 * Inv_LW;
            p.ub = m.ub + m.lub * Inv_L + m.wub * Inv_W + m.r#pub * Inv_LW;
            p.ub1 = m.ub1 + m.lub1 * Inv_L + m.wub1 * Inv_W + m.pub1 * Inv_LW;
            p.uc = m.uc + m.luc * Inv_L + m.wuc * Inv_W + m.puc * Inv_LW;
            p.uc1 = m.uc1 + m.luc1 * Inv_L + m.wuc1 * Inv_W + m.puc1 * Inv_LW;
            p.ud = m.ud + m.lud * Inv_L + m.wud * Inv_W + m.pud * Inv_LW;
            p.ud1 = m.ud1 + m.lud1 * Inv_L + m.wud1 * Inv_W + m.pud1 * Inv_LW;
            p.up = m.up + m.lup * Inv_L + m.wup * Inv_W + m.pup * Inv_LW;
            p.lp = m.lp + m.llp * Inv_L + m.wlp * Inv_W + m.plp * Inv_LW;
            p.eu = m.eu + m.leu * Inv_L + m.weu * Inv_W + m.peu * Inv_LW;
            p.u0 = m.u0 + m.lu0 * Inv_L + m.wu0 * Inv_W + m.pu0 * Inv_LW;
            p.ute = m.ute + m.lute * Inv_L + m.wute * Inv_W + m.pute * Inv_LW;
            p.ucs = m.ucs + m.lucs * Inv_L + m.wucs * Inv_W + m.pucs * Inv_LW;
            p.ucste = m.ucste + m.lucste * Inv_L + m.wucste * Inv_W + m.pucste * Inv_LW;
            p.voff = m.voff + m.lvoff * Inv_L + m.wvoff * Inv_W + m.pvoff * Inv_LW;
            p.tvoff = m.tvoff + m.ltvoff * Inv_L + m.wtvoff * Inv_W + m.ptvoff * Inv_LW;
            p.minv = m.minv + m.lminv * Inv_L + m.wminv * Inv_W + m.pminv * Inv_LW;
            p.minvcv = m.minvcv + m.lminvcv * Inv_L + m.wminvcv * Inv_W + m.pminvcv * Inv_LW;
            p.fprout = m.fprout + m.lfprout * Inv_L + m.wfprout * Inv_W + m.pfprout * Inv_LW;
            p.pdits = m.pdits + m.lpdits * Inv_L + m.wpdits * Inv_W + m.ppdits * Inv_LW;
            p.pditsd = m.pditsd + m.lpditsd * Inv_L + m.wpditsd * Inv_W + m.ppditsd * Inv_LW;
            p.delta = m.delta + m.ldelta * Inv_L + m.wdelta * Inv_W + m.pdelta * Inv_LW;
            p.rdsw = m.rdsw + m.lrdsw * Inv_L + m.wrdsw * Inv_W + m.prdsw * Inv_LW;
            p.rdw = m.rdw + m.lrdw * Inv_L + m.wrdw * Inv_W + m.prdw * Inv_LW;
            p.rsw = m.rsw + m.lrsw * Inv_L + m.wrsw * Inv_W + m.prsw * Inv_LW;
            p.prwg = m.prwg + m.lprwg * Inv_L + m.wprwg * Inv_W + m.pprwg * Inv_LW;
            p.prwb = m.prwb + m.lprwb * Inv_L + m.wprwb * Inv_W + m.pprwb * Inv_LW;
            p.prt = m.prt + m.lprt * Inv_L + m.wprt * Inv_W + m.pprt * Inv_LW;
            p.eta0 = m.eta0 + m.leta0 * Inv_L + m.weta0 * Inv_W + m.peta0 * Inv_LW;
            p.teta0 = m.teta0 + m.lteta0 * Inv_L + m.wteta0 * Inv_W + m.pteta0 * Inv_LW;
            p.tvoffcv = m.tvoffcv + m.ltvoffcv * Inv_L + m.wtvoffcv * Inv_W + m.ptvoffcv * Inv_LW;
            p.etab = m.etab + m.letab * Inv_L + m.wetab * Inv_W + m.petab * Inv_LW;
            p.pclm = m.pclm + m.lpclm * Inv_L + m.wpclm * Inv_W + m.ppclm * Inv_LW;
            p.pdibl1 = m.pdibl1 + m.lpdibl1 * Inv_L + m.wpdibl1 * Inv_W + m.ppdibl1 * Inv_LW;
            p.pdibl2 = m.pdibl2 + m.lpdibl2 * Inv_L + m.wpdibl2 * Inv_W + m.ppdibl2 * Inv_LW;
            p.pdiblb = m.pdiblb + m.lpdiblb * Inv_L + m.wpdiblb * Inv_W + m.ppdiblb * Inv_LW;
            p.pscbe1 = m.pscbe1 + m.lpscbe1 * Inv_L + m.wpscbe1 * Inv_W + m.ppscbe1 * Inv_LW;
            p.pscbe2 = m.pscbe2 + m.lpscbe2 * Inv_L + m.wpscbe2 * Inv_W + m.ppscbe2 * Inv_LW;
            p.pvag = m.pvag + m.lpvag * Inv_L + m.wpvag * Inv_W + m.ppvag * Inv_LW;
            p.wr = m.wr + m.lwr * Inv_L + m.wwr * Inv_W + m.pwr * Inv_LW;
            p.dwg = m.dwg + m.ldwg * Inv_L + m.wdwg * Inv_W + m.pdwg * Inv_LW;
            p.dwb = m.dwb + m.ldwb * Inv_L + m.wdwb * Inv_W + m.pdwb * Inv_LW;
            p.b0 = m.b0 + m.lb0 * Inv_L + m.wb0 * Inv_W + m.pb0 * Inv_LW;
            p.b1 = m.b1 + m.lb1 * Inv_L + m.wb1 * Inv_W + m.pb1 * Inv_LW;
            p.alpha0 = m.alpha0 + m.lalpha0 * Inv_L + m.walpha0 * Inv_W + m.palpha0 * Inv_LW;
            p.alpha1 = m.alpha1 + m.lalpha1 * Inv_L + m.walpha1 * Inv_W + m.palpha1 * Inv_LW;
            p.beta0 = m.beta0 + m.lbeta0 * Inv_L + m.wbeta0 * Inv_W + m.pbeta0 * Inv_LW;
            p.agidl = m.agidl + m.lagidl * Inv_L + m.wagidl * Inv_W + m.pagidl * Inv_LW;
            p.bgidl = m.bgidl + m.lbgidl * Inv_L + m.wbgidl * Inv_W + m.pbgidl * Inv_LW;
            p.cgidl = m.cgidl + m.lcgidl * Inv_L + m.wcgidl * Inv_W + m.pcgidl * Inv_LW;
            p.egidl = m.egidl + m.legidl * Inv_L + m.wegidl * Inv_W + m.pegidl * Inv_LW;
            p.rgidl = m.rgidl + m.lrgidl * Inv_L + m.wrgidl * Inv_W + m.prgidl * Inv_LW;
            p.kgidl = m.kgidl + m.lkgidl * Inv_L + m.wkgidl * Inv_W + m.pkgidl * Inv_LW;
            p.fgidl = m.fgidl + m.lfgidl * Inv_L + m.wfgidl * Inv_W + m.pfgidl * Inv_LW;
            p.agisl = m.agisl + m.lagisl * Inv_L + m.wagisl * Inv_W + m.pagisl * Inv_LW;
            p.bgisl = m.bgisl + m.lbgisl * Inv_L + m.wbgisl * Inv_W + m.pbgisl * Inv_LW;
            p.cgisl = m.cgisl + m.lcgisl * Inv_L + m.wcgisl * Inv_W + m.pcgisl * Inv_LW;
            p.egisl = m.egisl + m.legisl * Inv_L + m.wegisl * Inv_W + m.pegisl * Inv_LW;
            p.rgisl = m.rgisl + m.lrgisl * Inv_L + m.wrgisl * Inv_W + m.prgisl * Inv_LW;
            p.kgisl = m.kgisl + m.lkgisl * Inv_L + m.wkgisl * Inv_W + m.pkgisl * Inv_LW;
            p.fgisl = m.fgisl + m.lfgisl * Inv_L + m.wfgisl * Inv_W + m.pfgisl * Inv_LW;
            p.aigc = m.aigc + m.laigc * Inv_L + m.waigc * Inv_W + m.paigc * Inv_LW;
            p.bigc = m.bigc + m.lbigc * Inv_L + m.wbigc * Inv_W + m.pbigc * Inv_LW;
            p.cigc = m.cigc + m.lcigc * Inv_L + m.wcigc * Inv_W + m.pcigc * Inv_LW;
            p.aigsd = m.aigsd + m.laigsd * Inv_L + m.waigsd * Inv_W + m.paigsd * Inv_LW;
            p.bigsd = m.bigsd + m.lbigsd * Inv_L + m.wbigsd * Inv_W + m.pbigsd * Inv_LW;
            p.cigsd = m.cigsd + m.lcigsd * Inv_L + m.wcigsd * Inv_W + m.pcigsd * Inv_LW;
            p.aigs = m.aigs + m.laigs * Inv_L + m.waigs * Inv_W + m.paigs * Inv_LW;
            p.bigs = m.bigs + m.lbigs * Inv_L + m.wbigs * Inv_W + m.pbigs * Inv_LW;
            p.cigs = m.cigs + m.lcigs * Inv_L + m.wcigs * Inv_W + m.pcigs * Inv_LW;
            p.aigd = m.aigd + m.laigd * Inv_L + m.waigd * Inv_W + m.paigd * Inv_LW;
            p.bigd = m.bigd + m.lbigd * Inv_L + m.wbigd * Inv_W + m.pbigd * Inv_LW;
            p.cigd = m.cigd + m.lcigd * Inv_L + m.wcigd * Inv_W + m.pcigd * Inv_LW;
            p.aigbacc = m.aigbacc + m.laigbacc * Inv_L + m.waigbacc * Inv_W + m.paigbacc * Inv_LW;
            p.bigbacc = m.bigbacc + m.lbigbacc * Inv_L + m.wbigbacc * Inv_W + m.pbigbacc * Inv_LW;
            p.cigbacc = m.cigbacc + m.lcigbacc * Inv_L + m.wcigbacc * Inv_W + m.pcigbacc * Inv_LW;
            p.aigbinv = m.aigbinv + m.laigbinv * Inv_L + m.waigbinv * Inv_W + m.paigbinv * Inv_LW;
            p.bigbinv = m.bigbinv + m.lbigbinv * Inv_L + m.wbigbinv * Inv_W + m.pbigbinv * Inv_LW;
            p.cigbinv = m.cigbinv + m.lcigbinv * Inv_L + m.wcigbinv * Inv_W + m.pcigbinv * Inv_LW;
            p.nigc = m.nigc + m.lnigc * Inv_L + m.wnigc * Inv_W + m.pnigc * Inv_LW;
            p.nigbacc = m.nigbacc + m.lnigbacc * Inv_L + m.wnigbacc * Inv_W + m.pnigbacc * Inv_LW;
            p.nigbinv = m.nigbinv + m.lnigbinv * Inv_L + m.wnigbinv * Inv_W + m.pnigbinv * Inv_LW;
            p.ntox = m.ntox + m.lntox * Inv_L + m.wntox * Inv_W + m.pntox * Inv_LW;
            p.eigbinv = m.eigbinv + m.leigbinv * Inv_L + m.weigbinv * Inv_W + m.peigbinv * Inv_LW;
            p.pigcd = m.pigcd + m.lpigcd * Inv_L + m.wpigcd * Inv_W + m.ppigcd * Inv_LW;
            p.poxedge = m.poxedge + m.lpoxedge * Inv_L + m.wpoxedge * Inv_W + m.ppoxedge * Inv_LW;
            p.xrcrg1 = m.xrcrg1 + m.lxrcrg1 * Inv_L + m.wxrcrg1 * Inv_W + m.pxrcrg1 * Inv_LW;
            p.xrcrg2 = m.xrcrg2 + m.lxrcrg2 * Inv_L + m.wxrcrg2 * Inv_W + m.pxrcrg2 * Inv_LW;
            p.lambda = m.lambda + m.llambda * Inv_L + m.wlambda * Inv_W + m.plambda * Inv_LW;
            p.vtl = m.vtl + m.lvtl * Inv_L + m.wvtl * Inv_W + m.pvtl * Inv_LW;
            p.xn = m.xn + m.lxn * Inv_L + m.wxn * Inv_W + m.pxn * Inv_LW;
            p.vfbsdoff =
                m.vfbsdoff + m.lvfbsdoff * Inv_L + m.wvfbsdoff * Inv_W + m.pvfbsdoff * Inv_LW;
            p.tvfbsdoff =
                m.tvfbsdoff + m.ltvfbsdoff * Inv_L + m.wtvfbsdoff * Inv_W + m.ptvfbsdoff * Inv_LW;
            p.cgsl = m.cgsl + m.lcgsl * Inv_L + m.wcgsl * Inv_W + m.pcgsl * Inv_LW;
            p.cgdl = m.cgdl + m.lcgdl * Inv_L + m.wcgdl * Inv_W + m.pcgdl * Inv_LW;
            p.ckappas = m.ckappas + m.lckappas * Inv_L + m.wckappas * Inv_W + m.pckappas * Inv_LW;
            p.ckappad = m.ckappad + m.lckappad * Inv_L + m.wckappad * Inv_W + m.pckappad * Inv_LW;
            p.cf = m.cf + m.lcf * Inv_L + m.wcf * Inv_W + m.pcf * Inv_LW;
            p.clc = m.clc + m.lclc * Inv_L + m.wclc * Inv_W + m.pclc * Inv_LW;
            p.cle = m.cle + m.lcle * Inv_L + m.wcle * Inv_W + m.pcle * Inv_LW;
            p.vfbcv = m.vfbcv + m.lvfbcv * Inv_L + m.wvfbcv * Inv_W + m.pvfbcv * Inv_LW;
            p.acde = m.acde + m.lacde * Inv_L + m.wacde * Inv_W + m.pacde * Inv_LW;
            p.moin = m.moin + m.lmoin * Inv_L + m.wmoin * Inv_W + m.pmoin * Inv_LW;
            p.noff = m.noff + m.lnoff * Inv_L + m.wnoff * Inv_W + m.pnoff * Inv_LW;
            p.voffcv = m.voffcv + m.lvoffcv * Inv_L + m.wvoffcv * Inv_W + m.pvoffcv * Inv_LW;
            p.kvth0we = m.kvth0we + m.lkvth0we * Inv_L + m.wkvth0we * Inv_W + m.pkvth0we * Inv_LW;
            p.k2we = m.k2we + m.lk2we * Inv_L + m.wk2we * Inv_W + m.pk2we * Inv_LW;
            p.ku0we = m.ku0we + m.lku0we * Inv_L + m.wku0we * Inv_W + m.pku0we * Inv_LW;
            p.abulkCVfactor = 1.0 + pow((p.clc / p.leffCV), p.cle);
            T0 = (TRatio - 1.0);
            PowWeffWr = pow(p.weffCJ * 1.0e6, p.wr) * h.nf;
            T4 = 0.0;
            T3 = T4;
            T2 = T4;
            T1 = T4;
            p.ucs = p.ucs * pow(TRatio, p.ucste);
            if m.tempMod == 0 {
                p.ua = p.ua + p.ua1 * T0;
                p.ub = p.ub + p.ub1 * T0;
                p.uc = p.uc + p.uc1 * T0;
                p.ud = p.ud + p.ud1 * T0;
                p.vsattemp = p.vsat - p.at * T0;
                T10 = p.prt * T0;
                if m.rdsMod != 0 {
                    T1 = p.rdw + T10;
                    T2 = m.rdwmin + T10;
                    T3 = p.rsw + T10;
                    T4 = m.rswmin + T10;
                }
                p.rds0 = (p.rdsw + T10) * h.nf / PowWeffWr;
                p.rdswmin = (m.rdswmin + T10) * h.nf / PowWeffWr;
            } else {
                if m.tempMod == 3 {
                    p.ua = p.ua * pow(TRatio, p.ua1);
                    p.ub = p.ub * pow(TRatio, p.ub1);
                    p.uc = p.uc * pow(TRatio, p.uc1);
                    p.ud = p.ud * pow(TRatio, p.ud1);
                } else {
                    p.ua = p.ua * (1.0 + p.ua1 * delTemp);
                    p.ub = p.ub * (1.0 + p.ub1 * delTemp);
                    p.uc = p.uc * (1.0 + p.uc1 * delTemp);
                    p.ud = p.ud * (1.0 + p.ud1 * delTemp);
                }
                p.vsattemp = p.vsat * (1.0 - p.at * delTemp);
                T10 = 1.0 + p.prt * delTemp;
                if m.rdsMod != 0 {
                    T1 = p.rdw * T10;
                    T2 = m.rdwmin * T10;
                    T3 = p.rsw * T10;
                    T4 = m.rswmin * T10;
                }
                p.rds0 = p.rdsw * T10 * h.nf / PowWeffWr;
                p.rdswmin = m.rdswmin * T10 * h.nf / PowWeffWr;
            }
            if T1 < 0.0 {
                T1 = 0.0;
                warnings
                    .push("Warning: Rdw at current temperature is negative; set to 0.".to_string());
            }
            if T2 < 0.0 {
                T2 = 0.0;
                warnings.push(
                    "Warning: Rdwmin at current temperature is negative; set to 0.".to_string(),
                );
            }
            p.rd0 = T1 / PowWeffWr;
            p.rdwmin = T2 / PowWeffWr;
            if T3 < 0.0 {
                T3 = 0.0;
                warnings
                    .push("Warning: Rsw at current temperature is negative; set to 0.".to_string());
            }
            if T4 < 0.0 {
                T4 = 0.0;
                warnings.push(
                    "Warning: Rswmin at current temperature is negative; set to 0.".to_string(),
                );
            }
            p.rs0 = T3 / PowWeffWr;
            p.rswmin = T4 / PowWeffWr;
            if p.u0 > 1.0 {
                p.u0 = p.u0 / 1.0e4;
            }
            T5 = 1.0 - p.up * exp(-p.leff / p.lp);
            p.u0temp = p.u0 * T5 * pow(TRatio, p.ute);
            if p.eu < 0.0 {
                p.eu = 0.0;
                warnings.push("Warning: eu has been negative; reset to 0.0.".to_string());
            }
            if p.ucs < 0.0 {
                p.ucs = 0.0;
                warnings.push("Warning: ucs has been negative; reset to 0.0.".to_string());
            }
            p.vfbsdoff = p.vfbsdoff * (1.0 + p.tvfbsdoff * delTemp);
            p.voff = p.voff * (1.0 + p.tvoff * delTemp);
            p.nfactor = p.nfactor + p.tnfactor * delTemp / Tnom;
            p.voffcv = p.voffcv * (1.0 + p.tvoffcv * delTemp);
            p.eta0 = p.eta0 + p.teta0 * delTemp / Tnom;
            if (m.vtlGiven) && (m.vtl > 0.0) {
                if m.lc < 0.0 {
                    p.lc = 0.0;
                } else {
                    p.lc = m.lc;
                }
                T0 = p.leff / (p.xn * p.leff + p.lc);
                p.tfactor = (1.0 - T0) / (1.0 + T0);
            }
            p.cgdo = (m.cgdo + p.cf) * p.weffCV;
            p.cgso = (m.cgso + p.cf) * p.weffCV;
            p.cgbo = m.cgbo * p.leffCV * h.nf;
            if !m.ndepGiven && m.gamma1Given {
                T0 = p.gamma1 * m.coxe;
                p.ndep = 3.01248e22 * T0 * T0;
            }
            p.phi = Vtm0 * log(p.ndep / ni) + p.phin + 0.4;
            if p.phi <= 0.0 {
                warnings.push(format!(
                    "Fatal: Phi = {} is not positive. Please check Phin and Ndep",
                    c_g(p.phi)
                ));
                warnings.push(format!(
                    "       Phin = {}  Ndep = {} ",
                    c_g(p.phin),
                    c_g(p.ndep)
                ));
                Fatal_Flag = 1;
            }
            p.sqrtPhi = sqrt(p.phi);
            p.phis3 = p.sqrtPhi * p.phi;
            p.Xdep0 = sqrt(2.0 * epssub / (Charge_q * p.ndep * 1.0e6)) * p.sqrtPhi;
            p.sqrtXdep0 = sqrt(p.Xdep0);
            if m.mtrlMod == 0 {
                p.litl = sqrt(3.0 * 3.9 / epsrox * p.xj * toxe);
            } else {
                p.litl = sqrt(m.epsrsub / epsrox * p.xj * toxe);
            }
            p.vbi = Vtm0 * log(p.nsd * p.ndep / (ni * ni));
            if m.mtrlMod == 0 {
                if p.ngate > 0.0 {
                    p.vfbsd = Vtm0 * log(p.ngate / p.nsd);
                } else {
                    p.vfbsd = 0.0;
                }
            } else {
                T0 = Vtm0 * log(p.nsd / ni);
                T1 = 0.5 * Eg0;
                if T0 > T1 {
                    T0 = T1;
                }
                T2 = m.easub + T1 - f64::from(m.r#type) * T0;
                p.vfbsd = m.phig - T2;
            }
            p.cdep0 = sqrt(Charge_q * epssub * p.ndep * 1.0e6 / 2.0 / p.phi);
            p.ToxRatio = exp(p.ntox * log(m.toxref / toxe)) / toxe / toxe;
            p.ToxRatioEdge = exp(p.ntox * log(m.toxref / (toxe * p.poxedge)))
                / toxe
                / toxe
                / p.poxedge
                / p.poxedge;
            p.Aechvb = if m.r#type == NMOS {
                4.97232e-7
            } else {
                3.42537e-7
            };
            p.Bechvb = if m.r#type == NMOS {
                7.45669e11
            } else {
                1.16645e12
            };
            if m.v48intVersion <= BSIM4v48V480 {
                p.AechvbEdgeS = p.Aechvb * p.weff * m.dlcig * p.ToxRatioEdge;
                p.AechvbEdgeD = p.Aechvb * p.weff * m.dlcigd * p.ToxRatioEdge;
            } else {
                if m.dlcig < 0.0 {
                    warnings.push(format!(
                        "Warning: dlcig = {} is negative. Set to zero.",
                        c_g(m.dlcig)
                    ));
                    m.dlcig = 0.0;
                }
                p.AechvbEdgeS = p.Aechvb * p.weff * m.dlcig * p.ToxRatioEdge;
                if m.dlcigd < 0.0 {
                    warnings.push(format!(
                        "Warning: dlcigd = {} is negative. Set to zero.",
                        c_g(m.dlcigd)
                    ));
                    m.dlcigd = 0.0;
                }
                p.AechvbEdgeD = p.Aechvb * p.weff * m.dlcigd * p.ToxRatioEdge;
            }
            p.BechvbEdge = -p.Bechvb * toxe * p.poxedge;
            p.Aechvb *= p.weff * p.leff * p.ToxRatio;
            p.Bechvb *= -toxe;
            p.mstar = 0.5 + atan(p.minv) / PI;
            p.mstarcv = 0.5 + atan(p.minvcv) / PI;
            p.voffcbn = p.voff + m.voffl / p.leff;
            p.voffcbncv = p.voffcv + m.voffcvl / p.leff;
            p.ldeb = sqrt(epssub * Vtm0 / (Charge_q * p.ndep * 1.0e6)) / 3.0;
            p.acde *= pow((p.ndep / 2.0e16), -0.25);
            if m.k1Given || m.k2Given {
                if !m.k1Given {
                    warnings.push("Warning: k1 should be specified with k2.".to_string());
                    p.k1 = 0.53;
                }
                if !m.k2Given {
                    warnings.push("Warning: k2 should be specified with k1.".to_string());
                    p.k2 = -0.0186;
                }
                if m.nsubGiven {
                    warnings
                        .push("Warning: nsub is ignored because k1 or k2 is given.".to_string());
                }
                if m.xtGiven {
                    warnings.push("Warning: xt is ignored because k1 or k2 is given.".to_string());
                }
                if m.vbxGiven {
                    warnings.push("Warning: vbx is ignored because k1 or k2 is given.".to_string());
                }
                if m.gamma1Given {
                    warnings
                        .push("Warning: gamma1 is ignored because k1 or k2 is given.".to_string());
                }
                if m.gamma2Given {
                    warnings
                        .push("Warning: gamma2 is ignored because k1 or k2 is given.".to_string());
                }
            } else {
                if !m.vbxGiven {
                    p.vbx = p.phi - 7.7348e-4 * p.ndep * p.xt * p.xt;
                }
                if p.vbx > 0.0 {
                    p.vbx = -p.vbx;
                }
                if p.vbm > 0.0 {
                    p.vbm = -p.vbm;
                }
                if !m.gamma1Given {
                    p.gamma1 = 5.753e-12 * sqrt(p.ndep) / m.coxe;
                }
                if !m.gamma2Given {
                    p.gamma2 = 5.753e-12 * sqrt(p.nsub) / m.coxe;
                }
                T0 = p.gamma1 - p.gamma2;
                T1 = sqrt(p.phi - p.vbx) - p.sqrtPhi;
                T2 = sqrt(p.phi * (p.phi - p.vbm)) - p.phi;
                p.k2 = T0 * T1 / (2.0 * T2 + p.vbm);
                p.k1 = p.gamma2 - 2.0 * p.k2 * sqrt(p.phi - p.vbm);
            }
            if !m.vfbGiven {
                if m.vth0Given {
                    p.vfb = f64::from(m.r#type) * p.vth0 - p.phi - p.k1 * p.sqrtPhi;
                } else {
                    if (m.mtrlMod != 0) && (m.phigGiven) && (m.nsubGiven) {
                        T0 = Vtm0 * log(p.nsub / ni);
                        T1 = 0.5 * Eg0;
                        if T0 > T1 {
                            T0 = T1;
                        }
                        T2 = m.easub + T1 + f64::from(m.r#type) * T0;
                        p.vfb = m.phig - T2;
                    } else {
                        p.vfb = -1.0;
                    }
                }
            }
            if !m.vth0Given {
                p.vth0 = f64::from(m.r#type) * (p.vfb + p.phi + p.k1 * p.sqrtPhi);
            }
            p.k1ox = p.k1 * toxe / m.toxm;
            tmp = sqrt(epssub / (epsrox * EPS0) * toxe * p.Xdep0);
            T0 = p.dsub * p.leff / tmp;
            if T0 < EXP_THRESHOLD {
                T1 = exp(T0);
                T2 = T1 - 1.0;
                T3 = T2 * T2;
                T4 = T3 + 2.0 * T1 * MIN_EXP;
                p.theta0vb0 = T1 / T4;
            } else {
                p.theta0vb0 = 1.0 / (MAX_EXP - 2.0);
            }
            T0 = p.drout * p.leff / tmp;
            if T0 < EXP_THRESHOLD {
                T1 = exp(T0);
                T2 = T1 - 1.0;
                T3 = T2 * T2;
                T4 = T3 + 2.0 * T1 * MIN_EXP;
                T5 = T1 / T4;
            } else {
                T5 = 1.0 / (MAX_EXP - 2.0);
            }
            p.thetaRout = p.pdibl1 * T5 + p.pdibl2;
            tmp = sqrt(p.Xdep0);
            tmp1 = p.vbi - p.phi;
            tmp2 = m.factor1 * tmp;
            T0 = p.dvt1w * p.weff * p.leff / tmp2;
            if T0 < EXP_THRESHOLD {
                T1 = exp(T0);
                T2 = T1 - 1.0;
                T3 = T2 * T2;
                T4 = T3 + 2.0 * T1 * MIN_EXP;
                T8 = T1 / T4;
            } else {
                T8 = 1.0 / (MAX_EXP - 2.0);
            }
            T0 = p.dvt0w * T8;
            T8 = T0 * tmp1;
            T0 = p.dvt1 * p.leff / tmp2;
            if T0 < EXP_THRESHOLD {
                T1 = exp(T0);
                T2 = T1 - 1.0;
                T3 = T2 * T2;
                T4 = T3 + 2.0 * T1 * MIN_EXP;
                T9 = T1 / T4;
            } else {
                T9 = 1.0 / (MAX_EXP - 2.0);
            }
            T9 = p.dvt0 * T9 * tmp1;
            T4 = toxe * p.phi / (p.weff + p.w0);
            T0 = sqrt(1.0 + p.lpe0 / p.leff);
            if (m.tempMod == 1) || (m.tempMod == 0) {
                T3 = (p.kt1 + p.kt1l / p.leff) * (TRatio - 1.0);
            }
            if (m.tempMod == 2) || (m.tempMod == 3) {
                T3 = -p.kt1 * (TRatio - 1.0);
            }
            T5 = p.k1ox * (T0 - 1.0) * p.sqrtPhi + T3;
            p.vfbzbfactor = -T8 - T9 + p.k3 * T4 + T5 - p.phi - p.k1 * p.sqrtPhi;
            wlod = m.wlod;
            if m.wlod < 0.0 {
                warnings.push(format!(
                    "Warning: WLOD = {} is less than 0. 0.0 is used",
                    c_g(m.wlod)
                ));
                wlod = 0.0;
            }
            T0 = pow(Lnew, m.llodku0);
            W_tmp = Wnew + wlod;
            T1 = pow(W_tmp, m.wlodku0);
            tmp1 = m.lku0 / T0 + m.wku0 / T1 + m.pku0 / (T0 * T1);
            p.ku0 = 1.0 + tmp1;
            T0 = pow(Lnew, m.llodvth);
            T1 = pow(W_tmp, m.wlodvth);
            tmp1 = m.lkvth0 / T0 + m.wkvth0 / T1 + m.pkvth0 / (T0 * T1);
            p.kvth0 = 1.0 + tmp1;
            p.kvth0 = sqrt(p.kvth0 * p.kvth0 + DELTA);
            T0 = (TRatio - 1.0);
            p.ku0temp = p.ku0 * (1.0 + m.tku0 * T0) + DELTA;
            Inv_saref = 1.0 / (m.saref + 0.5 * Ldrn);
            Inv_sbref = 1.0 / (m.sbref + 0.5 * Ldrn);
            p.inv_od_ref = Inv_saref + Inv_sbref;
            p.rho_ref = m.ku0 / p.ku0temp * p.inv_od_ref;
            if m.mobMod == 3 {
                lt1 = m.factor1 * p.sqrtXdep0;
                T0 = p.dvt1 * p.leff / lt1;
                if T0 < EXP_THRESHOLD {
                    T1 = exp(T0);
                    T2 = T1 - 1.0;
                    T3 = T2 * T2;
                    T4 = T3 + 2.0 * T1 * MIN_EXP;
                    Theta0 = T1 / T4;
                } else {
                    Theta0 = 1.0 / (MAX_EXP - 2.0);
                }
                tmp1 = epssub / p.Xdep0;
                tmp2 = p.nfactor * tmp1;
                tmp3 = (tmp2 + p.cdsc * Theta0 + p.cit) / m.coxe;
                if tmp3 >= -0.5 {
                    n0 = 1.0 + tmp3;
                } else {
                    T0 = 1.0 / (3.0 + 8.0 * tmp3);
                    n0 = (1.0 + 3.0 * tmp3) * T0;
                }
                T0 = n0 * m.vtm;
                T1 = p.voffcbn;
                T2 = T1 / T0;
                if T2 < -EXP_THRESHOLD {
                    T3 = m.coxe * MIN_EXP / p.cdep0;
                    T4 = p.mstar + T3 * n0;
                } else if T2 > EXP_THRESHOLD {
                    T3 = m.coxe * MAX_EXP / p.cdep0;
                    T4 = p.mstar + T3 * n0;
                } else {
                    T3 = exp(T2) * m.coxe / p.cdep0;
                    T4 = p.mstar + T3 * n0;
                }
                p.VgsteffVth = T0 * log(2.0) / T4;
            }
            T0 = -p.dvtp3 * log(p.leff);
            T1 = dexp(T0);
            p.dvtp2factor = p.dvtp5 + p.dvtp2 * T1;
        }
        if (h.sa > 0.0) && (h.sb > 0.0) && ((h.nf == 1.0) || ((h.nf > 1.0) && (h.sd > 0.0))) {
            Inv_sa = 0.0;
            Inv_sb = 0.0;
            kvsat = m.kvsat;
            if m.kvsat < -1.0 {
                warnings.push(format!(
                    "Warning: KVSAT = {} is too small; -1.0 is used.",
                    c_g(m.kvsat)
                ));
                kvsat = -1.0;
            }
            if m.kvsat > 1.0 {
                warnings.push(format!(
                    "Warning: KVSAT = {} is too big; 1.0 is used.",
                    c_g(m.kvsat)
                ));
                kvsat = 1.0;
            }
            i = 0;
            while f64::from(i) < h.nf {
                T0 = 1.0 / h.nf / (h.sa + 0.5 * Ldrn + f64::from(i) * (h.sd + Ldrn));
                T1 = 1.0 / h.nf / (h.sb + 0.5 * Ldrn + f64::from(i) * (h.sd + Ldrn));
                Inv_sa += T0;
                Inv_sb += T1;
                i += 1;
            }
            Inv_ODeff = Inv_sa + Inv_sb;
            rho = m.ku0 / p.ku0temp * Inv_ODeff;
            T0 = (1.0 + rho) / (1.0 + p.rho_ref);
            h.u0temp = p.u0temp * T0;
            T1 = (1.0 + kvsat * rho) / (1.0 + kvsat * p.rho_ref);
            h.vsattemp = p.vsattemp * T1;
            OD_offset = Inv_ODeff - p.inv_od_ref;
            dvth0_lod = m.kvth0 / p.kvth0 * OD_offset;
            dk2_lod = m.stk2 / pow(p.kvth0, m.lodk2) * OD_offset;
            deta0_lod = m.steta0 / pow(p.kvth0, m.lodeta0) * OD_offset;
            h.vth0 = p.vth0 + dvth0_lod;
            h.eta0 = p.eta0 + deta0_lod;
            h.k2 = p.k2 + dk2_lod;
        } else {
            h.u0temp = p.u0temp;
            h.vth0 = p.vth0;
            h.vsattemp = p.vsattemp;
            h.eta0 = p.eta0;
            h.k2 = p.k2;
        }
        if m.wpemod != 0.0 {
            if (!h.scaGiven) && (!h.scbGiven) && (!h.sccGiven) {
                if (h.scGiven) && (h.sc > 0.0) {
                    T1 = h.sc + Wdrn;
                    T2 = 1.0 / m.scref;
                    h.sca = m.scref * m.scref / (h.sc * T1);
                    h.scb = ((0.1 * h.sc + 0.01 * m.scref) * exp(-10.0 * h.sc * T2)
                        - (0.1 * T1 + 0.01 * m.scref) * exp(-10.0 * T1 * T2))
                        / Wdrn;
                    h.scc = ((0.05 * h.sc + 0.0025 * m.scref) * exp(-20.0 * h.sc * T2)
                        - (0.05 * T1 + 0.0025 * m.scref) * exp(-20.0 * T1 * T2))
                        / Wdrn;
                } else {
                    warnings.push("Warning: No WPE as none of SCA, SCB, SCC, SC is given and/or SC not positive.".to_string());
                }
            }
            if h.sca < 0.0 {
                warnings.push(format!(
                    "Warning: SCA = {} is negative. Set to 0.0.",
                    c_g(h.sca)
                ));
                h.sca = 0.0;
            }
            if h.scb < 0.0 {
                warnings.push(format!(
                    "Warning: SCB = {} is negative. Set to 0.0.",
                    c_g(h.scb)
                ));
                h.scb = 0.0;
            }
            if h.scc < 0.0 {
                warnings.push(format!(
                    "Warning: SCC = {} is negative. Set to 0.0.",
                    c_g(h.scc)
                ));
                h.scc = 0.0;
            }
            if h.sc < 0.0 {
                warnings.push(format!(
                    "Warning: SC = {} is negative. Set to 0.0.",
                    c_g(h.sc)
                ));
                h.sc = 0.0;
            }
            sceff = h.sca + m.web * h.scb + m.wec * h.scc;
            h.vth0 += p.kvth0we * sceff;
            h.k2 += p.k2we * sceff;
            T3 = 1.0 + p.ku0we * sceff;
            if T3 <= 0.0 {
                T3 = 0.0;
                warnings.push(format!(
                    "Warning: ku0we = {} is negatively too high. Negative mobility! ",
                    c_g(p.ku0we)
                ));
            }
            h.u0temp *= T3;
        }
        h.vth0 += h.delvto;
        h.vfb = p.vfb + f64::from(m.r#type) * h.delvto;
        h.u0temp = p.u0temp * h.mulu0;
        T3 = f64::from(m.r#type) * h.vth0 - h.vfb - p.phi;
        T4 = T3 + T3;
        T5 = 2.5 * T3;
        h.vtfbphi1 = if m.r#type == NMOS { T4 } else { T5 };
        if h.vtfbphi1 < 0.0 {
            h.vtfbphi1 = 0.0;
        }
        h.vtfbphi2 = 4.0 * T3;
        if h.vtfbphi2 < 0.0 {
            h.vtfbphi2 = 0.0;
        }
        if h.k2 < 0.0 {
            T0 = 0.5 * p.k1 / h.k2;
            h.vbsc = 0.9 * (p.phi - T0 * T0);
            if h.vbsc > -3.0 {
                h.vbsc = -3.0;
            } else if h.vbsc < -30.0 {
                h.vbsc = -30.0;
            }
        } else {
            h.vbsc = -30.0;
        }
        if h.vbsc > p.vbm {
            h.vbsc = p.vbm;
        }
        h.k2ox = h.k2 * toxe / m.toxm;
        h.vfbzb = p.vfbzbfactor + f64::from(m.r#type) * h.vth0;
        h.cgso = p.cgso;
        h.cgdo = p.cgdo;
        lnl = log(p.leff * 1.0e6);
        lnw = log(p.weff * 1.0e6);
        lnnf = log(h.nf);
        bodymode = 5.0;
        if (!m.rbps0Given) || (!m.rbpd0Given) {
            bodymode = 1.0;
        } else if (!m.rbsbx0Given && !m.rbsby0Given) || (!m.rbdbx0Given && !m.rbdby0Given) {
            bodymode = 3.0;
        }
        if h.rbodyMod == 2 {
            if bodymode == 5.0 {
                rbsbx = m.rbsbx0 * exp(m.rbsdbxl * lnl + m.rbsdbxw * lnw + m.rbsdbxnf * lnnf);
                rbsby = m.rbsby0 * exp(m.rbsdbyl * lnl + m.rbsdbyw * lnw + m.rbsdbynf * lnnf);
                h.rbsb = rbsbx * rbsby / (rbsbx + rbsby);
                rbdbx = m.rbdbx0 * exp(m.rbsdbxl * lnl + m.rbsdbxw * lnw + m.rbsdbxnf * lnnf);
                rbdby = m.rbdby0 * exp(m.rbsdbyl * lnl + m.rbsdbyw * lnw + m.rbsdbynf * lnnf);
                h.rbdb = rbdbx * rbdby / (rbdbx + rbdby);
            }
            if (bodymode == 3.0) || (bodymode == 5.0) {
                h.rbps = m.rbps0 * exp(m.rbpsl * lnl + m.rbpsw * lnw + m.rbpsnf * lnnf);
                h.rbpd = m.rbpd0 * exp(m.rbpdl * lnl + m.rbpdw * lnw + m.rbpdnf * lnnf);
            }
            rbpbx = m.rbpbx0 * exp(m.rbpbxl * lnl + m.rbpbxw * lnw + m.rbpbxnf * lnnf);
            rbpby = m.rbpby0 * exp(m.rbpbyl * lnl + m.rbpbyw * lnw + m.rbpbynf * lnnf);
            h.rbpb = rbpbx * rbpby / (rbpbx + rbpby);
        }
        if (h.rbodyMod == 1) || ((h.rbodyMod == 2) && (bodymode == 5.0)) {
            if h.rbdb < 1.0e-3 {
                h.grbdb = 1.0e3;
            } else {
                h.grbdb = m.gbmin + 1.0 / h.rbdb;
            }
            if h.rbpb < 1.0e-3 {
                h.grbpb = 1.0e3;
            } else {
                h.grbpb = m.gbmin + 1.0 / h.rbpb;
            }
            if h.rbps < 1.0e-3 {
                h.grbps = 1.0e3;
            } else {
                h.grbps = m.gbmin + 1.0 / h.rbps;
            }
            if h.rbsb < 1.0e-3 {
                h.grbsb = 1.0e3;
            } else {
                h.grbsb = m.gbmin + 1.0 / h.rbsb;
            }
            if h.rbpd < 1.0e-3 {
                h.grbpd = 1.0e3;
            } else {
                h.grbpd = m.gbmin + 1.0 / h.rbpd;
            }
        }
        if (h.rbodyMod == 2) && (bodymode == 3.0) {
            h.grbsb = m.gbmin;
            h.grbdb = h.grbsb;
            if h.rbpb < 1.0e-3 {
                h.grbpb = 1.0e3;
            } else {
                h.grbpb = m.gbmin + 1.0 / h.rbpb;
            }
            if h.rbps < 1.0e-3 {
                h.grbps = 1.0e3;
            } else {
                h.grbps = m.gbmin + 1.0 / h.rbps;
            }
            if h.rbpd < 1.0e-3 {
                h.grbpd = 1.0e3;
            } else {
                h.grbpd = m.gbmin + 1.0 / h.rbpd;
            }
        }
        if (h.rbodyMod == 2) && (bodymode == 1.0) {
            h.grbsb = m.gbmin;
            h.grbdb = h.grbsb;
            h.grbpd = 1.0e3;
            h.grbps = h.grbpd;
            if h.rbpb < 1.0e-3 {
                h.grbpb = 1.0e3;
            } else {
                h.grbpb = m.gbmin + 1.0 / h.rbpb;
            }
        }
        h.grgeltd = m.rshg * (h.xgw + p.weffCJ / 3.0 / h.ngcon) / (h.ngcon * h.nf * (Lnew - m.xgl));
        if h.grgeltd > 0.0 {
            h.grgeltd = 1.0 / h.grgeltd;
        } else {
            h.grgeltd = 1.0e3;
            if h.rgateMod != 0 {
                warnings.push("Warning: The gate conductance reset to 1.0e3 mho.".to_string());
            }
        }
        DMCGeff = m.dmcg - m.dmcgt;
        DMCIeff = m.dmci;
        DMDGeff = m.dmdg - m.dmcgt;
        if h.sourcePerimeterGiven {
            if h.sourcePerimeter == 0.0 {
                h.Pseff = 0.0;
            } else if h.sourcePerimeter < 0.0 {
                warnings.push(
                    "Warning: Source Perimeter is specified as negative, it is set to zero."
                        .to_string(),
                );
                h.Pseff = 0.0;
            } else {
                if m.perMod == 0 {
                    h.Pseff = h.sourcePerimeter;
                } else {
                    h.Pseff = h.sourcePerimeter - p.weffCJ * h.nf;
                }
            }
        } else {
            (h.Pseff, dumPd, dumAs, dumAd) = pa_eff_geo(
                h.nf,
                h.geoMod,
                h.min,
                p.weffCJ,
                DMCGeff,
                DMCIeff,
                DMDGeff,
                (h.Pseff, dumPd, dumAs, dumAd),
                warnings,
            );
        }
        if h.Pseff < 0.0 {
            h.Pseff = 0.0;
            warnings.push("Warning: Pseff is negative, it is set to zero.".to_string());
        }
        if h.drainPerimeterGiven {
            if h.drainPerimeter == 0.0 {
                h.Pdeff = 0.0;
            } else if h.drainPerimeter < 0.0 {
                warnings.push(
                    "Warning: Drain Perimeter is specified as negative, it is set to zero."
                        .to_string(),
                );
                h.Pdeff = 0.0;
            } else {
                if m.perMod == 0 {
                    h.Pdeff = h.drainPerimeter;
                } else {
                    h.Pdeff = h.drainPerimeter - p.weffCJ * h.nf;
                }
            }
        } else {
            (dumPs, h.Pdeff, dumAs, dumAd) = pa_eff_geo(
                h.nf,
                h.geoMod,
                h.min,
                p.weffCJ,
                DMCGeff,
                DMCIeff,
                DMDGeff,
                (dumPs, h.Pdeff, dumAs, dumAd),
                warnings,
            );
        }
        if h.Pdeff < 0.0 {
            h.Pdeff = 0.0;
            warnings.push("Warning: Pdeff is negative, it is set to zero.".to_string());
        }
        if h.sourceAreaGiven {
            h.Aseff = h.sourceArea;
        } else {
            (dumPs, dumPd, h.Aseff, dumAd) = pa_eff_geo(
                h.nf,
                h.geoMod,
                h.min,
                p.weffCJ,
                DMCGeff,
                DMCIeff,
                DMDGeff,
                (dumPs, dumPd, h.Aseff, dumAd),
                warnings,
            );
        }
        if h.Aseff < 0.0 {
            h.Aseff = 0.0;
            warnings.push("Warning: Aseff is negative, it is set to zero.".to_string());
        }
        if h.drainAreaGiven {
            h.Adeff = h.drainArea;
        } else {
            (dumPs, dumPd, dumAs, h.Adeff) = pa_eff_geo(
                h.nf,
                h.geoMod,
                h.min,
                p.weffCJ,
                DMCGeff,
                DMCIeff,
                DMDGeff,
                (dumPs, dumPd, dumAs, h.Adeff),
                warnings,
            );
        }
        if h.Adeff < 0.0 {
            h.Adeff = 0.0;
            warnings.push("Warning: Adeff is negative, it is set to zero.".to_string());
        }
        if h.sNodePrime != h.sNode {
            h.sourceConductance = 0.0;
            if h.sourceSquaresGiven {
                h.sourceConductance = m.sheetResistance * h.sourceSquares;
            } else if h.rgeoMod > 0 {
                h.sourceConductance = rdseff_geo(
                    h.nf,
                    h.geoMod,
                    h.rgeoMod,
                    h.min,
                    p.weffCJ,
                    m.sheetResistance,
                    DMCGeff,
                    DMCIeff,
                    DMDGeff,
                    1,
                    warnings,
                );
            } else {
                h.sourceConductance = 0.0;
            }
            if h.sourceConductance > 0.0 {
                h.sourceConductance = 1.0 / h.sourceConductance;
            } else {
                h.sourceConductance = 1.0e3;
                warnings.push("Warning: Source conductance reset to 1.0e3 mho.".to_string());
            }
        } else {
            h.sourceConductance = 0.0;
        }
        if h.dNodePrime != h.dNode {
            h.drainConductance = 0.0;
            if h.drainSquaresGiven {
                h.drainConductance = m.sheetResistance * h.drainSquares;
            } else if h.rgeoMod > 0 {
                h.drainConductance = rdseff_geo(
                    h.nf,
                    h.geoMod,
                    h.rgeoMod,
                    h.min,
                    p.weffCJ,
                    m.sheetResistance,
                    DMCGeff,
                    DMCIeff,
                    DMDGeff,
                    0,
                    warnings,
                );
            } else {
                h.drainConductance = 0.0;
            }
            if h.drainConductance > 0.0 {
                h.drainConductance = 1.0 / h.drainConductance;
            } else {
                h.drainConductance = 1.0e3;
                warnings.push("Warning: Drain conductance reset to 1.0e3 mho.".to_string());
            }
        } else {
            h.drainConductance = 0.0;
        }
        Nvtms = m.vtm * m.SjctEmissionCoeff;
        if (h.Aseff <= 0.0) && (h.Pseff <= 0.0) {
            SourceSatCurrent = 0.0;
        } else {
            SourceSatCurrent = h.Aseff * m.SjctTempSatCurDensity
                + h.Pseff * m.SjctSidewallTempSatCurDensity
                + p.weffCJ * h.nf * m.SjctGateSidewallTempSatCurDensity;
        }
        if SourceSatCurrent > 0.0 {
            match m.dioMod {
                0 => {
                    if (m.bvs / Nvtms) > EXP_THRESHOLD {
                        h.XExpBVS = m.xjbvs * MIN_EXP;
                    } else {
                        h.XExpBVS = m.xjbvs * exp(-m.bvs / Nvtms);
                    }
                }
                1 => {
                    h.vjsmFwd = dio_ijth_vjm_eval(Nvtms, m.ijthsfwd, SourceSatCurrent, 0.0);
                    h.IVjsmFwd = SourceSatCurrent * exp(h.vjsmFwd / Nvtms);
                }
                2 => {
                    if (m.bvs / Nvtms) > EXP_THRESHOLD {
                        h.XExpBVS = m.xjbvs * MIN_EXP;
                        tmp = MIN_EXP;
                    } else {
                        h.XExpBVS = exp(-m.bvs / Nvtms);
                        tmp = h.XExpBVS;
                        h.XExpBVS *= m.xjbvs;
                    }
                    h.vjsmFwd = dio_ijth_vjm_eval(Nvtms, m.ijthsfwd, SourceSatCurrent, h.XExpBVS);
                    T0 = exp(h.vjsmFwd / Nvtms);
                    h.IVjsmFwd = SourceSatCurrent * (T0 - h.XExpBVS / T0 + h.XExpBVS - 1.0);
                    h.SslpFwd = SourceSatCurrent * (T0 + h.XExpBVS / T0) / Nvtms;
                    T2 = m.ijthsrev / SourceSatCurrent;
                    if T2 < 1.0 {
                        T2 = 10.0;
                        warnings.push(
                            "Warning: ijthsrev too small and set to 10 times IsbSat.".to_string(),
                        );
                    }
                    h.vjsmRev = -m.bvs - Nvtms * log((T2 - 1.0) / m.xjbvs);
                    T1 = m.xjbvs * exp(-(m.bvs + h.vjsmRev) / Nvtms);
                    h.IVjsmRev = SourceSatCurrent * (1.0 + T1);
                    h.SslpRev = -SourceSatCurrent * T1 / Nvtms;
                }
                _ => {
                    warnings.push(format!("Specified dioMod = {} not matched", m.dioMod));
                }
            }
        }
        Nvtmd = m.vtm * m.DjctEmissionCoeff;
        if (h.Adeff <= 0.0) && (h.Pdeff <= 0.0) {
            DrainSatCurrent = 0.0;
        } else {
            DrainSatCurrent = h.Adeff * m.DjctTempSatCurDensity
                + h.Pdeff * m.DjctSidewallTempSatCurDensity
                + p.weffCJ * h.nf * m.DjctGateSidewallTempSatCurDensity;
        }
        if DrainSatCurrent > 0.0 {
            match m.dioMod {
                0 => {
                    if (m.bvd / Nvtmd) > EXP_THRESHOLD {
                        h.XExpBVD = m.xjbvd * MIN_EXP;
                    } else {
                        h.XExpBVD = m.xjbvd * exp(-m.bvd / Nvtmd);
                    }
                }
                1 => {
                    h.vjdmFwd = dio_ijth_vjm_eval(Nvtmd, m.ijthdfwd, DrainSatCurrent, 0.0);
                    h.IVjdmFwd = DrainSatCurrent * exp(h.vjdmFwd / Nvtmd);
                }
                2 => {
                    if (m.bvd / Nvtmd) > EXP_THRESHOLD {
                        h.XExpBVD = m.xjbvd * MIN_EXP;
                        tmp = MIN_EXP;
                    } else {
                        h.XExpBVD = exp(-m.bvd / Nvtmd);
                        tmp = h.XExpBVD;
                        h.XExpBVD *= m.xjbvd;
                    }
                    h.vjdmFwd = dio_ijth_vjm_eval(Nvtmd, m.ijthdfwd, DrainSatCurrent, h.XExpBVD);
                    T0 = exp(h.vjdmFwd / Nvtmd);
                    h.IVjdmFwd = DrainSatCurrent * (T0 - h.XExpBVD / T0 + h.XExpBVD - 1.0);
                    h.DslpFwd = DrainSatCurrent * (T0 + h.XExpBVD / T0) / Nvtmd;
                    T2 = m.ijthdrev / DrainSatCurrent;
                    if T2 < 1.0 {
                        T2 = 10.0;
                        warnings.push(
                            "Warning: ijthdrev too small and set to 10 times IdbSat.".to_string(),
                        );
                    }
                    h.vjdmRev = -m.bvd - Nvtmd * log((T2 - 1.0) / m.xjbvd);
                    T1 = m.xjbvd * exp(-(m.bvd + h.vjdmRev) / Nvtmd);
                    h.IVjdmRev = DrainSatCurrent * (1.0 + T1);
                    h.DslpRev = -DrainSatCurrent * T1 / Nvtmd;
                }
                _ => {
                    warnings.push(format!("Specified dioMod = {} not matched", m.dioMod));
                }
            }
        }
        T0 = (TRatio - 1.0);
        m.njtsstemp = m.njts * (1.0 + m.tnjts * T0);
        m.njtsswstemp = m.njtssw * (1.0 + m.tnjtssw * T0);
        m.njtsswgstemp = m.njtsswg * (1.0 + m.tnjtsswg * T0);
        m.njtsdtemp = m.njtsd * (1.0 + m.tnjtsd * T0);
        m.njtsswdtemp = m.njtsswd * (1.0 + m.tnjtsswd * T0);
        m.njtsswgdtemp = m.njtsswgd * (1.0 + m.tnjtsswgd * T0);
        T7 = Eg0 / m.vtm * T0;
        T9 = m.xtss * T7;
        T1 = dexp(T9);
        T9 = m.xtsd * T7;
        T2 = dexp(T9);
        T9 = m.xtssws * T7;
        T3 = dexp(T9);
        T9 = m.xtsswd * T7;
        T4 = dexp(T9);
        T9 = m.xtsswgs * T7;
        T5 = dexp(T9);
        T9 = m.xtsswgd * T7;
        T6 = dexp(T9);
        if m.jtweff < 0.0 {
            m.jtweff = 0.0;
            warnings.push(
                "TAT width dependence effect is negative. Jtweff is clamped to zero.".to_string(),
            );
        }
        T11 = sqrt(m.jtweff / p.weffCJ) + 1.0;
        T10 = p.weffCJ * h.nf;
        h.SjctTempRevSatCur = T1 * h.Aseff * m.jtss;
        h.DjctTempRevSatCur = T2 * h.Adeff * m.jtsd;
        h.SswTempRevSatCur = T3 * h.Pseff * m.jtssws;
        h.DswTempRevSatCur = T4 * h.Pdeff * m.jtsswd;
        h.SswgTempRevSatCur = T5 * T10 * T11 * m.jtsswgs;
        h.DswgTempRevSatCur = T6 * T10 * T11 * m.jtsswgd;
        if m.mtrlMod != 0 && m.mtrlCompatMod == 0 {
            Vtm0eot = KboQ * m.tempeot;
            Vtmeot = Vtm0eot;
            vbieot = Vtm0eot * log(p.nsd * p.ndep / (ni * ni));
            phieot = Vtm0eot * log(p.ndep / ni) + p.phin + 0.4;
            if phieot <= 0.0 {
                warnings.push(format!(
                    "Fatal: phieot = {} is not positive. Please Check Phin and Ndep",
                    c_g(p.phi)
                ));
                warnings.push(format!("phieot = {} Ndep = {} ", c_g(p.phin), c_g(p.ndep)));
                Fatal_Flag = 1;
            }
            tmp2 = h.vfb + phieot;
            vddeot = f64::from(m.r#type) * m.vddeot;
            T0 = m.epsrgate * EPS0;
            if (p.ngate > 1.0e18) && (p.ngate < 1.0e25) && (vddeot > tmp2) && (T0 != 0.0) {
                T1 = 1.0e6 * CHARGE * T0 * p.ngate / (m.coxe * m.coxe);
                T8 = vddeot - tmp2;
                T4 = sqrt(1.0 + 2.0 * T8 / T1);
                T2 = 2.0 * T8 / (T4 + 1.0);
                T3 = 0.5 * T2 * T2 / T1;
                T7 = 1.12 - T3 - 0.05;
                T6 = sqrt(T7 * T7 + 0.224);
                T5 = 1.12 - 0.5 * (T7 + T6);
                Vgs_eff = vddeot - T5;
            } else {
                Vgs_eff = vddeot;
            }
            V0 = vbieot - phieot;
            lt1 = m.factor1 * p.sqrtXdep0;
            ltw = lt1;
            T0 = p.dvt1 * m.leffeot / lt1;
            if T0 < EXP_THRESHOLD {
                T1 = exp(T0);
                T2 = T1 - 1.0;
                T3 = T2 * T2;
                T4 = T3 + 2.0 * T1 * MIN_EXP;
                Theta0 = T1 / T4;
            } else {
                Theta0 = 1.0 / (MAX_EXP - 2.0);
            }
            Delt_vth = p.dvt0 * Theta0 * V0;
            T0 = p.dvt1w * m.weffeot * m.leffeot / ltw;
            if T0 < EXP_THRESHOLD {
                T1 = exp(T0);
                T2 = T1 - 1.0;
                T3 = T2 * T2;
                T4 = T3 + 2.0 * T1 * MIN_EXP;
                T5 = T1 / T4;
            } else {
                T5 = 1.0 / (MAX_EXP - 2.0);
            }
            T2 = p.dvt0w * T5 * V0;
            TempRatioeot = m.tempeot / m.tnom - 1.0;
            T0 = sqrt(1.0 + p.lpe0 / m.leffeot);
            T1 = p.k1ox * (T0 - 1.0) * sqrt(phieot) + (p.kt1 + p.kt1l / m.leffeot) * TempRatioeot;
            Vth_NarrowW = toxe * phieot / (m.weffeot + p.w0);
            Lpe_Vb = sqrt(1.0 + p.lpeb / m.leffeot);
            Vth = f64::from(m.r#type) * h.vth0 + (p.k1ox - p.k1) * sqrt(phieot) * Lpe_Vb
                - Delt_vth
                - T2
                + p.k3 * Vth_NarrowW
                + T1;
            tmp1 = epssub / p.Xdep0;
            tmp2 = p.nfactor * tmp1;
            tmp3 = (tmp2 + p.cdsc * Theta0 + p.cit) / m.coxe;
            if tmp3 >= -0.5 {
                n = 1.0 + tmp3;
            } else {
                T0 = 1.0 / (3.0 + 8.0 * tmp3);
                n = (1.0 + 3.0 * tmp3) * T0;
            }
            if p.dvtp0 > 0.0 {
                T3 = m.leffeot + p.dvtp0 * 2.0;
                if m.tempMod < 2 {
                    T4 = Vtmeot * log(m.leffeot / T3);
                } else {
                    T4 = Vtm0eot * log(m.leffeot / T3);
                }
                Vth -= n * T4;
            }
            Vgsteff = Vgs_eff - Vth;
            T3 = f64::from(m.r#type) * h.vth0 - h.vfb - phieot;
            T4 = T3 + T3;
            T5 = 2.5 * T3;
            vtfbphi2eot = 4.0 * T3;
            if vtfbphi2eot < 0.0 {
                vtfbphi2eot = 0.0;
            }
            niter = 0.0;
            toxpf = toxe;
            loop {
                toxpi = toxpf;
                tmp2 = 2.0e8 * toxpf;
                T0 = (Vgsteff + vtfbphi2eot) / tmp2;
                T1 = 1.0 + exp(m.bdos * 0.7 * log(T0));
                Tcen = m.ados * 1.9e-9 / T1;
                toxpf = toxe - epsrox / m.epsrsub * Tcen;
                niter += 1.0;
                if !((niter <= 4.0) && (ABS(toxpf - toxpi) > 1e-12)) {
                    break;
                }
            }
            h.toxp = toxpf;
            h.coxp = epsrox * EPS0 / h.toxp;
        } else {
            h.toxp = m.toxp;
            h.coxp = m.coxp;
        }
        let mut messages = Vec::new();
        if check && check_model(m, h, &mut p, Temp, model_name, &mut messages) {
            messages.push(format!(
                    "detected during BSIM4.8.3 parameter checking for \n    model {model_name} of device instance {instance_name}\n"
                ));
            return Err(bad_parameter(messages.join("\n")));
        }
        warnings.append(&mut messages);
    }
    if cached.is_none() {
        cache.last_length = Lnew;
        cache.sizes.push(p.clone());
    } else if let Some(slot) = cache
        .sizes
        .iter_mut()
        .find(|q| q.Length == p.Length && q.Width == p.Width && q.NFinger == p.NFinger)
    {
        *slot = p.clone();
    }
    Ok(p)
}

/// [`temperature_model`] then [`temperature_instance`] for a single
/// instance whose `BSIM4checkModel` C runs.
///
/// # Errors
/// As [`temperature_instance`].
pub(crate) fn temperature(
    m: &mut Model,
    h: &mut Instance,
    temperature: f64,
    model_name: &str,
    instance_name: &str,
    warnings: &mut Vec<String>,
) -> SpiceResult<SizeParam> {
    let locals = temperature_model(m, temperature, warnings);
    temperature_instance(m, h, &locals, true, model_name, instance_name, warnings)
}

/// `(model members, instance asks, messages)` of
/// [`super::ModelParameters::temperature_values`].
type TemperatureValues = (
    Vec<(&'static str, f64)>,
    Vec<(&'static str, f64)>,
    Vec<String>,
);

impl super::ModelParameters {
    /// A verification hook for the temperature stage: one instance set up
    /// by `b4set.c` and processed by `b4temp.c` (with its `b4check.c`
    /// check) at `temperature_celsius`, nominal `nominal_celsius`. Returns
    /// every numeric model member C's `@model[keyword]` ask reads (after
    /// `b4temp.c`), the instance asks `b4ask.c` answers from values
    /// `b4temp.c` leaves and `b4ld.c` does not overwrite (`weff`, `leff`,
    /// `cgbo`, the body resistances, the WPE distances), and the messages
    /// in C order. Not a simulation result.
    ///
    /// # Errors
    /// Invalid input or temperatures, or a fatal setup/temperature check.
    pub fn temperature_values(
        &self,
        instance: &crate::netlist::ast::DeviceInstance,
        temperature_celsius: f64,
        nominal_celsius: f64,
    ) -> SpiceResult<TemperatureValues> {
        use super::host::node;
        use crate::devices::schema::temperature_kelvin;
        let tnom = temperature_kelvin(nominal_celsius, &self.location)?;
        let temp = temperature_kelvin(temperature_celsius, &instance.location)?;
        let mut warnings = Vec::new();
        let mut m = super::apply::model(self)?;
        super::set::setup_model(&mut m, tnom, &mut warnings)?;
        let mut h = super::apply::instance(&self.instance_parameters(instance, 1.)?)?;
        (h.dNode, h.gNodeExt, h.sNode, h.bNode) = (node::D, node::G_EXT, node::S, node::B);
        super::set::setup_instance(&m, &mut h, false, &mut warnings)?;
        let model_name = instance.model.as_deref().unwrap_or_default();
        let p = temperature(
            &mut m,
            &mut h,
            temp,
            model_name,
            &instance.name,
            &mut warnings,
        )?;
        let mut model = Vec::new();
        for parameter in super::parameters::MODEL.iter().filter(|p| !p.redundant) {
            if !matches!(
                parameter.input_type,
                super::InputType::Real | super::InputType::Integer
            ) {
                continue;
            }
            let Some(field) = parameter.c_field else {
                continue;
            };
            let value = if let Some(v) = m.real_mut(field) {
                *v
            } else if let Some(v) = m.int_mut(field) {
                f64::from(*v)
            } else {
                return Err(SpiceError::circuit(format!(
                    "BSIM4 model member {field} is missing"
                )));
            };
            model.push((parameter.name, value));
        }
        let asks = vec![
            ("weff", p.weff),
            ("leff", p.leff),
            ("cgbo", p.cgbo * h.mult_q * h.m),
            ("rbdb", h.rbdb),
            ("rbsb", h.rbsb),
            ("rbpb", h.rbpb),
            ("rbps", h.rbps),
            ("rbpd", h.rbpd),
            ("sca", h.sca),
            ("scb", h.scb),
            ("scc", h.scc),
            ("sc", h.sc),
        ];
        Ok((model, asks, warnings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ijth_vjm_inverts_the_limited_diode_current() {
        // With XExpBV = 0 the limit is where Isb * (exp(V/Nvtm) - 1) = Ijth.
        let v = dio_ijth_vjm_eval(0.025, 0.1, 1e-14, 0.0);
        let current = 1e-14 * ((v / 0.025).exp() - 1.0);
        assert!((current - 0.1).abs() < 1e-12, "{current}");
    }

    #[test]
    fn c_g_spells_like_printf() {
        for (value, text) in [
            (0.0, "0"),
            (0.1, "0.1"),
            (-1e-9, "-1e-09"),
            (123456.0, "123456"),
            (1234567.0, "1.23457e+06"),
            (1e-4, "0.0001"),
            (-2.64404e10, "-2.64404e+10"),
        ] {
            assert_eq!(c_g(value), text);
        }
    }

    #[test]
    fn dexp_follows_the_c_macro_branches() {
        assert_eq!(dexp(35.0), MAX_EXP * 2.0);
        assert_eq!(dexp(-35.0), MIN_EXP);
        assert_eq!(dexp(1.0), 1f64.exp());
    }
}
