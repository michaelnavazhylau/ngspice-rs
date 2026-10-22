// Generated field facts (C member names and scalar types), not C
// implementation code. Source: ngspice 059066b532f7073fccf4173ef269309eafdc7f0c, bsim3/bsim3def.h.
// Regenerate: cargo xtask bsim3-schema --source /path/to/ngspice
#![allow(non_snake_case, clippy::struct_excessive_bools)]

/// `struct sBSIM3model`'s scalar members (868 of them), in declaration order.
#[derive(Clone, Default)]
pub(crate) struct Model {
    pub(crate) r#type: i32,
    pub(crate) mobMod: i32,
    pub(crate) capMod: i32,
    pub(crate) acmMod: i32,
    pub(crate) calcacm: i32,
    pub(crate) noiMod: i32,
    pub(crate) nqsMod: i32,
    pub(crate) acnqsMod: i32,
    pub(crate) binUnit: i32,
    pub(crate) paramChk: i32,
    pub(crate) version: String,
    pub(crate) tox: f64,
    pub(crate) toxm: f64,
    pub(crate) cdsc: f64,
    pub(crate) cdscb: f64,
    pub(crate) cdscd: f64,
    pub(crate) cit: f64,
    pub(crate) nfactor: f64,
    pub(crate) xj: f64,
    pub(crate) vsat: f64,
    pub(crate) at: f64,
    pub(crate) a0: f64,
    pub(crate) ags: f64,
    pub(crate) a1: f64,
    pub(crate) a2: f64,
    pub(crate) keta: f64,
    pub(crate) nsub: f64,
    pub(crate) npeak: f64,
    pub(crate) ngate: f64,
    pub(crate) gamma1: f64,
    pub(crate) gamma2: f64,
    pub(crate) vbx: f64,
    pub(crate) vbm: f64,
    pub(crate) xt: f64,
    pub(crate) k1: f64,
    pub(crate) kt1: f64,
    pub(crate) kt1l: f64,
    pub(crate) kt2: f64,
    pub(crate) k2: f64,
    pub(crate) k3: f64,
    pub(crate) k3b: f64,
    pub(crate) w0: f64,
    pub(crate) nlx: f64,
    pub(crate) dvt0: f64,
    pub(crate) dvt1: f64,
    pub(crate) dvt2: f64,
    pub(crate) dvt0w: f64,
    pub(crate) dvt1w: f64,
    pub(crate) dvt2w: f64,
    pub(crate) drout: f64,
    pub(crate) dsub: f64,
    pub(crate) vth0: f64,
    pub(crate) ua: f64,
    pub(crate) ua1: f64,
    pub(crate) ub: f64,
    pub(crate) ub1: f64,
    pub(crate) uc: f64,
    pub(crate) uc1: f64,
    pub(crate) u0: f64,
    pub(crate) ute: f64,
    pub(crate) voff: f64,
    pub(crate) delta: f64,
    pub(crate) rdsw: f64,
    pub(crate) prwg: f64,
    pub(crate) prwb: f64,
    pub(crate) prt: f64,
    pub(crate) eta0: f64,
    pub(crate) etab: f64,
    pub(crate) pclm: f64,
    pub(crate) pdibl1: f64,
    pub(crate) pdibl2: f64,
    pub(crate) pdiblb: f64,
    pub(crate) pscbe1: f64,
    pub(crate) pscbe2: f64,
    pub(crate) pvag: f64,
    pub(crate) wr: f64,
    pub(crate) dwg: f64,
    pub(crate) dwb: f64,
    pub(crate) b0: f64,
    pub(crate) b1: f64,
    pub(crate) alpha0: f64,
    pub(crate) alpha1: f64,
    pub(crate) beta0: f64,
    pub(crate) ijth: f64,
    pub(crate) vfb: f64,
    pub(crate) elm: f64,
    pub(crate) cgsl: f64,
    pub(crate) cgdl: f64,
    pub(crate) ckappa: f64,
    pub(crate) cf: f64,
    pub(crate) vfbcv: f64,
    pub(crate) clc: f64,
    pub(crate) cle: f64,
    pub(crate) dwc: f64,
    pub(crate) dlc: f64,
    pub(crate) noff: f64,
    pub(crate) voffcv: f64,
    pub(crate) acde: f64,
    pub(crate) moin: f64,
    pub(crate) tcj: f64,
    pub(crate) tcjsw: f64,
    pub(crate) tcjswg: f64,
    pub(crate) tpb: f64,
    pub(crate) tpbsw: f64,
    pub(crate) tpbswg: f64,
    pub(crate) xl: f64,
    pub(crate) xw: f64,
    pub(crate) hdif: f64,
    pub(crate) ldif: f64,
    pub(crate) ld: f64,
    pub(crate) rd: f64,
    pub(crate) rs: f64,
    pub(crate) rdc: f64,
    pub(crate) rsc: f64,
    pub(crate) wmlt: f64,
    pub(crate) lcdsc: f64,
    pub(crate) lcdscb: f64,
    pub(crate) lcdscd: f64,
    pub(crate) lcit: f64,
    pub(crate) lnfactor: f64,
    pub(crate) lxj: f64,
    pub(crate) lvsat: f64,
    pub(crate) lat: f64,
    pub(crate) la0: f64,
    pub(crate) lags: f64,
    pub(crate) la1: f64,
    pub(crate) la2: f64,
    pub(crate) lketa: f64,
    pub(crate) lnsub: f64,
    pub(crate) lnpeak: f64,
    pub(crate) lngate: f64,
    pub(crate) lgamma1: f64,
    pub(crate) lgamma2: f64,
    pub(crate) lvbx: f64,
    pub(crate) lvbm: f64,
    pub(crate) lxt: f64,
    pub(crate) lk1: f64,
    pub(crate) lkt1: f64,
    pub(crate) lkt1l: f64,
    pub(crate) lkt2: f64,
    pub(crate) lk2: f64,
    pub(crate) lk3: f64,
    pub(crate) lk3b: f64,
    pub(crate) lw0: f64,
    pub(crate) lnlx: f64,
    pub(crate) ldvt0: f64,
    pub(crate) ldvt1: f64,
    pub(crate) ldvt2: f64,
    pub(crate) ldvt0w: f64,
    pub(crate) ldvt1w: f64,
    pub(crate) ldvt2w: f64,
    pub(crate) ldrout: f64,
    pub(crate) ldsub: f64,
    pub(crate) lvth0: f64,
    pub(crate) lua: f64,
    pub(crate) lua1: f64,
    pub(crate) lub: f64,
    pub(crate) lub1: f64,
    pub(crate) luc: f64,
    pub(crate) luc1: f64,
    pub(crate) lu0: f64,
    pub(crate) lute: f64,
    pub(crate) lvoff: f64,
    pub(crate) ldelta: f64,
    pub(crate) lrdsw: f64,
    pub(crate) lprwg: f64,
    pub(crate) lprwb: f64,
    pub(crate) lprt: f64,
    pub(crate) leta0: f64,
    pub(crate) letab: f64,
    pub(crate) lpclm: f64,
    pub(crate) lpdibl1: f64,
    pub(crate) lpdibl2: f64,
    pub(crate) lpdiblb: f64,
    pub(crate) lpscbe1: f64,
    pub(crate) lpscbe2: f64,
    pub(crate) lpvag: f64,
    pub(crate) lwr: f64,
    pub(crate) ldwg: f64,
    pub(crate) ldwb: f64,
    pub(crate) lb0: f64,
    pub(crate) lb1: f64,
    pub(crate) lalpha0: f64,
    pub(crate) lalpha1: f64,
    pub(crate) lbeta0: f64,
    pub(crate) lvfb: f64,
    pub(crate) lelm: f64,
    pub(crate) lcgsl: f64,
    pub(crate) lcgdl: f64,
    pub(crate) lckappa: f64,
    pub(crate) lcf: f64,
    pub(crate) lclc: f64,
    pub(crate) lcle: f64,
    pub(crate) lvfbcv: f64,
    pub(crate) lnoff: f64,
    pub(crate) lvoffcv: f64,
    pub(crate) lacde: f64,
    pub(crate) lmoin: f64,
    pub(crate) wcdsc: f64,
    pub(crate) wcdscb: f64,
    pub(crate) wcdscd: f64,
    pub(crate) wcit: f64,
    pub(crate) wnfactor: f64,
    pub(crate) wxj: f64,
    pub(crate) wvsat: f64,
    pub(crate) wat: f64,
    pub(crate) wa0: f64,
    pub(crate) wags: f64,
    pub(crate) wa1: f64,
    pub(crate) wa2: f64,
    pub(crate) wketa: f64,
    pub(crate) wnsub: f64,
    pub(crate) wnpeak: f64,
    pub(crate) wngate: f64,
    pub(crate) wgamma1: f64,
    pub(crate) wgamma2: f64,
    pub(crate) wvbx: f64,
    pub(crate) wvbm: f64,
    pub(crate) wxt: f64,
    pub(crate) wk1: f64,
    pub(crate) wkt1: f64,
    pub(crate) wkt1l: f64,
    pub(crate) wkt2: f64,
    pub(crate) wk2: f64,
    pub(crate) wk3: f64,
    pub(crate) wk3b: f64,
    pub(crate) ww0: f64,
    pub(crate) wnlx: f64,
    pub(crate) wdvt0: f64,
    pub(crate) wdvt1: f64,
    pub(crate) wdvt2: f64,
    pub(crate) wdvt0w: f64,
    pub(crate) wdvt1w: f64,
    pub(crate) wdvt2w: f64,
    pub(crate) wdrout: f64,
    pub(crate) wdsub: f64,
    pub(crate) wvth0: f64,
    pub(crate) wua: f64,
    pub(crate) wua1: f64,
    pub(crate) wub: f64,
    pub(crate) wub1: f64,
    pub(crate) wuc: f64,
    pub(crate) wuc1: f64,
    pub(crate) wu0: f64,
    pub(crate) wute: f64,
    pub(crate) wvoff: f64,
    pub(crate) wdelta: f64,
    pub(crate) wrdsw: f64,
    pub(crate) wprwg: f64,
    pub(crate) wprwb: f64,
    pub(crate) wprt: f64,
    pub(crate) weta0: f64,
    pub(crate) wetab: f64,
    pub(crate) wpclm: f64,
    pub(crate) wpdibl1: f64,
    pub(crate) wpdibl2: f64,
    pub(crate) wpdiblb: f64,
    pub(crate) wpscbe1: f64,
    pub(crate) wpscbe2: f64,
    pub(crate) wpvag: f64,
    pub(crate) wwr: f64,
    pub(crate) wdwg: f64,
    pub(crate) wdwb: f64,
    pub(crate) wb0: f64,
    pub(crate) wb1: f64,
    pub(crate) walpha0: f64,
    pub(crate) walpha1: f64,
    pub(crate) wbeta0: f64,
    pub(crate) wvfb: f64,
    pub(crate) welm: f64,
    pub(crate) wcgsl: f64,
    pub(crate) wcgdl: f64,
    pub(crate) wckappa: f64,
    pub(crate) wcf: f64,
    pub(crate) wclc: f64,
    pub(crate) wcle: f64,
    pub(crate) wvfbcv: f64,
    pub(crate) wnoff: f64,
    pub(crate) wvoffcv: f64,
    pub(crate) wacde: f64,
    pub(crate) wmoin: f64,
    pub(crate) pcdsc: f64,
    pub(crate) pcdscb: f64,
    pub(crate) pcdscd: f64,
    pub(crate) pcit: f64,
    pub(crate) pnfactor: f64,
    pub(crate) pxj: f64,
    pub(crate) pvsat: f64,
    pub(crate) pat: f64,
    pub(crate) pa0: f64,
    pub(crate) pags: f64,
    pub(crate) pa1: f64,
    pub(crate) pa2: f64,
    pub(crate) pketa: f64,
    pub(crate) pnsub: f64,
    pub(crate) pnpeak: f64,
    pub(crate) pngate: f64,
    pub(crate) pgamma1: f64,
    pub(crate) pgamma2: f64,
    pub(crate) pvbx: f64,
    pub(crate) pvbm: f64,
    pub(crate) pxt: f64,
    pub(crate) pk1: f64,
    pub(crate) pkt1: f64,
    pub(crate) pkt1l: f64,
    pub(crate) pkt2: f64,
    pub(crate) pk2: f64,
    pub(crate) pk3: f64,
    pub(crate) pk3b: f64,
    pub(crate) pw0: f64,
    pub(crate) pnlx: f64,
    pub(crate) pdvt0: f64,
    pub(crate) pdvt1: f64,
    pub(crate) pdvt2: f64,
    pub(crate) pdvt0w: f64,
    pub(crate) pdvt1w: f64,
    pub(crate) pdvt2w: f64,
    pub(crate) pdrout: f64,
    pub(crate) pdsub: f64,
    pub(crate) pvth0: f64,
    pub(crate) pua: f64,
    pub(crate) pua1: f64,
    pub(crate) r#pub: f64,
    pub(crate) pub1: f64,
    pub(crate) puc: f64,
    pub(crate) puc1: f64,
    pub(crate) pu0: f64,
    pub(crate) pute: f64,
    pub(crate) pvoff: f64,
    pub(crate) pdelta: f64,
    pub(crate) prdsw: f64,
    pub(crate) pprwg: f64,
    pub(crate) pprwb: f64,
    pub(crate) pprt: f64,
    pub(crate) peta0: f64,
    pub(crate) petab: f64,
    pub(crate) ppclm: f64,
    pub(crate) ppdibl1: f64,
    pub(crate) ppdibl2: f64,
    pub(crate) ppdiblb: f64,
    pub(crate) ppscbe1: f64,
    pub(crate) ppscbe2: f64,
    pub(crate) ppvag: f64,
    pub(crate) pwr: f64,
    pub(crate) pdwg: f64,
    pub(crate) pdwb: f64,
    pub(crate) pb0: f64,
    pub(crate) pb1: f64,
    pub(crate) palpha0: f64,
    pub(crate) palpha1: f64,
    pub(crate) pbeta0: f64,
    pub(crate) pvfb: f64,
    pub(crate) pelm: f64,
    pub(crate) pcgsl: f64,
    pub(crate) pcgdl: f64,
    pub(crate) pckappa: f64,
    pub(crate) pcf: f64,
    pub(crate) pclc: f64,
    pub(crate) pcle: f64,
    pub(crate) pvfbcv: f64,
    pub(crate) pnoff: f64,
    pub(crate) pvoffcv: f64,
    pub(crate) pacde: f64,
    pub(crate) pmoin: f64,
    pub(crate) tnom: f64,
    pub(crate) cgso: f64,
    pub(crate) cgdo: f64,
    pub(crate) cgbo: f64,
    pub(crate) xpart: f64,
    pub(crate) cFringOut: f64,
    pub(crate) cFringMax: f64,
    pub(crate) sheetResistance: f64,
    pub(crate) jctSatCurDensity: f64,
    pub(crate) jctSidewallSatCurDensity: f64,
    pub(crate) bulkJctPotential: f64,
    pub(crate) bulkJctBotGradingCoeff: f64,
    pub(crate) bulkJctSideGradingCoeff: f64,
    pub(crate) bulkJctGateSideGradingCoeff: f64,
    pub(crate) sidewallJctPotential: f64,
    pub(crate) GatesidewallJctPotential: f64,
    pub(crate) unitAreaJctCap: f64,
    pub(crate) unitLengthSidewallJctCap: f64,
    pub(crate) unitLengthGateSidewallJctCap: f64,
    pub(crate) jctEmissionCoeff: f64,
    pub(crate) jctTempExponent: f64,
    pub(crate) Lint: f64,
    pub(crate) Ll: f64,
    pub(crate) Llc: f64,
    pub(crate) Lln: f64,
    pub(crate) Lw: f64,
    pub(crate) Lwc: f64,
    pub(crate) Lwn: f64,
    pub(crate) Lwl: f64,
    pub(crate) Lwlc: f64,
    pub(crate) Lmin: f64,
    pub(crate) Lmax: f64,
    pub(crate) Wint: f64,
    pub(crate) Wl: f64,
    pub(crate) Wlc: f64,
    pub(crate) Wln: f64,
    pub(crate) Ww: f64,
    pub(crate) Wwc: f64,
    pub(crate) Wwn: f64,
    pub(crate) Wwl: f64,
    pub(crate) Wwlc: f64,
    pub(crate) Wmin: f64,
    pub(crate) Wmax: f64,
    pub(crate) vtm: f64,
    pub(crate) cox: f64,
    pub(crate) cof1: f64,
    pub(crate) cof2: f64,
    pub(crate) cof3: f64,
    pub(crate) cof4: f64,
    pub(crate) vcrit: f64,
    pub(crate) factor1: f64,
    pub(crate) PhiB: f64,
    pub(crate) PhiBSW: f64,
    pub(crate) PhiBSWG: f64,
    pub(crate) jctTempSatCurDensity: f64,
    pub(crate) jctSidewallTempSatCurDensity: f64,
    pub(crate) unitAreaTempJctCap: f64,
    pub(crate) unitLengthSidewallTempJctCap: f64,
    pub(crate) unitLengthGateSidewallTempJctCap: f64,
    pub(crate) oxideTrapDensityA: f64,
    pub(crate) oxideTrapDensityB: f64,
    pub(crate) oxideTrapDensityC: f64,
    pub(crate) em: f64,
    pub(crate) ef: f64,
    pub(crate) af: f64,
    pub(crate) kf: f64,
    pub(crate) lintnoi: f64,
    pub(crate) vgsMax: f64,
    pub(crate) vgdMax: f64,
    pub(crate) vgbMax: f64,
    pub(crate) vdsMax: f64,
    pub(crate) vbsMax: f64,
    pub(crate) vbdMax: f64,
    pub(crate) vgsrMax: f64,
    pub(crate) vgdrMax: f64,
    pub(crate) vgbrMax: f64,
    pub(crate) vbsrMax: f64,
    pub(crate) vbdrMax: f64,
    pub(crate) InstCount: i32,
    pub(crate) mobModGiven: bool,
    pub(crate) binUnitGiven: bool,
    pub(crate) capModGiven: bool,
    pub(crate) acmModGiven: bool,
    pub(crate) calcacmGiven: bool,
    pub(crate) paramChkGiven: bool,
    pub(crate) noiModGiven: bool,
    pub(crate) nqsModGiven: bool,
    pub(crate) acnqsModGiven: bool,
    pub(crate) typeGiven: bool,
    pub(crate) toxGiven: bool,
    pub(crate) versionGiven: bool,
    pub(crate) toxmGiven: bool,
    pub(crate) cdscGiven: bool,
    pub(crate) cdscbGiven: bool,
    pub(crate) cdscdGiven: bool,
    pub(crate) citGiven: bool,
    pub(crate) nfactorGiven: bool,
    pub(crate) xjGiven: bool,
    pub(crate) vsatGiven: bool,
    pub(crate) atGiven: bool,
    pub(crate) a0Given: bool,
    pub(crate) agsGiven: bool,
    pub(crate) a1Given: bool,
    pub(crate) a2Given: bool,
    pub(crate) ketaGiven: bool,
    pub(crate) nsubGiven: bool,
    pub(crate) npeakGiven: bool,
    pub(crate) ngateGiven: bool,
    pub(crate) gamma1Given: bool,
    pub(crate) gamma2Given: bool,
    pub(crate) vbxGiven: bool,
    pub(crate) vbmGiven: bool,
    pub(crate) xtGiven: bool,
    pub(crate) k1Given: bool,
    pub(crate) kt1Given: bool,
    pub(crate) kt1lGiven: bool,
    pub(crate) kt2Given: bool,
    pub(crate) k2Given: bool,
    pub(crate) k3Given: bool,
    pub(crate) k3bGiven: bool,
    pub(crate) w0Given: bool,
    pub(crate) nlxGiven: bool,
    pub(crate) dvt0Given: bool,
    pub(crate) dvt1Given: bool,
    pub(crate) dvt2Given: bool,
    pub(crate) dvt0wGiven: bool,
    pub(crate) dvt1wGiven: bool,
    pub(crate) dvt2wGiven: bool,
    pub(crate) droutGiven: bool,
    pub(crate) dsubGiven: bool,
    pub(crate) vth0Given: bool,
    pub(crate) uaGiven: bool,
    pub(crate) ua1Given: bool,
    pub(crate) ubGiven: bool,
    pub(crate) ub1Given: bool,
    pub(crate) ucGiven: bool,
    pub(crate) uc1Given: bool,
    pub(crate) u0Given: bool,
    pub(crate) uteGiven: bool,
    pub(crate) voffGiven: bool,
    pub(crate) rdswGiven: bool,
    pub(crate) prwgGiven: bool,
    pub(crate) prwbGiven: bool,
    pub(crate) prtGiven: bool,
    pub(crate) eta0Given: bool,
    pub(crate) etabGiven: bool,
    pub(crate) pclmGiven: bool,
    pub(crate) pdibl1Given: bool,
    pub(crate) pdibl2Given: bool,
    pub(crate) pdiblbGiven: bool,
    pub(crate) pscbe1Given: bool,
    pub(crate) pscbe2Given: bool,
    pub(crate) pvagGiven: bool,
    pub(crate) deltaGiven: bool,
    pub(crate) wrGiven: bool,
    pub(crate) dwgGiven: bool,
    pub(crate) dwbGiven: bool,
    pub(crate) b0Given: bool,
    pub(crate) b1Given: bool,
    pub(crate) alpha0Given: bool,
    pub(crate) alpha1Given: bool,
    pub(crate) beta0Given: bool,
    pub(crate) ijthGiven: bool,
    pub(crate) vfbGiven: bool,
    pub(crate) elmGiven: bool,
    pub(crate) cgslGiven: bool,
    pub(crate) cgdlGiven: bool,
    pub(crate) ckappaGiven: bool,
    pub(crate) cfGiven: bool,
    pub(crate) vfbcvGiven: bool,
    pub(crate) clcGiven: bool,
    pub(crate) cleGiven: bool,
    pub(crate) dwcGiven: bool,
    pub(crate) dlcGiven: bool,
    pub(crate) noffGiven: bool,
    pub(crate) voffcvGiven: bool,
    pub(crate) acdeGiven: bool,
    pub(crate) moinGiven: bool,
    pub(crate) tcjGiven: bool,
    pub(crate) tcjswGiven: bool,
    pub(crate) tcjswgGiven: bool,
    pub(crate) tpbGiven: bool,
    pub(crate) tpbswGiven: bool,
    pub(crate) tpbswgGiven: bool,
    pub(crate) xlGiven: bool,
    pub(crate) xwGiven: bool,
    pub(crate) hdifGiven: bool,
    pub(crate) ldifGiven: bool,
    pub(crate) ldGiven: bool,
    pub(crate) rdGiven: bool,
    pub(crate) rsGiven: bool,
    pub(crate) rdcGiven: bool,
    pub(crate) rscGiven: bool,
    pub(crate) wmltGiven: bool,
    pub(crate) lcdscGiven: bool,
    pub(crate) lcdscbGiven: bool,
    pub(crate) lcdscdGiven: bool,
    pub(crate) lcitGiven: bool,
    pub(crate) lnfactorGiven: bool,
    pub(crate) lxjGiven: bool,
    pub(crate) lvsatGiven: bool,
    pub(crate) latGiven: bool,
    pub(crate) la0Given: bool,
    pub(crate) lagsGiven: bool,
    pub(crate) la1Given: bool,
    pub(crate) la2Given: bool,
    pub(crate) lketaGiven: bool,
    pub(crate) lnsubGiven: bool,
    pub(crate) lnpeakGiven: bool,
    pub(crate) lngateGiven: bool,
    pub(crate) lgamma1Given: bool,
    pub(crate) lgamma2Given: bool,
    pub(crate) lvbxGiven: bool,
    pub(crate) lvbmGiven: bool,
    pub(crate) lxtGiven: bool,
    pub(crate) lk1Given: bool,
    pub(crate) lkt1Given: bool,
    pub(crate) lkt1lGiven: bool,
    pub(crate) lkt2Given: bool,
    pub(crate) lk2Given: bool,
    pub(crate) lk3Given: bool,
    pub(crate) lk3bGiven: bool,
    pub(crate) lw0Given: bool,
    pub(crate) lnlxGiven: bool,
    pub(crate) ldvt0Given: bool,
    pub(crate) ldvt1Given: bool,
    pub(crate) ldvt2Given: bool,
    pub(crate) ldvt0wGiven: bool,
    pub(crate) ldvt1wGiven: bool,
    pub(crate) ldvt2wGiven: bool,
    pub(crate) ldroutGiven: bool,
    pub(crate) ldsubGiven: bool,
    pub(crate) lvth0Given: bool,
    pub(crate) luaGiven: bool,
    pub(crate) lua1Given: bool,
    pub(crate) lubGiven: bool,
    pub(crate) lub1Given: bool,
    pub(crate) lucGiven: bool,
    pub(crate) luc1Given: bool,
    pub(crate) lu0Given: bool,
    pub(crate) luteGiven: bool,
    pub(crate) lvoffGiven: bool,
    pub(crate) lrdswGiven: bool,
    pub(crate) lprwgGiven: bool,
    pub(crate) lprwbGiven: bool,
    pub(crate) lprtGiven: bool,
    pub(crate) leta0Given: bool,
    pub(crate) letabGiven: bool,
    pub(crate) lpclmGiven: bool,
    pub(crate) lpdibl1Given: bool,
    pub(crate) lpdibl2Given: bool,
    pub(crate) lpdiblbGiven: bool,
    pub(crate) lpscbe1Given: bool,
    pub(crate) lpscbe2Given: bool,
    pub(crate) lpvagGiven: bool,
    pub(crate) ldeltaGiven: bool,
    pub(crate) lwrGiven: bool,
    pub(crate) ldwgGiven: bool,
    pub(crate) ldwbGiven: bool,
    pub(crate) lb0Given: bool,
    pub(crate) lb1Given: bool,
    pub(crate) lalpha0Given: bool,
    pub(crate) lalpha1Given: bool,
    pub(crate) lbeta0Given: bool,
    pub(crate) lvfbGiven: bool,
    pub(crate) lelmGiven: bool,
    pub(crate) lcgslGiven: bool,
    pub(crate) lcgdlGiven: bool,
    pub(crate) lckappaGiven: bool,
    pub(crate) lcfGiven: bool,
    pub(crate) lclcGiven: bool,
    pub(crate) lcleGiven: bool,
    pub(crate) lvfbcvGiven: bool,
    pub(crate) lnoffGiven: bool,
    pub(crate) lvoffcvGiven: bool,
    pub(crate) lacdeGiven: bool,
    pub(crate) lmoinGiven: bool,
    pub(crate) wcdscGiven: bool,
    pub(crate) wcdscbGiven: bool,
    pub(crate) wcdscdGiven: bool,
    pub(crate) wcitGiven: bool,
    pub(crate) wnfactorGiven: bool,
    pub(crate) wxjGiven: bool,
    pub(crate) wvsatGiven: bool,
    pub(crate) watGiven: bool,
    pub(crate) wa0Given: bool,
    pub(crate) wagsGiven: bool,
    pub(crate) wa1Given: bool,
    pub(crate) wa2Given: bool,
    pub(crate) wketaGiven: bool,
    pub(crate) wnsubGiven: bool,
    pub(crate) wnpeakGiven: bool,
    pub(crate) wngateGiven: bool,
    pub(crate) wgamma1Given: bool,
    pub(crate) wgamma2Given: bool,
    pub(crate) wvbxGiven: bool,
    pub(crate) wvbmGiven: bool,
    pub(crate) wxtGiven: bool,
    pub(crate) wk1Given: bool,
    pub(crate) wkt1Given: bool,
    pub(crate) wkt1lGiven: bool,
    pub(crate) wkt2Given: bool,
    pub(crate) wk2Given: bool,
    pub(crate) wk3Given: bool,
    pub(crate) wk3bGiven: bool,
    pub(crate) ww0Given: bool,
    pub(crate) wnlxGiven: bool,
    pub(crate) wdvt0Given: bool,
    pub(crate) wdvt1Given: bool,
    pub(crate) wdvt2Given: bool,
    pub(crate) wdvt0wGiven: bool,
    pub(crate) wdvt1wGiven: bool,
    pub(crate) wdvt2wGiven: bool,
    pub(crate) wdroutGiven: bool,
    pub(crate) wdsubGiven: bool,
    pub(crate) wvth0Given: bool,
    pub(crate) wuaGiven: bool,
    pub(crate) wua1Given: bool,
    pub(crate) wubGiven: bool,
    pub(crate) wub1Given: bool,
    pub(crate) wucGiven: bool,
    pub(crate) wuc1Given: bool,
    pub(crate) wu0Given: bool,
    pub(crate) wuteGiven: bool,
    pub(crate) wvoffGiven: bool,
    pub(crate) wrdswGiven: bool,
    pub(crate) wprwgGiven: bool,
    pub(crate) wprwbGiven: bool,
    pub(crate) wprtGiven: bool,
    pub(crate) weta0Given: bool,
    pub(crate) wetabGiven: bool,
    pub(crate) wpclmGiven: bool,
    pub(crate) wpdibl1Given: bool,
    pub(crate) wpdibl2Given: bool,
    pub(crate) wpdiblbGiven: bool,
    pub(crate) wpscbe1Given: bool,
    pub(crate) wpscbe2Given: bool,
    pub(crate) wpvagGiven: bool,
    pub(crate) wdeltaGiven: bool,
    pub(crate) wwrGiven: bool,
    pub(crate) wdwgGiven: bool,
    pub(crate) wdwbGiven: bool,
    pub(crate) wb0Given: bool,
    pub(crate) wb1Given: bool,
    pub(crate) walpha0Given: bool,
    pub(crate) walpha1Given: bool,
    pub(crate) wbeta0Given: bool,
    pub(crate) wvfbGiven: bool,
    pub(crate) welmGiven: bool,
    pub(crate) wcgslGiven: bool,
    pub(crate) wcgdlGiven: bool,
    pub(crate) wckappaGiven: bool,
    pub(crate) wcfGiven: bool,
    pub(crate) wclcGiven: bool,
    pub(crate) wcleGiven: bool,
    pub(crate) wvfbcvGiven: bool,
    pub(crate) wnoffGiven: bool,
    pub(crate) wvoffcvGiven: bool,
    pub(crate) wacdeGiven: bool,
    pub(crate) wmoinGiven: bool,
    pub(crate) pcdscGiven: bool,
    pub(crate) pcdscbGiven: bool,
    pub(crate) pcdscdGiven: bool,
    pub(crate) pcitGiven: bool,
    pub(crate) pnfactorGiven: bool,
    pub(crate) pxjGiven: bool,
    pub(crate) pvsatGiven: bool,
    pub(crate) patGiven: bool,
    pub(crate) pa0Given: bool,
    pub(crate) pagsGiven: bool,
    pub(crate) pa1Given: bool,
    pub(crate) pa2Given: bool,
    pub(crate) pketaGiven: bool,
    pub(crate) pnsubGiven: bool,
    pub(crate) pnpeakGiven: bool,
    pub(crate) pngateGiven: bool,
    pub(crate) pgamma1Given: bool,
    pub(crate) pgamma2Given: bool,
    pub(crate) pvbxGiven: bool,
    pub(crate) pvbmGiven: bool,
    pub(crate) pxtGiven: bool,
    pub(crate) pk1Given: bool,
    pub(crate) pkt1Given: bool,
    pub(crate) pkt1lGiven: bool,
    pub(crate) pkt2Given: bool,
    pub(crate) pk2Given: bool,
    pub(crate) pk3Given: bool,
    pub(crate) pk3bGiven: bool,
    pub(crate) pw0Given: bool,
    pub(crate) pnlxGiven: bool,
    pub(crate) pdvt0Given: bool,
    pub(crate) pdvt1Given: bool,
    pub(crate) pdvt2Given: bool,
    pub(crate) pdvt0wGiven: bool,
    pub(crate) pdvt1wGiven: bool,
    pub(crate) pdvt2wGiven: bool,
    pub(crate) pdroutGiven: bool,
    pub(crate) pdsubGiven: bool,
    pub(crate) pvth0Given: bool,
    pub(crate) puaGiven: bool,
    pub(crate) pua1Given: bool,
    pub(crate) pubGiven: bool,
    pub(crate) pub1Given: bool,
    pub(crate) pucGiven: bool,
    pub(crate) puc1Given: bool,
    pub(crate) pu0Given: bool,
    pub(crate) puteGiven: bool,
    pub(crate) pvoffGiven: bool,
    pub(crate) prdswGiven: bool,
    pub(crate) pprwgGiven: bool,
    pub(crate) pprwbGiven: bool,
    pub(crate) pprtGiven: bool,
    pub(crate) peta0Given: bool,
    pub(crate) petabGiven: bool,
    pub(crate) ppclmGiven: bool,
    pub(crate) ppdibl1Given: bool,
    pub(crate) ppdibl2Given: bool,
    pub(crate) ppdiblbGiven: bool,
    pub(crate) ppscbe1Given: bool,
    pub(crate) ppscbe2Given: bool,
    pub(crate) ppvagGiven: bool,
    pub(crate) pdeltaGiven: bool,
    pub(crate) pwrGiven: bool,
    pub(crate) pdwgGiven: bool,
    pub(crate) pdwbGiven: bool,
    pub(crate) pb0Given: bool,
    pub(crate) pb1Given: bool,
    pub(crate) palpha0Given: bool,
    pub(crate) palpha1Given: bool,
    pub(crate) pbeta0Given: bool,
    pub(crate) pvfbGiven: bool,
    pub(crate) pelmGiven: bool,
    pub(crate) pcgslGiven: bool,
    pub(crate) pcgdlGiven: bool,
    pub(crate) pckappaGiven: bool,
    pub(crate) pcfGiven: bool,
    pub(crate) pclcGiven: bool,
    pub(crate) pcleGiven: bool,
    pub(crate) pvfbcvGiven: bool,
    pub(crate) pnoffGiven: bool,
    pub(crate) pvoffcvGiven: bool,
    pub(crate) pacdeGiven: bool,
    pub(crate) pmoinGiven: bool,
    pub(crate) useFringeGiven: bool,
    pub(crate) tnomGiven: bool,
    pub(crate) cgsoGiven: bool,
    pub(crate) cgdoGiven: bool,
    pub(crate) cgboGiven: bool,
    pub(crate) xpartGiven: bool,
    pub(crate) sheetResistanceGiven: bool,
    pub(crate) jctSatCurDensityGiven: bool,
    pub(crate) jctSidewallSatCurDensityGiven: bool,
    pub(crate) bulkJctPotentialGiven: bool,
    pub(crate) bulkJctBotGradingCoeffGiven: bool,
    pub(crate) sidewallJctPotentialGiven: bool,
    pub(crate) GatesidewallJctPotentialGiven: bool,
    pub(crate) bulkJctSideGradingCoeffGiven: bool,
    pub(crate) unitAreaJctCapGiven: bool,
    pub(crate) unitLengthSidewallJctCapGiven: bool,
    pub(crate) bulkJctGateSideGradingCoeffGiven: bool,
    pub(crate) unitLengthGateSidewallJctCapGiven: bool,
    pub(crate) jctEmissionCoeffGiven: bool,
    pub(crate) jctTempExponentGiven: bool,
    pub(crate) oxideTrapDensityAGiven: bool,
    pub(crate) oxideTrapDensityBGiven: bool,
    pub(crate) oxideTrapDensityCGiven: bool,
    pub(crate) emGiven: bool,
    pub(crate) efGiven: bool,
    pub(crate) afGiven: bool,
    pub(crate) kfGiven: bool,
    pub(crate) lintnoiGiven: bool,
    pub(crate) vgsMaxGiven: bool,
    pub(crate) vgdMaxGiven: bool,
    pub(crate) vgbMaxGiven: bool,
    pub(crate) vdsMaxGiven: bool,
    pub(crate) vbsMaxGiven: bool,
    pub(crate) vbdMaxGiven: bool,
    pub(crate) vgsrMaxGiven: bool,
    pub(crate) vgdrMaxGiven: bool,
    pub(crate) vgbrMaxGiven: bool,
    pub(crate) vbsrMaxGiven: bool,
    pub(crate) vbdrMaxGiven: bool,
    pub(crate) LintGiven: bool,
    pub(crate) LlGiven: bool,
    pub(crate) LlcGiven: bool,
    pub(crate) LlnGiven: bool,
    pub(crate) LwGiven: bool,
    pub(crate) LwcGiven: bool,
    pub(crate) LwnGiven: bool,
    pub(crate) LwlGiven: bool,
    pub(crate) LwlcGiven: bool,
    pub(crate) LminGiven: bool,
    pub(crate) LmaxGiven: bool,
    pub(crate) WintGiven: bool,
    pub(crate) WlGiven: bool,
    pub(crate) WlcGiven: bool,
    pub(crate) WlnGiven: bool,
    pub(crate) WwGiven: bool,
    pub(crate) WwcGiven: bool,
    pub(crate) WwnGiven: bool,
    pub(crate) WwlGiven: bool,
    pub(crate) WwlcGiven: bool,
    pub(crate) WminGiven: bool,
    pub(crate) WmaxGiven: bool,
}

#[rustfmt::skip]
impl Model {
    /// The `f64` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn real_mut(&mut self, c_name: &str) -> Option<&mut f64> {
        Some(match c_name {
            "BSIM3tox" => &mut self.tox,
            "BSIM3toxm" => &mut self.toxm,
            "BSIM3cdsc" => &mut self.cdsc,
            "BSIM3cdscb" => &mut self.cdscb,
            "BSIM3cdscd" => &mut self.cdscd,
            "BSIM3cit" => &mut self.cit,
            "BSIM3nfactor" => &mut self.nfactor,
            "BSIM3xj" => &mut self.xj,
            "BSIM3vsat" => &mut self.vsat,
            "BSIM3at" => &mut self.at,
            "BSIM3a0" => &mut self.a0,
            "BSIM3ags" => &mut self.ags,
            "BSIM3a1" => &mut self.a1,
            "BSIM3a2" => &mut self.a2,
            "BSIM3keta" => &mut self.keta,
            "BSIM3nsub" => &mut self.nsub,
            "BSIM3npeak" => &mut self.npeak,
            "BSIM3ngate" => &mut self.ngate,
            "BSIM3gamma1" => &mut self.gamma1,
            "BSIM3gamma2" => &mut self.gamma2,
            "BSIM3vbx" => &mut self.vbx,
            "BSIM3vbm" => &mut self.vbm,
            "BSIM3xt" => &mut self.xt,
            "BSIM3k1" => &mut self.k1,
            "BSIM3kt1" => &mut self.kt1,
            "BSIM3kt1l" => &mut self.kt1l,
            "BSIM3kt2" => &mut self.kt2,
            "BSIM3k2" => &mut self.k2,
            "BSIM3k3" => &mut self.k3,
            "BSIM3k3b" => &mut self.k3b,
            "BSIM3w0" => &mut self.w0,
            "BSIM3nlx" => &mut self.nlx,
            "BSIM3dvt0" => &mut self.dvt0,
            "BSIM3dvt1" => &mut self.dvt1,
            "BSIM3dvt2" => &mut self.dvt2,
            "BSIM3dvt0w" => &mut self.dvt0w,
            "BSIM3dvt1w" => &mut self.dvt1w,
            "BSIM3dvt2w" => &mut self.dvt2w,
            "BSIM3drout" => &mut self.drout,
            "BSIM3dsub" => &mut self.dsub,
            "BSIM3vth0" => &mut self.vth0,
            "BSIM3ua" => &mut self.ua,
            "BSIM3ua1" => &mut self.ua1,
            "BSIM3ub" => &mut self.ub,
            "BSIM3ub1" => &mut self.ub1,
            "BSIM3uc" => &mut self.uc,
            "BSIM3uc1" => &mut self.uc1,
            "BSIM3u0" => &mut self.u0,
            "BSIM3ute" => &mut self.ute,
            "BSIM3voff" => &mut self.voff,
            "BSIM3delta" => &mut self.delta,
            "BSIM3rdsw" => &mut self.rdsw,
            "BSIM3prwg" => &mut self.prwg,
            "BSIM3prwb" => &mut self.prwb,
            "BSIM3prt" => &mut self.prt,
            "BSIM3eta0" => &mut self.eta0,
            "BSIM3etab" => &mut self.etab,
            "BSIM3pclm" => &mut self.pclm,
            "BSIM3pdibl1" => &mut self.pdibl1,
            "BSIM3pdibl2" => &mut self.pdibl2,
            "BSIM3pdiblb" => &mut self.pdiblb,
            "BSIM3pscbe1" => &mut self.pscbe1,
            "BSIM3pscbe2" => &mut self.pscbe2,
            "BSIM3pvag" => &mut self.pvag,
            "BSIM3wr" => &mut self.wr,
            "BSIM3dwg" => &mut self.dwg,
            "BSIM3dwb" => &mut self.dwb,
            "BSIM3b0" => &mut self.b0,
            "BSIM3b1" => &mut self.b1,
            "BSIM3alpha0" => &mut self.alpha0,
            "BSIM3alpha1" => &mut self.alpha1,
            "BSIM3beta0" => &mut self.beta0,
            "BSIM3ijth" => &mut self.ijth,
            "BSIM3vfb" => &mut self.vfb,
            "BSIM3elm" => &mut self.elm,
            "BSIM3cgsl" => &mut self.cgsl,
            "BSIM3cgdl" => &mut self.cgdl,
            "BSIM3ckappa" => &mut self.ckappa,
            "BSIM3cf" => &mut self.cf,
            "BSIM3vfbcv" => &mut self.vfbcv,
            "BSIM3clc" => &mut self.clc,
            "BSIM3cle" => &mut self.cle,
            "BSIM3dwc" => &mut self.dwc,
            "BSIM3dlc" => &mut self.dlc,
            "BSIM3noff" => &mut self.noff,
            "BSIM3voffcv" => &mut self.voffcv,
            "BSIM3acde" => &mut self.acde,
            "BSIM3moin" => &mut self.moin,
            "BSIM3tcj" => &mut self.tcj,
            "BSIM3tcjsw" => &mut self.tcjsw,
            "BSIM3tcjswg" => &mut self.tcjswg,
            "BSIM3tpb" => &mut self.tpb,
            "BSIM3tpbsw" => &mut self.tpbsw,
            "BSIM3tpbswg" => &mut self.tpbswg,
            "BSIM3xl" => &mut self.xl,
            "BSIM3xw" => &mut self.xw,
            "BSIM3hdif" => &mut self.hdif,
            "BSIM3ldif" => &mut self.ldif,
            "BSIM3ld" => &mut self.ld,
            "BSIM3rd" => &mut self.rd,
            "BSIM3rs" => &mut self.rs,
            "BSIM3rdc" => &mut self.rdc,
            "BSIM3rsc" => &mut self.rsc,
            "BSIM3wmlt" => &mut self.wmlt,
            "BSIM3lcdsc" => &mut self.lcdsc,
            "BSIM3lcdscb" => &mut self.lcdscb,
            "BSIM3lcdscd" => &mut self.lcdscd,
            "BSIM3lcit" => &mut self.lcit,
            "BSIM3lnfactor" => &mut self.lnfactor,
            "BSIM3lxj" => &mut self.lxj,
            "BSIM3lvsat" => &mut self.lvsat,
            "BSIM3lat" => &mut self.lat,
            "BSIM3la0" => &mut self.la0,
            "BSIM3lags" => &mut self.lags,
            "BSIM3la1" => &mut self.la1,
            "BSIM3la2" => &mut self.la2,
            "BSIM3lketa" => &mut self.lketa,
            "BSIM3lnsub" => &mut self.lnsub,
            "BSIM3lnpeak" => &mut self.lnpeak,
            "BSIM3lngate" => &mut self.lngate,
            "BSIM3lgamma1" => &mut self.lgamma1,
            "BSIM3lgamma2" => &mut self.lgamma2,
            "BSIM3lvbx" => &mut self.lvbx,
            "BSIM3lvbm" => &mut self.lvbm,
            "BSIM3lxt" => &mut self.lxt,
            "BSIM3lk1" => &mut self.lk1,
            "BSIM3lkt1" => &mut self.lkt1,
            "BSIM3lkt1l" => &mut self.lkt1l,
            "BSIM3lkt2" => &mut self.lkt2,
            "BSIM3lk2" => &mut self.lk2,
            "BSIM3lk3" => &mut self.lk3,
            "BSIM3lk3b" => &mut self.lk3b,
            "BSIM3lw0" => &mut self.lw0,
            "BSIM3lnlx" => &mut self.lnlx,
            "BSIM3ldvt0" => &mut self.ldvt0,
            "BSIM3ldvt1" => &mut self.ldvt1,
            "BSIM3ldvt2" => &mut self.ldvt2,
            "BSIM3ldvt0w" => &mut self.ldvt0w,
            "BSIM3ldvt1w" => &mut self.ldvt1w,
            "BSIM3ldvt2w" => &mut self.ldvt2w,
            "BSIM3ldrout" => &mut self.ldrout,
            "BSIM3ldsub" => &mut self.ldsub,
            "BSIM3lvth0" => &mut self.lvth0,
            "BSIM3lua" => &mut self.lua,
            "BSIM3lua1" => &mut self.lua1,
            "BSIM3lub" => &mut self.lub,
            "BSIM3lub1" => &mut self.lub1,
            "BSIM3luc" => &mut self.luc,
            "BSIM3luc1" => &mut self.luc1,
            "BSIM3lu0" => &mut self.lu0,
            "BSIM3lute" => &mut self.lute,
            "BSIM3lvoff" => &mut self.lvoff,
            "BSIM3ldelta" => &mut self.ldelta,
            "BSIM3lrdsw" => &mut self.lrdsw,
            "BSIM3lprwg" => &mut self.lprwg,
            "BSIM3lprwb" => &mut self.lprwb,
            "BSIM3lprt" => &mut self.lprt,
            "BSIM3leta0" => &mut self.leta0,
            "BSIM3letab" => &mut self.letab,
            "BSIM3lpclm" => &mut self.lpclm,
            "BSIM3lpdibl1" => &mut self.lpdibl1,
            "BSIM3lpdibl2" => &mut self.lpdibl2,
            "BSIM3lpdiblb" => &mut self.lpdiblb,
            "BSIM3lpscbe1" => &mut self.lpscbe1,
            "BSIM3lpscbe2" => &mut self.lpscbe2,
            "BSIM3lpvag" => &mut self.lpvag,
            "BSIM3lwr" => &mut self.lwr,
            "BSIM3ldwg" => &mut self.ldwg,
            "BSIM3ldwb" => &mut self.ldwb,
            "BSIM3lb0" => &mut self.lb0,
            "BSIM3lb1" => &mut self.lb1,
            "BSIM3lalpha0" => &mut self.lalpha0,
            "BSIM3lalpha1" => &mut self.lalpha1,
            "BSIM3lbeta0" => &mut self.lbeta0,
            "BSIM3lvfb" => &mut self.lvfb,
            "BSIM3lelm" => &mut self.lelm,
            "BSIM3lcgsl" => &mut self.lcgsl,
            "BSIM3lcgdl" => &mut self.lcgdl,
            "BSIM3lckappa" => &mut self.lckappa,
            "BSIM3lcf" => &mut self.lcf,
            "BSIM3lclc" => &mut self.lclc,
            "BSIM3lcle" => &mut self.lcle,
            "BSIM3lvfbcv" => &mut self.lvfbcv,
            "BSIM3lnoff" => &mut self.lnoff,
            "BSIM3lvoffcv" => &mut self.lvoffcv,
            "BSIM3lacde" => &mut self.lacde,
            "BSIM3lmoin" => &mut self.lmoin,
            "BSIM3wcdsc" => &mut self.wcdsc,
            "BSIM3wcdscb" => &mut self.wcdscb,
            "BSIM3wcdscd" => &mut self.wcdscd,
            "BSIM3wcit" => &mut self.wcit,
            "BSIM3wnfactor" => &mut self.wnfactor,
            "BSIM3wxj" => &mut self.wxj,
            "BSIM3wvsat" => &mut self.wvsat,
            "BSIM3wat" => &mut self.wat,
            "BSIM3wa0" => &mut self.wa0,
            "BSIM3wags" => &mut self.wags,
            "BSIM3wa1" => &mut self.wa1,
            "BSIM3wa2" => &mut self.wa2,
            "BSIM3wketa" => &mut self.wketa,
            "BSIM3wnsub" => &mut self.wnsub,
            "BSIM3wnpeak" => &mut self.wnpeak,
            "BSIM3wngate" => &mut self.wngate,
            "BSIM3wgamma1" => &mut self.wgamma1,
            "BSIM3wgamma2" => &mut self.wgamma2,
            "BSIM3wvbx" => &mut self.wvbx,
            "BSIM3wvbm" => &mut self.wvbm,
            "BSIM3wxt" => &mut self.wxt,
            "BSIM3wk1" => &mut self.wk1,
            "BSIM3wkt1" => &mut self.wkt1,
            "BSIM3wkt1l" => &mut self.wkt1l,
            "BSIM3wkt2" => &mut self.wkt2,
            "BSIM3wk2" => &mut self.wk2,
            "BSIM3wk3" => &mut self.wk3,
            "BSIM3wk3b" => &mut self.wk3b,
            "BSIM3ww0" => &mut self.ww0,
            "BSIM3wnlx" => &mut self.wnlx,
            "BSIM3wdvt0" => &mut self.wdvt0,
            "BSIM3wdvt1" => &mut self.wdvt1,
            "BSIM3wdvt2" => &mut self.wdvt2,
            "BSIM3wdvt0w" => &mut self.wdvt0w,
            "BSIM3wdvt1w" => &mut self.wdvt1w,
            "BSIM3wdvt2w" => &mut self.wdvt2w,
            "BSIM3wdrout" => &mut self.wdrout,
            "BSIM3wdsub" => &mut self.wdsub,
            "BSIM3wvth0" => &mut self.wvth0,
            "BSIM3wua" => &mut self.wua,
            "BSIM3wua1" => &mut self.wua1,
            "BSIM3wub" => &mut self.wub,
            "BSIM3wub1" => &mut self.wub1,
            "BSIM3wuc" => &mut self.wuc,
            "BSIM3wuc1" => &mut self.wuc1,
            "BSIM3wu0" => &mut self.wu0,
            "BSIM3wute" => &mut self.wute,
            "BSIM3wvoff" => &mut self.wvoff,
            "BSIM3wdelta" => &mut self.wdelta,
            "BSIM3wrdsw" => &mut self.wrdsw,
            "BSIM3wprwg" => &mut self.wprwg,
            "BSIM3wprwb" => &mut self.wprwb,
            "BSIM3wprt" => &mut self.wprt,
            "BSIM3weta0" => &mut self.weta0,
            "BSIM3wetab" => &mut self.wetab,
            "BSIM3wpclm" => &mut self.wpclm,
            "BSIM3wpdibl1" => &mut self.wpdibl1,
            "BSIM3wpdibl2" => &mut self.wpdibl2,
            "BSIM3wpdiblb" => &mut self.wpdiblb,
            "BSIM3wpscbe1" => &mut self.wpscbe1,
            "BSIM3wpscbe2" => &mut self.wpscbe2,
            "BSIM3wpvag" => &mut self.wpvag,
            "BSIM3wwr" => &mut self.wwr,
            "BSIM3wdwg" => &mut self.wdwg,
            "BSIM3wdwb" => &mut self.wdwb,
            "BSIM3wb0" => &mut self.wb0,
            "BSIM3wb1" => &mut self.wb1,
            "BSIM3walpha0" => &mut self.walpha0,
            "BSIM3walpha1" => &mut self.walpha1,
            "BSIM3wbeta0" => &mut self.wbeta0,
            "BSIM3wvfb" => &mut self.wvfb,
            "BSIM3welm" => &mut self.welm,
            "BSIM3wcgsl" => &mut self.wcgsl,
            "BSIM3wcgdl" => &mut self.wcgdl,
            "BSIM3wckappa" => &mut self.wckappa,
            "BSIM3wcf" => &mut self.wcf,
            "BSIM3wclc" => &mut self.wclc,
            "BSIM3wcle" => &mut self.wcle,
            "BSIM3wvfbcv" => &mut self.wvfbcv,
            "BSIM3wnoff" => &mut self.wnoff,
            "BSIM3wvoffcv" => &mut self.wvoffcv,
            "BSIM3wacde" => &mut self.wacde,
            "BSIM3wmoin" => &mut self.wmoin,
            "BSIM3pcdsc" => &mut self.pcdsc,
            "BSIM3pcdscb" => &mut self.pcdscb,
            "BSIM3pcdscd" => &mut self.pcdscd,
            "BSIM3pcit" => &mut self.pcit,
            "BSIM3pnfactor" => &mut self.pnfactor,
            "BSIM3pxj" => &mut self.pxj,
            "BSIM3pvsat" => &mut self.pvsat,
            "BSIM3pat" => &mut self.pat,
            "BSIM3pa0" => &mut self.pa0,
            "BSIM3pags" => &mut self.pags,
            "BSIM3pa1" => &mut self.pa1,
            "BSIM3pa2" => &mut self.pa2,
            "BSIM3pketa" => &mut self.pketa,
            "BSIM3pnsub" => &mut self.pnsub,
            "BSIM3pnpeak" => &mut self.pnpeak,
            "BSIM3pngate" => &mut self.pngate,
            "BSIM3pgamma1" => &mut self.pgamma1,
            "BSIM3pgamma2" => &mut self.pgamma2,
            "BSIM3pvbx" => &mut self.pvbx,
            "BSIM3pvbm" => &mut self.pvbm,
            "BSIM3pxt" => &mut self.pxt,
            "BSIM3pk1" => &mut self.pk1,
            "BSIM3pkt1" => &mut self.pkt1,
            "BSIM3pkt1l" => &mut self.pkt1l,
            "BSIM3pkt2" => &mut self.pkt2,
            "BSIM3pk2" => &mut self.pk2,
            "BSIM3pk3" => &mut self.pk3,
            "BSIM3pk3b" => &mut self.pk3b,
            "BSIM3pw0" => &mut self.pw0,
            "BSIM3pnlx" => &mut self.pnlx,
            "BSIM3pdvt0" => &mut self.pdvt0,
            "BSIM3pdvt1" => &mut self.pdvt1,
            "BSIM3pdvt2" => &mut self.pdvt2,
            "BSIM3pdvt0w" => &mut self.pdvt0w,
            "BSIM3pdvt1w" => &mut self.pdvt1w,
            "BSIM3pdvt2w" => &mut self.pdvt2w,
            "BSIM3pdrout" => &mut self.pdrout,
            "BSIM3pdsub" => &mut self.pdsub,
            "BSIM3pvth0" => &mut self.pvth0,
            "BSIM3pua" => &mut self.pua,
            "BSIM3pua1" => &mut self.pua1,
            "BSIM3pub" => &mut self.r#pub,
            "BSIM3pub1" => &mut self.pub1,
            "BSIM3puc" => &mut self.puc,
            "BSIM3puc1" => &mut self.puc1,
            "BSIM3pu0" => &mut self.pu0,
            "BSIM3pute" => &mut self.pute,
            "BSIM3pvoff" => &mut self.pvoff,
            "BSIM3pdelta" => &mut self.pdelta,
            "BSIM3prdsw" => &mut self.prdsw,
            "BSIM3pprwg" => &mut self.pprwg,
            "BSIM3pprwb" => &mut self.pprwb,
            "BSIM3pprt" => &mut self.pprt,
            "BSIM3peta0" => &mut self.peta0,
            "BSIM3petab" => &mut self.petab,
            "BSIM3ppclm" => &mut self.ppclm,
            "BSIM3ppdibl1" => &mut self.ppdibl1,
            "BSIM3ppdibl2" => &mut self.ppdibl2,
            "BSIM3ppdiblb" => &mut self.ppdiblb,
            "BSIM3ppscbe1" => &mut self.ppscbe1,
            "BSIM3ppscbe2" => &mut self.ppscbe2,
            "BSIM3ppvag" => &mut self.ppvag,
            "BSIM3pwr" => &mut self.pwr,
            "BSIM3pdwg" => &mut self.pdwg,
            "BSIM3pdwb" => &mut self.pdwb,
            "BSIM3pb0" => &mut self.pb0,
            "BSIM3pb1" => &mut self.pb1,
            "BSIM3palpha0" => &mut self.palpha0,
            "BSIM3palpha1" => &mut self.palpha1,
            "BSIM3pbeta0" => &mut self.pbeta0,
            "BSIM3pvfb" => &mut self.pvfb,
            "BSIM3pelm" => &mut self.pelm,
            "BSIM3pcgsl" => &mut self.pcgsl,
            "BSIM3pcgdl" => &mut self.pcgdl,
            "BSIM3pckappa" => &mut self.pckappa,
            "BSIM3pcf" => &mut self.pcf,
            "BSIM3pclc" => &mut self.pclc,
            "BSIM3pcle" => &mut self.pcle,
            "BSIM3pvfbcv" => &mut self.pvfbcv,
            "BSIM3pnoff" => &mut self.pnoff,
            "BSIM3pvoffcv" => &mut self.pvoffcv,
            "BSIM3pacde" => &mut self.pacde,
            "BSIM3pmoin" => &mut self.pmoin,
            "BSIM3tnom" => &mut self.tnom,
            "BSIM3cgso" => &mut self.cgso,
            "BSIM3cgdo" => &mut self.cgdo,
            "BSIM3cgbo" => &mut self.cgbo,
            "BSIM3xpart" => &mut self.xpart,
            "BSIM3cFringOut" => &mut self.cFringOut,
            "BSIM3cFringMax" => &mut self.cFringMax,
            "BSIM3sheetResistance" => &mut self.sheetResistance,
            "BSIM3jctSatCurDensity" => &mut self.jctSatCurDensity,
            "BSIM3jctSidewallSatCurDensity" => &mut self.jctSidewallSatCurDensity,
            "BSIM3bulkJctPotential" => &mut self.bulkJctPotential,
            "BSIM3bulkJctBotGradingCoeff" => &mut self.bulkJctBotGradingCoeff,
            "BSIM3bulkJctSideGradingCoeff" => &mut self.bulkJctSideGradingCoeff,
            "BSIM3bulkJctGateSideGradingCoeff" => &mut self.bulkJctGateSideGradingCoeff,
            "BSIM3sidewallJctPotential" => &mut self.sidewallJctPotential,
            "BSIM3GatesidewallJctPotential" => &mut self.GatesidewallJctPotential,
            "BSIM3unitAreaJctCap" => &mut self.unitAreaJctCap,
            "BSIM3unitLengthSidewallJctCap" => &mut self.unitLengthSidewallJctCap,
            "BSIM3unitLengthGateSidewallJctCap" => &mut self.unitLengthGateSidewallJctCap,
            "BSIM3jctEmissionCoeff" => &mut self.jctEmissionCoeff,
            "BSIM3jctTempExponent" => &mut self.jctTempExponent,
            "BSIM3Lint" => &mut self.Lint,
            "BSIM3Ll" => &mut self.Ll,
            "BSIM3Llc" => &mut self.Llc,
            "BSIM3Lln" => &mut self.Lln,
            "BSIM3Lw" => &mut self.Lw,
            "BSIM3Lwc" => &mut self.Lwc,
            "BSIM3Lwn" => &mut self.Lwn,
            "BSIM3Lwl" => &mut self.Lwl,
            "BSIM3Lwlc" => &mut self.Lwlc,
            "BSIM3Lmin" => &mut self.Lmin,
            "BSIM3Lmax" => &mut self.Lmax,
            "BSIM3Wint" => &mut self.Wint,
            "BSIM3Wl" => &mut self.Wl,
            "BSIM3Wlc" => &mut self.Wlc,
            "BSIM3Wln" => &mut self.Wln,
            "BSIM3Ww" => &mut self.Ww,
            "BSIM3Wwc" => &mut self.Wwc,
            "BSIM3Wwn" => &mut self.Wwn,
            "BSIM3Wwl" => &mut self.Wwl,
            "BSIM3Wwlc" => &mut self.Wwlc,
            "BSIM3Wmin" => &mut self.Wmin,
            "BSIM3Wmax" => &mut self.Wmax,
            "BSIM3vtm" => &mut self.vtm,
            "BSIM3cox" => &mut self.cox,
            "BSIM3cof1" => &mut self.cof1,
            "BSIM3cof2" => &mut self.cof2,
            "BSIM3cof3" => &mut self.cof3,
            "BSIM3cof4" => &mut self.cof4,
            "BSIM3vcrit" => &mut self.vcrit,
            "BSIM3factor1" => &mut self.factor1,
            "BSIM3PhiB" => &mut self.PhiB,
            "BSIM3PhiBSW" => &mut self.PhiBSW,
            "BSIM3PhiBSWG" => &mut self.PhiBSWG,
            "BSIM3jctTempSatCurDensity" => &mut self.jctTempSatCurDensity,
            "BSIM3jctSidewallTempSatCurDensity" => &mut self.jctSidewallTempSatCurDensity,
            "BSIM3unitAreaTempJctCap" => &mut self.unitAreaTempJctCap,
            "BSIM3unitLengthSidewallTempJctCap" => &mut self.unitLengthSidewallTempJctCap,
            "BSIM3unitLengthGateSidewallTempJctCap" => &mut self.unitLengthGateSidewallTempJctCap,
            "BSIM3oxideTrapDensityA" => &mut self.oxideTrapDensityA,
            "BSIM3oxideTrapDensityB" => &mut self.oxideTrapDensityB,
            "BSIM3oxideTrapDensityC" => &mut self.oxideTrapDensityC,
            "BSIM3em" => &mut self.em,
            "BSIM3ef" => &mut self.ef,
            "BSIM3af" => &mut self.af,
            "BSIM3kf" => &mut self.kf,
            "BSIM3lintnoi" => &mut self.lintnoi,
            "BSIM3vgsMax" => &mut self.vgsMax,
            "BSIM3vgdMax" => &mut self.vgdMax,
            "BSIM3vgbMax" => &mut self.vgbMax,
            "BSIM3vdsMax" => &mut self.vdsMax,
            "BSIM3vbsMax" => &mut self.vbsMax,
            "BSIM3vbdMax" => &mut self.vbdMax,
            "BSIM3vgsrMax" => &mut self.vgsrMax,
            "BSIM3vgdrMax" => &mut self.vgdrMax,
            "BSIM3vgbrMax" => &mut self.vgbrMax,
            "BSIM3vbsrMax" => &mut self.vbsrMax,
            "BSIM3vbdrMax" => &mut self.vbdrMax,
            _ => return None,
        })
    }
    /// The `i32` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn int_mut(&mut self, c_name: &str) -> Option<&mut i32> {
        Some(match c_name {
            "BSIM3type" => &mut self.r#type,
            "BSIM3mobMod" => &mut self.mobMod,
            "BSIM3capMod" => &mut self.capMod,
            "BSIM3acmMod" => &mut self.acmMod,
            "BSIM3calcacm" => &mut self.calcacm,
            "BSIM3noiMod" => &mut self.noiMod,
            "BSIM3nqsMod" => &mut self.nqsMod,
            "BSIM3acnqsMod" => &mut self.acnqsMod,
            "BSIM3binUnit" => &mut self.binUnit,
            "BSIM3paramChk" => &mut self.paramChk,
            "BSIM3InstCount" => &mut self.InstCount,
            _ => return None,
        })
    }
    /// The `bool` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn flag_mut(&mut self, c_name: &str) -> Option<&mut bool> {
        Some(match c_name {
            "BSIM3mobModGiven" => &mut self.mobModGiven,
            "BSIM3binUnitGiven" => &mut self.binUnitGiven,
            "BSIM3capModGiven" => &mut self.capModGiven,
            "BSIM3acmModGiven" => &mut self.acmModGiven,
            "BSIM3calcacmGiven" => &mut self.calcacmGiven,
            "BSIM3paramChkGiven" => &mut self.paramChkGiven,
            "BSIM3noiModGiven" => &mut self.noiModGiven,
            "BSIM3nqsModGiven" => &mut self.nqsModGiven,
            "BSIM3acnqsModGiven" => &mut self.acnqsModGiven,
            "BSIM3typeGiven" => &mut self.typeGiven,
            "BSIM3toxGiven" => &mut self.toxGiven,
            "BSIM3versionGiven" => &mut self.versionGiven,
            "BSIM3toxmGiven" => &mut self.toxmGiven,
            "BSIM3cdscGiven" => &mut self.cdscGiven,
            "BSIM3cdscbGiven" => &mut self.cdscbGiven,
            "BSIM3cdscdGiven" => &mut self.cdscdGiven,
            "BSIM3citGiven" => &mut self.citGiven,
            "BSIM3nfactorGiven" => &mut self.nfactorGiven,
            "BSIM3xjGiven" => &mut self.xjGiven,
            "BSIM3vsatGiven" => &mut self.vsatGiven,
            "BSIM3atGiven" => &mut self.atGiven,
            "BSIM3a0Given" => &mut self.a0Given,
            "BSIM3agsGiven" => &mut self.agsGiven,
            "BSIM3a1Given" => &mut self.a1Given,
            "BSIM3a2Given" => &mut self.a2Given,
            "BSIM3ketaGiven" => &mut self.ketaGiven,
            "BSIM3nsubGiven" => &mut self.nsubGiven,
            "BSIM3npeakGiven" => &mut self.npeakGiven,
            "BSIM3ngateGiven" => &mut self.ngateGiven,
            "BSIM3gamma1Given" => &mut self.gamma1Given,
            "BSIM3gamma2Given" => &mut self.gamma2Given,
            "BSIM3vbxGiven" => &mut self.vbxGiven,
            "BSIM3vbmGiven" => &mut self.vbmGiven,
            "BSIM3xtGiven" => &mut self.xtGiven,
            "BSIM3k1Given" => &mut self.k1Given,
            "BSIM3kt1Given" => &mut self.kt1Given,
            "BSIM3kt1lGiven" => &mut self.kt1lGiven,
            "BSIM3kt2Given" => &mut self.kt2Given,
            "BSIM3k2Given" => &mut self.k2Given,
            "BSIM3k3Given" => &mut self.k3Given,
            "BSIM3k3bGiven" => &mut self.k3bGiven,
            "BSIM3w0Given" => &mut self.w0Given,
            "BSIM3nlxGiven" => &mut self.nlxGiven,
            "BSIM3dvt0Given" => &mut self.dvt0Given,
            "BSIM3dvt1Given" => &mut self.dvt1Given,
            "BSIM3dvt2Given" => &mut self.dvt2Given,
            "BSIM3dvt0wGiven" => &mut self.dvt0wGiven,
            "BSIM3dvt1wGiven" => &mut self.dvt1wGiven,
            "BSIM3dvt2wGiven" => &mut self.dvt2wGiven,
            "BSIM3droutGiven" => &mut self.droutGiven,
            "BSIM3dsubGiven" => &mut self.dsubGiven,
            "BSIM3vth0Given" => &mut self.vth0Given,
            "BSIM3uaGiven" => &mut self.uaGiven,
            "BSIM3ua1Given" => &mut self.ua1Given,
            "BSIM3ubGiven" => &mut self.ubGiven,
            "BSIM3ub1Given" => &mut self.ub1Given,
            "BSIM3ucGiven" => &mut self.ucGiven,
            "BSIM3uc1Given" => &mut self.uc1Given,
            "BSIM3u0Given" => &mut self.u0Given,
            "BSIM3uteGiven" => &mut self.uteGiven,
            "BSIM3voffGiven" => &mut self.voffGiven,
            "BSIM3rdswGiven" => &mut self.rdswGiven,
            "BSIM3prwgGiven" => &mut self.prwgGiven,
            "BSIM3prwbGiven" => &mut self.prwbGiven,
            "BSIM3prtGiven" => &mut self.prtGiven,
            "BSIM3eta0Given" => &mut self.eta0Given,
            "BSIM3etabGiven" => &mut self.etabGiven,
            "BSIM3pclmGiven" => &mut self.pclmGiven,
            "BSIM3pdibl1Given" => &mut self.pdibl1Given,
            "BSIM3pdibl2Given" => &mut self.pdibl2Given,
            "BSIM3pdiblbGiven" => &mut self.pdiblbGiven,
            "BSIM3pscbe1Given" => &mut self.pscbe1Given,
            "BSIM3pscbe2Given" => &mut self.pscbe2Given,
            "BSIM3pvagGiven" => &mut self.pvagGiven,
            "BSIM3deltaGiven" => &mut self.deltaGiven,
            "BSIM3wrGiven" => &mut self.wrGiven,
            "BSIM3dwgGiven" => &mut self.dwgGiven,
            "BSIM3dwbGiven" => &mut self.dwbGiven,
            "BSIM3b0Given" => &mut self.b0Given,
            "BSIM3b1Given" => &mut self.b1Given,
            "BSIM3alpha0Given" => &mut self.alpha0Given,
            "BSIM3alpha1Given" => &mut self.alpha1Given,
            "BSIM3beta0Given" => &mut self.beta0Given,
            "BSIM3ijthGiven" => &mut self.ijthGiven,
            "BSIM3vfbGiven" => &mut self.vfbGiven,
            "BSIM3elmGiven" => &mut self.elmGiven,
            "BSIM3cgslGiven" => &mut self.cgslGiven,
            "BSIM3cgdlGiven" => &mut self.cgdlGiven,
            "BSIM3ckappaGiven" => &mut self.ckappaGiven,
            "BSIM3cfGiven" => &mut self.cfGiven,
            "BSIM3vfbcvGiven" => &mut self.vfbcvGiven,
            "BSIM3clcGiven" => &mut self.clcGiven,
            "BSIM3cleGiven" => &mut self.cleGiven,
            "BSIM3dwcGiven" => &mut self.dwcGiven,
            "BSIM3dlcGiven" => &mut self.dlcGiven,
            "BSIM3noffGiven" => &mut self.noffGiven,
            "BSIM3voffcvGiven" => &mut self.voffcvGiven,
            "BSIM3acdeGiven" => &mut self.acdeGiven,
            "BSIM3moinGiven" => &mut self.moinGiven,
            "BSIM3tcjGiven" => &mut self.tcjGiven,
            "BSIM3tcjswGiven" => &mut self.tcjswGiven,
            "BSIM3tcjswgGiven" => &mut self.tcjswgGiven,
            "BSIM3tpbGiven" => &mut self.tpbGiven,
            "BSIM3tpbswGiven" => &mut self.tpbswGiven,
            "BSIM3tpbswgGiven" => &mut self.tpbswgGiven,
            "BSIM3xlGiven" => &mut self.xlGiven,
            "BSIM3xwGiven" => &mut self.xwGiven,
            "BSIM3hdifGiven" => &mut self.hdifGiven,
            "BSIM3ldifGiven" => &mut self.ldifGiven,
            "BSIM3ldGiven" => &mut self.ldGiven,
            "BSIM3rdGiven" => &mut self.rdGiven,
            "BSIM3rsGiven" => &mut self.rsGiven,
            "BSIM3rdcGiven" => &mut self.rdcGiven,
            "BSIM3rscGiven" => &mut self.rscGiven,
            "BSIM3wmltGiven" => &mut self.wmltGiven,
            "BSIM3lcdscGiven" => &mut self.lcdscGiven,
            "BSIM3lcdscbGiven" => &mut self.lcdscbGiven,
            "BSIM3lcdscdGiven" => &mut self.lcdscdGiven,
            "BSIM3lcitGiven" => &mut self.lcitGiven,
            "BSIM3lnfactorGiven" => &mut self.lnfactorGiven,
            "BSIM3lxjGiven" => &mut self.lxjGiven,
            "BSIM3lvsatGiven" => &mut self.lvsatGiven,
            "BSIM3latGiven" => &mut self.latGiven,
            "BSIM3la0Given" => &mut self.la0Given,
            "BSIM3lagsGiven" => &mut self.lagsGiven,
            "BSIM3la1Given" => &mut self.la1Given,
            "BSIM3la2Given" => &mut self.la2Given,
            "BSIM3lketaGiven" => &mut self.lketaGiven,
            "BSIM3lnsubGiven" => &mut self.lnsubGiven,
            "BSIM3lnpeakGiven" => &mut self.lnpeakGiven,
            "BSIM3lngateGiven" => &mut self.lngateGiven,
            "BSIM3lgamma1Given" => &mut self.lgamma1Given,
            "BSIM3lgamma2Given" => &mut self.lgamma2Given,
            "BSIM3lvbxGiven" => &mut self.lvbxGiven,
            "BSIM3lvbmGiven" => &mut self.lvbmGiven,
            "BSIM3lxtGiven" => &mut self.lxtGiven,
            "BSIM3lk1Given" => &mut self.lk1Given,
            "BSIM3lkt1Given" => &mut self.lkt1Given,
            "BSIM3lkt1lGiven" => &mut self.lkt1lGiven,
            "BSIM3lkt2Given" => &mut self.lkt2Given,
            "BSIM3lk2Given" => &mut self.lk2Given,
            "BSIM3lk3Given" => &mut self.lk3Given,
            "BSIM3lk3bGiven" => &mut self.lk3bGiven,
            "BSIM3lw0Given" => &mut self.lw0Given,
            "BSIM3lnlxGiven" => &mut self.lnlxGiven,
            "BSIM3ldvt0Given" => &mut self.ldvt0Given,
            "BSIM3ldvt1Given" => &mut self.ldvt1Given,
            "BSIM3ldvt2Given" => &mut self.ldvt2Given,
            "BSIM3ldvt0wGiven" => &mut self.ldvt0wGiven,
            "BSIM3ldvt1wGiven" => &mut self.ldvt1wGiven,
            "BSIM3ldvt2wGiven" => &mut self.ldvt2wGiven,
            "BSIM3ldroutGiven" => &mut self.ldroutGiven,
            "BSIM3ldsubGiven" => &mut self.ldsubGiven,
            "BSIM3lvth0Given" => &mut self.lvth0Given,
            "BSIM3luaGiven" => &mut self.luaGiven,
            "BSIM3lua1Given" => &mut self.lua1Given,
            "BSIM3lubGiven" => &mut self.lubGiven,
            "BSIM3lub1Given" => &mut self.lub1Given,
            "BSIM3lucGiven" => &mut self.lucGiven,
            "BSIM3luc1Given" => &mut self.luc1Given,
            "BSIM3lu0Given" => &mut self.lu0Given,
            "BSIM3luteGiven" => &mut self.luteGiven,
            "BSIM3lvoffGiven" => &mut self.lvoffGiven,
            "BSIM3lrdswGiven" => &mut self.lrdswGiven,
            "BSIM3lprwgGiven" => &mut self.lprwgGiven,
            "BSIM3lprwbGiven" => &mut self.lprwbGiven,
            "BSIM3lprtGiven" => &mut self.lprtGiven,
            "BSIM3leta0Given" => &mut self.leta0Given,
            "BSIM3letabGiven" => &mut self.letabGiven,
            "BSIM3lpclmGiven" => &mut self.lpclmGiven,
            "BSIM3lpdibl1Given" => &mut self.lpdibl1Given,
            "BSIM3lpdibl2Given" => &mut self.lpdibl2Given,
            "BSIM3lpdiblbGiven" => &mut self.lpdiblbGiven,
            "BSIM3lpscbe1Given" => &mut self.lpscbe1Given,
            "BSIM3lpscbe2Given" => &mut self.lpscbe2Given,
            "BSIM3lpvagGiven" => &mut self.lpvagGiven,
            "BSIM3ldeltaGiven" => &mut self.ldeltaGiven,
            "BSIM3lwrGiven" => &mut self.lwrGiven,
            "BSIM3ldwgGiven" => &mut self.ldwgGiven,
            "BSIM3ldwbGiven" => &mut self.ldwbGiven,
            "BSIM3lb0Given" => &mut self.lb0Given,
            "BSIM3lb1Given" => &mut self.lb1Given,
            "BSIM3lalpha0Given" => &mut self.lalpha0Given,
            "BSIM3lalpha1Given" => &mut self.lalpha1Given,
            "BSIM3lbeta0Given" => &mut self.lbeta0Given,
            "BSIM3lvfbGiven" => &mut self.lvfbGiven,
            "BSIM3lelmGiven" => &mut self.lelmGiven,
            "BSIM3lcgslGiven" => &mut self.lcgslGiven,
            "BSIM3lcgdlGiven" => &mut self.lcgdlGiven,
            "BSIM3lckappaGiven" => &mut self.lckappaGiven,
            "BSIM3lcfGiven" => &mut self.lcfGiven,
            "BSIM3lclcGiven" => &mut self.lclcGiven,
            "BSIM3lcleGiven" => &mut self.lcleGiven,
            "BSIM3lvfbcvGiven" => &mut self.lvfbcvGiven,
            "BSIM3lnoffGiven" => &mut self.lnoffGiven,
            "BSIM3lvoffcvGiven" => &mut self.lvoffcvGiven,
            "BSIM3lacdeGiven" => &mut self.lacdeGiven,
            "BSIM3lmoinGiven" => &mut self.lmoinGiven,
            "BSIM3wcdscGiven" => &mut self.wcdscGiven,
            "BSIM3wcdscbGiven" => &mut self.wcdscbGiven,
            "BSIM3wcdscdGiven" => &mut self.wcdscdGiven,
            "BSIM3wcitGiven" => &mut self.wcitGiven,
            "BSIM3wnfactorGiven" => &mut self.wnfactorGiven,
            "BSIM3wxjGiven" => &mut self.wxjGiven,
            "BSIM3wvsatGiven" => &mut self.wvsatGiven,
            "BSIM3watGiven" => &mut self.watGiven,
            "BSIM3wa0Given" => &mut self.wa0Given,
            "BSIM3wagsGiven" => &mut self.wagsGiven,
            "BSIM3wa1Given" => &mut self.wa1Given,
            "BSIM3wa2Given" => &mut self.wa2Given,
            "BSIM3wketaGiven" => &mut self.wketaGiven,
            "BSIM3wnsubGiven" => &mut self.wnsubGiven,
            "BSIM3wnpeakGiven" => &mut self.wnpeakGiven,
            "BSIM3wngateGiven" => &mut self.wngateGiven,
            "BSIM3wgamma1Given" => &mut self.wgamma1Given,
            "BSIM3wgamma2Given" => &mut self.wgamma2Given,
            "BSIM3wvbxGiven" => &mut self.wvbxGiven,
            "BSIM3wvbmGiven" => &mut self.wvbmGiven,
            "BSIM3wxtGiven" => &mut self.wxtGiven,
            "BSIM3wk1Given" => &mut self.wk1Given,
            "BSIM3wkt1Given" => &mut self.wkt1Given,
            "BSIM3wkt1lGiven" => &mut self.wkt1lGiven,
            "BSIM3wkt2Given" => &mut self.wkt2Given,
            "BSIM3wk2Given" => &mut self.wk2Given,
            "BSIM3wk3Given" => &mut self.wk3Given,
            "BSIM3wk3bGiven" => &mut self.wk3bGiven,
            "BSIM3ww0Given" => &mut self.ww0Given,
            "BSIM3wnlxGiven" => &mut self.wnlxGiven,
            "BSIM3wdvt0Given" => &mut self.wdvt0Given,
            "BSIM3wdvt1Given" => &mut self.wdvt1Given,
            "BSIM3wdvt2Given" => &mut self.wdvt2Given,
            "BSIM3wdvt0wGiven" => &mut self.wdvt0wGiven,
            "BSIM3wdvt1wGiven" => &mut self.wdvt1wGiven,
            "BSIM3wdvt2wGiven" => &mut self.wdvt2wGiven,
            "BSIM3wdroutGiven" => &mut self.wdroutGiven,
            "BSIM3wdsubGiven" => &mut self.wdsubGiven,
            "BSIM3wvth0Given" => &mut self.wvth0Given,
            "BSIM3wuaGiven" => &mut self.wuaGiven,
            "BSIM3wua1Given" => &mut self.wua1Given,
            "BSIM3wubGiven" => &mut self.wubGiven,
            "BSIM3wub1Given" => &mut self.wub1Given,
            "BSIM3wucGiven" => &mut self.wucGiven,
            "BSIM3wuc1Given" => &mut self.wuc1Given,
            "BSIM3wu0Given" => &mut self.wu0Given,
            "BSIM3wuteGiven" => &mut self.wuteGiven,
            "BSIM3wvoffGiven" => &mut self.wvoffGiven,
            "BSIM3wrdswGiven" => &mut self.wrdswGiven,
            "BSIM3wprwgGiven" => &mut self.wprwgGiven,
            "BSIM3wprwbGiven" => &mut self.wprwbGiven,
            "BSIM3wprtGiven" => &mut self.wprtGiven,
            "BSIM3weta0Given" => &mut self.weta0Given,
            "BSIM3wetabGiven" => &mut self.wetabGiven,
            "BSIM3wpclmGiven" => &mut self.wpclmGiven,
            "BSIM3wpdibl1Given" => &mut self.wpdibl1Given,
            "BSIM3wpdibl2Given" => &mut self.wpdibl2Given,
            "BSIM3wpdiblbGiven" => &mut self.wpdiblbGiven,
            "BSIM3wpscbe1Given" => &mut self.wpscbe1Given,
            "BSIM3wpscbe2Given" => &mut self.wpscbe2Given,
            "BSIM3wpvagGiven" => &mut self.wpvagGiven,
            "BSIM3wdeltaGiven" => &mut self.wdeltaGiven,
            "BSIM3wwrGiven" => &mut self.wwrGiven,
            "BSIM3wdwgGiven" => &mut self.wdwgGiven,
            "BSIM3wdwbGiven" => &mut self.wdwbGiven,
            "BSIM3wb0Given" => &mut self.wb0Given,
            "BSIM3wb1Given" => &mut self.wb1Given,
            "BSIM3walpha0Given" => &mut self.walpha0Given,
            "BSIM3walpha1Given" => &mut self.walpha1Given,
            "BSIM3wbeta0Given" => &mut self.wbeta0Given,
            "BSIM3wvfbGiven" => &mut self.wvfbGiven,
            "BSIM3welmGiven" => &mut self.welmGiven,
            "BSIM3wcgslGiven" => &mut self.wcgslGiven,
            "BSIM3wcgdlGiven" => &mut self.wcgdlGiven,
            "BSIM3wckappaGiven" => &mut self.wckappaGiven,
            "BSIM3wcfGiven" => &mut self.wcfGiven,
            "BSIM3wclcGiven" => &mut self.wclcGiven,
            "BSIM3wcleGiven" => &mut self.wcleGiven,
            "BSIM3wvfbcvGiven" => &mut self.wvfbcvGiven,
            "BSIM3wnoffGiven" => &mut self.wnoffGiven,
            "BSIM3wvoffcvGiven" => &mut self.wvoffcvGiven,
            "BSIM3wacdeGiven" => &mut self.wacdeGiven,
            "BSIM3wmoinGiven" => &mut self.wmoinGiven,
            "BSIM3pcdscGiven" => &mut self.pcdscGiven,
            "BSIM3pcdscbGiven" => &mut self.pcdscbGiven,
            "BSIM3pcdscdGiven" => &mut self.pcdscdGiven,
            "BSIM3pcitGiven" => &mut self.pcitGiven,
            "BSIM3pnfactorGiven" => &mut self.pnfactorGiven,
            "BSIM3pxjGiven" => &mut self.pxjGiven,
            "BSIM3pvsatGiven" => &mut self.pvsatGiven,
            "BSIM3patGiven" => &mut self.patGiven,
            "BSIM3pa0Given" => &mut self.pa0Given,
            "BSIM3pagsGiven" => &mut self.pagsGiven,
            "BSIM3pa1Given" => &mut self.pa1Given,
            "BSIM3pa2Given" => &mut self.pa2Given,
            "BSIM3pketaGiven" => &mut self.pketaGiven,
            "BSIM3pnsubGiven" => &mut self.pnsubGiven,
            "BSIM3pnpeakGiven" => &mut self.pnpeakGiven,
            "BSIM3pngateGiven" => &mut self.pngateGiven,
            "BSIM3pgamma1Given" => &mut self.pgamma1Given,
            "BSIM3pgamma2Given" => &mut self.pgamma2Given,
            "BSIM3pvbxGiven" => &mut self.pvbxGiven,
            "BSIM3pvbmGiven" => &mut self.pvbmGiven,
            "BSIM3pxtGiven" => &mut self.pxtGiven,
            "BSIM3pk1Given" => &mut self.pk1Given,
            "BSIM3pkt1Given" => &mut self.pkt1Given,
            "BSIM3pkt1lGiven" => &mut self.pkt1lGiven,
            "BSIM3pkt2Given" => &mut self.pkt2Given,
            "BSIM3pk2Given" => &mut self.pk2Given,
            "BSIM3pk3Given" => &mut self.pk3Given,
            "BSIM3pk3bGiven" => &mut self.pk3bGiven,
            "BSIM3pw0Given" => &mut self.pw0Given,
            "BSIM3pnlxGiven" => &mut self.pnlxGiven,
            "BSIM3pdvt0Given" => &mut self.pdvt0Given,
            "BSIM3pdvt1Given" => &mut self.pdvt1Given,
            "BSIM3pdvt2Given" => &mut self.pdvt2Given,
            "BSIM3pdvt0wGiven" => &mut self.pdvt0wGiven,
            "BSIM3pdvt1wGiven" => &mut self.pdvt1wGiven,
            "BSIM3pdvt2wGiven" => &mut self.pdvt2wGiven,
            "BSIM3pdroutGiven" => &mut self.pdroutGiven,
            "BSIM3pdsubGiven" => &mut self.pdsubGiven,
            "BSIM3pvth0Given" => &mut self.pvth0Given,
            "BSIM3puaGiven" => &mut self.puaGiven,
            "BSIM3pua1Given" => &mut self.pua1Given,
            "BSIM3pubGiven" => &mut self.pubGiven,
            "BSIM3pub1Given" => &mut self.pub1Given,
            "BSIM3pucGiven" => &mut self.pucGiven,
            "BSIM3puc1Given" => &mut self.puc1Given,
            "BSIM3pu0Given" => &mut self.pu0Given,
            "BSIM3puteGiven" => &mut self.puteGiven,
            "BSIM3pvoffGiven" => &mut self.pvoffGiven,
            "BSIM3prdswGiven" => &mut self.prdswGiven,
            "BSIM3pprwgGiven" => &mut self.pprwgGiven,
            "BSIM3pprwbGiven" => &mut self.pprwbGiven,
            "BSIM3pprtGiven" => &mut self.pprtGiven,
            "BSIM3peta0Given" => &mut self.peta0Given,
            "BSIM3petabGiven" => &mut self.petabGiven,
            "BSIM3ppclmGiven" => &mut self.ppclmGiven,
            "BSIM3ppdibl1Given" => &mut self.ppdibl1Given,
            "BSIM3ppdibl2Given" => &mut self.ppdibl2Given,
            "BSIM3ppdiblbGiven" => &mut self.ppdiblbGiven,
            "BSIM3ppscbe1Given" => &mut self.ppscbe1Given,
            "BSIM3ppscbe2Given" => &mut self.ppscbe2Given,
            "BSIM3ppvagGiven" => &mut self.ppvagGiven,
            "BSIM3pdeltaGiven" => &mut self.pdeltaGiven,
            "BSIM3pwrGiven" => &mut self.pwrGiven,
            "BSIM3pdwgGiven" => &mut self.pdwgGiven,
            "BSIM3pdwbGiven" => &mut self.pdwbGiven,
            "BSIM3pb0Given" => &mut self.pb0Given,
            "BSIM3pb1Given" => &mut self.pb1Given,
            "BSIM3palpha0Given" => &mut self.palpha0Given,
            "BSIM3palpha1Given" => &mut self.palpha1Given,
            "BSIM3pbeta0Given" => &mut self.pbeta0Given,
            "BSIM3pvfbGiven" => &mut self.pvfbGiven,
            "BSIM3pelmGiven" => &mut self.pelmGiven,
            "BSIM3pcgslGiven" => &mut self.pcgslGiven,
            "BSIM3pcgdlGiven" => &mut self.pcgdlGiven,
            "BSIM3pckappaGiven" => &mut self.pckappaGiven,
            "BSIM3pcfGiven" => &mut self.pcfGiven,
            "BSIM3pclcGiven" => &mut self.pclcGiven,
            "BSIM3pcleGiven" => &mut self.pcleGiven,
            "BSIM3pvfbcvGiven" => &mut self.pvfbcvGiven,
            "BSIM3pnoffGiven" => &mut self.pnoffGiven,
            "BSIM3pvoffcvGiven" => &mut self.pvoffcvGiven,
            "BSIM3pacdeGiven" => &mut self.pacdeGiven,
            "BSIM3pmoinGiven" => &mut self.pmoinGiven,
            "BSIM3useFringeGiven" => &mut self.useFringeGiven,
            "BSIM3tnomGiven" => &mut self.tnomGiven,
            "BSIM3cgsoGiven" => &mut self.cgsoGiven,
            "BSIM3cgdoGiven" => &mut self.cgdoGiven,
            "BSIM3cgboGiven" => &mut self.cgboGiven,
            "BSIM3xpartGiven" => &mut self.xpartGiven,
            "BSIM3sheetResistanceGiven" => &mut self.sheetResistanceGiven,
            "BSIM3jctSatCurDensityGiven" => &mut self.jctSatCurDensityGiven,
            "BSIM3jctSidewallSatCurDensityGiven" => &mut self.jctSidewallSatCurDensityGiven,
            "BSIM3bulkJctPotentialGiven" => &mut self.bulkJctPotentialGiven,
            "BSIM3bulkJctBotGradingCoeffGiven" => &mut self.bulkJctBotGradingCoeffGiven,
            "BSIM3sidewallJctPotentialGiven" => &mut self.sidewallJctPotentialGiven,
            "BSIM3GatesidewallJctPotentialGiven" => &mut self.GatesidewallJctPotentialGiven,
            "BSIM3bulkJctSideGradingCoeffGiven" => &mut self.bulkJctSideGradingCoeffGiven,
            "BSIM3unitAreaJctCapGiven" => &mut self.unitAreaJctCapGiven,
            "BSIM3unitLengthSidewallJctCapGiven" => &mut self.unitLengthSidewallJctCapGiven,
            "BSIM3bulkJctGateSideGradingCoeffGiven" => &mut self.bulkJctGateSideGradingCoeffGiven,
            "BSIM3unitLengthGateSidewallJctCapGiven" => &mut self.unitLengthGateSidewallJctCapGiven,
            "BSIM3jctEmissionCoeffGiven" => &mut self.jctEmissionCoeffGiven,
            "BSIM3jctTempExponentGiven" => &mut self.jctTempExponentGiven,
            "BSIM3oxideTrapDensityAGiven" => &mut self.oxideTrapDensityAGiven,
            "BSIM3oxideTrapDensityBGiven" => &mut self.oxideTrapDensityBGiven,
            "BSIM3oxideTrapDensityCGiven" => &mut self.oxideTrapDensityCGiven,
            "BSIM3emGiven" => &mut self.emGiven,
            "BSIM3efGiven" => &mut self.efGiven,
            "BSIM3afGiven" => &mut self.afGiven,
            "BSIM3kfGiven" => &mut self.kfGiven,
            "BSIM3lintnoiGiven" => &mut self.lintnoiGiven,
            "BSIM3vgsMaxGiven" => &mut self.vgsMaxGiven,
            "BSIM3vgdMaxGiven" => &mut self.vgdMaxGiven,
            "BSIM3vgbMaxGiven" => &mut self.vgbMaxGiven,
            "BSIM3vdsMaxGiven" => &mut self.vdsMaxGiven,
            "BSIM3vbsMaxGiven" => &mut self.vbsMaxGiven,
            "BSIM3vbdMaxGiven" => &mut self.vbdMaxGiven,
            "BSIM3vgsrMaxGiven" => &mut self.vgsrMaxGiven,
            "BSIM3vgdrMaxGiven" => &mut self.vgdrMaxGiven,
            "BSIM3vgbrMaxGiven" => &mut self.vgbrMaxGiven,
            "BSIM3vbsrMaxGiven" => &mut self.vbsrMaxGiven,
            "BSIM3vbdrMaxGiven" => &mut self.vbdrMaxGiven,
            "BSIM3LintGiven" => &mut self.LintGiven,
            "BSIM3LlGiven" => &mut self.LlGiven,
            "BSIM3LlcGiven" => &mut self.LlcGiven,
            "BSIM3LlnGiven" => &mut self.LlnGiven,
            "BSIM3LwGiven" => &mut self.LwGiven,
            "BSIM3LwcGiven" => &mut self.LwcGiven,
            "BSIM3LwnGiven" => &mut self.LwnGiven,
            "BSIM3LwlGiven" => &mut self.LwlGiven,
            "BSIM3LwlcGiven" => &mut self.LwlcGiven,
            "BSIM3LminGiven" => &mut self.LminGiven,
            "BSIM3LmaxGiven" => &mut self.LmaxGiven,
            "BSIM3WintGiven" => &mut self.WintGiven,
            "BSIM3WlGiven" => &mut self.WlGiven,
            "BSIM3WlcGiven" => &mut self.WlcGiven,
            "BSIM3WlnGiven" => &mut self.WlnGiven,
            "BSIM3WwGiven" => &mut self.WwGiven,
            "BSIM3WwcGiven" => &mut self.WwcGiven,
            "BSIM3WwnGiven" => &mut self.WwnGiven,
            "BSIM3WwlGiven" => &mut self.WwlGiven,
            "BSIM3WwlcGiven" => &mut self.WwlcGiven,
            "BSIM3WminGiven" => &mut self.WminGiven,
            "BSIM3WmaxGiven" => &mut self.WmaxGiven,
            _ => return None,
        })
    }
    /// C member names of the scalar members, in declaration order.
    #[allow(dead_code)]
    pub(crate) const MEMBERS: &[&str] = &[
        "BSIM3type",
        "BSIM3mobMod",
        "BSIM3capMod",
        "BSIM3acmMod",
        "BSIM3calcacm",
        "BSIM3noiMod",
        "BSIM3nqsMod",
        "BSIM3acnqsMod",
        "BSIM3binUnit",
        "BSIM3paramChk",
        "BSIM3tox",
        "BSIM3toxm",
        "BSIM3cdsc",
        "BSIM3cdscb",
        "BSIM3cdscd",
        "BSIM3cit",
        "BSIM3nfactor",
        "BSIM3xj",
        "BSIM3vsat",
        "BSIM3at",
        "BSIM3a0",
        "BSIM3ags",
        "BSIM3a1",
        "BSIM3a2",
        "BSIM3keta",
        "BSIM3nsub",
        "BSIM3npeak",
        "BSIM3ngate",
        "BSIM3gamma1",
        "BSIM3gamma2",
        "BSIM3vbx",
        "BSIM3vbm",
        "BSIM3xt",
        "BSIM3k1",
        "BSIM3kt1",
        "BSIM3kt1l",
        "BSIM3kt2",
        "BSIM3k2",
        "BSIM3k3",
        "BSIM3k3b",
        "BSIM3w0",
        "BSIM3nlx",
        "BSIM3dvt0",
        "BSIM3dvt1",
        "BSIM3dvt2",
        "BSIM3dvt0w",
        "BSIM3dvt1w",
        "BSIM3dvt2w",
        "BSIM3drout",
        "BSIM3dsub",
        "BSIM3vth0",
        "BSIM3ua",
        "BSIM3ua1",
        "BSIM3ub",
        "BSIM3ub1",
        "BSIM3uc",
        "BSIM3uc1",
        "BSIM3u0",
        "BSIM3ute",
        "BSIM3voff",
        "BSIM3delta",
        "BSIM3rdsw",
        "BSIM3prwg",
        "BSIM3prwb",
        "BSIM3prt",
        "BSIM3eta0",
        "BSIM3etab",
        "BSIM3pclm",
        "BSIM3pdibl1",
        "BSIM3pdibl2",
        "BSIM3pdiblb",
        "BSIM3pscbe1",
        "BSIM3pscbe2",
        "BSIM3pvag",
        "BSIM3wr",
        "BSIM3dwg",
        "BSIM3dwb",
        "BSIM3b0",
        "BSIM3b1",
        "BSIM3alpha0",
        "BSIM3alpha1",
        "BSIM3beta0",
        "BSIM3ijth",
        "BSIM3vfb",
        "BSIM3elm",
        "BSIM3cgsl",
        "BSIM3cgdl",
        "BSIM3ckappa",
        "BSIM3cf",
        "BSIM3vfbcv",
        "BSIM3clc",
        "BSIM3cle",
        "BSIM3dwc",
        "BSIM3dlc",
        "BSIM3noff",
        "BSIM3voffcv",
        "BSIM3acde",
        "BSIM3moin",
        "BSIM3tcj",
        "BSIM3tcjsw",
        "BSIM3tcjswg",
        "BSIM3tpb",
        "BSIM3tpbsw",
        "BSIM3tpbswg",
        "BSIM3xl",
        "BSIM3xw",
        "BSIM3hdif",
        "BSIM3ldif",
        "BSIM3ld",
        "BSIM3rd",
        "BSIM3rs",
        "BSIM3rdc",
        "BSIM3rsc",
        "BSIM3wmlt",
        "BSIM3lcdsc",
        "BSIM3lcdscb",
        "BSIM3lcdscd",
        "BSIM3lcit",
        "BSIM3lnfactor",
        "BSIM3lxj",
        "BSIM3lvsat",
        "BSIM3lat",
        "BSIM3la0",
        "BSIM3lags",
        "BSIM3la1",
        "BSIM3la2",
        "BSIM3lketa",
        "BSIM3lnsub",
        "BSIM3lnpeak",
        "BSIM3lngate",
        "BSIM3lgamma1",
        "BSIM3lgamma2",
        "BSIM3lvbx",
        "BSIM3lvbm",
        "BSIM3lxt",
        "BSIM3lk1",
        "BSIM3lkt1",
        "BSIM3lkt1l",
        "BSIM3lkt2",
        "BSIM3lk2",
        "BSIM3lk3",
        "BSIM3lk3b",
        "BSIM3lw0",
        "BSIM3lnlx",
        "BSIM3ldvt0",
        "BSIM3ldvt1",
        "BSIM3ldvt2",
        "BSIM3ldvt0w",
        "BSIM3ldvt1w",
        "BSIM3ldvt2w",
        "BSIM3ldrout",
        "BSIM3ldsub",
        "BSIM3lvth0",
        "BSIM3lua",
        "BSIM3lua1",
        "BSIM3lub",
        "BSIM3lub1",
        "BSIM3luc",
        "BSIM3luc1",
        "BSIM3lu0",
        "BSIM3lute",
        "BSIM3lvoff",
        "BSIM3ldelta",
        "BSIM3lrdsw",
        "BSIM3lprwg",
        "BSIM3lprwb",
        "BSIM3lprt",
        "BSIM3leta0",
        "BSIM3letab",
        "BSIM3lpclm",
        "BSIM3lpdibl1",
        "BSIM3lpdibl2",
        "BSIM3lpdiblb",
        "BSIM3lpscbe1",
        "BSIM3lpscbe2",
        "BSIM3lpvag",
        "BSIM3lwr",
        "BSIM3ldwg",
        "BSIM3ldwb",
        "BSIM3lb0",
        "BSIM3lb1",
        "BSIM3lalpha0",
        "BSIM3lalpha1",
        "BSIM3lbeta0",
        "BSIM3lvfb",
        "BSIM3lelm",
        "BSIM3lcgsl",
        "BSIM3lcgdl",
        "BSIM3lckappa",
        "BSIM3lcf",
        "BSIM3lclc",
        "BSIM3lcle",
        "BSIM3lvfbcv",
        "BSIM3lnoff",
        "BSIM3lvoffcv",
        "BSIM3lacde",
        "BSIM3lmoin",
        "BSIM3wcdsc",
        "BSIM3wcdscb",
        "BSIM3wcdscd",
        "BSIM3wcit",
        "BSIM3wnfactor",
        "BSIM3wxj",
        "BSIM3wvsat",
        "BSIM3wat",
        "BSIM3wa0",
        "BSIM3wags",
        "BSIM3wa1",
        "BSIM3wa2",
        "BSIM3wketa",
        "BSIM3wnsub",
        "BSIM3wnpeak",
        "BSIM3wngate",
        "BSIM3wgamma1",
        "BSIM3wgamma2",
        "BSIM3wvbx",
        "BSIM3wvbm",
        "BSIM3wxt",
        "BSIM3wk1",
        "BSIM3wkt1",
        "BSIM3wkt1l",
        "BSIM3wkt2",
        "BSIM3wk2",
        "BSIM3wk3",
        "BSIM3wk3b",
        "BSIM3ww0",
        "BSIM3wnlx",
        "BSIM3wdvt0",
        "BSIM3wdvt1",
        "BSIM3wdvt2",
        "BSIM3wdvt0w",
        "BSIM3wdvt1w",
        "BSIM3wdvt2w",
        "BSIM3wdrout",
        "BSIM3wdsub",
        "BSIM3wvth0",
        "BSIM3wua",
        "BSIM3wua1",
        "BSIM3wub",
        "BSIM3wub1",
        "BSIM3wuc",
        "BSIM3wuc1",
        "BSIM3wu0",
        "BSIM3wute",
        "BSIM3wvoff",
        "BSIM3wdelta",
        "BSIM3wrdsw",
        "BSIM3wprwg",
        "BSIM3wprwb",
        "BSIM3wprt",
        "BSIM3weta0",
        "BSIM3wetab",
        "BSIM3wpclm",
        "BSIM3wpdibl1",
        "BSIM3wpdibl2",
        "BSIM3wpdiblb",
        "BSIM3wpscbe1",
        "BSIM3wpscbe2",
        "BSIM3wpvag",
        "BSIM3wwr",
        "BSIM3wdwg",
        "BSIM3wdwb",
        "BSIM3wb0",
        "BSIM3wb1",
        "BSIM3walpha0",
        "BSIM3walpha1",
        "BSIM3wbeta0",
        "BSIM3wvfb",
        "BSIM3welm",
        "BSIM3wcgsl",
        "BSIM3wcgdl",
        "BSIM3wckappa",
        "BSIM3wcf",
        "BSIM3wclc",
        "BSIM3wcle",
        "BSIM3wvfbcv",
        "BSIM3wnoff",
        "BSIM3wvoffcv",
        "BSIM3wacde",
        "BSIM3wmoin",
        "BSIM3pcdsc",
        "BSIM3pcdscb",
        "BSIM3pcdscd",
        "BSIM3pcit",
        "BSIM3pnfactor",
        "BSIM3pxj",
        "BSIM3pvsat",
        "BSIM3pat",
        "BSIM3pa0",
        "BSIM3pags",
        "BSIM3pa1",
        "BSIM3pa2",
        "BSIM3pketa",
        "BSIM3pnsub",
        "BSIM3pnpeak",
        "BSIM3pngate",
        "BSIM3pgamma1",
        "BSIM3pgamma2",
        "BSIM3pvbx",
        "BSIM3pvbm",
        "BSIM3pxt",
        "BSIM3pk1",
        "BSIM3pkt1",
        "BSIM3pkt1l",
        "BSIM3pkt2",
        "BSIM3pk2",
        "BSIM3pk3",
        "BSIM3pk3b",
        "BSIM3pw0",
        "BSIM3pnlx",
        "BSIM3pdvt0",
        "BSIM3pdvt1",
        "BSIM3pdvt2",
        "BSIM3pdvt0w",
        "BSIM3pdvt1w",
        "BSIM3pdvt2w",
        "BSIM3pdrout",
        "BSIM3pdsub",
        "BSIM3pvth0",
        "BSIM3pua",
        "BSIM3pua1",
        "BSIM3pub",
        "BSIM3pub1",
        "BSIM3puc",
        "BSIM3puc1",
        "BSIM3pu0",
        "BSIM3pute",
        "BSIM3pvoff",
        "BSIM3pdelta",
        "BSIM3prdsw",
        "BSIM3pprwg",
        "BSIM3pprwb",
        "BSIM3pprt",
        "BSIM3peta0",
        "BSIM3petab",
        "BSIM3ppclm",
        "BSIM3ppdibl1",
        "BSIM3ppdibl2",
        "BSIM3ppdiblb",
        "BSIM3ppscbe1",
        "BSIM3ppscbe2",
        "BSIM3ppvag",
        "BSIM3pwr",
        "BSIM3pdwg",
        "BSIM3pdwb",
        "BSIM3pb0",
        "BSIM3pb1",
        "BSIM3palpha0",
        "BSIM3palpha1",
        "BSIM3pbeta0",
        "BSIM3pvfb",
        "BSIM3pelm",
        "BSIM3pcgsl",
        "BSIM3pcgdl",
        "BSIM3pckappa",
        "BSIM3pcf",
        "BSIM3pclc",
        "BSIM3pcle",
        "BSIM3pvfbcv",
        "BSIM3pnoff",
        "BSIM3pvoffcv",
        "BSIM3pacde",
        "BSIM3pmoin",
        "BSIM3tnom",
        "BSIM3cgso",
        "BSIM3cgdo",
        "BSIM3cgbo",
        "BSIM3xpart",
        "BSIM3cFringOut",
        "BSIM3cFringMax",
        "BSIM3sheetResistance",
        "BSIM3jctSatCurDensity",
        "BSIM3jctSidewallSatCurDensity",
        "BSIM3bulkJctPotential",
        "BSIM3bulkJctBotGradingCoeff",
        "BSIM3bulkJctSideGradingCoeff",
        "BSIM3bulkJctGateSideGradingCoeff",
        "BSIM3sidewallJctPotential",
        "BSIM3GatesidewallJctPotential",
        "BSIM3unitAreaJctCap",
        "BSIM3unitLengthSidewallJctCap",
        "BSIM3unitLengthGateSidewallJctCap",
        "BSIM3jctEmissionCoeff",
        "BSIM3jctTempExponent",
        "BSIM3Lint",
        "BSIM3Ll",
        "BSIM3Llc",
        "BSIM3Lln",
        "BSIM3Lw",
        "BSIM3Lwc",
        "BSIM3Lwn",
        "BSIM3Lwl",
        "BSIM3Lwlc",
        "BSIM3Lmin",
        "BSIM3Lmax",
        "BSIM3Wint",
        "BSIM3Wl",
        "BSIM3Wlc",
        "BSIM3Wln",
        "BSIM3Ww",
        "BSIM3Wwc",
        "BSIM3Wwn",
        "BSIM3Wwl",
        "BSIM3Wwlc",
        "BSIM3Wmin",
        "BSIM3Wmax",
        "BSIM3vtm",
        "BSIM3cox",
        "BSIM3cof1",
        "BSIM3cof2",
        "BSIM3cof3",
        "BSIM3cof4",
        "BSIM3vcrit",
        "BSIM3factor1",
        "BSIM3PhiB",
        "BSIM3PhiBSW",
        "BSIM3PhiBSWG",
        "BSIM3jctTempSatCurDensity",
        "BSIM3jctSidewallTempSatCurDensity",
        "BSIM3unitAreaTempJctCap",
        "BSIM3unitLengthSidewallTempJctCap",
        "BSIM3unitLengthGateSidewallTempJctCap",
        "BSIM3oxideTrapDensityA",
        "BSIM3oxideTrapDensityB",
        "BSIM3oxideTrapDensityC",
        "BSIM3em",
        "BSIM3ef",
        "BSIM3af",
        "BSIM3kf",
        "BSIM3lintnoi",
        "BSIM3vgsMax",
        "BSIM3vgdMax",
        "BSIM3vgbMax",
        "BSIM3vdsMax",
        "BSIM3vbsMax",
        "BSIM3vbdMax",
        "BSIM3vgsrMax",
        "BSIM3vgdrMax",
        "BSIM3vgbrMax",
        "BSIM3vbsrMax",
        "BSIM3vbdrMax",
        "BSIM3InstCount",
        "BSIM3mobModGiven",
        "BSIM3binUnitGiven",
        "BSIM3capModGiven",
        "BSIM3acmModGiven",
        "BSIM3calcacmGiven",
        "BSIM3paramChkGiven",
        "BSIM3noiModGiven",
        "BSIM3nqsModGiven",
        "BSIM3acnqsModGiven",
        "BSIM3typeGiven",
        "BSIM3toxGiven",
        "BSIM3versionGiven",
        "BSIM3toxmGiven",
        "BSIM3cdscGiven",
        "BSIM3cdscbGiven",
        "BSIM3cdscdGiven",
        "BSIM3citGiven",
        "BSIM3nfactorGiven",
        "BSIM3xjGiven",
        "BSIM3vsatGiven",
        "BSIM3atGiven",
        "BSIM3a0Given",
        "BSIM3agsGiven",
        "BSIM3a1Given",
        "BSIM3a2Given",
        "BSIM3ketaGiven",
        "BSIM3nsubGiven",
        "BSIM3npeakGiven",
        "BSIM3ngateGiven",
        "BSIM3gamma1Given",
        "BSIM3gamma2Given",
        "BSIM3vbxGiven",
        "BSIM3vbmGiven",
        "BSIM3xtGiven",
        "BSIM3k1Given",
        "BSIM3kt1Given",
        "BSIM3kt1lGiven",
        "BSIM3kt2Given",
        "BSIM3k2Given",
        "BSIM3k3Given",
        "BSIM3k3bGiven",
        "BSIM3w0Given",
        "BSIM3nlxGiven",
        "BSIM3dvt0Given",
        "BSIM3dvt1Given",
        "BSIM3dvt2Given",
        "BSIM3dvt0wGiven",
        "BSIM3dvt1wGiven",
        "BSIM3dvt2wGiven",
        "BSIM3droutGiven",
        "BSIM3dsubGiven",
        "BSIM3vth0Given",
        "BSIM3uaGiven",
        "BSIM3ua1Given",
        "BSIM3ubGiven",
        "BSIM3ub1Given",
        "BSIM3ucGiven",
        "BSIM3uc1Given",
        "BSIM3u0Given",
        "BSIM3uteGiven",
        "BSIM3voffGiven",
        "BSIM3rdswGiven",
        "BSIM3prwgGiven",
        "BSIM3prwbGiven",
        "BSIM3prtGiven",
        "BSIM3eta0Given",
        "BSIM3etabGiven",
        "BSIM3pclmGiven",
        "BSIM3pdibl1Given",
        "BSIM3pdibl2Given",
        "BSIM3pdiblbGiven",
        "BSIM3pscbe1Given",
        "BSIM3pscbe2Given",
        "BSIM3pvagGiven",
        "BSIM3deltaGiven",
        "BSIM3wrGiven",
        "BSIM3dwgGiven",
        "BSIM3dwbGiven",
        "BSIM3b0Given",
        "BSIM3b1Given",
        "BSIM3alpha0Given",
        "BSIM3alpha1Given",
        "BSIM3beta0Given",
        "BSIM3ijthGiven",
        "BSIM3vfbGiven",
        "BSIM3elmGiven",
        "BSIM3cgslGiven",
        "BSIM3cgdlGiven",
        "BSIM3ckappaGiven",
        "BSIM3cfGiven",
        "BSIM3vfbcvGiven",
        "BSIM3clcGiven",
        "BSIM3cleGiven",
        "BSIM3dwcGiven",
        "BSIM3dlcGiven",
        "BSIM3noffGiven",
        "BSIM3voffcvGiven",
        "BSIM3acdeGiven",
        "BSIM3moinGiven",
        "BSIM3tcjGiven",
        "BSIM3tcjswGiven",
        "BSIM3tcjswgGiven",
        "BSIM3tpbGiven",
        "BSIM3tpbswGiven",
        "BSIM3tpbswgGiven",
        "BSIM3xlGiven",
        "BSIM3xwGiven",
        "BSIM3hdifGiven",
        "BSIM3ldifGiven",
        "BSIM3ldGiven",
        "BSIM3rdGiven",
        "BSIM3rsGiven",
        "BSIM3rdcGiven",
        "BSIM3rscGiven",
        "BSIM3wmltGiven",
        "BSIM3lcdscGiven",
        "BSIM3lcdscbGiven",
        "BSIM3lcdscdGiven",
        "BSIM3lcitGiven",
        "BSIM3lnfactorGiven",
        "BSIM3lxjGiven",
        "BSIM3lvsatGiven",
        "BSIM3latGiven",
        "BSIM3la0Given",
        "BSIM3lagsGiven",
        "BSIM3la1Given",
        "BSIM3la2Given",
        "BSIM3lketaGiven",
        "BSIM3lnsubGiven",
        "BSIM3lnpeakGiven",
        "BSIM3lngateGiven",
        "BSIM3lgamma1Given",
        "BSIM3lgamma2Given",
        "BSIM3lvbxGiven",
        "BSIM3lvbmGiven",
        "BSIM3lxtGiven",
        "BSIM3lk1Given",
        "BSIM3lkt1Given",
        "BSIM3lkt1lGiven",
        "BSIM3lkt2Given",
        "BSIM3lk2Given",
        "BSIM3lk3Given",
        "BSIM3lk3bGiven",
        "BSIM3lw0Given",
        "BSIM3lnlxGiven",
        "BSIM3ldvt0Given",
        "BSIM3ldvt1Given",
        "BSIM3ldvt2Given",
        "BSIM3ldvt0wGiven",
        "BSIM3ldvt1wGiven",
        "BSIM3ldvt2wGiven",
        "BSIM3ldroutGiven",
        "BSIM3ldsubGiven",
        "BSIM3lvth0Given",
        "BSIM3luaGiven",
        "BSIM3lua1Given",
        "BSIM3lubGiven",
        "BSIM3lub1Given",
        "BSIM3lucGiven",
        "BSIM3luc1Given",
        "BSIM3lu0Given",
        "BSIM3luteGiven",
        "BSIM3lvoffGiven",
        "BSIM3lrdswGiven",
        "BSIM3lprwgGiven",
        "BSIM3lprwbGiven",
        "BSIM3lprtGiven",
        "BSIM3leta0Given",
        "BSIM3letabGiven",
        "BSIM3lpclmGiven",
        "BSIM3lpdibl1Given",
        "BSIM3lpdibl2Given",
        "BSIM3lpdiblbGiven",
        "BSIM3lpscbe1Given",
        "BSIM3lpscbe2Given",
        "BSIM3lpvagGiven",
        "BSIM3ldeltaGiven",
        "BSIM3lwrGiven",
        "BSIM3ldwgGiven",
        "BSIM3ldwbGiven",
        "BSIM3lb0Given",
        "BSIM3lb1Given",
        "BSIM3lalpha0Given",
        "BSIM3lalpha1Given",
        "BSIM3lbeta0Given",
        "BSIM3lvfbGiven",
        "BSIM3lelmGiven",
        "BSIM3lcgslGiven",
        "BSIM3lcgdlGiven",
        "BSIM3lckappaGiven",
        "BSIM3lcfGiven",
        "BSIM3lclcGiven",
        "BSIM3lcleGiven",
        "BSIM3lvfbcvGiven",
        "BSIM3lnoffGiven",
        "BSIM3lvoffcvGiven",
        "BSIM3lacdeGiven",
        "BSIM3lmoinGiven",
        "BSIM3wcdscGiven",
        "BSIM3wcdscbGiven",
        "BSIM3wcdscdGiven",
        "BSIM3wcitGiven",
        "BSIM3wnfactorGiven",
        "BSIM3wxjGiven",
        "BSIM3wvsatGiven",
        "BSIM3watGiven",
        "BSIM3wa0Given",
        "BSIM3wagsGiven",
        "BSIM3wa1Given",
        "BSIM3wa2Given",
        "BSIM3wketaGiven",
        "BSIM3wnsubGiven",
        "BSIM3wnpeakGiven",
        "BSIM3wngateGiven",
        "BSIM3wgamma1Given",
        "BSIM3wgamma2Given",
        "BSIM3wvbxGiven",
        "BSIM3wvbmGiven",
        "BSIM3wxtGiven",
        "BSIM3wk1Given",
        "BSIM3wkt1Given",
        "BSIM3wkt1lGiven",
        "BSIM3wkt2Given",
        "BSIM3wk2Given",
        "BSIM3wk3Given",
        "BSIM3wk3bGiven",
        "BSIM3ww0Given",
        "BSIM3wnlxGiven",
        "BSIM3wdvt0Given",
        "BSIM3wdvt1Given",
        "BSIM3wdvt2Given",
        "BSIM3wdvt0wGiven",
        "BSIM3wdvt1wGiven",
        "BSIM3wdvt2wGiven",
        "BSIM3wdroutGiven",
        "BSIM3wdsubGiven",
        "BSIM3wvth0Given",
        "BSIM3wuaGiven",
        "BSIM3wua1Given",
        "BSIM3wubGiven",
        "BSIM3wub1Given",
        "BSIM3wucGiven",
        "BSIM3wuc1Given",
        "BSIM3wu0Given",
        "BSIM3wuteGiven",
        "BSIM3wvoffGiven",
        "BSIM3wrdswGiven",
        "BSIM3wprwgGiven",
        "BSIM3wprwbGiven",
        "BSIM3wprtGiven",
        "BSIM3weta0Given",
        "BSIM3wetabGiven",
        "BSIM3wpclmGiven",
        "BSIM3wpdibl1Given",
        "BSIM3wpdibl2Given",
        "BSIM3wpdiblbGiven",
        "BSIM3wpscbe1Given",
        "BSIM3wpscbe2Given",
        "BSIM3wpvagGiven",
        "BSIM3wdeltaGiven",
        "BSIM3wwrGiven",
        "BSIM3wdwgGiven",
        "BSIM3wdwbGiven",
        "BSIM3wb0Given",
        "BSIM3wb1Given",
        "BSIM3walpha0Given",
        "BSIM3walpha1Given",
        "BSIM3wbeta0Given",
        "BSIM3wvfbGiven",
        "BSIM3welmGiven",
        "BSIM3wcgslGiven",
        "BSIM3wcgdlGiven",
        "BSIM3wckappaGiven",
        "BSIM3wcfGiven",
        "BSIM3wclcGiven",
        "BSIM3wcleGiven",
        "BSIM3wvfbcvGiven",
        "BSIM3wnoffGiven",
        "BSIM3wvoffcvGiven",
        "BSIM3wacdeGiven",
        "BSIM3wmoinGiven",
        "BSIM3pcdscGiven",
        "BSIM3pcdscbGiven",
        "BSIM3pcdscdGiven",
        "BSIM3pcitGiven",
        "BSIM3pnfactorGiven",
        "BSIM3pxjGiven",
        "BSIM3pvsatGiven",
        "BSIM3patGiven",
        "BSIM3pa0Given",
        "BSIM3pagsGiven",
        "BSIM3pa1Given",
        "BSIM3pa2Given",
        "BSIM3pketaGiven",
        "BSIM3pnsubGiven",
        "BSIM3pnpeakGiven",
        "BSIM3pngateGiven",
        "BSIM3pgamma1Given",
        "BSIM3pgamma2Given",
        "BSIM3pvbxGiven",
        "BSIM3pvbmGiven",
        "BSIM3pxtGiven",
        "BSIM3pk1Given",
        "BSIM3pkt1Given",
        "BSIM3pkt1lGiven",
        "BSIM3pkt2Given",
        "BSIM3pk2Given",
        "BSIM3pk3Given",
        "BSIM3pk3bGiven",
        "BSIM3pw0Given",
        "BSIM3pnlxGiven",
        "BSIM3pdvt0Given",
        "BSIM3pdvt1Given",
        "BSIM3pdvt2Given",
        "BSIM3pdvt0wGiven",
        "BSIM3pdvt1wGiven",
        "BSIM3pdvt2wGiven",
        "BSIM3pdroutGiven",
        "BSIM3pdsubGiven",
        "BSIM3pvth0Given",
        "BSIM3puaGiven",
        "BSIM3pua1Given",
        "BSIM3pubGiven",
        "BSIM3pub1Given",
        "BSIM3pucGiven",
        "BSIM3puc1Given",
        "BSIM3pu0Given",
        "BSIM3puteGiven",
        "BSIM3pvoffGiven",
        "BSIM3prdswGiven",
        "BSIM3pprwgGiven",
        "BSIM3pprwbGiven",
        "BSIM3pprtGiven",
        "BSIM3peta0Given",
        "BSIM3petabGiven",
        "BSIM3ppclmGiven",
        "BSIM3ppdibl1Given",
        "BSIM3ppdibl2Given",
        "BSIM3ppdiblbGiven",
        "BSIM3ppscbe1Given",
        "BSIM3ppscbe2Given",
        "BSIM3ppvagGiven",
        "BSIM3pdeltaGiven",
        "BSIM3pwrGiven",
        "BSIM3pdwgGiven",
        "BSIM3pdwbGiven",
        "BSIM3pb0Given",
        "BSIM3pb1Given",
        "BSIM3palpha0Given",
        "BSIM3palpha1Given",
        "BSIM3pbeta0Given",
        "BSIM3pvfbGiven",
        "BSIM3pelmGiven",
        "BSIM3pcgslGiven",
        "BSIM3pcgdlGiven",
        "BSIM3pckappaGiven",
        "BSIM3pcfGiven",
        "BSIM3pclcGiven",
        "BSIM3pcleGiven",
        "BSIM3pvfbcvGiven",
        "BSIM3pnoffGiven",
        "BSIM3pvoffcvGiven",
        "BSIM3pacdeGiven",
        "BSIM3pmoinGiven",
        "BSIM3useFringeGiven",
        "BSIM3tnomGiven",
        "BSIM3cgsoGiven",
        "BSIM3cgdoGiven",
        "BSIM3cgboGiven",
        "BSIM3xpartGiven",
        "BSIM3sheetResistanceGiven",
        "BSIM3jctSatCurDensityGiven",
        "BSIM3jctSidewallSatCurDensityGiven",
        "BSIM3bulkJctPotentialGiven",
        "BSIM3bulkJctBotGradingCoeffGiven",
        "BSIM3sidewallJctPotentialGiven",
        "BSIM3GatesidewallJctPotentialGiven",
        "BSIM3bulkJctSideGradingCoeffGiven",
        "BSIM3unitAreaJctCapGiven",
        "BSIM3unitLengthSidewallJctCapGiven",
        "BSIM3bulkJctGateSideGradingCoeffGiven",
        "BSIM3unitLengthGateSidewallJctCapGiven",
        "BSIM3jctEmissionCoeffGiven",
        "BSIM3jctTempExponentGiven",
        "BSIM3oxideTrapDensityAGiven",
        "BSIM3oxideTrapDensityBGiven",
        "BSIM3oxideTrapDensityCGiven",
        "BSIM3emGiven",
        "BSIM3efGiven",
        "BSIM3afGiven",
        "BSIM3kfGiven",
        "BSIM3lintnoiGiven",
        "BSIM3vgsMaxGiven",
        "BSIM3vgdMaxGiven",
        "BSIM3vgbMaxGiven",
        "BSIM3vdsMaxGiven",
        "BSIM3vbsMaxGiven",
        "BSIM3vbdMaxGiven",
        "BSIM3vgsrMaxGiven",
        "BSIM3vgdrMaxGiven",
        "BSIM3vgbrMaxGiven",
        "BSIM3vbsrMaxGiven",
        "BSIM3vbdrMaxGiven",
        "BSIM3LintGiven",
        "BSIM3LlGiven",
        "BSIM3LlcGiven",
        "BSIM3LlnGiven",
        "BSIM3LwGiven",
        "BSIM3LwcGiven",
        "BSIM3LwnGiven",
        "BSIM3LwlGiven",
        "BSIM3LwlcGiven",
        "BSIM3LminGiven",
        "BSIM3LmaxGiven",
        "BSIM3WintGiven",
        "BSIM3WlGiven",
        "BSIM3WlcGiven",
        "BSIM3WlnGiven",
        "BSIM3WwGiven",
        "BSIM3WwcGiven",
        "BSIM3WwnGiven",
        "BSIM3WwlGiven",
        "BSIM3WwlcGiven",
        "BSIM3WminGiven",
        "BSIM3WmaxGiven",
    ];

    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,
    /// a given bit as 0 or 1).
    #[allow(dead_code)]
    pub(crate) fn to_values(&self) -> Vec<f64> {
        vec![
            f64::from(self.r#type),
            f64::from(self.mobMod),
            f64::from(self.capMod),
            f64::from(self.acmMod),
            f64::from(self.calcacm),
            f64::from(self.noiMod),
            f64::from(self.nqsMod),
            f64::from(self.acnqsMod),
            f64::from(self.binUnit),
            f64::from(self.paramChk),
            self.tox,
            self.toxm,
            self.cdsc,
            self.cdscb,
            self.cdscd,
            self.cit,
            self.nfactor,
            self.xj,
            self.vsat,
            self.at,
            self.a0,
            self.ags,
            self.a1,
            self.a2,
            self.keta,
            self.nsub,
            self.npeak,
            self.ngate,
            self.gamma1,
            self.gamma2,
            self.vbx,
            self.vbm,
            self.xt,
            self.k1,
            self.kt1,
            self.kt1l,
            self.kt2,
            self.k2,
            self.k3,
            self.k3b,
            self.w0,
            self.nlx,
            self.dvt0,
            self.dvt1,
            self.dvt2,
            self.dvt0w,
            self.dvt1w,
            self.dvt2w,
            self.drout,
            self.dsub,
            self.vth0,
            self.ua,
            self.ua1,
            self.ub,
            self.ub1,
            self.uc,
            self.uc1,
            self.u0,
            self.ute,
            self.voff,
            self.delta,
            self.rdsw,
            self.prwg,
            self.prwb,
            self.prt,
            self.eta0,
            self.etab,
            self.pclm,
            self.pdibl1,
            self.pdibl2,
            self.pdiblb,
            self.pscbe1,
            self.pscbe2,
            self.pvag,
            self.wr,
            self.dwg,
            self.dwb,
            self.b0,
            self.b1,
            self.alpha0,
            self.alpha1,
            self.beta0,
            self.ijth,
            self.vfb,
            self.elm,
            self.cgsl,
            self.cgdl,
            self.ckappa,
            self.cf,
            self.vfbcv,
            self.clc,
            self.cle,
            self.dwc,
            self.dlc,
            self.noff,
            self.voffcv,
            self.acde,
            self.moin,
            self.tcj,
            self.tcjsw,
            self.tcjswg,
            self.tpb,
            self.tpbsw,
            self.tpbswg,
            self.xl,
            self.xw,
            self.hdif,
            self.ldif,
            self.ld,
            self.rd,
            self.rs,
            self.rdc,
            self.rsc,
            self.wmlt,
            self.lcdsc,
            self.lcdscb,
            self.lcdscd,
            self.lcit,
            self.lnfactor,
            self.lxj,
            self.lvsat,
            self.lat,
            self.la0,
            self.lags,
            self.la1,
            self.la2,
            self.lketa,
            self.lnsub,
            self.lnpeak,
            self.lngate,
            self.lgamma1,
            self.lgamma2,
            self.lvbx,
            self.lvbm,
            self.lxt,
            self.lk1,
            self.lkt1,
            self.lkt1l,
            self.lkt2,
            self.lk2,
            self.lk3,
            self.lk3b,
            self.lw0,
            self.lnlx,
            self.ldvt0,
            self.ldvt1,
            self.ldvt2,
            self.ldvt0w,
            self.ldvt1w,
            self.ldvt2w,
            self.ldrout,
            self.ldsub,
            self.lvth0,
            self.lua,
            self.lua1,
            self.lub,
            self.lub1,
            self.luc,
            self.luc1,
            self.lu0,
            self.lute,
            self.lvoff,
            self.ldelta,
            self.lrdsw,
            self.lprwg,
            self.lprwb,
            self.lprt,
            self.leta0,
            self.letab,
            self.lpclm,
            self.lpdibl1,
            self.lpdibl2,
            self.lpdiblb,
            self.lpscbe1,
            self.lpscbe2,
            self.lpvag,
            self.lwr,
            self.ldwg,
            self.ldwb,
            self.lb0,
            self.lb1,
            self.lalpha0,
            self.lalpha1,
            self.lbeta0,
            self.lvfb,
            self.lelm,
            self.lcgsl,
            self.lcgdl,
            self.lckappa,
            self.lcf,
            self.lclc,
            self.lcle,
            self.lvfbcv,
            self.lnoff,
            self.lvoffcv,
            self.lacde,
            self.lmoin,
            self.wcdsc,
            self.wcdscb,
            self.wcdscd,
            self.wcit,
            self.wnfactor,
            self.wxj,
            self.wvsat,
            self.wat,
            self.wa0,
            self.wags,
            self.wa1,
            self.wa2,
            self.wketa,
            self.wnsub,
            self.wnpeak,
            self.wngate,
            self.wgamma1,
            self.wgamma2,
            self.wvbx,
            self.wvbm,
            self.wxt,
            self.wk1,
            self.wkt1,
            self.wkt1l,
            self.wkt2,
            self.wk2,
            self.wk3,
            self.wk3b,
            self.ww0,
            self.wnlx,
            self.wdvt0,
            self.wdvt1,
            self.wdvt2,
            self.wdvt0w,
            self.wdvt1w,
            self.wdvt2w,
            self.wdrout,
            self.wdsub,
            self.wvth0,
            self.wua,
            self.wua1,
            self.wub,
            self.wub1,
            self.wuc,
            self.wuc1,
            self.wu0,
            self.wute,
            self.wvoff,
            self.wdelta,
            self.wrdsw,
            self.wprwg,
            self.wprwb,
            self.wprt,
            self.weta0,
            self.wetab,
            self.wpclm,
            self.wpdibl1,
            self.wpdibl2,
            self.wpdiblb,
            self.wpscbe1,
            self.wpscbe2,
            self.wpvag,
            self.wwr,
            self.wdwg,
            self.wdwb,
            self.wb0,
            self.wb1,
            self.walpha0,
            self.walpha1,
            self.wbeta0,
            self.wvfb,
            self.welm,
            self.wcgsl,
            self.wcgdl,
            self.wckappa,
            self.wcf,
            self.wclc,
            self.wcle,
            self.wvfbcv,
            self.wnoff,
            self.wvoffcv,
            self.wacde,
            self.wmoin,
            self.pcdsc,
            self.pcdscb,
            self.pcdscd,
            self.pcit,
            self.pnfactor,
            self.pxj,
            self.pvsat,
            self.pat,
            self.pa0,
            self.pags,
            self.pa1,
            self.pa2,
            self.pketa,
            self.pnsub,
            self.pnpeak,
            self.pngate,
            self.pgamma1,
            self.pgamma2,
            self.pvbx,
            self.pvbm,
            self.pxt,
            self.pk1,
            self.pkt1,
            self.pkt1l,
            self.pkt2,
            self.pk2,
            self.pk3,
            self.pk3b,
            self.pw0,
            self.pnlx,
            self.pdvt0,
            self.pdvt1,
            self.pdvt2,
            self.pdvt0w,
            self.pdvt1w,
            self.pdvt2w,
            self.pdrout,
            self.pdsub,
            self.pvth0,
            self.pua,
            self.pua1,
            self.r#pub,
            self.pub1,
            self.puc,
            self.puc1,
            self.pu0,
            self.pute,
            self.pvoff,
            self.pdelta,
            self.prdsw,
            self.pprwg,
            self.pprwb,
            self.pprt,
            self.peta0,
            self.petab,
            self.ppclm,
            self.ppdibl1,
            self.ppdibl2,
            self.ppdiblb,
            self.ppscbe1,
            self.ppscbe2,
            self.ppvag,
            self.pwr,
            self.pdwg,
            self.pdwb,
            self.pb0,
            self.pb1,
            self.palpha0,
            self.palpha1,
            self.pbeta0,
            self.pvfb,
            self.pelm,
            self.pcgsl,
            self.pcgdl,
            self.pckappa,
            self.pcf,
            self.pclc,
            self.pcle,
            self.pvfbcv,
            self.pnoff,
            self.pvoffcv,
            self.pacde,
            self.pmoin,
            self.tnom,
            self.cgso,
            self.cgdo,
            self.cgbo,
            self.xpart,
            self.cFringOut,
            self.cFringMax,
            self.sheetResistance,
            self.jctSatCurDensity,
            self.jctSidewallSatCurDensity,
            self.bulkJctPotential,
            self.bulkJctBotGradingCoeff,
            self.bulkJctSideGradingCoeff,
            self.bulkJctGateSideGradingCoeff,
            self.sidewallJctPotential,
            self.GatesidewallJctPotential,
            self.unitAreaJctCap,
            self.unitLengthSidewallJctCap,
            self.unitLengthGateSidewallJctCap,
            self.jctEmissionCoeff,
            self.jctTempExponent,
            self.Lint,
            self.Ll,
            self.Llc,
            self.Lln,
            self.Lw,
            self.Lwc,
            self.Lwn,
            self.Lwl,
            self.Lwlc,
            self.Lmin,
            self.Lmax,
            self.Wint,
            self.Wl,
            self.Wlc,
            self.Wln,
            self.Ww,
            self.Wwc,
            self.Wwn,
            self.Wwl,
            self.Wwlc,
            self.Wmin,
            self.Wmax,
            self.vtm,
            self.cox,
            self.cof1,
            self.cof2,
            self.cof3,
            self.cof4,
            self.vcrit,
            self.factor1,
            self.PhiB,
            self.PhiBSW,
            self.PhiBSWG,
            self.jctTempSatCurDensity,
            self.jctSidewallTempSatCurDensity,
            self.unitAreaTempJctCap,
            self.unitLengthSidewallTempJctCap,
            self.unitLengthGateSidewallTempJctCap,
            self.oxideTrapDensityA,
            self.oxideTrapDensityB,
            self.oxideTrapDensityC,
            self.em,
            self.ef,
            self.af,
            self.kf,
            self.lintnoi,
            self.vgsMax,
            self.vgdMax,
            self.vgbMax,
            self.vdsMax,
            self.vbsMax,
            self.vbdMax,
            self.vgsrMax,
            self.vgdrMax,
            self.vgbrMax,
            self.vbsrMax,
            self.vbdrMax,
            f64::from(self.InstCount),
            f64::from(u8::from(self.mobModGiven)),
            f64::from(u8::from(self.binUnitGiven)),
            f64::from(u8::from(self.capModGiven)),
            f64::from(u8::from(self.acmModGiven)),
            f64::from(u8::from(self.calcacmGiven)),
            f64::from(u8::from(self.paramChkGiven)),
            f64::from(u8::from(self.noiModGiven)),
            f64::from(u8::from(self.nqsModGiven)),
            f64::from(u8::from(self.acnqsModGiven)),
            f64::from(u8::from(self.typeGiven)),
            f64::from(u8::from(self.toxGiven)),
            f64::from(u8::from(self.versionGiven)),
            f64::from(u8::from(self.toxmGiven)),
            f64::from(u8::from(self.cdscGiven)),
            f64::from(u8::from(self.cdscbGiven)),
            f64::from(u8::from(self.cdscdGiven)),
            f64::from(u8::from(self.citGiven)),
            f64::from(u8::from(self.nfactorGiven)),
            f64::from(u8::from(self.xjGiven)),
            f64::from(u8::from(self.vsatGiven)),
            f64::from(u8::from(self.atGiven)),
            f64::from(u8::from(self.a0Given)),
            f64::from(u8::from(self.agsGiven)),
            f64::from(u8::from(self.a1Given)),
            f64::from(u8::from(self.a2Given)),
            f64::from(u8::from(self.ketaGiven)),
            f64::from(u8::from(self.nsubGiven)),
            f64::from(u8::from(self.npeakGiven)),
            f64::from(u8::from(self.ngateGiven)),
            f64::from(u8::from(self.gamma1Given)),
            f64::from(u8::from(self.gamma2Given)),
            f64::from(u8::from(self.vbxGiven)),
            f64::from(u8::from(self.vbmGiven)),
            f64::from(u8::from(self.xtGiven)),
            f64::from(u8::from(self.k1Given)),
            f64::from(u8::from(self.kt1Given)),
            f64::from(u8::from(self.kt1lGiven)),
            f64::from(u8::from(self.kt2Given)),
            f64::from(u8::from(self.k2Given)),
            f64::from(u8::from(self.k3Given)),
            f64::from(u8::from(self.k3bGiven)),
            f64::from(u8::from(self.w0Given)),
            f64::from(u8::from(self.nlxGiven)),
            f64::from(u8::from(self.dvt0Given)),
            f64::from(u8::from(self.dvt1Given)),
            f64::from(u8::from(self.dvt2Given)),
            f64::from(u8::from(self.dvt0wGiven)),
            f64::from(u8::from(self.dvt1wGiven)),
            f64::from(u8::from(self.dvt2wGiven)),
            f64::from(u8::from(self.droutGiven)),
            f64::from(u8::from(self.dsubGiven)),
            f64::from(u8::from(self.vth0Given)),
            f64::from(u8::from(self.uaGiven)),
            f64::from(u8::from(self.ua1Given)),
            f64::from(u8::from(self.ubGiven)),
            f64::from(u8::from(self.ub1Given)),
            f64::from(u8::from(self.ucGiven)),
            f64::from(u8::from(self.uc1Given)),
            f64::from(u8::from(self.u0Given)),
            f64::from(u8::from(self.uteGiven)),
            f64::from(u8::from(self.voffGiven)),
            f64::from(u8::from(self.rdswGiven)),
            f64::from(u8::from(self.prwgGiven)),
            f64::from(u8::from(self.prwbGiven)),
            f64::from(u8::from(self.prtGiven)),
            f64::from(u8::from(self.eta0Given)),
            f64::from(u8::from(self.etabGiven)),
            f64::from(u8::from(self.pclmGiven)),
            f64::from(u8::from(self.pdibl1Given)),
            f64::from(u8::from(self.pdibl2Given)),
            f64::from(u8::from(self.pdiblbGiven)),
            f64::from(u8::from(self.pscbe1Given)),
            f64::from(u8::from(self.pscbe2Given)),
            f64::from(u8::from(self.pvagGiven)),
            f64::from(u8::from(self.deltaGiven)),
            f64::from(u8::from(self.wrGiven)),
            f64::from(u8::from(self.dwgGiven)),
            f64::from(u8::from(self.dwbGiven)),
            f64::from(u8::from(self.b0Given)),
            f64::from(u8::from(self.b1Given)),
            f64::from(u8::from(self.alpha0Given)),
            f64::from(u8::from(self.alpha1Given)),
            f64::from(u8::from(self.beta0Given)),
            f64::from(u8::from(self.ijthGiven)),
            f64::from(u8::from(self.vfbGiven)),
            f64::from(u8::from(self.elmGiven)),
            f64::from(u8::from(self.cgslGiven)),
            f64::from(u8::from(self.cgdlGiven)),
            f64::from(u8::from(self.ckappaGiven)),
            f64::from(u8::from(self.cfGiven)),
            f64::from(u8::from(self.vfbcvGiven)),
            f64::from(u8::from(self.clcGiven)),
            f64::from(u8::from(self.cleGiven)),
            f64::from(u8::from(self.dwcGiven)),
            f64::from(u8::from(self.dlcGiven)),
            f64::from(u8::from(self.noffGiven)),
            f64::from(u8::from(self.voffcvGiven)),
            f64::from(u8::from(self.acdeGiven)),
            f64::from(u8::from(self.moinGiven)),
            f64::from(u8::from(self.tcjGiven)),
            f64::from(u8::from(self.tcjswGiven)),
            f64::from(u8::from(self.tcjswgGiven)),
            f64::from(u8::from(self.tpbGiven)),
            f64::from(u8::from(self.tpbswGiven)),
            f64::from(u8::from(self.tpbswgGiven)),
            f64::from(u8::from(self.xlGiven)),
            f64::from(u8::from(self.xwGiven)),
            f64::from(u8::from(self.hdifGiven)),
            f64::from(u8::from(self.ldifGiven)),
            f64::from(u8::from(self.ldGiven)),
            f64::from(u8::from(self.rdGiven)),
            f64::from(u8::from(self.rsGiven)),
            f64::from(u8::from(self.rdcGiven)),
            f64::from(u8::from(self.rscGiven)),
            f64::from(u8::from(self.wmltGiven)),
            f64::from(u8::from(self.lcdscGiven)),
            f64::from(u8::from(self.lcdscbGiven)),
            f64::from(u8::from(self.lcdscdGiven)),
            f64::from(u8::from(self.lcitGiven)),
            f64::from(u8::from(self.lnfactorGiven)),
            f64::from(u8::from(self.lxjGiven)),
            f64::from(u8::from(self.lvsatGiven)),
            f64::from(u8::from(self.latGiven)),
            f64::from(u8::from(self.la0Given)),
            f64::from(u8::from(self.lagsGiven)),
            f64::from(u8::from(self.la1Given)),
            f64::from(u8::from(self.la2Given)),
            f64::from(u8::from(self.lketaGiven)),
            f64::from(u8::from(self.lnsubGiven)),
            f64::from(u8::from(self.lnpeakGiven)),
            f64::from(u8::from(self.lngateGiven)),
            f64::from(u8::from(self.lgamma1Given)),
            f64::from(u8::from(self.lgamma2Given)),
            f64::from(u8::from(self.lvbxGiven)),
            f64::from(u8::from(self.lvbmGiven)),
            f64::from(u8::from(self.lxtGiven)),
            f64::from(u8::from(self.lk1Given)),
            f64::from(u8::from(self.lkt1Given)),
            f64::from(u8::from(self.lkt1lGiven)),
            f64::from(u8::from(self.lkt2Given)),
            f64::from(u8::from(self.lk2Given)),
            f64::from(u8::from(self.lk3Given)),
            f64::from(u8::from(self.lk3bGiven)),
            f64::from(u8::from(self.lw0Given)),
            f64::from(u8::from(self.lnlxGiven)),
            f64::from(u8::from(self.ldvt0Given)),
            f64::from(u8::from(self.ldvt1Given)),
            f64::from(u8::from(self.ldvt2Given)),
            f64::from(u8::from(self.ldvt0wGiven)),
            f64::from(u8::from(self.ldvt1wGiven)),
            f64::from(u8::from(self.ldvt2wGiven)),
            f64::from(u8::from(self.ldroutGiven)),
            f64::from(u8::from(self.ldsubGiven)),
            f64::from(u8::from(self.lvth0Given)),
            f64::from(u8::from(self.luaGiven)),
            f64::from(u8::from(self.lua1Given)),
            f64::from(u8::from(self.lubGiven)),
            f64::from(u8::from(self.lub1Given)),
            f64::from(u8::from(self.lucGiven)),
            f64::from(u8::from(self.luc1Given)),
            f64::from(u8::from(self.lu0Given)),
            f64::from(u8::from(self.luteGiven)),
            f64::from(u8::from(self.lvoffGiven)),
            f64::from(u8::from(self.lrdswGiven)),
            f64::from(u8::from(self.lprwgGiven)),
            f64::from(u8::from(self.lprwbGiven)),
            f64::from(u8::from(self.lprtGiven)),
            f64::from(u8::from(self.leta0Given)),
            f64::from(u8::from(self.letabGiven)),
            f64::from(u8::from(self.lpclmGiven)),
            f64::from(u8::from(self.lpdibl1Given)),
            f64::from(u8::from(self.lpdibl2Given)),
            f64::from(u8::from(self.lpdiblbGiven)),
            f64::from(u8::from(self.lpscbe1Given)),
            f64::from(u8::from(self.lpscbe2Given)),
            f64::from(u8::from(self.lpvagGiven)),
            f64::from(u8::from(self.ldeltaGiven)),
            f64::from(u8::from(self.lwrGiven)),
            f64::from(u8::from(self.ldwgGiven)),
            f64::from(u8::from(self.ldwbGiven)),
            f64::from(u8::from(self.lb0Given)),
            f64::from(u8::from(self.lb1Given)),
            f64::from(u8::from(self.lalpha0Given)),
            f64::from(u8::from(self.lalpha1Given)),
            f64::from(u8::from(self.lbeta0Given)),
            f64::from(u8::from(self.lvfbGiven)),
            f64::from(u8::from(self.lelmGiven)),
            f64::from(u8::from(self.lcgslGiven)),
            f64::from(u8::from(self.lcgdlGiven)),
            f64::from(u8::from(self.lckappaGiven)),
            f64::from(u8::from(self.lcfGiven)),
            f64::from(u8::from(self.lclcGiven)),
            f64::from(u8::from(self.lcleGiven)),
            f64::from(u8::from(self.lvfbcvGiven)),
            f64::from(u8::from(self.lnoffGiven)),
            f64::from(u8::from(self.lvoffcvGiven)),
            f64::from(u8::from(self.lacdeGiven)),
            f64::from(u8::from(self.lmoinGiven)),
            f64::from(u8::from(self.wcdscGiven)),
            f64::from(u8::from(self.wcdscbGiven)),
            f64::from(u8::from(self.wcdscdGiven)),
            f64::from(u8::from(self.wcitGiven)),
            f64::from(u8::from(self.wnfactorGiven)),
            f64::from(u8::from(self.wxjGiven)),
            f64::from(u8::from(self.wvsatGiven)),
            f64::from(u8::from(self.watGiven)),
            f64::from(u8::from(self.wa0Given)),
            f64::from(u8::from(self.wagsGiven)),
            f64::from(u8::from(self.wa1Given)),
            f64::from(u8::from(self.wa2Given)),
            f64::from(u8::from(self.wketaGiven)),
            f64::from(u8::from(self.wnsubGiven)),
            f64::from(u8::from(self.wnpeakGiven)),
            f64::from(u8::from(self.wngateGiven)),
            f64::from(u8::from(self.wgamma1Given)),
            f64::from(u8::from(self.wgamma2Given)),
            f64::from(u8::from(self.wvbxGiven)),
            f64::from(u8::from(self.wvbmGiven)),
            f64::from(u8::from(self.wxtGiven)),
            f64::from(u8::from(self.wk1Given)),
            f64::from(u8::from(self.wkt1Given)),
            f64::from(u8::from(self.wkt1lGiven)),
            f64::from(u8::from(self.wkt2Given)),
            f64::from(u8::from(self.wk2Given)),
            f64::from(u8::from(self.wk3Given)),
            f64::from(u8::from(self.wk3bGiven)),
            f64::from(u8::from(self.ww0Given)),
            f64::from(u8::from(self.wnlxGiven)),
            f64::from(u8::from(self.wdvt0Given)),
            f64::from(u8::from(self.wdvt1Given)),
            f64::from(u8::from(self.wdvt2Given)),
            f64::from(u8::from(self.wdvt0wGiven)),
            f64::from(u8::from(self.wdvt1wGiven)),
            f64::from(u8::from(self.wdvt2wGiven)),
            f64::from(u8::from(self.wdroutGiven)),
            f64::from(u8::from(self.wdsubGiven)),
            f64::from(u8::from(self.wvth0Given)),
            f64::from(u8::from(self.wuaGiven)),
            f64::from(u8::from(self.wua1Given)),
            f64::from(u8::from(self.wubGiven)),
            f64::from(u8::from(self.wub1Given)),
            f64::from(u8::from(self.wucGiven)),
            f64::from(u8::from(self.wuc1Given)),
            f64::from(u8::from(self.wu0Given)),
            f64::from(u8::from(self.wuteGiven)),
            f64::from(u8::from(self.wvoffGiven)),
            f64::from(u8::from(self.wrdswGiven)),
            f64::from(u8::from(self.wprwgGiven)),
            f64::from(u8::from(self.wprwbGiven)),
            f64::from(u8::from(self.wprtGiven)),
            f64::from(u8::from(self.weta0Given)),
            f64::from(u8::from(self.wetabGiven)),
            f64::from(u8::from(self.wpclmGiven)),
            f64::from(u8::from(self.wpdibl1Given)),
            f64::from(u8::from(self.wpdibl2Given)),
            f64::from(u8::from(self.wpdiblbGiven)),
            f64::from(u8::from(self.wpscbe1Given)),
            f64::from(u8::from(self.wpscbe2Given)),
            f64::from(u8::from(self.wpvagGiven)),
            f64::from(u8::from(self.wdeltaGiven)),
            f64::from(u8::from(self.wwrGiven)),
            f64::from(u8::from(self.wdwgGiven)),
            f64::from(u8::from(self.wdwbGiven)),
            f64::from(u8::from(self.wb0Given)),
            f64::from(u8::from(self.wb1Given)),
            f64::from(u8::from(self.walpha0Given)),
            f64::from(u8::from(self.walpha1Given)),
            f64::from(u8::from(self.wbeta0Given)),
            f64::from(u8::from(self.wvfbGiven)),
            f64::from(u8::from(self.welmGiven)),
            f64::from(u8::from(self.wcgslGiven)),
            f64::from(u8::from(self.wcgdlGiven)),
            f64::from(u8::from(self.wckappaGiven)),
            f64::from(u8::from(self.wcfGiven)),
            f64::from(u8::from(self.wclcGiven)),
            f64::from(u8::from(self.wcleGiven)),
            f64::from(u8::from(self.wvfbcvGiven)),
            f64::from(u8::from(self.wnoffGiven)),
            f64::from(u8::from(self.wvoffcvGiven)),
            f64::from(u8::from(self.wacdeGiven)),
            f64::from(u8::from(self.wmoinGiven)),
            f64::from(u8::from(self.pcdscGiven)),
            f64::from(u8::from(self.pcdscbGiven)),
            f64::from(u8::from(self.pcdscdGiven)),
            f64::from(u8::from(self.pcitGiven)),
            f64::from(u8::from(self.pnfactorGiven)),
            f64::from(u8::from(self.pxjGiven)),
            f64::from(u8::from(self.pvsatGiven)),
            f64::from(u8::from(self.patGiven)),
            f64::from(u8::from(self.pa0Given)),
            f64::from(u8::from(self.pagsGiven)),
            f64::from(u8::from(self.pa1Given)),
            f64::from(u8::from(self.pa2Given)),
            f64::from(u8::from(self.pketaGiven)),
            f64::from(u8::from(self.pnsubGiven)),
            f64::from(u8::from(self.pnpeakGiven)),
            f64::from(u8::from(self.pngateGiven)),
            f64::from(u8::from(self.pgamma1Given)),
            f64::from(u8::from(self.pgamma2Given)),
            f64::from(u8::from(self.pvbxGiven)),
            f64::from(u8::from(self.pvbmGiven)),
            f64::from(u8::from(self.pxtGiven)),
            f64::from(u8::from(self.pk1Given)),
            f64::from(u8::from(self.pkt1Given)),
            f64::from(u8::from(self.pkt1lGiven)),
            f64::from(u8::from(self.pkt2Given)),
            f64::from(u8::from(self.pk2Given)),
            f64::from(u8::from(self.pk3Given)),
            f64::from(u8::from(self.pk3bGiven)),
            f64::from(u8::from(self.pw0Given)),
            f64::from(u8::from(self.pnlxGiven)),
            f64::from(u8::from(self.pdvt0Given)),
            f64::from(u8::from(self.pdvt1Given)),
            f64::from(u8::from(self.pdvt2Given)),
            f64::from(u8::from(self.pdvt0wGiven)),
            f64::from(u8::from(self.pdvt1wGiven)),
            f64::from(u8::from(self.pdvt2wGiven)),
            f64::from(u8::from(self.pdroutGiven)),
            f64::from(u8::from(self.pdsubGiven)),
            f64::from(u8::from(self.pvth0Given)),
            f64::from(u8::from(self.puaGiven)),
            f64::from(u8::from(self.pua1Given)),
            f64::from(u8::from(self.pubGiven)),
            f64::from(u8::from(self.pub1Given)),
            f64::from(u8::from(self.pucGiven)),
            f64::from(u8::from(self.puc1Given)),
            f64::from(u8::from(self.pu0Given)),
            f64::from(u8::from(self.puteGiven)),
            f64::from(u8::from(self.pvoffGiven)),
            f64::from(u8::from(self.prdswGiven)),
            f64::from(u8::from(self.pprwgGiven)),
            f64::from(u8::from(self.pprwbGiven)),
            f64::from(u8::from(self.pprtGiven)),
            f64::from(u8::from(self.peta0Given)),
            f64::from(u8::from(self.petabGiven)),
            f64::from(u8::from(self.ppclmGiven)),
            f64::from(u8::from(self.ppdibl1Given)),
            f64::from(u8::from(self.ppdibl2Given)),
            f64::from(u8::from(self.ppdiblbGiven)),
            f64::from(u8::from(self.ppscbe1Given)),
            f64::from(u8::from(self.ppscbe2Given)),
            f64::from(u8::from(self.ppvagGiven)),
            f64::from(u8::from(self.pdeltaGiven)),
            f64::from(u8::from(self.pwrGiven)),
            f64::from(u8::from(self.pdwgGiven)),
            f64::from(u8::from(self.pdwbGiven)),
            f64::from(u8::from(self.pb0Given)),
            f64::from(u8::from(self.pb1Given)),
            f64::from(u8::from(self.palpha0Given)),
            f64::from(u8::from(self.palpha1Given)),
            f64::from(u8::from(self.pbeta0Given)),
            f64::from(u8::from(self.pvfbGiven)),
            f64::from(u8::from(self.pelmGiven)),
            f64::from(u8::from(self.pcgslGiven)),
            f64::from(u8::from(self.pcgdlGiven)),
            f64::from(u8::from(self.pckappaGiven)),
            f64::from(u8::from(self.pcfGiven)),
            f64::from(u8::from(self.pclcGiven)),
            f64::from(u8::from(self.pcleGiven)),
            f64::from(u8::from(self.pvfbcvGiven)),
            f64::from(u8::from(self.pnoffGiven)),
            f64::from(u8::from(self.pvoffcvGiven)),
            f64::from(u8::from(self.pacdeGiven)),
            f64::from(u8::from(self.pmoinGiven)),
            f64::from(u8::from(self.useFringeGiven)),
            f64::from(u8::from(self.tnomGiven)),
            f64::from(u8::from(self.cgsoGiven)),
            f64::from(u8::from(self.cgdoGiven)),
            f64::from(u8::from(self.cgboGiven)),
            f64::from(u8::from(self.xpartGiven)),
            f64::from(u8::from(self.sheetResistanceGiven)),
            f64::from(u8::from(self.jctSatCurDensityGiven)),
            f64::from(u8::from(self.jctSidewallSatCurDensityGiven)),
            f64::from(u8::from(self.bulkJctPotentialGiven)),
            f64::from(u8::from(self.bulkJctBotGradingCoeffGiven)),
            f64::from(u8::from(self.sidewallJctPotentialGiven)),
            f64::from(u8::from(self.GatesidewallJctPotentialGiven)),
            f64::from(u8::from(self.bulkJctSideGradingCoeffGiven)),
            f64::from(u8::from(self.unitAreaJctCapGiven)),
            f64::from(u8::from(self.unitLengthSidewallJctCapGiven)),
            f64::from(u8::from(self.bulkJctGateSideGradingCoeffGiven)),
            f64::from(u8::from(self.unitLengthGateSidewallJctCapGiven)),
            f64::from(u8::from(self.jctEmissionCoeffGiven)),
            f64::from(u8::from(self.jctTempExponentGiven)),
            f64::from(u8::from(self.oxideTrapDensityAGiven)),
            f64::from(u8::from(self.oxideTrapDensityBGiven)),
            f64::from(u8::from(self.oxideTrapDensityCGiven)),
            f64::from(u8::from(self.emGiven)),
            f64::from(u8::from(self.efGiven)),
            f64::from(u8::from(self.afGiven)),
            f64::from(u8::from(self.kfGiven)),
            f64::from(u8::from(self.lintnoiGiven)),
            f64::from(u8::from(self.vgsMaxGiven)),
            f64::from(u8::from(self.vgdMaxGiven)),
            f64::from(u8::from(self.vgbMaxGiven)),
            f64::from(u8::from(self.vdsMaxGiven)),
            f64::from(u8::from(self.vbsMaxGiven)),
            f64::from(u8::from(self.vbdMaxGiven)),
            f64::from(u8::from(self.vgsrMaxGiven)),
            f64::from(u8::from(self.vgdrMaxGiven)),
            f64::from(u8::from(self.vgbrMaxGiven)),
            f64::from(u8::from(self.vbsrMaxGiven)),
            f64::from(u8::from(self.vbdrMaxGiven)),
            f64::from(u8::from(self.LintGiven)),
            f64::from(u8::from(self.LlGiven)),
            f64::from(u8::from(self.LlcGiven)),
            f64::from(u8::from(self.LlnGiven)),
            f64::from(u8::from(self.LwGiven)),
            f64::from(u8::from(self.LwcGiven)),
            f64::from(u8::from(self.LwnGiven)),
            f64::from(u8::from(self.LwlGiven)),
            f64::from(u8::from(self.LwlcGiven)),
            f64::from(u8::from(self.LminGiven)),
            f64::from(u8::from(self.LmaxGiven)),
            f64::from(u8::from(self.WintGiven)),
            f64::from(u8::from(self.WlGiven)),
            f64::from(u8::from(self.WlcGiven)),
            f64::from(u8::from(self.WlnGiven)),
            f64::from(u8::from(self.WwGiven)),
            f64::from(u8::from(self.WwcGiven)),
            f64::from(u8::from(self.WwnGiven)),
            f64::from(u8::from(self.WwlGiven)),
            f64::from(u8::from(self.WwlcGiven)),
            f64::from(u8::from(self.WminGiven)),
            f64::from(u8::from(self.WmaxGiven)),
        ]
    }

    /// Stores numbers produced by [`Self::to_values`]; `values` must have
    /// [`Self::MEMBERS`]`.len()` entries.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub(crate) fn set_values(&mut self, values: &[f64]) {
        self.r#type = values[0] as i32;
        self.mobMod = values[1] as i32;
        self.capMod = values[2] as i32;
        self.acmMod = values[3] as i32;
        self.calcacm = values[4] as i32;
        self.noiMod = values[5] as i32;
        self.nqsMod = values[6] as i32;
        self.acnqsMod = values[7] as i32;
        self.binUnit = values[8] as i32;
        self.paramChk = values[9] as i32;
        self.tox = values[10];
        self.toxm = values[11];
        self.cdsc = values[12];
        self.cdscb = values[13];
        self.cdscd = values[14];
        self.cit = values[15];
        self.nfactor = values[16];
        self.xj = values[17];
        self.vsat = values[18];
        self.at = values[19];
        self.a0 = values[20];
        self.ags = values[21];
        self.a1 = values[22];
        self.a2 = values[23];
        self.keta = values[24];
        self.nsub = values[25];
        self.npeak = values[26];
        self.ngate = values[27];
        self.gamma1 = values[28];
        self.gamma2 = values[29];
        self.vbx = values[30];
        self.vbm = values[31];
        self.xt = values[32];
        self.k1 = values[33];
        self.kt1 = values[34];
        self.kt1l = values[35];
        self.kt2 = values[36];
        self.k2 = values[37];
        self.k3 = values[38];
        self.k3b = values[39];
        self.w0 = values[40];
        self.nlx = values[41];
        self.dvt0 = values[42];
        self.dvt1 = values[43];
        self.dvt2 = values[44];
        self.dvt0w = values[45];
        self.dvt1w = values[46];
        self.dvt2w = values[47];
        self.drout = values[48];
        self.dsub = values[49];
        self.vth0 = values[50];
        self.ua = values[51];
        self.ua1 = values[52];
        self.ub = values[53];
        self.ub1 = values[54];
        self.uc = values[55];
        self.uc1 = values[56];
        self.u0 = values[57];
        self.ute = values[58];
        self.voff = values[59];
        self.delta = values[60];
        self.rdsw = values[61];
        self.prwg = values[62];
        self.prwb = values[63];
        self.prt = values[64];
        self.eta0 = values[65];
        self.etab = values[66];
        self.pclm = values[67];
        self.pdibl1 = values[68];
        self.pdibl2 = values[69];
        self.pdiblb = values[70];
        self.pscbe1 = values[71];
        self.pscbe2 = values[72];
        self.pvag = values[73];
        self.wr = values[74];
        self.dwg = values[75];
        self.dwb = values[76];
        self.b0 = values[77];
        self.b1 = values[78];
        self.alpha0 = values[79];
        self.alpha1 = values[80];
        self.beta0 = values[81];
        self.ijth = values[82];
        self.vfb = values[83];
        self.elm = values[84];
        self.cgsl = values[85];
        self.cgdl = values[86];
        self.ckappa = values[87];
        self.cf = values[88];
        self.vfbcv = values[89];
        self.clc = values[90];
        self.cle = values[91];
        self.dwc = values[92];
        self.dlc = values[93];
        self.noff = values[94];
        self.voffcv = values[95];
        self.acde = values[96];
        self.moin = values[97];
        self.tcj = values[98];
        self.tcjsw = values[99];
        self.tcjswg = values[100];
        self.tpb = values[101];
        self.tpbsw = values[102];
        self.tpbswg = values[103];
        self.xl = values[104];
        self.xw = values[105];
        self.hdif = values[106];
        self.ldif = values[107];
        self.ld = values[108];
        self.rd = values[109];
        self.rs = values[110];
        self.rdc = values[111];
        self.rsc = values[112];
        self.wmlt = values[113];
        self.lcdsc = values[114];
        self.lcdscb = values[115];
        self.lcdscd = values[116];
        self.lcit = values[117];
        self.lnfactor = values[118];
        self.lxj = values[119];
        self.lvsat = values[120];
        self.lat = values[121];
        self.la0 = values[122];
        self.lags = values[123];
        self.la1 = values[124];
        self.la2 = values[125];
        self.lketa = values[126];
        self.lnsub = values[127];
        self.lnpeak = values[128];
        self.lngate = values[129];
        self.lgamma1 = values[130];
        self.lgamma2 = values[131];
        self.lvbx = values[132];
        self.lvbm = values[133];
        self.lxt = values[134];
        self.lk1 = values[135];
        self.lkt1 = values[136];
        self.lkt1l = values[137];
        self.lkt2 = values[138];
        self.lk2 = values[139];
        self.lk3 = values[140];
        self.lk3b = values[141];
        self.lw0 = values[142];
        self.lnlx = values[143];
        self.ldvt0 = values[144];
        self.ldvt1 = values[145];
        self.ldvt2 = values[146];
        self.ldvt0w = values[147];
        self.ldvt1w = values[148];
        self.ldvt2w = values[149];
        self.ldrout = values[150];
        self.ldsub = values[151];
        self.lvth0 = values[152];
        self.lua = values[153];
        self.lua1 = values[154];
        self.lub = values[155];
        self.lub1 = values[156];
        self.luc = values[157];
        self.luc1 = values[158];
        self.lu0 = values[159];
        self.lute = values[160];
        self.lvoff = values[161];
        self.ldelta = values[162];
        self.lrdsw = values[163];
        self.lprwg = values[164];
        self.lprwb = values[165];
        self.lprt = values[166];
        self.leta0 = values[167];
        self.letab = values[168];
        self.lpclm = values[169];
        self.lpdibl1 = values[170];
        self.lpdibl2 = values[171];
        self.lpdiblb = values[172];
        self.lpscbe1 = values[173];
        self.lpscbe2 = values[174];
        self.lpvag = values[175];
        self.lwr = values[176];
        self.ldwg = values[177];
        self.ldwb = values[178];
        self.lb0 = values[179];
        self.lb1 = values[180];
        self.lalpha0 = values[181];
        self.lalpha1 = values[182];
        self.lbeta0 = values[183];
        self.lvfb = values[184];
        self.lelm = values[185];
        self.lcgsl = values[186];
        self.lcgdl = values[187];
        self.lckappa = values[188];
        self.lcf = values[189];
        self.lclc = values[190];
        self.lcle = values[191];
        self.lvfbcv = values[192];
        self.lnoff = values[193];
        self.lvoffcv = values[194];
        self.lacde = values[195];
        self.lmoin = values[196];
        self.wcdsc = values[197];
        self.wcdscb = values[198];
        self.wcdscd = values[199];
        self.wcit = values[200];
        self.wnfactor = values[201];
        self.wxj = values[202];
        self.wvsat = values[203];
        self.wat = values[204];
        self.wa0 = values[205];
        self.wags = values[206];
        self.wa1 = values[207];
        self.wa2 = values[208];
        self.wketa = values[209];
        self.wnsub = values[210];
        self.wnpeak = values[211];
        self.wngate = values[212];
        self.wgamma1 = values[213];
        self.wgamma2 = values[214];
        self.wvbx = values[215];
        self.wvbm = values[216];
        self.wxt = values[217];
        self.wk1 = values[218];
        self.wkt1 = values[219];
        self.wkt1l = values[220];
        self.wkt2 = values[221];
        self.wk2 = values[222];
        self.wk3 = values[223];
        self.wk3b = values[224];
        self.ww0 = values[225];
        self.wnlx = values[226];
        self.wdvt0 = values[227];
        self.wdvt1 = values[228];
        self.wdvt2 = values[229];
        self.wdvt0w = values[230];
        self.wdvt1w = values[231];
        self.wdvt2w = values[232];
        self.wdrout = values[233];
        self.wdsub = values[234];
        self.wvth0 = values[235];
        self.wua = values[236];
        self.wua1 = values[237];
        self.wub = values[238];
        self.wub1 = values[239];
        self.wuc = values[240];
        self.wuc1 = values[241];
        self.wu0 = values[242];
        self.wute = values[243];
        self.wvoff = values[244];
        self.wdelta = values[245];
        self.wrdsw = values[246];
        self.wprwg = values[247];
        self.wprwb = values[248];
        self.wprt = values[249];
        self.weta0 = values[250];
        self.wetab = values[251];
        self.wpclm = values[252];
        self.wpdibl1 = values[253];
        self.wpdibl2 = values[254];
        self.wpdiblb = values[255];
        self.wpscbe1 = values[256];
        self.wpscbe2 = values[257];
        self.wpvag = values[258];
        self.wwr = values[259];
        self.wdwg = values[260];
        self.wdwb = values[261];
        self.wb0 = values[262];
        self.wb1 = values[263];
        self.walpha0 = values[264];
        self.walpha1 = values[265];
        self.wbeta0 = values[266];
        self.wvfb = values[267];
        self.welm = values[268];
        self.wcgsl = values[269];
        self.wcgdl = values[270];
        self.wckappa = values[271];
        self.wcf = values[272];
        self.wclc = values[273];
        self.wcle = values[274];
        self.wvfbcv = values[275];
        self.wnoff = values[276];
        self.wvoffcv = values[277];
        self.wacde = values[278];
        self.wmoin = values[279];
        self.pcdsc = values[280];
        self.pcdscb = values[281];
        self.pcdscd = values[282];
        self.pcit = values[283];
        self.pnfactor = values[284];
        self.pxj = values[285];
        self.pvsat = values[286];
        self.pat = values[287];
        self.pa0 = values[288];
        self.pags = values[289];
        self.pa1 = values[290];
        self.pa2 = values[291];
        self.pketa = values[292];
        self.pnsub = values[293];
        self.pnpeak = values[294];
        self.pngate = values[295];
        self.pgamma1 = values[296];
        self.pgamma2 = values[297];
        self.pvbx = values[298];
        self.pvbm = values[299];
        self.pxt = values[300];
        self.pk1 = values[301];
        self.pkt1 = values[302];
        self.pkt1l = values[303];
        self.pkt2 = values[304];
        self.pk2 = values[305];
        self.pk3 = values[306];
        self.pk3b = values[307];
        self.pw0 = values[308];
        self.pnlx = values[309];
        self.pdvt0 = values[310];
        self.pdvt1 = values[311];
        self.pdvt2 = values[312];
        self.pdvt0w = values[313];
        self.pdvt1w = values[314];
        self.pdvt2w = values[315];
        self.pdrout = values[316];
        self.pdsub = values[317];
        self.pvth0 = values[318];
        self.pua = values[319];
        self.pua1 = values[320];
        self.r#pub = values[321];
        self.pub1 = values[322];
        self.puc = values[323];
        self.puc1 = values[324];
        self.pu0 = values[325];
        self.pute = values[326];
        self.pvoff = values[327];
        self.pdelta = values[328];
        self.prdsw = values[329];
        self.pprwg = values[330];
        self.pprwb = values[331];
        self.pprt = values[332];
        self.peta0 = values[333];
        self.petab = values[334];
        self.ppclm = values[335];
        self.ppdibl1 = values[336];
        self.ppdibl2 = values[337];
        self.ppdiblb = values[338];
        self.ppscbe1 = values[339];
        self.ppscbe2 = values[340];
        self.ppvag = values[341];
        self.pwr = values[342];
        self.pdwg = values[343];
        self.pdwb = values[344];
        self.pb0 = values[345];
        self.pb1 = values[346];
        self.palpha0 = values[347];
        self.palpha1 = values[348];
        self.pbeta0 = values[349];
        self.pvfb = values[350];
        self.pelm = values[351];
        self.pcgsl = values[352];
        self.pcgdl = values[353];
        self.pckappa = values[354];
        self.pcf = values[355];
        self.pclc = values[356];
        self.pcle = values[357];
        self.pvfbcv = values[358];
        self.pnoff = values[359];
        self.pvoffcv = values[360];
        self.pacde = values[361];
        self.pmoin = values[362];
        self.tnom = values[363];
        self.cgso = values[364];
        self.cgdo = values[365];
        self.cgbo = values[366];
        self.xpart = values[367];
        self.cFringOut = values[368];
        self.cFringMax = values[369];
        self.sheetResistance = values[370];
        self.jctSatCurDensity = values[371];
        self.jctSidewallSatCurDensity = values[372];
        self.bulkJctPotential = values[373];
        self.bulkJctBotGradingCoeff = values[374];
        self.bulkJctSideGradingCoeff = values[375];
        self.bulkJctGateSideGradingCoeff = values[376];
        self.sidewallJctPotential = values[377];
        self.GatesidewallJctPotential = values[378];
        self.unitAreaJctCap = values[379];
        self.unitLengthSidewallJctCap = values[380];
        self.unitLengthGateSidewallJctCap = values[381];
        self.jctEmissionCoeff = values[382];
        self.jctTempExponent = values[383];
        self.Lint = values[384];
        self.Ll = values[385];
        self.Llc = values[386];
        self.Lln = values[387];
        self.Lw = values[388];
        self.Lwc = values[389];
        self.Lwn = values[390];
        self.Lwl = values[391];
        self.Lwlc = values[392];
        self.Lmin = values[393];
        self.Lmax = values[394];
        self.Wint = values[395];
        self.Wl = values[396];
        self.Wlc = values[397];
        self.Wln = values[398];
        self.Ww = values[399];
        self.Wwc = values[400];
        self.Wwn = values[401];
        self.Wwl = values[402];
        self.Wwlc = values[403];
        self.Wmin = values[404];
        self.Wmax = values[405];
        self.vtm = values[406];
        self.cox = values[407];
        self.cof1 = values[408];
        self.cof2 = values[409];
        self.cof3 = values[410];
        self.cof4 = values[411];
        self.vcrit = values[412];
        self.factor1 = values[413];
        self.PhiB = values[414];
        self.PhiBSW = values[415];
        self.PhiBSWG = values[416];
        self.jctTempSatCurDensity = values[417];
        self.jctSidewallTempSatCurDensity = values[418];
        self.unitAreaTempJctCap = values[419];
        self.unitLengthSidewallTempJctCap = values[420];
        self.unitLengthGateSidewallTempJctCap = values[421];
        self.oxideTrapDensityA = values[422];
        self.oxideTrapDensityB = values[423];
        self.oxideTrapDensityC = values[424];
        self.em = values[425];
        self.ef = values[426];
        self.af = values[427];
        self.kf = values[428];
        self.lintnoi = values[429];
        self.vgsMax = values[430];
        self.vgdMax = values[431];
        self.vgbMax = values[432];
        self.vdsMax = values[433];
        self.vbsMax = values[434];
        self.vbdMax = values[435];
        self.vgsrMax = values[436];
        self.vgdrMax = values[437];
        self.vgbrMax = values[438];
        self.vbsrMax = values[439];
        self.vbdrMax = values[440];
        self.InstCount = values[441] as i32;
        self.mobModGiven = values[442] != 0.0;
        self.binUnitGiven = values[443] != 0.0;
        self.capModGiven = values[444] != 0.0;
        self.acmModGiven = values[445] != 0.0;
        self.calcacmGiven = values[446] != 0.0;
        self.paramChkGiven = values[447] != 0.0;
        self.noiModGiven = values[448] != 0.0;
        self.nqsModGiven = values[449] != 0.0;
        self.acnqsModGiven = values[450] != 0.0;
        self.typeGiven = values[451] != 0.0;
        self.toxGiven = values[452] != 0.0;
        self.versionGiven = values[453] != 0.0;
        self.toxmGiven = values[454] != 0.0;
        self.cdscGiven = values[455] != 0.0;
        self.cdscbGiven = values[456] != 0.0;
        self.cdscdGiven = values[457] != 0.0;
        self.citGiven = values[458] != 0.0;
        self.nfactorGiven = values[459] != 0.0;
        self.xjGiven = values[460] != 0.0;
        self.vsatGiven = values[461] != 0.0;
        self.atGiven = values[462] != 0.0;
        self.a0Given = values[463] != 0.0;
        self.agsGiven = values[464] != 0.0;
        self.a1Given = values[465] != 0.0;
        self.a2Given = values[466] != 0.0;
        self.ketaGiven = values[467] != 0.0;
        self.nsubGiven = values[468] != 0.0;
        self.npeakGiven = values[469] != 0.0;
        self.ngateGiven = values[470] != 0.0;
        self.gamma1Given = values[471] != 0.0;
        self.gamma2Given = values[472] != 0.0;
        self.vbxGiven = values[473] != 0.0;
        self.vbmGiven = values[474] != 0.0;
        self.xtGiven = values[475] != 0.0;
        self.k1Given = values[476] != 0.0;
        self.kt1Given = values[477] != 0.0;
        self.kt1lGiven = values[478] != 0.0;
        self.kt2Given = values[479] != 0.0;
        self.k2Given = values[480] != 0.0;
        self.k3Given = values[481] != 0.0;
        self.k3bGiven = values[482] != 0.0;
        self.w0Given = values[483] != 0.0;
        self.nlxGiven = values[484] != 0.0;
        self.dvt0Given = values[485] != 0.0;
        self.dvt1Given = values[486] != 0.0;
        self.dvt2Given = values[487] != 0.0;
        self.dvt0wGiven = values[488] != 0.0;
        self.dvt1wGiven = values[489] != 0.0;
        self.dvt2wGiven = values[490] != 0.0;
        self.droutGiven = values[491] != 0.0;
        self.dsubGiven = values[492] != 0.0;
        self.vth0Given = values[493] != 0.0;
        self.uaGiven = values[494] != 0.0;
        self.ua1Given = values[495] != 0.0;
        self.ubGiven = values[496] != 0.0;
        self.ub1Given = values[497] != 0.0;
        self.ucGiven = values[498] != 0.0;
        self.uc1Given = values[499] != 0.0;
        self.u0Given = values[500] != 0.0;
        self.uteGiven = values[501] != 0.0;
        self.voffGiven = values[502] != 0.0;
        self.rdswGiven = values[503] != 0.0;
        self.prwgGiven = values[504] != 0.0;
        self.prwbGiven = values[505] != 0.0;
        self.prtGiven = values[506] != 0.0;
        self.eta0Given = values[507] != 0.0;
        self.etabGiven = values[508] != 0.0;
        self.pclmGiven = values[509] != 0.0;
        self.pdibl1Given = values[510] != 0.0;
        self.pdibl2Given = values[511] != 0.0;
        self.pdiblbGiven = values[512] != 0.0;
        self.pscbe1Given = values[513] != 0.0;
        self.pscbe2Given = values[514] != 0.0;
        self.pvagGiven = values[515] != 0.0;
        self.deltaGiven = values[516] != 0.0;
        self.wrGiven = values[517] != 0.0;
        self.dwgGiven = values[518] != 0.0;
        self.dwbGiven = values[519] != 0.0;
        self.b0Given = values[520] != 0.0;
        self.b1Given = values[521] != 0.0;
        self.alpha0Given = values[522] != 0.0;
        self.alpha1Given = values[523] != 0.0;
        self.beta0Given = values[524] != 0.0;
        self.ijthGiven = values[525] != 0.0;
        self.vfbGiven = values[526] != 0.0;
        self.elmGiven = values[527] != 0.0;
        self.cgslGiven = values[528] != 0.0;
        self.cgdlGiven = values[529] != 0.0;
        self.ckappaGiven = values[530] != 0.0;
        self.cfGiven = values[531] != 0.0;
        self.vfbcvGiven = values[532] != 0.0;
        self.clcGiven = values[533] != 0.0;
        self.cleGiven = values[534] != 0.0;
        self.dwcGiven = values[535] != 0.0;
        self.dlcGiven = values[536] != 0.0;
        self.noffGiven = values[537] != 0.0;
        self.voffcvGiven = values[538] != 0.0;
        self.acdeGiven = values[539] != 0.0;
        self.moinGiven = values[540] != 0.0;
        self.tcjGiven = values[541] != 0.0;
        self.tcjswGiven = values[542] != 0.0;
        self.tcjswgGiven = values[543] != 0.0;
        self.tpbGiven = values[544] != 0.0;
        self.tpbswGiven = values[545] != 0.0;
        self.tpbswgGiven = values[546] != 0.0;
        self.xlGiven = values[547] != 0.0;
        self.xwGiven = values[548] != 0.0;
        self.hdifGiven = values[549] != 0.0;
        self.ldifGiven = values[550] != 0.0;
        self.ldGiven = values[551] != 0.0;
        self.rdGiven = values[552] != 0.0;
        self.rsGiven = values[553] != 0.0;
        self.rdcGiven = values[554] != 0.0;
        self.rscGiven = values[555] != 0.0;
        self.wmltGiven = values[556] != 0.0;
        self.lcdscGiven = values[557] != 0.0;
        self.lcdscbGiven = values[558] != 0.0;
        self.lcdscdGiven = values[559] != 0.0;
        self.lcitGiven = values[560] != 0.0;
        self.lnfactorGiven = values[561] != 0.0;
        self.lxjGiven = values[562] != 0.0;
        self.lvsatGiven = values[563] != 0.0;
        self.latGiven = values[564] != 0.0;
        self.la0Given = values[565] != 0.0;
        self.lagsGiven = values[566] != 0.0;
        self.la1Given = values[567] != 0.0;
        self.la2Given = values[568] != 0.0;
        self.lketaGiven = values[569] != 0.0;
        self.lnsubGiven = values[570] != 0.0;
        self.lnpeakGiven = values[571] != 0.0;
        self.lngateGiven = values[572] != 0.0;
        self.lgamma1Given = values[573] != 0.0;
        self.lgamma2Given = values[574] != 0.0;
        self.lvbxGiven = values[575] != 0.0;
        self.lvbmGiven = values[576] != 0.0;
        self.lxtGiven = values[577] != 0.0;
        self.lk1Given = values[578] != 0.0;
        self.lkt1Given = values[579] != 0.0;
        self.lkt1lGiven = values[580] != 0.0;
        self.lkt2Given = values[581] != 0.0;
        self.lk2Given = values[582] != 0.0;
        self.lk3Given = values[583] != 0.0;
        self.lk3bGiven = values[584] != 0.0;
        self.lw0Given = values[585] != 0.0;
        self.lnlxGiven = values[586] != 0.0;
        self.ldvt0Given = values[587] != 0.0;
        self.ldvt1Given = values[588] != 0.0;
        self.ldvt2Given = values[589] != 0.0;
        self.ldvt0wGiven = values[590] != 0.0;
        self.ldvt1wGiven = values[591] != 0.0;
        self.ldvt2wGiven = values[592] != 0.0;
        self.ldroutGiven = values[593] != 0.0;
        self.ldsubGiven = values[594] != 0.0;
        self.lvth0Given = values[595] != 0.0;
        self.luaGiven = values[596] != 0.0;
        self.lua1Given = values[597] != 0.0;
        self.lubGiven = values[598] != 0.0;
        self.lub1Given = values[599] != 0.0;
        self.lucGiven = values[600] != 0.0;
        self.luc1Given = values[601] != 0.0;
        self.lu0Given = values[602] != 0.0;
        self.luteGiven = values[603] != 0.0;
        self.lvoffGiven = values[604] != 0.0;
        self.lrdswGiven = values[605] != 0.0;
        self.lprwgGiven = values[606] != 0.0;
        self.lprwbGiven = values[607] != 0.0;
        self.lprtGiven = values[608] != 0.0;
        self.leta0Given = values[609] != 0.0;
        self.letabGiven = values[610] != 0.0;
        self.lpclmGiven = values[611] != 0.0;
        self.lpdibl1Given = values[612] != 0.0;
        self.lpdibl2Given = values[613] != 0.0;
        self.lpdiblbGiven = values[614] != 0.0;
        self.lpscbe1Given = values[615] != 0.0;
        self.lpscbe2Given = values[616] != 0.0;
        self.lpvagGiven = values[617] != 0.0;
        self.ldeltaGiven = values[618] != 0.0;
        self.lwrGiven = values[619] != 0.0;
        self.ldwgGiven = values[620] != 0.0;
        self.ldwbGiven = values[621] != 0.0;
        self.lb0Given = values[622] != 0.0;
        self.lb1Given = values[623] != 0.0;
        self.lalpha0Given = values[624] != 0.0;
        self.lalpha1Given = values[625] != 0.0;
        self.lbeta0Given = values[626] != 0.0;
        self.lvfbGiven = values[627] != 0.0;
        self.lelmGiven = values[628] != 0.0;
        self.lcgslGiven = values[629] != 0.0;
        self.lcgdlGiven = values[630] != 0.0;
        self.lckappaGiven = values[631] != 0.0;
        self.lcfGiven = values[632] != 0.0;
        self.lclcGiven = values[633] != 0.0;
        self.lcleGiven = values[634] != 0.0;
        self.lvfbcvGiven = values[635] != 0.0;
        self.lnoffGiven = values[636] != 0.0;
        self.lvoffcvGiven = values[637] != 0.0;
        self.lacdeGiven = values[638] != 0.0;
        self.lmoinGiven = values[639] != 0.0;
        self.wcdscGiven = values[640] != 0.0;
        self.wcdscbGiven = values[641] != 0.0;
        self.wcdscdGiven = values[642] != 0.0;
        self.wcitGiven = values[643] != 0.0;
        self.wnfactorGiven = values[644] != 0.0;
        self.wxjGiven = values[645] != 0.0;
        self.wvsatGiven = values[646] != 0.0;
        self.watGiven = values[647] != 0.0;
        self.wa0Given = values[648] != 0.0;
        self.wagsGiven = values[649] != 0.0;
        self.wa1Given = values[650] != 0.0;
        self.wa2Given = values[651] != 0.0;
        self.wketaGiven = values[652] != 0.0;
        self.wnsubGiven = values[653] != 0.0;
        self.wnpeakGiven = values[654] != 0.0;
        self.wngateGiven = values[655] != 0.0;
        self.wgamma1Given = values[656] != 0.0;
        self.wgamma2Given = values[657] != 0.0;
        self.wvbxGiven = values[658] != 0.0;
        self.wvbmGiven = values[659] != 0.0;
        self.wxtGiven = values[660] != 0.0;
        self.wk1Given = values[661] != 0.0;
        self.wkt1Given = values[662] != 0.0;
        self.wkt1lGiven = values[663] != 0.0;
        self.wkt2Given = values[664] != 0.0;
        self.wk2Given = values[665] != 0.0;
        self.wk3Given = values[666] != 0.0;
        self.wk3bGiven = values[667] != 0.0;
        self.ww0Given = values[668] != 0.0;
        self.wnlxGiven = values[669] != 0.0;
        self.wdvt0Given = values[670] != 0.0;
        self.wdvt1Given = values[671] != 0.0;
        self.wdvt2Given = values[672] != 0.0;
        self.wdvt0wGiven = values[673] != 0.0;
        self.wdvt1wGiven = values[674] != 0.0;
        self.wdvt2wGiven = values[675] != 0.0;
        self.wdroutGiven = values[676] != 0.0;
        self.wdsubGiven = values[677] != 0.0;
        self.wvth0Given = values[678] != 0.0;
        self.wuaGiven = values[679] != 0.0;
        self.wua1Given = values[680] != 0.0;
        self.wubGiven = values[681] != 0.0;
        self.wub1Given = values[682] != 0.0;
        self.wucGiven = values[683] != 0.0;
        self.wuc1Given = values[684] != 0.0;
        self.wu0Given = values[685] != 0.0;
        self.wuteGiven = values[686] != 0.0;
        self.wvoffGiven = values[687] != 0.0;
        self.wrdswGiven = values[688] != 0.0;
        self.wprwgGiven = values[689] != 0.0;
        self.wprwbGiven = values[690] != 0.0;
        self.wprtGiven = values[691] != 0.0;
        self.weta0Given = values[692] != 0.0;
        self.wetabGiven = values[693] != 0.0;
        self.wpclmGiven = values[694] != 0.0;
        self.wpdibl1Given = values[695] != 0.0;
        self.wpdibl2Given = values[696] != 0.0;
        self.wpdiblbGiven = values[697] != 0.0;
        self.wpscbe1Given = values[698] != 0.0;
        self.wpscbe2Given = values[699] != 0.0;
        self.wpvagGiven = values[700] != 0.0;
        self.wdeltaGiven = values[701] != 0.0;
        self.wwrGiven = values[702] != 0.0;
        self.wdwgGiven = values[703] != 0.0;
        self.wdwbGiven = values[704] != 0.0;
        self.wb0Given = values[705] != 0.0;
        self.wb1Given = values[706] != 0.0;
        self.walpha0Given = values[707] != 0.0;
        self.walpha1Given = values[708] != 0.0;
        self.wbeta0Given = values[709] != 0.0;
        self.wvfbGiven = values[710] != 0.0;
        self.welmGiven = values[711] != 0.0;
        self.wcgslGiven = values[712] != 0.0;
        self.wcgdlGiven = values[713] != 0.0;
        self.wckappaGiven = values[714] != 0.0;
        self.wcfGiven = values[715] != 0.0;
        self.wclcGiven = values[716] != 0.0;
        self.wcleGiven = values[717] != 0.0;
        self.wvfbcvGiven = values[718] != 0.0;
        self.wnoffGiven = values[719] != 0.0;
        self.wvoffcvGiven = values[720] != 0.0;
        self.wacdeGiven = values[721] != 0.0;
        self.wmoinGiven = values[722] != 0.0;
        self.pcdscGiven = values[723] != 0.0;
        self.pcdscbGiven = values[724] != 0.0;
        self.pcdscdGiven = values[725] != 0.0;
        self.pcitGiven = values[726] != 0.0;
        self.pnfactorGiven = values[727] != 0.0;
        self.pxjGiven = values[728] != 0.0;
        self.pvsatGiven = values[729] != 0.0;
        self.patGiven = values[730] != 0.0;
        self.pa0Given = values[731] != 0.0;
        self.pagsGiven = values[732] != 0.0;
        self.pa1Given = values[733] != 0.0;
        self.pa2Given = values[734] != 0.0;
        self.pketaGiven = values[735] != 0.0;
        self.pnsubGiven = values[736] != 0.0;
        self.pnpeakGiven = values[737] != 0.0;
        self.pngateGiven = values[738] != 0.0;
        self.pgamma1Given = values[739] != 0.0;
        self.pgamma2Given = values[740] != 0.0;
        self.pvbxGiven = values[741] != 0.0;
        self.pvbmGiven = values[742] != 0.0;
        self.pxtGiven = values[743] != 0.0;
        self.pk1Given = values[744] != 0.0;
        self.pkt1Given = values[745] != 0.0;
        self.pkt1lGiven = values[746] != 0.0;
        self.pkt2Given = values[747] != 0.0;
        self.pk2Given = values[748] != 0.0;
        self.pk3Given = values[749] != 0.0;
        self.pk3bGiven = values[750] != 0.0;
        self.pw0Given = values[751] != 0.0;
        self.pnlxGiven = values[752] != 0.0;
        self.pdvt0Given = values[753] != 0.0;
        self.pdvt1Given = values[754] != 0.0;
        self.pdvt2Given = values[755] != 0.0;
        self.pdvt0wGiven = values[756] != 0.0;
        self.pdvt1wGiven = values[757] != 0.0;
        self.pdvt2wGiven = values[758] != 0.0;
        self.pdroutGiven = values[759] != 0.0;
        self.pdsubGiven = values[760] != 0.0;
        self.pvth0Given = values[761] != 0.0;
        self.puaGiven = values[762] != 0.0;
        self.pua1Given = values[763] != 0.0;
        self.pubGiven = values[764] != 0.0;
        self.pub1Given = values[765] != 0.0;
        self.pucGiven = values[766] != 0.0;
        self.puc1Given = values[767] != 0.0;
        self.pu0Given = values[768] != 0.0;
        self.puteGiven = values[769] != 0.0;
        self.pvoffGiven = values[770] != 0.0;
        self.prdswGiven = values[771] != 0.0;
        self.pprwgGiven = values[772] != 0.0;
        self.pprwbGiven = values[773] != 0.0;
        self.pprtGiven = values[774] != 0.0;
        self.peta0Given = values[775] != 0.0;
        self.petabGiven = values[776] != 0.0;
        self.ppclmGiven = values[777] != 0.0;
        self.ppdibl1Given = values[778] != 0.0;
        self.ppdibl2Given = values[779] != 0.0;
        self.ppdiblbGiven = values[780] != 0.0;
        self.ppscbe1Given = values[781] != 0.0;
        self.ppscbe2Given = values[782] != 0.0;
        self.ppvagGiven = values[783] != 0.0;
        self.pdeltaGiven = values[784] != 0.0;
        self.pwrGiven = values[785] != 0.0;
        self.pdwgGiven = values[786] != 0.0;
        self.pdwbGiven = values[787] != 0.0;
        self.pb0Given = values[788] != 0.0;
        self.pb1Given = values[789] != 0.0;
        self.palpha0Given = values[790] != 0.0;
        self.palpha1Given = values[791] != 0.0;
        self.pbeta0Given = values[792] != 0.0;
        self.pvfbGiven = values[793] != 0.0;
        self.pelmGiven = values[794] != 0.0;
        self.pcgslGiven = values[795] != 0.0;
        self.pcgdlGiven = values[796] != 0.0;
        self.pckappaGiven = values[797] != 0.0;
        self.pcfGiven = values[798] != 0.0;
        self.pclcGiven = values[799] != 0.0;
        self.pcleGiven = values[800] != 0.0;
        self.pvfbcvGiven = values[801] != 0.0;
        self.pnoffGiven = values[802] != 0.0;
        self.pvoffcvGiven = values[803] != 0.0;
        self.pacdeGiven = values[804] != 0.0;
        self.pmoinGiven = values[805] != 0.0;
        self.useFringeGiven = values[806] != 0.0;
        self.tnomGiven = values[807] != 0.0;
        self.cgsoGiven = values[808] != 0.0;
        self.cgdoGiven = values[809] != 0.0;
        self.cgboGiven = values[810] != 0.0;
        self.xpartGiven = values[811] != 0.0;
        self.sheetResistanceGiven = values[812] != 0.0;
        self.jctSatCurDensityGiven = values[813] != 0.0;
        self.jctSidewallSatCurDensityGiven = values[814] != 0.0;
        self.bulkJctPotentialGiven = values[815] != 0.0;
        self.bulkJctBotGradingCoeffGiven = values[816] != 0.0;
        self.sidewallJctPotentialGiven = values[817] != 0.0;
        self.GatesidewallJctPotentialGiven = values[818] != 0.0;
        self.bulkJctSideGradingCoeffGiven = values[819] != 0.0;
        self.unitAreaJctCapGiven = values[820] != 0.0;
        self.unitLengthSidewallJctCapGiven = values[821] != 0.0;
        self.bulkJctGateSideGradingCoeffGiven = values[822] != 0.0;
        self.unitLengthGateSidewallJctCapGiven = values[823] != 0.0;
        self.jctEmissionCoeffGiven = values[824] != 0.0;
        self.jctTempExponentGiven = values[825] != 0.0;
        self.oxideTrapDensityAGiven = values[826] != 0.0;
        self.oxideTrapDensityBGiven = values[827] != 0.0;
        self.oxideTrapDensityCGiven = values[828] != 0.0;
        self.emGiven = values[829] != 0.0;
        self.efGiven = values[830] != 0.0;
        self.afGiven = values[831] != 0.0;
        self.kfGiven = values[832] != 0.0;
        self.lintnoiGiven = values[833] != 0.0;
        self.vgsMaxGiven = values[834] != 0.0;
        self.vgdMaxGiven = values[835] != 0.0;
        self.vgbMaxGiven = values[836] != 0.0;
        self.vdsMaxGiven = values[837] != 0.0;
        self.vbsMaxGiven = values[838] != 0.0;
        self.vbdMaxGiven = values[839] != 0.0;
        self.vgsrMaxGiven = values[840] != 0.0;
        self.vgdrMaxGiven = values[841] != 0.0;
        self.vgbrMaxGiven = values[842] != 0.0;
        self.vbsrMaxGiven = values[843] != 0.0;
        self.vbdrMaxGiven = values[844] != 0.0;
        self.LintGiven = values[845] != 0.0;
        self.LlGiven = values[846] != 0.0;
        self.LlcGiven = values[847] != 0.0;
        self.LlnGiven = values[848] != 0.0;
        self.LwGiven = values[849] != 0.0;
        self.LwcGiven = values[850] != 0.0;
        self.LwnGiven = values[851] != 0.0;
        self.LwlGiven = values[852] != 0.0;
        self.LwlcGiven = values[853] != 0.0;
        self.LminGiven = values[854] != 0.0;
        self.LmaxGiven = values[855] != 0.0;
        self.WintGiven = values[856] != 0.0;
        self.WlGiven = values[857] != 0.0;
        self.WlcGiven = values[858] != 0.0;
        self.WlnGiven = values[859] != 0.0;
        self.WwGiven = values[860] != 0.0;
        self.WwcGiven = values[861] != 0.0;
        self.WwnGiven = values[862] != 0.0;
        self.WwlGiven = values[863] != 0.0;
        self.WwlcGiven = values[864] != 0.0;
        self.WminGiven = values[865] != 0.0;
        self.WmaxGiven = values[866] != 0.0;
    }
}

/// `struct bsim3SizeDependParam`'s scalar members (120 of them), in declaration order.
#[derive(Clone, Default)]
pub(crate) struct SizeParam {
    pub(crate) Width: f64,
    pub(crate) Length: f64,
    pub(crate) cdsc: f64,
    pub(crate) cdscb: f64,
    pub(crate) cdscd: f64,
    pub(crate) cit: f64,
    pub(crate) nfactor: f64,
    pub(crate) xj: f64,
    pub(crate) vsat: f64,
    pub(crate) at: f64,
    pub(crate) a0: f64,
    pub(crate) ags: f64,
    pub(crate) a1: f64,
    pub(crate) a2: f64,
    pub(crate) keta: f64,
    pub(crate) nsub: f64,
    pub(crate) npeak: f64,
    pub(crate) ngate: f64,
    pub(crate) gamma1: f64,
    pub(crate) gamma2: f64,
    pub(crate) vbx: f64,
    pub(crate) vbi: f64,
    pub(crate) vbm: f64,
    pub(crate) vbsc: f64,
    pub(crate) xt: f64,
    pub(crate) phi: f64,
    pub(crate) litl: f64,
    pub(crate) k1: f64,
    pub(crate) kt1: f64,
    pub(crate) kt1l: f64,
    pub(crate) kt2: f64,
    pub(crate) k2: f64,
    pub(crate) k3: f64,
    pub(crate) k3b: f64,
    pub(crate) w0: f64,
    pub(crate) nlx: f64,
    pub(crate) dvt0: f64,
    pub(crate) dvt1: f64,
    pub(crate) dvt2: f64,
    pub(crate) dvt0w: f64,
    pub(crate) dvt1w: f64,
    pub(crate) dvt2w: f64,
    pub(crate) drout: f64,
    pub(crate) dsub: f64,
    pub(crate) vth0: f64,
    pub(crate) ua: f64,
    pub(crate) ua1: f64,
    pub(crate) ub: f64,
    pub(crate) ub1: f64,
    pub(crate) uc: f64,
    pub(crate) uc1: f64,
    pub(crate) u0: f64,
    pub(crate) ute: f64,
    pub(crate) voff: f64,
    pub(crate) vfb: f64,
    pub(crate) delta: f64,
    pub(crate) rdsw: f64,
    pub(crate) rds0: f64,
    pub(crate) prwg: f64,
    pub(crate) prwb: f64,
    pub(crate) prt: f64,
    pub(crate) eta0: f64,
    pub(crate) etab: f64,
    pub(crate) pclm: f64,
    pub(crate) pdibl1: f64,
    pub(crate) pdibl2: f64,
    pub(crate) pdiblb: f64,
    pub(crate) pscbe1: f64,
    pub(crate) pscbe2: f64,
    pub(crate) pvag: f64,
    pub(crate) wr: f64,
    pub(crate) dwg: f64,
    pub(crate) dwb: f64,
    pub(crate) b0: f64,
    pub(crate) b1: f64,
    pub(crate) alpha0: f64,
    pub(crate) alpha1: f64,
    pub(crate) beta0: f64,
    pub(crate) elm: f64,
    pub(crate) cgsl: f64,
    pub(crate) cgdl: f64,
    pub(crate) ckappa: f64,
    pub(crate) cf: f64,
    pub(crate) clc: f64,
    pub(crate) cle: f64,
    pub(crate) vfbcv: f64,
    pub(crate) noff: f64,
    pub(crate) voffcv: f64,
    pub(crate) acde: f64,
    pub(crate) moin: f64,
    pub(crate) dw: f64,
    pub(crate) dl: f64,
    pub(crate) leff: f64,
    pub(crate) weff: f64,
    pub(crate) dwc: f64,
    pub(crate) dlc: f64,
    pub(crate) leffCV: f64,
    pub(crate) weffCV: f64,
    pub(crate) abulkCVfactor: f64,
    pub(crate) cgso: f64,
    pub(crate) cgdo: f64,
    pub(crate) cgbo: f64,
    pub(crate) tconst: f64,
    pub(crate) u0temp: f64,
    pub(crate) vsattemp: f64,
    pub(crate) sqrtPhi: f64,
    pub(crate) phis3: f64,
    pub(crate) Xdep0: f64,
    pub(crate) sqrtXdep0: f64,
    pub(crate) theta0vb0: f64,
    pub(crate) thetaRout: f64,
    pub(crate) cof1: f64,
    pub(crate) cof2: f64,
    pub(crate) cof3: f64,
    pub(crate) cof4: f64,
    pub(crate) cdep0: f64,
    pub(crate) vfbzb: f64,
    pub(crate) ldeb: f64,
    pub(crate) k1ox: f64,
    pub(crate) k2ox: f64,
}

#[rustfmt::skip]
impl SizeParam {
    /// The `f64` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn real_mut(&mut self, c_name: &str) -> Option<&mut f64> {
        Some(match c_name {
            "Width" => &mut self.Width,
            "Length" => &mut self.Length,
            "BSIM3cdsc" => &mut self.cdsc,
            "BSIM3cdscb" => &mut self.cdscb,
            "BSIM3cdscd" => &mut self.cdscd,
            "BSIM3cit" => &mut self.cit,
            "BSIM3nfactor" => &mut self.nfactor,
            "BSIM3xj" => &mut self.xj,
            "BSIM3vsat" => &mut self.vsat,
            "BSIM3at" => &mut self.at,
            "BSIM3a0" => &mut self.a0,
            "BSIM3ags" => &mut self.ags,
            "BSIM3a1" => &mut self.a1,
            "BSIM3a2" => &mut self.a2,
            "BSIM3keta" => &mut self.keta,
            "BSIM3nsub" => &mut self.nsub,
            "BSIM3npeak" => &mut self.npeak,
            "BSIM3ngate" => &mut self.ngate,
            "BSIM3gamma1" => &mut self.gamma1,
            "BSIM3gamma2" => &mut self.gamma2,
            "BSIM3vbx" => &mut self.vbx,
            "BSIM3vbi" => &mut self.vbi,
            "BSIM3vbm" => &mut self.vbm,
            "BSIM3vbsc" => &mut self.vbsc,
            "BSIM3xt" => &mut self.xt,
            "BSIM3phi" => &mut self.phi,
            "BSIM3litl" => &mut self.litl,
            "BSIM3k1" => &mut self.k1,
            "BSIM3kt1" => &mut self.kt1,
            "BSIM3kt1l" => &mut self.kt1l,
            "BSIM3kt2" => &mut self.kt2,
            "BSIM3k2" => &mut self.k2,
            "BSIM3k3" => &mut self.k3,
            "BSIM3k3b" => &mut self.k3b,
            "BSIM3w0" => &mut self.w0,
            "BSIM3nlx" => &mut self.nlx,
            "BSIM3dvt0" => &mut self.dvt0,
            "BSIM3dvt1" => &mut self.dvt1,
            "BSIM3dvt2" => &mut self.dvt2,
            "BSIM3dvt0w" => &mut self.dvt0w,
            "BSIM3dvt1w" => &mut self.dvt1w,
            "BSIM3dvt2w" => &mut self.dvt2w,
            "BSIM3drout" => &mut self.drout,
            "BSIM3dsub" => &mut self.dsub,
            "BSIM3vth0" => &mut self.vth0,
            "BSIM3ua" => &mut self.ua,
            "BSIM3ua1" => &mut self.ua1,
            "BSIM3ub" => &mut self.ub,
            "BSIM3ub1" => &mut self.ub1,
            "BSIM3uc" => &mut self.uc,
            "BSIM3uc1" => &mut self.uc1,
            "BSIM3u0" => &mut self.u0,
            "BSIM3ute" => &mut self.ute,
            "BSIM3voff" => &mut self.voff,
            "BSIM3vfb" => &mut self.vfb,
            "BSIM3delta" => &mut self.delta,
            "BSIM3rdsw" => &mut self.rdsw,
            "BSIM3rds0" => &mut self.rds0,
            "BSIM3prwg" => &mut self.prwg,
            "BSIM3prwb" => &mut self.prwb,
            "BSIM3prt" => &mut self.prt,
            "BSIM3eta0" => &mut self.eta0,
            "BSIM3etab" => &mut self.etab,
            "BSIM3pclm" => &mut self.pclm,
            "BSIM3pdibl1" => &mut self.pdibl1,
            "BSIM3pdibl2" => &mut self.pdibl2,
            "BSIM3pdiblb" => &mut self.pdiblb,
            "BSIM3pscbe1" => &mut self.pscbe1,
            "BSIM3pscbe2" => &mut self.pscbe2,
            "BSIM3pvag" => &mut self.pvag,
            "BSIM3wr" => &mut self.wr,
            "BSIM3dwg" => &mut self.dwg,
            "BSIM3dwb" => &mut self.dwb,
            "BSIM3b0" => &mut self.b0,
            "BSIM3b1" => &mut self.b1,
            "BSIM3alpha0" => &mut self.alpha0,
            "BSIM3alpha1" => &mut self.alpha1,
            "BSIM3beta0" => &mut self.beta0,
            "BSIM3elm" => &mut self.elm,
            "BSIM3cgsl" => &mut self.cgsl,
            "BSIM3cgdl" => &mut self.cgdl,
            "BSIM3ckappa" => &mut self.ckappa,
            "BSIM3cf" => &mut self.cf,
            "BSIM3clc" => &mut self.clc,
            "BSIM3cle" => &mut self.cle,
            "BSIM3vfbcv" => &mut self.vfbcv,
            "BSIM3noff" => &mut self.noff,
            "BSIM3voffcv" => &mut self.voffcv,
            "BSIM3acde" => &mut self.acde,
            "BSIM3moin" => &mut self.moin,
            "BSIM3dw" => &mut self.dw,
            "BSIM3dl" => &mut self.dl,
            "BSIM3leff" => &mut self.leff,
            "BSIM3weff" => &mut self.weff,
            "BSIM3dwc" => &mut self.dwc,
            "BSIM3dlc" => &mut self.dlc,
            "BSIM3leffCV" => &mut self.leffCV,
            "BSIM3weffCV" => &mut self.weffCV,
            "BSIM3abulkCVfactor" => &mut self.abulkCVfactor,
            "BSIM3cgso" => &mut self.cgso,
            "BSIM3cgdo" => &mut self.cgdo,
            "BSIM3cgbo" => &mut self.cgbo,
            "BSIM3tconst" => &mut self.tconst,
            "BSIM3u0temp" => &mut self.u0temp,
            "BSIM3vsattemp" => &mut self.vsattemp,
            "BSIM3sqrtPhi" => &mut self.sqrtPhi,
            "BSIM3phis3" => &mut self.phis3,
            "BSIM3Xdep0" => &mut self.Xdep0,
            "BSIM3sqrtXdep0" => &mut self.sqrtXdep0,
            "BSIM3theta0vb0" => &mut self.theta0vb0,
            "BSIM3thetaRout" => &mut self.thetaRout,
            "BSIM3cof1" => &mut self.cof1,
            "BSIM3cof2" => &mut self.cof2,
            "BSIM3cof3" => &mut self.cof3,
            "BSIM3cof4" => &mut self.cof4,
            "BSIM3cdep0" => &mut self.cdep0,
            "BSIM3vfbzb" => &mut self.vfbzb,
            "BSIM3ldeb" => &mut self.ldeb,
            "BSIM3k1ox" => &mut self.k1ox,
            "BSIM3k2ox" => &mut self.k2ox,
            _ => return None,
        })
    }
    /// C member names of the scalar members, in declaration order.
    #[allow(dead_code)]
    pub(crate) const MEMBERS: &[&str] = &[
        "Width",
        "Length",
        "BSIM3cdsc",
        "BSIM3cdscb",
        "BSIM3cdscd",
        "BSIM3cit",
        "BSIM3nfactor",
        "BSIM3xj",
        "BSIM3vsat",
        "BSIM3at",
        "BSIM3a0",
        "BSIM3ags",
        "BSIM3a1",
        "BSIM3a2",
        "BSIM3keta",
        "BSIM3nsub",
        "BSIM3npeak",
        "BSIM3ngate",
        "BSIM3gamma1",
        "BSIM3gamma2",
        "BSIM3vbx",
        "BSIM3vbi",
        "BSIM3vbm",
        "BSIM3vbsc",
        "BSIM3xt",
        "BSIM3phi",
        "BSIM3litl",
        "BSIM3k1",
        "BSIM3kt1",
        "BSIM3kt1l",
        "BSIM3kt2",
        "BSIM3k2",
        "BSIM3k3",
        "BSIM3k3b",
        "BSIM3w0",
        "BSIM3nlx",
        "BSIM3dvt0",
        "BSIM3dvt1",
        "BSIM3dvt2",
        "BSIM3dvt0w",
        "BSIM3dvt1w",
        "BSIM3dvt2w",
        "BSIM3drout",
        "BSIM3dsub",
        "BSIM3vth0",
        "BSIM3ua",
        "BSIM3ua1",
        "BSIM3ub",
        "BSIM3ub1",
        "BSIM3uc",
        "BSIM3uc1",
        "BSIM3u0",
        "BSIM3ute",
        "BSIM3voff",
        "BSIM3vfb",
        "BSIM3delta",
        "BSIM3rdsw",
        "BSIM3rds0",
        "BSIM3prwg",
        "BSIM3prwb",
        "BSIM3prt",
        "BSIM3eta0",
        "BSIM3etab",
        "BSIM3pclm",
        "BSIM3pdibl1",
        "BSIM3pdibl2",
        "BSIM3pdiblb",
        "BSIM3pscbe1",
        "BSIM3pscbe2",
        "BSIM3pvag",
        "BSIM3wr",
        "BSIM3dwg",
        "BSIM3dwb",
        "BSIM3b0",
        "BSIM3b1",
        "BSIM3alpha0",
        "BSIM3alpha1",
        "BSIM3beta0",
        "BSIM3elm",
        "BSIM3cgsl",
        "BSIM3cgdl",
        "BSIM3ckappa",
        "BSIM3cf",
        "BSIM3clc",
        "BSIM3cle",
        "BSIM3vfbcv",
        "BSIM3noff",
        "BSIM3voffcv",
        "BSIM3acde",
        "BSIM3moin",
        "BSIM3dw",
        "BSIM3dl",
        "BSIM3leff",
        "BSIM3weff",
        "BSIM3dwc",
        "BSIM3dlc",
        "BSIM3leffCV",
        "BSIM3weffCV",
        "BSIM3abulkCVfactor",
        "BSIM3cgso",
        "BSIM3cgdo",
        "BSIM3cgbo",
        "BSIM3tconst",
        "BSIM3u0temp",
        "BSIM3vsattemp",
        "BSIM3sqrtPhi",
        "BSIM3phis3",
        "BSIM3Xdep0",
        "BSIM3sqrtXdep0",
        "BSIM3theta0vb0",
        "BSIM3thetaRout",
        "BSIM3cof1",
        "BSIM3cof2",
        "BSIM3cof3",
        "BSIM3cof4",
        "BSIM3cdep0",
        "BSIM3vfbzb",
        "BSIM3ldeb",
        "BSIM3k1ox",
        "BSIM3k2ox",
    ];

    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,
    /// a given bit as 0 or 1).
    #[allow(dead_code)]
    pub(crate) fn to_values(&self) -> Vec<f64> {
        vec![
            self.Width,
            self.Length,
            self.cdsc,
            self.cdscb,
            self.cdscd,
            self.cit,
            self.nfactor,
            self.xj,
            self.vsat,
            self.at,
            self.a0,
            self.ags,
            self.a1,
            self.a2,
            self.keta,
            self.nsub,
            self.npeak,
            self.ngate,
            self.gamma1,
            self.gamma2,
            self.vbx,
            self.vbi,
            self.vbm,
            self.vbsc,
            self.xt,
            self.phi,
            self.litl,
            self.k1,
            self.kt1,
            self.kt1l,
            self.kt2,
            self.k2,
            self.k3,
            self.k3b,
            self.w0,
            self.nlx,
            self.dvt0,
            self.dvt1,
            self.dvt2,
            self.dvt0w,
            self.dvt1w,
            self.dvt2w,
            self.drout,
            self.dsub,
            self.vth0,
            self.ua,
            self.ua1,
            self.ub,
            self.ub1,
            self.uc,
            self.uc1,
            self.u0,
            self.ute,
            self.voff,
            self.vfb,
            self.delta,
            self.rdsw,
            self.rds0,
            self.prwg,
            self.prwb,
            self.prt,
            self.eta0,
            self.etab,
            self.pclm,
            self.pdibl1,
            self.pdibl2,
            self.pdiblb,
            self.pscbe1,
            self.pscbe2,
            self.pvag,
            self.wr,
            self.dwg,
            self.dwb,
            self.b0,
            self.b1,
            self.alpha0,
            self.alpha1,
            self.beta0,
            self.elm,
            self.cgsl,
            self.cgdl,
            self.ckappa,
            self.cf,
            self.clc,
            self.cle,
            self.vfbcv,
            self.noff,
            self.voffcv,
            self.acde,
            self.moin,
            self.dw,
            self.dl,
            self.leff,
            self.weff,
            self.dwc,
            self.dlc,
            self.leffCV,
            self.weffCV,
            self.abulkCVfactor,
            self.cgso,
            self.cgdo,
            self.cgbo,
            self.tconst,
            self.u0temp,
            self.vsattemp,
            self.sqrtPhi,
            self.phis3,
            self.Xdep0,
            self.sqrtXdep0,
            self.theta0vb0,
            self.thetaRout,
            self.cof1,
            self.cof2,
            self.cof3,
            self.cof4,
            self.cdep0,
            self.vfbzb,
            self.ldeb,
            self.k1ox,
            self.k2ox,
        ]
    }

    /// Stores numbers produced by [`Self::to_values`]; `values` must have
    /// [`Self::MEMBERS`]`.len()` entries.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub(crate) fn set_values(&mut self, values: &[f64]) {
        self.Width = values[0];
        self.Length = values[1];
        self.cdsc = values[2];
        self.cdscb = values[3];
        self.cdscd = values[4];
        self.cit = values[5];
        self.nfactor = values[6];
        self.xj = values[7];
        self.vsat = values[8];
        self.at = values[9];
        self.a0 = values[10];
        self.ags = values[11];
        self.a1 = values[12];
        self.a2 = values[13];
        self.keta = values[14];
        self.nsub = values[15];
        self.npeak = values[16];
        self.ngate = values[17];
        self.gamma1 = values[18];
        self.gamma2 = values[19];
        self.vbx = values[20];
        self.vbi = values[21];
        self.vbm = values[22];
        self.vbsc = values[23];
        self.xt = values[24];
        self.phi = values[25];
        self.litl = values[26];
        self.k1 = values[27];
        self.kt1 = values[28];
        self.kt1l = values[29];
        self.kt2 = values[30];
        self.k2 = values[31];
        self.k3 = values[32];
        self.k3b = values[33];
        self.w0 = values[34];
        self.nlx = values[35];
        self.dvt0 = values[36];
        self.dvt1 = values[37];
        self.dvt2 = values[38];
        self.dvt0w = values[39];
        self.dvt1w = values[40];
        self.dvt2w = values[41];
        self.drout = values[42];
        self.dsub = values[43];
        self.vth0 = values[44];
        self.ua = values[45];
        self.ua1 = values[46];
        self.ub = values[47];
        self.ub1 = values[48];
        self.uc = values[49];
        self.uc1 = values[50];
        self.u0 = values[51];
        self.ute = values[52];
        self.voff = values[53];
        self.vfb = values[54];
        self.delta = values[55];
        self.rdsw = values[56];
        self.rds0 = values[57];
        self.prwg = values[58];
        self.prwb = values[59];
        self.prt = values[60];
        self.eta0 = values[61];
        self.etab = values[62];
        self.pclm = values[63];
        self.pdibl1 = values[64];
        self.pdibl2 = values[65];
        self.pdiblb = values[66];
        self.pscbe1 = values[67];
        self.pscbe2 = values[68];
        self.pvag = values[69];
        self.wr = values[70];
        self.dwg = values[71];
        self.dwb = values[72];
        self.b0 = values[73];
        self.b1 = values[74];
        self.alpha0 = values[75];
        self.alpha1 = values[76];
        self.beta0 = values[77];
        self.elm = values[78];
        self.cgsl = values[79];
        self.cgdl = values[80];
        self.ckappa = values[81];
        self.cf = values[82];
        self.clc = values[83];
        self.cle = values[84];
        self.vfbcv = values[85];
        self.noff = values[86];
        self.voffcv = values[87];
        self.acde = values[88];
        self.moin = values[89];
        self.dw = values[90];
        self.dl = values[91];
        self.leff = values[92];
        self.weff = values[93];
        self.dwc = values[94];
        self.dlc = values[95];
        self.leffCV = values[96];
        self.weffCV = values[97];
        self.abulkCVfactor = values[98];
        self.cgso = values[99];
        self.cgdo = values[100];
        self.cgbo = values[101];
        self.tconst = values[102];
        self.u0temp = values[103];
        self.vsattemp = values[104];
        self.sqrtPhi = values[105];
        self.phis3 = values[106];
        self.Xdep0 = values[107];
        self.sqrtXdep0 = values[108];
        self.theta0vb0 = values[109];
        self.thetaRout = values[110];
        self.cof1 = values[111];
        self.cof2 = values[112];
        self.cof3 = values[113];
        self.cof4 = values[114];
        self.cdep0 = values[115];
        self.vfbzb = values[116];
        self.ldeb = values[117];
        self.k1ox = values[118];
        self.k2ox = values[119];
    }
}

/// `struct sBSIM3instance`'s scalar members (140 of them), in declaration order.
#[derive(Clone, Default)]
pub(crate) struct Instance {
    pub(crate) dNode: i32,
    pub(crate) gNode: i32,
    pub(crate) sNode: i32,
    pub(crate) bNode: i32,
    pub(crate) dNodePrime: i32,
    pub(crate) sNodePrime: i32,
    pub(crate) qNode: i32,
    pub(crate) ueff: f64,
    pub(crate) thetavth: f64,
    pub(crate) von: f64,
    pub(crate) vdsat: f64,
    pub(crate) cgdo: f64,
    pub(crate) cgso: f64,
    pub(crate) vjsm: f64,
    pub(crate) IsEvjsm: f64,
    pub(crate) vjdm: f64,
    pub(crate) IsEvjdm: f64,
    pub(crate) l: f64,
    pub(crate) w: f64,
    pub(crate) m: f64,
    pub(crate) drainArea: f64,
    pub(crate) sourceArea: f64,
    pub(crate) drainSquares: f64,
    pub(crate) sourceSquares: f64,
    pub(crate) drainPerimeter: f64,
    pub(crate) sourcePerimeter: f64,
    pub(crate) sourceConductance: f64,
    pub(crate) drainConductance: f64,
    pub(crate) delvto: f64,
    pub(crate) mulu0: f64,
    pub(crate) vth0: f64,
    pub(crate) vfb: f64,
    pub(crate) vfbzb: f64,
    pub(crate) u0temp: f64,
    pub(crate) tconst: f64,
    pub(crate) icVBS: f64,
    pub(crate) icVDS: f64,
    pub(crate) icVGS: f64,
    pub(crate) off: i32,
    pub(crate) mode: i32,
    pub(crate) nqsMod: i32,
    pub(crate) acnqsMod: i32,
    pub(crate) geo: i32,
    pub(crate) qinv: f64,
    pub(crate) cd: f64,
    pub(crate) cbs: f64,
    pub(crate) cbd: f64,
    pub(crate) csub: f64,
    pub(crate) gm: f64,
    pub(crate) gds: f64,
    pub(crate) gmbs: f64,
    pub(crate) gbd: f64,
    pub(crate) gbs: f64,
    pub(crate) gbbs: f64,
    pub(crate) gbgs: f64,
    pub(crate) gbds: f64,
    pub(crate) cggb: f64,
    pub(crate) cgdb: f64,
    pub(crate) cgsb: f64,
    pub(crate) cbgb: f64,
    pub(crate) cbdb: f64,
    pub(crate) cbsb: f64,
    pub(crate) cdgb: f64,
    pub(crate) cddb: f64,
    pub(crate) cdsb: f64,
    pub(crate) capbd: f64,
    pub(crate) capbs: f64,
    pub(crate) cqgb: f64,
    pub(crate) cqdb: f64,
    pub(crate) cqsb: f64,
    pub(crate) cqbb: f64,
    pub(crate) qgate: f64,
    pub(crate) qbulk: f64,
    pub(crate) qdrn: f64,
    pub(crate) gtau: f64,
    pub(crate) gtg: f64,
    pub(crate) gtd: f64,
    pub(crate) gts: f64,
    pub(crate) gtb: f64,
    pub(crate) rds: f64,
    pub(crate) Vgsteff: f64,
    pub(crate) Vdseff: f64,
    pub(crate) Abulk: f64,
    pub(crate) AbovVgst2Vtm: f64,
    pub(crate) taunet: f64,
    pub(crate) lGiven: bool,
    pub(crate) wGiven: bool,
    pub(crate) mGiven: bool,
    pub(crate) drainAreaGiven: bool,
    pub(crate) sourceAreaGiven: bool,
    pub(crate) drainSquaresGiven: bool,
    pub(crate) sourceSquaresGiven: bool,
    pub(crate) drainPerimeterGiven: bool,
    pub(crate) sourcePerimeterGiven: bool,
    pub(crate) delvtoGiven: bool,
    pub(crate) mulu0Given: bool,
    pub(crate) dNodePrimeSet: bool,
    pub(crate) sNodePrimeSet: bool,
    pub(crate) icVBSGiven: bool,
    pub(crate) icVDSGiven: bool,
    pub(crate) icVGSGiven: bool,
    pub(crate) nqsModGiven: bool,
    pub(crate) acnqsModGiven: bool,
    pub(crate) geoGiven: bool,
    pub(crate) rhsG: f64,
    pub(crate) rhsB: f64,
    pub(crate) rhsD: f64,
    pub(crate) rhsS: f64,
    pub(crate) rhsQ: f64,
    pub(crate) DdPt: f64,
    pub(crate) GgPt: f64,
    pub(crate) SsPt: f64,
    pub(crate) BbPt: f64,
    pub(crate) DPdpPt: f64,
    pub(crate) SPspPt: f64,
    pub(crate) DdpPt: f64,
    pub(crate) GbPt: f64,
    pub(crate) GdpPt: f64,
    pub(crate) GspPt: f64,
    pub(crate) SspPt: f64,
    pub(crate) BdpPt: f64,
    pub(crate) BspPt: f64,
    pub(crate) DPspPt: f64,
    pub(crate) DPdPt: f64,
    pub(crate) BgPt: f64,
    pub(crate) DPgPt: f64,
    pub(crate) SPgPt: f64,
    pub(crate) SPsPt: f64,
    pub(crate) DPbPt: f64,
    pub(crate) SPbPt: f64,
    pub(crate) SPdpPt: f64,
    pub(crate) QqPt: f64,
    pub(crate) QdpPt: f64,
    pub(crate) QgPt: f64,
    pub(crate) QspPt: f64,
    pub(crate) QbPt: f64,
    pub(crate) DPqPt: f64,
    pub(crate) GqPt: f64,
    pub(crate) SPqPt: f64,
    pub(crate) BqPt: f64,
}

#[rustfmt::skip]
impl Instance {
    /// The `f64` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn real_mut(&mut self, c_name: &str) -> Option<&mut f64> {
        Some(match c_name {
            "BSIM3ueff" => &mut self.ueff,
            "BSIM3thetavth" => &mut self.thetavth,
            "BSIM3von" => &mut self.von,
            "BSIM3vdsat" => &mut self.vdsat,
            "BSIM3cgdo" => &mut self.cgdo,
            "BSIM3cgso" => &mut self.cgso,
            "BSIM3vjsm" => &mut self.vjsm,
            "BSIM3IsEvjsm" => &mut self.IsEvjsm,
            "BSIM3vjdm" => &mut self.vjdm,
            "BSIM3IsEvjdm" => &mut self.IsEvjdm,
            "BSIM3l" => &mut self.l,
            "BSIM3w" => &mut self.w,
            "BSIM3m" => &mut self.m,
            "BSIM3drainArea" => &mut self.drainArea,
            "BSIM3sourceArea" => &mut self.sourceArea,
            "BSIM3drainSquares" => &mut self.drainSquares,
            "BSIM3sourceSquares" => &mut self.sourceSquares,
            "BSIM3drainPerimeter" => &mut self.drainPerimeter,
            "BSIM3sourcePerimeter" => &mut self.sourcePerimeter,
            "BSIM3sourceConductance" => &mut self.sourceConductance,
            "BSIM3drainConductance" => &mut self.drainConductance,
            "BSIM3delvto" => &mut self.delvto,
            "BSIM3mulu0" => &mut self.mulu0,
            "BSIM3vth0" => &mut self.vth0,
            "BSIM3vfb" => &mut self.vfb,
            "BSIM3vfbzb" => &mut self.vfbzb,
            "BSIM3u0temp" => &mut self.u0temp,
            "BSIM3tconst" => &mut self.tconst,
            "BSIM3icVBS" => &mut self.icVBS,
            "BSIM3icVDS" => &mut self.icVDS,
            "BSIM3icVGS" => &mut self.icVGS,
            "BSIM3qinv" => &mut self.qinv,
            "BSIM3cd" => &mut self.cd,
            "BSIM3cbs" => &mut self.cbs,
            "BSIM3cbd" => &mut self.cbd,
            "BSIM3csub" => &mut self.csub,
            "BSIM3gm" => &mut self.gm,
            "BSIM3gds" => &mut self.gds,
            "BSIM3gmbs" => &mut self.gmbs,
            "BSIM3gbd" => &mut self.gbd,
            "BSIM3gbs" => &mut self.gbs,
            "BSIM3gbbs" => &mut self.gbbs,
            "BSIM3gbgs" => &mut self.gbgs,
            "BSIM3gbds" => &mut self.gbds,
            "BSIM3cggb" => &mut self.cggb,
            "BSIM3cgdb" => &mut self.cgdb,
            "BSIM3cgsb" => &mut self.cgsb,
            "BSIM3cbgb" => &mut self.cbgb,
            "BSIM3cbdb" => &mut self.cbdb,
            "BSIM3cbsb" => &mut self.cbsb,
            "BSIM3cdgb" => &mut self.cdgb,
            "BSIM3cddb" => &mut self.cddb,
            "BSIM3cdsb" => &mut self.cdsb,
            "BSIM3capbd" => &mut self.capbd,
            "BSIM3capbs" => &mut self.capbs,
            "BSIM3cqgb" => &mut self.cqgb,
            "BSIM3cqdb" => &mut self.cqdb,
            "BSIM3cqsb" => &mut self.cqsb,
            "BSIM3cqbb" => &mut self.cqbb,
            "BSIM3qgate" => &mut self.qgate,
            "BSIM3qbulk" => &mut self.qbulk,
            "BSIM3qdrn" => &mut self.qdrn,
            "BSIM3gtau" => &mut self.gtau,
            "BSIM3gtg" => &mut self.gtg,
            "BSIM3gtd" => &mut self.gtd,
            "BSIM3gts" => &mut self.gts,
            "BSIM3gtb" => &mut self.gtb,
            "BSIM3rds" => &mut self.rds,
            "BSIM3Vgsteff" => &mut self.Vgsteff,
            "BSIM3Vdseff" => &mut self.Vdseff,
            "BSIM3Abulk" => &mut self.Abulk,
            "BSIM3AbovVgst2Vtm" => &mut self.AbovVgst2Vtm,
            "BSIM3taunet" => &mut self.taunet,
            "BSIM3rhsG" => &mut self.rhsG,
            "BSIM3rhsB" => &mut self.rhsB,
            "BSIM3rhsD" => &mut self.rhsD,
            "BSIM3rhsS" => &mut self.rhsS,
            "BSIM3rhsQ" => &mut self.rhsQ,
            "BSIM3DdPt" => &mut self.DdPt,
            "BSIM3GgPt" => &mut self.GgPt,
            "BSIM3SsPt" => &mut self.SsPt,
            "BSIM3BbPt" => &mut self.BbPt,
            "BSIM3DPdpPt" => &mut self.DPdpPt,
            "BSIM3SPspPt" => &mut self.SPspPt,
            "BSIM3DdpPt" => &mut self.DdpPt,
            "BSIM3GbPt" => &mut self.GbPt,
            "BSIM3GdpPt" => &mut self.GdpPt,
            "BSIM3GspPt" => &mut self.GspPt,
            "BSIM3SspPt" => &mut self.SspPt,
            "BSIM3BdpPt" => &mut self.BdpPt,
            "BSIM3BspPt" => &mut self.BspPt,
            "BSIM3DPspPt" => &mut self.DPspPt,
            "BSIM3DPdPt" => &mut self.DPdPt,
            "BSIM3BgPt" => &mut self.BgPt,
            "BSIM3DPgPt" => &mut self.DPgPt,
            "BSIM3SPgPt" => &mut self.SPgPt,
            "BSIM3SPsPt" => &mut self.SPsPt,
            "BSIM3DPbPt" => &mut self.DPbPt,
            "BSIM3SPbPt" => &mut self.SPbPt,
            "BSIM3SPdpPt" => &mut self.SPdpPt,
            "BSIM3QqPt" => &mut self.QqPt,
            "BSIM3QdpPt" => &mut self.QdpPt,
            "BSIM3QgPt" => &mut self.QgPt,
            "BSIM3QspPt" => &mut self.QspPt,
            "BSIM3QbPt" => &mut self.QbPt,
            "BSIM3DPqPt" => &mut self.DPqPt,
            "BSIM3GqPt" => &mut self.GqPt,
            "BSIM3SPqPt" => &mut self.SPqPt,
            "BSIM3BqPt" => &mut self.BqPt,
            _ => return None,
        })
    }
    /// The `i32` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn int_mut(&mut self, c_name: &str) -> Option<&mut i32> {
        Some(match c_name {
            "BSIM3dNode" => &mut self.dNode,
            "BSIM3gNode" => &mut self.gNode,
            "BSIM3sNode" => &mut self.sNode,
            "BSIM3bNode" => &mut self.bNode,
            "BSIM3dNodePrime" => &mut self.dNodePrime,
            "BSIM3sNodePrime" => &mut self.sNodePrime,
            "BSIM3qNode" => &mut self.qNode,
            "BSIM3off" => &mut self.off,
            "BSIM3mode" => &mut self.mode,
            "BSIM3nqsMod" => &mut self.nqsMod,
            "BSIM3acnqsMod" => &mut self.acnqsMod,
            "BSIM3geo" => &mut self.geo,
            _ => return None,
        })
    }
    /// The `bool` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn flag_mut(&mut self, c_name: &str) -> Option<&mut bool> {
        Some(match c_name {
            "BSIM3lGiven" => &mut self.lGiven,
            "BSIM3wGiven" => &mut self.wGiven,
            "BSIM3mGiven" => &mut self.mGiven,
            "BSIM3drainAreaGiven" => &mut self.drainAreaGiven,
            "BSIM3sourceAreaGiven" => &mut self.sourceAreaGiven,
            "BSIM3drainSquaresGiven" => &mut self.drainSquaresGiven,
            "BSIM3sourceSquaresGiven" => &mut self.sourceSquaresGiven,
            "BSIM3drainPerimeterGiven" => &mut self.drainPerimeterGiven,
            "BSIM3sourcePerimeterGiven" => &mut self.sourcePerimeterGiven,
            "BSIM3delvtoGiven" => &mut self.delvtoGiven,
            "BSIM3mulu0Given" => &mut self.mulu0Given,
            "BSIM3dNodePrimeSet" => &mut self.dNodePrimeSet,
            "BSIM3sNodePrimeSet" => &mut self.sNodePrimeSet,
            "BSIM3icVBSGiven" => &mut self.icVBSGiven,
            "BSIM3icVDSGiven" => &mut self.icVDSGiven,
            "BSIM3icVGSGiven" => &mut self.icVGSGiven,
            "BSIM3nqsModGiven" => &mut self.nqsModGiven,
            "BSIM3acnqsModGiven" => &mut self.acnqsModGiven,
            "BSIM3geoGiven" => &mut self.geoGiven,
            _ => return None,
        })
    }
    /// C member names of the scalar members, in declaration order.
    #[allow(dead_code)]
    pub(crate) const MEMBERS: &[&str] = &[
        "BSIM3dNode",
        "BSIM3gNode",
        "BSIM3sNode",
        "BSIM3bNode",
        "BSIM3dNodePrime",
        "BSIM3sNodePrime",
        "BSIM3qNode",
        "BSIM3ueff",
        "BSIM3thetavth",
        "BSIM3von",
        "BSIM3vdsat",
        "BSIM3cgdo",
        "BSIM3cgso",
        "BSIM3vjsm",
        "BSIM3IsEvjsm",
        "BSIM3vjdm",
        "BSIM3IsEvjdm",
        "BSIM3l",
        "BSIM3w",
        "BSIM3m",
        "BSIM3drainArea",
        "BSIM3sourceArea",
        "BSIM3drainSquares",
        "BSIM3sourceSquares",
        "BSIM3drainPerimeter",
        "BSIM3sourcePerimeter",
        "BSIM3sourceConductance",
        "BSIM3drainConductance",
        "BSIM3delvto",
        "BSIM3mulu0",
        "BSIM3vth0",
        "BSIM3vfb",
        "BSIM3vfbzb",
        "BSIM3u0temp",
        "BSIM3tconst",
        "BSIM3icVBS",
        "BSIM3icVDS",
        "BSIM3icVGS",
        "BSIM3off",
        "BSIM3mode",
        "BSIM3nqsMod",
        "BSIM3acnqsMod",
        "BSIM3geo",
        "BSIM3qinv",
        "BSIM3cd",
        "BSIM3cbs",
        "BSIM3cbd",
        "BSIM3csub",
        "BSIM3gm",
        "BSIM3gds",
        "BSIM3gmbs",
        "BSIM3gbd",
        "BSIM3gbs",
        "BSIM3gbbs",
        "BSIM3gbgs",
        "BSIM3gbds",
        "BSIM3cggb",
        "BSIM3cgdb",
        "BSIM3cgsb",
        "BSIM3cbgb",
        "BSIM3cbdb",
        "BSIM3cbsb",
        "BSIM3cdgb",
        "BSIM3cddb",
        "BSIM3cdsb",
        "BSIM3capbd",
        "BSIM3capbs",
        "BSIM3cqgb",
        "BSIM3cqdb",
        "BSIM3cqsb",
        "BSIM3cqbb",
        "BSIM3qgate",
        "BSIM3qbulk",
        "BSIM3qdrn",
        "BSIM3gtau",
        "BSIM3gtg",
        "BSIM3gtd",
        "BSIM3gts",
        "BSIM3gtb",
        "BSIM3rds",
        "BSIM3Vgsteff",
        "BSIM3Vdseff",
        "BSIM3Abulk",
        "BSIM3AbovVgst2Vtm",
        "BSIM3taunet",
        "BSIM3lGiven",
        "BSIM3wGiven",
        "BSIM3mGiven",
        "BSIM3drainAreaGiven",
        "BSIM3sourceAreaGiven",
        "BSIM3drainSquaresGiven",
        "BSIM3sourceSquaresGiven",
        "BSIM3drainPerimeterGiven",
        "BSIM3sourcePerimeterGiven",
        "BSIM3delvtoGiven",
        "BSIM3mulu0Given",
        "BSIM3dNodePrimeSet",
        "BSIM3sNodePrimeSet",
        "BSIM3icVBSGiven",
        "BSIM3icVDSGiven",
        "BSIM3icVGSGiven",
        "BSIM3nqsModGiven",
        "BSIM3acnqsModGiven",
        "BSIM3geoGiven",
        "BSIM3rhsG",
        "BSIM3rhsB",
        "BSIM3rhsD",
        "BSIM3rhsS",
        "BSIM3rhsQ",
        "BSIM3DdPt",
        "BSIM3GgPt",
        "BSIM3SsPt",
        "BSIM3BbPt",
        "BSIM3DPdpPt",
        "BSIM3SPspPt",
        "BSIM3DdpPt",
        "BSIM3GbPt",
        "BSIM3GdpPt",
        "BSIM3GspPt",
        "BSIM3SspPt",
        "BSIM3BdpPt",
        "BSIM3BspPt",
        "BSIM3DPspPt",
        "BSIM3DPdPt",
        "BSIM3BgPt",
        "BSIM3DPgPt",
        "BSIM3SPgPt",
        "BSIM3SPsPt",
        "BSIM3DPbPt",
        "BSIM3SPbPt",
        "BSIM3SPdpPt",
        "BSIM3QqPt",
        "BSIM3QdpPt",
        "BSIM3QgPt",
        "BSIM3QspPt",
        "BSIM3QbPt",
        "BSIM3DPqPt",
        "BSIM3GqPt",
        "BSIM3SPqPt",
        "BSIM3BqPt",
    ];

    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,
    /// a given bit as 0 or 1).
    #[allow(dead_code)]
    pub(crate) fn to_values(&self) -> Vec<f64> {
        vec![
            f64::from(self.dNode),
            f64::from(self.gNode),
            f64::from(self.sNode),
            f64::from(self.bNode),
            f64::from(self.dNodePrime),
            f64::from(self.sNodePrime),
            f64::from(self.qNode),
            self.ueff,
            self.thetavth,
            self.von,
            self.vdsat,
            self.cgdo,
            self.cgso,
            self.vjsm,
            self.IsEvjsm,
            self.vjdm,
            self.IsEvjdm,
            self.l,
            self.w,
            self.m,
            self.drainArea,
            self.sourceArea,
            self.drainSquares,
            self.sourceSquares,
            self.drainPerimeter,
            self.sourcePerimeter,
            self.sourceConductance,
            self.drainConductance,
            self.delvto,
            self.mulu0,
            self.vth0,
            self.vfb,
            self.vfbzb,
            self.u0temp,
            self.tconst,
            self.icVBS,
            self.icVDS,
            self.icVGS,
            f64::from(self.off),
            f64::from(self.mode),
            f64::from(self.nqsMod),
            f64::from(self.acnqsMod),
            f64::from(self.geo),
            self.qinv,
            self.cd,
            self.cbs,
            self.cbd,
            self.csub,
            self.gm,
            self.gds,
            self.gmbs,
            self.gbd,
            self.gbs,
            self.gbbs,
            self.gbgs,
            self.gbds,
            self.cggb,
            self.cgdb,
            self.cgsb,
            self.cbgb,
            self.cbdb,
            self.cbsb,
            self.cdgb,
            self.cddb,
            self.cdsb,
            self.capbd,
            self.capbs,
            self.cqgb,
            self.cqdb,
            self.cqsb,
            self.cqbb,
            self.qgate,
            self.qbulk,
            self.qdrn,
            self.gtau,
            self.gtg,
            self.gtd,
            self.gts,
            self.gtb,
            self.rds,
            self.Vgsteff,
            self.Vdseff,
            self.Abulk,
            self.AbovVgst2Vtm,
            self.taunet,
            f64::from(u8::from(self.lGiven)),
            f64::from(u8::from(self.wGiven)),
            f64::from(u8::from(self.mGiven)),
            f64::from(u8::from(self.drainAreaGiven)),
            f64::from(u8::from(self.sourceAreaGiven)),
            f64::from(u8::from(self.drainSquaresGiven)),
            f64::from(u8::from(self.sourceSquaresGiven)),
            f64::from(u8::from(self.drainPerimeterGiven)),
            f64::from(u8::from(self.sourcePerimeterGiven)),
            f64::from(u8::from(self.delvtoGiven)),
            f64::from(u8::from(self.mulu0Given)),
            f64::from(u8::from(self.dNodePrimeSet)),
            f64::from(u8::from(self.sNodePrimeSet)),
            f64::from(u8::from(self.icVBSGiven)),
            f64::from(u8::from(self.icVDSGiven)),
            f64::from(u8::from(self.icVGSGiven)),
            f64::from(u8::from(self.nqsModGiven)),
            f64::from(u8::from(self.acnqsModGiven)),
            f64::from(u8::from(self.geoGiven)),
            self.rhsG,
            self.rhsB,
            self.rhsD,
            self.rhsS,
            self.rhsQ,
            self.DdPt,
            self.GgPt,
            self.SsPt,
            self.BbPt,
            self.DPdpPt,
            self.SPspPt,
            self.DdpPt,
            self.GbPt,
            self.GdpPt,
            self.GspPt,
            self.SspPt,
            self.BdpPt,
            self.BspPt,
            self.DPspPt,
            self.DPdPt,
            self.BgPt,
            self.DPgPt,
            self.SPgPt,
            self.SPsPt,
            self.DPbPt,
            self.SPbPt,
            self.SPdpPt,
            self.QqPt,
            self.QdpPt,
            self.QgPt,
            self.QspPt,
            self.QbPt,
            self.DPqPt,
            self.GqPt,
            self.SPqPt,
            self.BqPt,
        ]
    }

    /// Stores numbers produced by [`Self::to_values`]; `values` must have
    /// [`Self::MEMBERS`]`.len()` entries.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub(crate) fn set_values(&mut self, values: &[f64]) {
        self.dNode = values[0] as i32;
        self.gNode = values[1] as i32;
        self.sNode = values[2] as i32;
        self.bNode = values[3] as i32;
        self.dNodePrime = values[4] as i32;
        self.sNodePrime = values[5] as i32;
        self.qNode = values[6] as i32;
        self.ueff = values[7];
        self.thetavth = values[8];
        self.von = values[9];
        self.vdsat = values[10];
        self.cgdo = values[11];
        self.cgso = values[12];
        self.vjsm = values[13];
        self.IsEvjsm = values[14];
        self.vjdm = values[15];
        self.IsEvjdm = values[16];
        self.l = values[17];
        self.w = values[18];
        self.m = values[19];
        self.drainArea = values[20];
        self.sourceArea = values[21];
        self.drainSquares = values[22];
        self.sourceSquares = values[23];
        self.drainPerimeter = values[24];
        self.sourcePerimeter = values[25];
        self.sourceConductance = values[26];
        self.drainConductance = values[27];
        self.delvto = values[28];
        self.mulu0 = values[29];
        self.vth0 = values[30];
        self.vfb = values[31];
        self.vfbzb = values[32];
        self.u0temp = values[33];
        self.tconst = values[34];
        self.icVBS = values[35];
        self.icVDS = values[36];
        self.icVGS = values[37];
        self.off = values[38] as i32;
        self.mode = values[39] as i32;
        self.nqsMod = values[40] as i32;
        self.acnqsMod = values[41] as i32;
        self.geo = values[42] as i32;
        self.qinv = values[43];
        self.cd = values[44];
        self.cbs = values[45];
        self.cbd = values[46];
        self.csub = values[47];
        self.gm = values[48];
        self.gds = values[49];
        self.gmbs = values[50];
        self.gbd = values[51];
        self.gbs = values[52];
        self.gbbs = values[53];
        self.gbgs = values[54];
        self.gbds = values[55];
        self.cggb = values[56];
        self.cgdb = values[57];
        self.cgsb = values[58];
        self.cbgb = values[59];
        self.cbdb = values[60];
        self.cbsb = values[61];
        self.cdgb = values[62];
        self.cddb = values[63];
        self.cdsb = values[64];
        self.capbd = values[65];
        self.capbs = values[66];
        self.cqgb = values[67];
        self.cqdb = values[68];
        self.cqsb = values[69];
        self.cqbb = values[70];
        self.qgate = values[71];
        self.qbulk = values[72];
        self.qdrn = values[73];
        self.gtau = values[74];
        self.gtg = values[75];
        self.gtd = values[76];
        self.gts = values[77];
        self.gtb = values[78];
        self.rds = values[79];
        self.Vgsteff = values[80];
        self.Vdseff = values[81];
        self.Abulk = values[82];
        self.AbovVgst2Vtm = values[83];
        self.taunet = values[84];
        self.lGiven = values[85] != 0.0;
        self.wGiven = values[86] != 0.0;
        self.mGiven = values[87] != 0.0;
        self.drainAreaGiven = values[88] != 0.0;
        self.sourceAreaGiven = values[89] != 0.0;
        self.drainSquaresGiven = values[90] != 0.0;
        self.sourceSquaresGiven = values[91] != 0.0;
        self.drainPerimeterGiven = values[92] != 0.0;
        self.sourcePerimeterGiven = values[93] != 0.0;
        self.delvtoGiven = values[94] != 0.0;
        self.mulu0Given = values[95] != 0.0;
        self.dNodePrimeSet = values[96] != 0.0;
        self.sNodePrimeSet = values[97] != 0.0;
        self.icVBSGiven = values[98] != 0.0;
        self.icVDSGiven = values[99] != 0.0;
        self.icVGSGiven = values[100] != 0.0;
        self.nqsModGiven = values[101] != 0.0;
        self.acnqsModGiven = values[102] != 0.0;
        self.geoGiven = values[103] != 0.0;
        self.rhsG = values[104];
        self.rhsB = values[105];
        self.rhsD = values[106];
        self.rhsS = values[107];
        self.rhsQ = values[108];
        self.DdPt = values[109];
        self.GgPt = values[110];
        self.SsPt = values[111];
        self.BbPt = values[112];
        self.DPdpPt = values[113];
        self.SPspPt = values[114];
        self.DdpPt = values[115];
        self.GbPt = values[116];
        self.GdpPt = values[117];
        self.GspPt = values[118];
        self.SspPt = values[119];
        self.BdpPt = values[120];
        self.BspPt = values[121];
        self.DPspPt = values[122];
        self.DPdPt = values[123];
        self.BgPt = values[124];
        self.DPgPt = values[125];
        self.SPgPt = values[126];
        self.SPsPt = values[127];
        self.DPbPt = values[128];
        self.SPbPt = values[129];
        self.SPdpPt = values[130];
        self.QqPt = values[131];
        self.QdpPt = values[132];
        self.QgPt = values[133];
        self.QspPt = values[134];
        self.QbPt = values[135];
        self.DPqPt = values[136];
        self.GqPt = values[137];
        self.SPqPt = values[138];
        self.BqPt = values[139];
    }
}
