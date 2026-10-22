//! `b3set.c` (`BSIM3setup`): model defaults, then instance defaults and the
//! internal nodes C creates. Matrix elements are listed in `elements.rs`.
//!
//! Both passes follow C statement by statement over C's own members. The
//! ACM source/drain resistance model (`acmMod > 0`, `ACM_SourceDrainResistances`
//! in `src/spicelib/devices/bsim3/b3set.c`'s helper) is refused explicitly.
#![allow(non_snake_case)] // C's member names.
#![allow(clippy::excessive_precision)] // C's literal spellings.
#![allow(clippy::approx_constant)] // b3set.c's intentionally rounded PI.
#![allow(clippy::nonminimal_bool)] // C's negated given-bit tests, kept as written.
#![allow(unused_parens)] // C's parenthesisation is kept verbatim.

use super::fields::{Instance, Model};
use super::host::node;
use crate::primitives::{SpiceError, SpiceResult};

const EPSOX: f64 = 3.453133e-11;
const PI: f64 = 3.141592654;

fn log(x: f64) -> f64 {
    x.ln()
}

/// `BSIM3setup`'s model pass: every given-guarded default and the dependent
/// defaults, in C order. `nominal_temperature` is `CKTnomTemp` in kelvin.
/// C's `printf` warnings are appended to `warnings` verbatim (without their
/// trailing newline).
#[allow(clippy::too_many_lines)] // One C function, kept whole.
pub(crate) fn setup_model(model: &mut Model, nominal_temperature: f64, warnings: &mut Vec<String>) {
    if !model.typeGiven {
        model.r#type = 1;
    }
    if !model.mobModGiven {
        model.mobMod = 1;
    }
    if !model.binUnitGiven {
        model.binUnit = 1;
    }
    if !model.paramChkGiven {
        model.paramChk = 0;
    }
    if !model.capModGiven {
        model.capMod = 3;
    }
    if !model.acmModGiven {
        model.acmMod = 0;
    }
    if !model.calcacmGiven {
        model.calcacm = 0;
    }
    if !model.noiModGiven {
        model.noiMod = 1;
    }
    if !model.nqsModGiven {
        model.nqsMod = 0;
    } else if (model.nqsMod != 0) && (model.nqsMod != 1) {
        model.nqsMod = 0;
        warnings.push("Warning: nqsMod has been set to its default value: 0.".to_string());
    }
    if !model.acnqsModGiven {
        model.acnqsMod = 0;
    } else if (model.acnqsMod != 0) && (model.acnqsMod != 1) {
        model.acnqsMod = 0;
        warnings.push("Warning: acnqsMod has been set to its default value: 0.".to_string());
    }
    if !model.versionGiven {
        model.version = "3.3.0".to_string();
    }
    if !model.toxGiven {
        model.tox = 150.0e-10;
    }
    model.cox = 3.453133e-11 / model.tox;
    if !model.toxmGiven {
        model.toxm = model.tox;
    }
    if !model.cdscGiven {
        model.cdsc = 2.4e-4;
    }
    if !model.cdscbGiven {
        model.cdscb = 0.0;
    }
    if !model.cdscdGiven {
        model.cdscd = 0.0;
    }
    if !model.citGiven {
        model.cit = 0.0;
    }
    if !model.nfactorGiven {
        model.nfactor = 1.0;
    }
    if !model.xjGiven {
        model.xj = 0.15e-6;
    }
    if !model.vsatGiven {
        model.vsat = 8.0e4;
    }
    if !model.atGiven {
        model.at = 3.3e4;
    }
    if !model.a0Given {
        model.a0 = 1.0;
    }
    if !model.agsGiven {
        model.ags = 0.0;
    }
    if !model.a1Given {
        model.a1 = 0.0;
    }
    if !model.a2Given {
        model.a2 = 1.0;
    }
    if !model.ketaGiven {
        model.keta = -0.047;
    }
    if !model.nsubGiven {
        model.nsub = 6.0e16;
    }
    if !model.npeakGiven {
        model.npeak = 1.7e17;
    }
    if !model.ngateGiven {
        model.ngate = 0.0;
    }
    if !model.vbmGiven {
        model.vbm = -3.0;
    }
    if !model.xtGiven {
        model.xt = 1.55e-7;
    }
    if !model.kt1Given {
        model.kt1 = -0.11;
    }
    if !model.kt1lGiven {
        model.kt1l = 0.0;
    }
    if !model.kt2Given {
        model.kt2 = 0.022;
    }
    if !model.k3Given {
        model.k3 = 80.0;
    }
    if !model.k3bGiven {
        model.k3b = 0.0;
    }
    if !model.w0Given {
        model.w0 = 2.5e-6;
    }
    if !model.nlxGiven {
        model.nlx = 1.74e-7;
    }
    if !model.dvt0Given {
        model.dvt0 = 2.2;
    }
    if !model.dvt1Given {
        model.dvt1 = 0.53;
    }
    if !model.dvt2Given {
        model.dvt2 = -0.032;
    }
    if !model.dvt0wGiven {
        model.dvt0w = 0.0;
    }
    if !model.dvt1wGiven {
        model.dvt1w = 5.3e6;
    }
    if !model.dvt2wGiven {
        model.dvt2w = -0.032;
    }
    if !model.droutGiven {
        model.drout = 0.56;
    }
    if !model.dsubGiven {
        model.dsub = model.drout;
    }
    if !model.vth0Given {
        model.vth0 = if (model.r#type == 1) { 0.7 } else { -0.7 };
    }
    if !model.uaGiven {
        model.ua = 2.25e-9;
    }
    if !model.ua1Given {
        model.ua1 = 4.31e-9;
    }
    if !model.ubGiven {
        model.ub = 5.87e-19;
    }
    if !model.ub1Given {
        model.ub1 = -7.61e-18;
    }
    if !model.ucGiven {
        model.uc = if (model.mobMod == 3) {
            -0.0465
        } else {
            -0.0465e-9
        };
    }
    if !model.uc1Given {
        model.uc1 = if (model.mobMod == 3) {
            -0.056
        } else {
            -0.056e-9
        };
    }
    if !model.u0Given {
        model.u0 = if (model.r#type == 1) { 0.067 } else { 0.025 };
    }
    if !model.uteGiven {
        model.ute = -1.5;
    }
    if !model.voffGiven {
        model.voff = -0.08;
    }
    if !model.deltaGiven {
        model.delta = 0.01;
    }
    if !model.rdswGiven {
        model.rdsw = 0.0;
    }
    if !model.prwgGiven {
        model.prwg = 0.0;
    }
    if !model.prwbGiven {
        model.prwb = 0.0;
    }
    if !model.prtGiven {
        model.prt = 0.0;
    }
    if !model.eta0Given {
        model.eta0 = 0.08;
    }
    if !model.etabGiven {
        model.etab = -0.07;
    }
    if !model.pclmGiven {
        model.pclm = 1.3;
    }
    if !model.pdibl1Given {
        model.pdibl1 = 0.39;
    }
    if !model.pdibl2Given {
        model.pdibl2 = 0.0086;
    }
    if !model.pdiblbGiven {
        model.pdiblb = 0.0;
    }
    if !model.pscbe1Given {
        model.pscbe1 = 4.24e8;
    }
    if !model.pscbe2Given {
        model.pscbe2 = 1.0e-5;
    }
    if !model.pvagGiven {
        model.pvag = 0.0;
    }
    if !model.wrGiven {
        model.wr = 1.0;
    }
    if !model.dwgGiven {
        model.dwg = 0.0;
    }
    if !model.dwbGiven {
        model.dwb = 0.0;
    }
    if !model.b0Given {
        model.b0 = 0.0;
    }
    if !model.b1Given {
        model.b1 = 0.0;
    }
    if !model.alpha0Given {
        model.alpha0 = 0.0;
    }
    if !model.alpha1Given {
        model.alpha1 = 0.0;
    }
    if !model.beta0Given {
        model.beta0 = 30.0;
    }
    if !model.ijthGiven {
        model.ijth = 0.1;
    }
    if !model.elmGiven {
        model.elm = 5.0;
    }
    if !model.cgslGiven {
        model.cgsl = 0.0;
    }
    if !model.cgdlGiven {
        model.cgdl = 0.0;
    }
    if !model.ckappaGiven {
        model.ckappa = 0.6;
    }
    if !model.clcGiven {
        model.clc = 0.1e-6;
    }
    if !model.cleGiven {
        model.cle = 0.6;
    }
    if !model.vfbcvGiven {
        model.vfbcv = -1.0;
    }
    if !model.acdeGiven {
        model.acde = 1.0;
    }
    if !model.moinGiven {
        model.moin = 15.0;
    }
    if !model.noffGiven {
        model.noff = 1.0;
    }
    if !model.voffcvGiven {
        model.voffcv = 0.0;
    }
    if !model.tcjGiven {
        model.tcj = 0.0;
    }
    if !model.tpbGiven {
        model.tpb = 0.0;
    }
    if !model.tcjswGiven {
        model.tcjsw = 0.0;
    }
    if !model.tpbswGiven {
        model.tpbsw = 0.0;
    }
    if !model.tcjswgGiven {
        model.tcjswg = 0.0;
    }
    if !model.tpbswgGiven {
        model.tpbswg = 0.0;
    }
    if !model.hdifGiven {
        model.hdif = 0.0;
    }
    if !model.ldifGiven {
        model.ldif = 0.0;
    }
    if !model.ldGiven {
        model.ld = 0.0;
    }
    if !model.rdGiven {
        model.rd = 0.0;
    }
    if !model.rsGiven {
        model.rs = 0.0;
    }
    if !model.rdcGiven {
        model.rdc = 0.0;
    }
    if !model.rscGiven {
        model.rsc = 0.0;
    }
    if !model.wmltGiven {
        model.wmlt = 1.0;
    }
    if !model.lcdscGiven {
        model.lcdsc = 0.0;
    }
    if !model.lcdscbGiven {
        model.lcdscb = 0.0;
    }
    if !model.lcdscdGiven {
        model.lcdscd = 0.0;
    }
    if !model.lcitGiven {
        model.lcit = 0.0;
    }
    if !model.lnfactorGiven {
        model.lnfactor = 0.0;
    }
    if !model.lxjGiven {
        model.lxj = 0.0;
    }
    if !model.lvsatGiven {
        model.lvsat = 0.0;
    }
    if !model.latGiven {
        model.lat = 0.0;
    }
    if !model.la0Given {
        model.la0 = 0.0;
    }
    if !model.lagsGiven {
        model.lags = 0.0;
    }
    if !model.la1Given {
        model.la1 = 0.0;
    }
    if !model.la2Given {
        model.la2 = 0.0;
    }
    if !model.lketaGiven {
        model.lketa = 0.0;
    }
    if !model.lnsubGiven {
        model.lnsub = 0.0;
    }
    if !model.lnpeakGiven {
        model.lnpeak = 0.0;
    }
    if !model.lngateGiven {
        model.lngate = 0.0;
    }
    if !model.lvbmGiven {
        model.lvbm = 0.0;
    }
    if !model.lxtGiven {
        model.lxt = 0.0;
    }
    if !model.lkt1Given {
        model.lkt1 = 0.0;
    }
    if !model.lkt1lGiven {
        model.lkt1l = 0.0;
    }
    if !model.lkt2Given {
        model.lkt2 = 0.0;
    }
    if !model.lk3Given {
        model.lk3 = 0.0;
    }
    if !model.lk3bGiven {
        model.lk3b = 0.0;
    }
    if !model.lw0Given {
        model.lw0 = 0.0;
    }
    if !model.lnlxGiven {
        model.lnlx = 0.0;
    }
    if !model.ldvt0Given {
        model.ldvt0 = 0.0;
    }
    if !model.ldvt1Given {
        model.ldvt1 = 0.0;
    }
    if !model.ldvt2Given {
        model.ldvt2 = 0.0;
    }
    if !model.ldvt0wGiven {
        model.ldvt0w = 0.0;
    }
    if !model.ldvt1wGiven {
        model.ldvt1w = 0.0;
    }
    if !model.ldvt2wGiven {
        model.ldvt2w = 0.0;
    }
    if !model.ldroutGiven {
        model.ldrout = 0.0;
    }
    if !model.ldsubGiven {
        model.ldsub = 0.0;
    }
    if !model.lvth0Given {
        model.lvth0 = 0.0;
    }
    if !model.luaGiven {
        model.lua = 0.0;
    }
    if !model.lua1Given {
        model.lua1 = 0.0;
    }
    if !model.lubGiven {
        model.lub = 0.0;
    }
    if !model.lub1Given {
        model.lub1 = 0.0;
    }
    if !model.lucGiven {
        model.luc = 0.0;
    }
    if !model.luc1Given {
        model.luc1 = 0.0;
    }
    if !model.lu0Given {
        model.lu0 = 0.0;
    }
    if !model.luteGiven {
        model.lute = 0.0;
    }
    if !model.lvoffGiven {
        model.lvoff = 0.0;
    }
    if !model.ldeltaGiven {
        model.ldelta = 0.0;
    }
    if !model.lrdswGiven {
        model.lrdsw = 0.0;
    }
    if !model.lprwbGiven {
        model.lprwb = 0.0;
    }
    if !model.lprwgGiven {
        model.lprwg = 0.0;
    }
    if !model.lprtGiven {
        model.lprt = 0.0;
    }
    if !model.leta0Given {
        model.leta0 = 0.0;
    }
    if !model.letabGiven {
        model.letab = -0.0;
    }
    if !model.lpclmGiven {
        model.lpclm = 0.0;
    }
    if !model.lpdibl1Given {
        model.lpdibl1 = 0.0;
    }
    if !model.lpdibl2Given {
        model.lpdibl2 = 0.0;
    }
    if !model.lpdiblbGiven {
        model.lpdiblb = 0.0;
    }
    if !model.lpscbe1Given {
        model.lpscbe1 = 0.0;
    }
    if !model.lpscbe2Given {
        model.lpscbe2 = 0.0;
    }
    if !model.lpvagGiven {
        model.lpvag = 0.0;
    }
    if !model.lwrGiven {
        model.lwr = 0.0;
    }
    if !model.ldwgGiven {
        model.ldwg = 0.0;
    }
    if !model.ldwbGiven {
        model.ldwb = 0.0;
    }
    if !model.lb0Given {
        model.lb0 = 0.0;
    }
    if !model.lb1Given {
        model.lb1 = 0.0;
    }
    if !model.lalpha0Given {
        model.lalpha0 = 0.0;
    }
    if !model.lalpha1Given {
        model.lalpha1 = 0.0;
    }
    if !model.lbeta0Given {
        model.lbeta0 = 0.0;
    }
    if !model.lvfbGiven {
        model.lvfb = 0.0;
    }
    if !model.lelmGiven {
        model.lelm = 0.0;
    }
    if !model.lcgslGiven {
        model.lcgsl = 0.0;
    }
    if !model.lcgdlGiven {
        model.lcgdl = 0.0;
    }
    if !model.lckappaGiven {
        model.lckappa = 0.0;
    }
    if !model.lclcGiven {
        model.lclc = 0.0;
    }
    if !model.lcleGiven {
        model.lcle = 0.0;
    }
    if !model.lcfGiven {
        model.lcf = 0.0;
    }
    if !model.lvfbcvGiven {
        model.lvfbcv = 0.0;
    }
    if !model.lacdeGiven {
        model.lacde = 0.0;
    }
    if !model.lmoinGiven {
        model.lmoin = 0.0;
    }
    if !model.lnoffGiven {
        model.lnoff = 0.0;
    }
    if !model.lvoffcvGiven {
        model.lvoffcv = 0.0;
    }
    if !model.wcdscGiven {
        model.wcdsc = 0.0;
    }
    if !model.wcdscbGiven {
        model.wcdscb = 0.0;
    }
    if !model.wcdscdGiven {
        model.wcdscd = 0.0;
    }
    if !model.wcitGiven {
        model.wcit = 0.0;
    }
    if !model.wnfactorGiven {
        model.wnfactor = 0.0;
    }
    if !model.wxjGiven {
        model.wxj = 0.0;
    }
    if !model.wvsatGiven {
        model.wvsat = 0.0;
    }
    if !model.watGiven {
        model.wat = 0.0;
    }
    if !model.wa0Given {
        model.wa0 = 0.0;
    }
    if !model.wagsGiven {
        model.wags = 0.0;
    }
    if !model.wa1Given {
        model.wa1 = 0.0;
    }
    if !model.wa2Given {
        model.wa2 = 0.0;
    }
    if !model.wketaGiven {
        model.wketa = 0.0;
    }
    if !model.wnsubGiven {
        model.wnsub = 0.0;
    }
    if !model.wnpeakGiven {
        model.wnpeak = 0.0;
    }
    if !model.wngateGiven {
        model.wngate = 0.0;
    }
    if !model.wvbmGiven {
        model.wvbm = 0.0;
    }
    if !model.wxtGiven {
        model.wxt = 0.0;
    }
    if !model.wkt1Given {
        model.wkt1 = 0.0;
    }
    if !model.wkt1lGiven {
        model.wkt1l = 0.0;
    }
    if !model.wkt2Given {
        model.wkt2 = 0.0;
    }
    if !model.wk3Given {
        model.wk3 = 0.0;
    }
    if !model.wk3bGiven {
        model.wk3b = 0.0;
    }
    if !model.ww0Given {
        model.ww0 = 0.0;
    }
    if !model.wnlxGiven {
        model.wnlx = 0.0;
    }
    if !model.wdvt0Given {
        model.wdvt0 = 0.0;
    }
    if !model.wdvt1Given {
        model.wdvt1 = 0.0;
    }
    if !model.wdvt2Given {
        model.wdvt2 = 0.0;
    }
    if !model.wdvt0wGiven {
        model.wdvt0w = 0.0;
    }
    if !model.wdvt1wGiven {
        model.wdvt1w = 0.0;
    }
    if !model.wdvt2wGiven {
        model.wdvt2w = 0.0;
    }
    if !model.wdroutGiven {
        model.wdrout = 0.0;
    }
    if !model.wdsubGiven {
        model.wdsub = 0.0;
    }
    if !model.wvth0Given {
        model.wvth0 = 0.0;
    }
    if !model.wuaGiven {
        model.wua = 0.0;
    }
    if !model.wua1Given {
        model.wua1 = 0.0;
    }
    if !model.wubGiven {
        model.wub = 0.0;
    }
    if !model.wub1Given {
        model.wub1 = 0.0;
    }
    if !model.wucGiven {
        model.wuc = 0.0;
    }
    if !model.wuc1Given {
        model.wuc1 = 0.0;
    }
    if !model.wu0Given {
        model.wu0 = 0.0;
    }
    if !model.wuteGiven {
        model.wute = 0.0;
    }
    if !model.wvoffGiven {
        model.wvoff = 0.0;
    }
    if !model.wdeltaGiven {
        model.wdelta = 0.0;
    }
    if !model.wrdswGiven {
        model.wrdsw = 0.0;
    }
    if !model.wprwbGiven {
        model.wprwb = 0.0;
    }
    if !model.wprwgGiven {
        model.wprwg = 0.0;
    }
    if !model.wprtGiven {
        model.wprt = 0.0;
    }
    if !model.weta0Given {
        model.weta0 = 0.0;
    }
    if !model.wetabGiven {
        model.wetab = 0.0;
    }
    if !model.wpclmGiven {
        model.wpclm = 0.0;
    }
    if !model.wpdibl1Given {
        model.wpdibl1 = 0.0;
    }
    if !model.wpdibl2Given {
        model.wpdibl2 = 0.0;
    }
    if !model.wpdiblbGiven {
        model.wpdiblb = 0.0;
    }
    if !model.wpscbe1Given {
        model.wpscbe1 = 0.0;
    }
    if !model.wpscbe2Given {
        model.wpscbe2 = 0.0;
    }
    if !model.wpvagGiven {
        model.wpvag = 0.0;
    }
    if !model.wwrGiven {
        model.wwr = 0.0;
    }
    if !model.wdwgGiven {
        model.wdwg = 0.0;
    }
    if !model.wdwbGiven {
        model.wdwb = 0.0;
    }
    if !model.wb0Given {
        model.wb0 = 0.0;
    }
    if !model.wb1Given {
        model.wb1 = 0.0;
    }
    if !model.walpha0Given {
        model.walpha0 = 0.0;
    }
    if !model.walpha1Given {
        model.walpha1 = 0.0;
    }
    if !model.wbeta0Given {
        model.wbeta0 = 0.0;
    }
    if !model.wvfbGiven {
        model.wvfb = 0.0;
    }
    if !model.welmGiven {
        model.welm = 0.0;
    }
    if !model.wcgslGiven {
        model.wcgsl = 0.0;
    }
    if !model.wcgdlGiven {
        model.wcgdl = 0.0;
    }
    if !model.wckappaGiven {
        model.wckappa = 0.0;
    }
    if !model.wcfGiven {
        model.wcf = 0.0;
    }
    if !model.wclcGiven {
        model.wclc = 0.0;
    }
    if !model.wcleGiven {
        model.wcle = 0.0;
    }
    if !model.wvfbcvGiven {
        model.wvfbcv = 0.0;
    }
    if !model.wacdeGiven {
        model.wacde = 0.0;
    }
    if !model.wmoinGiven {
        model.wmoin = 0.0;
    }
    if !model.wnoffGiven {
        model.wnoff = 0.0;
    }
    if !model.wvoffcvGiven {
        model.wvoffcv = 0.0;
    }
    if !model.pcdscGiven {
        model.pcdsc = 0.0;
    }
    if !model.pcdscbGiven {
        model.pcdscb = 0.0;
    }
    if !model.pcdscdGiven {
        model.pcdscd = 0.0;
    }
    if !model.pcitGiven {
        model.pcit = 0.0;
    }
    if !model.pnfactorGiven {
        model.pnfactor = 0.0;
    }
    if !model.pxjGiven {
        model.pxj = 0.0;
    }
    if !model.pvsatGiven {
        model.pvsat = 0.0;
    }
    if !model.patGiven {
        model.pat = 0.0;
    }
    if !model.pa0Given {
        model.pa0 = 0.0;
    }
    if !model.pagsGiven {
        model.pags = 0.0;
    }
    if !model.pa1Given {
        model.pa1 = 0.0;
    }
    if !model.pa2Given {
        model.pa2 = 0.0;
    }
    if !model.pketaGiven {
        model.pketa = 0.0;
    }
    if !model.pnsubGiven {
        model.pnsub = 0.0;
    }
    if !model.pnpeakGiven {
        model.pnpeak = 0.0;
    }
    if !model.pngateGiven {
        model.pngate = 0.0;
    }
    if !model.pvbmGiven {
        model.pvbm = 0.0;
    }
    if !model.pxtGiven {
        model.pxt = 0.0;
    }
    if !model.pkt1Given {
        model.pkt1 = 0.0;
    }
    if !model.pkt1lGiven {
        model.pkt1l = 0.0;
    }
    if !model.pkt2Given {
        model.pkt2 = 0.0;
    }
    if !model.pk3Given {
        model.pk3 = 0.0;
    }
    if !model.pk3bGiven {
        model.pk3b = 0.0;
    }
    if !model.pw0Given {
        model.pw0 = 0.0;
    }
    if !model.pnlxGiven {
        model.pnlx = 0.0;
    }
    if !model.pdvt0Given {
        model.pdvt0 = 0.0;
    }
    if !model.pdvt1Given {
        model.pdvt1 = 0.0;
    }
    if !model.pdvt2Given {
        model.pdvt2 = 0.0;
    }
    if !model.pdvt0wGiven {
        model.pdvt0w = 0.0;
    }
    if !model.pdvt1wGiven {
        model.pdvt1w = 0.0;
    }
    if !model.pdvt2wGiven {
        model.pdvt2w = 0.0;
    }
    if !model.pdroutGiven {
        model.pdrout = 0.0;
    }
    if !model.pdsubGiven {
        model.pdsub = 0.0;
    }
    if !model.pvth0Given {
        model.pvth0 = 0.0;
    }
    if !model.puaGiven {
        model.pua = 0.0;
    }
    if !model.pua1Given {
        model.pua1 = 0.0;
    }
    if !model.pubGiven {
        model.r#pub = 0.0;
    }
    if !model.pub1Given {
        model.pub1 = 0.0;
    }
    if !model.pucGiven {
        model.puc = 0.0;
    }
    if !model.puc1Given {
        model.puc1 = 0.0;
    }
    if !model.pu0Given {
        model.pu0 = 0.0;
    }
    if !model.puteGiven {
        model.pute = 0.0;
    }
    if !model.pvoffGiven {
        model.pvoff = 0.0;
    }
    if !model.pdeltaGiven {
        model.pdelta = 0.0;
    }
    if !model.prdswGiven {
        model.prdsw = 0.0;
    }
    if !model.pprwbGiven {
        model.pprwb = 0.0;
    }
    if !model.pprwgGiven {
        model.pprwg = 0.0;
    }
    if !model.pprtGiven {
        model.pprt = 0.0;
    }
    if !model.peta0Given {
        model.peta0 = 0.0;
    }
    if !model.petabGiven {
        model.petab = 0.0;
    }
    if !model.ppclmGiven {
        model.ppclm = 0.0;
    }
    if !model.ppdibl1Given {
        model.ppdibl1 = 0.0;
    }
    if !model.ppdibl2Given {
        model.ppdibl2 = 0.0;
    }
    if !model.ppdiblbGiven {
        model.ppdiblb = 0.0;
    }
    if !model.ppscbe1Given {
        model.ppscbe1 = 0.0;
    }
    if !model.ppscbe2Given {
        model.ppscbe2 = 0.0;
    }
    if !model.ppvagGiven {
        model.ppvag = 0.0;
    }
    if !model.pwrGiven {
        model.pwr = 0.0;
    }
    if !model.pdwgGiven {
        model.pdwg = 0.0;
    }
    if !model.pdwbGiven {
        model.pdwb = 0.0;
    }
    if !model.pb0Given {
        model.pb0 = 0.0;
    }
    if !model.pb1Given {
        model.pb1 = 0.0;
    }
    if !model.palpha0Given {
        model.palpha0 = 0.0;
    }
    if !model.palpha1Given {
        model.palpha1 = 0.0;
    }
    if !model.pbeta0Given {
        model.pbeta0 = 0.0;
    }
    if !model.pvfbGiven {
        model.pvfb = 0.0;
    }
    if !model.pelmGiven {
        model.pelm = 0.0;
    }
    if !model.pcgslGiven {
        model.pcgsl = 0.0;
    }
    if !model.pcgdlGiven {
        model.pcgdl = 0.0;
    }
    if !model.pckappaGiven {
        model.pckappa = 0.0;
    }
    if !model.pcfGiven {
        model.pcf = 0.0;
    }
    if !model.pclcGiven {
        model.pclc = 0.0;
    }
    if !model.pcleGiven {
        model.pcle = 0.0;
    }
    if !model.pvfbcvGiven {
        model.pvfbcv = 0.0;
    }
    if !model.pacdeGiven {
        model.pacde = 0.0;
    }
    if !model.pmoinGiven {
        model.pmoin = 0.0;
    }
    if !model.pnoffGiven {
        model.pnoff = 0.0;
    }
    if !model.pvoffcvGiven {
        model.pvoffcv = 0.0;
    }
    if !model.tnomGiven {
        model.tnom = nominal_temperature;
    }
    if !model.lintnoiGiven {
        model.lintnoi = 0.0;
    }
    if !model.LintGiven {
        model.Lint = 0.0;
    }
    if !model.LlGiven {
        model.Ll = 0.0;
    }
    if !model.LlcGiven {
        model.Llc = model.Ll;
    }
    if !model.LlnGiven {
        model.Lln = 1.0;
    }
    if !model.LwGiven {
        model.Lw = 0.0;
    }
    if !model.LwcGiven {
        model.Lwc = model.Lw;
    }
    if !model.LwnGiven {
        model.Lwn = 1.0;
    }
    if !model.LwlGiven {
        model.Lwl = 0.0;
    }
    if !model.LwlcGiven {
        model.Lwlc = model.Lwl;
    }
    if !model.LminGiven {
        model.Lmin = 0.0;
    }
    if !model.LmaxGiven {
        model.Lmax = 1.0;
    }
    if !model.WintGiven {
        model.Wint = 0.0;
    }
    if !model.WlGiven {
        model.Wl = 0.0;
    }
    if !model.WlcGiven {
        model.Wlc = model.Wl;
    }
    if !model.WlnGiven {
        model.Wln = 1.0;
    }
    if !model.WwGiven {
        model.Ww = 0.0;
    }
    if !model.WwcGiven {
        model.Wwc = model.Ww;
    }
    if !model.WwnGiven {
        model.Wwn = 1.0;
    }
    if !model.WwlGiven {
        model.Wwl = 0.0;
    }
    if !model.WwlcGiven {
        model.Wwlc = model.Wwl;
    }
    if !model.WminGiven {
        model.Wmin = 0.0;
    }
    if !model.WmaxGiven {
        model.Wmax = 1.0;
    }
    if !model.dwcGiven {
        model.dwc = model.Wint;
    }
    if !model.dlcGiven {
        model.dlc = model.Lint;
    }
    if !model.xlGiven {
        model.xl = 0.0;
    }
    if !model.xwGiven {
        model.xw = 0.0;
    }
    if !model.cfGiven {
        model.cf = 2.0 * EPSOX / PI * log(1.0 + 0.4e-6 / model.tox);
    }
    if !model.cgdoGiven {
        if model.dlcGiven && (model.dlc > 0.0) {
            model.cgdo = model.dlc * model.cox - model.cgdl;
        } else {
            model.cgdo = 0.6 * model.xj * model.cox;
        }
    }
    if !model.cgsoGiven {
        if model.dlcGiven && (model.dlc > 0.0) {
            model.cgso = model.dlc * model.cox - model.cgsl;
        } else {
            model.cgso = 0.6 * model.xj * model.cox;
        }
    }
    if !model.cgboGiven {
        model.cgbo = 2.0 * model.dwc * model.cox;
    }
    if !model.xpartGiven {
        model.xpart = 0.0;
    }
    if !model.sheetResistanceGiven {
        model.sheetResistance = 0.0;
    }
    if !model.unitAreaJctCapGiven {
        model.unitAreaJctCap = 5.0E-4;
    }
    if !model.unitLengthSidewallJctCapGiven {
        model.unitLengthSidewallJctCap = 5.0E-10;
    }
    if !model.unitLengthGateSidewallJctCapGiven {
        model.unitLengthGateSidewallJctCap = model.unitLengthSidewallJctCap;
    }
    if !model.jctSatCurDensityGiven {
        model.jctSatCurDensity = 1.0E-4;
    }
    if !model.jctSidewallSatCurDensityGiven {
        model.jctSidewallSatCurDensity = 0.0;
    }
    if !model.bulkJctPotentialGiven {
        model.bulkJctPotential = 1.0;
    }
    if !model.sidewallJctPotentialGiven {
        model.sidewallJctPotential = 1.0;
    }
    if !model.GatesidewallJctPotentialGiven {
        model.GatesidewallJctPotential = model.sidewallJctPotential;
    }
    if !model.bulkJctBotGradingCoeffGiven {
        model.bulkJctBotGradingCoeff = 0.5;
    }
    if !model.bulkJctSideGradingCoeffGiven {
        model.bulkJctSideGradingCoeff = 0.33;
    }
    if !model.bulkJctGateSideGradingCoeffGiven {
        model.bulkJctGateSideGradingCoeff = model.bulkJctSideGradingCoeff;
    }
    if !model.jctEmissionCoeffGiven {
        model.jctEmissionCoeff = 1.0;
    }
    if !model.jctTempExponentGiven {
        model.jctTempExponent = 3.0;
    }
    if !model.oxideTrapDensityAGiven {
        if model.r#type == 1 {
            model.oxideTrapDensityA = 1e20;
        } else {
            model.oxideTrapDensityA = 9.9e18;
        }
    }
    if !model.oxideTrapDensityBGiven {
        if model.r#type == 1 {
            model.oxideTrapDensityB = 5e4;
        } else {
            model.oxideTrapDensityB = 2.4e3;
        }
    }
    if !model.oxideTrapDensityCGiven {
        if model.r#type == 1 {
            model.oxideTrapDensityC = -1.4e-12;
        } else {
            model.oxideTrapDensityC = 1.4e-12;
        }
    }
    if !model.emGiven {
        model.em = 4.1e7;
    }
    if !model.efGiven {
        model.ef = 1.0;
    }
    if !model.afGiven {
        model.af = 1.0;
    }
    if !model.kfGiven {
        model.kf = 0.0;
    }
    if !model.vgsMaxGiven {
        model.vgsMax = 1e99;
    }
    if !model.vgdMaxGiven {
        model.vgdMax = 1e99;
    }
    if !model.vgbMaxGiven {
        model.vgbMax = 1e99;
    }
    if !model.vdsMaxGiven {
        model.vdsMax = 1e99;
    }
    if !model.vbsMaxGiven {
        model.vbsMax = 1e99;
    }
    if !model.vbdMaxGiven {
        model.vbdMax = 1e99;
    }
    if !model.vgsrMaxGiven {
        model.vgsrMax = 1e99;
    }
    if !model.vgdrMaxGiven {
        model.vgdrMax = 1e99;
    }
    if !model.vgbrMaxGiven {
        model.vgbrMax = 1e99;
    }
    if !model.vbsrMaxGiven {
        model.vbsrMax = 1e99;
    }
    if !model.vbdrMaxGiven {
        model.vbdrMax = 1e99;
    }
}

/// `BSIM3setup`'s instance pass: the instance defaults, then the drain,
/// source and NQS charge nodes C creates, as `(local index, suffix)` in
/// creation order. `here`'s node fields must hold the external local nodes.
///
/// # Errors
/// `acmMod > 0` (ACM source/drain resistances are not ported).
pub(crate) fn setup_instance(
    model: &Model,
    here: &mut Instance,
    warnings: &mut Vec<String>,
) -> SpiceResult<Vec<(i32, &'static str)>> {
    if !here.drainAreaGiven {
        here.drainArea = 0.0;
    }
    if !here.drainPerimeterGiven {
        here.drainPerimeter = 0.0;
    }
    if !here.drainSquaresGiven {
        if model.acmMod == 0 {
            here.drainSquares = 1.0;
        } else {
            here.drainSquares = 0.0;
        }
    }
    if !here.delvtoGiven {
        here.delvto = 0.0;
    }
    if !here.mulu0Given {
        here.mulu0 = 1.0;
    }
    if !here.icVBSGiven {
        here.icVBS = 0.0;
    }
    if !here.icVDSGiven {
        here.icVDS = 0.0;
    }
    if !here.icVGSGiven {
        here.icVGS = 0.0;
    }
    if !here.lGiven {
        here.l = 5.0e-6;
    }
    if !here.sourceAreaGiven {
        here.sourceArea = 0.0;
    }
    if !here.sourcePerimeterGiven {
        here.sourcePerimeter = 0.0;
    }
    if !here.sourceSquaresGiven {
        if model.acmMod == 0 {
            here.sourceSquares = 1.0;
        } else {
            here.sourceSquares = 0.0;
        }
    }
    if !here.wGiven {
        here.w = 5.0e-6;
    }
    if !here.nqsModGiven {
        here.nqsMod = model.nqsMod;
    } else if (here.nqsMod != 0) && (here.nqsMod != 1) {
        here.nqsMod = model.nqsMod;
        warnings.push(format!(
            "Warning: nqsMod has been set to its global value {}.",
            model.nqsMod
        ));
    }
    if !here.acnqsModGiven {
        here.acnqsMod = model.acnqsMod;
    } else if (here.acnqsMod != 0) && (here.acnqsMod != 1) {
        here.acnqsMod = model.acnqsMod;
        warnings.push(format!(
            "Warning: acnqsMod has been set to its global value {}.",
            model.acnqsMod
        ));
    }
    if !here.geoGiven {
        here.geo = 0;
    }
    if !here.mGiven {
        here.m = 1.0;
    }
    let mut created = Vec::new();
    let (drainResistance, sourceResistance);
    if model.acmMod == 0 {
        drainResistance = model.sheetResistance * here.drainSquares;
        sourceResistance = model.sheetResistance * here.sourceSquares;
    } else {
        return Err(SpiceError::not_yet_ported(
            format!("BSIM3 acmMod={} source/drain resistances", model.acmMod),
            "src/spicelib/devices/bsim3/b3set.c (ACM_SourceDrainResistances)",
        ));
    }
    // `CKTmkVolt` runs only while the prime is still unset (C's 0); the
    // port's fresh instance has it unset, so the node is created.
    if drainResistance != 0.0 {
        here.dNodePrime = node::DP;
        created.push((node::DP, "drain"));
    } else {
        here.dNodePrime = here.dNode;
    }
    if sourceResistance != 0.0 {
        here.sNodePrime = node::SP;
        created.push((node::SP, "source"));
    } else {
        here.sNodePrime = here.sNode;
    }
    if here.nqsMod != 0 {
        here.qNode = node::Q;
        created.push((node::Q, "charge"));
    } else {
        here.qNode = node::GROUND;
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::super::{descriptor, parameters};
    use super::{Model, setup_model};

    #[test]
    fn literal_defaults_agree_with_the_generated_facts() {
        let mut model = Model {
            r#type: 1,
            typeGiven: true,
            ..Model::default()
        };
        let mut warnings = Vec::new();
        setup_model(&mut model, 300.15, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut checked = 0;
        for &(keyword, expected) in super::super::defaults_data::LITERALS {
            let p = descriptor(parameters::MODEL, keyword).unwrap();
            let field = p.c_field.unwrap();
            let actual = if let Some(v) = model.real_mut(field) {
                *v
            } else {
                f64::from(*model.int_mut(field).unwrap())
            };
            assert_eq!(actual.to_bits(), expected.to_bits(), "{keyword}");
            checked += 1;
        }
        assert_eq!(checked, 376);
        assert_eq!((model.vth0, model.u0, model.tnom), (0.7, 0.067, 300.15));
        assert_eq!(model.version, "3.3.0");
    }
}
