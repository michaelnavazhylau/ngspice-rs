//! `b3temp.c` (`BSIM3temp`): the model's temperature pass, then each
//! instance's size-dependent parameters (`b3check.c` runs on every new
//! geometry) and temperature-dependent instance values.
//!
//! The pass follows C statement by statement over C's own members and local
//! names. C keeps the size-dependent parameter sets as a linked list of
//! knots on the model, shared by every instance of the same `(l, w)`, and its
//! instance loop reads function locals that an earlier iteration left: an
//! instance reusing an existing knot computes `tconst` with the `T0` the last
//! iteration assigned. [`SizeCache`] carries both between calls on one model.
#![allow(non_snake_case)] // C's member and local names.
#![allow(unused_assignments, unused_mut)] // C assigns locals that some paths never read.
#![allow(non_upper_case_globals)] // C's `Charge_q`.
#![allow(clippy::excessive_precision)] // C's literals.
#![allow(clippy::float_cmp)] // C compares floating point exactly.
#![allow(clippy::nonminimal_bool)] // C's conditions.
#![allow(clippy::double_parens)] // C's parenthesisation is kept verbatim.
#![allow(unused_parens)] // C's parenthesisation is kept verbatim.
#![allow(clippy::too_many_lines)] // One C function.
#![allow(clippy::assign_op_pattern)] // `x = x * y` is kept as C writes it.
#![allow(clippy::manual_clamp)] // C's two-sided `if` limits (`clamp` differs on NaN).

use super::check;
use super::fields::{Instance, Model, SizeParam};
use crate::primitives::{SpiceError, SpiceResult};

const KboQ: f64 = 8.617087e-5;
const EPSOX: f64 = 3.453133e-11;
const EPSSI: f64 = 1.03594e-10;
const MIN_EXP: f64 = 1.713908431e-15;
const EXP_THRESHOLD: f64 = 34.0;
const Charge_q: f64 = 1.60219e-19;
/// `CONSTvt0 = CONSTboltz * REFTEMP / CHARGE` (`main.c`), REFTEMP = 27 C.
const CONSTvt0: f64 = 1.38064852e-23 * (27.0 + 273.15) / 1.6021766208e-19;
/// `CONSTroot2 = CONSTsqrt2` (`main.c`).
const CONSTroot2: f64 = std::f64::consts::SQRT_2;

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

/// C's `E_BADPARM` with its `IFerrorf(ERR_FATAL, ...)` message.
fn bad_parameter(message: String) -> SpiceError {
    SpiceError::circuit(message)
}

/// The locals `BSIM3temp`'s model pass computes and its instance loop reads.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ModelTemp {
    /// C local `TRatio`.
    pub(crate) TRatio: f64,
    /// C local `Vtm0`.
    pub(crate) Vtm0: f64,
    /// C local `ni`.
    pub(crate) ni: f64,
}

/// The model's size-dependent parameter knots and the `BSIM3temp` function
/// local the instance loop carries across iterations (`T0`).
#[derive(Clone, Default)]
pub(crate) struct SizeCache {
    knots: Vec<SizeParam>,
    T0: f64,
}

impl SizeCache {
    /// A fresh pass: C frees the model's knots at the start of every
    /// `BSIM3temp` call.
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// `BSIM3temp`'s model pass at `CKTtemp` = `temperature` (K). C's
/// `fprintf` warnings are appended to `warnings`.
pub(crate) fn temperature_model(
    model: &mut Model,
    temperature: f64,
    warnings: &mut Vec<String>,
) -> ModelTemp {
    let mut Temp: f64 = temperature;
    let (mut Tnom, mut TRatio, mut Vtm0, mut Eg0, mut ni, mut Eg) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut T0, mut T1, mut delTemp) = (0.0_f64, 0.0_f64, 0.0_f64);
    Temp = temperature;
    if model.bulkJctPotential < 0.1 {
        model.bulkJctPotential = 0.1;
        warnings.push("Given pb is less than 0.1. Pb is set to 0.1.".to_string());
    }
    if model.sidewallJctPotential < 0.1 {
        model.sidewallJctPotential = 0.1;
        warnings.push("Given pbsw is less than 0.1. Pbsw is set to 0.1.".to_string());
    }
    if model.GatesidewallJctPotential < 0.1 {
        model.GatesidewallJctPotential = 0.1;
        warnings.push("Given pbswg is less than 0.1. Pbswg is set to 0.1.".to_string());
    }
    Tnom = model.tnom;
    TRatio = Temp / Tnom;
    model.vcrit = CONSTvt0 * log(CONSTvt0 / (CONSTroot2 * 1.0e-14));
    model.factor1 = sqrt(EPSSI / EPSOX * model.tox);
    Vtm0 = KboQ * Tnom;
    Eg0 = 1.16 - 7.02e-4 * Tnom * Tnom / (Tnom + 1108.0);
    ni = 1.45e10 * (Tnom / 300.15) * sqrt(Tnom / 300.15) * exp(21.5565981 - Eg0 / (2.0 * Vtm0));
    model.vtm = KboQ * Temp;
    Eg = 1.16 - 7.02e-4 * Temp * Temp / (Temp + 1108.0);
    if Temp != Tnom {
        T0 = Eg0 / Vtm0 - Eg / model.vtm + model.jctTempExponent * log(Temp / Tnom);
        T1 = exp(T0 / model.jctEmissionCoeff);
        model.jctTempSatCurDensity = model.jctSatCurDensity * T1;
        model.jctSidewallTempSatCurDensity = model.jctSidewallSatCurDensity * T1;
    } else {
        model.jctTempSatCurDensity = model.jctSatCurDensity;
        model.jctSidewallTempSatCurDensity = model.jctSidewallSatCurDensity;
    }
    if model.jctTempSatCurDensity < 0.0 {
        model.jctTempSatCurDensity = 0.0;
    }
    if model.jctSidewallTempSatCurDensity < 0.0 {
        model.jctSidewallTempSatCurDensity = 0.0;
    }
    delTemp = temperature - model.tnom;
    T0 = model.tcj * delTemp;
    if T0 >= -1.0 {
        model.unitAreaTempJctCap = model.unitAreaJctCap * (1.0 + T0);
    } else if model.unitAreaJctCap > 0.0 {
        model.unitAreaTempJctCap = 0.0;
        warnings.push(
            "Temperature effect has caused cj to be negative. Cj is clamped to zero.".to_string(),
        );
    }
    T0 = model.tcjsw * delTemp;
    if T0 >= -1.0 {
        model.unitLengthSidewallTempJctCap = model.unitLengthSidewallJctCap * (1.0 + T0);
    } else if model.unitLengthSidewallJctCap > 0.0 {
        model.unitLengthSidewallTempJctCap = 0.0;
        warnings.push(
            "Temperature effect has caused cjsw to be negative. Cjsw is clamped to zero."
                .to_string(),
        );
    }
    T0 = model.tcjswg * delTemp;
    if T0 >= -1.0 {
        model.unitLengthGateSidewallTempJctCap = model.unitLengthGateSidewallJctCap * (1.0 + T0);
    } else if model.unitLengthGateSidewallJctCap > 0.0 {
        model.unitLengthGateSidewallTempJctCap = 0.0;
        warnings.push(
            "Temperature effect has caused cjswg to be negative. Cjswg is clamped to zero."
                .to_string(),
        );
    }
    model.PhiB = model.bulkJctPotential - model.tpb * delTemp;
    if model.PhiB < 0.01 {
        model.PhiB = 0.01;
        warnings.push(
            "Temperature effect has caused pb to be less than 0.01. Pb is clamped to 0.01."
                .to_string(),
        );
    }
    model.PhiBSW = model.sidewallJctPotential - model.tpbsw * delTemp;
    if model.PhiBSW <= 0.01 {
        model.PhiBSW = 0.01;
        warnings.push(
            "Temperature effect has caused pbsw to be less than 0.01. Pbsw is clamped to 0.01."
                .to_string(),
        );
    }
    model.PhiBSWG = model.GatesidewallJctPotential - model.tpbswg * delTemp;
    if model.PhiBSWG <= 0.01 {
        model.PhiBSWG = 0.01;
        warnings.push(
            "Temperature effect has caused pbswg to be less than 0.01. Pbswg is clamped to 0.01."
                .to_string(),
        );
    }
    ModelTemp { TRatio, Vtm0, ni }
}

/// `BSIM3temp`'s instance loop body for `here`: its size-dependent
/// parameters (from `cache`'s knot for `(l, w)`, else computed and checked by
/// `b3check.c` and added to `cache`), then its temperature-dependent values.
/// `sensitivity` is C's `CKTcurJob->JOBtype >= 9` (a `.sens` job), which
/// silences the k1/k2 warnings. `model_name`/`instance_name` spell C's
/// messages.
///
/// # Errors
/// Nonpositive effective sizes and fatal `b3check.c` results (`E_BADPARM`,
/// with C's messages), and `acmMod > 0`.
#[allow(clippy::too_many_arguments)] // C's loop state.
pub(crate) fn temperature_instance(
    model: &mut Model,
    here: &mut Instance,
    locals: &ModelTemp,
    cache: &mut SizeCache,
    sensitivity: bool,
    model_name: &str,
    instance_name: &str,
    warnings: &mut Vec<String>,
) -> SpiceResult<SizeParam> {
    let ModelTemp { TRatio, Vtm0, ni } = *locals;
    let mut T0: f64 = cache.T0;
    let (mut tmp, mut tmp1, mut tmp2, mut tmp3) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut T1, mut T2, mut T3, mut T4, mut T5) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut Ldrn, mut Wdrn, mut Inv_L, mut Inv_W, mut Inv_LW) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut Nvtm, mut SourceSatCurrent, mut DrainSatCurrent) = (0.0_f64, 0.0_f64, 0.0_f64);
    let found = cache
        .knots
        .iter()
        .find(|knot| (here.l == knot.Length) && (here.w == knot.Width))
        .cloned();
    let pParam = if let Some(knot) = found {
        knot
    } else {
        let mut pParam = SizeParam::default();
        Ldrn = here.l;
        Wdrn = here.w;
        pParam.Length = Ldrn;
        pParam.Width = Wdrn;
        T0 = pow(Ldrn, model.Lln);
        T1 = pow(Wdrn, model.Lwn);
        tmp1 = model.Ll / T0 + model.Lw / T1 + model.Lwl / (T0 * T1);
        pParam.dl = model.Lint + tmp1;
        tmp2 = model.Llc / T0 + model.Lwc / T1 + model.Lwlc / (T0 * T1);
        pParam.dlc = model.dlc + tmp2;
        T2 = pow(Ldrn, model.Wln);
        T3 = pow(Wdrn, model.Wwn);
        tmp1 = model.Wl / T2 + model.Ww / T3 + model.Wwl / (T2 * T3);
        pParam.dw = model.Wint + tmp1;
        tmp2 = model.Wlc / T2 + model.Wwc / T3 + model.Wwlc / (T2 * T3);
        pParam.dwc = model.dwc + tmp2;
        pParam.leff = here.l + model.xl - 2.0 * pParam.dl;
        if pParam.leff <= 0.0 {
            return Err(bad_parameter(format!(
                "BSIM3: mosfet {}, model {}: Effective channel length <= 0",
                model_name, instance_name
            )));
        }
        pParam.weff = here.w + model.xw - 2.0 * pParam.dw;
        if pParam.weff <= 0.0 {
            return Err(bad_parameter(format!(
                "BSIM3: mosfet {}, model {}: Effective channel width <= 0",
                model_name, instance_name
            )));
        }
        pParam.leffCV = here.l + model.xl - 2.0 * pParam.dlc;
        if pParam.leffCV <= 0.0 {
            return Err(bad_parameter(format!(
                "BSIM3: mosfet {}, model {}: Effective channel length for C-V <= 0",
                model_name, instance_name
            )));
        }
        pParam.weffCV = here.w + model.xw - 2.0 * pParam.dwc;
        if pParam.weffCV <= 0.0 {
            return Err(bad_parameter(format!(
                "BSIM3: mosfet {}, model {}: Effective channel width for C-V <= 0",
                model_name, instance_name
            )));
        }
        if model.binUnit == 1 {
            Inv_L = 1.0e-6 / pParam.leff;
            Inv_W = 1.0e-6 / pParam.weff;
            Inv_LW = 1.0e-12 / (pParam.leff * pParam.weff);
        } else {
            Inv_L = 1.0 / pParam.leff;
            Inv_W = 1.0 / pParam.weff;
            Inv_LW = 1.0 / (pParam.leff * pParam.weff);
        }
        pParam.cdsc = model.cdsc + model.lcdsc * Inv_L + model.wcdsc * Inv_W + model.pcdsc * Inv_LW;
        pParam.cdscb =
            model.cdscb + model.lcdscb * Inv_L + model.wcdscb * Inv_W + model.pcdscb * Inv_LW;
        pParam.cdscd =
            model.cdscd + model.lcdscd * Inv_L + model.wcdscd * Inv_W + model.pcdscd * Inv_LW;
        pParam.cit = model.cit + model.lcit * Inv_L + model.wcit * Inv_W + model.pcit * Inv_LW;
        pParam.nfactor = model.nfactor
            + model.lnfactor * Inv_L
            + model.wnfactor * Inv_W
            + model.pnfactor * Inv_LW;
        pParam.xj = model.xj + model.lxj * Inv_L + model.wxj * Inv_W + model.pxj * Inv_LW;
        pParam.vsat = model.vsat + model.lvsat * Inv_L + model.wvsat * Inv_W + model.pvsat * Inv_LW;
        pParam.at = model.at + model.lat * Inv_L + model.wat * Inv_W + model.pat * Inv_LW;
        pParam.a0 = model.a0 + model.la0 * Inv_L + model.wa0 * Inv_W + model.pa0 * Inv_LW;
        pParam.ags = model.ags + model.lags * Inv_L + model.wags * Inv_W + model.pags * Inv_LW;
        pParam.a1 = model.a1 + model.la1 * Inv_L + model.wa1 * Inv_W + model.pa1 * Inv_LW;
        pParam.a2 = model.a2 + model.la2 * Inv_L + model.wa2 * Inv_W + model.pa2 * Inv_LW;
        pParam.keta = model.keta + model.lketa * Inv_L + model.wketa * Inv_W + model.pketa * Inv_LW;
        pParam.nsub = model.nsub + model.lnsub * Inv_L + model.wnsub * Inv_W + model.pnsub * Inv_LW;
        pParam.npeak =
            model.npeak + model.lnpeak * Inv_L + model.wnpeak * Inv_W + model.pnpeak * Inv_LW;
        pParam.ngate =
            model.ngate + model.lngate * Inv_L + model.wngate * Inv_W + model.pngate * Inv_LW;
        pParam.gamma1 =
            model.gamma1 + model.lgamma1 * Inv_L + model.wgamma1 * Inv_W + model.pgamma1 * Inv_LW;
        pParam.gamma2 =
            model.gamma2 + model.lgamma2 * Inv_L + model.wgamma2 * Inv_W + model.pgamma2 * Inv_LW;
        pParam.vbx = model.vbx + model.lvbx * Inv_L + model.wvbx * Inv_W + model.pvbx * Inv_LW;
        pParam.vbm = model.vbm + model.lvbm * Inv_L + model.wvbm * Inv_W + model.pvbm * Inv_LW;
        pParam.xt = model.xt + model.lxt * Inv_L + model.wxt * Inv_W + model.pxt * Inv_LW;
        pParam.vfb = model.vfb + model.lvfb * Inv_L + model.wvfb * Inv_W + model.pvfb * Inv_LW;
        pParam.k1 = model.k1 + model.lk1 * Inv_L + model.wk1 * Inv_W + model.pk1 * Inv_LW;
        pParam.kt1 = model.kt1 + model.lkt1 * Inv_L + model.wkt1 * Inv_W + model.pkt1 * Inv_LW;
        pParam.kt1l = model.kt1l + model.lkt1l * Inv_L + model.wkt1l * Inv_W + model.pkt1l * Inv_LW;
        pParam.k2 = model.k2 + model.lk2 * Inv_L + model.wk2 * Inv_W + model.pk2 * Inv_LW;
        pParam.kt2 = model.kt2 + model.lkt2 * Inv_L + model.wkt2 * Inv_W + model.pkt2 * Inv_LW;
        pParam.k3 = model.k3 + model.lk3 * Inv_L + model.wk3 * Inv_W + model.pk3 * Inv_LW;
        pParam.k3b = model.k3b + model.lk3b * Inv_L + model.wk3b * Inv_W + model.pk3b * Inv_LW;
        pParam.w0 = model.w0 + model.lw0 * Inv_L + model.ww0 * Inv_W + model.pw0 * Inv_LW;
        pParam.nlx = model.nlx + model.lnlx * Inv_L + model.wnlx * Inv_W + model.pnlx * Inv_LW;
        pParam.dvt0 = model.dvt0 + model.ldvt0 * Inv_L + model.wdvt0 * Inv_W + model.pdvt0 * Inv_LW;
        pParam.dvt1 = model.dvt1 + model.ldvt1 * Inv_L + model.wdvt1 * Inv_W + model.pdvt1 * Inv_LW;
        pParam.dvt2 = model.dvt2 + model.ldvt2 * Inv_L + model.wdvt2 * Inv_W + model.pdvt2 * Inv_LW;
        pParam.dvt0w =
            model.dvt0w + model.ldvt0w * Inv_L + model.wdvt0w * Inv_W + model.pdvt0w * Inv_LW;
        pParam.dvt1w =
            model.dvt1w + model.ldvt1w * Inv_L + model.wdvt1w * Inv_W + model.pdvt1w * Inv_LW;
        pParam.dvt2w =
            model.dvt2w + model.ldvt2w * Inv_L + model.wdvt2w * Inv_W + model.pdvt2w * Inv_LW;
        pParam.drout =
            model.drout + model.ldrout * Inv_L + model.wdrout * Inv_W + model.pdrout * Inv_LW;
        pParam.dsub = model.dsub + model.ldsub * Inv_L + model.wdsub * Inv_W + model.pdsub * Inv_LW;
        pParam.vth0 = model.vth0 + model.lvth0 * Inv_L + model.wvth0 * Inv_W + model.pvth0 * Inv_LW;
        pParam.ua = model.ua + model.lua * Inv_L + model.wua * Inv_W + model.pua * Inv_LW;
        pParam.ua1 = model.ua1 + model.lua1 * Inv_L + model.wua1 * Inv_W + model.pua1 * Inv_LW;
        pParam.ub = model.ub + model.lub * Inv_L + model.wub * Inv_W + model.r#pub * Inv_LW;
        pParam.ub1 = model.ub1 + model.lub1 * Inv_L + model.wub1 * Inv_W + model.pub1 * Inv_LW;
        pParam.uc = model.uc + model.luc * Inv_L + model.wuc * Inv_W + model.puc * Inv_LW;
        pParam.uc1 = model.uc1 + model.luc1 * Inv_L + model.wuc1 * Inv_W + model.puc1 * Inv_LW;
        pParam.u0 = model.u0 + model.lu0 * Inv_L + model.wu0 * Inv_W + model.pu0 * Inv_LW;
        pParam.ute = model.ute + model.lute * Inv_L + model.wute * Inv_W + model.pute * Inv_LW;
        pParam.voff = model.voff + model.lvoff * Inv_L + model.wvoff * Inv_W + model.pvoff * Inv_LW;
        pParam.delta =
            model.delta + model.ldelta * Inv_L + model.wdelta * Inv_W + model.pdelta * Inv_LW;
        pParam.rdsw = model.rdsw + model.lrdsw * Inv_L + model.wrdsw * Inv_W + model.prdsw * Inv_LW;
        pParam.prwg = model.prwg + model.lprwg * Inv_L + model.wprwg * Inv_W + model.pprwg * Inv_LW;
        pParam.prwb = model.prwb + model.lprwb * Inv_L + model.wprwb * Inv_W + model.pprwb * Inv_LW;
        pParam.prt = model.prt + model.lprt * Inv_L + model.wprt * Inv_W + model.pprt * Inv_LW;
        pParam.eta0 = model.eta0 + model.leta0 * Inv_L + model.weta0 * Inv_W + model.peta0 * Inv_LW;
        pParam.etab = model.etab + model.letab * Inv_L + model.wetab * Inv_W + model.petab * Inv_LW;
        pParam.pclm = model.pclm + model.lpclm * Inv_L + model.wpclm * Inv_W + model.ppclm * Inv_LW;
        pParam.pdibl1 =
            model.pdibl1 + model.lpdibl1 * Inv_L + model.wpdibl1 * Inv_W + model.ppdibl1 * Inv_LW;
        pParam.pdibl2 =
            model.pdibl2 + model.lpdibl2 * Inv_L + model.wpdibl2 * Inv_W + model.ppdibl2 * Inv_LW;
        pParam.pdiblb =
            model.pdiblb + model.lpdiblb * Inv_L + model.wpdiblb * Inv_W + model.ppdiblb * Inv_LW;
        pParam.pscbe1 =
            model.pscbe1 + model.lpscbe1 * Inv_L + model.wpscbe1 * Inv_W + model.ppscbe1 * Inv_LW;
        pParam.pscbe2 =
            model.pscbe2 + model.lpscbe2 * Inv_L + model.wpscbe2 * Inv_W + model.ppscbe2 * Inv_LW;
        pParam.pvag = model.pvag + model.lpvag * Inv_L + model.wpvag * Inv_W + model.ppvag * Inv_LW;
        pParam.wr = model.wr + model.lwr * Inv_L + model.wwr * Inv_W + model.pwr * Inv_LW;
        pParam.dwg = model.dwg + model.ldwg * Inv_L + model.wdwg * Inv_W + model.pdwg * Inv_LW;
        pParam.dwb = model.dwb + model.ldwb * Inv_L + model.wdwb * Inv_W + model.pdwb * Inv_LW;
        pParam.b0 = model.b0 + model.lb0 * Inv_L + model.wb0 * Inv_W + model.pb0 * Inv_LW;
        pParam.b1 = model.b1 + model.lb1 * Inv_L + model.wb1 * Inv_W + model.pb1 * Inv_LW;
        pParam.alpha0 =
            model.alpha0 + model.lalpha0 * Inv_L + model.walpha0 * Inv_W + model.palpha0 * Inv_LW;
        pParam.alpha1 =
            model.alpha1 + model.lalpha1 * Inv_L + model.walpha1 * Inv_W + model.palpha1 * Inv_LW;
        pParam.beta0 =
            model.beta0 + model.lbeta0 * Inv_L + model.wbeta0 * Inv_W + model.pbeta0 * Inv_LW;
        pParam.elm = model.elm + model.lelm * Inv_L + model.welm * Inv_W + model.pelm * Inv_LW;
        pParam.cgsl = model.cgsl + model.lcgsl * Inv_L + model.wcgsl * Inv_W + model.pcgsl * Inv_LW;
        pParam.cgdl = model.cgdl + model.lcgdl * Inv_L + model.wcgdl * Inv_W + model.pcgdl * Inv_LW;
        pParam.ckappa =
            model.ckappa + model.lckappa * Inv_L + model.wckappa * Inv_W + model.pckappa * Inv_LW;
        pParam.cf = model.cf + model.lcf * Inv_L + model.wcf * Inv_W + model.pcf * Inv_LW;
        pParam.clc = model.clc + model.lclc * Inv_L + model.wclc * Inv_W + model.pclc * Inv_LW;
        pParam.cle = model.cle + model.lcle * Inv_L + model.wcle * Inv_W + model.pcle * Inv_LW;
        pParam.vfbcv =
            model.vfbcv + model.lvfbcv * Inv_L + model.wvfbcv * Inv_W + model.pvfbcv * Inv_LW;
        pParam.acde = model.acde + model.lacde * Inv_L + model.wacde * Inv_W + model.pacde * Inv_LW;
        pParam.moin = model.moin + model.lmoin * Inv_L + model.wmoin * Inv_W + model.pmoin * Inv_LW;
        pParam.noff = model.noff + model.lnoff * Inv_L + model.wnoff * Inv_W + model.pnoff * Inv_LW;
        pParam.voffcv =
            model.voffcv + model.lvoffcv * Inv_L + model.wvoffcv * Inv_W + model.pvoffcv * Inv_LW;
        pParam.abulkCVfactor = 1.0 + pow((pParam.clc / pParam.leffCV), pParam.cle);
        T0 = (TRatio - 1.0);
        pParam.ua = pParam.ua + pParam.ua1 * T0;
        pParam.ub = pParam.ub + pParam.ub1 * T0;
        pParam.uc = pParam.uc + pParam.uc1 * T0;
        if pParam.u0 > 1.0 {
            pParam.u0 = pParam.u0 / 1.0e4;
        }
        pParam.u0temp = pParam.u0 * pow(TRatio, pParam.ute);
        pParam.vsattemp = pParam.vsat - pParam.at * T0;
        pParam.rds0 = (pParam.rdsw + pParam.prt * T0) / pow(pParam.weff * 1E6, pParam.wr);
        let checked = warnings.len();
        if check::check_model(model, here, &mut pParam, warnings) {
            // C prints the check list to stderr, then `IFerrorf`'s summary.
            let fatals: Vec<&str> = warnings[checked..]
                .iter()
                .map(String::as_str)
                .filter(|message| message.starts_with("Fatal:"))
                .collect();
            return Err(bad_parameter(format!(
                "Fatal error(s) detected during BSIM3V3.3 parameter checking: {}",
                fatals.join(" ")
            )));
        }
        pParam.cgdo = (model.cgdo + pParam.cf) * pParam.weffCV;
        pParam.cgso = (model.cgso + pParam.cf) * pParam.weffCV;
        pParam.cgbo = model.cgbo * pParam.leffCV;
        T0 = pParam.leffCV * pParam.leffCV;
        pParam.tconst =
            pParam.u0temp * pParam.elm / (model.cox * pParam.weffCV * pParam.leffCV * T0);
        if !model.npeakGiven && model.gamma1Given {
            T0 = pParam.gamma1 * model.cox;
            pParam.npeak = 3.021E22 * T0 * T0;
        }
        pParam.phi = 2.0 * Vtm0 * log(pParam.npeak / ni);
        pParam.sqrtPhi = sqrt(pParam.phi);
        pParam.phis3 = pParam.sqrtPhi * pParam.phi;
        pParam.Xdep0 = sqrt(2.0 * EPSSI / (Charge_q * pParam.npeak * 1.0e6)) * pParam.sqrtPhi;
        pParam.sqrtXdep0 = sqrt(pParam.Xdep0);
        pParam.litl = sqrt(3.0 * pParam.xj * model.tox);
        pParam.vbi = Vtm0 * log(1.0e20 * pParam.npeak / (ni * ni));
        pParam.cdep0 = sqrt(Charge_q * EPSSI * pParam.npeak * 1.0e6 / 2.0 / pParam.phi);
        pParam.ldeb = sqrt(EPSSI * Vtm0 / (Charge_q * pParam.npeak * 1.0e6)) / 3.0;
        pParam.acde *= pow((pParam.npeak / 2.0e16), -0.25);
        if model.k1Given || model.k2Given {
            if !model.k1Given {
                if !sensitivity {
                    warnings.push("Warning: k1 should be specified with k2.".to_string());
                }
                pParam.k1 = 0.53;
            }
            if !model.k2Given {
                if !sensitivity {
                    warnings.push("Warning: k2 should be specified with k1.".to_string());
                }
                pParam.k2 = -0.0186;
            }
            if !sensitivity {
                if model.nsubGiven {
                    warnings
                        .push("Warning: nsub is ignored because k1 or k2 is given.".to_string());
                }
                if model.xtGiven {
                    warnings.push("Warning: xt is ignored because k1 or k2 is given.".to_string());
                }
                if model.vbxGiven {
                    warnings.push("Warning: vbx is ignored because k1 or k2 is given.".to_string());
                }
                if model.gamma1Given {
                    warnings
                        .push("Warning: gamma1 is ignored because k1 or k2 is given.".to_string());
                }
                if model.gamma2Given {
                    warnings
                        .push("Warning: gamma2 is ignored because k1 or k2 is given.".to_string());
                }
            }
        } else {
            if !model.vbxGiven {
                pParam.vbx = pParam.phi - 7.7348e-4 * pParam.npeak * pParam.xt * pParam.xt;
            }
            if pParam.vbx > 0.0 {
                pParam.vbx = -pParam.vbx;
            }
            if pParam.vbm > 0.0 {
                pParam.vbm = -pParam.vbm;
            }
            if !model.gamma1Given {
                pParam.gamma1 = 5.753e-12 * sqrt(pParam.npeak) / model.cox;
            }
            if !model.gamma2Given {
                pParam.gamma2 = 5.753e-12 * sqrt(pParam.nsub) / model.cox;
            }
            T0 = pParam.gamma1 - pParam.gamma2;
            T1 = sqrt(pParam.phi - pParam.vbx) - pParam.sqrtPhi;
            T2 = sqrt(pParam.phi * (pParam.phi - pParam.vbm)) - pParam.phi;
            pParam.k2 = T0 * T1 / (2.0 * T2 + pParam.vbm);
            pParam.k1 = pParam.gamma2 - 2.0 * pParam.k2 * sqrt(pParam.phi - pParam.vbm);
        }
        if pParam.k2 < 0.0 {
            T0 = 0.5 * pParam.k1 / pParam.k2;
            pParam.vbsc = 0.9 * (pParam.phi - T0 * T0);
            if pParam.vbsc > -3.0 {
                pParam.vbsc = -3.0;
            } else if pParam.vbsc < -30.0 {
                pParam.vbsc = -30.0;
            }
        } else {
            pParam.vbsc = -30.0;
        }
        if pParam.vbsc > pParam.vbm {
            pParam.vbsc = pParam.vbm;
        }
        if !model.vfbGiven {
            if model.vth0Given {
                pParam.vfb =
                    f64::from(model.r#type) * pParam.vth0 - pParam.phi - pParam.k1 * pParam.sqrtPhi;
            } else {
                pParam.vfb = -1.0;
            }
        }
        if !model.vth0Given {
            pParam.vth0 =
                f64::from(model.r#type) * (pParam.vfb + pParam.phi + pParam.k1 * pParam.sqrtPhi);
        }
        pParam.k1ox = pParam.k1 * model.tox / model.toxm;
        pParam.k2ox = pParam.k2 * model.tox / model.toxm;
        T1 = sqrt(EPSSI / EPSOX * model.tox * pParam.Xdep0);
        T0 = exp(-0.5 * pParam.dsub * pParam.leff / T1);
        pParam.theta0vb0 = (T0 + 2.0 * T0 * T0);
        T0 = exp(-0.5 * pParam.drout * pParam.leff / T1);
        T2 = (T0 + 2.0 * T0 * T0);
        pParam.thetaRout = pParam.pdibl1 * T2 + pParam.pdibl2;
        tmp = sqrt(pParam.Xdep0);
        tmp1 = pParam.vbi - pParam.phi;
        tmp2 = model.factor1 * tmp;
        T0 = -0.5 * pParam.dvt1w * pParam.weff * pParam.leff / tmp2;
        if T0 > -EXP_THRESHOLD {
            T1 = exp(T0);
            T2 = T1 * (1.0 + 2.0 * T1);
        } else {
            T1 = MIN_EXP;
            T2 = T1 * (1.0 + 2.0 * T1);
        }
        T0 = pParam.dvt0w * T2;
        T2 = T0 * tmp1;
        T0 = -0.5 * pParam.dvt1 * pParam.leff / tmp2;
        if T0 > -EXP_THRESHOLD {
            T1 = exp(T0);
            T3 = T1 * (1.0 + 2.0 * T1);
        } else {
            T1 = MIN_EXP;
            T3 = T1 * (1.0 + 2.0 * T1);
        }
        T3 = pParam.dvt0 * T3 * tmp1;
        T4 = model.tox * pParam.phi / (pParam.weff + pParam.w0);
        T0 = sqrt(1.0 + pParam.nlx / pParam.leff);
        T5 = pParam.k1ox * (T0 - 1.0) * pParam.sqrtPhi
            + (pParam.kt1 + pParam.kt1l / pParam.leff) * (TRatio - 1.0);
        tmp3 = f64::from(model.r#type) * pParam.vth0 - T2 - T3 + pParam.k3 * T4 + T5;
        pParam.vfbzb = tmp3 - pParam.phi - pParam.k1 * pParam.sqrtPhi;
        // C label `stray-close-brace:`.
        cache.knots.push(pParam.clone());
        pParam
    };
    here.vth0 = pParam.vth0 + here.delvto;
    here.vfb = pParam.vfb + f64::from(model.r#type) * here.delvto;
    here.vfbzb = pParam.vfbzb + f64::from(model.r#type) * here.delvto;
    here.u0temp = pParam.u0temp * here.mulu0;
    here.tconst = here.u0temp * pParam.elm / (model.cox * pParam.weffCV * pParam.leffCV * T0);
    let mut drainResistance: f64 = 0.0;
    let mut sourceResistance: f64 = 0.0;
    if model.acmMod == 0 {
        drainResistance = model.sheetResistance * here.drainSquares;
        sourceResistance = model.sheetResistance * here.sourceSquares;
    } else {
        return Err(SpiceError::not_yet_ported(
            format!("BSIM3 acmMod={} source/drain resistances", model.acmMod),
            "src/spicelib/devices/bsim3/b3temp.c (ACM_SourceDrainResistances)",
        ));
    }
    if drainResistance > 0.0 {
        here.drainConductance = 1.0 / drainResistance;
    } else {
        here.drainConductance = 0.0;
    }
    if sourceResistance > 0.0 {
        here.sourceConductance = 1.0 / sourceResistance;
    } else {
        here.sourceConductance = 0.0;
    }
    here.cgso = pParam.cgso;
    here.cgdo = pParam.cgdo;
    Nvtm = model.vtm * model.jctEmissionCoeff;
    if (here.sourceArea <= 0.0) && (here.sourcePerimeter <= 0.0) {
        SourceSatCurrent = 1.0e-14;
    } else {
        SourceSatCurrent = here.sourceArea * model.jctTempSatCurDensity
            + here.sourcePerimeter * model.jctSidewallTempSatCurDensity;
    }
    if (SourceSatCurrent > 0.0) && (model.ijth > 0.0) {
        here.vjsm = Nvtm * log(model.ijth / SourceSatCurrent + 1.0);
        here.IsEvjsm = SourceSatCurrent * exp(here.vjsm / Nvtm);
    }
    if (here.drainArea <= 0.0) && (here.drainPerimeter <= 0.0) {
        DrainSatCurrent = 1.0e-14;
    } else {
        DrainSatCurrent = here.drainArea * model.jctTempSatCurDensity
            + here.drainPerimeter * model.jctSidewallTempSatCurDensity;
    }
    if (DrainSatCurrent > 0.0) && (model.ijth > 0.0) {
        here.vjdm = Nvtm * log(model.ijth / DrainSatCurrent + 1.0);
        here.IsEvjdm = DrainSatCurrent * exp(here.vjdm / Nvtm);
    }
    // C label `stray-close-brace:`.
    // C label `stray-close-brace:`.
    cache.T0 = T0;
    Ok(pParam)
}
