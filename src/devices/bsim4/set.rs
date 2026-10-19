//! `b4set.c` (`BSIM4setup`): model defaults, then instance defaults and the
//! internal nodes C creates. Matrix elements are listed in `elements.rs`.
//!
//! The model pass follows C statement by statement over C's own members,
//! including its quirk that any VERSION beginning `4.8` selects the 4.8.3
//! equations, because C tests `strncmp(version, "4.8", 3)` first.
#![allow(non_snake_case)] // C's member names.
#![allow(clippy::excessive_precision)] // C's literal spellings.
#![allow(clippy::double_parens)] // C's parenthesised conditions, kept as written.
#![allow(clippy::nonminimal_bool)] // C's negated given-bit tests, kept as written.

use super::fields::{Instance, Model};
use super::geo::rdseff_geo;
use super::host::node;
use super::{InputType, ModelParameters, parameters};
use crate::devices::schema::temperature_kelvin;
use crate::primitives::{Real, SpiceError, SpiceResult};

/// C's `!strncmp(a, b, n)` for a literal `b` at least `n` bytes long.
fn c_strncmp_eq(a: &str, b: &str, n: usize) -> bool {
    a.as_bytes().starts_with(&b.as_bytes()[..n])
}

/// `BSIM4setup`'s model pass: every given-guarded default, the dependent
/// defaults and the model selector checks, in C order. `nominal_temperature`
/// is `CKTnomTemp` in kelvin. C's `printf` warnings are appended to
/// `warnings` verbatim.
///
/// # Errors
/// None today: C's model pass has no error return. The `Result` keeps the
/// stage signature uniform.
#[allow(clippy::too_many_lines)] // One C function, kept whole.
#[allow(clippy::unnecessary_wraps)] // Uniform stage signature.
pub(crate) fn setup_model(
    m: &mut Model,
    nominal_temperature: f64,
    warnings: &mut Vec<String>,
) -> SpiceResult<()> {
    if !m.typeGiven {
        m.r#type = 1;
    }
    if !m.mobModGiven {
        m.mobMod = 0;
    } else if (m.mobMod != 0)
        && (m.mobMod != 1)
        && (m.mobMod != 2)
        && (m.mobMod != 3)
        && (m.mobMod != 4)
        && (m.mobMod != 5)
        && (m.mobMod != 6)
    {
        m.mobMod = 0;
        warnings.push("Warning: mobMod has been set to its default value: 0.".into());
    }
    if !m.binUnitGiven {
        m.binUnit = 1;
    }
    if !m.paramChkGiven {
        m.paramChk = 1;
    }
    if !m.dioModGiven {
        m.dioMod = 1;
    } else if (m.dioMod != 0) && (m.dioMod != 1) && (m.dioMod != 2) {
        m.dioMod = 1;
        warnings.push("Warning: dioMod has been set to its default value: 1.".into());
    }
    if !m.cvchargeModGiven {
        m.cvchargeMod = 0;
    }
    if !m.capModGiven {
        m.capMod = 2;
    } else if (m.capMod != 0) && (m.capMod != 1) && (m.capMod != 2) {
        m.capMod = 2;
        warnings.push("Warning: capMod has been set to its default value: 2.".into());
    }
    if !m.rdsModGiven {
        m.rdsMod = 0;
    } else if (m.rdsMod != 0) && (m.rdsMod != 1) {
        m.rdsMod = 0;
        warnings.push("Warning: rdsMod has been set to its default value: 0.".into());
    }
    if !m.rbodyModGiven {
        m.rbodyMod = 0;
    } else if (m.rbodyMod != 0) && (m.rbodyMod != 1) && (m.rbodyMod != 2) {
        m.rbodyMod = 0;
        warnings.push("Warning: rbodyMod has been set to its default value: 0.".into());
    }
    if !m.rgateModGiven {
        m.rgateMod = 0;
    } else if (m.rgateMod != 0) && (m.rgateMod != 1) && (m.rgateMod != 2) && (m.rgateMod != 3) {
        m.rgateMod = 0;
        warnings.push("Warning: rgateMod has been set to its default value: 0.".into());
    }
    if !m.perModGiven {
        m.perMod = 1;
    } else if (m.perMod != 0) && (m.perMod != 1) {
        m.perMod = 1;
        warnings.push("Warning: perMod has been set to its default value: 1.".into());
    }
    if !m.geoModGiven {
        m.geoMod = 0;
    }
    if !m.fnoiModGiven {
        m.fnoiMod = 1;
    } else if (m.fnoiMod != 0) && (m.fnoiMod != 1) {
        m.fnoiMod = 1;
        warnings.push("Warning: fnoiMod has been set to its default value: 1.".into());
    }
    if !m.tnoiModGiven {
        m.tnoiMod = 0;
    } else if (m.tnoiMod != 0) && (m.tnoiMod != 1) && (m.tnoiMod != 2) {
        m.tnoiMod = 0;
        warnings.push("Warning: tnoiMod has been set to its default value: 0.".into());
    }
    if !m.trnqsModGiven {
        m.trnqsMod = 0;
    } else if (m.trnqsMod != 0) && (m.trnqsMod != 1) {
        m.trnqsMod = 0;
        warnings.push("Warning: trnqsMod has been set to its default value: 0.".into());
    }
    if !m.acnqsModGiven {
        m.acnqsMod = 0;
    } else if (m.acnqsMod != 0) && (m.acnqsMod != 1) {
        m.acnqsMod = 0;
        warnings.push("Warning: acnqsMod has been set to its default value: 0.".into());
    }
    if !m.mtrlModGiven {
        m.mtrlMod = 0;
    } else if (m.mtrlMod != 0) && (m.mtrlMod != 1) {
        m.mtrlMod = 0;
        warnings.push("Warning: mtrlMod has been set to its default value: 0.".into());
    }
    if !m.mtrlCompatModGiven {
        m.mtrlCompatMod = 0;
    } else if (m.mtrlCompatMod != 0) && (m.mtrlCompatMod != 1) {
        m.mtrlCompatMod = 0;
        warnings.push("Warning: mtrlCompatMod has been set to its default value: 0.".into());
    }
    if !m.igcModGiven {
        m.igcMod = 0;
    } else if (m.igcMod != 0) && (m.igcMod != 1) && (m.igcMod != 2) {
        m.igcMod = 0;
        warnings.push("Warning: igcMod has been set to its default value: 0.".into());
    }
    if !m.igbModGiven {
        m.igbMod = 0;
    } else if (m.igbMod != 0) && (m.igbMod != 1) {
        m.igbMod = 0;
        warnings.push("Warning: igbMod has been set to its default value: 0.".into());
    }
    if !m.tempModGiven {
        m.tempMod = 0;
    } else if (m.tempMod != 0) && (m.tempMod != 1) && (m.tempMod != 2) && (m.tempMod != 3) {
        m.tempMod = 0;
        warnings.push("Warning: tempMod has been set to its default value: 0.".into());
    }
    if !m.versionGiven {
        m.version = String::from("4.8.3");
    }
    if (m.version == "4.8.3")
        || (c_strncmp_eq(&m.version, "4.83", 4))
        || (c_strncmp_eq(&m.version, "4.8", 3))
    {
        m.v48intVersion = 483;
    } else if (m.version == "4.8.2") || (c_strncmp_eq(&m.version, "4.82", 4)) {
        m.v48intVersion = 482;
    } else if (m.version == "4.8.1") || (c_strncmp_eq(&m.version, "4.81", 4)) {
        m.v48intVersion = 481;
    } else if (m.version == "4.8.0") || (c_strncmp_eq(&m.version, "4.80", 4)) {
        m.v48intVersion = 480;
    } else {
        warnings.push("Warning: unknown BSIM4 version. Working now with BSIM4.8.3.".into());
        m.v48intVersion = 483;
    }
    if !m.toxrefGiven {
        m.toxref = 30.0e-10;
    }
    if !m.eotGiven {
        m.eot = 15.0e-10;
    }
    if !m.vddeotGiven {
        m.vddeot = if m.r#type == 1 { 1.5 } else { -1.5 };
    }
    if !m.tempeotGiven {
        m.tempeot = 300.15;
    }
    if !m.leffeotGiven {
        m.leffeot = 1.0;
    }
    if !m.weffeotGiven {
        m.weffeot = 10.0;
    }
    if !m.adosGiven {
        m.ados = 1.0;
    }
    if !m.bdosGiven {
        m.bdos = 1.0;
    }
    if !m.toxeGiven {
        m.toxe = 30.0e-10;
    }
    if !m.toxpGiven {
        m.toxp = m.toxe;
    }
    if !m.toxmGiven {
        m.toxm = m.toxe;
    }
    if !m.dtoxGiven {
        m.dtox = 0.0;
    }
    if !m.epsroxGiven {
        m.epsrox = 3.9;
    }
    if !m.cdscGiven {
        m.cdsc = 2.4e-4;
    }
    if !m.cdscbGiven {
        m.cdscb = 0.0;
    }
    if !m.cdscdGiven {
        m.cdscd = 0.0;
    }
    if !m.citGiven {
        m.cit = 0.0;
    }
    if !m.nfactorGiven {
        m.nfactor = 1.0;
    }
    if !m.xjGiven {
        m.xj = 0.15e-6;
    }
    if !m.vsatGiven {
        m.vsat = 8.0e4;
    }
    if !m.atGiven {
        m.at = 3.3e4;
    }
    if !m.a0Given {
        m.a0 = 1.0;
    }
    if !m.agsGiven {
        m.ags = 0.0;
    }
    if !m.a1Given {
        m.a1 = 0.0;
    }
    if !m.a2Given {
        m.a2 = 1.0;
    }
    if !m.ketaGiven {
        m.keta = -0.047;
    }
    if !m.ketacGiven {
        m.ketac = m.keta;
    }
    if !m.nsubGiven {
        m.nsub = 6.0e16;
    }
    if !m.phigGiven {
        m.phig = 4.05;
    }
    if !m.epsrgateGiven {
        m.epsrgate = 11.7;
    }
    if !m.easubGiven {
        m.easub = 4.05;
    }
    if !m.epsrsubGiven {
        m.epsrsub = 11.7;
    }
    if !m.ni0subGiven {
        m.ni0sub = 1.45e10;
    }
    if !m.bg0subGiven {
        m.bg0sub = 1.16;
    }
    if !m.tbgasubGiven {
        m.tbgasub = 7.02e-4;
    }
    if !m.tbgbsubGiven {
        m.tbgbsub = 1108.0;
    }
    if !m.ndepGiven {
        m.ndep = 1.7e17;
    }
    if !m.nsdGiven {
        m.nsd = 1.0e20;
    }
    if !m.phinGiven {
        m.phin = 0.0;
    }
    if !m.ngateGiven {
        m.ngate = 0.0;
    }
    if !m.vbmGiven {
        m.vbm = -3.0;
    }
    if !m.xtGiven {
        m.xt = 1.55e-7;
    }
    if !m.kt1Given {
        m.kt1 = -0.11;
    }
    if !m.kt1lGiven {
        m.kt1l = 0.0;
    }
    if !m.kt2Given {
        m.kt2 = 0.022;
    }
    if !m.k3Given {
        m.k3 = 80.0;
    }
    if !m.k3bGiven {
        m.k3b = 0.0;
    }
    if !m.w0Given {
        m.w0 = 2.5e-6;
    }
    if !m.lpe0Given {
        m.lpe0 = 1.74e-7;
    }
    if !m.lpebGiven {
        m.lpeb = 0.0;
    }
    if !m.dvtp0Given {
        m.dvtp0 = 0.0;
    }
    if !m.dvtp1Given {
        m.dvtp1 = 0.0;
    }
    if !m.dvtp2Given {
        m.dvtp2 = 0.0;
    }
    if !m.dvtp3Given {
        m.dvtp3 = 0.0;
    }
    if !m.dvtp4Given {
        m.dvtp4 = 0.0;
    }
    if !m.dvtp5Given {
        m.dvtp5 = 0.0;
    }
    if !m.dvt0Given {
        m.dvt0 = 2.2;
    }
    if !m.dvt1Given {
        m.dvt1 = 0.53;
    }
    if !m.dvt2Given {
        m.dvt2 = -0.032;
    }
    if !m.dvt0wGiven {
        m.dvt0w = 0.0;
    }
    if !m.dvt1wGiven {
        m.dvt1w = 5.3e6;
    }
    if !m.dvt2wGiven {
        m.dvt2w = -0.032;
    }
    if !m.droutGiven {
        m.drout = 0.56;
    }
    if !m.dsubGiven {
        m.dsub = m.drout;
    }
    if !m.vth0Given {
        m.vth0 = if m.r#type == 1 { 0.7 } else { -0.7 };
    }
    if !m.vfbGiven {
        m.vfb = -1.0;
    }
    if !m.euGiven {
        m.eu = if m.r#type == 1 { 1.67 } else { 1.0 };
    }
    if !m.ucsGiven {
        m.ucs = if m.r#type == 1 { 1.67 } else { 1.0 };
    }
    if m.v48intVersion <= 480 {
        if !m.uaGiven {
            m.ua = if m.mobMod == 2 { 1.0e-15 } else { 1.0e-9 };
        }
        if !m.ucGiven {
            m.uc = if m.mobMod == 1 { -0.0465 } else { -0.0465e-9 };
        }
        if !m.uc1Given {
            m.uc1 = if m.mobMod == 1 { -0.056 } else { -0.056e-9 };
        }
    } else {
        if !m.uaGiven {
            m.ua = if m.mobMod == 2 || m.mobMod == 6 {
                1.0e-15
            } else {
                1.0e-9
            };
        }
        if !m.ucGiven {
            m.uc = if m.mobMod == 1 || m.mobMod == 5 {
                -0.0465
            } else {
                -0.0465e-9
            };
        }
        if !m.uc1Given {
            m.uc1 = if m.mobMod == 1 || m.mobMod == 5 {
                -0.056
            } else {
                -0.056e-9
            };
        }
    }
    if !m.ua1Given {
        m.ua1 = 1.0e-9;
    }
    if !m.ubGiven {
        m.ub = 1.0e-19;
    }
    if !m.ub1Given {
        m.ub1 = -1.0e-18;
    }
    if !m.udGiven {
        m.ud = 0.0;
    }
    if !m.ud1Given {
        m.ud1 = 0.0;
    }
    if !m.upGiven {
        m.up = 0.0;
    }
    if !m.lpGiven {
        m.lp = 1.0e-8;
    }
    if !m.u0Given {
        m.u0 = if m.r#type == 1 { 0.067 } else { 0.025 };
    }
    if !m.uteGiven {
        m.ute = -1.5;
    }
    if !m.ucsteGiven {
        m.ucste = -4.775e-3;
    }
    if !m.voffGiven {
        m.voff = -0.08;
    }
    if !m.vofflGiven {
        m.voffl = 0.0;
    }
    if !m.voffcvlGiven {
        m.voffcvl = 0.0;
    }
    if !m.minvGiven {
        m.minv = 0.0;
    }
    if !m.minvcvGiven {
        m.minvcv = 0.0;
    }
    if !m.fproutGiven {
        m.fprout = 0.0;
    }
    if !m.pditsGiven {
        m.pdits = 0.0;
    }
    if !m.pditsdGiven {
        m.pditsd = 0.0;
    }
    if !m.pditslGiven {
        m.pditsl = 0.0;
    }
    if !m.deltaGiven {
        m.delta = 0.01;
    }
    if !m.rdswminGiven {
        m.rdswmin = 0.0;
    }
    if !m.rdwminGiven {
        m.rdwmin = 0.0;
    }
    if !m.rswminGiven {
        m.rswmin = 0.0;
    }
    if !m.rdswGiven {
        m.rdsw = 200.0;
    }
    if !m.rdwGiven {
        m.rdw = 100.0;
    }
    if !m.rswGiven {
        m.rsw = 100.0;
    }
    if !m.prwgGiven {
        m.prwg = 1.0;
    }
    if !m.prwbGiven {
        m.prwb = 0.0;
    }
    if !m.prtGiven {
        m.prt = 0.0;
    }
    if !m.eta0Given {
        m.eta0 = 0.08;
    }
    if !m.etabGiven {
        m.etab = -0.07;
    }
    if !m.pclmGiven {
        m.pclm = 1.3;
    }
    if !m.pdibl1Given {
        m.pdibl1 = 0.39;
    }
    if !m.pdibl2Given {
        m.pdibl2 = 0.0086;
    }
    if !m.pdiblbGiven {
        m.pdiblb = 0.0;
    }
    if !m.pscbe1Given {
        m.pscbe1 = 4.24e8;
    }
    if !m.pscbe2Given {
        m.pscbe2 = 1.0e-5;
    }
    if !m.pvagGiven {
        m.pvag = 0.0;
    }
    if !m.wrGiven {
        m.wr = 1.0;
    }
    if !m.dwgGiven {
        m.dwg = 0.0;
    }
    if !m.dwbGiven {
        m.dwb = 0.0;
    }
    if !m.b0Given {
        m.b0 = 0.0;
    }
    if !m.b1Given {
        m.b1 = 0.0;
    }
    if !m.alpha0Given {
        m.alpha0 = 0.0;
    }
    if !m.alpha1Given {
        m.alpha1 = 0.0;
    }
    if !m.beta0Given {
        m.beta0 = 0.0;
    }
    if !m.gidlModGiven {
        m.gidlMod = 0;
    }
    if !m.agidlGiven {
        m.agidl = 0.0;
    }
    if !m.bgidlGiven {
        m.bgidl = 2.3e9;
    }
    if !m.cgidlGiven {
        m.cgidl = 0.5;
    }
    if !m.egidlGiven {
        m.egidl = 0.8;
    }
    if !m.rgidlGiven {
        m.rgidl = 1.0;
    }
    if !m.kgidlGiven {
        m.kgidl = 0.0;
    }
    if !m.fgidlGiven {
        m.fgidl = 1.0;
    }
    if !m.agislGiven {
        m.agisl = m.agidl;
    }
    if !m.bgislGiven {
        m.bgisl = m.bgidl;
    }
    if !m.cgislGiven {
        m.cgisl = m.cgidl;
    }
    if !m.egislGiven {
        m.egisl = m.egidl;
    }
    if !m.rgislGiven {
        m.rgisl = m.rgidl;
    }
    if !m.kgislGiven {
        m.kgisl = m.kgidl;
    }
    if !m.fgislGiven {
        m.fgisl = m.fgidl;
    }
    if !m.aigcGiven {
        m.aigc = if m.r#type == 1 { 1.36e-2 } else { 9.80e-3 };
    }
    if !m.bigcGiven {
        m.bigc = if m.r#type == 1 { 1.71e-3 } else { 7.59e-4 };
    }
    if !m.cigcGiven {
        m.cigc = if m.r#type == 1 { 0.075 } else { 0.03 };
    }
    if m.aigsdGiven {
        m.aigd = m.aigsd;
        m.aigs = m.aigd;
    } else {
        m.aigsd = if m.r#type == 1 { 1.36e-2 } else { 9.80e-3 };
        if !m.aigsGiven {
            m.aigs = if m.r#type == 1 { 1.36e-2 } else { 9.80e-3 };
        }
        if !m.aigdGiven {
            m.aigd = if m.r#type == 1 { 1.36e-2 } else { 9.80e-3 };
        }
    }
    if m.bigsdGiven {
        m.bigd = m.bigsd;
        m.bigs = m.bigd;
    } else {
        m.bigsd = if m.r#type == 1 { 1.71e-3 } else { 7.59e-4 };
        if !m.bigsGiven {
            m.bigs = if m.r#type == 1 { 1.71e-3 } else { 7.59e-4 };
        }
        if !m.bigdGiven {
            m.bigd = if m.r#type == 1 { 1.71e-3 } else { 7.59e-4 };
        }
    }
    if m.cigsdGiven {
        m.cigd = m.cigsd;
        m.cigs = m.cigd;
    } else {
        m.cigsd = if m.r#type == 1 { 0.075 } else { 0.03 };
        if !m.cigsGiven {
            m.cigs = if m.r#type == 1 { 0.075 } else { 0.03 };
        }
        if !m.cigdGiven {
            m.cigd = if m.r#type == 1 { 0.075 } else { 0.03 };
        }
    }
    if !m.aigbaccGiven {
        m.aigbacc = 1.36e-2;
    }
    if !m.bigbaccGiven {
        m.bigbacc = 1.71e-3;
    }
    if !m.cigbaccGiven {
        m.cigbacc = 0.075;
    }
    if !m.aigbinvGiven {
        m.aigbinv = 1.11e-2;
    }
    if !m.bigbinvGiven {
        m.bigbinv = 9.49e-4;
    }
    if !m.cigbinvGiven {
        m.cigbinv = 0.006;
    }
    if !m.nigcGiven {
        m.nigc = 1.0;
    }
    if !m.nigbinvGiven {
        m.nigbinv = 3.0;
    }
    if !m.nigbaccGiven {
        m.nigbacc = 1.0;
    }
    if !m.ntoxGiven {
        m.ntox = 1.0;
    }
    if !m.eigbinvGiven {
        m.eigbinv = 1.1;
    }
    if !m.pigcdGiven {
        m.pigcd = 1.0;
    }
    if !m.poxedgeGiven {
        m.poxedge = 1.0;
    }
    if !m.xrcrg1Given {
        m.xrcrg1 = 12.0;
    }
    if !m.xrcrg2Given {
        m.xrcrg2 = 1.0;
    }
    if !m.ijthsfwdGiven {
        m.ijthsfwd = 0.1;
    }
    if !m.ijthdfwdGiven {
        m.ijthdfwd = m.ijthsfwd;
    }
    if !m.ijthsrevGiven {
        m.ijthsrev = 0.1;
    }
    if !m.ijthdrevGiven {
        m.ijthdrev = m.ijthsrev;
    }
    if !m.tnoiaGiven {
        m.tnoia = 1.5;
    }
    if !m.tnoibGiven {
        m.tnoib = 3.5;
    }
    if !m.tnoicGiven {
        m.tnoic = 0.0;
    }
    if !m.rnoiaGiven {
        m.rnoia = 0.577;
    }
    if !m.rnoibGiven {
        m.rnoib = 0.5164;
    }
    if !m.gidlclampGiven {
        m.gidlclamp = -1e-5;
    }
    if !m.idovvdscGiven {
        m.idovvdsc = 1e-9;
    }
    if !m.rnoicGiven {
        m.rnoic = 0.395;
    }
    if !m.ntnoiGiven {
        m.ntnoi = 1.0;
    }
    if !m.lambdaGiven {
        m.lambda = 0.0;
    }
    if !m.vtlGiven {
        m.vtl = 2.0e5;
    }
    if !m.xnGiven {
        m.xn = 3.0;
    }
    if !m.lcGiven {
        m.lc = 5.0e-9;
    }
    if !m.vfbsdoffGiven {
        m.vfbsdoff = 0.0;
    }
    if !m.tvfbsdoffGiven {
        m.tvfbsdoff = 0.0;
    }
    if !m.tvoffGiven {
        m.tvoff = 0.0;
    }
    if !m.tnfactorGiven {
        m.tnfactor = 0.0;
    }
    if !m.teta0Given {
        m.teta0 = 0.0;
    }
    if !m.tvoffcvGiven {
        m.tvoffcv = 0.0;
    }
    if !m.lintnoiGiven {
        m.lintnoi = 0.0;
    }
    if !m.xjbvsGiven {
        m.xjbvs = 1.0;
    }
    if !m.xjbvdGiven {
        m.xjbvd = m.xjbvs;
    }
    if !m.bvsGiven {
        m.bvs = 10.0;
    }
    if !m.bvdGiven {
        m.bvd = m.bvs;
    }
    if !m.gbminGiven {
        m.gbmin = 1.0e-12;
    }
    if !m.rbdbGiven {
        m.rbdb = 50.0;
    }
    if !m.rbpbGiven {
        m.rbpb = 50.0;
    }
    if !m.rbsbGiven {
        m.rbsb = 50.0;
    }
    if !m.rbpsGiven {
        m.rbps = 50.0;
    }
    if !m.rbpdGiven {
        m.rbpd = 50.0;
    }
    if !m.rbps0Given {
        m.rbps0 = 50.0;
    }
    if !m.rbpslGiven {
        m.rbpsl = 0.0;
    }
    if !m.rbpswGiven {
        m.rbpsw = 0.0;
    }
    if !m.rbpsnfGiven {
        m.rbpsnf = 0.0;
    }
    if !m.rbpd0Given {
        m.rbpd0 = 50.0;
    }
    if !m.rbpdlGiven {
        m.rbpdl = 0.0;
    }
    if !m.rbpdwGiven {
        m.rbpdw = 0.0;
    }
    if !m.rbpdnfGiven {
        m.rbpdnf = 0.0;
    }
    if !m.rbpbx0Given {
        m.rbpbx0 = 100.0;
    }
    if !m.rbpbxlGiven {
        m.rbpbxl = 0.0;
    }
    if !m.rbpbxwGiven {
        m.rbpbxw = 0.0;
    }
    if !m.rbpbxnfGiven {
        m.rbpbxnf = 0.0;
    }
    if !m.rbpby0Given {
        m.rbpby0 = 100.0;
    }
    if !m.rbpbylGiven {
        m.rbpbyl = 0.0;
    }
    if !m.rbpbywGiven {
        m.rbpbyw = 0.0;
    }
    if !m.rbpbynfGiven {
        m.rbpbynf = 0.0;
    }
    if !m.rbsbx0Given {
        m.rbsbx0 = 100.0;
    }
    if !m.rbsby0Given {
        m.rbsby0 = 100.0;
    }
    if !m.rbdbx0Given {
        m.rbdbx0 = 100.0;
    }
    if !m.rbdby0Given {
        m.rbdby0 = 100.0;
    }
    if !m.rbsdbxlGiven {
        m.rbsdbxl = 0.0;
    }
    if !m.rbsdbxwGiven {
        m.rbsdbxw = 0.0;
    }
    if !m.rbsdbxnfGiven {
        m.rbsdbxnf = 0.0;
    }
    if !m.rbsdbylGiven {
        m.rbsdbyl = 0.0;
    }
    if !m.rbsdbywGiven {
        m.rbsdbyw = 0.0;
    }
    if !m.rbsdbynfGiven {
        m.rbsdbynf = 0.0;
    }
    if !m.cgslGiven {
        m.cgsl = 0.0;
    }
    if !m.cgdlGiven {
        m.cgdl = 0.0;
    }
    if !m.ckappasGiven {
        m.ckappas = 0.6;
    }
    if !m.ckappadGiven {
        m.ckappad = m.ckappas;
    }
    if !m.clcGiven {
        m.clc = 0.1e-6;
    }
    if !m.cleGiven {
        m.cle = 0.6;
    }
    if !m.vfbcvGiven {
        m.vfbcv = -1.0;
    }
    if !m.acdeGiven {
        m.acde = 1.0;
    }
    if !m.moinGiven {
        m.moin = 15.0;
    }
    if !m.noffGiven {
        m.noff = 1.0;
    }
    if !m.voffcvGiven {
        m.voffcv = 0.0;
    }
    if !m.dmcgGiven {
        m.dmcg = 0.0;
    }
    if !m.dmciGiven {
        m.dmci = m.dmcg;
    }
    if !m.dmdgGiven {
        m.dmdg = 0.0;
    }
    if !m.dmcgtGiven {
        m.dmcgt = 0.0;
    }
    if !m.xgwGiven {
        m.xgw = 0.0;
    }
    if !m.xglGiven {
        m.xgl = 0.0;
    }
    if !m.rshgGiven {
        m.rshg = 0.1;
    }
    if !m.ngconGiven {
        m.ngcon = 1.0;
    }
    if !m.tcjGiven {
        m.tcj = 0.0;
    }
    if !m.tpbGiven {
        m.tpb = 0.0;
    }
    if !m.tcjswGiven {
        m.tcjsw = 0.0;
    }
    if !m.tpbswGiven {
        m.tpbsw = 0.0;
    }
    if !m.tcjswgGiven {
        m.tcjswg = 0.0;
    }
    if !m.tpbswgGiven {
        m.tpbswg = 0.0;
    }
    if !m.lcdscGiven {
        m.lcdsc = 0.0;
    }
    if !m.lcdscbGiven {
        m.lcdscb = 0.0;
    }
    if !m.lcdscdGiven {
        m.lcdscd = 0.0;
    }
    if !m.lcitGiven {
        m.lcit = 0.0;
    }
    if !m.lnfactorGiven {
        m.lnfactor = 0.0;
    }
    if !m.lxjGiven {
        m.lxj = 0.0;
    }
    if !m.lvsatGiven {
        m.lvsat = 0.0;
    }
    if !m.latGiven {
        m.lat = 0.0;
    }
    if !m.la0Given {
        m.la0 = 0.0;
    }
    if !m.lagsGiven {
        m.lags = 0.0;
    }
    if !m.la1Given {
        m.la1 = 0.0;
    }
    if !m.la2Given {
        m.la2 = 0.0;
    }
    if !m.lketaGiven {
        m.lketa = 0.0;
    }
    if !m.lketacGiven {
        m.lketac = m.lketa;
    }
    if !m.lnsubGiven {
        m.lnsub = 0.0;
    }
    if !m.lndepGiven {
        m.lndep = 0.0;
    }
    if !m.lnsdGiven {
        m.lnsd = 0.0;
    }
    if !m.lphinGiven {
        m.lphin = 0.0;
    }
    if !m.lngateGiven {
        m.lngate = 0.0;
    }
    if !m.lvbmGiven {
        m.lvbm = 0.0;
    }
    if !m.lxtGiven {
        m.lxt = 0.0;
    }
    if !m.lk1Given {
        m.lk1 = 0.0;
    }
    if !m.lkt1Given {
        m.lkt1 = 0.0;
    }
    if !m.lkt1lGiven {
        m.lkt1l = 0.0;
    }
    if !m.lkt2Given {
        m.lkt2 = 0.0;
    }
    if !m.lk2Given {
        m.lk2 = 0.0;
    }
    if !m.lk3Given {
        m.lk3 = 0.0;
    }
    if !m.lk3bGiven {
        m.lk3b = 0.0;
    }
    if !m.lw0Given {
        m.lw0 = 0.0;
    }
    if !m.llpe0Given {
        m.llpe0 = 0.0;
    }
    if !m.llpebGiven {
        m.llpeb = 0.0;
    }
    if !m.ldvtp0Given {
        m.ldvtp0 = 0.0;
    }
    if !m.ldvtp1Given {
        m.ldvtp1 = 0.0;
    }
    if !m.ldvtp2Given {
        m.ldvtp2 = 0.0;
    }
    if !m.ldvtp3Given {
        m.ldvtp3 = 0.0;
    }
    if !m.ldvtp4Given {
        m.ldvtp4 = 0.0;
    }
    if !m.ldvtp5Given {
        m.ldvtp5 = 0.0;
    }
    if !m.ldvt0Given {
        m.ldvt0 = 0.0;
    }
    if !m.ldvt1Given {
        m.ldvt1 = 0.0;
    }
    if !m.ldvt2Given {
        m.ldvt2 = 0.0;
    }
    if !m.ldvt0wGiven {
        m.ldvt0w = 0.0;
    }
    if !m.ldvt1wGiven {
        m.ldvt1w = 0.0;
    }
    if !m.ldvt2wGiven {
        m.ldvt2w = 0.0;
    }
    if !m.ldroutGiven {
        m.ldrout = 0.0;
    }
    if !m.ldsubGiven {
        m.ldsub = 0.0;
    }
    if !m.lvth0Given {
        m.lvth0 = 0.0;
    }
    if !m.luaGiven {
        m.lua = 0.0;
    }
    if !m.lua1Given {
        m.lua1 = 0.0;
    }
    if !m.lubGiven {
        m.lub = 0.0;
    }
    if !m.lub1Given {
        m.lub1 = 0.0;
    }
    if !m.lucGiven {
        m.luc = 0.0;
    }
    if !m.luc1Given {
        m.luc1 = 0.0;
    }
    if !m.ludGiven {
        m.lud = 0.0;
    }
    if !m.lud1Given {
        m.lud1 = 0.0;
    }
    if !m.lupGiven {
        m.lup = 0.0;
    }
    if !m.llpGiven {
        m.llp = 0.0;
    }
    if !m.lu0Given {
        m.lu0 = 0.0;
    }
    if !m.luteGiven {
        m.lute = 0.0;
    }
    if !m.lucsteGiven {
        m.lucste = 0.0;
    }
    if !m.lvoffGiven {
        m.lvoff = 0.0;
    }
    if !m.lminvGiven {
        m.lminv = 0.0;
    }
    if !m.lminvcvGiven {
        m.lminvcv = 0.0;
    }
    if !m.lfproutGiven {
        m.lfprout = 0.0;
    }
    if !m.lpditsGiven {
        m.lpdits = 0.0;
    }
    if !m.lpditsdGiven {
        m.lpditsd = 0.0;
    }
    if !m.ldeltaGiven {
        m.ldelta = 0.0;
    }
    if !m.lrdswGiven {
        m.lrdsw = 0.0;
    }
    if !m.lrdwGiven {
        m.lrdw = 0.0;
    }
    if !m.lrswGiven {
        m.lrsw = 0.0;
    }
    if !m.lprwbGiven {
        m.lprwb = 0.0;
    }
    if !m.lprwgGiven {
        m.lprwg = 0.0;
    }
    if !m.lprtGiven {
        m.lprt = 0.0;
    }
    if !m.leta0Given {
        m.leta0 = 0.0;
    }
    if !m.letabGiven {
        m.letab = -0.0;
    }
    if !m.lpclmGiven {
        m.lpclm = 0.0;
    }
    if !m.lpdibl1Given {
        m.lpdibl1 = 0.0;
    }
    if !m.lpdibl2Given {
        m.lpdibl2 = 0.0;
    }
    if !m.lpdiblbGiven {
        m.lpdiblb = 0.0;
    }
    if !m.lpscbe1Given {
        m.lpscbe1 = 0.0;
    }
    if !m.lpscbe2Given {
        m.lpscbe2 = 0.0;
    }
    if !m.lpvagGiven {
        m.lpvag = 0.0;
    }
    if !m.lwrGiven {
        m.lwr = 0.0;
    }
    if !m.ldwgGiven {
        m.ldwg = 0.0;
    }
    if !m.ldwbGiven {
        m.ldwb = 0.0;
    }
    if !m.lb0Given {
        m.lb0 = 0.0;
    }
    if !m.lb1Given {
        m.lb1 = 0.0;
    }
    if !m.lalpha0Given {
        m.lalpha0 = 0.0;
    }
    if !m.lalpha1Given {
        m.lalpha1 = 0.0;
    }
    if !m.lbeta0Given {
        m.lbeta0 = 0.0;
    }
    if !m.lagidlGiven {
        m.lagidl = 0.0;
    }
    if !m.lbgidlGiven {
        m.lbgidl = 0.0;
    }
    if !m.lcgidlGiven {
        m.lcgidl = 0.0;
    }
    if !m.legidlGiven {
        m.legidl = 0.0;
    }
    if !m.lrgidlGiven {
        m.lrgidl = 0.0;
    }
    if !m.lkgidlGiven {
        m.lkgidl = 0.0;
    }
    if !m.lfgidlGiven {
        m.lfgidl = 0.0;
    }
    if !m.lagislGiven {
        m.lagisl = m.lagidl;
    }
    if !m.lbgislGiven {
        m.lbgisl = m.lbgidl;
    }
    if !m.lcgislGiven {
        m.lcgisl = m.lcgidl;
    }
    if !m.legislGiven {
        m.legisl = m.legidl;
    }
    if !m.lrgislGiven {
        m.lrgisl = m.lrgidl;
    }
    if !m.lkgislGiven {
        m.lkgisl = m.lkgidl;
    }
    if !m.lfgislGiven {
        m.lfgisl = m.lfgidl;
    }
    if !m.laigcGiven {
        m.laigc = 0.0;
    }
    if !m.lbigcGiven {
        m.lbigc = 0.0;
    }
    if !m.lcigcGiven {
        m.lcigc = 0.0;
    }
    if !m.aigsdGiven && (m.aigsGiven || m.aigdGiven) {
        if !m.laigsGiven {
            m.laigs = 0.0;
        }
        if !m.laigdGiven {
            m.laigd = 0.0;
        }
    } else {
        if !m.laigsdGiven {
            m.laigsd = 0.0;
        }
        m.laigd = m.laigsd;
        m.laigs = m.laigd;
    }
    if !m.bigsdGiven && (m.bigsGiven || m.bigdGiven) {
        if !m.lbigsGiven {
            m.lbigs = 0.0;
        }
        if !m.lbigdGiven {
            m.lbigd = 0.0;
        }
    } else {
        if !m.lbigsdGiven {
            m.lbigsd = 0.0;
        }
        m.lbigd = m.lbigsd;
        m.lbigs = m.lbigd;
    }
    if !m.cigsdGiven && (m.cigsGiven || m.cigdGiven) {
        if !m.lcigsGiven {
            m.lcigs = 0.0;
        }
        if !m.lcigdGiven {
            m.lcigd = 0.0;
        }
    } else {
        if !m.lcigsdGiven {
            m.lcigsd = 0.0;
        }
        m.lcigd = m.lcigsd;
        m.lcigs = m.lcigd;
    }
    if !m.laigbaccGiven {
        m.laigbacc = 0.0;
    }
    if !m.lbigbaccGiven {
        m.lbigbacc = 0.0;
    }
    if !m.lcigbaccGiven {
        m.lcigbacc = 0.0;
    }
    if !m.laigbinvGiven {
        m.laigbinv = 0.0;
    }
    if !m.lbigbinvGiven {
        m.lbigbinv = 0.0;
    }
    if !m.lcigbinvGiven {
        m.lcigbinv = 0.0;
    }
    if !m.lnigcGiven {
        m.lnigc = 0.0;
    }
    if !m.lnigbinvGiven {
        m.lnigbinv = 0.0;
    }
    if !m.lnigbaccGiven {
        m.lnigbacc = 0.0;
    }
    if !m.lntoxGiven {
        m.lntox = 0.0;
    }
    if !m.leigbinvGiven {
        m.leigbinv = 0.0;
    }
    if !m.lpigcdGiven {
        m.lpigcd = 0.0;
    }
    if !m.lpoxedgeGiven {
        m.lpoxedge = 0.0;
    }
    if !m.lxrcrg1Given {
        m.lxrcrg1 = 0.0;
    }
    if !m.lxrcrg2Given {
        m.lxrcrg2 = 0.0;
    }
    if !m.leuGiven {
        m.leu = 0.0;
    }
    if !m.lucsGiven {
        m.lucs = 0.0;
    }
    if !m.lvfbGiven {
        m.lvfb = 0.0;
    }
    if !m.llambdaGiven {
        m.llambda = 0.0;
    }
    if !m.lvtlGiven {
        m.lvtl = 0.0;
    }
    if !m.lxnGiven {
        m.lxn = 0.0;
    }
    if !m.lvfbsdoffGiven {
        m.lvfbsdoff = 0.0;
    }
    if !m.ltvfbsdoffGiven {
        m.ltvfbsdoff = 0.0;
    }
    if !m.ltvoffGiven {
        m.ltvoff = 0.0;
    }
    if !m.ltnfactorGiven {
        m.ltnfactor = 0.0;
    }
    if !m.lteta0Given {
        m.lteta0 = 0.0;
    }
    if !m.ltvoffcvGiven {
        m.ltvoffcv = 0.0;
    }
    if !m.lcgslGiven {
        m.lcgsl = 0.0;
    }
    if !m.lcgdlGiven {
        m.lcgdl = 0.0;
    }
    if !m.lckappasGiven {
        m.lckappas = 0.0;
    }
    if !m.lckappadGiven {
        m.lckappad = 0.0;
    }
    if !m.lclcGiven {
        m.lclc = 0.0;
    }
    if !m.lcleGiven {
        m.lcle = 0.0;
    }
    if !m.lcfGiven {
        m.lcf = 0.0;
    }
    if !m.lvfbcvGiven {
        m.lvfbcv = 0.0;
    }
    if !m.lacdeGiven {
        m.lacde = 0.0;
    }
    if !m.lmoinGiven {
        m.lmoin = 0.0;
    }
    if !m.lnoffGiven {
        m.lnoff = 0.0;
    }
    if !m.lvoffcvGiven {
        m.lvoffcv = 0.0;
    }
    if !m.wcdscGiven {
        m.wcdsc = 0.0;
    }
    if !m.wcdscbGiven {
        m.wcdscb = 0.0;
    }
    if !m.wcdscdGiven {
        m.wcdscd = 0.0;
    }
    if !m.wcitGiven {
        m.wcit = 0.0;
    }
    if !m.wnfactorGiven {
        m.wnfactor = 0.0;
    }
    if !m.wxjGiven {
        m.wxj = 0.0;
    }
    if !m.wvsatGiven {
        m.wvsat = 0.0;
    }
    if !m.watGiven {
        m.wat = 0.0;
    }
    if !m.wa0Given {
        m.wa0 = 0.0;
    }
    if !m.wagsGiven {
        m.wags = 0.0;
    }
    if !m.wa1Given {
        m.wa1 = 0.0;
    }
    if !m.wa2Given {
        m.wa2 = 0.0;
    }
    if !m.wketaGiven {
        m.wketa = 0.0;
    }
    if !m.wketacGiven {
        m.wketac = m.wketa;
    }
    if !m.wnsubGiven {
        m.wnsub = 0.0;
    }
    if !m.wndepGiven {
        m.wndep = 0.0;
    }
    if !m.wnsdGiven {
        m.wnsd = 0.0;
    }
    if !m.wphinGiven {
        m.wphin = 0.0;
    }
    if !m.wngateGiven {
        m.wngate = 0.0;
    }
    if !m.wvbmGiven {
        m.wvbm = 0.0;
    }
    if !m.wxtGiven {
        m.wxt = 0.0;
    }
    if !m.wk1Given {
        m.wk1 = 0.0;
    }
    if !m.wkt1Given {
        m.wkt1 = 0.0;
    }
    if !m.wkt1lGiven {
        m.wkt1l = 0.0;
    }
    if !m.wkt2Given {
        m.wkt2 = 0.0;
    }
    if !m.wk2Given {
        m.wk2 = 0.0;
    }
    if !m.wk3Given {
        m.wk3 = 0.0;
    }
    if !m.wk3bGiven {
        m.wk3b = 0.0;
    }
    if !m.ww0Given {
        m.ww0 = 0.0;
    }
    if !m.wlpe0Given {
        m.wlpe0 = 0.0;
    }
    if !m.wlpebGiven {
        m.wlpeb = 0.0;
    }
    if !m.wdvtp0Given {
        m.wdvtp0 = 0.0;
    }
    if !m.wdvtp1Given {
        m.wdvtp1 = 0.0;
    }
    if !m.wdvtp2Given {
        m.wdvtp2 = 0.0;
    }
    if !m.wdvtp3Given {
        m.wdvtp3 = 0.0;
    }
    if !m.wdvtp4Given {
        m.wdvtp4 = 0.0;
    }
    if !m.wdvtp5Given {
        m.wdvtp5 = 0.0;
    }
    if !m.wdvt0Given {
        m.wdvt0 = 0.0;
    }
    if !m.wdvt1Given {
        m.wdvt1 = 0.0;
    }
    if !m.wdvt2Given {
        m.wdvt2 = 0.0;
    }
    if !m.wdvt0wGiven {
        m.wdvt0w = 0.0;
    }
    if !m.wdvt1wGiven {
        m.wdvt1w = 0.0;
    }
    if !m.wdvt2wGiven {
        m.wdvt2w = 0.0;
    }
    if !m.wdroutGiven {
        m.wdrout = 0.0;
    }
    if !m.wdsubGiven {
        m.wdsub = 0.0;
    }
    if !m.wvth0Given {
        m.wvth0 = 0.0;
    }
    if !m.wuaGiven {
        m.wua = 0.0;
    }
    if !m.wua1Given {
        m.wua1 = 0.0;
    }
    if !m.wubGiven {
        m.wub = 0.0;
    }
    if !m.wub1Given {
        m.wub1 = 0.0;
    }
    if !m.wucGiven {
        m.wuc = 0.0;
    }
    if !m.wuc1Given {
        m.wuc1 = 0.0;
    }
    if !m.wudGiven {
        m.wud = 0.0;
    }
    if !m.wud1Given {
        m.wud1 = 0.0;
    }
    if !m.wupGiven {
        m.wup = 0.0;
    }
    if !m.wlpGiven {
        m.wlp = 0.0;
    }
    if !m.wu0Given {
        m.wu0 = 0.0;
    }
    if !m.wuteGiven {
        m.wute = 0.0;
    }
    if !m.wucsteGiven {
        m.wucste = 0.0;
    }
    if !m.wvoffGiven {
        m.wvoff = 0.0;
    }
    if !m.wminvGiven {
        m.wminv = 0.0;
    }
    if !m.wminvcvGiven {
        m.wminvcv = 0.0;
    }
    if !m.wfproutGiven {
        m.wfprout = 0.0;
    }
    if !m.wpditsGiven {
        m.wpdits = 0.0;
    }
    if !m.wpditsdGiven {
        m.wpditsd = 0.0;
    }
    if !m.wdeltaGiven {
        m.wdelta = 0.0;
    }
    if !m.wrdswGiven {
        m.wrdsw = 0.0;
    }
    if !m.wrdwGiven {
        m.wrdw = 0.0;
    }
    if !m.wrswGiven {
        m.wrsw = 0.0;
    }
    if !m.wprwbGiven {
        m.wprwb = 0.0;
    }
    if !m.wprwgGiven {
        m.wprwg = 0.0;
    }
    if !m.wprtGiven {
        m.wprt = 0.0;
    }
    if !m.weta0Given {
        m.weta0 = 0.0;
    }
    if !m.wetabGiven {
        m.wetab = 0.0;
    }
    if !m.wpclmGiven {
        m.wpclm = 0.0;
    }
    if !m.wpdibl1Given {
        m.wpdibl1 = 0.0;
    }
    if !m.wpdibl2Given {
        m.wpdibl2 = 0.0;
    }
    if !m.wpdiblbGiven {
        m.wpdiblb = 0.0;
    }
    if !m.wpscbe1Given {
        m.wpscbe1 = 0.0;
    }
    if !m.wpscbe2Given {
        m.wpscbe2 = 0.0;
    }
    if !m.wpvagGiven {
        m.wpvag = 0.0;
    }
    if !m.wwrGiven {
        m.wwr = 0.0;
    }
    if !m.wdwgGiven {
        m.wdwg = 0.0;
    }
    if !m.wdwbGiven {
        m.wdwb = 0.0;
    }
    if !m.wb0Given {
        m.wb0 = 0.0;
    }
    if !m.wb1Given {
        m.wb1 = 0.0;
    }
    if !m.walpha0Given {
        m.walpha0 = 0.0;
    }
    if !m.walpha1Given {
        m.walpha1 = 0.0;
    }
    if !m.wbeta0Given {
        m.wbeta0 = 0.0;
    }
    if !m.wagidlGiven {
        m.wagidl = 0.0;
    }
    if !m.wbgidlGiven {
        m.wbgidl = 0.0;
    }
    if !m.wcgidlGiven {
        m.wcgidl = 0.0;
    }
    if !m.wegidlGiven {
        m.wegidl = 0.0;
    }
    if !m.wrgidlGiven {
        m.wrgidl = 0.0;
    }
    if !m.wkgidlGiven {
        m.wkgidl = 0.0;
    }
    if !m.wfgidlGiven {
        m.wfgidl = 0.0;
    }
    if !m.wagislGiven {
        m.wagisl = m.wagidl;
    }
    if !m.wbgislGiven {
        m.wbgisl = m.wbgidl;
    }
    if !m.wcgislGiven {
        m.wcgisl = m.wcgidl;
    }
    if !m.wegislGiven {
        m.wegisl = m.wegidl;
    }
    if !m.wrgislGiven {
        m.wrgisl = m.wrgidl;
    }
    if !m.wkgislGiven {
        m.wkgisl = m.wkgidl;
    }
    if !m.wfgislGiven {
        m.wfgisl = m.wfgidl;
    }
    if !m.waigcGiven {
        m.waigc = 0.0;
    }
    if !m.wbigcGiven {
        m.wbigc = 0.0;
    }
    if !m.wcigcGiven {
        m.wcigc = 0.0;
    }
    if !m.aigsdGiven && (m.aigsGiven || m.aigdGiven) {
        if !m.waigsGiven {
            m.waigs = 0.0;
        }
        if !m.waigdGiven {
            m.waigd = 0.0;
        }
    } else {
        if !m.waigsdGiven {
            m.waigsd = 0.0;
        }
        m.waigd = m.waigsd;
        m.waigs = m.waigd;
    }
    if !m.bigsdGiven && (m.bigsGiven || m.bigdGiven) {
        if !m.wbigsGiven {
            m.wbigs = 0.0;
        }
        if !m.wbigdGiven {
            m.wbigd = 0.0;
        }
    } else {
        if !m.wbigsdGiven {
            m.wbigsd = 0.0;
        }
        m.wbigd = m.wbigsd;
        m.wbigs = m.wbigd;
    }
    if !m.cigsdGiven && (m.cigsGiven || m.cigdGiven) {
        if !m.wcigsGiven {
            m.wcigs = 0.0;
        }
        if !m.wcigdGiven {
            m.wcigd = 0.0;
        }
    } else {
        if !m.wcigsdGiven {
            m.wcigsd = 0.0;
        }
        m.wcigd = m.wcigsd;
        m.wcigs = m.wcigd;
    }
    if !m.waigbaccGiven {
        m.waigbacc = 0.0;
    }
    if !m.wbigbaccGiven {
        m.wbigbacc = 0.0;
    }
    if !m.wcigbaccGiven {
        m.wcigbacc = 0.0;
    }
    if !m.waigbinvGiven {
        m.waigbinv = 0.0;
    }
    if !m.wbigbinvGiven {
        m.wbigbinv = 0.0;
    }
    if !m.wcigbinvGiven {
        m.wcigbinv = 0.0;
    }
    if !m.wnigcGiven {
        m.wnigc = 0.0;
    }
    if !m.wnigbinvGiven {
        m.wnigbinv = 0.0;
    }
    if !m.wnigbaccGiven {
        m.wnigbacc = 0.0;
    }
    if !m.wntoxGiven {
        m.wntox = 0.0;
    }
    if !m.weigbinvGiven {
        m.weigbinv = 0.0;
    }
    if !m.wpigcdGiven {
        m.wpigcd = 0.0;
    }
    if !m.wpoxedgeGiven {
        m.wpoxedge = 0.0;
    }
    if !m.wxrcrg1Given {
        m.wxrcrg1 = 0.0;
    }
    if !m.wxrcrg2Given {
        m.wxrcrg2 = 0.0;
    }
    if !m.weuGiven {
        m.weu = 0.0;
    }
    if !m.wucsGiven {
        m.wucs = 0.0;
    }
    if !m.wvfbGiven {
        m.wvfb = 0.0;
    }
    if !m.wlambdaGiven {
        m.wlambda = 0.0;
    }
    if !m.wvtlGiven {
        m.wvtl = 0.0;
    }
    if !m.wxnGiven {
        m.wxn = 0.0;
    }
    if !m.wvfbsdoffGiven {
        m.wvfbsdoff = 0.0;
    }
    if !m.wtvfbsdoffGiven {
        m.wtvfbsdoff = 0.0;
    }
    if !m.wtvoffGiven {
        m.wtvoff = 0.0;
    }
    if !m.wtnfactorGiven {
        m.wtnfactor = 0.0;
    }
    if !m.wteta0Given {
        m.wteta0 = 0.0;
    }
    if !m.wtvoffcvGiven {
        m.wtvoffcv = 0.0;
    }
    if !m.wcgslGiven {
        m.wcgsl = 0.0;
    }
    if !m.wcgdlGiven {
        m.wcgdl = 0.0;
    }
    if !m.wckappasGiven {
        m.wckappas = 0.0;
    }
    if !m.wckappadGiven {
        m.wckappad = 0.0;
    }
    if !m.wcfGiven {
        m.wcf = 0.0;
    }
    if !m.wclcGiven {
        m.wclc = 0.0;
    }
    if !m.wcleGiven {
        m.wcle = 0.0;
    }
    if !m.wvfbcvGiven {
        m.wvfbcv = 0.0;
    }
    if !m.wacdeGiven {
        m.wacde = 0.0;
    }
    if !m.wmoinGiven {
        m.wmoin = 0.0;
    }
    if !m.wnoffGiven {
        m.wnoff = 0.0;
    }
    if !m.wvoffcvGiven {
        m.wvoffcv = 0.0;
    }
    if !m.pcdscGiven {
        m.pcdsc = 0.0;
    }
    if !m.pcdscbGiven {
        m.pcdscb = 0.0;
    }
    if !m.pcdscdGiven {
        m.pcdscd = 0.0;
    }
    if !m.pcitGiven {
        m.pcit = 0.0;
    }
    if !m.pnfactorGiven {
        m.pnfactor = 0.0;
    }
    if !m.pxjGiven {
        m.pxj = 0.0;
    }
    if !m.pvsatGiven {
        m.pvsat = 0.0;
    }
    if !m.patGiven {
        m.pat = 0.0;
    }
    if !m.pa0Given {
        m.pa0 = 0.0;
    }
    if !m.pagsGiven {
        m.pags = 0.0;
    }
    if !m.pa1Given {
        m.pa1 = 0.0;
    }
    if !m.pa2Given {
        m.pa2 = 0.0;
    }
    if !m.pketaGiven {
        m.pketa = 0.0;
    }
    if !m.pketacGiven {
        m.pketac = m.pketa;
    }
    if !m.pnsubGiven {
        m.pnsub = 0.0;
    }
    if !m.pndepGiven {
        m.pndep = 0.0;
    }
    if !m.pnsdGiven {
        m.pnsd = 0.0;
    }
    if !m.pphinGiven {
        m.pphin = 0.0;
    }
    if !m.pngateGiven {
        m.pngate = 0.0;
    }
    if !m.pvbmGiven {
        m.pvbm = 0.0;
    }
    if !m.pxtGiven {
        m.pxt = 0.0;
    }
    if !m.pk1Given {
        m.pk1 = 0.0;
    }
    if !m.pkt1Given {
        m.pkt1 = 0.0;
    }
    if !m.pkt1lGiven {
        m.pkt1l = 0.0;
    }
    if !m.pk2Given {
        m.pk2 = 0.0;
    }
    if !m.pkt2Given {
        m.pkt2 = 0.0;
    }
    if !m.pk3Given {
        m.pk3 = 0.0;
    }
    if !m.pk3bGiven {
        m.pk3b = 0.0;
    }
    if !m.pw0Given {
        m.pw0 = 0.0;
    }
    if !m.plpe0Given {
        m.plpe0 = 0.0;
    }
    if !m.plpebGiven {
        m.plpeb = 0.0;
    }
    if !m.pdvtp0Given {
        m.pdvtp0 = 0.0;
    }
    if !m.pdvtp1Given {
        m.pdvtp1 = 0.0;
    }
    if !m.pdvtp2Given {
        m.pdvtp2 = 0.0;
    }
    if !m.pdvtp3Given {
        m.pdvtp3 = 0.0;
    }
    if !m.pdvtp4Given {
        m.pdvtp4 = 0.0;
    }
    if !m.pdvtp5Given {
        m.pdvtp5 = 0.0;
    }
    if !m.pdvt0Given {
        m.pdvt0 = 0.0;
    }
    if !m.pdvt1Given {
        m.pdvt1 = 0.0;
    }
    if !m.pdvt2Given {
        m.pdvt2 = 0.0;
    }
    if !m.pdvt0wGiven {
        m.pdvt0w = 0.0;
    }
    if !m.pdvt1wGiven {
        m.pdvt1w = 0.0;
    }
    if !m.pdvt2wGiven {
        m.pdvt2w = 0.0;
    }
    if !m.pdroutGiven {
        m.pdrout = 0.0;
    }
    if !m.pdsubGiven {
        m.pdsub = 0.0;
    }
    if !m.pvth0Given {
        m.pvth0 = 0.0;
    }
    if !m.puaGiven {
        m.pua = 0.0;
    }
    if !m.pua1Given {
        m.pua1 = 0.0;
    }
    if !m.pubGiven {
        m.r#pub = 0.0;
    }
    if !m.pub1Given {
        m.pub1 = 0.0;
    }
    if !m.pucGiven {
        m.puc = 0.0;
    }
    if !m.puc1Given {
        m.puc1 = 0.0;
    }
    if !m.pudGiven {
        m.pud = 0.0;
    }
    if !m.pud1Given {
        m.pud1 = 0.0;
    }
    if !m.pupGiven {
        m.pup = 0.0;
    }
    if !m.plpGiven {
        m.plp = 0.0;
    }
    if !m.pu0Given {
        m.pu0 = 0.0;
    }
    if !m.puteGiven {
        m.pute = 0.0;
    }
    if !m.pucsteGiven {
        m.pucste = 0.0;
    }
    if !m.pvoffGiven {
        m.pvoff = 0.0;
    }
    if !m.pminvGiven {
        m.pminv = 0.0;
    }
    if !m.pminvcvGiven {
        m.pminvcv = 0.0;
    }
    if !m.pfproutGiven {
        m.pfprout = 0.0;
    }
    if !m.ppditsGiven {
        m.ppdits = 0.0;
    }
    if !m.ppditsdGiven {
        m.ppditsd = 0.0;
    }
    if !m.pdeltaGiven {
        m.pdelta = 0.0;
    }
    if !m.prdswGiven {
        m.prdsw = 0.0;
    }
    if !m.prdwGiven {
        m.prdw = 0.0;
    }
    if !m.prswGiven {
        m.prsw = 0.0;
    }
    if !m.pprwbGiven {
        m.pprwb = 0.0;
    }
    if !m.pprwgGiven {
        m.pprwg = 0.0;
    }
    if !m.pprtGiven {
        m.pprt = 0.0;
    }
    if !m.peta0Given {
        m.peta0 = 0.0;
    }
    if !m.petabGiven {
        m.petab = 0.0;
    }
    if !m.ppclmGiven {
        m.ppclm = 0.0;
    }
    if !m.ppdibl1Given {
        m.ppdibl1 = 0.0;
    }
    if !m.ppdibl2Given {
        m.ppdibl2 = 0.0;
    }
    if !m.ppdiblbGiven {
        m.ppdiblb = 0.0;
    }
    if !m.ppscbe1Given {
        m.ppscbe1 = 0.0;
    }
    if !m.ppscbe2Given {
        m.ppscbe2 = 0.0;
    }
    if !m.ppvagGiven {
        m.ppvag = 0.0;
    }
    if !m.pwrGiven {
        m.pwr = 0.0;
    }
    if !m.pdwgGiven {
        m.pdwg = 0.0;
    }
    if !m.pdwbGiven {
        m.pdwb = 0.0;
    }
    if !m.pb0Given {
        m.pb0 = 0.0;
    }
    if !m.pb1Given {
        m.pb1 = 0.0;
    }
    if !m.palpha0Given {
        m.palpha0 = 0.0;
    }
    if !m.palpha1Given {
        m.palpha1 = 0.0;
    }
    if !m.pbeta0Given {
        m.pbeta0 = 0.0;
    }
    if !m.pagidlGiven {
        m.pagidl = 0.0;
    }
    if !m.pbgidlGiven {
        m.pbgidl = 0.0;
    }
    if !m.pcgidlGiven {
        m.pcgidl = 0.0;
    }
    if !m.pegidlGiven {
        m.pegidl = 0.0;
    }
    if !m.prgidlGiven {
        m.prgidl = 0.0;
    }
    if !m.pkgidlGiven {
        m.pkgidl = 0.0;
    }
    if !m.pfgidlGiven {
        m.pfgidl = 0.0;
    }
    if !m.pagislGiven {
        m.pagisl = m.pagidl;
    }
    if !m.pbgislGiven {
        m.pbgisl = m.pbgidl;
    }
    if !m.pcgislGiven {
        m.pcgisl = m.pcgidl;
    }
    if !m.pegislGiven {
        m.pegisl = m.pegidl;
    }
    if !m.prgislGiven {
        m.prgisl = m.prgidl;
    }
    if !m.pkgislGiven {
        m.pkgisl = m.pkgidl;
    }
    if !m.pfgislGiven {
        m.pfgisl = m.pfgidl;
    }
    if !m.paigcGiven {
        m.paigc = 0.0;
    }
    if !m.pbigcGiven {
        m.pbigc = 0.0;
    }
    if !m.pcigcGiven {
        m.pcigc = 0.0;
    }
    if !m.aigsdGiven && (m.aigsGiven || m.aigdGiven) {
        if !m.paigsGiven {
            m.paigs = 0.0;
        }
        if !m.paigdGiven {
            m.paigd = 0.0;
        }
    } else {
        if !m.paigsdGiven {
            m.paigsd = 0.0;
        }
        m.paigd = m.paigsd;
        m.paigs = m.paigd;
    }
    if !m.bigsdGiven && (m.bigsGiven || m.bigdGiven) {
        if !m.pbigsGiven {
            m.pbigs = 0.0;
        }
        if !m.pbigdGiven {
            m.pbigd = 0.0;
        }
    } else {
        if !m.pbigsdGiven {
            m.pbigsd = 0.0;
        }
        m.pbigd = m.pbigsd;
        m.pbigs = m.pbigd;
    }
    if !m.cigsdGiven && (m.cigsGiven || m.cigdGiven) {
        if !m.pcigsGiven {
            m.pcigs = 0.0;
        }
        if !m.pcigdGiven {
            m.pcigd = 0.0;
        }
    } else {
        if !m.pcigsdGiven {
            m.pcigsd = 0.0;
        }
        m.pcigd = m.pcigsd;
        m.pcigs = m.pcigd;
    }
    if !m.paigbaccGiven {
        m.paigbacc = 0.0;
    }
    if !m.pbigbaccGiven {
        m.pbigbacc = 0.0;
    }
    if !m.pcigbaccGiven {
        m.pcigbacc = 0.0;
    }
    if !m.paigbinvGiven {
        m.paigbinv = 0.0;
    }
    if !m.pbigbinvGiven {
        m.pbigbinv = 0.0;
    }
    if !m.pcigbinvGiven {
        m.pcigbinv = 0.0;
    }
    if !m.pnigcGiven {
        m.pnigc = 0.0;
    }
    if !m.pnigbinvGiven {
        m.pnigbinv = 0.0;
    }
    if !m.pnigbaccGiven {
        m.pnigbacc = 0.0;
    }
    if !m.pntoxGiven {
        m.pntox = 0.0;
    }
    if !m.peigbinvGiven {
        m.peigbinv = 0.0;
    }
    if !m.ppigcdGiven {
        m.ppigcd = 0.0;
    }
    if !m.ppoxedgeGiven {
        m.ppoxedge = 0.0;
    }
    if !m.pxrcrg1Given {
        m.pxrcrg1 = 0.0;
    }
    if !m.pxrcrg2Given {
        m.pxrcrg2 = 0.0;
    }
    if !m.peuGiven {
        m.peu = 0.0;
    }
    if !m.pucsGiven {
        m.pucs = 0.0;
    }
    if !m.pvfbGiven {
        m.pvfb = 0.0;
    }
    if !m.plambdaGiven {
        m.plambda = 0.0;
    }
    if !m.pvtlGiven {
        m.pvtl = 0.0;
    }
    if !m.pxnGiven {
        m.pxn = 0.0;
    }
    if !m.pvfbsdoffGiven {
        m.pvfbsdoff = 0.0;
    }
    if !m.ptvfbsdoffGiven {
        m.ptvfbsdoff = 0.0;
    }
    if !m.ptvoffGiven {
        m.ptvoff = 0.0;
    }
    if !m.ptnfactorGiven {
        m.ptnfactor = 0.0;
    }
    if !m.pteta0Given {
        m.pteta0 = 0.0;
    }
    if !m.ptvoffcvGiven {
        m.ptvoffcv = 0.0;
    }
    if !m.pcgslGiven {
        m.pcgsl = 0.0;
    }
    if !m.pcgdlGiven {
        m.pcgdl = 0.0;
    }
    if !m.pckappasGiven {
        m.pckappas = 0.0;
    }
    if !m.pckappadGiven {
        m.pckappad = 0.0;
    }
    if !m.pcfGiven {
        m.pcf = 0.0;
    }
    if !m.pclcGiven {
        m.pclc = 0.0;
    }
    if !m.pcleGiven {
        m.pcle = 0.0;
    }
    if !m.pvfbcvGiven {
        m.pvfbcv = 0.0;
    }
    if !m.pacdeGiven {
        m.pacde = 0.0;
    }
    if !m.pmoinGiven {
        m.pmoin = 0.0;
    }
    if !m.pnoffGiven {
        m.pnoff = 0.0;
    }
    if !m.pvoffcvGiven {
        m.pvoffcv = 0.0;
    }
    if !m.gamma1Given {
        m.gamma1 = 0.0;
    }
    if !m.lgamma1Given {
        m.lgamma1 = 0.0;
    }
    if !m.wgamma1Given {
        m.wgamma1 = 0.0;
    }
    if !m.pgamma1Given {
        m.pgamma1 = 0.0;
    }
    if !m.gamma2Given {
        m.gamma2 = 0.0;
    }
    if !m.lgamma2Given {
        m.lgamma2 = 0.0;
    }
    if !m.wgamma2Given {
        m.wgamma2 = 0.0;
    }
    if !m.pgamma2Given {
        m.pgamma2 = 0.0;
    }
    if !m.vbxGiven {
        m.vbx = 0.0;
    }
    if !m.lvbxGiven {
        m.lvbx = 0.0;
    }
    if !m.wvbxGiven {
        m.wvbx = 0.0;
    }
    if !m.pvbxGiven {
        m.pvbx = 0.0;
    }
    if !m.tnomGiven {
        m.tnom = nominal_temperature;
    }
    if !m.LintGiven {
        m.Lint = 0.0;
    }
    if !m.LlGiven {
        m.Ll = 0.0;
    }
    if !m.LlcGiven {
        m.Llc = m.Ll;
    }
    if !m.LlnGiven {
        m.Lln = 1.0;
    }
    if !m.LwGiven {
        m.Lw = 0.0;
    }
    if !m.LwcGiven {
        m.Lwc = m.Lw;
    }
    if !m.LwnGiven {
        m.Lwn = 1.0;
    }
    if !m.LwlGiven {
        m.Lwl = 0.0;
    }
    if !m.LwlcGiven {
        m.Lwlc = m.Lwl;
    }
    if !m.LminGiven {
        m.Lmin = 0.0;
    }
    if !m.LmaxGiven {
        m.Lmax = 1.0;
    }
    if !m.WintGiven {
        m.Wint = 0.0;
    }
    if !m.WlGiven {
        m.Wl = 0.0;
    }
    if !m.WlcGiven {
        m.Wlc = m.Wl;
    }
    if !m.WlnGiven {
        m.Wln = 1.0;
    }
    if !m.WwGiven {
        m.Ww = 0.0;
    }
    if !m.WwcGiven {
        m.Wwc = m.Ww;
    }
    if !m.WwnGiven {
        m.Wwn = 1.0;
    }
    if !m.WwlGiven {
        m.Wwl = 0.0;
    }
    if !m.WwlcGiven {
        m.Wwlc = m.Wwl;
    }
    if !m.WminGiven {
        m.Wmin = 0.0;
    }
    if !m.WmaxGiven {
        m.Wmax = 1.0;
    }
    if !m.dwcGiven {
        m.dwc = m.Wint;
    }
    if !m.dlcGiven {
        m.dlc = m.Lint;
    }
    if !m.xlGiven {
        m.xl = 0.0;
    }
    if !m.xwGiven {
        m.xw = 0.0;
    }
    if !m.dlcigGiven {
        m.dlcig = m.Lint;
    }
    if !m.dlcigdGiven {
        if m.dlcigGiven {
            m.dlcigd = m.dlcig;
        } else {
            m.dlcigd = m.Lint;
        }
    }
    if !m.dwjGiven {
        m.dwj = m.dwc;
    }
    if !m.xpartGiven {
        m.xpart = 0.0;
    }
    if !m.sheetResistanceGiven {
        m.sheetResistance = 0.0;
    }
    if !m.SunitAreaJctCapGiven {
        m.SunitAreaJctCap = 5.0E-4;
    }
    if !m.DunitAreaJctCapGiven {
        m.DunitAreaJctCap = m.SunitAreaJctCap;
    }
    if !m.SunitLengthSidewallJctCapGiven {
        m.SunitLengthSidewallJctCap = 5.0E-10;
    }
    if !m.DunitLengthSidewallJctCapGiven {
        m.DunitLengthSidewallJctCap = m.SunitLengthSidewallJctCap;
    }
    if !m.SunitLengthGateSidewallJctCapGiven {
        m.SunitLengthGateSidewallJctCap = m.SunitLengthSidewallJctCap;
    }
    if !m.DunitLengthGateSidewallJctCapGiven {
        m.DunitLengthGateSidewallJctCap = m.SunitLengthGateSidewallJctCap;
    }
    if !m.SjctSatCurDensityGiven {
        m.SjctSatCurDensity = 1.0E-4;
    }
    if !m.DjctSatCurDensityGiven {
        m.DjctSatCurDensity = m.SjctSatCurDensity;
    }
    if !m.SjctSidewallSatCurDensityGiven {
        m.SjctSidewallSatCurDensity = 0.0;
    }
    if !m.DjctSidewallSatCurDensityGiven {
        m.DjctSidewallSatCurDensity = m.SjctSidewallSatCurDensity;
    }
    if !m.SjctGateSidewallSatCurDensityGiven {
        m.SjctGateSidewallSatCurDensity = 0.0;
    }
    if !m.DjctGateSidewallSatCurDensityGiven {
        m.DjctGateSidewallSatCurDensity = m.SjctGateSidewallSatCurDensity;
    }
    if !m.SbulkJctPotentialGiven {
        m.SbulkJctPotential = 1.0;
    }
    if !m.DbulkJctPotentialGiven {
        m.DbulkJctPotential = m.SbulkJctPotential;
    }
    if !m.SsidewallJctPotentialGiven {
        m.SsidewallJctPotential = 1.0;
    }
    if !m.DsidewallJctPotentialGiven {
        m.DsidewallJctPotential = m.SsidewallJctPotential;
    }
    if !m.SGatesidewallJctPotentialGiven {
        m.SGatesidewallJctPotential = m.SsidewallJctPotential;
    }
    if !m.DGatesidewallJctPotentialGiven {
        m.DGatesidewallJctPotential = m.SGatesidewallJctPotential;
    }
    if !m.SbulkJctBotGradingCoeffGiven {
        m.SbulkJctBotGradingCoeff = 0.5;
    }
    if !m.DbulkJctBotGradingCoeffGiven {
        m.DbulkJctBotGradingCoeff = m.SbulkJctBotGradingCoeff;
    }
    if !m.SbulkJctSideGradingCoeffGiven {
        m.SbulkJctSideGradingCoeff = 0.33;
    }
    if !m.DbulkJctSideGradingCoeffGiven {
        m.DbulkJctSideGradingCoeff = m.SbulkJctSideGradingCoeff;
    }
    if !m.SbulkJctGateSideGradingCoeffGiven {
        m.SbulkJctGateSideGradingCoeff = m.SbulkJctSideGradingCoeff;
    }
    if !m.DbulkJctGateSideGradingCoeffGiven {
        m.DbulkJctGateSideGradingCoeff = m.SbulkJctGateSideGradingCoeff;
    }
    if !m.SjctEmissionCoeffGiven {
        m.SjctEmissionCoeff = 1.0;
    }
    if !m.DjctEmissionCoeffGiven {
        m.DjctEmissionCoeff = m.SjctEmissionCoeff;
    }
    if !m.SjctTempExponentGiven {
        m.SjctTempExponent = 3.0;
    }
    if !m.DjctTempExponentGiven {
        m.DjctTempExponent = m.SjctTempExponent;
    }
    if !m.jtssGiven {
        m.jtss = 0.0;
    }
    if !m.jtsdGiven {
        m.jtsd = m.jtss;
    }
    if !m.jtsswsGiven {
        m.jtssws = 0.0;
    }
    if !m.jtsswdGiven {
        m.jtsswd = m.jtssws;
    }
    if !m.jtsswgsGiven {
        m.jtsswgs = 0.0;
    }
    if !m.jtsswgdGiven {
        m.jtsswgd = m.jtsswgs;
    }
    if !m.jtweffGiven {
        m.jtweff = 0.0;
    }
    if !m.njtsGiven {
        m.njts = 20.0;
    }
    if !m.njtsswGiven {
        m.njtssw = 20.0;
    }
    if !m.njtsswgGiven {
        m.njtsswg = 20.0;
    }
    if !m.njtsdGiven {
        if m.njtsGiven {
            m.njtsd = m.njts;
        } else {
            m.njtsd = 20.0;
        }
    }
    if !m.njtsswdGiven {
        if m.njtsswGiven {
            m.njtsswd = m.njtssw;
        } else {
            m.njtsswd = 20.0;
        }
    }
    if !m.njtsswgdGiven {
        if m.njtsswgGiven {
            m.njtsswgd = m.njtsswg;
        } else {
            m.njtsswgd = 20.0;
        }
    }
    if !m.xtssGiven {
        m.xtss = 0.02;
    }
    if !m.xtsdGiven {
        m.xtsd = m.xtss;
    }
    if !m.xtsswsGiven {
        m.xtssws = 0.02;
    }
    if !m.xtsswdGiven {
        m.xtsswd = m.xtssws;
    }
    if !m.xtsswgsGiven {
        m.xtsswgs = 0.02;
    }
    if !m.xtsswgdGiven {
        m.xtsswgd = m.xtsswgs;
    }
    if !m.tnjtsGiven {
        m.tnjts = 0.0;
    }
    if !m.tnjtsswGiven {
        m.tnjtssw = 0.0;
    }
    if !m.tnjtsswgGiven {
        m.tnjtsswg = 0.0;
    }
    if !m.tnjtsdGiven {
        if m.tnjtsGiven {
            m.tnjtsd = m.tnjts;
        } else {
            m.tnjtsd = 0.0;
        }
    }
    if !m.tnjtsswdGiven {
        if m.tnjtsswGiven {
            m.tnjtsswd = m.tnjtssw;
        } else {
            m.tnjtsswd = 0.0;
        }
    }
    if !m.tnjtsswgdGiven {
        if m.tnjtsswgGiven {
            m.tnjtsswgd = m.tnjtsswg;
        } else {
            m.tnjtsswgd = 0.0;
        }
    }
    if !m.vtssGiven {
        m.vtss = 10.0;
    }
    if !m.vtsdGiven {
        m.vtsd = m.vtss;
    }
    if !m.vtsswsGiven {
        m.vtssws = 10.0;
    }
    if !m.vtsswdGiven {
        m.vtsswd = m.vtssws;
    }
    if !m.vtsswgsGiven {
        m.vtsswgs = 10.0;
    }
    if !m.vtsswgdGiven {
        m.vtsswgd = m.vtsswgs;
    }
    if !m.oxideTrapDensityAGiven {
        if m.r#type == 1 {
            m.oxideTrapDensityA = 6.25e41;
        } else {
            m.oxideTrapDensityA = 6.188e40;
        }
    }
    if !m.oxideTrapDensityBGiven {
        if m.r#type == 1 {
            m.oxideTrapDensityB = 3.125e26;
        } else {
            m.oxideTrapDensityB = 1.5e25;
        }
    }
    if !m.oxideTrapDensityCGiven {
        m.oxideTrapDensityC = 8.75e9;
    }
    if !m.emGiven {
        m.em = 4.1e7;
    }
    if !m.efGiven {
        m.ef = 1.0;
    }
    if !m.afGiven {
        m.af = 1.0;
    }
    if !m.kfGiven {
        m.kf = 0.0;
    }
    if !m.vgsMaxGiven {
        m.vgsMax = 1e99;
    }
    if !m.vgdMaxGiven {
        m.vgdMax = 1e99;
    }
    if !m.vgbMaxGiven {
        m.vgbMax = 1e99;
    }
    if !m.vdsMaxGiven {
        m.vdsMax = 1e99;
    }
    if !m.vbsMaxGiven {
        m.vbsMax = 1e99;
    }
    if !m.vbdMaxGiven {
        m.vbdMax = 1e99;
    }
    if !m.vgsrMaxGiven {
        m.vgsrMax = 1e99;
    }
    if !m.vgdrMaxGiven {
        m.vgdrMax = 1e99;
    }
    if !m.vgbrMaxGiven {
        m.vgbrMax = 1e99;
    }
    if !m.vbsrMaxGiven {
        m.vbsrMax = 1e99;
    }
    if !m.vbdrMaxGiven {
        m.vbdrMax = 1e99;
    }
    if !m.sarefGiven {
        m.saref = 1e-6;
    }
    if !m.sbrefGiven {
        m.sbref = 1e-6;
    }
    if !m.wlodGiven {
        m.wlod = 0.0;
    }
    if !m.ku0Given {
        m.ku0 = 0.0;
    }
    if !m.kvsatGiven {
        m.kvsat = 0.0;
    }
    if !m.kvth0Given {
        m.kvth0 = 0.0;
    }
    if !m.tku0Given {
        m.tku0 = 0.0;
    }
    if !m.llodku0Given {
        m.llodku0 = 0.0;
    }
    if !m.wlodku0Given {
        m.wlodku0 = 0.0;
    }
    if !m.llodvthGiven {
        m.llodvth = 0.0;
    }
    if !m.wlodvthGiven {
        m.wlodvth = 0.0;
    }
    if !m.lku0Given {
        m.lku0 = 0.0;
    }
    if !m.wku0Given {
        m.wku0 = 0.0;
    }
    if !m.pku0Given {
        m.pku0 = 0.0;
    }
    if !m.lkvth0Given {
        m.lkvth0 = 0.0;
    }
    if !m.wkvth0Given {
        m.wkvth0 = 0.0;
    }
    if !m.pkvth0Given {
        m.pkvth0 = 0.0;
    }
    if !m.stk2Given {
        m.stk2 = 0.0;
    }
    if !m.lodk2Given {
        m.lodk2 = 1.0;
    }
    if !m.steta0Given {
        m.steta0 = 0.0;
    }
    if !m.lodeta0Given {
        m.lodeta0 = 1.0;
    }
    if !m.webGiven {
        m.web = 0.0;
    }
    if !m.wecGiven {
        m.wec = 0.0;
    }
    if !m.kvth0weGiven {
        m.kvth0we = 0.0;
    }
    if !m.k2weGiven {
        m.k2we = 0.0;
    }
    if !m.ku0weGiven {
        m.ku0we = 0.0;
    }
    if !m.screfGiven {
        m.scref = 1.0E-6;
    }
    if !m.wpemodGiven {
        m.wpemod = 0.0;
    } else if (m.wpemod != 0.0) && (m.wpemod != 1.0) {
        m.wpemod = 0.0;
        warnings.push("Warning: wpemod has been set to its default value: 0.".into());
    }
    if !m.lkvth0weGiven {
        m.lkvth0we = 0.0;
    }
    if !m.lk2weGiven {
        m.lk2we = 0.0;
    }
    if !m.lku0weGiven {
        m.lku0we = 0.0;
    }
    if !m.wkvth0weGiven {
        m.wkvth0we = 0.0;
    }
    if !m.wk2weGiven {
        m.wk2we = 0.0;
    }
    if !m.wku0weGiven {
        m.wku0we = 0.0;
    }
    if !m.pkvth0weGiven {
        m.pkvth0we = 0.0;
    }
    if !m.pk2weGiven {
        m.pk2we = 0.0;
    }
    if !m.pku0weGiven {
        m.pku0we = 0.0;
    }

    Ok(())
}

/// `BSIM4setup`'s instance pass: instance defaults and selector overrides,
/// then the internal nodes to create, as `(local node index, CKTmkVolt
/// suffix)` in C's creation order. The caller has set the four external
/// node fields; this sets every internal node field (an uncreated node
/// aliases its external node, an uncreated `qNode` is
/// [`super::host::node::GROUND`]). `noise_analysis` is C's
/// `noiseAnalGiven` (the deck requests a `.noise` analysis).
///
/// # Errors
/// None today: C's only error here is `CKTmkVolt` running out of memory.
#[allow(clippy::too_many_lines)] // One C loop body, kept whole.
#[allow(clippy::unnecessary_wraps)] // Uniform stage signature.
pub(crate) fn setup_instance(
    m: &Model,
    h: &mut Instance,
    noise_analysis: bool,
    warnings: &mut Vec<String>,
) -> SpiceResult<Vec<(i32, &'static str)>> {
    // End of C's model pass, before its instance loop.
    let DMCGeff = m.dmcg - m.dmcgt;
    let DMCIeff = m.dmci;
    let DMDGeff = m.dmdg - m.dmcgt;
    let mut created = Vec::new();

    if !h.lGiven {
        h.l = 5.0e-6;
    }
    if !h.wGiven {
        h.w = 5.0e-6;
    }
    if !h.mGiven {
        h.m = 1.0;
    }
    if !h.mult_iGiven {
        h.mult_i = 1.0;
    }
    if !h.mult_qGiven {
        h.mult_q = 1.0;
    }
    if !h.mult_fnGiven {
        h.mult_fn = h.mult_i;
    }
    if !h.nfGiven {
        h.nf = 1.0;
    }
    if !h.minGiven {
        h.min = 0; // integer
    }
    if !h.icVDSGiven {
        h.icVDS = 0.0;
    }
    if !h.icVGSGiven {
        h.icVGS = 0.0;
    }
    if !h.icVBSGiven {
        h.icVBS = 0.0;
    }
    if !h.drainAreaGiven {
        h.drainArea = 0.0;
    }
    if !h.drainPerimeterGiven {
        h.drainPerimeter = 0.0;
    }
    if !h.drainSquaresGiven {
        h.drainSquares = 1.0;
    }
    if !h.sourceAreaGiven {
        h.sourceArea = 0.0;
    }
    if !h.sourcePerimeterGiven {
        h.sourcePerimeter = 0.0;
    }
    if !h.sourceSquaresGiven {
        h.sourceSquares = 1.0;
    }

    if !h.rbdbGiven {
        h.rbdb = m.rbdb; // in ohm
    }
    if !h.rbsbGiven {
        h.rbsb = m.rbsb;
    }
    if !h.rbpbGiven {
        h.rbpb = m.rbpb;
    }
    if !h.rbpsGiven {
        h.rbps = m.rbps;
    }
    if !h.rbpdGiven {
        h.rbpd = m.rbpd;
    }
    if !h.delvtoGiven {
        h.delvto = 0.0;
    }
    if !h.mulu0Given {
        h.mulu0 = 1.0;
    }
    if !h.xgwGiven {
        h.xgw = m.xgw;
    }
    if !h.ngconGiven {
        h.ngcon = m.ngcon;
    }
    if !h.dtempGiven {
        h.dtemp = 0.0;
    }

    // Process instance model selectors, some may override their global
    // counterparts.
    if !h.rbodyModGiven {
        h.rbodyMod = m.rbodyMod;
    } else if (h.rbodyMod != 0) && (h.rbodyMod != 1) && (h.rbodyMod != 2) {
        h.rbodyMod = m.rbodyMod;
        warnings.push(format!(
            "Warning: rbodyMod has been set to its global value {}.",
            m.rbodyMod
        ));
    }

    if !h.rgateModGiven {
        h.rgateMod = m.rgateMod;
    } else if (h.rgateMod != 0) && (h.rgateMod != 1) && (h.rgateMod != 2) && (h.rgateMod != 3) {
        h.rgateMod = m.rgateMod;
        warnings.push(format!(
            "Warning: rgateMod has been set to its global value {}.",
            m.rgateMod
        ));
    }

    if !h.geoModGiven {
        h.geoMod = m.geoMod;
    }
    if !h.rgeoModGiven {
        h.rgeoMod = 0;
    }
    if !h.trnqsModGiven {
        h.trnqsMod = m.trnqsMod;
    } else if (h.trnqsMod != 0) && (h.trnqsMod != 1) {
        h.trnqsMod = m.trnqsMod;
        warnings.push(format!(
            "Warning: trnqsMod has been set to its global value {}.",
            m.trnqsMod
        ));
    }

    if !h.acnqsModGiven {
        h.acnqsMod = m.acnqsMod;
    } else if (h.acnqsMod != 0) && (h.acnqsMod != 1) {
        h.acnqsMod = m.acnqsMod;
        warnings.push(format!(
            "Warning: acnqsMod has been set to its global value {}.",
            m.acnqsMod
        ));
    }

    // Stress effect.
    if !h.saGiven {
        h.sa = 0.0;
    }
    if !h.sbGiven {
        h.sb = 0.0;
    }
    if !h.sdGiven {
        h.sd = 2.0 * m.dmcg;
    }
    // Well proximity effect.
    if !h.scaGiven {
        h.sca = 0.0;
    }
    if !h.scbGiven {
        h.scb = 0.0;
    }
    if !h.sccGiven {
        h.scc = 0.0;
    }
    if !h.scGiven {
        h.sc = 0.0; // m
    }

    // Process drain series resistance.
    let mut createNode = false;
    if (m.rdsMod != 0) || (m.tnoiMod == 1 && noise_analysis) {
        createNode = true;
    } else if m.sheetResistance > 0.0 {
        if h.drainSquaresGiven && h.drainSquares > 0.0 {
            createNode = true;
        } else if !h.drainSquaresGiven && (h.rgeoMod != 0) {
            let Rtot = rdseff_geo(
                h.nf,
                h.geoMod,
                h.rgeoMod,
                h.min,
                h.w,
                m.sheetResistance,
                DMCGeff,
                DMCIeff,
                DMDGeff,
                0,
                warnings,
            );
            if Rtot > 0.0 {
                createNode = true;
            }
        }
    }
    if createNode {
        h.dNodePrime = node::DP;
        created.push((node::DP, "drain"));
    } else {
        h.dNodePrime = h.dNode;
    }

    // Process source series resistance.
    createNode = false;
    if (m.rdsMod != 0) || (m.tnoiMod == 1 && noise_analysis) {
        createNode = true;
    } else if m.sheetResistance > 0.0 {
        if h.sourceSquaresGiven && h.sourceSquares > 0.0 {
            createNode = true;
        } else if !h.sourceSquaresGiven && (h.rgeoMod != 0) {
            let Rtot = rdseff_geo(
                h.nf,
                h.geoMod,
                h.rgeoMod,
                h.min,
                h.w,
                m.sheetResistance,
                DMCGeff,
                DMCIeff,
                DMDGeff,
                1,
                warnings,
            );
            if Rtot > 0.0 {
                createNode = true;
            }
        }
    }
    if createNode {
        h.sNodePrime = node::SP;
        created.push((node::SP, "source"));
    } else {
        h.sNodePrime = h.sNode;
    }

    if h.rgateMod > 0 {
        h.gNodePrime = node::GP;
        created.push((node::GP, "gate"));
    } else {
        h.gNodePrime = h.gNodeExt;
    }

    if h.rgateMod == 3 {
        h.gNodeMid = node::GM;
        created.push((node::GM, "midgate"));
    } else {
        h.gNodeMid = h.gNodeExt;
    }

    // Internal body nodes for the body resistance model.
    if (h.rbodyMod == 1) || (h.rbodyMod == 2) {
        h.dbNode = node::DB;
        created.push((node::DB, "dbody"));
        h.bNodePrime = node::BP;
        created.push((node::BP, "body"));
        h.sbNode = node::SB;
        created.push((node::SB, "sbody"));
    } else {
        h.dbNode = h.bNode;
        h.bNodePrime = h.bNode;
        h.sbNode = h.bNode;
    }

    // NQS node.
    if h.trnqsMod != 0 {
        h.qNode = node::Q;
        created.push((node::Q, "charge"));
    } else {
        h.qNode = node::GROUND;
    }
    Ok(created)
}

impl ModelParameters {
    /// Every numeric model keyword with the value C's `BSIM4setup` model
    /// pass leaves in its member: the explicit setters, then `b4set.c`'s
    /// defaults at nominal temperature `nominal_temperature_celsius`.
    /// Aliases (`vtho`, ...) and ask-only, flag and string rows are
    /// omitted. Integer members are reported as reals. Values are before
    /// `b4temp.c`, which the C `@model[...]` ask sees after; this is a
    /// verification hook for the setup stage, not a simulation result.
    ///
    /// # Errors
    /// An invalid nominal temperature, or an internal member mismatch.
    pub fn setup_values(
        &self,
        nominal_temperature_celsius: Real,
    ) -> SpiceResult<Vec<(&'static str, Real)>> {
        let tnom = temperature_kelvin(nominal_temperature_celsius, &self.location)?;
        let mut m = super::apply::model(self)?;
        let mut warnings = Vec::new();
        setup_model(&mut m, tnom, &mut warnings)?;
        numeric_members(&mut m)
    }
}

/// `(keyword, value)` for every non-redundant numeric MODEL row.
fn numeric_members(m: &mut Model) -> SpiceResult<Vec<(&'static str, Real)>> {
    let mut values = Vec::new();
    for p in parameters::MODEL.iter().filter(|p| !p.redundant) {
        if !matches!(p.input_type, InputType::Real | InputType::Integer) {
            continue;
        }
        let Some(field) = p.c_field else { continue };
        let value = if let Some(v) = m.real_mut(field) {
            *v
        } else if let Some(v) = m.int_mut(field) {
            Real::from(*v)
        } else {
            return Err(SpiceError::circuit(format!(
                "BSIM4 model member {field} for {} is missing",
                p.name
            )));
        };
        values.push((p.name, value));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaulted(polarity: i32) -> (Model, Vec<String>) {
        let mut m = Model {
            r#type: polarity,
            typeGiven: true,
            ..Model::default()
        };
        let mut warnings = Vec::new();
        setup_model(&mut m, 300.15, &mut warnings).unwrap();
        (m, warnings)
    }

    #[test]
    fn literal_defaults_agree_with_the_generated_facts() {
        let (mut m, warnings) = defaulted(1);
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut checked = 0;
        for &(keyword, expected) in super::super::defaults_data::LITERALS {
            let p = super::super::descriptor(parameters::MODEL, keyword).unwrap();
            let field = p.c_field.unwrap();
            let actual = if let Some(v) = m.real_mut(field) {
                *v
            } else {
                Real::from(*m.int_mut(field).unwrap())
            };
            assert_eq!(actual.to_bits(), expected.to_bits(), "{keyword}");
            checked += 1;
        }
        assert_eq!(checked, 777);
    }

    #[test]
    fn polarity_and_dependent_defaults() {
        let (n, _) = defaulted(1);
        let (p, _) = defaulted(-1);
        assert_eq!((n.vth0, p.vth0), (0.7, -0.7));
        assert_eq!((n.u0, p.u0), (0.067, 0.025));
        assert_eq!(n.toxp, n.toxe);
        assert_eq!(n.tnom, 300.15);
        assert_eq!(n.version, "4.8.3");
        assert_eq!(n.v48intVersion, 483);
        assert_eq!(n.ua, 1.0e-9);
    }

    #[test]
    fn out_of_range_selectors_warn_as_c_does() {
        let mut m = Model {
            r#type: 1,
            typeGiven: true,
            capMod: 7,
            capModGiven: true,
            mtrlMod: 5,
            mtrlModGiven: true,
            version: "4.7".into(),
            versionGiven: true,
            ..Model::default()
        };
        let mut warnings = Vec::new();
        setup_model(&mut m, 300.15, &mut warnings).unwrap();
        assert_eq!(m.capMod, 2);
        assert_eq!(m.mtrlMod, 0);
        assert_eq!(m.v48intVersion, 483);
        assert_eq!(
            warnings,
            [
                "Warning: capMod has been set to its default value: 2.",
                "Warning: mtrlMod has been set to its default value: 0.",
                "Warning: unknown BSIM4 version. Working now with BSIM4.8.3.",
            ]
        );
        // Any 4.8x spelling selects 4.8.3 through the "4.8" prefix test.
        assert!(c_strncmp_eq("4.8.0", "4.8", 3));
        assert!(!c_strncmp_eq("4.8", "4.80", 4));
    }

    fn instance() -> Instance {
        Instance {
            dNode: node::D,
            gNodeExt: node::G_EXT,
            sNode: node::S,
            bNode: node::B,
            ..Instance::default()
        }
    }

    #[test]
    fn default_instance_creates_no_nodes() {
        let (m, _) = defaulted(1);
        let mut h = instance();
        let mut w = Vec::new();
        let created = setup_instance(&m, &mut h, false, &mut w).unwrap();
        assert!(created.is_empty());
        assert_eq!((h.l, h.w, h.nf, h.m), (5.0e-6, 5.0e-6, 1.0, 1.0));
        assert_eq!(
            [h.dNodePrime, h.sNodePrime, h.gNodePrime, h.gNodeMid],
            [node::D, node::S, node::G_EXT, node::G_EXT]
        );
        assert_eq!([h.dbNode, h.bNodePrime, h.sbNode], [node::B; 3]);
        assert_eq!(h.qNode, node::GROUND);
        assert_eq!(h.sd, 2.0 * m.dmcg);
    }

    #[test]
    fn every_internal_node_in_c_creation_order() {
        let (mut m, _) = defaulted(1);
        m.rdsMod = 1;
        let mut h = instance();
        h.rgateMod = 3;
        h.rgateModGiven = true;
        h.rbodyMod = 1;
        h.rbodyModGiven = true;
        h.trnqsMod = 1;
        h.trnqsModGiven = true;
        let mut w = Vec::new();
        let created = setup_instance(&m, &mut h, false, &mut w).unwrap();
        let suffixes: Vec<_> = created.iter().map(|c| c.1).collect();
        assert_eq!(
            suffixes,
            [
                "drain", "source", "gate", "midgate", "dbody", "body", "sbody", "charge"
            ]
        );
        assert!(w.is_empty());
    }

    #[test]
    fn invalid_instance_selector_falls_back_with_warning() {
        let (m, _) = defaulted(1);
        let mut h = instance();
        h.rbodyMod = 5;
        h.rbodyModGiven = true;
        let mut w = Vec::new();
        setup_instance(&m, &mut h, false, &mut w).unwrap();
        assert_eq!(h.rbodyMod, 0);
        assert_eq!(w, ["Warning: rbodyMod has been set to its global value 0."]);
    }

    #[test]
    fn sheet_resistance_with_squares_or_rgeo_creates_primes() {
        let (mut m, _) = defaulted(1);
        m.sheetResistance = 10.0;
        let mut h = instance();
        h.drainSquares = 2.0;
        h.drainSquaresGiven = true;
        let mut w = Vec::new();
        let created = setup_instance(&m, &mut h, false, &mut w).unwrap();
        assert_eq!(created, [(node::DP, "drain")]);
        // tnoiMod=1 creates both primes only for a deck with .noise.
        let (mut m, _) = defaulted(1);
        m.tnoiMod = 1;
        let mut h = instance();
        assert!(
            setup_instance(&m, &mut h, false, &mut w)
                .unwrap()
                .is_empty()
        );
        let mut h = instance();
        assert_eq!(setup_instance(&m, &mut h, true, &mut w).unwrap().len(), 2);
    }
}
