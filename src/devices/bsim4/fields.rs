// Generated field facts (C member names and scalar types), not C
// implementation code. Source: ngspice 059066b532f7073fccf4173ef269309eafdc7f0c, bsim4/bsim4def.h.
// Regenerate: cargo xtask bsim4-schema --source /path/to/ngspice
#![allow(non_snake_case, clippy::struct_excessive_bools)]

/// `struct sBSIM4model`'s scalar members (1824 of them), in declaration order.
#[derive(Clone, Default)]
pub(crate) struct Model {
    pub(crate) r#type: i32,
    pub(crate) mobMod: i32,
    pub(crate) cvchargeMod: i32,
    pub(crate) capMod: i32,
    pub(crate) dioMod: i32,
    pub(crate) trnqsMod: i32,
    pub(crate) acnqsMod: i32,
    pub(crate) fnoiMod: i32,
    pub(crate) tnoiMod: i32,
    pub(crate) rdsMod: i32,
    pub(crate) rbodyMod: i32,
    pub(crate) rgateMod: i32,
    pub(crate) perMod: i32,
    pub(crate) geoMod: i32,
    pub(crate) mtrlMod: i32,
    pub(crate) mtrlCompatMod: i32,
    pub(crate) gidlMod: i32,
    pub(crate) igcMod: i32,
    pub(crate) igbMod: i32,
    pub(crate) tempMod: i32,
    pub(crate) binUnit: i32,
    pub(crate) paramChk: i32,
    pub(crate) version: String,
    pub(crate) v48intVersion: i32,
    pub(crate) eot: f64,
    pub(crate) vddeot: f64,
    pub(crate) tempeot: f64,
    pub(crate) leffeot: f64,
    pub(crate) weffeot: f64,
    pub(crate) ados: f64,
    pub(crate) bdos: f64,
    pub(crate) toxe: f64,
    pub(crate) toxp: f64,
    pub(crate) toxm: f64,
    pub(crate) dtox: f64,
    pub(crate) epsrox: f64,
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
    pub(crate) ketac: f64,
    pub(crate) nsub: f64,
    pub(crate) phig: f64,
    pub(crate) epsrgate: f64,
    pub(crate) easub: f64,
    pub(crate) epsrsub: f64,
    pub(crate) ni0sub: f64,
    pub(crate) bg0sub: f64,
    pub(crate) tbgasub: f64,
    pub(crate) tbgbsub: f64,
    pub(crate) ndep: f64,
    pub(crate) nsd: f64,
    pub(crate) phin: f64,
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
    pub(crate) dvtp0: f64,
    pub(crate) dvtp1: f64,
    pub(crate) dvtp2: f64,
    pub(crate) dvtp3: f64,
    pub(crate) dvtp4: f64,
    pub(crate) dvtp5: f64,
    pub(crate) lpe0: f64,
    pub(crate) lpeb: f64,
    pub(crate) dvt0: f64,
    pub(crate) dvt1: f64,
    pub(crate) dvt2: f64,
    pub(crate) dvt0w: f64,
    pub(crate) dvt1w: f64,
    pub(crate) dvt2w: f64,
    pub(crate) drout: f64,
    pub(crate) dsub: f64,
    pub(crate) vth0: f64,
    pub(crate) eu: f64,
    pub(crate) ucs: f64,
    pub(crate) ua: f64,
    pub(crate) ua1: f64,
    pub(crate) ub: f64,
    pub(crate) ub1: f64,
    pub(crate) uc: f64,
    pub(crate) uc1: f64,
    pub(crate) ud: f64,
    pub(crate) ud1: f64,
    pub(crate) up: f64,
    pub(crate) lp: f64,
    pub(crate) u0: f64,
    pub(crate) ute: f64,
    pub(crate) ucste: f64,
    pub(crate) voff: f64,
    pub(crate) tvoff: f64,
    pub(crate) tnfactor: f64,
    pub(crate) teta0: f64,
    pub(crate) tvoffcv: f64,
    pub(crate) minv: f64,
    pub(crate) minvcv: f64,
    pub(crate) voffl: f64,
    pub(crate) voffcvl: f64,
    pub(crate) delta: f64,
    pub(crate) rdsw: f64,
    pub(crate) rdswmin: f64,
    pub(crate) rdwmin: f64,
    pub(crate) rswmin: f64,
    pub(crate) rsw: f64,
    pub(crate) rdw: f64,
    pub(crate) prwg: f64,
    pub(crate) prwb: f64,
    pub(crate) prt: f64,
    pub(crate) eta0: f64,
    pub(crate) etab: f64,
    pub(crate) pclm: f64,
    pub(crate) pdibl1: f64,
    pub(crate) pdibl2: f64,
    pub(crate) pdiblb: f64,
    pub(crate) fprout: f64,
    pub(crate) pdits: f64,
    pub(crate) pditsd: f64,
    pub(crate) pditsl: f64,
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
    pub(crate) agidl: f64,
    pub(crate) bgidl: f64,
    pub(crate) cgidl: f64,
    pub(crate) egidl: f64,
    pub(crate) fgidl: f64,
    pub(crate) kgidl: f64,
    pub(crate) rgidl: f64,
    pub(crate) agisl: f64,
    pub(crate) bgisl: f64,
    pub(crate) cgisl: f64,
    pub(crate) egisl: f64,
    pub(crate) fgisl: f64,
    pub(crate) kgisl: f64,
    pub(crate) rgisl: f64,
    pub(crate) aigc: f64,
    pub(crate) bigc: f64,
    pub(crate) cigc: f64,
    pub(crate) aigsd: f64,
    pub(crate) bigsd: f64,
    pub(crate) cigsd: f64,
    pub(crate) aigs: f64,
    pub(crate) bigs: f64,
    pub(crate) cigs: f64,
    pub(crate) aigd: f64,
    pub(crate) bigd: f64,
    pub(crate) cigd: f64,
    pub(crate) aigbacc: f64,
    pub(crate) bigbacc: f64,
    pub(crate) cigbacc: f64,
    pub(crate) aigbinv: f64,
    pub(crate) bigbinv: f64,
    pub(crate) cigbinv: f64,
    pub(crate) nigc: f64,
    pub(crate) nigbacc: f64,
    pub(crate) nigbinv: f64,
    pub(crate) ntox: f64,
    pub(crate) eigbinv: f64,
    pub(crate) pigcd: f64,
    pub(crate) poxedge: f64,
    pub(crate) toxref: f64,
    pub(crate) ijthdfwd: f64,
    pub(crate) ijthsfwd: f64,
    pub(crate) ijthdrev: f64,
    pub(crate) ijthsrev: f64,
    pub(crate) xjbvd: f64,
    pub(crate) xjbvs: f64,
    pub(crate) bvd: f64,
    pub(crate) bvs: f64,
    pub(crate) jtss: f64,
    pub(crate) jtsd: f64,
    pub(crate) jtssws: f64,
    pub(crate) jtsswd: f64,
    pub(crate) jtsswgs: f64,
    pub(crate) jtsswgd: f64,
    pub(crate) jtweff: f64,
    pub(crate) njts: f64,
    pub(crate) njtssw: f64,
    pub(crate) njtsswg: f64,
    pub(crate) njtsd: f64,
    pub(crate) njtsswd: f64,
    pub(crate) njtsswgd: f64,
    pub(crate) xtss: f64,
    pub(crate) xtsd: f64,
    pub(crate) xtssws: f64,
    pub(crate) xtsswd: f64,
    pub(crate) xtsswgs: f64,
    pub(crate) xtsswgd: f64,
    pub(crate) tnjts: f64,
    pub(crate) tnjtssw: f64,
    pub(crate) tnjtsswg: f64,
    pub(crate) tnjtsd: f64,
    pub(crate) tnjtsswd: f64,
    pub(crate) tnjtsswgd: f64,
    pub(crate) vtss: f64,
    pub(crate) vtsd: f64,
    pub(crate) vtssws: f64,
    pub(crate) vtsswd: f64,
    pub(crate) vtsswgs: f64,
    pub(crate) vtsswgd: f64,
    pub(crate) xrcrg1: f64,
    pub(crate) xrcrg2: f64,
    pub(crate) lambda: f64,
    pub(crate) vtl: f64,
    pub(crate) lc: f64,
    pub(crate) xn: f64,
    pub(crate) vfbsdoff: f64,
    pub(crate) lintnoi: f64,
    pub(crate) tvfbsdoff: f64,
    pub(crate) vfb: f64,
    pub(crate) gbmin: f64,
    pub(crate) rbdb: f64,
    pub(crate) rbsb: f64,
    pub(crate) rbpb: f64,
    pub(crate) rbps: f64,
    pub(crate) rbpd: f64,
    pub(crate) rbps0: f64,
    pub(crate) rbpsl: f64,
    pub(crate) rbpsw: f64,
    pub(crate) rbpsnf: f64,
    pub(crate) rbpd0: f64,
    pub(crate) rbpdl: f64,
    pub(crate) rbpdw: f64,
    pub(crate) rbpdnf: f64,
    pub(crate) rbpbx0: f64,
    pub(crate) rbpbxl: f64,
    pub(crate) rbpbxw: f64,
    pub(crate) rbpbxnf: f64,
    pub(crate) rbpby0: f64,
    pub(crate) rbpbyl: f64,
    pub(crate) rbpbyw: f64,
    pub(crate) rbpbynf: f64,
    pub(crate) rbsbx0: f64,
    pub(crate) rbsby0: f64,
    pub(crate) rbdbx0: f64,
    pub(crate) rbdby0: f64,
    pub(crate) rbsdbxl: f64,
    pub(crate) rbsdbxw: f64,
    pub(crate) rbsdbxnf: f64,
    pub(crate) rbsdbyl: f64,
    pub(crate) rbsdbyw: f64,
    pub(crate) rbsdbynf: f64,
    pub(crate) tnoia: f64,
    pub(crate) tnoib: f64,
    pub(crate) tnoic: f64,
    pub(crate) rnoia: f64,
    pub(crate) rnoib: f64,
    pub(crate) rnoic: f64,
    pub(crate) ntnoi: f64,
    pub(crate) cgsl: f64,
    pub(crate) cgdl: f64,
    pub(crate) ckappas: f64,
    pub(crate) ckappad: f64,
    pub(crate) cf: f64,
    pub(crate) vfbcv: f64,
    pub(crate) clc: f64,
    pub(crate) cle: f64,
    pub(crate) dwc: f64,
    pub(crate) dlc: f64,
    pub(crate) xw: f64,
    pub(crate) xl: f64,
    pub(crate) dlcig: f64,
    pub(crate) dlcigd: f64,
    pub(crate) dwj: f64,
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
    pub(crate) dmcg: f64,
    pub(crate) dmci: f64,
    pub(crate) dmdg: f64,
    pub(crate) dmcgt: f64,
    pub(crate) xgw: f64,
    pub(crate) xgl: f64,
    pub(crate) rshg: f64,
    pub(crate) ngcon: f64,
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
    pub(crate) lketac: f64,
    pub(crate) lnsub: f64,
    pub(crate) lndep: f64,
    pub(crate) lnsd: f64,
    pub(crate) lphin: f64,
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
    pub(crate) ldvtp0: f64,
    pub(crate) ldvtp1: f64,
    pub(crate) ldvtp2: f64,
    pub(crate) ldvtp3: f64,
    pub(crate) ldvtp4: f64,
    pub(crate) ldvtp5: f64,
    pub(crate) llpe0: f64,
    pub(crate) llpeb: f64,
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
    pub(crate) lud: f64,
    pub(crate) lud1: f64,
    pub(crate) lup: f64,
    pub(crate) llp: f64,
    pub(crate) lu0: f64,
    pub(crate) leu: f64,
    pub(crate) lucs: f64,
    pub(crate) lute: f64,
    pub(crate) lucste: f64,
    pub(crate) lvoff: f64,
    pub(crate) ltvoff: f64,
    pub(crate) ltnfactor: f64,
    pub(crate) lteta0: f64,
    pub(crate) ltvoffcv: f64,
    pub(crate) lminv: f64,
    pub(crate) lminvcv: f64,
    pub(crate) ldelta: f64,
    pub(crate) lrdsw: f64,
    pub(crate) lrsw: f64,
    pub(crate) lrdw: f64,
    pub(crate) lprwg: f64,
    pub(crate) lprwb: f64,
    pub(crate) lprt: f64,
    pub(crate) leta0: f64,
    pub(crate) letab: f64,
    pub(crate) lpclm: f64,
    pub(crate) lpdibl1: f64,
    pub(crate) lpdibl2: f64,
    pub(crate) lpdiblb: f64,
    pub(crate) lfprout: f64,
    pub(crate) lpdits: f64,
    pub(crate) lpditsd: f64,
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
    pub(crate) lagidl: f64,
    pub(crate) lbgidl: f64,
    pub(crate) lcgidl: f64,
    pub(crate) legidl: f64,
    pub(crate) lfgidl: f64,
    pub(crate) lkgidl: f64,
    pub(crate) lrgidl: f64,
    pub(crate) lagisl: f64,
    pub(crate) lbgisl: f64,
    pub(crate) lcgisl: f64,
    pub(crate) legisl: f64,
    pub(crate) lfgisl: f64,
    pub(crate) lkgisl: f64,
    pub(crate) lrgisl: f64,
    pub(crate) laigc: f64,
    pub(crate) lbigc: f64,
    pub(crate) lcigc: f64,
    pub(crate) laigsd: f64,
    pub(crate) lbigsd: f64,
    pub(crate) lcigsd: f64,
    pub(crate) laigs: f64,
    pub(crate) lbigs: f64,
    pub(crate) lcigs: f64,
    pub(crate) laigd: f64,
    pub(crate) lbigd: f64,
    pub(crate) lcigd: f64,
    pub(crate) laigbacc: f64,
    pub(crate) lbigbacc: f64,
    pub(crate) lcigbacc: f64,
    pub(crate) laigbinv: f64,
    pub(crate) lbigbinv: f64,
    pub(crate) lcigbinv: f64,
    pub(crate) lnigc: f64,
    pub(crate) lnigbacc: f64,
    pub(crate) lnigbinv: f64,
    pub(crate) lntox: f64,
    pub(crate) leigbinv: f64,
    pub(crate) lpigcd: f64,
    pub(crate) lpoxedge: f64,
    pub(crate) lxrcrg1: f64,
    pub(crate) lxrcrg2: f64,
    pub(crate) llambda: f64,
    pub(crate) lvtl: f64,
    pub(crate) lxn: f64,
    pub(crate) lvfbsdoff: f64,
    pub(crate) ltvfbsdoff: f64,
    pub(crate) lcgsl: f64,
    pub(crate) lcgdl: f64,
    pub(crate) lckappas: f64,
    pub(crate) lckappad: f64,
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
    pub(crate) wketac: f64,
    pub(crate) wnsub: f64,
    pub(crate) wndep: f64,
    pub(crate) wnsd: f64,
    pub(crate) wphin: f64,
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
    pub(crate) wdvtp0: f64,
    pub(crate) wdvtp1: f64,
    pub(crate) wdvtp2: f64,
    pub(crate) wdvtp3: f64,
    pub(crate) wdvtp4: f64,
    pub(crate) wdvtp5: f64,
    pub(crate) wlpe0: f64,
    pub(crate) wlpeb: f64,
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
    pub(crate) wud: f64,
    pub(crate) wud1: f64,
    pub(crate) wup: f64,
    pub(crate) wlp: f64,
    pub(crate) wu0: f64,
    pub(crate) weu: f64,
    pub(crate) wucs: f64,
    pub(crate) wute: f64,
    pub(crate) wucste: f64,
    pub(crate) wvoff: f64,
    pub(crate) wtvoff: f64,
    pub(crate) wtnfactor: f64,
    pub(crate) wteta0: f64,
    pub(crate) wtvoffcv: f64,
    pub(crate) wminv: f64,
    pub(crate) wminvcv: f64,
    pub(crate) wdelta: f64,
    pub(crate) wrdsw: f64,
    pub(crate) wrsw: f64,
    pub(crate) wrdw: f64,
    pub(crate) wprwg: f64,
    pub(crate) wprwb: f64,
    pub(crate) wprt: f64,
    pub(crate) weta0: f64,
    pub(crate) wetab: f64,
    pub(crate) wpclm: f64,
    pub(crate) wpdibl1: f64,
    pub(crate) wpdibl2: f64,
    pub(crate) wpdiblb: f64,
    pub(crate) wfprout: f64,
    pub(crate) wpdits: f64,
    pub(crate) wpditsd: f64,
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
    pub(crate) wagidl: f64,
    pub(crate) wbgidl: f64,
    pub(crate) wcgidl: f64,
    pub(crate) wegidl: f64,
    pub(crate) wfgidl: f64,
    pub(crate) wkgidl: f64,
    pub(crate) wrgidl: f64,
    pub(crate) wagisl: f64,
    pub(crate) wbgisl: f64,
    pub(crate) wcgisl: f64,
    pub(crate) wegisl: f64,
    pub(crate) wfgisl: f64,
    pub(crate) wkgisl: f64,
    pub(crate) wrgisl: f64,
    pub(crate) waigc: f64,
    pub(crate) wbigc: f64,
    pub(crate) wcigc: f64,
    pub(crate) waigsd: f64,
    pub(crate) wbigsd: f64,
    pub(crate) wcigsd: f64,
    pub(crate) waigs: f64,
    pub(crate) wbigs: f64,
    pub(crate) wcigs: f64,
    pub(crate) waigd: f64,
    pub(crate) wbigd: f64,
    pub(crate) wcigd: f64,
    pub(crate) waigbacc: f64,
    pub(crate) wbigbacc: f64,
    pub(crate) wcigbacc: f64,
    pub(crate) waigbinv: f64,
    pub(crate) wbigbinv: f64,
    pub(crate) wcigbinv: f64,
    pub(crate) wnigc: f64,
    pub(crate) wnigbacc: f64,
    pub(crate) wnigbinv: f64,
    pub(crate) wntox: f64,
    pub(crate) weigbinv: f64,
    pub(crate) wpigcd: f64,
    pub(crate) wpoxedge: f64,
    pub(crate) wxrcrg1: f64,
    pub(crate) wxrcrg2: f64,
    pub(crate) wlambda: f64,
    pub(crate) wvtl: f64,
    pub(crate) wxn: f64,
    pub(crate) wvfbsdoff: f64,
    pub(crate) wtvfbsdoff: f64,
    pub(crate) wcgsl: f64,
    pub(crate) wcgdl: f64,
    pub(crate) wckappas: f64,
    pub(crate) wckappad: f64,
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
    pub(crate) pketac: f64,
    pub(crate) pnsub: f64,
    pub(crate) pndep: f64,
    pub(crate) pnsd: f64,
    pub(crate) pphin: f64,
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
    pub(crate) pdvtp0: f64,
    pub(crate) pdvtp1: f64,
    pub(crate) pdvtp2: f64,
    pub(crate) pdvtp3: f64,
    pub(crate) pdvtp4: f64,
    pub(crate) pdvtp5: f64,
    pub(crate) plpe0: f64,
    pub(crate) plpeb: f64,
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
    pub(crate) pud: f64,
    pub(crate) pud1: f64,
    pub(crate) pup: f64,
    pub(crate) plp: f64,
    pub(crate) pu0: f64,
    pub(crate) peu: f64,
    pub(crate) pucs: f64,
    pub(crate) pute: f64,
    pub(crate) pucste: f64,
    pub(crate) pvoff: f64,
    pub(crate) ptvoff: f64,
    pub(crate) ptnfactor: f64,
    pub(crate) pteta0: f64,
    pub(crate) ptvoffcv: f64,
    pub(crate) pminv: f64,
    pub(crate) pminvcv: f64,
    pub(crate) pdelta: f64,
    pub(crate) prdsw: f64,
    pub(crate) prsw: f64,
    pub(crate) prdw: f64,
    pub(crate) pprwg: f64,
    pub(crate) pprwb: f64,
    pub(crate) pprt: f64,
    pub(crate) peta0: f64,
    pub(crate) petab: f64,
    pub(crate) ppclm: f64,
    pub(crate) ppdibl1: f64,
    pub(crate) ppdibl2: f64,
    pub(crate) ppdiblb: f64,
    pub(crate) pfprout: f64,
    pub(crate) ppdits: f64,
    pub(crate) ppditsd: f64,
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
    pub(crate) pagidl: f64,
    pub(crate) pbgidl: f64,
    pub(crate) pcgidl: f64,
    pub(crate) pegidl: f64,
    pub(crate) pfgidl: f64,
    pub(crate) pkgidl: f64,
    pub(crate) prgidl: f64,
    pub(crate) pagisl: f64,
    pub(crate) pbgisl: f64,
    pub(crate) pcgisl: f64,
    pub(crate) pegisl: f64,
    pub(crate) pfgisl: f64,
    pub(crate) pkgisl: f64,
    pub(crate) prgisl: f64,
    pub(crate) paigc: f64,
    pub(crate) pbigc: f64,
    pub(crate) pcigc: f64,
    pub(crate) paigsd: f64,
    pub(crate) pbigsd: f64,
    pub(crate) pcigsd: f64,
    pub(crate) paigs: f64,
    pub(crate) pbigs: f64,
    pub(crate) pcigs: f64,
    pub(crate) paigd: f64,
    pub(crate) pbigd: f64,
    pub(crate) pcigd: f64,
    pub(crate) paigbacc: f64,
    pub(crate) pbigbacc: f64,
    pub(crate) pcigbacc: f64,
    pub(crate) paigbinv: f64,
    pub(crate) pbigbinv: f64,
    pub(crate) pcigbinv: f64,
    pub(crate) pnigc: f64,
    pub(crate) pnigbacc: f64,
    pub(crate) pnigbinv: f64,
    pub(crate) pntox: f64,
    pub(crate) peigbinv: f64,
    pub(crate) ppigcd: f64,
    pub(crate) ppoxedge: f64,
    pub(crate) pxrcrg1: f64,
    pub(crate) pxrcrg2: f64,
    pub(crate) plambda: f64,
    pub(crate) pvtl: f64,
    pub(crate) pxn: f64,
    pub(crate) pvfbsdoff: f64,
    pub(crate) ptvfbsdoff: f64,
    pub(crate) pcgsl: f64,
    pub(crate) pcgdl: f64,
    pub(crate) pckappas: f64,
    pub(crate) pckappad: f64,
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
    pub(crate) SjctSatCurDensity: f64,
    pub(crate) DjctSatCurDensity: f64,
    pub(crate) SjctSidewallSatCurDensity: f64,
    pub(crate) DjctSidewallSatCurDensity: f64,
    pub(crate) SjctGateSidewallSatCurDensity: f64,
    pub(crate) DjctGateSidewallSatCurDensity: f64,
    pub(crate) SbulkJctPotential: f64,
    pub(crate) DbulkJctPotential: f64,
    pub(crate) SbulkJctBotGradingCoeff: f64,
    pub(crate) DbulkJctBotGradingCoeff: f64,
    pub(crate) SbulkJctSideGradingCoeff: f64,
    pub(crate) DbulkJctSideGradingCoeff: f64,
    pub(crate) SbulkJctGateSideGradingCoeff: f64,
    pub(crate) DbulkJctGateSideGradingCoeff: f64,
    pub(crate) SsidewallJctPotential: f64,
    pub(crate) DsidewallJctPotential: f64,
    pub(crate) SGatesidewallJctPotential: f64,
    pub(crate) DGatesidewallJctPotential: f64,
    pub(crate) SunitAreaJctCap: f64,
    pub(crate) DunitAreaJctCap: f64,
    pub(crate) SunitLengthSidewallJctCap: f64,
    pub(crate) DunitLengthSidewallJctCap: f64,
    pub(crate) SunitLengthGateSidewallJctCap: f64,
    pub(crate) DunitLengthGateSidewallJctCap: f64,
    pub(crate) SjctEmissionCoeff: f64,
    pub(crate) DjctEmissionCoeff: f64,
    pub(crate) SjctTempExponent: f64,
    pub(crate) DjctTempExponent: f64,
    pub(crate) njtsstemp: f64,
    pub(crate) njtsswstemp: f64,
    pub(crate) njtsswgstemp: f64,
    pub(crate) njtsdtemp: f64,
    pub(crate) njtsswdtemp: f64,
    pub(crate) njtsswgdtemp: f64,
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
    pub(crate) saref: f64,
    pub(crate) sbref: f64,
    pub(crate) wlod: f64,
    pub(crate) ku0: f64,
    pub(crate) kvsat: f64,
    pub(crate) kvth0: f64,
    pub(crate) tku0: f64,
    pub(crate) llodku0: f64,
    pub(crate) wlodku0: f64,
    pub(crate) llodvth: f64,
    pub(crate) wlodvth: f64,
    pub(crate) lku0: f64,
    pub(crate) wku0: f64,
    pub(crate) pku0: f64,
    pub(crate) lkvth0: f64,
    pub(crate) wkvth0: f64,
    pub(crate) pkvth0: f64,
    pub(crate) stk2: f64,
    pub(crate) lodk2: f64,
    pub(crate) steta0: f64,
    pub(crate) lodeta0: f64,
    pub(crate) web: f64,
    pub(crate) wec: f64,
    pub(crate) kvth0we: f64,
    pub(crate) k2we: f64,
    pub(crate) ku0we: f64,
    pub(crate) scref: f64,
    pub(crate) wpemod: f64,
    pub(crate) lkvth0we: f64,
    pub(crate) lk2we: f64,
    pub(crate) lku0we: f64,
    pub(crate) wkvth0we: f64,
    pub(crate) wk2we: f64,
    pub(crate) wku0we: f64,
    pub(crate) pkvth0we: f64,
    pub(crate) pk2we: f64,
    pub(crate) pku0we: f64,
    pub(crate) Eg0: f64,
    pub(crate) vtm: f64,
    pub(crate) vtm0: f64,
    pub(crate) coxe: f64,
    pub(crate) coxp: f64,
    pub(crate) cof1: f64,
    pub(crate) cof2: f64,
    pub(crate) cof3: f64,
    pub(crate) cof4: f64,
    pub(crate) vcrit: f64,
    pub(crate) factor1: f64,
    pub(crate) PhiBS: f64,
    pub(crate) PhiBSWS: f64,
    pub(crate) PhiBSWGS: f64,
    pub(crate) SjctTempSatCurDensity: f64,
    pub(crate) SjctSidewallTempSatCurDensity: f64,
    pub(crate) SjctGateSidewallTempSatCurDensity: f64,
    pub(crate) PhiBD: f64,
    pub(crate) PhiBSWD: f64,
    pub(crate) PhiBSWGD: f64,
    pub(crate) DjctTempSatCurDensity: f64,
    pub(crate) DjctSidewallTempSatCurDensity: f64,
    pub(crate) DjctGateSidewallTempSatCurDensity: f64,
    pub(crate) SunitAreaTempJctCap: f64,
    pub(crate) DunitAreaTempJctCap: f64,
    pub(crate) SunitLengthSidewallTempJctCap: f64,
    pub(crate) DunitLengthSidewallTempJctCap: f64,
    pub(crate) SunitLengthGateSidewallTempJctCap: f64,
    pub(crate) DunitLengthGateSidewallTempJctCap: f64,
    pub(crate) oxideTrapDensityA: f64,
    pub(crate) oxideTrapDensityB: f64,
    pub(crate) oxideTrapDensityC: f64,
    pub(crate) em: f64,
    pub(crate) ef: f64,
    pub(crate) af: f64,
    pub(crate) kf: f64,
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
    pub(crate) gidlclamp: f64,
    pub(crate) idovvdsc: f64,
    pub(crate) InstCount: i32,
    pub(crate) mobModGiven: bool,
    pub(crate) binUnitGiven: bool,
    pub(crate) cvchargeModGiven: bool,
    pub(crate) capModGiven: bool,
    pub(crate) dioModGiven: bool,
    pub(crate) rdsModGiven: bool,
    pub(crate) rbodyModGiven: bool,
    pub(crate) rgateModGiven: bool,
    pub(crate) perModGiven: bool,
    pub(crate) geoModGiven: bool,
    pub(crate) paramChkGiven: bool,
    pub(crate) trnqsModGiven: bool,
    pub(crate) acnqsModGiven: bool,
    pub(crate) fnoiModGiven: bool,
    pub(crate) tnoiModGiven: bool,
    pub(crate) mtrlModGiven: bool,
    pub(crate) mtrlCompatModGiven: bool,
    pub(crate) gidlModGiven: bool,
    pub(crate) igcModGiven: bool,
    pub(crate) igbModGiven: bool,
    pub(crate) tempModGiven: bool,
    pub(crate) typeGiven: bool,
    pub(crate) toxrefGiven: bool,
    pub(crate) eotGiven: bool,
    pub(crate) vddeotGiven: bool,
    pub(crate) tempeotGiven: bool,
    pub(crate) leffeotGiven: bool,
    pub(crate) weffeotGiven: bool,
    pub(crate) adosGiven: bool,
    pub(crate) bdosGiven: bool,
    pub(crate) toxeGiven: bool,
    pub(crate) toxpGiven: bool,
    pub(crate) toxmGiven: bool,
    pub(crate) dtoxGiven: bool,
    pub(crate) epsroxGiven: bool,
    pub(crate) versionGiven: bool,
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
    pub(crate) ketacGiven: bool,
    pub(crate) nsubGiven: bool,
    pub(crate) phigGiven: bool,
    pub(crate) epsrgateGiven: bool,
    pub(crate) easubGiven: bool,
    pub(crate) epsrsubGiven: bool,
    pub(crate) ni0subGiven: bool,
    pub(crate) bg0subGiven: bool,
    pub(crate) tbgasubGiven: bool,
    pub(crate) tbgbsubGiven: bool,
    pub(crate) ndepGiven: bool,
    pub(crate) nsdGiven: bool,
    pub(crate) phinGiven: bool,
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
    pub(crate) dvtp0Given: bool,
    pub(crate) dvtp1Given: bool,
    pub(crate) dvtp2Given: bool,
    pub(crate) dvtp3Given: bool,
    pub(crate) dvtp4Given: bool,
    pub(crate) dvtp5Given: bool,
    pub(crate) lpe0Given: bool,
    pub(crate) lpebGiven: bool,
    pub(crate) dvt0Given: bool,
    pub(crate) dvt1Given: bool,
    pub(crate) dvt2Given: bool,
    pub(crate) dvt0wGiven: bool,
    pub(crate) dvt1wGiven: bool,
    pub(crate) dvt2wGiven: bool,
    pub(crate) droutGiven: bool,
    pub(crate) dsubGiven: bool,
    pub(crate) vth0Given: bool,
    pub(crate) euGiven: bool,
    pub(crate) ucsGiven: bool,
    pub(crate) uaGiven: bool,
    pub(crate) ua1Given: bool,
    pub(crate) ubGiven: bool,
    pub(crate) ub1Given: bool,
    pub(crate) ucGiven: bool,
    pub(crate) uc1Given: bool,
    pub(crate) udGiven: bool,
    pub(crate) ud1Given: bool,
    pub(crate) upGiven: bool,
    pub(crate) lpGiven: bool,
    pub(crate) u0Given: bool,
    pub(crate) uteGiven: bool,
    pub(crate) ucsteGiven: bool,
    pub(crate) voffGiven: bool,
    pub(crate) tvoffGiven: bool,
    pub(crate) tnfactorGiven: bool,
    pub(crate) teta0Given: bool,
    pub(crate) tvoffcvGiven: bool,
    pub(crate) vofflGiven: bool,
    pub(crate) voffcvlGiven: bool,
    pub(crate) minvGiven: bool,
    pub(crate) minvcvGiven: bool,
    pub(crate) rdswGiven: bool,
    pub(crate) rdswminGiven: bool,
    pub(crate) rdwminGiven: bool,
    pub(crate) rswminGiven: bool,
    pub(crate) rswGiven: bool,
    pub(crate) rdwGiven: bool,
    pub(crate) prwgGiven: bool,
    pub(crate) prwbGiven: bool,
    pub(crate) prtGiven: bool,
    pub(crate) eta0Given: bool,
    pub(crate) etabGiven: bool,
    pub(crate) pclmGiven: bool,
    pub(crate) pdibl1Given: bool,
    pub(crate) pdibl2Given: bool,
    pub(crate) pdiblbGiven: bool,
    pub(crate) fproutGiven: bool,
    pub(crate) pditsGiven: bool,
    pub(crate) pditsdGiven: bool,
    pub(crate) pditslGiven: bool,
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
    pub(crate) agidlGiven: bool,
    pub(crate) bgidlGiven: bool,
    pub(crate) cgidlGiven: bool,
    pub(crate) egidlGiven: bool,
    pub(crate) fgidlGiven: bool,
    pub(crate) kgidlGiven: bool,
    pub(crate) rgidlGiven: bool,
    pub(crate) agislGiven: bool,
    pub(crate) bgislGiven: bool,
    pub(crate) cgislGiven: bool,
    pub(crate) egislGiven: bool,
    pub(crate) fgislGiven: bool,
    pub(crate) kgislGiven: bool,
    pub(crate) rgislGiven: bool,
    pub(crate) aigcGiven: bool,
    pub(crate) bigcGiven: bool,
    pub(crate) cigcGiven: bool,
    pub(crate) aigsdGiven: bool,
    pub(crate) bigsdGiven: bool,
    pub(crate) cigsdGiven: bool,
    pub(crate) aigsGiven: bool,
    pub(crate) bigsGiven: bool,
    pub(crate) cigsGiven: bool,
    pub(crate) aigdGiven: bool,
    pub(crate) bigdGiven: bool,
    pub(crate) cigdGiven: bool,
    pub(crate) aigbaccGiven: bool,
    pub(crate) bigbaccGiven: bool,
    pub(crate) cigbaccGiven: bool,
    pub(crate) aigbinvGiven: bool,
    pub(crate) bigbinvGiven: bool,
    pub(crate) cigbinvGiven: bool,
    pub(crate) nigcGiven: bool,
    pub(crate) nigbinvGiven: bool,
    pub(crate) nigbaccGiven: bool,
    pub(crate) ntoxGiven: bool,
    pub(crate) eigbinvGiven: bool,
    pub(crate) pigcdGiven: bool,
    pub(crate) poxedgeGiven: bool,
    pub(crate) ijthdfwdGiven: bool,
    pub(crate) ijthsfwdGiven: bool,
    pub(crate) ijthdrevGiven: bool,
    pub(crate) ijthsrevGiven: bool,
    pub(crate) xjbvdGiven: bool,
    pub(crate) xjbvsGiven: bool,
    pub(crate) bvdGiven: bool,
    pub(crate) bvsGiven: bool,
    pub(crate) jtssGiven: bool,
    pub(crate) jtsdGiven: bool,
    pub(crate) jtsswsGiven: bool,
    pub(crate) jtsswdGiven: bool,
    pub(crate) jtsswgsGiven: bool,
    pub(crate) jtsswgdGiven: bool,
    pub(crate) jtweffGiven: bool,
    pub(crate) njtsGiven: bool,
    pub(crate) njtsswGiven: bool,
    pub(crate) njtsswgGiven: bool,
    pub(crate) njtsdGiven: bool,
    pub(crate) njtsswdGiven: bool,
    pub(crate) njtsswgdGiven: bool,
    pub(crate) xtssGiven: bool,
    pub(crate) xtsdGiven: bool,
    pub(crate) xtsswsGiven: bool,
    pub(crate) xtsswdGiven: bool,
    pub(crate) xtsswgsGiven: bool,
    pub(crate) xtsswgdGiven: bool,
    pub(crate) tnjtsGiven: bool,
    pub(crate) tnjtsswGiven: bool,
    pub(crate) tnjtsswgGiven: bool,
    pub(crate) tnjtsdGiven: bool,
    pub(crate) tnjtsswdGiven: bool,
    pub(crate) tnjtsswgdGiven: bool,
    pub(crate) vtssGiven: bool,
    pub(crate) vtsdGiven: bool,
    pub(crate) vtsswsGiven: bool,
    pub(crate) vtsswdGiven: bool,
    pub(crate) vtsswgsGiven: bool,
    pub(crate) vtsswgdGiven: bool,
    pub(crate) vfbGiven: bool,
    pub(crate) gbminGiven: bool,
    pub(crate) rbdbGiven: bool,
    pub(crate) rbsbGiven: bool,
    pub(crate) rbpsGiven: bool,
    pub(crate) rbpdGiven: bool,
    pub(crate) rbpbGiven: bool,
    pub(crate) rbps0Given: bool,
    pub(crate) rbpslGiven: bool,
    pub(crate) rbpswGiven: bool,
    pub(crate) rbpsnfGiven: bool,
    pub(crate) rbpd0Given: bool,
    pub(crate) rbpdlGiven: bool,
    pub(crate) rbpdwGiven: bool,
    pub(crate) rbpdnfGiven: bool,
    pub(crate) rbpbx0Given: bool,
    pub(crate) rbpbxlGiven: bool,
    pub(crate) rbpbxwGiven: bool,
    pub(crate) rbpbxnfGiven: bool,
    pub(crate) rbpby0Given: bool,
    pub(crate) rbpbylGiven: bool,
    pub(crate) rbpbywGiven: bool,
    pub(crate) rbpbynfGiven: bool,
    pub(crate) rbsbx0Given: bool,
    pub(crate) rbsby0Given: bool,
    pub(crate) rbdbx0Given: bool,
    pub(crate) rbdby0Given: bool,
    pub(crate) rbsdbxlGiven: bool,
    pub(crate) rbsdbxwGiven: bool,
    pub(crate) rbsdbxnfGiven: bool,
    pub(crate) rbsdbylGiven: bool,
    pub(crate) rbsdbywGiven: bool,
    pub(crate) rbsdbynfGiven: bool,
    pub(crate) xrcrg1Given: bool,
    pub(crate) xrcrg2Given: bool,
    pub(crate) tnoiaGiven: bool,
    pub(crate) tnoibGiven: bool,
    pub(crate) tnoicGiven: bool,
    pub(crate) rnoiaGiven: bool,
    pub(crate) rnoibGiven: bool,
    pub(crate) rnoicGiven: bool,
    pub(crate) ntnoiGiven: bool,
    pub(crate) lambdaGiven: bool,
    pub(crate) vtlGiven: bool,
    pub(crate) lcGiven: bool,
    pub(crate) xnGiven: bool,
    pub(crate) vfbsdoffGiven: bool,
    pub(crate) lintnoiGiven: bool,
    pub(crate) tvfbsdoffGiven: bool,
    pub(crate) cgslGiven: bool,
    pub(crate) cgdlGiven: bool,
    pub(crate) ckappasGiven: bool,
    pub(crate) ckappadGiven: bool,
    pub(crate) cfGiven: bool,
    pub(crate) vfbcvGiven: bool,
    pub(crate) clcGiven: bool,
    pub(crate) cleGiven: bool,
    pub(crate) dwcGiven: bool,
    pub(crate) dlcGiven: bool,
    pub(crate) xwGiven: bool,
    pub(crate) xlGiven: bool,
    pub(crate) dlcigGiven: bool,
    pub(crate) dlcigdGiven: bool,
    pub(crate) dwjGiven: bool,
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
    pub(crate) dmcgGiven: bool,
    pub(crate) dmciGiven: bool,
    pub(crate) dmdgGiven: bool,
    pub(crate) dmcgtGiven: bool,
    pub(crate) xgwGiven: bool,
    pub(crate) xglGiven: bool,
    pub(crate) rshgGiven: bool,
    pub(crate) ngconGiven: bool,
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
    pub(crate) lketacGiven: bool,
    pub(crate) lnsubGiven: bool,
    pub(crate) lndepGiven: bool,
    pub(crate) lnsdGiven: bool,
    pub(crate) lphinGiven: bool,
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
    pub(crate) ldvtp0Given: bool,
    pub(crate) ldvtp1Given: bool,
    pub(crate) ldvtp2Given: bool,
    pub(crate) ldvtp3Given: bool,
    pub(crate) ldvtp4Given: bool,
    pub(crate) ldvtp5Given: bool,
    pub(crate) llpe0Given: bool,
    pub(crate) llpebGiven: bool,
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
    pub(crate) ludGiven: bool,
    pub(crate) lud1Given: bool,
    pub(crate) lupGiven: bool,
    pub(crate) llpGiven: bool,
    pub(crate) lu0Given: bool,
    pub(crate) leuGiven: bool,
    pub(crate) lucsGiven: bool,
    pub(crate) luteGiven: bool,
    pub(crate) lucsteGiven: bool,
    pub(crate) lvoffGiven: bool,
    pub(crate) ltvoffGiven: bool,
    pub(crate) ltnfactorGiven: bool,
    pub(crate) lteta0Given: bool,
    pub(crate) ltvoffcvGiven: bool,
    pub(crate) lminvGiven: bool,
    pub(crate) lminvcvGiven: bool,
    pub(crate) lrdswGiven: bool,
    pub(crate) lrswGiven: bool,
    pub(crate) lrdwGiven: bool,
    pub(crate) lprwgGiven: bool,
    pub(crate) lprwbGiven: bool,
    pub(crate) lprtGiven: bool,
    pub(crate) leta0Given: bool,
    pub(crate) letabGiven: bool,
    pub(crate) lpclmGiven: bool,
    pub(crate) lpdibl1Given: bool,
    pub(crate) lpdibl2Given: bool,
    pub(crate) lpdiblbGiven: bool,
    pub(crate) lfproutGiven: bool,
    pub(crate) lpditsGiven: bool,
    pub(crate) lpditsdGiven: bool,
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
    pub(crate) lagidlGiven: bool,
    pub(crate) lbgidlGiven: bool,
    pub(crate) lcgidlGiven: bool,
    pub(crate) legidlGiven: bool,
    pub(crate) lfgidlGiven: bool,
    pub(crate) lkgidlGiven: bool,
    pub(crate) lrgidlGiven: bool,
    pub(crate) lagislGiven: bool,
    pub(crate) lbgislGiven: bool,
    pub(crate) lcgislGiven: bool,
    pub(crate) legislGiven: bool,
    pub(crate) lfgislGiven: bool,
    pub(crate) lkgislGiven: bool,
    pub(crate) lrgislGiven: bool,
    pub(crate) laigcGiven: bool,
    pub(crate) lbigcGiven: bool,
    pub(crate) lcigcGiven: bool,
    pub(crate) laigsdGiven: bool,
    pub(crate) lbigsdGiven: bool,
    pub(crate) lcigsdGiven: bool,
    pub(crate) laigsGiven: bool,
    pub(crate) lbigsGiven: bool,
    pub(crate) lcigsGiven: bool,
    pub(crate) laigdGiven: bool,
    pub(crate) lbigdGiven: bool,
    pub(crate) lcigdGiven: bool,
    pub(crate) laigbaccGiven: bool,
    pub(crate) lbigbaccGiven: bool,
    pub(crate) lcigbaccGiven: bool,
    pub(crate) laigbinvGiven: bool,
    pub(crate) lbigbinvGiven: bool,
    pub(crate) lcigbinvGiven: bool,
    pub(crate) lnigcGiven: bool,
    pub(crate) lnigbinvGiven: bool,
    pub(crate) lnigbaccGiven: bool,
    pub(crate) lntoxGiven: bool,
    pub(crate) leigbinvGiven: bool,
    pub(crate) lpigcdGiven: bool,
    pub(crate) lpoxedgeGiven: bool,
    pub(crate) lxrcrg1Given: bool,
    pub(crate) lxrcrg2Given: bool,
    pub(crate) llambdaGiven: bool,
    pub(crate) lvtlGiven: bool,
    pub(crate) lxnGiven: bool,
    pub(crate) lvfbsdoffGiven: bool,
    pub(crate) ltvfbsdoffGiven: bool,
    pub(crate) lcgslGiven: bool,
    pub(crate) lcgdlGiven: bool,
    pub(crate) lckappasGiven: bool,
    pub(crate) lckappadGiven: bool,
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
    pub(crate) wketacGiven: bool,
    pub(crate) wnsubGiven: bool,
    pub(crate) wndepGiven: bool,
    pub(crate) wnsdGiven: bool,
    pub(crate) wphinGiven: bool,
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
    pub(crate) wdvtp0Given: bool,
    pub(crate) wdvtp1Given: bool,
    pub(crate) wdvtp2Given: bool,
    pub(crate) wdvtp3Given: bool,
    pub(crate) wdvtp4Given: bool,
    pub(crate) wdvtp5Given: bool,
    pub(crate) wlpe0Given: bool,
    pub(crate) wlpebGiven: bool,
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
    pub(crate) wudGiven: bool,
    pub(crate) wud1Given: bool,
    pub(crate) wupGiven: bool,
    pub(crate) wlpGiven: bool,
    pub(crate) wu0Given: bool,
    pub(crate) weuGiven: bool,
    pub(crate) wucsGiven: bool,
    pub(crate) wuteGiven: bool,
    pub(crate) wucsteGiven: bool,
    pub(crate) wvoffGiven: bool,
    pub(crate) wtvoffGiven: bool,
    pub(crate) wtnfactorGiven: bool,
    pub(crate) wteta0Given: bool,
    pub(crate) wtvoffcvGiven: bool,
    pub(crate) wminvGiven: bool,
    pub(crate) wminvcvGiven: bool,
    pub(crate) wrdswGiven: bool,
    pub(crate) wrswGiven: bool,
    pub(crate) wrdwGiven: bool,
    pub(crate) wprwgGiven: bool,
    pub(crate) wprwbGiven: bool,
    pub(crate) wprtGiven: bool,
    pub(crate) weta0Given: bool,
    pub(crate) wetabGiven: bool,
    pub(crate) wpclmGiven: bool,
    pub(crate) wpdibl1Given: bool,
    pub(crate) wpdibl2Given: bool,
    pub(crate) wpdiblbGiven: bool,
    pub(crate) wfproutGiven: bool,
    pub(crate) wpditsGiven: bool,
    pub(crate) wpditsdGiven: bool,
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
    pub(crate) wagidlGiven: bool,
    pub(crate) wbgidlGiven: bool,
    pub(crate) wcgidlGiven: bool,
    pub(crate) wegidlGiven: bool,
    pub(crate) wfgidlGiven: bool,
    pub(crate) wkgidlGiven: bool,
    pub(crate) wrgidlGiven: bool,
    pub(crate) wagislGiven: bool,
    pub(crate) wbgislGiven: bool,
    pub(crate) wcgislGiven: bool,
    pub(crate) wegislGiven: bool,
    pub(crate) wfgislGiven: bool,
    pub(crate) wkgislGiven: bool,
    pub(crate) wrgislGiven: bool,
    pub(crate) waigcGiven: bool,
    pub(crate) wbigcGiven: bool,
    pub(crate) wcigcGiven: bool,
    pub(crate) waigsdGiven: bool,
    pub(crate) wbigsdGiven: bool,
    pub(crate) wcigsdGiven: bool,
    pub(crate) waigsGiven: bool,
    pub(crate) wbigsGiven: bool,
    pub(crate) wcigsGiven: bool,
    pub(crate) waigdGiven: bool,
    pub(crate) wbigdGiven: bool,
    pub(crate) wcigdGiven: bool,
    pub(crate) waigbaccGiven: bool,
    pub(crate) wbigbaccGiven: bool,
    pub(crate) wcigbaccGiven: bool,
    pub(crate) waigbinvGiven: bool,
    pub(crate) wbigbinvGiven: bool,
    pub(crate) wcigbinvGiven: bool,
    pub(crate) wnigcGiven: bool,
    pub(crate) wnigbinvGiven: bool,
    pub(crate) wnigbaccGiven: bool,
    pub(crate) wntoxGiven: bool,
    pub(crate) weigbinvGiven: bool,
    pub(crate) wpigcdGiven: bool,
    pub(crate) wpoxedgeGiven: bool,
    pub(crate) wxrcrg1Given: bool,
    pub(crate) wxrcrg2Given: bool,
    pub(crate) wlambdaGiven: bool,
    pub(crate) wvtlGiven: bool,
    pub(crate) wxnGiven: bool,
    pub(crate) wvfbsdoffGiven: bool,
    pub(crate) wtvfbsdoffGiven: bool,
    pub(crate) wcgslGiven: bool,
    pub(crate) wcgdlGiven: bool,
    pub(crate) wckappasGiven: bool,
    pub(crate) wckappadGiven: bool,
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
    pub(crate) pketacGiven: bool,
    pub(crate) pnsubGiven: bool,
    pub(crate) pndepGiven: bool,
    pub(crate) pnsdGiven: bool,
    pub(crate) pphinGiven: bool,
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
    pub(crate) pdvtp0Given: bool,
    pub(crate) pdvtp1Given: bool,
    pub(crate) pdvtp2Given: bool,
    pub(crate) pdvtp3Given: bool,
    pub(crate) pdvtp4Given: bool,
    pub(crate) pdvtp5Given: bool,
    pub(crate) plpe0Given: bool,
    pub(crate) plpebGiven: bool,
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
    pub(crate) pudGiven: bool,
    pub(crate) pud1Given: bool,
    pub(crate) pupGiven: bool,
    pub(crate) plpGiven: bool,
    pub(crate) pu0Given: bool,
    pub(crate) peuGiven: bool,
    pub(crate) pucsGiven: bool,
    pub(crate) puteGiven: bool,
    pub(crate) pucsteGiven: bool,
    pub(crate) pvoffGiven: bool,
    pub(crate) ptvoffGiven: bool,
    pub(crate) ptnfactorGiven: bool,
    pub(crate) pteta0Given: bool,
    pub(crate) ptvoffcvGiven: bool,
    pub(crate) pminvGiven: bool,
    pub(crate) pminvcvGiven: bool,
    pub(crate) prdswGiven: bool,
    pub(crate) prswGiven: bool,
    pub(crate) prdwGiven: bool,
    pub(crate) pprwgGiven: bool,
    pub(crate) pprwbGiven: bool,
    pub(crate) pprtGiven: bool,
    pub(crate) peta0Given: bool,
    pub(crate) petabGiven: bool,
    pub(crate) ppclmGiven: bool,
    pub(crate) ppdibl1Given: bool,
    pub(crate) ppdibl2Given: bool,
    pub(crate) ppdiblbGiven: bool,
    pub(crate) pfproutGiven: bool,
    pub(crate) ppditsGiven: bool,
    pub(crate) ppditsdGiven: bool,
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
    pub(crate) pagidlGiven: bool,
    pub(crate) pbgidlGiven: bool,
    pub(crate) pcgidlGiven: bool,
    pub(crate) pegidlGiven: bool,
    pub(crate) pfgidlGiven: bool,
    pub(crate) pkgidlGiven: bool,
    pub(crate) prgidlGiven: bool,
    pub(crate) pagislGiven: bool,
    pub(crate) pbgislGiven: bool,
    pub(crate) pcgislGiven: bool,
    pub(crate) pegislGiven: bool,
    pub(crate) pfgislGiven: bool,
    pub(crate) pkgislGiven: bool,
    pub(crate) prgislGiven: bool,
    pub(crate) paigcGiven: bool,
    pub(crate) pbigcGiven: bool,
    pub(crate) pcigcGiven: bool,
    pub(crate) paigsdGiven: bool,
    pub(crate) pbigsdGiven: bool,
    pub(crate) pcigsdGiven: bool,
    pub(crate) paigsGiven: bool,
    pub(crate) pbigsGiven: bool,
    pub(crate) pcigsGiven: bool,
    pub(crate) paigdGiven: bool,
    pub(crate) pbigdGiven: bool,
    pub(crate) pcigdGiven: bool,
    pub(crate) paigbaccGiven: bool,
    pub(crate) pbigbaccGiven: bool,
    pub(crate) pcigbaccGiven: bool,
    pub(crate) paigbinvGiven: bool,
    pub(crate) pbigbinvGiven: bool,
    pub(crate) pcigbinvGiven: bool,
    pub(crate) pnigcGiven: bool,
    pub(crate) pnigbinvGiven: bool,
    pub(crate) pnigbaccGiven: bool,
    pub(crate) pntoxGiven: bool,
    pub(crate) peigbinvGiven: bool,
    pub(crate) ppigcdGiven: bool,
    pub(crate) ppoxedgeGiven: bool,
    pub(crate) pxrcrg1Given: bool,
    pub(crate) pxrcrg2Given: bool,
    pub(crate) plambdaGiven: bool,
    pub(crate) pvtlGiven: bool,
    pub(crate) pxnGiven: bool,
    pub(crate) pvfbsdoffGiven: bool,
    pub(crate) ptvfbsdoffGiven: bool,
    pub(crate) pcgslGiven: bool,
    pub(crate) pcgdlGiven: bool,
    pub(crate) pckappasGiven: bool,
    pub(crate) pckappadGiven: bool,
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
    pub(crate) SjctSatCurDensityGiven: bool,
    pub(crate) SjctSidewallSatCurDensityGiven: bool,
    pub(crate) SjctGateSidewallSatCurDensityGiven: bool,
    pub(crate) SbulkJctPotentialGiven: bool,
    pub(crate) SbulkJctBotGradingCoeffGiven: bool,
    pub(crate) SsidewallJctPotentialGiven: bool,
    pub(crate) SGatesidewallJctPotentialGiven: bool,
    pub(crate) SbulkJctSideGradingCoeffGiven: bool,
    pub(crate) SunitAreaJctCapGiven: bool,
    pub(crate) SunitLengthSidewallJctCapGiven: bool,
    pub(crate) SbulkJctGateSideGradingCoeffGiven: bool,
    pub(crate) SunitLengthGateSidewallJctCapGiven: bool,
    pub(crate) SjctEmissionCoeffGiven: bool,
    pub(crate) SjctTempExponentGiven: bool,
    pub(crate) DjctSatCurDensityGiven: bool,
    pub(crate) DjctSidewallSatCurDensityGiven: bool,
    pub(crate) DjctGateSidewallSatCurDensityGiven: bool,
    pub(crate) DbulkJctPotentialGiven: bool,
    pub(crate) DbulkJctBotGradingCoeffGiven: bool,
    pub(crate) DsidewallJctPotentialGiven: bool,
    pub(crate) DGatesidewallJctPotentialGiven: bool,
    pub(crate) DbulkJctSideGradingCoeffGiven: bool,
    pub(crate) DunitAreaJctCapGiven: bool,
    pub(crate) DunitLengthSidewallJctCapGiven: bool,
    pub(crate) DbulkJctGateSideGradingCoeffGiven: bool,
    pub(crate) DunitLengthGateSidewallJctCapGiven: bool,
    pub(crate) DjctEmissionCoeffGiven: bool,
    pub(crate) DjctTempExponentGiven: bool,
    pub(crate) oxideTrapDensityAGiven: bool,
    pub(crate) oxideTrapDensityBGiven: bool,
    pub(crate) oxideTrapDensityCGiven: bool,
    pub(crate) emGiven: bool,
    pub(crate) efGiven: bool,
    pub(crate) afGiven: bool,
    pub(crate) kfGiven: bool,
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
    pub(crate) sarefGiven: bool,
    pub(crate) sbrefGiven: bool,
    pub(crate) wlodGiven: bool,
    pub(crate) ku0Given: bool,
    pub(crate) kvsatGiven: bool,
    pub(crate) kvth0Given: bool,
    pub(crate) tku0Given: bool,
    pub(crate) llodku0Given: bool,
    pub(crate) wlodku0Given: bool,
    pub(crate) llodvthGiven: bool,
    pub(crate) wlodvthGiven: bool,
    pub(crate) lku0Given: bool,
    pub(crate) wku0Given: bool,
    pub(crate) pku0Given: bool,
    pub(crate) lkvth0Given: bool,
    pub(crate) wkvth0Given: bool,
    pub(crate) pkvth0Given: bool,
    pub(crate) stk2Given: bool,
    pub(crate) lodk2Given: bool,
    pub(crate) steta0Given: bool,
    pub(crate) lodeta0Given: bool,
    pub(crate) webGiven: bool,
    pub(crate) wecGiven: bool,
    pub(crate) kvth0weGiven: bool,
    pub(crate) k2weGiven: bool,
    pub(crate) ku0weGiven: bool,
    pub(crate) screfGiven: bool,
    pub(crate) wpemodGiven: bool,
    pub(crate) lkvth0weGiven: bool,
    pub(crate) lk2weGiven: bool,
    pub(crate) lku0weGiven: bool,
    pub(crate) wkvth0weGiven: bool,
    pub(crate) wk2weGiven: bool,
    pub(crate) wku0weGiven: bool,
    pub(crate) pkvth0weGiven: bool,
    pub(crate) pk2weGiven: bool,
    pub(crate) pku0weGiven: bool,
    pub(crate) gidlclampGiven: bool,
    pub(crate) idovvdscGiven: bool,
}

#[rustfmt::skip]
impl Model {
    /// The `f64` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn real_mut(&mut self, c_name: &str) -> Option<&mut f64> {
        Some(match c_name {
            "BSIM4eot" => &mut self.eot,
            "BSIM4vddeot" => &mut self.vddeot,
            "BSIM4tempeot" => &mut self.tempeot,
            "BSIM4leffeot" => &mut self.leffeot,
            "BSIM4weffeot" => &mut self.weffeot,
            "BSIM4ados" => &mut self.ados,
            "BSIM4bdos" => &mut self.bdos,
            "BSIM4toxe" => &mut self.toxe,
            "BSIM4toxp" => &mut self.toxp,
            "BSIM4toxm" => &mut self.toxm,
            "BSIM4dtox" => &mut self.dtox,
            "BSIM4epsrox" => &mut self.epsrox,
            "BSIM4cdsc" => &mut self.cdsc,
            "BSIM4cdscb" => &mut self.cdscb,
            "BSIM4cdscd" => &mut self.cdscd,
            "BSIM4cit" => &mut self.cit,
            "BSIM4nfactor" => &mut self.nfactor,
            "BSIM4xj" => &mut self.xj,
            "BSIM4vsat" => &mut self.vsat,
            "BSIM4at" => &mut self.at,
            "BSIM4a0" => &mut self.a0,
            "BSIM4ags" => &mut self.ags,
            "BSIM4a1" => &mut self.a1,
            "BSIM4a2" => &mut self.a2,
            "BSIM4keta" => &mut self.keta,
            "BSIM4ketac" => &mut self.ketac,
            "BSIM4nsub" => &mut self.nsub,
            "BSIM4phig" => &mut self.phig,
            "BSIM4epsrgate" => &mut self.epsrgate,
            "BSIM4easub" => &mut self.easub,
            "BSIM4epsrsub" => &mut self.epsrsub,
            "BSIM4ni0sub" => &mut self.ni0sub,
            "BSIM4bg0sub" => &mut self.bg0sub,
            "BSIM4tbgasub" => &mut self.tbgasub,
            "BSIM4tbgbsub" => &mut self.tbgbsub,
            "BSIM4ndep" => &mut self.ndep,
            "BSIM4nsd" => &mut self.nsd,
            "BSIM4phin" => &mut self.phin,
            "BSIM4ngate" => &mut self.ngate,
            "BSIM4gamma1" => &mut self.gamma1,
            "BSIM4gamma2" => &mut self.gamma2,
            "BSIM4vbx" => &mut self.vbx,
            "BSIM4vbm" => &mut self.vbm,
            "BSIM4xt" => &mut self.xt,
            "BSIM4k1" => &mut self.k1,
            "BSIM4kt1" => &mut self.kt1,
            "BSIM4kt1l" => &mut self.kt1l,
            "BSIM4kt2" => &mut self.kt2,
            "BSIM4k2" => &mut self.k2,
            "BSIM4k3" => &mut self.k3,
            "BSIM4k3b" => &mut self.k3b,
            "BSIM4w0" => &mut self.w0,
            "BSIM4dvtp0" => &mut self.dvtp0,
            "BSIM4dvtp1" => &mut self.dvtp1,
            "BSIM4dvtp2" => &mut self.dvtp2,
            "BSIM4dvtp3" => &mut self.dvtp3,
            "BSIM4dvtp4" => &mut self.dvtp4,
            "BSIM4dvtp5" => &mut self.dvtp5,
            "BSIM4lpe0" => &mut self.lpe0,
            "BSIM4lpeb" => &mut self.lpeb,
            "BSIM4dvt0" => &mut self.dvt0,
            "BSIM4dvt1" => &mut self.dvt1,
            "BSIM4dvt2" => &mut self.dvt2,
            "BSIM4dvt0w" => &mut self.dvt0w,
            "BSIM4dvt1w" => &mut self.dvt1w,
            "BSIM4dvt2w" => &mut self.dvt2w,
            "BSIM4drout" => &mut self.drout,
            "BSIM4dsub" => &mut self.dsub,
            "BSIM4vth0" => &mut self.vth0,
            "BSIM4eu" => &mut self.eu,
            "BSIM4ucs" => &mut self.ucs,
            "BSIM4ua" => &mut self.ua,
            "BSIM4ua1" => &mut self.ua1,
            "BSIM4ub" => &mut self.ub,
            "BSIM4ub1" => &mut self.ub1,
            "BSIM4uc" => &mut self.uc,
            "BSIM4uc1" => &mut self.uc1,
            "BSIM4ud" => &mut self.ud,
            "BSIM4ud1" => &mut self.ud1,
            "BSIM4up" => &mut self.up,
            "BSIM4lp" => &mut self.lp,
            "BSIM4u0" => &mut self.u0,
            "BSIM4ute" => &mut self.ute,
            "BSIM4ucste" => &mut self.ucste,
            "BSIM4voff" => &mut self.voff,
            "BSIM4tvoff" => &mut self.tvoff,
            "BSIM4tnfactor" => &mut self.tnfactor,
            "BSIM4teta0" => &mut self.teta0,
            "BSIM4tvoffcv" => &mut self.tvoffcv,
            "BSIM4minv" => &mut self.minv,
            "BSIM4minvcv" => &mut self.minvcv,
            "BSIM4voffl" => &mut self.voffl,
            "BSIM4voffcvl" => &mut self.voffcvl,
            "BSIM4delta" => &mut self.delta,
            "BSIM4rdsw" => &mut self.rdsw,
            "BSIM4rdswmin" => &mut self.rdswmin,
            "BSIM4rdwmin" => &mut self.rdwmin,
            "BSIM4rswmin" => &mut self.rswmin,
            "BSIM4rsw" => &mut self.rsw,
            "BSIM4rdw" => &mut self.rdw,
            "BSIM4prwg" => &mut self.prwg,
            "BSIM4prwb" => &mut self.prwb,
            "BSIM4prt" => &mut self.prt,
            "BSIM4eta0" => &mut self.eta0,
            "BSIM4etab" => &mut self.etab,
            "BSIM4pclm" => &mut self.pclm,
            "BSIM4pdibl1" => &mut self.pdibl1,
            "BSIM4pdibl2" => &mut self.pdibl2,
            "BSIM4pdiblb" => &mut self.pdiblb,
            "BSIM4fprout" => &mut self.fprout,
            "BSIM4pdits" => &mut self.pdits,
            "BSIM4pditsd" => &mut self.pditsd,
            "BSIM4pditsl" => &mut self.pditsl,
            "BSIM4pscbe1" => &mut self.pscbe1,
            "BSIM4pscbe2" => &mut self.pscbe2,
            "BSIM4pvag" => &mut self.pvag,
            "BSIM4wr" => &mut self.wr,
            "BSIM4dwg" => &mut self.dwg,
            "BSIM4dwb" => &mut self.dwb,
            "BSIM4b0" => &mut self.b0,
            "BSIM4b1" => &mut self.b1,
            "BSIM4alpha0" => &mut self.alpha0,
            "BSIM4alpha1" => &mut self.alpha1,
            "BSIM4beta0" => &mut self.beta0,
            "BSIM4agidl" => &mut self.agidl,
            "BSIM4bgidl" => &mut self.bgidl,
            "BSIM4cgidl" => &mut self.cgidl,
            "BSIM4egidl" => &mut self.egidl,
            "BSIM4fgidl" => &mut self.fgidl,
            "BSIM4kgidl" => &mut self.kgidl,
            "BSIM4rgidl" => &mut self.rgidl,
            "BSIM4agisl" => &mut self.agisl,
            "BSIM4bgisl" => &mut self.bgisl,
            "BSIM4cgisl" => &mut self.cgisl,
            "BSIM4egisl" => &mut self.egisl,
            "BSIM4fgisl" => &mut self.fgisl,
            "BSIM4kgisl" => &mut self.kgisl,
            "BSIM4rgisl" => &mut self.rgisl,
            "BSIM4aigc" => &mut self.aigc,
            "BSIM4bigc" => &mut self.bigc,
            "BSIM4cigc" => &mut self.cigc,
            "BSIM4aigsd" => &mut self.aigsd,
            "BSIM4bigsd" => &mut self.bigsd,
            "BSIM4cigsd" => &mut self.cigsd,
            "BSIM4aigs" => &mut self.aigs,
            "BSIM4bigs" => &mut self.bigs,
            "BSIM4cigs" => &mut self.cigs,
            "BSIM4aigd" => &mut self.aigd,
            "BSIM4bigd" => &mut self.bigd,
            "BSIM4cigd" => &mut self.cigd,
            "BSIM4aigbacc" => &mut self.aigbacc,
            "BSIM4bigbacc" => &mut self.bigbacc,
            "BSIM4cigbacc" => &mut self.cigbacc,
            "BSIM4aigbinv" => &mut self.aigbinv,
            "BSIM4bigbinv" => &mut self.bigbinv,
            "BSIM4cigbinv" => &mut self.cigbinv,
            "BSIM4nigc" => &mut self.nigc,
            "BSIM4nigbacc" => &mut self.nigbacc,
            "BSIM4nigbinv" => &mut self.nigbinv,
            "BSIM4ntox" => &mut self.ntox,
            "BSIM4eigbinv" => &mut self.eigbinv,
            "BSIM4pigcd" => &mut self.pigcd,
            "BSIM4poxedge" => &mut self.poxedge,
            "BSIM4toxref" => &mut self.toxref,
            "BSIM4ijthdfwd" => &mut self.ijthdfwd,
            "BSIM4ijthsfwd" => &mut self.ijthsfwd,
            "BSIM4ijthdrev" => &mut self.ijthdrev,
            "BSIM4ijthsrev" => &mut self.ijthsrev,
            "BSIM4xjbvd" => &mut self.xjbvd,
            "BSIM4xjbvs" => &mut self.xjbvs,
            "BSIM4bvd" => &mut self.bvd,
            "BSIM4bvs" => &mut self.bvs,
            "BSIM4jtss" => &mut self.jtss,
            "BSIM4jtsd" => &mut self.jtsd,
            "BSIM4jtssws" => &mut self.jtssws,
            "BSIM4jtsswd" => &mut self.jtsswd,
            "BSIM4jtsswgs" => &mut self.jtsswgs,
            "BSIM4jtsswgd" => &mut self.jtsswgd,
            "BSIM4jtweff" => &mut self.jtweff,
            "BSIM4njts" => &mut self.njts,
            "BSIM4njtssw" => &mut self.njtssw,
            "BSIM4njtsswg" => &mut self.njtsswg,
            "BSIM4njtsd" => &mut self.njtsd,
            "BSIM4njtsswd" => &mut self.njtsswd,
            "BSIM4njtsswgd" => &mut self.njtsswgd,
            "BSIM4xtss" => &mut self.xtss,
            "BSIM4xtsd" => &mut self.xtsd,
            "BSIM4xtssws" => &mut self.xtssws,
            "BSIM4xtsswd" => &mut self.xtsswd,
            "BSIM4xtsswgs" => &mut self.xtsswgs,
            "BSIM4xtsswgd" => &mut self.xtsswgd,
            "BSIM4tnjts" => &mut self.tnjts,
            "BSIM4tnjtssw" => &mut self.tnjtssw,
            "BSIM4tnjtsswg" => &mut self.tnjtsswg,
            "BSIM4tnjtsd" => &mut self.tnjtsd,
            "BSIM4tnjtsswd" => &mut self.tnjtsswd,
            "BSIM4tnjtsswgd" => &mut self.tnjtsswgd,
            "BSIM4vtss" => &mut self.vtss,
            "BSIM4vtsd" => &mut self.vtsd,
            "BSIM4vtssws" => &mut self.vtssws,
            "BSIM4vtsswd" => &mut self.vtsswd,
            "BSIM4vtsswgs" => &mut self.vtsswgs,
            "BSIM4vtsswgd" => &mut self.vtsswgd,
            "BSIM4xrcrg1" => &mut self.xrcrg1,
            "BSIM4xrcrg2" => &mut self.xrcrg2,
            "BSIM4lambda" => &mut self.lambda,
            "BSIM4vtl" => &mut self.vtl,
            "BSIM4lc" => &mut self.lc,
            "BSIM4xn" => &mut self.xn,
            "BSIM4vfbsdoff" => &mut self.vfbsdoff,
            "BSIM4lintnoi" => &mut self.lintnoi,
            "BSIM4tvfbsdoff" => &mut self.tvfbsdoff,
            "BSIM4vfb" => &mut self.vfb,
            "BSIM4gbmin" => &mut self.gbmin,
            "BSIM4rbdb" => &mut self.rbdb,
            "BSIM4rbsb" => &mut self.rbsb,
            "BSIM4rbpb" => &mut self.rbpb,
            "BSIM4rbps" => &mut self.rbps,
            "BSIM4rbpd" => &mut self.rbpd,
            "BSIM4rbps0" => &mut self.rbps0,
            "BSIM4rbpsl" => &mut self.rbpsl,
            "BSIM4rbpsw" => &mut self.rbpsw,
            "BSIM4rbpsnf" => &mut self.rbpsnf,
            "BSIM4rbpd0" => &mut self.rbpd0,
            "BSIM4rbpdl" => &mut self.rbpdl,
            "BSIM4rbpdw" => &mut self.rbpdw,
            "BSIM4rbpdnf" => &mut self.rbpdnf,
            "BSIM4rbpbx0" => &mut self.rbpbx0,
            "BSIM4rbpbxl" => &mut self.rbpbxl,
            "BSIM4rbpbxw" => &mut self.rbpbxw,
            "BSIM4rbpbxnf" => &mut self.rbpbxnf,
            "BSIM4rbpby0" => &mut self.rbpby0,
            "BSIM4rbpbyl" => &mut self.rbpbyl,
            "BSIM4rbpbyw" => &mut self.rbpbyw,
            "BSIM4rbpbynf" => &mut self.rbpbynf,
            "BSIM4rbsbx0" => &mut self.rbsbx0,
            "BSIM4rbsby0" => &mut self.rbsby0,
            "BSIM4rbdbx0" => &mut self.rbdbx0,
            "BSIM4rbdby0" => &mut self.rbdby0,
            "BSIM4rbsdbxl" => &mut self.rbsdbxl,
            "BSIM4rbsdbxw" => &mut self.rbsdbxw,
            "BSIM4rbsdbxnf" => &mut self.rbsdbxnf,
            "BSIM4rbsdbyl" => &mut self.rbsdbyl,
            "BSIM4rbsdbyw" => &mut self.rbsdbyw,
            "BSIM4rbsdbynf" => &mut self.rbsdbynf,
            "BSIM4tnoia" => &mut self.tnoia,
            "BSIM4tnoib" => &mut self.tnoib,
            "BSIM4tnoic" => &mut self.tnoic,
            "BSIM4rnoia" => &mut self.rnoia,
            "BSIM4rnoib" => &mut self.rnoib,
            "BSIM4rnoic" => &mut self.rnoic,
            "BSIM4ntnoi" => &mut self.ntnoi,
            "BSIM4cgsl" => &mut self.cgsl,
            "BSIM4cgdl" => &mut self.cgdl,
            "BSIM4ckappas" => &mut self.ckappas,
            "BSIM4ckappad" => &mut self.ckappad,
            "BSIM4cf" => &mut self.cf,
            "BSIM4vfbcv" => &mut self.vfbcv,
            "BSIM4clc" => &mut self.clc,
            "BSIM4cle" => &mut self.cle,
            "BSIM4dwc" => &mut self.dwc,
            "BSIM4dlc" => &mut self.dlc,
            "BSIM4xw" => &mut self.xw,
            "BSIM4xl" => &mut self.xl,
            "BSIM4dlcig" => &mut self.dlcig,
            "BSIM4dlcigd" => &mut self.dlcigd,
            "BSIM4dwj" => &mut self.dwj,
            "BSIM4noff" => &mut self.noff,
            "BSIM4voffcv" => &mut self.voffcv,
            "BSIM4acde" => &mut self.acde,
            "BSIM4moin" => &mut self.moin,
            "BSIM4tcj" => &mut self.tcj,
            "BSIM4tcjsw" => &mut self.tcjsw,
            "BSIM4tcjswg" => &mut self.tcjswg,
            "BSIM4tpb" => &mut self.tpb,
            "BSIM4tpbsw" => &mut self.tpbsw,
            "BSIM4tpbswg" => &mut self.tpbswg,
            "BSIM4dmcg" => &mut self.dmcg,
            "BSIM4dmci" => &mut self.dmci,
            "BSIM4dmdg" => &mut self.dmdg,
            "BSIM4dmcgt" => &mut self.dmcgt,
            "BSIM4xgw" => &mut self.xgw,
            "BSIM4xgl" => &mut self.xgl,
            "BSIM4rshg" => &mut self.rshg,
            "BSIM4ngcon" => &mut self.ngcon,
            "BSIM4lcdsc" => &mut self.lcdsc,
            "BSIM4lcdscb" => &mut self.lcdscb,
            "BSIM4lcdscd" => &mut self.lcdscd,
            "BSIM4lcit" => &mut self.lcit,
            "BSIM4lnfactor" => &mut self.lnfactor,
            "BSIM4lxj" => &mut self.lxj,
            "BSIM4lvsat" => &mut self.lvsat,
            "BSIM4lat" => &mut self.lat,
            "BSIM4la0" => &mut self.la0,
            "BSIM4lags" => &mut self.lags,
            "BSIM4la1" => &mut self.la1,
            "BSIM4la2" => &mut self.la2,
            "BSIM4lketa" => &mut self.lketa,
            "BSIM4lketac" => &mut self.lketac,
            "BSIM4lnsub" => &mut self.lnsub,
            "BSIM4lndep" => &mut self.lndep,
            "BSIM4lnsd" => &mut self.lnsd,
            "BSIM4lphin" => &mut self.lphin,
            "BSIM4lngate" => &mut self.lngate,
            "BSIM4lgamma1" => &mut self.lgamma1,
            "BSIM4lgamma2" => &mut self.lgamma2,
            "BSIM4lvbx" => &mut self.lvbx,
            "BSIM4lvbm" => &mut self.lvbm,
            "BSIM4lxt" => &mut self.lxt,
            "BSIM4lk1" => &mut self.lk1,
            "BSIM4lkt1" => &mut self.lkt1,
            "BSIM4lkt1l" => &mut self.lkt1l,
            "BSIM4lkt2" => &mut self.lkt2,
            "BSIM4lk2" => &mut self.lk2,
            "BSIM4lk3" => &mut self.lk3,
            "BSIM4lk3b" => &mut self.lk3b,
            "BSIM4lw0" => &mut self.lw0,
            "BSIM4ldvtp0" => &mut self.ldvtp0,
            "BSIM4ldvtp1" => &mut self.ldvtp1,
            "BSIM4ldvtp2" => &mut self.ldvtp2,
            "BSIM4ldvtp3" => &mut self.ldvtp3,
            "BSIM4ldvtp4" => &mut self.ldvtp4,
            "BSIM4ldvtp5" => &mut self.ldvtp5,
            "BSIM4llpe0" => &mut self.llpe0,
            "BSIM4llpeb" => &mut self.llpeb,
            "BSIM4ldvt0" => &mut self.ldvt0,
            "BSIM4ldvt1" => &mut self.ldvt1,
            "BSIM4ldvt2" => &mut self.ldvt2,
            "BSIM4ldvt0w" => &mut self.ldvt0w,
            "BSIM4ldvt1w" => &mut self.ldvt1w,
            "BSIM4ldvt2w" => &mut self.ldvt2w,
            "BSIM4ldrout" => &mut self.ldrout,
            "BSIM4ldsub" => &mut self.ldsub,
            "BSIM4lvth0" => &mut self.lvth0,
            "BSIM4lua" => &mut self.lua,
            "BSIM4lua1" => &mut self.lua1,
            "BSIM4lub" => &mut self.lub,
            "BSIM4lub1" => &mut self.lub1,
            "BSIM4luc" => &mut self.luc,
            "BSIM4luc1" => &mut self.luc1,
            "BSIM4lud" => &mut self.lud,
            "BSIM4lud1" => &mut self.lud1,
            "BSIM4lup" => &mut self.lup,
            "BSIM4llp" => &mut self.llp,
            "BSIM4lu0" => &mut self.lu0,
            "BSIM4leu" => &mut self.leu,
            "BSIM4lucs" => &mut self.lucs,
            "BSIM4lute" => &mut self.lute,
            "BSIM4lucste" => &mut self.lucste,
            "BSIM4lvoff" => &mut self.lvoff,
            "BSIM4ltvoff" => &mut self.ltvoff,
            "BSIM4ltnfactor" => &mut self.ltnfactor,
            "BSIM4lteta0" => &mut self.lteta0,
            "BSIM4ltvoffcv" => &mut self.ltvoffcv,
            "BSIM4lminv" => &mut self.lminv,
            "BSIM4lminvcv" => &mut self.lminvcv,
            "BSIM4ldelta" => &mut self.ldelta,
            "BSIM4lrdsw" => &mut self.lrdsw,
            "BSIM4lrsw" => &mut self.lrsw,
            "BSIM4lrdw" => &mut self.lrdw,
            "BSIM4lprwg" => &mut self.lprwg,
            "BSIM4lprwb" => &mut self.lprwb,
            "BSIM4lprt" => &mut self.lprt,
            "BSIM4leta0" => &mut self.leta0,
            "BSIM4letab" => &mut self.letab,
            "BSIM4lpclm" => &mut self.lpclm,
            "BSIM4lpdibl1" => &mut self.lpdibl1,
            "BSIM4lpdibl2" => &mut self.lpdibl2,
            "BSIM4lpdiblb" => &mut self.lpdiblb,
            "BSIM4lfprout" => &mut self.lfprout,
            "BSIM4lpdits" => &mut self.lpdits,
            "BSIM4lpditsd" => &mut self.lpditsd,
            "BSIM4lpscbe1" => &mut self.lpscbe1,
            "BSIM4lpscbe2" => &mut self.lpscbe2,
            "BSIM4lpvag" => &mut self.lpvag,
            "BSIM4lwr" => &mut self.lwr,
            "BSIM4ldwg" => &mut self.ldwg,
            "BSIM4ldwb" => &mut self.ldwb,
            "BSIM4lb0" => &mut self.lb0,
            "BSIM4lb1" => &mut self.lb1,
            "BSIM4lalpha0" => &mut self.lalpha0,
            "BSIM4lalpha1" => &mut self.lalpha1,
            "BSIM4lbeta0" => &mut self.lbeta0,
            "BSIM4lvfb" => &mut self.lvfb,
            "BSIM4lagidl" => &mut self.lagidl,
            "BSIM4lbgidl" => &mut self.lbgidl,
            "BSIM4lcgidl" => &mut self.lcgidl,
            "BSIM4legidl" => &mut self.legidl,
            "BSIM4lfgidl" => &mut self.lfgidl,
            "BSIM4lkgidl" => &mut self.lkgidl,
            "BSIM4lrgidl" => &mut self.lrgidl,
            "BSIM4lagisl" => &mut self.lagisl,
            "BSIM4lbgisl" => &mut self.lbgisl,
            "BSIM4lcgisl" => &mut self.lcgisl,
            "BSIM4legisl" => &mut self.legisl,
            "BSIM4lfgisl" => &mut self.lfgisl,
            "BSIM4lkgisl" => &mut self.lkgisl,
            "BSIM4lrgisl" => &mut self.lrgisl,
            "BSIM4laigc" => &mut self.laigc,
            "BSIM4lbigc" => &mut self.lbigc,
            "BSIM4lcigc" => &mut self.lcigc,
            "BSIM4laigsd" => &mut self.laigsd,
            "BSIM4lbigsd" => &mut self.lbigsd,
            "BSIM4lcigsd" => &mut self.lcigsd,
            "BSIM4laigs" => &mut self.laigs,
            "BSIM4lbigs" => &mut self.lbigs,
            "BSIM4lcigs" => &mut self.lcigs,
            "BSIM4laigd" => &mut self.laigd,
            "BSIM4lbigd" => &mut self.lbigd,
            "BSIM4lcigd" => &mut self.lcigd,
            "BSIM4laigbacc" => &mut self.laigbacc,
            "BSIM4lbigbacc" => &mut self.lbigbacc,
            "BSIM4lcigbacc" => &mut self.lcigbacc,
            "BSIM4laigbinv" => &mut self.laigbinv,
            "BSIM4lbigbinv" => &mut self.lbigbinv,
            "BSIM4lcigbinv" => &mut self.lcigbinv,
            "BSIM4lnigc" => &mut self.lnigc,
            "BSIM4lnigbacc" => &mut self.lnigbacc,
            "BSIM4lnigbinv" => &mut self.lnigbinv,
            "BSIM4lntox" => &mut self.lntox,
            "BSIM4leigbinv" => &mut self.leigbinv,
            "BSIM4lpigcd" => &mut self.lpigcd,
            "BSIM4lpoxedge" => &mut self.lpoxedge,
            "BSIM4lxrcrg1" => &mut self.lxrcrg1,
            "BSIM4lxrcrg2" => &mut self.lxrcrg2,
            "BSIM4llambda" => &mut self.llambda,
            "BSIM4lvtl" => &mut self.lvtl,
            "BSIM4lxn" => &mut self.lxn,
            "BSIM4lvfbsdoff" => &mut self.lvfbsdoff,
            "BSIM4ltvfbsdoff" => &mut self.ltvfbsdoff,
            "BSIM4lcgsl" => &mut self.lcgsl,
            "BSIM4lcgdl" => &mut self.lcgdl,
            "BSIM4lckappas" => &mut self.lckappas,
            "BSIM4lckappad" => &mut self.lckappad,
            "BSIM4lcf" => &mut self.lcf,
            "BSIM4lclc" => &mut self.lclc,
            "BSIM4lcle" => &mut self.lcle,
            "BSIM4lvfbcv" => &mut self.lvfbcv,
            "BSIM4lnoff" => &mut self.lnoff,
            "BSIM4lvoffcv" => &mut self.lvoffcv,
            "BSIM4lacde" => &mut self.lacde,
            "BSIM4lmoin" => &mut self.lmoin,
            "BSIM4wcdsc" => &mut self.wcdsc,
            "BSIM4wcdscb" => &mut self.wcdscb,
            "BSIM4wcdscd" => &mut self.wcdscd,
            "BSIM4wcit" => &mut self.wcit,
            "BSIM4wnfactor" => &mut self.wnfactor,
            "BSIM4wxj" => &mut self.wxj,
            "BSIM4wvsat" => &mut self.wvsat,
            "BSIM4wat" => &mut self.wat,
            "BSIM4wa0" => &mut self.wa0,
            "BSIM4wags" => &mut self.wags,
            "BSIM4wa1" => &mut self.wa1,
            "BSIM4wa2" => &mut self.wa2,
            "BSIM4wketa" => &mut self.wketa,
            "BSIM4wketac" => &mut self.wketac,
            "BSIM4wnsub" => &mut self.wnsub,
            "BSIM4wndep" => &mut self.wndep,
            "BSIM4wnsd" => &mut self.wnsd,
            "BSIM4wphin" => &mut self.wphin,
            "BSIM4wngate" => &mut self.wngate,
            "BSIM4wgamma1" => &mut self.wgamma1,
            "BSIM4wgamma2" => &mut self.wgamma2,
            "BSIM4wvbx" => &mut self.wvbx,
            "BSIM4wvbm" => &mut self.wvbm,
            "BSIM4wxt" => &mut self.wxt,
            "BSIM4wk1" => &mut self.wk1,
            "BSIM4wkt1" => &mut self.wkt1,
            "BSIM4wkt1l" => &mut self.wkt1l,
            "BSIM4wkt2" => &mut self.wkt2,
            "BSIM4wk2" => &mut self.wk2,
            "BSIM4wk3" => &mut self.wk3,
            "BSIM4wk3b" => &mut self.wk3b,
            "BSIM4ww0" => &mut self.ww0,
            "BSIM4wdvtp0" => &mut self.wdvtp0,
            "BSIM4wdvtp1" => &mut self.wdvtp1,
            "BSIM4wdvtp2" => &mut self.wdvtp2,
            "BSIM4wdvtp3" => &mut self.wdvtp3,
            "BSIM4wdvtp4" => &mut self.wdvtp4,
            "BSIM4wdvtp5" => &mut self.wdvtp5,
            "BSIM4wlpe0" => &mut self.wlpe0,
            "BSIM4wlpeb" => &mut self.wlpeb,
            "BSIM4wdvt0" => &mut self.wdvt0,
            "BSIM4wdvt1" => &mut self.wdvt1,
            "BSIM4wdvt2" => &mut self.wdvt2,
            "BSIM4wdvt0w" => &mut self.wdvt0w,
            "BSIM4wdvt1w" => &mut self.wdvt1w,
            "BSIM4wdvt2w" => &mut self.wdvt2w,
            "BSIM4wdrout" => &mut self.wdrout,
            "BSIM4wdsub" => &mut self.wdsub,
            "BSIM4wvth0" => &mut self.wvth0,
            "BSIM4wua" => &mut self.wua,
            "BSIM4wua1" => &mut self.wua1,
            "BSIM4wub" => &mut self.wub,
            "BSIM4wub1" => &mut self.wub1,
            "BSIM4wuc" => &mut self.wuc,
            "BSIM4wuc1" => &mut self.wuc1,
            "BSIM4wud" => &mut self.wud,
            "BSIM4wud1" => &mut self.wud1,
            "BSIM4wup" => &mut self.wup,
            "BSIM4wlp" => &mut self.wlp,
            "BSIM4wu0" => &mut self.wu0,
            "BSIM4weu" => &mut self.weu,
            "BSIM4wucs" => &mut self.wucs,
            "BSIM4wute" => &mut self.wute,
            "BSIM4wucste" => &mut self.wucste,
            "BSIM4wvoff" => &mut self.wvoff,
            "BSIM4wtvoff" => &mut self.wtvoff,
            "BSIM4wtnfactor" => &mut self.wtnfactor,
            "BSIM4wteta0" => &mut self.wteta0,
            "BSIM4wtvoffcv" => &mut self.wtvoffcv,
            "BSIM4wminv" => &mut self.wminv,
            "BSIM4wminvcv" => &mut self.wminvcv,
            "BSIM4wdelta" => &mut self.wdelta,
            "BSIM4wrdsw" => &mut self.wrdsw,
            "BSIM4wrsw" => &mut self.wrsw,
            "BSIM4wrdw" => &mut self.wrdw,
            "BSIM4wprwg" => &mut self.wprwg,
            "BSIM4wprwb" => &mut self.wprwb,
            "BSIM4wprt" => &mut self.wprt,
            "BSIM4weta0" => &mut self.weta0,
            "BSIM4wetab" => &mut self.wetab,
            "BSIM4wpclm" => &mut self.wpclm,
            "BSIM4wpdibl1" => &mut self.wpdibl1,
            "BSIM4wpdibl2" => &mut self.wpdibl2,
            "BSIM4wpdiblb" => &mut self.wpdiblb,
            "BSIM4wfprout" => &mut self.wfprout,
            "BSIM4wpdits" => &mut self.wpdits,
            "BSIM4wpditsd" => &mut self.wpditsd,
            "BSIM4wpscbe1" => &mut self.wpscbe1,
            "BSIM4wpscbe2" => &mut self.wpscbe2,
            "BSIM4wpvag" => &mut self.wpvag,
            "BSIM4wwr" => &mut self.wwr,
            "BSIM4wdwg" => &mut self.wdwg,
            "BSIM4wdwb" => &mut self.wdwb,
            "BSIM4wb0" => &mut self.wb0,
            "BSIM4wb1" => &mut self.wb1,
            "BSIM4walpha0" => &mut self.walpha0,
            "BSIM4walpha1" => &mut self.walpha1,
            "BSIM4wbeta0" => &mut self.wbeta0,
            "BSIM4wvfb" => &mut self.wvfb,
            "BSIM4wagidl" => &mut self.wagidl,
            "BSIM4wbgidl" => &mut self.wbgidl,
            "BSIM4wcgidl" => &mut self.wcgidl,
            "BSIM4wegidl" => &mut self.wegidl,
            "BSIM4wfgidl" => &mut self.wfgidl,
            "BSIM4wkgidl" => &mut self.wkgidl,
            "BSIM4wrgidl" => &mut self.wrgidl,
            "BSIM4wagisl" => &mut self.wagisl,
            "BSIM4wbgisl" => &mut self.wbgisl,
            "BSIM4wcgisl" => &mut self.wcgisl,
            "BSIM4wegisl" => &mut self.wegisl,
            "BSIM4wfgisl" => &mut self.wfgisl,
            "BSIM4wkgisl" => &mut self.wkgisl,
            "BSIM4wrgisl" => &mut self.wrgisl,
            "BSIM4waigc" => &mut self.waigc,
            "BSIM4wbigc" => &mut self.wbigc,
            "BSIM4wcigc" => &mut self.wcigc,
            "BSIM4waigsd" => &mut self.waigsd,
            "BSIM4wbigsd" => &mut self.wbigsd,
            "BSIM4wcigsd" => &mut self.wcigsd,
            "BSIM4waigs" => &mut self.waigs,
            "BSIM4wbigs" => &mut self.wbigs,
            "BSIM4wcigs" => &mut self.wcigs,
            "BSIM4waigd" => &mut self.waigd,
            "BSIM4wbigd" => &mut self.wbigd,
            "BSIM4wcigd" => &mut self.wcigd,
            "BSIM4waigbacc" => &mut self.waigbacc,
            "BSIM4wbigbacc" => &mut self.wbigbacc,
            "BSIM4wcigbacc" => &mut self.wcigbacc,
            "BSIM4waigbinv" => &mut self.waigbinv,
            "BSIM4wbigbinv" => &mut self.wbigbinv,
            "BSIM4wcigbinv" => &mut self.wcigbinv,
            "BSIM4wnigc" => &mut self.wnigc,
            "BSIM4wnigbacc" => &mut self.wnigbacc,
            "BSIM4wnigbinv" => &mut self.wnigbinv,
            "BSIM4wntox" => &mut self.wntox,
            "BSIM4weigbinv" => &mut self.weigbinv,
            "BSIM4wpigcd" => &mut self.wpigcd,
            "BSIM4wpoxedge" => &mut self.wpoxedge,
            "BSIM4wxrcrg1" => &mut self.wxrcrg1,
            "BSIM4wxrcrg2" => &mut self.wxrcrg2,
            "BSIM4wlambda" => &mut self.wlambda,
            "BSIM4wvtl" => &mut self.wvtl,
            "BSIM4wxn" => &mut self.wxn,
            "BSIM4wvfbsdoff" => &mut self.wvfbsdoff,
            "BSIM4wtvfbsdoff" => &mut self.wtvfbsdoff,
            "BSIM4wcgsl" => &mut self.wcgsl,
            "BSIM4wcgdl" => &mut self.wcgdl,
            "BSIM4wckappas" => &mut self.wckappas,
            "BSIM4wckappad" => &mut self.wckappad,
            "BSIM4wcf" => &mut self.wcf,
            "BSIM4wclc" => &mut self.wclc,
            "BSIM4wcle" => &mut self.wcle,
            "BSIM4wvfbcv" => &mut self.wvfbcv,
            "BSIM4wnoff" => &mut self.wnoff,
            "BSIM4wvoffcv" => &mut self.wvoffcv,
            "BSIM4wacde" => &mut self.wacde,
            "BSIM4wmoin" => &mut self.wmoin,
            "BSIM4pcdsc" => &mut self.pcdsc,
            "BSIM4pcdscb" => &mut self.pcdscb,
            "BSIM4pcdscd" => &mut self.pcdscd,
            "BSIM4pcit" => &mut self.pcit,
            "BSIM4pnfactor" => &mut self.pnfactor,
            "BSIM4pxj" => &mut self.pxj,
            "BSIM4pvsat" => &mut self.pvsat,
            "BSIM4pat" => &mut self.pat,
            "BSIM4pa0" => &mut self.pa0,
            "BSIM4pags" => &mut self.pags,
            "BSIM4pa1" => &mut self.pa1,
            "BSIM4pa2" => &mut self.pa2,
            "BSIM4pketa" => &mut self.pketa,
            "BSIM4pketac" => &mut self.pketac,
            "BSIM4pnsub" => &mut self.pnsub,
            "BSIM4pndep" => &mut self.pndep,
            "BSIM4pnsd" => &mut self.pnsd,
            "BSIM4pphin" => &mut self.pphin,
            "BSIM4pngate" => &mut self.pngate,
            "BSIM4pgamma1" => &mut self.pgamma1,
            "BSIM4pgamma2" => &mut self.pgamma2,
            "BSIM4pvbx" => &mut self.pvbx,
            "BSIM4pvbm" => &mut self.pvbm,
            "BSIM4pxt" => &mut self.pxt,
            "BSIM4pk1" => &mut self.pk1,
            "BSIM4pkt1" => &mut self.pkt1,
            "BSIM4pkt1l" => &mut self.pkt1l,
            "BSIM4pkt2" => &mut self.pkt2,
            "BSIM4pk2" => &mut self.pk2,
            "BSIM4pk3" => &mut self.pk3,
            "BSIM4pk3b" => &mut self.pk3b,
            "BSIM4pw0" => &mut self.pw0,
            "BSIM4pdvtp0" => &mut self.pdvtp0,
            "BSIM4pdvtp1" => &mut self.pdvtp1,
            "BSIM4pdvtp2" => &mut self.pdvtp2,
            "BSIM4pdvtp3" => &mut self.pdvtp3,
            "BSIM4pdvtp4" => &mut self.pdvtp4,
            "BSIM4pdvtp5" => &mut self.pdvtp5,
            "BSIM4plpe0" => &mut self.plpe0,
            "BSIM4plpeb" => &mut self.plpeb,
            "BSIM4pdvt0" => &mut self.pdvt0,
            "BSIM4pdvt1" => &mut self.pdvt1,
            "BSIM4pdvt2" => &mut self.pdvt2,
            "BSIM4pdvt0w" => &mut self.pdvt0w,
            "BSIM4pdvt1w" => &mut self.pdvt1w,
            "BSIM4pdvt2w" => &mut self.pdvt2w,
            "BSIM4pdrout" => &mut self.pdrout,
            "BSIM4pdsub" => &mut self.pdsub,
            "BSIM4pvth0" => &mut self.pvth0,
            "BSIM4pua" => &mut self.pua,
            "BSIM4pua1" => &mut self.pua1,
            "BSIM4pub" => &mut self.r#pub,
            "BSIM4pub1" => &mut self.pub1,
            "BSIM4puc" => &mut self.puc,
            "BSIM4puc1" => &mut self.puc1,
            "BSIM4pud" => &mut self.pud,
            "BSIM4pud1" => &mut self.pud1,
            "BSIM4pup" => &mut self.pup,
            "BSIM4plp" => &mut self.plp,
            "BSIM4pu0" => &mut self.pu0,
            "BSIM4peu" => &mut self.peu,
            "BSIM4pucs" => &mut self.pucs,
            "BSIM4pute" => &mut self.pute,
            "BSIM4pucste" => &mut self.pucste,
            "BSIM4pvoff" => &mut self.pvoff,
            "BSIM4ptvoff" => &mut self.ptvoff,
            "BSIM4ptnfactor" => &mut self.ptnfactor,
            "BSIM4pteta0" => &mut self.pteta0,
            "BSIM4ptvoffcv" => &mut self.ptvoffcv,
            "BSIM4pminv" => &mut self.pminv,
            "BSIM4pminvcv" => &mut self.pminvcv,
            "BSIM4pdelta" => &mut self.pdelta,
            "BSIM4prdsw" => &mut self.prdsw,
            "BSIM4prsw" => &mut self.prsw,
            "BSIM4prdw" => &mut self.prdw,
            "BSIM4pprwg" => &mut self.pprwg,
            "BSIM4pprwb" => &mut self.pprwb,
            "BSIM4pprt" => &mut self.pprt,
            "BSIM4peta0" => &mut self.peta0,
            "BSIM4petab" => &mut self.petab,
            "BSIM4ppclm" => &mut self.ppclm,
            "BSIM4ppdibl1" => &mut self.ppdibl1,
            "BSIM4ppdibl2" => &mut self.ppdibl2,
            "BSIM4ppdiblb" => &mut self.ppdiblb,
            "BSIM4pfprout" => &mut self.pfprout,
            "BSIM4ppdits" => &mut self.ppdits,
            "BSIM4ppditsd" => &mut self.ppditsd,
            "BSIM4ppscbe1" => &mut self.ppscbe1,
            "BSIM4ppscbe2" => &mut self.ppscbe2,
            "BSIM4ppvag" => &mut self.ppvag,
            "BSIM4pwr" => &mut self.pwr,
            "BSIM4pdwg" => &mut self.pdwg,
            "BSIM4pdwb" => &mut self.pdwb,
            "BSIM4pb0" => &mut self.pb0,
            "BSIM4pb1" => &mut self.pb1,
            "BSIM4palpha0" => &mut self.palpha0,
            "BSIM4palpha1" => &mut self.palpha1,
            "BSIM4pbeta0" => &mut self.pbeta0,
            "BSIM4pvfb" => &mut self.pvfb,
            "BSIM4pagidl" => &mut self.pagidl,
            "BSIM4pbgidl" => &mut self.pbgidl,
            "BSIM4pcgidl" => &mut self.pcgidl,
            "BSIM4pegidl" => &mut self.pegidl,
            "BSIM4pfgidl" => &mut self.pfgidl,
            "BSIM4pkgidl" => &mut self.pkgidl,
            "BSIM4prgidl" => &mut self.prgidl,
            "BSIM4pagisl" => &mut self.pagisl,
            "BSIM4pbgisl" => &mut self.pbgisl,
            "BSIM4pcgisl" => &mut self.pcgisl,
            "BSIM4pegisl" => &mut self.pegisl,
            "BSIM4pfgisl" => &mut self.pfgisl,
            "BSIM4pkgisl" => &mut self.pkgisl,
            "BSIM4prgisl" => &mut self.prgisl,
            "BSIM4paigc" => &mut self.paigc,
            "BSIM4pbigc" => &mut self.pbigc,
            "BSIM4pcigc" => &mut self.pcigc,
            "BSIM4paigsd" => &mut self.paigsd,
            "BSIM4pbigsd" => &mut self.pbigsd,
            "BSIM4pcigsd" => &mut self.pcigsd,
            "BSIM4paigs" => &mut self.paigs,
            "BSIM4pbigs" => &mut self.pbigs,
            "BSIM4pcigs" => &mut self.pcigs,
            "BSIM4paigd" => &mut self.paigd,
            "BSIM4pbigd" => &mut self.pbigd,
            "BSIM4pcigd" => &mut self.pcigd,
            "BSIM4paigbacc" => &mut self.paigbacc,
            "BSIM4pbigbacc" => &mut self.pbigbacc,
            "BSIM4pcigbacc" => &mut self.pcigbacc,
            "BSIM4paigbinv" => &mut self.paigbinv,
            "BSIM4pbigbinv" => &mut self.pbigbinv,
            "BSIM4pcigbinv" => &mut self.pcigbinv,
            "BSIM4pnigc" => &mut self.pnigc,
            "BSIM4pnigbacc" => &mut self.pnigbacc,
            "BSIM4pnigbinv" => &mut self.pnigbinv,
            "BSIM4pntox" => &mut self.pntox,
            "BSIM4peigbinv" => &mut self.peigbinv,
            "BSIM4ppigcd" => &mut self.ppigcd,
            "BSIM4ppoxedge" => &mut self.ppoxedge,
            "BSIM4pxrcrg1" => &mut self.pxrcrg1,
            "BSIM4pxrcrg2" => &mut self.pxrcrg2,
            "BSIM4plambda" => &mut self.plambda,
            "BSIM4pvtl" => &mut self.pvtl,
            "BSIM4pxn" => &mut self.pxn,
            "BSIM4pvfbsdoff" => &mut self.pvfbsdoff,
            "BSIM4ptvfbsdoff" => &mut self.ptvfbsdoff,
            "BSIM4pcgsl" => &mut self.pcgsl,
            "BSIM4pcgdl" => &mut self.pcgdl,
            "BSIM4pckappas" => &mut self.pckappas,
            "BSIM4pckappad" => &mut self.pckappad,
            "BSIM4pcf" => &mut self.pcf,
            "BSIM4pclc" => &mut self.pclc,
            "BSIM4pcle" => &mut self.pcle,
            "BSIM4pvfbcv" => &mut self.pvfbcv,
            "BSIM4pnoff" => &mut self.pnoff,
            "BSIM4pvoffcv" => &mut self.pvoffcv,
            "BSIM4pacde" => &mut self.pacde,
            "BSIM4pmoin" => &mut self.pmoin,
            "BSIM4tnom" => &mut self.tnom,
            "BSIM4cgso" => &mut self.cgso,
            "BSIM4cgdo" => &mut self.cgdo,
            "BSIM4cgbo" => &mut self.cgbo,
            "BSIM4xpart" => &mut self.xpart,
            "BSIM4cFringOut" => &mut self.cFringOut,
            "BSIM4cFringMax" => &mut self.cFringMax,
            "BSIM4sheetResistance" => &mut self.sheetResistance,
            "BSIM4SjctSatCurDensity" => &mut self.SjctSatCurDensity,
            "BSIM4DjctSatCurDensity" => &mut self.DjctSatCurDensity,
            "BSIM4SjctSidewallSatCurDensity" => &mut self.SjctSidewallSatCurDensity,
            "BSIM4DjctSidewallSatCurDensity" => &mut self.DjctSidewallSatCurDensity,
            "BSIM4SjctGateSidewallSatCurDensity" => &mut self.SjctGateSidewallSatCurDensity,
            "BSIM4DjctGateSidewallSatCurDensity" => &mut self.DjctGateSidewallSatCurDensity,
            "BSIM4SbulkJctPotential" => &mut self.SbulkJctPotential,
            "BSIM4DbulkJctPotential" => &mut self.DbulkJctPotential,
            "BSIM4SbulkJctBotGradingCoeff" => &mut self.SbulkJctBotGradingCoeff,
            "BSIM4DbulkJctBotGradingCoeff" => &mut self.DbulkJctBotGradingCoeff,
            "BSIM4SbulkJctSideGradingCoeff" => &mut self.SbulkJctSideGradingCoeff,
            "BSIM4DbulkJctSideGradingCoeff" => &mut self.DbulkJctSideGradingCoeff,
            "BSIM4SbulkJctGateSideGradingCoeff" => &mut self.SbulkJctGateSideGradingCoeff,
            "BSIM4DbulkJctGateSideGradingCoeff" => &mut self.DbulkJctGateSideGradingCoeff,
            "BSIM4SsidewallJctPotential" => &mut self.SsidewallJctPotential,
            "BSIM4DsidewallJctPotential" => &mut self.DsidewallJctPotential,
            "BSIM4SGatesidewallJctPotential" => &mut self.SGatesidewallJctPotential,
            "BSIM4DGatesidewallJctPotential" => &mut self.DGatesidewallJctPotential,
            "BSIM4SunitAreaJctCap" => &mut self.SunitAreaJctCap,
            "BSIM4DunitAreaJctCap" => &mut self.DunitAreaJctCap,
            "BSIM4SunitLengthSidewallJctCap" => &mut self.SunitLengthSidewallJctCap,
            "BSIM4DunitLengthSidewallJctCap" => &mut self.DunitLengthSidewallJctCap,
            "BSIM4SunitLengthGateSidewallJctCap" => &mut self.SunitLengthGateSidewallJctCap,
            "BSIM4DunitLengthGateSidewallJctCap" => &mut self.DunitLengthGateSidewallJctCap,
            "BSIM4SjctEmissionCoeff" => &mut self.SjctEmissionCoeff,
            "BSIM4DjctEmissionCoeff" => &mut self.DjctEmissionCoeff,
            "BSIM4SjctTempExponent" => &mut self.SjctTempExponent,
            "BSIM4DjctTempExponent" => &mut self.DjctTempExponent,
            "BSIM4njtsstemp" => &mut self.njtsstemp,
            "BSIM4njtsswstemp" => &mut self.njtsswstemp,
            "BSIM4njtsswgstemp" => &mut self.njtsswgstemp,
            "BSIM4njtsdtemp" => &mut self.njtsdtemp,
            "BSIM4njtsswdtemp" => &mut self.njtsswdtemp,
            "BSIM4njtsswgdtemp" => &mut self.njtsswgdtemp,
            "BSIM4Lint" => &mut self.Lint,
            "BSIM4Ll" => &mut self.Ll,
            "BSIM4Llc" => &mut self.Llc,
            "BSIM4Lln" => &mut self.Lln,
            "BSIM4Lw" => &mut self.Lw,
            "BSIM4Lwc" => &mut self.Lwc,
            "BSIM4Lwn" => &mut self.Lwn,
            "BSIM4Lwl" => &mut self.Lwl,
            "BSIM4Lwlc" => &mut self.Lwlc,
            "BSIM4Lmin" => &mut self.Lmin,
            "BSIM4Lmax" => &mut self.Lmax,
            "BSIM4Wint" => &mut self.Wint,
            "BSIM4Wl" => &mut self.Wl,
            "BSIM4Wlc" => &mut self.Wlc,
            "BSIM4Wln" => &mut self.Wln,
            "BSIM4Ww" => &mut self.Ww,
            "BSIM4Wwc" => &mut self.Wwc,
            "BSIM4Wwn" => &mut self.Wwn,
            "BSIM4Wwl" => &mut self.Wwl,
            "BSIM4Wwlc" => &mut self.Wwlc,
            "BSIM4Wmin" => &mut self.Wmin,
            "BSIM4Wmax" => &mut self.Wmax,
            "BSIM4saref" => &mut self.saref,
            "BSIM4sbref" => &mut self.sbref,
            "BSIM4wlod" => &mut self.wlod,
            "BSIM4ku0" => &mut self.ku0,
            "BSIM4kvsat" => &mut self.kvsat,
            "BSIM4kvth0" => &mut self.kvth0,
            "BSIM4tku0" => &mut self.tku0,
            "BSIM4llodku0" => &mut self.llodku0,
            "BSIM4wlodku0" => &mut self.wlodku0,
            "BSIM4llodvth" => &mut self.llodvth,
            "BSIM4wlodvth" => &mut self.wlodvth,
            "BSIM4lku0" => &mut self.lku0,
            "BSIM4wku0" => &mut self.wku0,
            "BSIM4pku0" => &mut self.pku0,
            "BSIM4lkvth0" => &mut self.lkvth0,
            "BSIM4wkvth0" => &mut self.wkvth0,
            "BSIM4pkvth0" => &mut self.pkvth0,
            "BSIM4stk2" => &mut self.stk2,
            "BSIM4lodk2" => &mut self.lodk2,
            "BSIM4steta0" => &mut self.steta0,
            "BSIM4lodeta0" => &mut self.lodeta0,
            "BSIM4web" => &mut self.web,
            "BSIM4wec" => &mut self.wec,
            "BSIM4kvth0we" => &mut self.kvth0we,
            "BSIM4k2we" => &mut self.k2we,
            "BSIM4ku0we" => &mut self.ku0we,
            "BSIM4scref" => &mut self.scref,
            "BSIM4wpemod" => &mut self.wpemod,
            "BSIM4lkvth0we" => &mut self.lkvth0we,
            "BSIM4lk2we" => &mut self.lk2we,
            "BSIM4lku0we" => &mut self.lku0we,
            "BSIM4wkvth0we" => &mut self.wkvth0we,
            "BSIM4wk2we" => &mut self.wk2we,
            "BSIM4wku0we" => &mut self.wku0we,
            "BSIM4pkvth0we" => &mut self.pkvth0we,
            "BSIM4pk2we" => &mut self.pk2we,
            "BSIM4pku0we" => &mut self.pku0we,
            "BSIM4Eg0" => &mut self.Eg0,
            "BSIM4vtm" => &mut self.vtm,
            "BSIM4vtm0" => &mut self.vtm0,
            "BSIM4coxe" => &mut self.coxe,
            "BSIM4coxp" => &mut self.coxp,
            "BSIM4cof1" => &mut self.cof1,
            "BSIM4cof2" => &mut self.cof2,
            "BSIM4cof3" => &mut self.cof3,
            "BSIM4cof4" => &mut self.cof4,
            "BSIM4vcrit" => &mut self.vcrit,
            "BSIM4factor1" => &mut self.factor1,
            "BSIM4PhiBS" => &mut self.PhiBS,
            "BSIM4PhiBSWS" => &mut self.PhiBSWS,
            "BSIM4PhiBSWGS" => &mut self.PhiBSWGS,
            "BSIM4SjctTempSatCurDensity" => &mut self.SjctTempSatCurDensity,
            "BSIM4SjctSidewallTempSatCurDensity" => &mut self.SjctSidewallTempSatCurDensity,
            "BSIM4SjctGateSidewallTempSatCurDensity" => &mut self.SjctGateSidewallTempSatCurDensity,
            "BSIM4PhiBD" => &mut self.PhiBD,
            "BSIM4PhiBSWD" => &mut self.PhiBSWD,
            "BSIM4PhiBSWGD" => &mut self.PhiBSWGD,
            "BSIM4DjctTempSatCurDensity" => &mut self.DjctTempSatCurDensity,
            "BSIM4DjctSidewallTempSatCurDensity" => &mut self.DjctSidewallTempSatCurDensity,
            "BSIM4DjctGateSidewallTempSatCurDensity" => &mut self.DjctGateSidewallTempSatCurDensity,
            "BSIM4SunitAreaTempJctCap" => &mut self.SunitAreaTempJctCap,
            "BSIM4DunitAreaTempJctCap" => &mut self.DunitAreaTempJctCap,
            "BSIM4SunitLengthSidewallTempJctCap" => &mut self.SunitLengthSidewallTempJctCap,
            "BSIM4DunitLengthSidewallTempJctCap" => &mut self.DunitLengthSidewallTempJctCap,
            "BSIM4SunitLengthGateSidewallTempJctCap" => &mut self.SunitLengthGateSidewallTempJctCap,
            "BSIM4DunitLengthGateSidewallTempJctCap" => &mut self.DunitLengthGateSidewallTempJctCap,
            "BSIM4oxideTrapDensityA" => &mut self.oxideTrapDensityA,
            "BSIM4oxideTrapDensityB" => &mut self.oxideTrapDensityB,
            "BSIM4oxideTrapDensityC" => &mut self.oxideTrapDensityC,
            "BSIM4em" => &mut self.em,
            "BSIM4ef" => &mut self.ef,
            "BSIM4af" => &mut self.af,
            "BSIM4kf" => &mut self.kf,
            "BSIM4vgsMax" => &mut self.vgsMax,
            "BSIM4vgdMax" => &mut self.vgdMax,
            "BSIM4vgbMax" => &mut self.vgbMax,
            "BSIM4vdsMax" => &mut self.vdsMax,
            "BSIM4vbsMax" => &mut self.vbsMax,
            "BSIM4vbdMax" => &mut self.vbdMax,
            "BSIM4vgsrMax" => &mut self.vgsrMax,
            "BSIM4vgdrMax" => &mut self.vgdrMax,
            "BSIM4vgbrMax" => &mut self.vgbrMax,
            "BSIM4vbsrMax" => &mut self.vbsrMax,
            "BSIM4vbdrMax" => &mut self.vbdrMax,
            "BSIM4gidlclamp" => &mut self.gidlclamp,
            "BSIM4idovvdsc" => &mut self.idovvdsc,
            _ => return None,
        })
    }
    /// The `i32` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn int_mut(&mut self, c_name: &str) -> Option<&mut i32> {
        Some(match c_name {
            "BSIM4type" => &mut self.r#type,
            "BSIM4mobMod" => &mut self.mobMod,
            "BSIM4cvchargeMod" => &mut self.cvchargeMod,
            "BSIM4capMod" => &mut self.capMod,
            "BSIM4dioMod" => &mut self.dioMod,
            "BSIM4trnqsMod" => &mut self.trnqsMod,
            "BSIM4acnqsMod" => &mut self.acnqsMod,
            "BSIM4fnoiMod" => &mut self.fnoiMod,
            "BSIM4tnoiMod" => &mut self.tnoiMod,
            "BSIM4rdsMod" => &mut self.rdsMod,
            "BSIM4rbodyMod" => &mut self.rbodyMod,
            "BSIM4rgateMod" => &mut self.rgateMod,
            "BSIM4perMod" => &mut self.perMod,
            "BSIM4geoMod" => &mut self.geoMod,
            "BSIM4mtrlMod" => &mut self.mtrlMod,
            "BSIM4mtrlCompatMod" => &mut self.mtrlCompatMod,
            "BSIM4gidlMod" => &mut self.gidlMod,
            "BSIM4igcMod" => &mut self.igcMod,
            "BSIM4igbMod" => &mut self.igbMod,
            "BSIM4tempMod" => &mut self.tempMod,
            "BSIM4binUnit" => &mut self.binUnit,
            "BSIM4paramChk" => &mut self.paramChk,
            "BSIM4v48intVersion" => &mut self.v48intVersion,
            "BSIM4InstCount" => &mut self.InstCount,
            _ => return None,
        })
    }
    /// The `bool` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn flag_mut(&mut self, c_name: &str) -> Option<&mut bool> {
        Some(match c_name {
            "BSIM4mobModGiven" => &mut self.mobModGiven,
            "BSIM4binUnitGiven" => &mut self.binUnitGiven,
            "BSIM4cvchargeModGiven" => &mut self.cvchargeModGiven,
            "BSIM4capModGiven" => &mut self.capModGiven,
            "BSIM4dioModGiven" => &mut self.dioModGiven,
            "BSIM4rdsModGiven" => &mut self.rdsModGiven,
            "BSIM4rbodyModGiven" => &mut self.rbodyModGiven,
            "BSIM4rgateModGiven" => &mut self.rgateModGiven,
            "BSIM4perModGiven" => &mut self.perModGiven,
            "BSIM4geoModGiven" => &mut self.geoModGiven,
            "BSIM4paramChkGiven" => &mut self.paramChkGiven,
            "BSIM4trnqsModGiven" => &mut self.trnqsModGiven,
            "BSIM4acnqsModGiven" => &mut self.acnqsModGiven,
            "BSIM4fnoiModGiven" => &mut self.fnoiModGiven,
            "BSIM4tnoiModGiven" => &mut self.tnoiModGiven,
            "BSIM4mtrlModGiven" => &mut self.mtrlModGiven,
            "BSIM4mtrlCompatModGiven" => &mut self.mtrlCompatModGiven,
            "BSIM4gidlModGiven" => &mut self.gidlModGiven,
            "BSIM4igcModGiven" => &mut self.igcModGiven,
            "BSIM4igbModGiven" => &mut self.igbModGiven,
            "BSIM4tempModGiven" => &mut self.tempModGiven,
            "BSIM4typeGiven" => &mut self.typeGiven,
            "BSIM4toxrefGiven" => &mut self.toxrefGiven,
            "BSIM4eotGiven" => &mut self.eotGiven,
            "BSIM4vddeotGiven" => &mut self.vddeotGiven,
            "BSIM4tempeotGiven" => &mut self.tempeotGiven,
            "BSIM4leffeotGiven" => &mut self.leffeotGiven,
            "BSIM4weffeotGiven" => &mut self.weffeotGiven,
            "BSIM4adosGiven" => &mut self.adosGiven,
            "BSIM4bdosGiven" => &mut self.bdosGiven,
            "BSIM4toxeGiven" => &mut self.toxeGiven,
            "BSIM4toxpGiven" => &mut self.toxpGiven,
            "BSIM4toxmGiven" => &mut self.toxmGiven,
            "BSIM4dtoxGiven" => &mut self.dtoxGiven,
            "BSIM4epsroxGiven" => &mut self.epsroxGiven,
            "BSIM4versionGiven" => &mut self.versionGiven,
            "BSIM4cdscGiven" => &mut self.cdscGiven,
            "BSIM4cdscbGiven" => &mut self.cdscbGiven,
            "BSIM4cdscdGiven" => &mut self.cdscdGiven,
            "BSIM4citGiven" => &mut self.citGiven,
            "BSIM4nfactorGiven" => &mut self.nfactorGiven,
            "BSIM4xjGiven" => &mut self.xjGiven,
            "BSIM4vsatGiven" => &mut self.vsatGiven,
            "BSIM4atGiven" => &mut self.atGiven,
            "BSIM4a0Given" => &mut self.a0Given,
            "BSIM4agsGiven" => &mut self.agsGiven,
            "BSIM4a1Given" => &mut self.a1Given,
            "BSIM4a2Given" => &mut self.a2Given,
            "BSIM4ketaGiven" => &mut self.ketaGiven,
            "BSIM4ketacGiven" => &mut self.ketacGiven,
            "BSIM4nsubGiven" => &mut self.nsubGiven,
            "BSIM4phigGiven" => &mut self.phigGiven,
            "BSIM4epsrgateGiven" => &mut self.epsrgateGiven,
            "BSIM4easubGiven" => &mut self.easubGiven,
            "BSIM4epsrsubGiven" => &mut self.epsrsubGiven,
            "BSIM4ni0subGiven" => &mut self.ni0subGiven,
            "BSIM4bg0subGiven" => &mut self.bg0subGiven,
            "BSIM4tbgasubGiven" => &mut self.tbgasubGiven,
            "BSIM4tbgbsubGiven" => &mut self.tbgbsubGiven,
            "BSIM4ndepGiven" => &mut self.ndepGiven,
            "BSIM4nsdGiven" => &mut self.nsdGiven,
            "BSIM4phinGiven" => &mut self.phinGiven,
            "BSIM4ngateGiven" => &mut self.ngateGiven,
            "BSIM4gamma1Given" => &mut self.gamma1Given,
            "BSIM4gamma2Given" => &mut self.gamma2Given,
            "BSIM4vbxGiven" => &mut self.vbxGiven,
            "BSIM4vbmGiven" => &mut self.vbmGiven,
            "BSIM4xtGiven" => &mut self.xtGiven,
            "BSIM4k1Given" => &mut self.k1Given,
            "BSIM4kt1Given" => &mut self.kt1Given,
            "BSIM4kt1lGiven" => &mut self.kt1lGiven,
            "BSIM4kt2Given" => &mut self.kt2Given,
            "BSIM4k2Given" => &mut self.k2Given,
            "BSIM4k3Given" => &mut self.k3Given,
            "BSIM4k3bGiven" => &mut self.k3bGiven,
            "BSIM4w0Given" => &mut self.w0Given,
            "BSIM4dvtp0Given" => &mut self.dvtp0Given,
            "BSIM4dvtp1Given" => &mut self.dvtp1Given,
            "BSIM4dvtp2Given" => &mut self.dvtp2Given,
            "BSIM4dvtp3Given" => &mut self.dvtp3Given,
            "BSIM4dvtp4Given" => &mut self.dvtp4Given,
            "BSIM4dvtp5Given" => &mut self.dvtp5Given,
            "BSIM4lpe0Given" => &mut self.lpe0Given,
            "BSIM4lpebGiven" => &mut self.lpebGiven,
            "BSIM4dvt0Given" => &mut self.dvt0Given,
            "BSIM4dvt1Given" => &mut self.dvt1Given,
            "BSIM4dvt2Given" => &mut self.dvt2Given,
            "BSIM4dvt0wGiven" => &mut self.dvt0wGiven,
            "BSIM4dvt1wGiven" => &mut self.dvt1wGiven,
            "BSIM4dvt2wGiven" => &mut self.dvt2wGiven,
            "BSIM4droutGiven" => &mut self.droutGiven,
            "BSIM4dsubGiven" => &mut self.dsubGiven,
            "BSIM4vth0Given" => &mut self.vth0Given,
            "BSIM4euGiven" => &mut self.euGiven,
            "BSIM4ucsGiven" => &mut self.ucsGiven,
            "BSIM4uaGiven" => &mut self.uaGiven,
            "BSIM4ua1Given" => &mut self.ua1Given,
            "BSIM4ubGiven" => &mut self.ubGiven,
            "BSIM4ub1Given" => &mut self.ub1Given,
            "BSIM4ucGiven" => &mut self.ucGiven,
            "BSIM4uc1Given" => &mut self.uc1Given,
            "BSIM4udGiven" => &mut self.udGiven,
            "BSIM4ud1Given" => &mut self.ud1Given,
            "BSIM4upGiven" => &mut self.upGiven,
            "BSIM4lpGiven" => &mut self.lpGiven,
            "BSIM4u0Given" => &mut self.u0Given,
            "BSIM4uteGiven" => &mut self.uteGiven,
            "BSIM4ucsteGiven" => &mut self.ucsteGiven,
            "BSIM4voffGiven" => &mut self.voffGiven,
            "BSIM4tvoffGiven" => &mut self.tvoffGiven,
            "BSIM4tnfactorGiven" => &mut self.tnfactorGiven,
            "BSIM4teta0Given" => &mut self.teta0Given,
            "BSIM4tvoffcvGiven" => &mut self.tvoffcvGiven,
            "BSIM4vofflGiven" => &mut self.vofflGiven,
            "BSIM4voffcvlGiven" => &mut self.voffcvlGiven,
            "BSIM4minvGiven" => &mut self.minvGiven,
            "BSIM4minvcvGiven" => &mut self.minvcvGiven,
            "BSIM4rdswGiven" => &mut self.rdswGiven,
            "BSIM4rdswminGiven" => &mut self.rdswminGiven,
            "BSIM4rdwminGiven" => &mut self.rdwminGiven,
            "BSIM4rswminGiven" => &mut self.rswminGiven,
            "BSIM4rswGiven" => &mut self.rswGiven,
            "BSIM4rdwGiven" => &mut self.rdwGiven,
            "BSIM4prwgGiven" => &mut self.prwgGiven,
            "BSIM4prwbGiven" => &mut self.prwbGiven,
            "BSIM4prtGiven" => &mut self.prtGiven,
            "BSIM4eta0Given" => &mut self.eta0Given,
            "BSIM4etabGiven" => &mut self.etabGiven,
            "BSIM4pclmGiven" => &mut self.pclmGiven,
            "BSIM4pdibl1Given" => &mut self.pdibl1Given,
            "BSIM4pdibl2Given" => &mut self.pdibl2Given,
            "BSIM4pdiblbGiven" => &mut self.pdiblbGiven,
            "BSIM4fproutGiven" => &mut self.fproutGiven,
            "BSIM4pditsGiven" => &mut self.pditsGiven,
            "BSIM4pditsdGiven" => &mut self.pditsdGiven,
            "BSIM4pditslGiven" => &mut self.pditslGiven,
            "BSIM4pscbe1Given" => &mut self.pscbe1Given,
            "BSIM4pscbe2Given" => &mut self.pscbe2Given,
            "BSIM4pvagGiven" => &mut self.pvagGiven,
            "BSIM4deltaGiven" => &mut self.deltaGiven,
            "BSIM4wrGiven" => &mut self.wrGiven,
            "BSIM4dwgGiven" => &mut self.dwgGiven,
            "BSIM4dwbGiven" => &mut self.dwbGiven,
            "BSIM4b0Given" => &mut self.b0Given,
            "BSIM4b1Given" => &mut self.b1Given,
            "BSIM4alpha0Given" => &mut self.alpha0Given,
            "BSIM4alpha1Given" => &mut self.alpha1Given,
            "BSIM4beta0Given" => &mut self.beta0Given,
            "BSIM4agidlGiven" => &mut self.agidlGiven,
            "BSIM4bgidlGiven" => &mut self.bgidlGiven,
            "BSIM4cgidlGiven" => &mut self.cgidlGiven,
            "BSIM4egidlGiven" => &mut self.egidlGiven,
            "BSIM4fgidlGiven" => &mut self.fgidlGiven,
            "BSIM4kgidlGiven" => &mut self.kgidlGiven,
            "BSIM4rgidlGiven" => &mut self.rgidlGiven,
            "BSIM4agislGiven" => &mut self.agislGiven,
            "BSIM4bgislGiven" => &mut self.bgislGiven,
            "BSIM4cgislGiven" => &mut self.cgislGiven,
            "BSIM4egislGiven" => &mut self.egislGiven,
            "BSIM4fgislGiven" => &mut self.fgislGiven,
            "BSIM4kgislGiven" => &mut self.kgislGiven,
            "BSIM4rgislGiven" => &mut self.rgislGiven,
            "BSIM4aigcGiven" => &mut self.aigcGiven,
            "BSIM4bigcGiven" => &mut self.bigcGiven,
            "BSIM4cigcGiven" => &mut self.cigcGiven,
            "BSIM4aigsdGiven" => &mut self.aigsdGiven,
            "BSIM4bigsdGiven" => &mut self.bigsdGiven,
            "BSIM4cigsdGiven" => &mut self.cigsdGiven,
            "BSIM4aigsGiven" => &mut self.aigsGiven,
            "BSIM4bigsGiven" => &mut self.bigsGiven,
            "BSIM4cigsGiven" => &mut self.cigsGiven,
            "BSIM4aigdGiven" => &mut self.aigdGiven,
            "BSIM4bigdGiven" => &mut self.bigdGiven,
            "BSIM4cigdGiven" => &mut self.cigdGiven,
            "BSIM4aigbaccGiven" => &mut self.aigbaccGiven,
            "BSIM4bigbaccGiven" => &mut self.bigbaccGiven,
            "BSIM4cigbaccGiven" => &mut self.cigbaccGiven,
            "BSIM4aigbinvGiven" => &mut self.aigbinvGiven,
            "BSIM4bigbinvGiven" => &mut self.bigbinvGiven,
            "BSIM4cigbinvGiven" => &mut self.cigbinvGiven,
            "BSIM4nigcGiven" => &mut self.nigcGiven,
            "BSIM4nigbinvGiven" => &mut self.nigbinvGiven,
            "BSIM4nigbaccGiven" => &mut self.nigbaccGiven,
            "BSIM4ntoxGiven" => &mut self.ntoxGiven,
            "BSIM4eigbinvGiven" => &mut self.eigbinvGiven,
            "BSIM4pigcdGiven" => &mut self.pigcdGiven,
            "BSIM4poxedgeGiven" => &mut self.poxedgeGiven,
            "BSIM4ijthdfwdGiven" => &mut self.ijthdfwdGiven,
            "BSIM4ijthsfwdGiven" => &mut self.ijthsfwdGiven,
            "BSIM4ijthdrevGiven" => &mut self.ijthdrevGiven,
            "BSIM4ijthsrevGiven" => &mut self.ijthsrevGiven,
            "BSIM4xjbvdGiven" => &mut self.xjbvdGiven,
            "BSIM4xjbvsGiven" => &mut self.xjbvsGiven,
            "BSIM4bvdGiven" => &mut self.bvdGiven,
            "BSIM4bvsGiven" => &mut self.bvsGiven,
            "BSIM4jtssGiven" => &mut self.jtssGiven,
            "BSIM4jtsdGiven" => &mut self.jtsdGiven,
            "BSIM4jtsswsGiven" => &mut self.jtsswsGiven,
            "BSIM4jtsswdGiven" => &mut self.jtsswdGiven,
            "BSIM4jtsswgsGiven" => &mut self.jtsswgsGiven,
            "BSIM4jtsswgdGiven" => &mut self.jtsswgdGiven,
            "BSIM4jtweffGiven" => &mut self.jtweffGiven,
            "BSIM4njtsGiven" => &mut self.njtsGiven,
            "BSIM4njtsswGiven" => &mut self.njtsswGiven,
            "BSIM4njtsswgGiven" => &mut self.njtsswgGiven,
            "BSIM4njtsdGiven" => &mut self.njtsdGiven,
            "BSIM4njtsswdGiven" => &mut self.njtsswdGiven,
            "BSIM4njtsswgdGiven" => &mut self.njtsswgdGiven,
            "BSIM4xtssGiven" => &mut self.xtssGiven,
            "BSIM4xtsdGiven" => &mut self.xtsdGiven,
            "BSIM4xtsswsGiven" => &mut self.xtsswsGiven,
            "BSIM4xtsswdGiven" => &mut self.xtsswdGiven,
            "BSIM4xtsswgsGiven" => &mut self.xtsswgsGiven,
            "BSIM4xtsswgdGiven" => &mut self.xtsswgdGiven,
            "BSIM4tnjtsGiven" => &mut self.tnjtsGiven,
            "BSIM4tnjtsswGiven" => &mut self.tnjtsswGiven,
            "BSIM4tnjtsswgGiven" => &mut self.tnjtsswgGiven,
            "BSIM4tnjtsdGiven" => &mut self.tnjtsdGiven,
            "BSIM4tnjtsswdGiven" => &mut self.tnjtsswdGiven,
            "BSIM4tnjtsswgdGiven" => &mut self.tnjtsswgdGiven,
            "BSIM4vtssGiven" => &mut self.vtssGiven,
            "BSIM4vtsdGiven" => &mut self.vtsdGiven,
            "BSIM4vtsswsGiven" => &mut self.vtsswsGiven,
            "BSIM4vtsswdGiven" => &mut self.vtsswdGiven,
            "BSIM4vtsswgsGiven" => &mut self.vtsswgsGiven,
            "BSIM4vtsswgdGiven" => &mut self.vtsswgdGiven,
            "BSIM4vfbGiven" => &mut self.vfbGiven,
            "BSIM4gbminGiven" => &mut self.gbminGiven,
            "BSIM4rbdbGiven" => &mut self.rbdbGiven,
            "BSIM4rbsbGiven" => &mut self.rbsbGiven,
            "BSIM4rbpsGiven" => &mut self.rbpsGiven,
            "BSIM4rbpdGiven" => &mut self.rbpdGiven,
            "BSIM4rbpbGiven" => &mut self.rbpbGiven,
            "BSIM4rbps0Given" => &mut self.rbps0Given,
            "BSIM4rbpslGiven" => &mut self.rbpslGiven,
            "BSIM4rbpswGiven" => &mut self.rbpswGiven,
            "BSIM4rbpsnfGiven" => &mut self.rbpsnfGiven,
            "BSIM4rbpd0Given" => &mut self.rbpd0Given,
            "BSIM4rbpdlGiven" => &mut self.rbpdlGiven,
            "BSIM4rbpdwGiven" => &mut self.rbpdwGiven,
            "BSIM4rbpdnfGiven" => &mut self.rbpdnfGiven,
            "BSIM4rbpbx0Given" => &mut self.rbpbx0Given,
            "BSIM4rbpbxlGiven" => &mut self.rbpbxlGiven,
            "BSIM4rbpbxwGiven" => &mut self.rbpbxwGiven,
            "BSIM4rbpbxnfGiven" => &mut self.rbpbxnfGiven,
            "BSIM4rbpby0Given" => &mut self.rbpby0Given,
            "BSIM4rbpbylGiven" => &mut self.rbpbylGiven,
            "BSIM4rbpbywGiven" => &mut self.rbpbywGiven,
            "BSIM4rbpbynfGiven" => &mut self.rbpbynfGiven,
            "BSIM4rbsbx0Given" => &mut self.rbsbx0Given,
            "BSIM4rbsby0Given" => &mut self.rbsby0Given,
            "BSIM4rbdbx0Given" => &mut self.rbdbx0Given,
            "BSIM4rbdby0Given" => &mut self.rbdby0Given,
            "BSIM4rbsdbxlGiven" => &mut self.rbsdbxlGiven,
            "BSIM4rbsdbxwGiven" => &mut self.rbsdbxwGiven,
            "BSIM4rbsdbxnfGiven" => &mut self.rbsdbxnfGiven,
            "BSIM4rbsdbylGiven" => &mut self.rbsdbylGiven,
            "BSIM4rbsdbywGiven" => &mut self.rbsdbywGiven,
            "BSIM4rbsdbynfGiven" => &mut self.rbsdbynfGiven,
            "BSIM4xrcrg1Given" => &mut self.xrcrg1Given,
            "BSIM4xrcrg2Given" => &mut self.xrcrg2Given,
            "BSIM4tnoiaGiven" => &mut self.tnoiaGiven,
            "BSIM4tnoibGiven" => &mut self.tnoibGiven,
            "BSIM4tnoicGiven" => &mut self.tnoicGiven,
            "BSIM4rnoiaGiven" => &mut self.rnoiaGiven,
            "BSIM4rnoibGiven" => &mut self.rnoibGiven,
            "BSIM4rnoicGiven" => &mut self.rnoicGiven,
            "BSIM4ntnoiGiven" => &mut self.ntnoiGiven,
            "BSIM4lambdaGiven" => &mut self.lambdaGiven,
            "BSIM4vtlGiven" => &mut self.vtlGiven,
            "BSIM4lcGiven" => &mut self.lcGiven,
            "BSIM4xnGiven" => &mut self.xnGiven,
            "BSIM4vfbsdoffGiven" => &mut self.vfbsdoffGiven,
            "BSIM4lintnoiGiven" => &mut self.lintnoiGiven,
            "BSIM4tvfbsdoffGiven" => &mut self.tvfbsdoffGiven,
            "BSIM4cgslGiven" => &mut self.cgslGiven,
            "BSIM4cgdlGiven" => &mut self.cgdlGiven,
            "BSIM4ckappasGiven" => &mut self.ckappasGiven,
            "BSIM4ckappadGiven" => &mut self.ckappadGiven,
            "BSIM4cfGiven" => &mut self.cfGiven,
            "BSIM4vfbcvGiven" => &mut self.vfbcvGiven,
            "BSIM4clcGiven" => &mut self.clcGiven,
            "BSIM4cleGiven" => &mut self.cleGiven,
            "BSIM4dwcGiven" => &mut self.dwcGiven,
            "BSIM4dlcGiven" => &mut self.dlcGiven,
            "BSIM4xwGiven" => &mut self.xwGiven,
            "BSIM4xlGiven" => &mut self.xlGiven,
            "BSIM4dlcigGiven" => &mut self.dlcigGiven,
            "BSIM4dlcigdGiven" => &mut self.dlcigdGiven,
            "BSIM4dwjGiven" => &mut self.dwjGiven,
            "BSIM4noffGiven" => &mut self.noffGiven,
            "BSIM4voffcvGiven" => &mut self.voffcvGiven,
            "BSIM4acdeGiven" => &mut self.acdeGiven,
            "BSIM4moinGiven" => &mut self.moinGiven,
            "BSIM4tcjGiven" => &mut self.tcjGiven,
            "BSIM4tcjswGiven" => &mut self.tcjswGiven,
            "BSIM4tcjswgGiven" => &mut self.tcjswgGiven,
            "BSIM4tpbGiven" => &mut self.tpbGiven,
            "BSIM4tpbswGiven" => &mut self.tpbswGiven,
            "BSIM4tpbswgGiven" => &mut self.tpbswgGiven,
            "BSIM4dmcgGiven" => &mut self.dmcgGiven,
            "BSIM4dmciGiven" => &mut self.dmciGiven,
            "BSIM4dmdgGiven" => &mut self.dmdgGiven,
            "BSIM4dmcgtGiven" => &mut self.dmcgtGiven,
            "BSIM4xgwGiven" => &mut self.xgwGiven,
            "BSIM4xglGiven" => &mut self.xglGiven,
            "BSIM4rshgGiven" => &mut self.rshgGiven,
            "BSIM4ngconGiven" => &mut self.ngconGiven,
            "BSIM4lcdscGiven" => &mut self.lcdscGiven,
            "BSIM4lcdscbGiven" => &mut self.lcdscbGiven,
            "BSIM4lcdscdGiven" => &mut self.lcdscdGiven,
            "BSIM4lcitGiven" => &mut self.lcitGiven,
            "BSIM4lnfactorGiven" => &mut self.lnfactorGiven,
            "BSIM4lxjGiven" => &mut self.lxjGiven,
            "BSIM4lvsatGiven" => &mut self.lvsatGiven,
            "BSIM4latGiven" => &mut self.latGiven,
            "BSIM4la0Given" => &mut self.la0Given,
            "BSIM4lagsGiven" => &mut self.lagsGiven,
            "BSIM4la1Given" => &mut self.la1Given,
            "BSIM4la2Given" => &mut self.la2Given,
            "BSIM4lketaGiven" => &mut self.lketaGiven,
            "BSIM4lketacGiven" => &mut self.lketacGiven,
            "BSIM4lnsubGiven" => &mut self.lnsubGiven,
            "BSIM4lndepGiven" => &mut self.lndepGiven,
            "BSIM4lnsdGiven" => &mut self.lnsdGiven,
            "BSIM4lphinGiven" => &mut self.lphinGiven,
            "BSIM4lngateGiven" => &mut self.lngateGiven,
            "BSIM4lgamma1Given" => &mut self.lgamma1Given,
            "BSIM4lgamma2Given" => &mut self.lgamma2Given,
            "BSIM4lvbxGiven" => &mut self.lvbxGiven,
            "BSIM4lvbmGiven" => &mut self.lvbmGiven,
            "BSIM4lxtGiven" => &mut self.lxtGiven,
            "BSIM4lk1Given" => &mut self.lk1Given,
            "BSIM4lkt1Given" => &mut self.lkt1Given,
            "BSIM4lkt1lGiven" => &mut self.lkt1lGiven,
            "BSIM4lkt2Given" => &mut self.lkt2Given,
            "BSIM4lk2Given" => &mut self.lk2Given,
            "BSIM4lk3Given" => &mut self.lk3Given,
            "BSIM4lk3bGiven" => &mut self.lk3bGiven,
            "BSIM4lw0Given" => &mut self.lw0Given,
            "BSIM4ldvtp0Given" => &mut self.ldvtp0Given,
            "BSIM4ldvtp1Given" => &mut self.ldvtp1Given,
            "BSIM4ldvtp2Given" => &mut self.ldvtp2Given,
            "BSIM4ldvtp3Given" => &mut self.ldvtp3Given,
            "BSIM4ldvtp4Given" => &mut self.ldvtp4Given,
            "BSIM4ldvtp5Given" => &mut self.ldvtp5Given,
            "BSIM4llpe0Given" => &mut self.llpe0Given,
            "BSIM4llpebGiven" => &mut self.llpebGiven,
            "BSIM4ldvt0Given" => &mut self.ldvt0Given,
            "BSIM4ldvt1Given" => &mut self.ldvt1Given,
            "BSIM4ldvt2Given" => &mut self.ldvt2Given,
            "BSIM4ldvt0wGiven" => &mut self.ldvt0wGiven,
            "BSIM4ldvt1wGiven" => &mut self.ldvt1wGiven,
            "BSIM4ldvt2wGiven" => &mut self.ldvt2wGiven,
            "BSIM4ldroutGiven" => &mut self.ldroutGiven,
            "BSIM4ldsubGiven" => &mut self.ldsubGiven,
            "BSIM4lvth0Given" => &mut self.lvth0Given,
            "BSIM4luaGiven" => &mut self.luaGiven,
            "BSIM4lua1Given" => &mut self.lua1Given,
            "BSIM4lubGiven" => &mut self.lubGiven,
            "BSIM4lub1Given" => &mut self.lub1Given,
            "BSIM4lucGiven" => &mut self.lucGiven,
            "BSIM4luc1Given" => &mut self.luc1Given,
            "BSIM4ludGiven" => &mut self.ludGiven,
            "BSIM4lud1Given" => &mut self.lud1Given,
            "BSIM4lupGiven" => &mut self.lupGiven,
            "BSIM4llpGiven" => &mut self.llpGiven,
            "BSIM4lu0Given" => &mut self.lu0Given,
            "BSIM4leuGiven" => &mut self.leuGiven,
            "BSIM4lucsGiven" => &mut self.lucsGiven,
            "BSIM4luteGiven" => &mut self.luteGiven,
            "BSIM4lucsteGiven" => &mut self.lucsteGiven,
            "BSIM4lvoffGiven" => &mut self.lvoffGiven,
            "BSIM4ltvoffGiven" => &mut self.ltvoffGiven,
            "BSIM4ltnfactorGiven" => &mut self.ltnfactorGiven,
            "BSIM4lteta0Given" => &mut self.lteta0Given,
            "BSIM4ltvoffcvGiven" => &mut self.ltvoffcvGiven,
            "BSIM4lminvGiven" => &mut self.lminvGiven,
            "BSIM4lminvcvGiven" => &mut self.lminvcvGiven,
            "BSIM4lrdswGiven" => &mut self.lrdswGiven,
            "BSIM4lrswGiven" => &mut self.lrswGiven,
            "BSIM4lrdwGiven" => &mut self.lrdwGiven,
            "BSIM4lprwgGiven" => &mut self.lprwgGiven,
            "BSIM4lprwbGiven" => &mut self.lprwbGiven,
            "BSIM4lprtGiven" => &mut self.lprtGiven,
            "BSIM4leta0Given" => &mut self.leta0Given,
            "BSIM4letabGiven" => &mut self.letabGiven,
            "BSIM4lpclmGiven" => &mut self.lpclmGiven,
            "BSIM4lpdibl1Given" => &mut self.lpdibl1Given,
            "BSIM4lpdibl2Given" => &mut self.lpdibl2Given,
            "BSIM4lpdiblbGiven" => &mut self.lpdiblbGiven,
            "BSIM4lfproutGiven" => &mut self.lfproutGiven,
            "BSIM4lpditsGiven" => &mut self.lpditsGiven,
            "BSIM4lpditsdGiven" => &mut self.lpditsdGiven,
            "BSIM4lpscbe1Given" => &mut self.lpscbe1Given,
            "BSIM4lpscbe2Given" => &mut self.lpscbe2Given,
            "BSIM4lpvagGiven" => &mut self.lpvagGiven,
            "BSIM4ldeltaGiven" => &mut self.ldeltaGiven,
            "BSIM4lwrGiven" => &mut self.lwrGiven,
            "BSIM4ldwgGiven" => &mut self.ldwgGiven,
            "BSIM4ldwbGiven" => &mut self.ldwbGiven,
            "BSIM4lb0Given" => &mut self.lb0Given,
            "BSIM4lb1Given" => &mut self.lb1Given,
            "BSIM4lalpha0Given" => &mut self.lalpha0Given,
            "BSIM4lalpha1Given" => &mut self.lalpha1Given,
            "BSIM4lbeta0Given" => &mut self.lbeta0Given,
            "BSIM4lvfbGiven" => &mut self.lvfbGiven,
            "BSIM4lagidlGiven" => &mut self.lagidlGiven,
            "BSIM4lbgidlGiven" => &mut self.lbgidlGiven,
            "BSIM4lcgidlGiven" => &mut self.lcgidlGiven,
            "BSIM4legidlGiven" => &mut self.legidlGiven,
            "BSIM4lfgidlGiven" => &mut self.lfgidlGiven,
            "BSIM4lkgidlGiven" => &mut self.lkgidlGiven,
            "BSIM4lrgidlGiven" => &mut self.lrgidlGiven,
            "BSIM4lagislGiven" => &mut self.lagislGiven,
            "BSIM4lbgislGiven" => &mut self.lbgislGiven,
            "BSIM4lcgislGiven" => &mut self.lcgislGiven,
            "BSIM4legislGiven" => &mut self.legislGiven,
            "BSIM4lfgislGiven" => &mut self.lfgislGiven,
            "BSIM4lkgislGiven" => &mut self.lkgislGiven,
            "BSIM4lrgislGiven" => &mut self.lrgislGiven,
            "BSIM4laigcGiven" => &mut self.laigcGiven,
            "BSIM4lbigcGiven" => &mut self.lbigcGiven,
            "BSIM4lcigcGiven" => &mut self.lcigcGiven,
            "BSIM4laigsdGiven" => &mut self.laigsdGiven,
            "BSIM4lbigsdGiven" => &mut self.lbigsdGiven,
            "BSIM4lcigsdGiven" => &mut self.lcigsdGiven,
            "BSIM4laigsGiven" => &mut self.laigsGiven,
            "BSIM4lbigsGiven" => &mut self.lbigsGiven,
            "BSIM4lcigsGiven" => &mut self.lcigsGiven,
            "BSIM4laigdGiven" => &mut self.laigdGiven,
            "BSIM4lbigdGiven" => &mut self.lbigdGiven,
            "BSIM4lcigdGiven" => &mut self.lcigdGiven,
            "BSIM4laigbaccGiven" => &mut self.laigbaccGiven,
            "BSIM4lbigbaccGiven" => &mut self.lbigbaccGiven,
            "BSIM4lcigbaccGiven" => &mut self.lcigbaccGiven,
            "BSIM4laigbinvGiven" => &mut self.laigbinvGiven,
            "BSIM4lbigbinvGiven" => &mut self.lbigbinvGiven,
            "BSIM4lcigbinvGiven" => &mut self.lcigbinvGiven,
            "BSIM4lnigcGiven" => &mut self.lnigcGiven,
            "BSIM4lnigbinvGiven" => &mut self.lnigbinvGiven,
            "BSIM4lnigbaccGiven" => &mut self.lnigbaccGiven,
            "BSIM4lntoxGiven" => &mut self.lntoxGiven,
            "BSIM4leigbinvGiven" => &mut self.leigbinvGiven,
            "BSIM4lpigcdGiven" => &mut self.lpigcdGiven,
            "BSIM4lpoxedgeGiven" => &mut self.lpoxedgeGiven,
            "BSIM4lxrcrg1Given" => &mut self.lxrcrg1Given,
            "BSIM4lxrcrg2Given" => &mut self.lxrcrg2Given,
            "BSIM4llambdaGiven" => &mut self.llambdaGiven,
            "BSIM4lvtlGiven" => &mut self.lvtlGiven,
            "BSIM4lxnGiven" => &mut self.lxnGiven,
            "BSIM4lvfbsdoffGiven" => &mut self.lvfbsdoffGiven,
            "BSIM4ltvfbsdoffGiven" => &mut self.ltvfbsdoffGiven,
            "BSIM4lcgslGiven" => &mut self.lcgslGiven,
            "BSIM4lcgdlGiven" => &mut self.lcgdlGiven,
            "BSIM4lckappasGiven" => &mut self.lckappasGiven,
            "BSIM4lckappadGiven" => &mut self.lckappadGiven,
            "BSIM4lcfGiven" => &mut self.lcfGiven,
            "BSIM4lclcGiven" => &mut self.lclcGiven,
            "BSIM4lcleGiven" => &mut self.lcleGiven,
            "BSIM4lvfbcvGiven" => &mut self.lvfbcvGiven,
            "BSIM4lnoffGiven" => &mut self.lnoffGiven,
            "BSIM4lvoffcvGiven" => &mut self.lvoffcvGiven,
            "BSIM4lacdeGiven" => &mut self.lacdeGiven,
            "BSIM4lmoinGiven" => &mut self.lmoinGiven,
            "BSIM4wcdscGiven" => &mut self.wcdscGiven,
            "BSIM4wcdscbGiven" => &mut self.wcdscbGiven,
            "BSIM4wcdscdGiven" => &mut self.wcdscdGiven,
            "BSIM4wcitGiven" => &mut self.wcitGiven,
            "BSIM4wnfactorGiven" => &mut self.wnfactorGiven,
            "BSIM4wxjGiven" => &mut self.wxjGiven,
            "BSIM4wvsatGiven" => &mut self.wvsatGiven,
            "BSIM4watGiven" => &mut self.watGiven,
            "BSIM4wa0Given" => &mut self.wa0Given,
            "BSIM4wagsGiven" => &mut self.wagsGiven,
            "BSIM4wa1Given" => &mut self.wa1Given,
            "BSIM4wa2Given" => &mut self.wa2Given,
            "BSIM4wketaGiven" => &mut self.wketaGiven,
            "BSIM4wketacGiven" => &mut self.wketacGiven,
            "BSIM4wnsubGiven" => &mut self.wnsubGiven,
            "BSIM4wndepGiven" => &mut self.wndepGiven,
            "BSIM4wnsdGiven" => &mut self.wnsdGiven,
            "BSIM4wphinGiven" => &mut self.wphinGiven,
            "BSIM4wngateGiven" => &mut self.wngateGiven,
            "BSIM4wgamma1Given" => &mut self.wgamma1Given,
            "BSIM4wgamma2Given" => &mut self.wgamma2Given,
            "BSIM4wvbxGiven" => &mut self.wvbxGiven,
            "BSIM4wvbmGiven" => &mut self.wvbmGiven,
            "BSIM4wxtGiven" => &mut self.wxtGiven,
            "BSIM4wk1Given" => &mut self.wk1Given,
            "BSIM4wkt1Given" => &mut self.wkt1Given,
            "BSIM4wkt1lGiven" => &mut self.wkt1lGiven,
            "BSIM4wkt2Given" => &mut self.wkt2Given,
            "BSIM4wk2Given" => &mut self.wk2Given,
            "BSIM4wk3Given" => &mut self.wk3Given,
            "BSIM4wk3bGiven" => &mut self.wk3bGiven,
            "BSIM4ww0Given" => &mut self.ww0Given,
            "BSIM4wdvtp0Given" => &mut self.wdvtp0Given,
            "BSIM4wdvtp1Given" => &mut self.wdvtp1Given,
            "BSIM4wdvtp2Given" => &mut self.wdvtp2Given,
            "BSIM4wdvtp3Given" => &mut self.wdvtp3Given,
            "BSIM4wdvtp4Given" => &mut self.wdvtp4Given,
            "BSIM4wdvtp5Given" => &mut self.wdvtp5Given,
            "BSIM4wlpe0Given" => &mut self.wlpe0Given,
            "BSIM4wlpebGiven" => &mut self.wlpebGiven,
            "BSIM4wdvt0Given" => &mut self.wdvt0Given,
            "BSIM4wdvt1Given" => &mut self.wdvt1Given,
            "BSIM4wdvt2Given" => &mut self.wdvt2Given,
            "BSIM4wdvt0wGiven" => &mut self.wdvt0wGiven,
            "BSIM4wdvt1wGiven" => &mut self.wdvt1wGiven,
            "BSIM4wdvt2wGiven" => &mut self.wdvt2wGiven,
            "BSIM4wdroutGiven" => &mut self.wdroutGiven,
            "BSIM4wdsubGiven" => &mut self.wdsubGiven,
            "BSIM4wvth0Given" => &mut self.wvth0Given,
            "BSIM4wuaGiven" => &mut self.wuaGiven,
            "BSIM4wua1Given" => &mut self.wua1Given,
            "BSIM4wubGiven" => &mut self.wubGiven,
            "BSIM4wub1Given" => &mut self.wub1Given,
            "BSIM4wucGiven" => &mut self.wucGiven,
            "BSIM4wuc1Given" => &mut self.wuc1Given,
            "BSIM4wudGiven" => &mut self.wudGiven,
            "BSIM4wud1Given" => &mut self.wud1Given,
            "BSIM4wupGiven" => &mut self.wupGiven,
            "BSIM4wlpGiven" => &mut self.wlpGiven,
            "BSIM4wu0Given" => &mut self.wu0Given,
            "BSIM4weuGiven" => &mut self.weuGiven,
            "BSIM4wucsGiven" => &mut self.wucsGiven,
            "BSIM4wuteGiven" => &mut self.wuteGiven,
            "BSIM4wucsteGiven" => &mut self.wucsteGiven,
            "BSIM4wvoffGiven" => &mut self.wvoffGiven,
            "BSIM4wtvoffGiven" => &mut self.wtvoffGiven,
            "BSIM4wtnfactorGiven" => &mut self.wtnfactorGiven,
            "BSIM4wteta0Given" => &mut self.wteta0Given,
            "BSIM4wtvoffcvGiven" => &mut self.wtvoffcvGiven,
            "BSIM4wminvGiven" => &mut self.wminvGiven,
            "BSIM4wminvcvGiven" => &mut self.wminvcvGiven,
            "BSIM4wrdswGiven" => &mut self.wrdswGiven,
            "BSIM4wrswGiven" => &mut self.wrswGiven,
            "BSIM4wrdwGiven" => &mut self.wrdwGiven,
            "BSIM4wprwgGiven" => &mut self.wprwgGiven,
            "BSIM4wprwbGiven" => &mut self.wprwbGiven,
            "BSIM4wprtGiven" => &mut self.wprtGiven,
            "BSIM4weta0Given" => &mut self.weta0Given,
            "BSIM4wetabGiven" => &mut self.wetabGiven,
            "BSIM4wpclmGiven" => &mut self.wpclmGiven,
            "BSIM4wpdibl1Given" => &mut self.wpdibl1Given,
            "BSIM4wpdibl2Given" => &mut self.wpdibl2Given,
            "BSIM4wpdiblbGiven" => &mut self.wpdiblbGiven,
            "BSIM4wfproutGiven" => &mut self.wfproutGiven,
            "BSIM4wpditsGiven" => &mut self.wpditsGiven,
            "BSIM4wpditsdGiven" => &mut self.wpditsdGiven,
            "BSIM4wpscbe1Given" => &mut self.wpscbe1Given,
            "BSIM4wpscbe2Given" => &mut self.wpscbe2Given,
            "BSIM4wpvagGiven" => &mut self.wpvagGiven,
            "BSIM4wdeltaGiven" => &mut self.wdeltaGiven,
            "BSIM4wwrGiven" => &mut self.wwrGiven,
            "BSIM4wdwgGiven" => &mut self.wdwgGiven,
            "BSIM4wdwbGiven" => &mut self.wdwbGiven,
            "BSIM4wb0Given" => &mut self.wb0Given,
            "BSIM4wb1Given" => &mut self.wb1Given,
            "BSIM4walpha0Given" => &mut self.walpha0Given,
            "BSIM4walpha1Given" => &mut self.walpha1Given,
            "BSIM4wbeta0Given" => &mut self.wbeta0Given,
            "BSIM4wvfbGiven" => &mut self.wvfbGiven,
            "BSIM4wagidlGiven" => &mut self.wagidlGiven,
            "BSIM4wbgidlGiven" => &mut self.wbgidlGiven,
            "BSIM4wcgidlGiven" => &mut self.wcgidlGiven,
            "BSIM4wegidlGiven" => &mut self.wegidlGiven,
            "BSIM4wfgidlGiven" => &mut self.wfgidlGiven,
            "BSIM4wkgidlGiven" => &mut self.wkgidlGiven,
            "BSIM4wrgidlGiven" => &mut self.wrgidlGiven,
            "BSIM4wagislGiven" => &mut self.wagislGiven,
            "BSIM4wbgislGiven" => &mut self.wbgislGiven,
            "BSIM4wcgislGiven" => &mut self.wcgislGiven,
            "BSIM4wegislGiven" => &mut self.wegislGiven,
            "BSIM4wfgislGiven" => &mut self.wfgislGiven,
            "BSIM4wkgislGiven" => &mut self.wkgislGiven,
            "BSIM4wrgislGiven" => &mut self.wrgislGiven,
            "BSIM4waigcGiven" => &mut self.waigcGiven,
            "BSIM4wbigcGiven" => &mut self.wbigcGiven,
            "BSIM4wcigcGiven" => &mut self.wcigcGiven,
            "BSIM4waigsdGiven" => &mut self.waigsdGiven,
            "BSIM4wbigsdGiven" => &mut self.wbigsdGiven,
            "BSIM4wcigsdGiven" => &mut self.wcigsdGiven,
            "BSIM4waigsGiven" => &mut self.waigsGiven,
            "BSIM4wbigsGiven" => &mut self.wbigsGiven,
            "BSIM4wcigsGiven" => &mut self.wcigsGiven,
            "BSIM4waigdGiven" => &mut self.waigdGiven,
            "BSIM4wbigdGiven" => &mut self.wbigdGiven,
            "BSIM4wcigdGiven" => &mut self.wcigdGiven,
            "BSIM4waigbaccGiven" => &mut self.waigbaccGiven,
            "BSIM4wbigbaccGiven" => &mut self.wbigbaccGiven,
            "BSIM4wcigbaccGiven" => &mut self.wcigbaccGiven,
            "BSIM4waigbinvGiven" => &mut self.waigbinvGiven,
            "BSIM4wbigbinvGiven" => &mut self.wbigbinvGiven,
            "BSIM4wcigbinvGiven" => &mut self.wcigbinvGiven,
            "BSIM4wnigcGiven" => &mut self.wnigcGiven,
            "BSIM4wnigbinvGiven" => &mut self.wnigbinvGiven,
            "BSIM4wnigbaccGiven" => &mut self.wnigbaccGiven,
            "BSIM4wntoxGiven" => &mut self.wntoxGiven,
            "BSIM4weigbinvGiven" => &mut self.weigbinvGiven,
            "BSIM4wpigcdGiven" => &mut self.wpigcdGiven,
            "BSIM4wpoxedgeGiven" => &mut self.wpoxedgeGiven,
            "BSIM4wxrcrg1Given" => &mut self.wxrcrg1Given,
            "BSIM4wxrcrg2Given" => &mut self.wxrcrg2Given,
            "BSIM4wlambdaGiven" => &mut self.wlambdaGiven,
            "BSIM4wvtlGiven" => &mut self.wvtlGiven,
            "BSIM4wxnGiven" => &mut self.wxnGiven,
            "BSIM4wvfbsdoffGiven" => &mut self.wvfbsdoffGiven,
            "BSIM4wtvfbsdoffGiven" => &mut self.wtvfbsdoffGiven,
            "BSIM4wcgslGiven" => &mut self.wcgslGiven,
            "BSIM4wcgdlGiven" => &mut self.wcgdlGiven,
            "BSIM4wckappasGiven" => &mut self.wckappasGiven,
            "BSIM4wckappadGiven" => &mut self.wckappadGiven,
            "BSIM4wcfGiven" => &mut self.wcfGiven,
            "BSIM4wclcGiven" => &mut self.wclcGiven,
            "BSIM4wcleGiven" => &mut self.wcleGiven,
            "BSIM4wvfbcvGiven" => &mut self.wvfbcvGiven,
            "BSIM4wnoffGiven" => &mut self.wnoffGiven,
            "BSIM4wvoffcvGiven" => &mut self.wvoffcvGiven,
            "BSIM4wacdeGiven" => &mut self.wacdeGiven,
            "BSIM4wmoinGiven" => &mut self.wmoinGiven,
            "BSIM4pcdscGiven" => &mut self.pcdscGiven,
            "BSIM4pcdscbGiven" => &mut self.pcdscbGiven,
            "BSIM4pcdscdGiven" => &mut self.pcdscdGiven,
            "BSIM4pcitGiven" => &mut self.pcitGiven,
            "BSIM4pnfactorGiven" => &mut self.pnfactorGiven,
            "BSIM4pxjGiven" => &mut self.pxjGiven,
            "BSIM4pvsatGiven" => &mut self.pvsatGiven,
            "BSIM4patGiven" => &mut self.patGiven,
            "BSIM4pa0Given" => &mut self.pa0Given,
            "BSIM4pagsGiven" => &mut self.pagsGiven,
            "BSIM4pa1Given" => &mut self.pa1Given,
            "BSIM4pa2Given" => &mut self.pa2Given,
            "BSIM4pketaGiven" => &mut self.pketaGiven,
            "BSIM4pketacGiven" => &mut self.pketacGiven,
            "BSIM4pnsubGiven" => &mut self.pnsubGiven,
            "BSIM4pndepGiven" => &mut self.pndepGiven,
            "BSIM4pnsdGiven" => &mut self.pnsdGiven,
            "BSIM4pphinGiven" => &mut self.pphinGiven,
            "BSIM4pngateGiven" => &mut self.pngateGiven,
            "BSIM4pgamma1Given" => &mut self.pgamma1Given,
            "BSIM4pgamma2Given" => &mut self.pgamma2Given,
            "BSIM4pvbxGiven" => &mut self.pvbxGiven,
            "BSIM4pvbmGiven" => &mut self.pvbmGiven,
            "BSIM4pxtGiven" => &mut self.pxtGiven,
            "BSIM4pk1Given" => &mut self.pk1Given,
            "BSIM4pkt1Given" => &mut self.pkt1Given,
            "BSIM4pkt1lGiven" => &mut self.pkt1lGiven,
            "BSIM4pkt2Given" => &mut self.pkt2Given,
            "BSIM4pk2Given" => &mut self.pk2Given,
            "BSIM4pk3Given" => &mut self.pk3Given,
            "BSIM4pk3bGiven" => &mut self.pk3bGiven,
            "BSIM4pw0Given" => &mut self.pw0Given,
            "BSIM4pdvtp0Given" => &mut self.pdvtp0Given,
            "BSIM4pdvtp1Given" => &mut self.pdvtp1Given,
            "BSIM4pdvtp2Given" => &mut self.pdvtp2Given,
            "BSIM4pdvtp3Given" => &mut self.pdvtp3Given,
            "BSIM4pdvtp4Given" => &mut self.pdvtp4Given,
            "BSIM4pdvtp5Given" => &mut self.pdvtp5Given,
            "BSIM4plpe0Given" => &mut self.plpe0Given,
            "BSIM4plpebGiven" => &mut self.plpebGiven,
            "BSIM4pdvt0Given" => &mut self.pdvt0Given,
            "BSIM4pdvt1Given" => &mut self.pdvt1Given,
            "BSIM4pdvt2Given" => &mut self.pdvt2Given,
            "BSIM4pdvt0wGiven" => &mut self.pdvt0wGiven,
            "BSIM4pdvt1wGiven" => &mut self.pdvt1wGiven,
            "BSIM4pdvt2wGiven" => &mut self.pdvt2wGiven,
            "BSIM4pdroutGiven" => &mut self.pdroutGiven,
            "BSIM4pdsubGiven" => &mut self.pdsubGiven,
            "BSIM4pvth0Given" => &mut self.pvth0Given,
            "BSIM4puaGiven" => &mut self.puaGiven,
            "BSIM4pua1Given" => &mut self.pua1Given,
            "BSIM4pubGiven" => &mut self.pubGiven,
            "BSIM4pub1Given" => &mut self.pub1Given,
            "BSIM4pucGiven" => &mut self.pucGiven,
            "BSIM4puc1Given" => &mut self.puc1Given,
            "BSIM4pudGiven" => &mut self.pudGiven,
            "BSIM4pud1Given" => &mut self.pud1Given,
            "BSIM4pupGiven" => &mut self.pupGiven,
            "BSIM4plpGiven" => &mut self.plpGiven,
            "BSIM4pu0Given" => &mut self.pu0Given,
            "BSIM4peuGiven" => &mut self.peuGiven,
            "BSIM4pucsGiven" => &mut self.pucsGiven,
            "BSIM4puteGiven" => &mut self.puteGiven,
            "BSIM4pucsteGiven" => &mut self.pucsteGiven,
            "BSIM4pvoffGiven" => &mut self.pvoffGiven,
            "BSIM4ptvoffGiven" => &mut self.ptvoffGiven,
            "BSIM4ptnfactorGiven" => &mut self.ptnfactorGiven,
            "BSIM4pteta0Given" => &mut self.pteta0Given,
            "BSIM4ptvoffcvGiven" => &mut self.ptvoffcvGiven,
            "BSIM4pminvGiven" => &mut self.pminvGiven,
            "BSIM4pminvcvGiven" => &mut self.pminvcvGiven,
            "BSIM4prdswGiven" => &mut self.prdswGiven,
            "BSIM4prswGiven" => &mut self.prswGiven,
            "BSIM4prdwGiven" => &mut self.prdwGiven,
            "BSIM4pprwgGiven" => &mut self.pprwgGiven,
            "BSIM4pprwbGiven" => &mut self.pprwbGiven,
            "BSIM4pprtGiven" => &mut self.pprtGiven,
            "BSIM4peta0Given" => &mut self.peta0Given,
            "BSIM4petabGiven" => &mut self.petabGiven,
            "BSIM4ppclmGiven" => &mut self.ppclmGiven,
            "BSIM4ppdibl1Given" => &mut self.ppdibl1Given,
            "BSIM4ppdibl2Given" => &mut self.ppdibl2Given,
            "BSIM4ppdiblbGiven" => &mut self.ppdiblbGiven,
            "BSIM4pfproutGiven" => &mut self.pfproutGiven,
            "BSIM4ppditsGiven" => &mut self.ppditsGiven,
            "BSIM4ppditsdGiven" => &mut self.ppditsdGiven,
            "BSIM4ppscbe1Given" => &mut self.ppscbe1Given,
            "BSIM4ppscbe2Given" => &mut self.ppscbe2Given,
            "BSIM4ppvagGiven" => &mut self.ppvagGiven,
            "BSIM4pdeltaGiven" => &mut self.pdeltaGiven,
            "BSIM4pwrGiven" => &mut self.pwrGiven,
            "BSIM4pdwgGiven" => &mut self.pdwgGiven,
            "BSIM4pdwbGiven" => &mut self.pdwbGiven,
            "BSIM4pb0Given" => &mut self.pb0Given,
            "BSIM4pb1Given" => &mut self.pb1Given,
            "BSIM4palpha0Given" => &mut self.palpha0Given,
            "BSIM4palpha1Given" => &mut self.palpha1Given,
            "BSIM4pbeta0Given" => &mut self.pbeta0Given,
            "BSIM4pvfbGiven" => &mut self.pvfbGiven,
            "BSIM4pagidlGiven" => &mut self.pagidlGiven,
            "BSIM4pbgidlGiven" => &mut self.pbgidlGiven,
            "BSIM4pcgidlGiven" => &mut self.pcgidlGiven,
            "BSIM4pegidlGiven" => &mut self.pegidlGiven,
            "BSIM4pfgidlGiven" => &mut self.pfgidlGiven,
            "BSIM4pkgidlGiven" => &mut self.pkgidlGiven,
            "BSIM4prgidlGiven" => &mut self.prgidlGiven,
            "BSIM4pagislGiven" => &mut self.pagislGiven,
            "BSIM4pbgislGiven" => &mut self.pbgislGiven,
            "BSIM4pcgislGiven" => &mut self.pcgislGiven,
            "BSIM4pegislGiven" => &mut self.pegislGiven,
            "BSIM4pfgislGiven" => &mut self.pfgislGiven,
            "BSIM4pkgislGiven" => &mut self.pkgislGiven,
            "BSIM4prgislGiven" => &mut self.prgislGiven,
            "BSIM4paigcGiven" => &mut self.paigcGiven,
            "BSIM4pbigcGiven" => &mut self.pbigcGiven,
            "BSIM4pcigcGiven" => &mut self.pcigcGiven,
            "BSIM4paigsdGiven" => &mut self.paigsdGiven,
            "BSIM4pbigsdGiven" => &mut self.pbigsdGiven,
            "BSIM4pcigsdGiven" => &mut self.pcigsdGiven,
            "BSIM4paigsGiven" => &mut self.paigsGiven,
            "BSIM4pbigsGiven" => &mut self.pbigsGiven,
            "BSIM4pcigsGiven" => &mut self.pcigsGiven,
            "BSIM4paigdGiven" => &mut self.paigdGiven,
            "BSIM4pbigdGiven" => &mut self.pbigdGiven,
            "BSIM4pcigdGiven" => &mut self.pcigdGiven,
            "BSIM4paigbaccGiven" => &mut self.paigbaccGiven,
            "BSIM4pbigbaccGiven" => &mut self.pbigbaccGiven,
            "BSIM4pcigbaccGiven" => &mut self.pcigbaccGiven,
            "BSIM4paigbinvGiven" => &mut self.paigbinvGiven,
            "BSIM4pbigbinvGiven" => &mut self.pbigbinvGiven,
            "BSIM4pcigbinvGiven" => &mut self.pcigbinvGiven,
            "BSIM4pnigcGiven" => &mut self.pnigcGiven,
            "BSIM4pnigbinvGiven" => &mut self.pnigbinvGiven,
            "BSIM4pnigbaccGiven" => &mut self.pnigbaccGiven,
            "BSIM4pntoxGiven" => &mut self.pntoxGiven,
            "BSIM4peigbinvGiven" => &mut self.peigbinvGiven,
            "BSIM4ppigcdGiven" => &mut self.ppigcdGiven,
            "BSIM4ppoxedgeGiven" => &mut self.ppoxedgeGiven,
            "BSIM4pxrcrg1Given" => &mut self.pxrcrg1Given,
            "BSIM4pxrcrg2Given" => &mut self.pxrcrg2Given,
            "BSIM4plambdaGiven" => &mut self.plambdaGiven,
            "BSIM4pvtlGiven" => &mut self.pvtlGiven,
            "BSIM4pxnGiven" => &mut self.pxnGiven,
            "BSIM4pvfbsdoffGiven" => &mut self.pvfbsdoffGiven,
            "BSIM4ptvfbsdoffGiven" => &mut self.ptvfbsdoffGiven,
            "BSIM4pcgslGiven" => &mut self.pcgslGiven,
            "BSIM4pcgdlGiven" => &mut self.pcgdlGiven,
            "BSIM4pckappasGiven" => &mut self.pckappasGiven,
            "BSIM4pckappadGiven" => &mut self.pckappadGiven,
            "BSIM4pcfGiven" => &mut self.pcfGiven,
            "BSIM4pclcGiven" => &mut self.pclcGiven,
            "BSIM4pcleGiven" => &mut self.pcleGiven,
            "BSIM4pvfbcvGiven" => &mut self.pvfbcvGiven,
            "BSIM4pnoffGiven" => &mut self.pnoffGiven,
            "BSIM4pvoffcvGiven" => &mut self.pvoffcvGiven,
            "BSIM4pacdeGiven" => &mut self.pacdeGiven,
            "BSIM4pmoinGiven" => &mut self.pmoinGiven,
            "BSIM4useFringeGiven" => &mut self.useFringeGiven,
            "BSIM4tnomGiven" => &mut self.tnomGiven,
            "BSIM4cgsoGiven" => &mut self.cgsoGiven,
            "BSIM4cgdoGiven" => &mut self.cgdoGiven,
            "BSIM4cgboGiven" => &mut self.cgboGiven,
            "BSIM4xpartGiven" => &mut self.xpartGiven,
            "BSIM4sheetResistanceGiven" => &mut self.sheetResistanceGiven,
            "BSIM4SjctSatCurDensityGiven" => &mut self.SjctSatCurDensityGiven,
            "BSIM4SjctSidewallSatCurDensityGiven" => &mut self.SjctSidewallSatCurDensityGiven,
            "BSIM4SjctGateSidewallSatCurDensityGiven" => &mut self.SjctGateSidewallSatCurDensityGiven,
            "BSIM4SbulkJctPotentialGiven" => &mut self.SbulkJctPotentialGiven,
            "BSIM4SbulkJctBotGradingCoeffGiven" => &mut self.SbulkJctBotGradingCoeffGiven,
            "BSIM4SsidewallJctPotentialGiven" => &mut self.SsidewallJctPotentialGiven,
            "BSIM4SGatesidewallJctPotentialGiven" => &mut self.SGatesidewallJctPotentialGiven,
            "BSIM4SbulkJctSideGradingCoeffGiven" => &mut self.SbulkJctSideGradingCoeffGiven,
            "BSIM4SunitAreaJctCapGiven" => &mut self.SunitAreaJctCapGiven,
            "BSIM4SunitLengthSidewallJctCapGiven" => &mut self.SunitLengthSidewallJctCapGiven,
            "BSIM4SbulkJctGateSideGradingCoeffGiven" => &mut self.SbulkJctGateSideGradingCoeffGiven,
            "BSIM4SunitLengthGateSidewallJctCapGiven" => &mut self.SunitLengthGateSidewallJctCapGiven,
            "BSIM4SjctEmissionCoeffGiven" => &mut self.SjctEmissionCoeffGiven,
            "BSIM4SjctTempExponentGiven" => &mut self.SjctTempExponentGiven,
            "BSIM4DjctSatCurDensityGiven" => &mut self.DjctSatCurDensityGiven,
            "BSIM4DjctSidewallSatCurDensityGiven" => &mut self.DjctSidewallSatCurDensityGiven,
            "BSIM4DjctGateSidewallSatCurDensityGiven" => &mut self.DjctGateSidewallSatCurDensityGiven,
            "BSIM4DbulkJctPotentialGiven" => &mut self.DbulkJctPotentialGiven,
            "BSIM4DbulkJctBotGradingCoeffGiven" => &mut self.DbulkJctBotGradingCoeffGiven,
            "BSIM4DsidewallJctPotentialGiven" => &mut self.DsidewallJctPotentialGiven,
            "BSIM4DGatesidewallJctPotentialGiven" => &mut self.DGatesidewallJctPotentialGiven,
            "BSIM4DbulkJctSideGradingCoeffGiven" => &mut self.DbulkJctSideGradingCoeffGiven,
            "BSIM4DunitAreaJctCapGiven" => &mut self.DunitAreaJctCapGiven,
            "BSIM4DunitLengthSidewallJctCapGiven" => &mut self.DunitLengthSidewallJctCapGiven,
            "BSIM4DbulkJctGateSideGradingCoeffGiven" => &mut self.DbulkJctGateSideGradingCoeffGiven,
            "BSIM4DunitLengthGateSidewallJctCapGiven" => &mut self.DunitLengthGateSidewallJctCapGiven,
            "BSIM4DjctEmissionCoeffGiven" => &mut self.DjctEmissionCoeffGiven,
            "BSIM4DjctTempExponentGiven" => &mut self.DjctTempExponentGiven,
            "BSIM4oxideTrapDensityAGiven" => &mut self.oxideTrapDensityAGiven,
            "BSIM4oxideTrapDensityBGiven" => &mut self.oxideTrapDensityBGiven,
            "BSIM4oxideTrapDensityCGiven" => &mut self.oxideTrapDensityCGiven,
            "BSIM4emGiven" => &mut self.emGiven,
            "BSIM4efGiven" => &mut self.efGiven,
            "BSIM4afGiven" => &mut self.afGiven,
            "BSIM4kfGiven" => &mut self.kfGiven,
            "BSIM4vgsMaxGiven" => &mut self.vgsMaxGiven,
            "BSIM4vgdMaxGiven" => &mut self.vgdMaxGiven,
            "BSIM4vgbMaxGiven" => &mut self.vgbMaxGiven,
            "BSIM4vdsMaxGiven" => &mut self.vdsMaxGiven,
            "BSIM4vbsMaxGiven" => &mut self.vbsMaxGiven,
            "BSIM4vbdMaxGiven" => &mut self.vbdMaxGiven,
            "BSIM4vgsrMaxGiven" => &mut self.vgsrMaxGiven,
            "BSIM4vgdrMaxGiven" => &mut self.vgdrMaxGiven,
            "BSIM4vgbrMaxGiven" => &mut self.vgbrMaxGiven,
            "BSIM4vbsrMaxGiven" => &mut self.vbsrMaxGiven,
            "BSIM4vbdrMaxGiven" => &mut self.vbdrMaxGiven,
            "BSIM4LintGiven" => &mut self.LintGiven,
            "BSIM4LlGiven" => &mut self.LlGiven,
            "BSIM4LlcGiven" => &mut self.LlcGiven,
            "BSIM4LlnGiven" => &mut self.LlnGiven,
            "BSIM4LwGiven" => &mut self.LwGiven,
            "BSIM4LwcGiven" => &mut self.LwcGiven,
            "BSIM4LwnGiven" => &mut self.LwnGiven,
            "BSIM4LwlGiven" => &mut self.LwlGiven,
            "BSIM4LwlcGiven" => &mut self.LwlcGiven,
            "BSIM4LminGiven" => &mut self.LminGiven,
            "BSIM4LmaxGiven" => &mut self.LmaxGiven,
            "BSIM4WintGiven" => &mut self.WintGiven,
            "BSIM4WlGiven" => &mut self.WlGiven,
            "BSIM4WlcGiven" => &mut self.WlcGiven,
            "BSIM4WlnGiven" => &mut self.WlnGiven,
            "BSIM4WwGiven" => &mut self.WwGiven,
            "BSIM4WwcGiven" => &mut self.WwcGiven,
            "BSIM4WwnGiven" => &mut self.WwnGiven,
            "BSIM4WwlGiven" => &mut self.WwlGiven,
            "BSIM4WwlcGiven" => &mut self.WwlcGiven,
            "BSIM4WminGiven" => &mut self.WminGiven,
            "BSIM4WmaxGiven" => &mut self.WmaxGiven,
            "BSIM4sarefGiven" => &mut self.sarefGiven,
            "BSIM4sbrefGiven" => &mut self.sbrefGiven,
            "BSIM4wlodGiven" => &mut self.wlodGiven,
            "BSIM4ku0Given" => &mut self.ku0Given,
            "BSIM4kvsatGiven" => &mut self.kvsatGiven,
            "BSIM4kvth0Given" => &mut self.kvth0Given,
            "BSIM4tku0Given" => &mut self.tku0Given,
            "BSIM4llodku0Given" => &mut self.llodku0Given,
            "BSIM4wlodku0Given" => &mut self.wlodku0Given,
            "BSIM4llodvthGiven" => &mut self.llodvthGiven,
            "BSIM4wlodvthGiven" => &mut self.wlodvthGiven,
            "BSIM4lku0Given" => &mut self.lku0Given,
            "BSIM4wku0Given" => &mut self.wku0Given,
            "BSIM4pku0Given" => &mut self.pku0Given,
            "BSIM4lkvth0Given" => &mut self.lkvth0Given,
            "BSIM4wkvth0Given" => &mut self.wkvth0Given,
            "BSIM4pkvth0Given" => &mut self.pkvth0Given,
            "BSIM4stk2Given" => &mut self.stk2Given,
            "BSIM4lodk2Given" => &mut self.lodk2Given,
            "BSIM4steta0Given" => &mut self.steta0Given,
            "BSIM4lodeta0Given" => &mut self.lodeta0Given,
            "BSIM4webGiven" => &mut self.webGiven,
            "BSIM4wecGiven" => &mut self.wecGiven,
            "BSIM4kvth0weGiven" => &mut self.kvth0weGiven,
            "BSIM4k2weGiven" => &mut self.k2weGiven,
            "BSIM4ku0weGiven" => &mut self.ku0weGiven,
            "BSIM4screfGiven" => &mut self.screfGiven,
            "BSIM4wpemodGiven" => &mut self.wpemodGiven,
            "BSIM4lkvth0weGiven" => &mut self.lkvth0weGiven,
            "BSIM4lk2weGiven" => &mut self.lk2weGiven,
            "BSIM4lku0weGiven" => &mut self.lku0weGiven,
            "BSIM4wkvth0weGiven" => &mut self.wkvth0weGiven,
            "BSIM4wk2weGiven" => &mut self.wk2weGiven,
            "BSIM4wku0weGiven" => &mut self.wku0weGiven,
            "BSIM4pkvth0weGiven" => &mut self.pkvth0weGiven,
            "BSIM4pk2weGiven" => &mut self.pk2weGiven,
            "BSIM4pku0weGiven" => &mut self.pku0weGiven,
            "BSIM4gidlclampGiven" => &mut self.gidlclampGiven,
            "BSIM4idovvdscGiven" => &mut self.idovvdscGiven,
            _ => return None,
        })
    }
    /// C member names of the scalar members, in declaration order.
    #[allow(dead_code)]
    pub(crate) const MEMBERS: &[&str] = &[
        "BSIM4type",
        "BSIM4mobMod",
        "BSIM4cvchargeMod",
        "BSIM4capMod",
        "BSIM4dioMod",
        "BSIM4trnqsMod",
        "BSIM4acnqsMod",
        "BSIM4fnoiMod",
        "BSIM4tnoiMod",
        "BSIM4rdsMod",
        "BSIM4rbodyMod",
        "BSIM4rgateMod",
        "BSIM4perMod",
        "BSIM4geoMod",
        "BSIM4mtrlMod",
        "BSIM4mtrlCompatMod",
        "BSIM4gidlMod",
        "BSIM4igcMod",
        "BSIM4igbMod",
        "BSIM4tempMod",
        "BSIM4binUnit",
        "BSIM4paramChk",
        "BSIM4v48intVersion",
        "BSIM4eot",
        "BSIM4vddeot",
        "BSIM4tempeot",
        "BSIM4leffeot",
        "BSIM4weffeot",
        "BSIM4ados",
        "BSIM4bdos",
        "BSIM4toxe",
        "BSIM4toxp",
        "BSIM4toxm",
        "BSIM4dtox",
        "BSIM4epsrox",
        "BSIM4cdsc",
        "BSIM4cdscb",
        "BSIM4cdscd",
        "BSIM4cit",
        "BSIM4nfactor",
        "BSIM4xj",
        "BSIM4vsat",
        "BSIM4at",
        "BSIM4a0",
        "BSIM4ags",
        "BSIM4a1",
        "BSIM4a2",
        "BSIM4keta",
        "BSIM4ketac",
        "BSIM4nsub",
        "BSIM4phig",
        "BSIM4epsrgate",
        "BSIM4easub",
        "BSIM4epsrsub",
        "BSIM4ni0sub",
        "BSIM4bg0sub",
        "BSIM4tbgasub",
        "BSIM4tbgbsub",
        "BSIM4ndep",
        "BSIM4nsd",
        "BSIM4phin",
        "BSIM4ngate",
        "BSIM4gamma1",
        "BSIM4gamma2",
        "BSIM4vbx",
        "BSIM4vbm",
        "BSIM4xt",
        "BSIM4k1",
        "BSIM4kt1",
        "BSIM4kt1l",
        "BSIM4kt2",
        "BSIM4k2",
        "BSIM4k3",
        "BSIM4k3b",
        "BSIM4w0",
        "BSIM4dvtp0",
        "BSIM4dvtp1",
        "BSIM4dvtp2",
        "BSIM4dvtp3",
        "BSIM4dvtp4",
        "BSIM4dvtp5",
        "BSIM4lpe0",
        "BSIM4lpeb",
        "BSIM4dvt0",
        "BSIM4dvt1",
        "BSIM4dvt2",
        "BSIM4dvt0w",
        "BSIM4dvt1w",
        "BSIM4dvt2w",
        "BSIM4drout",
        "BSIM4dsub",
        "BSIM4vth0",
        "BSIM4eu",
        "BSIM4ucs",
        "BSIM4ua",
        "BSIM4ua1",
        "BSIM4ub",
        "BSIM4ub1",
        "BSIM4uc",
        "BSIM4uc1",
        "BSIM4ud",
        "BSIM4ud1",
        "BSIM4up",
        "BSIM4lp",
        "BSIM4u0",
        "BSIM4ute",
        "BSIM4ucste",
        "BSIM4voff",
        "BSIM4tvoff",
        "BSIM4tnfactor",
        "BSIM4teta0",
        "BSIM4tvoffcv",
        "BSIM4minv",
        "BSIM4minvcv",
        "BSIM4voffl",
        "BSIM4voffcvl",
        "BSIM4delta",
        "BSIM4rdsw",
        "BSIM4rdswmin",
        "BSIM4rdwmin",
        "BSIM4rswmin",
        "BSIM4rsw",
        "BSIM4rdw",
        "BSIM4prwg",
        "BSIM4prwb",
        "BSIM4prt",
        "BSIM4eta0",
        "BSIM4etab",
        "BSIM4pclm",
        "BSIM4pdibl1",
        "BSIM4pdibl2",
        "BSIM4pdiblb",
        "BSIM4fprout",
        "BSIM4pdits",
        "BSIM4pditsd",
        "BSIM4pditsl",
        "BSIM4pscbe1",
        "BSIM4pscbe2",
        "BSIM4pvag",
        "BSIM4wr",
        "BSIM4dwg",
        "BSIM4dwb",
        "BSIM4b0",
        "BSIM4b1",
        "BSIM4alpha0",
        "BSIM4alpha1",
        "BSIM4beta0",
        "BSIM4agidl",
        "BSIM4bgidl",
        "BSIM4cgidl",
        "BSIM4egidl",
        "BSIM4fgidl",
        "BSIM4kgidl",
        "BSIM4rgidl",
        "BSIM4agisl",
        "BSIM4bgisl",
        "BSIM4cgisl",
        "BSIM4egisl",
        "BSIM4fgisl",
        "BSIM4kgisl",
        "BSIM4rgisl",
        "BSIM4aigc",
        "BSIM4bigc",
        "BSIM4cigc",
        "BSIM4aigsd",
        "BSIM4bigsd",
        "BSIM4cigsd",
        "BSIM4aigs",
        "BSIM4bigs",
        "BSIM4cigs",
        "BSIM4aigd",
        "BSIM4bigd",
        "BSIM4cigd",
        "BSIM4aigbacc",
        "BSIM4bigbacc",
        "BSIM4cigbacc",
        "BSIM4aigbinv",
        "BSIM4bigbinv",
        "BSIM4cigbinv",
        "BSIM4nigc",
        "BSIM4nigbacc",
        "BSIM4nigbinv",
        "BSIM4ntox",
        "BSIM4eigbinv",
        "BSIM4pigcd",
        "BSIM4poxedge",
        "BSIM4toxref",
        "BSIM4ijthdfwd",
        "BSIM4ijthsfwd",
        "BSIM4ijthdrev",
        "BSIM4ijthsrev",
        "BSIM4xjbvd",
        "BSIM4xjbvs",
        "BSIM4bvd",
        "BSIM4bvs",
        "BSIM4jtss",
        "BSIM4jtsd",
        "BSIM4jtssws",
        "BSIM4jtsswd",
        "BSIM4jtsswgs",
        "BSIM4jtsswgd",
        "BSIM4jtweff",
        "BSIM4njts",
        "BSIM4njtssw",
        "BSIM4njtsswg",
        "BSIM4njtsd",
        "BSIM4njtsswd",
        "BSIM4njtsswgd",
        "BSIM4xtss",
        "BSIM4xtsd",
        "BSIM4xtssws",
        "BSIM4xtsswd",
        "BSIM4xtsswgs",
        "BSIM4xtsswgd",
        "BSIM4tnjts",
        "BSIM4tnjtssw",
        "BSIM4tnjtsswg",
        "BSIM4tnjtsd",
        "BSIM4tnjtsswd",
        "BSIM4tnjtsswgd",
        "BSIM4vtss",
        "BSIM4vtsd",
        "BSIM4vtssws",
        "BSIM4vtsswd",
        "BSIM4vtsswgs",
        "BSIM4vtsswgd",
        "BSIM4xrcrg1",
        "BSIM4xrcrg2",
        "BSIM4lambda",
        "BSIM4vtl",
        "BSIM4lc",
        "BSIM4xn",
        "BSIM4vfbsdoff",
        "BSIM4lintnoi",
        "BSIM4tvfbsdoff",
        "BSIM4vfb",
        "BSIM4gbmin",
        "BSIM4rbdb",
        "BSIM4rbsb",
        "BSIM4rbpb",
        "BSIM4rbps",
        "BSIM4rbpd",
        "BSIM4rbps0",
        "BSIM4rbpsl",
        "BSIM4rbpsw",
        "BSIM4rbpsnf",
        "BSIM4rbpd0",
        "BSIM4rbpdl",
        "BSIM4rbpdw",
        "BSIM4rbpdnf",
        "BSIM4rbpbx0",
        "BSIM4rbpbxl",
        "BSIM4rbpbxw",
        "BSIM4rbpbxnf",
        "BSIM4rbpby0",
        "BSIM4rbpbyl",
        "BSIM4rbpbyw",
        "BSIM4rbpbynf",
        "BSIM4rbsbx0",
        "BSIM4rbsby0",
        "BSIM4rbdbx0",
        "BSIM4rbdby0",
        "BSIM4rbsdbxl",
        "BSIM4rbsdbxw",
        "BSIM4rbsdbxnf",
        "BSIM4rbsdbyl",
        "BSIM4rbsdbyw",
        "BSIM4rbsdbynf",
        "BSIM4tnoia",
        "BSIM4tnoib",
        "BSIM4tnoic",
        "BSIM4rnoia",
        "BSIM4rnoib",
        "BSIM4rnoic",
        "BSIM4ntnoi",
        "BSIM4cgsl",
        "BSIM4cgdl",
        "BSIM4ckappas",
        "BSIM4ckappad",
        "BSIM4cf",
        "BSIM4vfbcv",
        "BSIM4clc",
        "BSIM4cle",
        "BSIM4dwc",
        "BSIM4dlc",
        "BSIM4xw",
        "BSIM4xl",
        "BSIM4dlcig",
        "BSIM4dlcigd",
        "BSIM4dwj",
        "BSIM4noff",
        "BSIM4voffcv",
        "BSIM4acde",
        "BSIM4moin",
        "BSIM4tcj",
        "BSIM4tcjsw",
        "BSIM4tcjswg",
        "BSIM4tpb",
        "BSIM4tpbsw",
        "BSIM4tpbswg",
        "BSIM4dmcg",
        "BSIM4dmci",
        "BSIM4dmdg",
        "BSIM4dmcgt",
        "BSIM4xgw",
        "BSIM4xgl",
        "BSIM4rshg",
        "BSIM4ngcon",
        "BSIM4lcdsc",
        "BSIM4lcdscb",
        "BSIM4lcdscd",
        "BSIM4lcit",
        "BSIM4lnfactor",
        "BSIM4lxj",
        "BSIM4lvsat",
        "BSIM4lat",
        "BSIM4la0",
        "BSIM4lags",
        "BSIM4la1",
        "BSIM4la2",
        "BSIM4lketa",
        "BSIM4lketac",
        "BSIM4lnsub",
        "BSIM4lndep",
        "BSIM4lnsd",
        "BSIM4lphin",
        "BSIM4lngate",
        "BSIM4lgamma1",
        "BSIM4lgamma2",
        "BSIM4lvbx",
        "BSIM4lvbm",
        "BSIM4lxt",
        "BSIM4lk1",
        "BSIM4lkt1",
        "BSIM4lkt1l",
        "BSIM4lkt2",
        "BSIM4lk2",
        "BSIM4lk3",
        "BSIM4lk3b",
        "BSIM4lw0",
        "BSIM4ldvtp0",
        "BSIM4ldvtp1",
        "BSIM4ldvtp2",
        "BSIM4ldvtp3",
        "BSIM4ldvtp4",
        "BSIM4ldvtp5",
        "BSIM4llpe0",
        "BSIM4llpeb",
        "BSIM4ldvt0",
        "BSIM4ldvt1",
        "BSIM4ldvt2",
        "BSIM4ldvt0w",
        "BSIM4ldvt1w",
        "BSIM4ldvt2w",
        "BSIM4ldrout",
        "BSIM4ldsub",
        "BSIM4lvth0",
        "BSIM4lua",
        "BSIM4lua1",
        "BSIM4lub",
        "BSIM4lub1",
        "BSIM4luc",
        "BSIM4luc1",
        "BSIM4lud",
        "BSIM4lud1",
        "BSIM4lup",
        "BSIM4llp",
        "BSIM4lu0",
        "BSIM4leu",
        "BSIM4lucs",
        "BSIM4lute",
        "BSIM4lucste",
        "BSIM4lvoff",
        "BSIM4ltvoff",
        "BSIM4ltnfactor",
        "BSIM4lteta0",
        "BSIM4ltvoffcv",
        "BSIM4lminv",
        "BSIM4lminvcv",
        "BSIM4ldelta",
        "BSIM4lrdsw",
        "BSIM4lrsw",
        "BSIM4lrdw",
        "BSIM4lprwg",
        "BSIM4lprwb",
        "BSIM4lprt",
        "BSIM4leta0",
        "BSIM4letab",
        "BSIM4lpclm",
        "BSIM4lpdibl1",
        "BSIM4lpdibl2",
        "BSIM4lpdiblb",
        "BSIM4lfprout",
        "BSIM4lpdits",
        "BSIM4lpditsd",
        "BSIM4lpscbe1",
        "BSIM4lpscbe2",
        "BSIM4lpvag",
        "BSIM4lwr",
        "BSIM4ldwg",
        "BSIM4ldwb",
        "BSIM4lb0",
        "BSIM4lb1",
        "BSIM4lalpha0",
        "BSIM4lalpha1",
        "BSIM4lbeta0",
        "BSIM4lvfb",
        "BSIM4lagidl",
        "BSIM4lbgidl",
        "BSIM4lcgidl",
        "BSIM4legidl",
        "BSIM4lfgidl",
        "BSIM4lkgidl",
        "BSIM4lrgidl",
        "BSIM4lagisl",
        "BSIM4lbgisl",
        "BSIM4lcgisl",
        "BSIM4legisl",
        "BSIM4lfgisl",
        "BSIM4lkgisl",
        "BSIM4lrgisl",
        "BSIM4laigc",
        "BSIM4lbigc",
        "BSIM4lcigc",
        "BSIM4laigsd",
        "BSIM4lbigsd",
        "BSIM4lcigsd",
        "BSIM4laigs",
        "BSIM4lbigs",
        "BSIM4lcigs",
        "BSIM4laigd",
        "BSIM4lbigd",
        "BSIM4lcigd",
        "BSIM4laigbacc",
        "BSIM4lbigbacc",
        "BSIM4lcigbacc",
        "BSIM4laigbinv",
        "BSIM4lbigbinv",
        "BSIM4lcigbinv",
        "BSIM4lnigc",
        "BSIM4lnigbacc",
        "BSIM4lnigbinv",
        "BSIM4lntox",
        "BSIM4leigbinv",
        "BSIM4lpigcd",
        "BSIM4lpoxedge",
        "BSIM4lxrcrg1",
        "BSIM4lxrcrg2",
        "BSIM4llambda",
        "BSIM4lvtl",
        "BSIM4lxn",
        "BSIM4lvfbsdoff",
        "BSIM4ltvfbsdoff",
        "BSIM4lcgsl",
        "BSIM4lcgdl",
        "BSIM4lckappas",
        "BSIM4lckappad",
        "BSIM4lcf",
        "BSIM4lclc",
        "BSIM4lcle",
        "BSIM4lvfbcv",
        "BSIM4lnoff",
        "BSIM4lvoffcv",
        "BSIM4lacde",
        "BSIM4lmoin",
        "BSIM4wcdsc",
        "BSIM4wcdscb",
        "BSIM4wcdscd",
        "BSIM4wcit",
        "BSIM4wnfactor",
        "BSIM4wxj",
        "BSIM4wvsat",
        "BSIM4wat",
        "BSIM4wa0",
        "BSIM4wags",
        "BSIM4wa1",
        "BSIM4wa2",
        "BSIM4wketa",
        "BSIM4wketac",
        "BSIM4wnsub",
        "BSIM4wndep",
        "BSIM4wnsd",
        "BSIM4wphin",
        "BSIM4wngate",
        "BSIM4wgamma1",
        "BSIM4wgamma2",
        "BSIM4wvbx",
        "BSIM4wvbm",
        "BSIM4wxt",
        "BSIM4wk1",
        "BSIM4wkt1",
        "BSIM4wkt1l",
        "BSIM4wkt2",
        "BSIM4wk2",
        "BSIM4wk3",
        "BSIM4wk3b",
        "BSIM4ww0",
        "BSIM4wdvtp0",
        "BSIM4wdvtp1",
        "BSIM4wdvtp2",
        "BSIM4wdvtp3",
        "BSIM4wdvtp4",
        "BSIM4wdvtp5",
        "BSIM4wlpe0",
        "BSIM4wlpeb",
        "BSIM4wdvt0",
        "BSIM4wdvt1",
        "BSIM4wdvt2",
        "BSIM4wdvt0w",
        "BSIM4wdvt1w",
        "BSIM4wdvt2w",
        "BSIM4wdrout",
        "BSIM4wdsub",
        "BSIM4wvth0",
        "BSIM4wua",
        "BSIM4wua1",
        "BSIM4wub",
        "BSIM4wub1",
        "BSIM4wuc",
        "BSIM4wuc1",
        "BSIM4wud",
        "BSIM4wud1",
        "BSIM4wup",
        "BSIM4wlp",
        "BSIM4wu0",
        "BSIM4weu",
        "BSIM4wucs",
        "BSIM4wute",
        "BSIM4wucste",
        "BSIM4wvoff",
        "BSIM4wtvoff",
        "BSIM4wtnfactor",
        "BSIM4wteta0",
        "BSIM4wtvoffcv",
        "BSIM4wminv",
        "BSIM4wminvcv",
        "BSIM4wdelta",
        "BSIM4wrdsw",
        "BSIM4wrsw",
        "BSIM4wrdw",
        "BSIM4wprwg",
        "BSIM4wprwb",
        "BSIM4wprt",
        "BSIM4weta0",
        "BSIM4wetab",
        "BSIM4wpclm",
        "BSIM4wpdibl1",
        "BSIM4wpdibl2",
        "BSIM4wpdiblb",
        "BSIM4wfprout",
        "BSIM4wpdits",
        "BSIM4wpditsd",
        "BSIM4wpscbe1",
        "BSIM4wpscbe2",
        "BSIM4wpvag",
        "BSIM4wwr",
        "BSIM4wdwg",
        "BSIM4wdwb",
        "BSIM4wb0",
        "BSIM4wb1",
        "BSIM4walpha0",
        "BSIM4walpha1",
        "BSIM4wbeta0",
        "BSIM4wvfb",
        "BSIM4wagidl",
        "BSIM4wbgidl",
        "BSIM4wcgidl",
        "BSIM4wegidl",
        "BSIM4wfgidl",
        "BSIM4wkgidl",
        "BSIM4wrgidl",
        "BSIM4wagisl",
        "BSIM4wbgisl",
        "BSIM4wcgisl",
        "BSIM4wegisl",
        "BSIM4wfgisl",
        "BSIM4wkgisl",
        "BSIM4wrgisl",
        "BSIM4waigc",
        "BSIM4wbigc",
        "BSIM4wcigc",
        "BSIM4waigsd",
        "BSIM4wbigsd",
        "BSIM4wcigsd",
        "BSIM4waigs",
        "BSIM4wbigs",
        "BSIM4wcigs",
        "BSIM4waigd",
        "BSIM4wbigd",
        "BSIM4wcigd",
        "BSIM4waigbacc",
        "BSIM4wbigbacc",
        "BSIM4wcigbacc",
        "BSIM4waigbinv",
        "BSIM4wbigbinv",
        "BSIM4wcigbinv",
        "BSIM4wnigc",
        "BSIM4wnigbacc",
        "BSIM4wnigbinv",
        "BSIM4wntox",
        "BSIM4weigbinv",
        "BSIM4wpigcd",
        "BSIM4wpoxedge",
        "BSIM4wxrcrg1",
        "BSIM4wxrcrg2",
        "BSIM4wlambda",
        "BSIM4wvtl",
        "BSIM4wxn",
        "BSIM4wvfbsdoff",
        "BSIM4wtvfbsdoff",
        "BSIM4wcgsl",
        "BSIM4wcgdl",
        "BSIM4wckappas",
        "BSIM4wckappad",
        "BSIM4wcf",
        "BSIM4wclc",
        "BSIM4wcle",
        "BSIM4wvfbcv",
        "BSIM4wnoff",
        "BSIM4wvoffcv",
        "BSIM4wacde",
        "BSIM4wmoin",
        "BSIM4pcdsc",
        "BSIM4pcdscb",
        "BSIM4pcdscd",
        "BSIM4pcit",
        "BSIM4pnfactor",
        "BSIM4pxj",
        "BSIM4pvsat",
        "BSIM4pat",
        "BSIM4pa0",
        "BSIM4pags",
        "BSIM4pa1",
        "BSIM4pa2",
        "BSIM4pketa",
        "BSIM4pketac",
        "BSIM4pnsub",
        "BSIM4pndep",
        "BSIM4pnsd",
        "BSIM4pphin",
        "BSIM4pngate",
        "BSIM4pgamma1",
        "BSIM4pgamma2",
        "BSIM4pvbx",
        "BSIM4pvbm",
        "BSIM4pxt",
        "BSIM4pk1",
        "BSIM4pkt1",
        "BSIM4pkt1l",
        "BSIM4pkt2",
        "BSIM4pk2",
        "BSIM4pk3",
        "BSIM4pk3b",
        "BSIM4pw0",
        "BSIM4pdvtp0",
        "BSIM4pdvtp1",
        "BSIM4pdvtp2",
        "BSIM4pdvtp3",
        "BSIM4pdvtp4",
        "BSIM4pdvtp5",
        "BSIM4plpe0",
        "BSIM4plpeb",
        "BSIM4pdvt0",
        "BSIM4pdvt1",
        "BSIM4pdvt2",
        "BSIM4pdvt0w",
        "BSIM4pdvt1w",
        "BSIM4pdvt2w",
        "BSIM4pdrout",
        "BSIM4pdsub",
        "BSIM4pvth0",
        "BSIM4pua",
        "BSIM4pua1",
        "BSIM4pub",
        "BSIM4pub1",
        "BSIM4puc",
        "BSIM4puc1",
        "BSIM4pud",
        "BSIM4pud1",
        "BSIM4pup",
        "BSIM4plp",
        "BSIM4pu0",
        "BSIM4peu",
        "BSIM4pucs",
        "BSIM4pute",
        "BSIM4pucste",
        "BSIM4pvoff",
        "BSIM4ptvoff",
        "BSIM4ptnfactor",
        "BSIM4pteta0",
        "BSIM4ptvoffcv",
        "BSIM4pminv",
        "BSIM4pminvcv",
        "BSIM4pdelta",
        "BSIM4prdsw",
        "BSIM4prsw",
        "BSIM4prdw",
        "BSIM4pprwg",
        "BSIM4pprwb",
        "BSIM4pprt",
        "BSIM4peta0",
        "BSIM4petab",
        "BSIM4ppclm",
        "BSIM4ppdibl1",
        "BSIM4ppdibl2",
        "BSIM4ppdiblb",
        "BSIM4pfprout",
        "BSIM4ppdits",
        "BSIM4ppditsd",
        "BSIM4ppscbe1",
        "BSIM4ppscbe2",
        "BSIM4ppvag",
        "BSIM4pwr",
        "BSIM4pdwg",
        "BSIM4pdwb",
        "BSIM4pb0",
        "BSIM4pb1",
        "BSIM4palpha0",
        "BSIM4palpha1",
        "BSIM4pbeta0",
        "BSIM4pvfb",
        "BSIM4pagidl",
        "BSIM4pbgidl",
        "BSIM4pcgidl",
        "BSIM4pegidl",
        "BSIM4pfgidl",
        "BSIM4pkgidl",
        "BSIM4prgidl",
        "BSIM4pagisl",
        "BSIM4pbgisl",
        "BSIM4pcgisl",
        "BSIM4pegisl",
        "BSIM4pfgisl",
        "BSIM4pkgisl",
        "BSIM4prgisl",
        "BSIM4paigc",
        "BSIM4pbigc",
        "BSIM4pcigc",
        "BSIM4paigsd",
        "BSIM4pbigsd",
        "BSIM4pcigsd",
        "BSIM4paigs",
        "BSIM4pbigs",
        "BSIM4pcigs",
        "BSIM4paigd",
        "BSIM4pbigd",
        "BSIM4pcigd",
        "BSIM4paigbacc",
        "BSIM4pbigbacc",
        "BSIM4pcigbacc",
        "BSIM4paigbinv",
        "BSIM4pbigbinv",
        "BSIM4pcigbinv",
        "BSIM4pnigc",
        "BSIM4pnigbacc",
        "BSIM4pnigbinv",
        "BSIM4pntox",
        "BSIM4peigbinv",
        "BSIM4ppigcd",
        "BSIM4ppoxedge",
        "BSIM4pxrcrg1",
        "BSIM4pxrcrg2",
        "BSIM4plambda",
        "BSIM4pvtl",
        "BSIM4pxn",
        "BSIM4pvfbsdoff",
        "BSIM4ptvfbsdoff",
        "BSIM4pcgsl",
        "BSIM4pcgdl",
        "BSIM4pckappas",
        "BSIM4pckappad",
        "BSIM4pcf",
        "BSIM4pclc",
        "BSIM4pcle",
        "BSIM4pvfbcv",
        "BSIM4pnoff",
        "BSIM4pvoffcv",
        "BSIM4pacde",
        "BSIM4pmoin",
        "BSIM4tnom",
        "BSIM4cgso",
        "BSIM4cgdo",
        "BSIM4cgbo",
        "BSIM4xpart",
        "BSIM4cFringOut",
        "BSIM4cFringMax",
        "BSIM4sheetResistance",
        "BSIM4SjctSatCurDensity",
        "BSIM4DjctSatCurDensity",
        "BSIM4SjctSidewallSatCurDensity",
        "BSIM4DjctSidewallSatCurDensity",
        "BSIM4SjctGateSidewallSatCurDensity",
        "BSIM4DjctGateSidewallSatCurDensity",
        "BSIM4SbulkJctPotential",
        "BSIM4DbulkJctPotential",
        "BSIM4SbulkJctBotGradingCoeff",
        "BSIM4DbulkJctBotGradingCoeff",
        "BSIM4SbulkJctSideGradingCoeff",
        "BSIM4DbulkJctSideGradingCoeff",
        "BSIM4SbulkJctGateSideGradingCoeff",
        "BSIM4DbulkJctGateSideGradingCoeff",
        "BSIM4SsidewallJctPotential",
        "BSIM4DsidewallJctPotential",
        "BSIM4SGatesidewallJctPotential",
        "BSIM4DGatesidewallJctPotential",
        "BSIM4SunitAreaJctCap",
        "BSIM4DunitAreaJctCap",
        "BSIM4SunitLengthSidewallJctCap",
        "BSIM4DunitLengthSidewallJctCap",
        "BSIM4SunitLengthGateSidewallJctCap",
        "BSIM4DunitLengthGateSidewallJctCap",
        "BSIM4SjctEmissionCoeff",
        "BSIM4DjctEmissionCoeff",
        "BSIM4SjctTempExponent",
        "BSIM4DjctTempExponent",
        "BSIM4njtsstemp",
        "BSIM4njtsswstemp",
        "BSIM4njtsswgstemp",
        "BSIM4njtsdtemp",
        "BSIM4njtsswdtemp",
        "BSIM4njtsswgdtemp",
        "BSIM4Lint",
        "BSIM4Ll",
        "BSIM4Llc",
        "BSIM4Lln",
        "BSIM4Lw",
        "BSIM4Lwc",
        "BSIM4Lwn",
        "BSIM4Lwl",
        "BSIM4Lwlc",
        "BSIM4Lmin",
        "BSIM4Lmax",
        "BSIM4Wint",
        "BSIM4Wl",
        "BSIM4Wlc",
        "BSIM4Wln",
        "BSIM4Ww",
        "BSIM4Wwc",
        "BSIM4Wwn",
        "BSIM4Wwl",
        "BSIM4Wwlc",
        "BSIM4Wmin",
        "BSIM4Wmax",
        "BSIM4saref",
        "BSIM4sbref",
        "BSIM4wlod",
        "BSIM4ku0",
        "BSIM4kvsat",
        "BSIM4kvth0",
        "BSIM4tku0",
        "BSIM4llodku0",
        "BSIM4wlodku0",
        "BSIM4llodvth",
        "BSIM4wlodvth",
        "BSIM4lku0",
        "BSIM4wku0",
        "BSIM4pku0",
        "BSIM4lkvth0",
        "BSIM4wkvth0",
        "BSIM4pkvth0",
        "BSIM4stk2",
        "BSIM4lodk2",
        "BSIM4steta0",
        "BSIM4lodeta0",
        "BSIM4web",
        "BSIM4wec",
        "BSIM4kvth0we",
        "BSIM4k2we",
        "BSIM4ku0we",
        "BSIM4scref",
        "BSIM4wpemod",
        "BSIM4lkvth0we",
        "BSIM4lk2we",
        "BSIM4lku0we",
        "BSIM4wkvth0we",
        "BSIM4wk2we",
        "BSIM4wku0we",
        "BSIM4pkvth0we",
        "BSIM4pk2we",
        "BSIM4pku0we",
        "BSIM4Eg0",
        "BSIM4vtm",
        "BSIM4vtm0",
        "BSIM4coxe",
        "BSIM4coxp",
        "BSIM4cof1",
        "BSIM4cof2",
        "BSIM4cof3",
        "BSIM4cof4",
        "BSIM4vcrit",
        "BSIM4factor1",
        "BSIM4PhiBS",
        "BSIM4PhiBSWS",
        "BSIM4PhiBSWGS",
        "BSIM4SjctTempSatCurDensity",
        "BSIM4SjctSidewallTempSatCurDensity",
        "BSIM4SjctGateSidewallTempSatCurDensity",
        "BSIM4PhiBD",
        "BSIM4PhiBSWD",
        "BSIM4PhiBSWGD",
        "BSIM4DjctTempSatCurDensity",
        "BSIM4DjctSidewallTempSatCurDensity",
        "BSIM4DjctGateSidewallTempSatCurDensity",
        "BSIM4SunitAreaTempJctCap",
        "BSIM4DunitAreaTempJctCap",
        "BSIM4SunitLengthSidewallTempJctCap",
        "BSIM4DunitLengthSidewallTempJctCap",
        "BSIM4SunitLengthGateSidewallTempJctCap",
        "BSIM4DunitLengthGateSidewallTempJctCap",
        "BSIM4oxideTrapDensityA",
        "BSIM4oxideTrapDensityB",
        "BSIM4oxideTrapDensityC",
        "BSIM4em",
        "BSIM4ef",
        "BSIM4af",
        "BSIM4kf",
        "BSIM4vgsMax",
        "BSIM4vgdMax",
        "BSIM4vgbMax",
        "BSIM4vdsMax",
        "BSIM4vbsMax",
        "BSIM4vbdMax",
        "BSIM4vgsrMax",
        "BSIM4vgdrMax",
        "BSIM4vgbrMax",
        "BSIM4vbsrMax",
        "BSIM4vbdrMax",
        "BSIM4gidlclamp",
        "BSIM4idovvdsc",
        "BSIM4InstCount",
        "BSIM4mobModGiven",
        "BSIM4binUnitGiven",
        "BSIM4cvchargeModGiven",
        "BSIM4capModGiven",
        "BSIM4dioModGiven",
        "BSIM4rdsModGiven",
        "BSIM4rbodyModGiven",
        "BSIM4rgateModGiven",
        "BSIM4perModGiven",
        "BSIM4geoModGiven",
        "BSIM4paramChkGiven",
        "BSIM4trnqsModGiven",
        "BSIM4acnqsModGiven",
        "BSIM4fnoiModGiven",
        "BSIM4tnoiModGiven",
        "BSIM4mtrlModGiven",
        "BSIM4mtrlCompatModGiven",
        "BSIM4gidlModGiven",
        "BSIM4igcModGiven",
        "BSIM4igbModGiven",
        "BSIM4tempModGiven",
        "BSIM4typeGiven",
        "BSIM4toxrefGiven",
        "BSIM4eotGiven",
        "BSIM4vddeotGiven",
        "BSIM4tempeotGiven",
        "BSIM4leffeotGiven",
        "BSIM4weffeotGiven",
        "BSIM4adosGiven",
        "BSIM4bdosGiven",
        "BSIM4toxeGiven",
        "BSIM4toxpGiven",
        "BSIM4toxmGiven",
        "BSIM4dtoxGiven",
        "BSIM4epsroxGiven",
        "BSIM4versionGiven",
        "BSIM4cdscGiven",
        "BSIM4cdscbGiven",
        "BSIM4cdscdGiven",
        "BSIM4citGiven",
        "BSIM4nfactorGiven",
        "BSIM4xjGiven",
        "BSIM4vsatGiven",
        "BSIM4atGiven",
        "BSIM4a0Given",
        "BSIM4agsGiven",
        "BSIM4a1Given",
        "BSIM4a2Given",
        "BSIM4ketaGiven",
        "BSIM4ketacGiven",
        "BSIM4nsubGiven",
        "BSIM4phigGiven",
        "BSIM4epsrgateGiven",
        "BSIM4easubGiven",
        "BSIM4epsrsubGiven",
        "BSIM4ni0subGiven",
        "BSIM4bg0subGiven",
        "BSIM4tbgasubGiven",
        "BSIM4tbgbsubGiven",
        "BSIM4ndepGiven",
        "BSIM4nsdGiven",
        "BSIM4phinGiven",
        "BSIM4ngateGiven",
        "BSIM4gamma1Given",
        "BSIM4gamma2Given",
        "BSIM4vbxGiven",
        "BSIM4vbmGiven",
        "BSIM4xtGiven",
        "BSIM4k1Given",
        "BSIM4kt1Given",
        "BSIM4kt1lGiven",
        "BSIM4kt2Given",
        "BSIM4k2Given",
        "BSIM4k3Given",
        "BSIM4k3bGiven",
        "BSIM4w0Given",
        "BSIM4dvtp0Given",
        "BSIM4dvtp1Given",
        "BSIM4dvtp2Given",
        "BSIM4dvtp3Given",
        "BSIM4dvtp4Given",
        "BSIM4dvtp5Given",
        "BSIM4lpe0Given",
        "BSIM4lpebGiven",
        "BSIM4dvt0Given",
        "BSIM4dvt1Given",
        "BSIM4dvt2Given",
        "BSIM4dvt0wGiven",
        "BSIM4dvt1wGiven",
        "BSIM4dvt2wGiven",
        "BSIM4droutGiven",
        "BSIM4dsubGiven",
        "BSIM4vth0Given",
        "BSIM4euGiven",
        "BSIM4ucsGiven",
        "BSIM4uaGiven",
        "BSIM4ua1Given",
        "BSIM4ubGiven",
        "BSIM4ub1Given",
        "BSIM4ucGiven",
        "BSIM4uc1Given",
        "BSIM4udGiven",
        "BSIM4ud1Given",
        "BSIM4upGiven",
        "BSIM4lpGiven",
        "BSIM4u0Given",
        "BSIM4uteGiven",
        "BSIM4ucsteGiven",
        "BSIM4voffGiven",
        "BSIM4tvoffGiven",
        "BSIM4tnfactorGiven",
        "BSIM4teta0Given",
        "BSIM4tvoffcvGiven",
        "BSIM4vofflGiven",
        "BSIM4voffcvlGiven",
        "BSIM4minvGiven",
        "BSIM4minvcvGiven",
        "BSIM4rdswGiven",
        "BSIM4rdswminGiven",
        "BSIM4rdwminGiven",
        "BSIM4rswminGiven",
        "BSIM4rswGiven",
        "BSIM4rdwGiven",
        "BSIM4prwgGiven",
        "BSIM4prwbGiven",
        "BSIM4prtGiven",
        "BSIM4eta0Given",
        "BSIM4etabGiven",
        "BSIM4pclmGiven",
        "BSIM4pdibl1Given",
        "BSIM4pdibl2Given",
        "BSIM4pdiblbGiven",
        "BSIM4fproutGiven",
        "BSIM4pditsGiven",
        "BSIM4pditsdGiven",
        "BSIM4pditslGiven",
        "BSIM4pscbe1Given",
        "BSIM4pscbe2Given",
        "BSIM4pvagGiven",
        "BSIM4deltaGiven",
        "BSIM4wrGiven",
        "BSIM4dwgGiven",
        "BSIM4dwbGiven",
        "BSIM4b0Given",
        "BSIM4b1Given",
        "BSIM4alpha0Given",
        "BSIM4alpha1Given",
        "BSIM4beta0Given",
        "BSIM4agidlGiven",
        "BSIM4bgidlGiven",
        "BSIM4cgidlGiven",
        "BSIM4egidlGiven",
        "BSIM4fgidlGiven",
        "BSIM4kgidlGiven",
        "BSIM4rgidlGiven",
        "BSIM4agislGiven",
        "BSIM4bgislGiven",
        "BSIM4cgislGiven",
        "BSIM4egislGiven",
        "BSIM4fgislGiven",
        "BSIM4kgislGiven",
        "BSIM4rgislGiven",
        "BSIM4aigcGiven",
        "BSIM4bigcGiven",
        "BSIM4cigcGiven",
        "BSIM4aigsdGiven",
        "BSIM4bigsdGiven",
        "BSIM4cigsdGiven",
        "BSIM4aigsGiven",
        "BSIM4bigsGiven",
        "BSIM4cigsGiven",
        "BSIM4aigdGiven",
        "BSIM4bigdGiven",
        "BSIM4cigdGiven",
        "BSIM4aigbaccGiven",
        "BSIM4bigbaccGiven",
        "BSIM4cigbaccGiven",
        "BSIM4aigbinvGiven",
        "BSIM4bigbinvGiven",
        "BSIM4cigbinvGiven",
        "BSIM4nigcGiven",
        "BSIM4nigbinvGiven",
        "BSIM4nigbaccGiven",
        "BSIM4ntoxGiven",
        "BSIM4eigbinvGiven",
        "BSIM4pigcdGiven",
        "BSIM4poxedgeGiven",
        "BSIM4ijthdfwdGiven",
        "BSIM4ijthsfwdGiven",
        "BSIM4ijthdrevGiven",
        "BSIM4ijthsrevGiven",
        "BSIM4xjbvdGiven",
        "BSIM4xjbvsGiven",
        "BSIM4bvdGiven",
        "BSIM4bvsGiven",
        "BSIM4jtssGiven",
        "BSIM4jtsdGiven",
        "BSIM4jtsswsGiven",
        "BSIM4jtsswdGiven",
        "BSIM4jtsswgsGiven",
        "BSIM4jtsswgdGiven",
        "BSIM4jtweffGiven",
        "BSIM4njtsGiven",
        "BSIM4njtsswGiven",
        "BSIM4njtsswgGiven",
        "BSIM4njtsdGiven",
        "BSIM4njtsswdGiven",
        "BSIM4njtsswgdGiven",
        "BSIM4xtssGiven",
        "BSIM4xtsdGiven",
        "BSIM4xtsswsGiven",
        "BSIM4xtsswdGiven",
        "BSIM4xtsswgsGiven",
        "BSIM4xtsswgdGiven",
        "BSIM4tnjtsGiven",
        "BSIM4tnjtsswGiven",
        "BSIM4tnjtsswgGiven",
        "BSIM4tnjtsdGiven",
        "BSIM4tnjtsswdGiven",
        "BSIM4tnjtsswgdGiven",
        "BSIM4vtssGiven",
        "BSIM4vtsdGiven",
        "BSIM4vtsswsGiven",
        "BSIM4vtsswdGiven",
        "BSIM4vtsswgsGiven",
        "BSIM4vtsswgdGiven",
        "BSIM4vfbGiven",
        "BSIM4gbminGiven",
        "BSIM4rbdbGiven",
        "BSIM4rbsbGiven",
        "BSIM4rbpsGiven",
        "BSIM4rbpdGiven",
        "BSIM4rbpbGiven",
        "BSIM4rbps0Given",
        "BSIM4rbpslGiven",
        "BSIM4rbpswGiven",
        "BSIM4rbpsnfGiven",
        "BSIM4rbpd0Given",
        "BSIM4rbpdlGiven",
        "BSIM4rbpdwGiven",
        "BSIM4rbpdnfGiven",
        "BSIM4rbpbx0Given",
        "BSIM4rbpbxlGiven",
        "BSIM4rbpbxwGiven",
        "BSIM4rbpbxnfGiven",
        "BSIM4rbpby0Given",
        "BSIM4rbpbylGiven",
        "BSIM4rbpbywGiven",
        "BSIM4rbpbynfGiven",
        "BSIM4rbsbx0Given",
        "BSIM4rbsby0Given",
        "BSIM4rbdbx0Given",
        "BSIM4rbdby0Given",
        "BSIM4rbsdbxlGiven",
        "BSIM4rbsdbxwGiven",
        "BSIM4rbsdbxnfGiven",
        "BSIM4rbsdbylGiven",
        "BSIM4rbsdbywGiven",
        "BSIM4rbsdbynfGiven",
        "BSIM4xrcrg1Given",
        "BSIM4xrcrg2Given",
        "BSIM4tnoiaGiven",
        "BSIM4tnoibGiven",
        "BSIM4tnoicGiven",
        "BSIM4rnoiaGiven",
        "BSIM4rnoibGiven",
        "BSIM4rnoicGiven",
        "BSIM4ntnoiGiven",
        "BSIM4lambdaGiven",
        "BSIM4vtlGiven",
        "BSIM4lcGiven",
        "BSIM4xnGiven",
        "BSIM4vfbsdoffGiven",
        "BSIM4lintnoiGiven",
        "BSIM4tvfbsdoffGiven",
        "BSIM4cgslGiven",
        "BSIM4cgdlGiven",
        "BSIM4ckappasGiven",
        "BSIM4ckappadGiven",
        "BSIM4cfGiven",
        "BSIM4vfbcvGiven",
        "BSIM4clcGiven",
        "BSIM4cleGiven",
        "BSIM4dwcGiven",
        "BSIM4dlcGiven",
        "BSIM4xwGiven",
        "BSIM4xlGiven",
        "BSIM4dlcigGiven",
        "BSIM4dlcigdGiven",
        "BSIM4dwjGiven",
        "BSIM4noffGiven",
        "BSIM4voffcvGiven",
        "BSIM4acdeGiven",
        "BSIM4moinGiven",
        "BSIM4tcjGiven",
        "BSIM4tcjswGiven",
        "BSIM4tcjswgGiven",
        "BSIM4tpbGiven",
        "BSIM4tpbswGiven",
        "BSIM4tpbswgGiven",
        "BSIM4dmcgGiven",
        "BSIM4dmciGiven",
        "BSIM4dmdgGiven",
        "BSIM4dmcgtGiven",
        "BSIM4xgwGiven",
        "BSIM4xglGiven",
        "BSIM4rshgGiven",
        "BSIM4ngconGiven",
        "BSIM4lcdscGiven",
        "BSIM4lcdscbGiven",
        "BSIM4lcdscdGiven",
        "BSIM4lcitGiven",
        "BSIM4lnfactorGiven",
        "BSIM4lxjGiven",
        "BSIM4lvsatGiven",
        "BSIM4latGiven",
        "BSIM4la0Given",
        "BSIM4lagsGiven",
        "BSIM4la1Given",
        "BSIM4la2Given",
        "BSIM4lketaGiven",
        "BSIM4lketacGiven",
        "BSIM4lnsubGiven",
        "BSIM4lndepGiven",
        "BSIM4lnsdGiven",
        "BSIM4lphinGiven",
        "BSIM4lngateGiven",
        "BSIM4lgamma1Given",
        "BSIM4lgamma2Given",
        "BSIM4lvbxGiven",
        "BSIM4lvbmGiven",
        "BSIM4lxtGiven",
        "BSIM4lk1Given",
        "BSIM4lkt1Given",
        "BSIM4lkt1lGiven",
        "BSIM4lkt2Given",
        "BSIM4lk2Given",
        "BSIM4lk3Given",
        "BSIM4lk3bGiven",
        "BSIM4lw0Given",
        "BSIM4ldvtp0Given",
        "BSIM4ldvtp1Given",
        "BSIM4ldvtp2Given",
        "BSIM4ldvtp3Given",
        "BSIM4ldvtp4Given",
        "BSIM4ldvtp5Given",
        "BSIM4llpe0Given",
        "BSIM4llpebGiven",
        "BSIM4ldvt0Given",
        "BSIM4ldvt1Given",
        "BSIM4ldvt2Given",
        "BSIM4ldvt0wGiven",
        "BSIM4ldvt1wGiven",
        "BSIM4ldvt2wGiven",
        "BSIM4ldroutGiven",
        "BSIM4ldsubGiven",
        "BSIM4lvth0Given",
        "BSIM4luaGiven",
        "BSIM4lua1Given",
        "BSIM4lubGiven",
        "BSIM4lub1Given",
        "BSIM4lucGiven",
        "BSIM4luc1Given",
        "BSIM4ludGiven",
        "BSIM4lud1Given",
        "BSIM4lupGiven",
        "BSIM4llpGiven",
        "BSIM4lu0Given",
        "BSIM4leuGiven",
        "BSIM4lucsGiven",
        "BSIM4luteGiven",
        "BSIM4lucsteGiven",
        "BSIM4lvoffGiven",
        "BSIM4ltvoffGiven",
        "BSIM4ltnfactorGiven",
        "BSIM4lteta0Given",
        "BSIM4ltvoffcvGiven",
        "BSIM4lminvGiven",
        "BSIM4lminvcvGiven",
        "BSIM4lrdswGiven",
        "BSIM4lrswGiven",
        "BSIM4lrdwGiven",
        "BSIM4lprwgGiven",
        "BSIM4lprwbGiven",
        "BSIM4lprtGiven",
        "BSIM4leta0Given",
        "BSIM4letabGiven",
        "BSIM4lpclmGiven",
        "BSIM4lpdibl1Given",
        "BSIM4lpdibl2Given",
        "BSIM4lpdiblbGiven",
        "BSIM4lfproutGiven",
        "BSIM4lpditsGiven",
        "BSIM4lpditsdGiven",
        "BSIM4lpscbe1Given",
        "BSIM4lpscbe2Given",
        "BSIM4lpvagGiven",
        "BSIM4ldeltaGiven",
        "BSIM4lwrGiven",
        "BSIM4ldwgGiven",
        "BSIM4ldwbGiven",
        "BSIM4lb0Given",
        "BSIM4lb1Given",
        "BSIM4lalpha0Given",
        "BSIM4lalpha1Given",
        "BSIM4lbeta0Given",
        "BSIM4lvfbGiven",
        "BSIM4lagidlGiven",
        "BSIM4lbgidlGiven",
        "BSIM4lcgidlGiven",
        "BSIM4legidlGiven",
        "BSIM4lfgidlGiven",
        "BSIM4lkgidlGiven",
        "BSIM4lrgidlGiven",
        "BSIM4lagislGiven",
        "BSIM4lbgislGiven",
        "BSIM4lcgislGiven",
        "BSIM4legislGiven",
        "BSIM4lfgislGiven",
        "BSIM4lkgislGiven",
        "BSIM4lrgislGiven",
        "BSIM4laigcGiven",
        "BSIM4lbigcGiven",
        "BSIM4lcigcGiven",
        "BSIM4laigsdGiven",
        "BSIM4lbigsdGiven",
        "BSIM4lcigsdGiven",
        "BSIM4laigsGiven",
        "BSIM4lbigsGiven",
        "BSIM4lcigsGiven",
        "BSIM4laigdGiven",
        "BSIM4lbigdGiven",
        "BSIM4lcigdGiven",
        "BSIM4laigbaccGiven",
        "BSIM4lbigbaccGiven",
        "BSIM4lcigbaccGiven",
        "BSIM4laigbinvGiven",
        "BSIM4lbigbinvGiven",
        "BSIM4lcigbinvGiven",
        "BSIM4lnigcGiven",
        "BSIM4lnigbinvGiven",
        "BSIM4lnigbaccGiven",
        "BSIM4lntoxGiven",
        "BSIM4leigbinvGiven",
        "BSIM4lpigcdGiven",
        "BSIM4lpoxedgeGiven",
        "BSIM4lxrcrg1Given",
        "BSIM4lxrcrg2Given",
        "BSIM4llambdaGiven",
        "BSIM4lvtlGiven",
        "BSIM4lxnGiven",
        "BSIM4lvfbsdoffGiven",
        "BSIM4ltvfbsdoffGiven",
        "BSIM4lcgslGiven",
        "BSIM4lcgdlGiven",
        "BSIM4lckappasGiven",
        "BSIM4lckappadGiven",
        "BSIM4lcfGiven",
        "BSIM4lclcGiven",
        "BSIM4lcleGiven",
        "BSIM4lvfbcvGiven",
        "BSIM4lnoffGiven",
        "BSIM4lvoffcvGiven",
        "BSIM4lacdeGiven",
        "BSIM4lmoinGiven",
        "BSIM4wcdscGiven",
        "BSIM4wcdscbGiven",
        "BSIM4wcdscdGiven",
        "BSIM4wcitGiven",
        "BSIM4wnfactorGiven",
        "BSIM4wxjGiven",
        "BSIM4wvsatGiven",
        "BSIM4watGiven",
        "BSIM4wa0Given",
        "BSIM4wagsGiven",
        "BSIM4wa1Given",
        "BSIM4wa2Given",
        "BSIM4wketaGiven",
        "BSIM4wketacGiven",
        "BSIM4wnsubGiven",
        "BSIM4wndepGiven",
        "BSIM4wnsdGiven",
        "BSIM4wphinGiven",
        "BSIM4wngateGiven",
        "BSIM4wgamma1Given",
        "BSIM4wgamma2Given",
        "BSIM4wvbxGiven",
        "BSIM4wvbmGiven",
        "BSIM4wxtGiven",
        "BSIM4wk1Given",
        "BSIM4wkt1Given",
        "BSIM4wkt1lGiven",
        "BSIM4wkt2Given",
        "BSIM4wk2Given",
        "BSIM4wk3Given",
        "BSIM4wk3bGiven",
        "BSIM4ww0Given",
        "BSIM4wdvtp0Given",
        "BSIM4wdvtp1Given",
        "BSIM4wdvtp2Given",
        "BSIM4wdvtp3Given",
        "BSIM4wdvtp4Given",
        "BSIM4wdvtp5Given",
        "BSIM4wlpe0Given",
        "BSIM4wlpebGiven",
        "BSIM4wdvt0Given",
        "BSIM4wdvt1Given",
        "BSIM4wdvt2Given",
        "BSIM4wdvt0wGiven",
        "BSIM4wdvt1wGiven",
        "BSIM4wdvt2wGiven",
        "BSIM4wdroutGiven",
        "BSIM4wdsubGiven",
        "BSIM4wvth0Given",
        "BSIM4wuaGiven",
        "BSIM4wua1Given",
        "BSIM4wubGiven",
        "BSIM4wub1Given",
        "BSIM4wucGiven",
        "BSIM4wuc1Given",
        "BSIM4wudGiven",
        "BSIM4wud1Given",
        "BSIM4wupGiven",
        "BSIM4wlpGiven",
        "BSIM4wu0Given",
        "BSIM4weuGiven",
        "BSIM4wucsGiven",
        "BSIM4wuteGiven",
        "BSIM4wucsteGiven",
        "BSIM4wvoffGiven",
        "BSIM4wtvoffGiven",
        "BSIM4wtnfactorGiven",
        "BSIM4wteta0Given",
        "BSIM4wtvoffcvGiven",
        "BSIM4wminvGiven",
        "BSIM4wminvcvGiven",
        "BSIM4wrdswGiven",
        "BSIM4wrswGiven",
        "BSIM4wrdwGiven",
        "BSIM4wprwgGiven",
        "BSIM4wprwbGiven",
        "BSIM4wprtGiven",
        "BSIM4weta0Given",
        "BSIM4wetabGiven",
        "BSIM4wpclmGiven",
        "BSIM4wpdibl1Given",
        "BSIM4wpdibl2Given",
        "BSIM4wpdiblbGiven",
        "BSIM4wfproutGiven",
        "BSIM4wpditsGiven",
        "BSIM4wpditsdGiven",
        "BSIM4wpscbe1Given",
        "BSIM4wpscbe2Given",
        "BSIM4wpvagGiven",
        "BSIM4wdeltaGiven",
        "BSIM4wwrGiven",
        "BSIM4wdwgGiven",
        "BSIM4wdwbGiven",
        "BSIM4wb0Given",
        "BSIM4wb1Given",
        "BSIM4walpha0Given",
        "BSIM4walpha1Given",
        "BSIM4wbeta0Given",
        "BSIM4wvfbGiven",
        "BSIM4wagidlGiven",
        "BSIM4wbgidlGiven",
        "BSIM4wcgidlGiven",
        "BSIM4wegidlGiven",
        "BSIM4wfgidlGiven",
        "BSIM4wkgidlGiven",
        "BSIM4wrgidlGiven",
        "BSIM4wagislGiven",
        "BSIM4wbgislGiven",
        "BSIM4wcgislGiven",
        "BSIM4wegislGiven",
        "BSIM4wfgislGiven",
        "BSIM4wkgislGiven",
        "BSIM4wrgislGiven",
        "BSIM4waigcGiven",
        "BSIM4wbigcGiven",
        "BSIM4wcigcGiven",
        "BSIM4waigsdGiven",
        "BSIM4wbigsdGiven",
        "BSIM4wcigsdGiven",
        "BSIM4waigsGiven",
        "BSIM4wbigsGiven",
        "BSIM4wcigsGiven",
        "BSIM4waigdGiven",
        "BSIM4wbigdGiven",
        "BSIM4wcigdGiven",
        "BSIM4waigbaccGiven",
        "BSIM4wbigbaccGiven",
        "BSIM4wcigbaccGiven",
        "BSIM4waigbinvGiven",
        "BSIM4wbigbinvGiven",
        "BSIM4wcigbinvGiven",
        "BSIM4wnigcGiven",
        "BSIM4wnigbinvGiven",
        "BSIM4wnigbaccGiven",
        "BSIM4wntoxGiven",
        "BSIM4weigbinvGiven",
        "BSIM4wpigcdGiven",
        "BSIM4wpoxedgeGiven",
        "BSIM4wxrcrg1Given",
        "BSIM4wxrcrg2Given",
        "BSIM4wlambdaGiven",
        "BSIM4wvtlGiven",
        "BSIM4wxnGiven",
        "BSIM4wvfbsdoffGiven",
        "BSIM4wtvfbsdoffGiven",
        "BSIM4wcgslGiven",
        "BSIM4wcgdlGiven",
        "BSIM4wckappasGiven",
        "BSIM4wckappadGiven",
        "BSIM4wcfGiven",
        "BSIM4wclcGiven",
        "BSIM4wcleGiven",
        "BSIM4wvfbcvGiven",
        "BSIM4wnoffGiven",
        "BSIM4wvoffcvGiven",
        "BSIM4wacdeGiven",
        "BSIM4wmoinGiven",
        "BSIM4pcdscGiven",
        "BSIM4pcdscbGiven",
        "BSIM4pcdscdGiven",
        "BSIM4pcitGiven",
        "BSIM4pnfactorGiven",
        "BSIM4pxjGiven",
        "BSIM4pvsatGiven",
        "BSIM4patGiven",
        "BSIM4pa0Given",
        "BSIM4pagsGiven",
        "BSIM4pa1Given",
        "BSIM4pa2Given",
        "BSIM4pketaGiven",
        "BSIM4pketacGiven",
        "BSIM4pnsubGiven",
        "BSIM4pndepGiven",
        "BSIM4pnsdGiven",
        "BSIM4pphinGiven",
        "BSIM4pngateGiven",
        "BSIM4pgamma1Given",
        "BSIM4pgamma2Given",
        "BSIM4pvbxGiven",
        "BSIM4pvbmGiven",
        "BSIM4pxtGiven",
        "BSIM4pk1Given",
        "BSIM4pkt1Given",
        "BSIM4pkt1lGiven",
        "BSIM4pkt2Given",
        "BSIM4pk2Given",
        "BSIM4pk3Given",
        "BSIM4pk3bGiven",
        "BSIM4pw0Given",
        "BSIM4pdvtp0Given",
        "BSIM4pdvtp1Given",
        "BSIM4pdvtp2Given",
        "BSIM4pdvtp3Given",
        "BSIM4pdvtp4Given",
        "BSIM4pdvtp5Given",
        "BSIM4plpe0Given",
        "BSIM4plpebGiven",
        "BSIM4pdvt0Given",
        "BSIM4pdvt1Given",
        "BSIM4pdvt2Given",
        "BSIM4pdvt0wGiven",
        "BSIM4pdvt1wGiven",
        "BSIM4pdvt2wGiven",
        "BSIM4pdroutGiven",
        "BSIM4pdsubGiven",
        "BSIM4pvth0Given",
        "BSIM4puaGiven",
        "BSIM4pua1Given",
        "BSIM4pubGiven",
        "BSIM4pub1Given",
        "BSIM4pucGiven",
        "BSIM4puc1Given",
        "BSIM4pudGiven",
        "BSIM4pud1Given",
        "BSIM4pupGiven",
        "BSIM4plpGiven",
        "BSIM4pu0Given",
        "BSIM4peuGiven",
        "BSIM4pucsGiven",
        "BSIM4puteGiven",
        "BSIM4pucsteGiven",
        "BSIM4pvoffGiven",
        "BSIM4ptvoffGiven",
        "BSIM4ptnfactorGiven",
        "BSIM4pteta0Given",
        "BSIM4ptvoffcvGiven",
        "BSIM4pminvGiven",
        "BSIM4pminvcvGiven",
        "BSIM4prdswGiven",
        "BSIM4prswGiven",
        "BSIM4prdwGiven",
        "BSIM4pprwgGiven",
        "BSIM4pprwbGiven",
        "BSIM4pprtGiven",
        "BSIM4peta0Given",
        "BSIM4petabGiven",
        "BSIM4ppclmGiven",
        "BSIM4ppdibl1Given",
        "BSIM4ppdibl2Given",
        "BSIM4ppdiblbGiven",
        "BSIM4pfproutGiven",
        "BSIM4ppditsGiven",
        "BSIM4ppditsdGiven",
        "BSIM4ppscbe1Given",
        "BSIM4ppscbe2Given",
        "BSIM4ppvagGiven",
        "BSIM4pdeltaGiven",
        "BSIM4pwrGiven",
        "BSIM4pdwgGiven",
        "BSIM4pdwbGiven",
        "BSIM4pb0Given",
        "BSIM4pb1Given",
        "BSIM4palpha0Given",
        "BSIM4palpha1Given",
        "BSIM4pbeta0Given",
        "BSIM4pvfbGiven",
        "BSIM4pagidlGiven",
        "BSIM4pbgidlGiven",
        "BSIM4pcgidlGiven",
        "BSIM4pegidlGiven",
        "BSIM4pfgidlGiven",
        "BSIM4pkgidlGiven",
        "BSIM4prgidlGiven",
        "BSIM4pagislGiven",
        "BSIM4pbgislGiven",
        "BSIM4pcgislGiven",
        "BSIM4pegislGiven",
        "BSIM4pfgislGiven",
        "BSIM4pkgislGiven",
        "BSIM4prgislGiven",
        "BSIM4paigcGiven",
        "BSIM4pbigcGiven",
        "BSIM4pcigcGiven",
        "BSIM4paigsdGiven",
        "BSIM4pbigsdGiven",
        "BSIM4pcigsdGiven",
        "BSIM4paigsGiven",
        "BSIM4pbigsGiven",
        "BSIM4pcigsGiven",
        "BSIM4paigdGiven",
        "BSIM4pbigdGiven",
        "BSIM4pcigdGiven",
        "BSIM4paigbaccGiven",
        "BSIM4pbigbaccGiven",
        "BSIM4pcigbaccGiven",
        "BSIM4paigbinvGiven",
        "BSIM4pbigbinvGiven",
        "BSIM4pcigbinvGiven",
        "BSIM4pnigcGiven",
        "BSIM4pnigbinvGiven",
        "BSIM4pnigbaccGiven",
        "BSIM4pntoxGiven",
        "BSIM4peigbinvGiven",
        "BSIM4ppigcdGiven",
        "BSIM4ppoxedgeGiven",
        "BSIM4pxrcrg1Given",
        "BSIM4pxrcrg2Given",
        "BSIM4plambdaGiven",
        "BSIM4pvtlGiven",
        "BSIM4pxnGiven",
        "BSIM4pvfbsdoffGiven",
        "BSIM4ptvfbsdoffGiven",
        "BSIM4pcgslGiven",
        "BSIM4pcgdlGiven",
        "BSIM4pckappasGiven",
        "BSIM4pckappadGiven",
        "BSIM4pcfGiven",
        "BSIM4pclcGiven",
        "BSIM4pcleGiven",
        "BSIM4pvfbcvGiven",
        "BSIM4pnoffGiven",
        "BSIM4pvoffcvGiven",
        "BSIM4pacdeGiven",
        "BSIM4pmoinGiven",
        "BSIM4useFringeGiven",
        "BSIM4tnomGiven",
        "BSIM4cgsoGiven",
        "BSIM4cgdoGiven",
        "BSIM4cgboGiven",
        "BSIM4xpartGiven",
        "BSIM4sheetResistanceGiven",
        "BSIM4SjctSatCurDensityGiven",
        "BSIM4SjctSidewallSatCurDensityGiven",
        "BSIM4SjctGateSidewallSatCurDensityGiven",
        "BSIM4SbulkJctPotentialGiven",
        "BSIM4SbulkJctBotGradingCoeffGiven",
        "BSIM4SsidewallJctPotentialGiven",
        "BSIM4SGatesidewallJctPotentialGiven",
        "BSIM4SbulkJctSideGradingCoeffGiven",
        "BSIM4SunitAreaJctCapGiven",
        "BSIM4SunitLengthSidewallJctCapGiven",
        "BSIM4SbulkJctGateSideGradingCoeffGiven",
        "BSIM4SunitLengthGateSidewallJctCapGiven",
        "BSIM4SjctEmissionCoeffGiven",
        "BSIM4SjctTempExponentGiven",
        "BSIM4DjctSatCurDensityGiven",
        "BSIM4DjctSidewallSatCurDensityGiven",
        "BSIM4DjctGateSidewallSatCurDensityGiven",
        "BSIM4DbulkJctPotentialGiven",
        "BSIM4DbulkJctBotGradingCoeffGiven",
        "BSIM4DsidewallJctPotentialGiven",
        "BSIM4DGatesidewallJctPotentialGiven",
        "BSIM4DbulkJctSideGradingCoeffGiven",
        "BSIM4DunitAreaJctCapGiven",
        "BSIM4DunitLengthSidewallJctCapGiven",
        "BSIM4DbulkJctGateSideGradingCoeffGiven",
        "BSIM4DunitLengthGateSidewallJctCapGiven",
        "BSIM4DjctEmissionCoeffGiven",
        "BSIM4DjctTempExponentGiven",
        "BSIM4oxideTrapDensityAGiven",
        "BSIM4oxideTrapDensityBGiven",
        "BSIM4oxideTrapDensityCGiven",
        "BSIM4emGiven",
        "BSIM4efGiven",
        "BSIM4afGiven",
        "BSIM4kfGiven",
        "BSIM4vgsMaxGiven",
        "BSIM4vgdMaxGiven",
        "BSIM4vgbMaxGiven",
        "BSIM4vdsMaxGiven",
        "BSIM4vbsMaxGiven",
        "BSIM4vbdMaxGiven",
        "BSIM4vgsrMaxGiven",
        "BSIM4vgdrMaxGiven",
        "BSIM4vgbrMaxGiven",
        "BSIM4vbsrMaxGiven",
        "BSIM4vbdrMaxGiven",
        "BSIM4LintGiven",
        "BSIM4LlGiven",
        "BSIM4LlcGiven",
        "BSIM4LlnGiven",
        "BSIM4LwGiven",
        "BSIM4LwcGiven",
        "BSIM4LwnGiven",
        "BSIM4LwlGiven",
        "BSIM4LwlcGiven",
        "BSIM4LminGiven",
        "BSIM4LmaxGiven",
        "BSIM4WintGiven",
        "BSIM4WlGiven",
        "BSIM4WlcGiven",
        "BSIM4WlnGiven",
        "BSIM4WwGiven",
        "BSIM4WwcGiven",
        "BSIM4WwnGiven",
        "BSIM4WwlGiven",
        "BSIM4WwlcGiven",
        "BSIM4WminGiven",
        "BSIM4WmaxGiven",
        "BSIM4sarefGiven",
        "BSIM4sbrefGiven",
        "BSIM4wlodGiven",
        "BSIM4ku0Given",
        "BSIM4kvsatGiven",
        "BSIM4kvth0Given",
        "BSIM4tku0Given",
        "BSIM4llodku0Given",
        "BSIM4wlodku0Given",
        "BSIM4llodvthGiven",
        "BSIM4wlodvthGiven",
        "BSIM4lku0Given",
        "BSIM4wku0Given",
        "BSIM4pku0Given",
        "BSIM4lkvth0Given",
        "BSIM4wkvth0Given",
        "BSIM4pkvth0Given",
        "BSIM4stk2Given",
        "BSIM4lodk2Given",
        "BSIM4steta0Given",
        "BSIM4lodeta0Given",
        "BSIM4webGiven",
        "BSIM4wecGiven",
        "BSIM4kvth0weGiven",
        "BSIM4k2weGiven",
        "BSIM4ku0weGiven",
        "BSIM4screfGiven",
        "BSIM4wpemodGiven",
        "BSIM4lkvth0weGiven",
        "BSIM4lk2weGiven",
        "BSIM4lku0weGiven",
        "BSIM4wkvth0weGiven",
        "BSIM4wk2weGiven",
        "BSIM4wku0weGiven",
        "BSIM4pkvth0weGiven",
        "BSIM4pk2weGiven",
        "BSIM4pku0weGiven",
        "BSIM4gidlclampGiven",
        "BSIM4idovvdscGiven",
    ];

    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,
    /// a given bit as 0 or 1).
    #[allow(dead_code)]
    pub(crate) fn to_values(&self) -> Vec<f64> {
        vec![
            f64::from(self.r#type),
            f64::from(self.mobMod),
            f64::from(self.cvchargeMod),
            f64::from(self.capMod),
            f64::from(self.dioMod),
            f64::from(self.trnqsMod),
            f64::from(self.acnqsMod),
            f64::from(self.fnoiMod),
            f64::from(self.tnoiMod),
            f64::from(self.rdsMod),
            f64::from(self.rbodyMod),
            f64::from(self.rgateMod),
            f64::from(self.perMod),
            f64::from(self.geoMod),
            f64::from(self.mtrlMod),
            f64::from(self.mtrlCompatMod),
            f64::from(self.gidlMod),
            f64::from(self.igcMod),
            f64::from(self.igbMod),
            f64::from(self.tempMod),
            f64::from(self.binUnit),
            f64::from(self.paramChk),
            f64::from(self.v48intVersion),
            self.eot,
            self.vddeot,
            self.tempeot,
            self.leffeot,
            self.weffeot,
            self.ados,
            self.bdos,
            self.toxe,
            self.toxp,
            self.toxm,
            self.dtox,
            self.epsrox,
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
            self.ketac,
            self.nsub,
            self.phig,
            self.epsrgate,
            self.easub,
            self.epsrsub,
            self.ni0sub,
            self.bg0sub,
            self.tbgasub,
            self.tbgbsub,
            self.ndep,
            self.nsd,
            self.phin,
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
            self.dvtp0,
            self.dvtp1,
            self.dvtp2,
            self.dvtp3,
            self.dvtp4,
            self.dvtp5,
            self.lpe0,
            self.lpeb,
            self.dvt0,
            self.dvt1,
            self.dvt2,
            self.dvt0w,
            self.dvt1w,
            self.dvt2w,
            self.drout,
            self.dsub,
            self.vth0,
            self.eu,
            self.ucs,
            self.ua,
            self.ua1,
            self.ub,
            self.ub1,
            self.uc,
            self.uc1,
            self.ud,
            self.ud1,
            self.up,
            self.lp,
            self.u0,
            self.ute,
            self.ucste,
            self.voff,
            self.tvoff,
            self.tnfactor,
            self.teta0,
            self.tvoffcv,
            self.minv,
            self.minvcv,
            self.voffl,
            self.voffcvl,
            self.delta,
            self.rdsw,
            self.rdswmin,
            self.rdwmin,
            self.rswmin,
            self.rsw,
            self.rdw,
            self.prwg,
            self.prwb,
            self.prt,
            self.eta0,
            self.etab,
            self.pclm,
            self.pdibl1,
            self.pdibl2,
            self.pdiblb,
            self.fprout,
            self.pdits,
            self.pditsd,
            self.pditsl,
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
            self.agidl,
            self.bgidl,
            self.cgidl,
            self.egidl,
            self.fgidl,
            self.kgidl,
            self.rgidl,
            self.agisl,
            self.bgisl,
            self.cgisl,
            self.egisl,
            self.fgisl,
            self.kgisl,
            self.rgisl,
            self.aigc,
            self.bigc,
            self.cigc,
            self.aigsd,
            self.bigsd,
            self.cigsd,
            self.aigs,
            self.bigs,
            self.cigs,
            self.aigd,
            self.bigd,
            self.cigd,
            self.aigbacc,
            self.bigbacc,
            self.cigbacc,
            self.aigbinv,
            self.bigbinv,
            self.cigbinv,
            self.nigc,
            self.nigbacc,
            self.nigbinv,
            self.ntox,
            self.eigbinv,
            self.pigcd,
            self.poxedge,
            self.toxref,
            self.ijthdfwd,
            self.ijthsfwd,
            self.ijthdrev,
            self.ijthsrev,
            self.xjbvd,
            self.xjbvs,
            self.bvd,
            self.bvs,
            self.jtss,
            self.jtsd,
            self.jtssws,
            self.jtsswd,
            self.jtsswgs,
            self.jtsswgd,
            self.jtweff,
            self.njts,
            self.njtssw,
            self.njtsswg,
            self.njtsd,
            self.njtsswd,
            self.njtsswgd,
            self.xtss,
            self.xtsd,
            self.xtssws,
            self.xtsswd,
            self.xtsswgs,
            self.xtsswgd,
            self.tnjts,
            self.tnjtssw,
            self.tnjtsswg,
            self.tnjtsd,
            self.tnjtsswd,
            self.tnjtsswgd,
            self.vtss,
            self.vtsd,
            self.vtssws,
            self.vtsswd,
            self.vtsswgs,
            self.vtsswgd,
            self.xrcrg1,
            self.xrcrg2,
            self.lambda,
            self.vtl,
            self.lc,
            self.xn,
            self.vfbsdoff,
            self.lintnoi,
            self.tvfbsdoff,
            self.vfb,
            self.gbmin,
            self.rbdb,
            self.rbsb,
            self.rbpb,
            self.rbps,
            self.rbpd,
            self.rbps0,
            self.rbpsl,
            self.rbpsw,
            self.rbpsnf,
            self.rbpd0,
            self.rbpdl,
            self.rbpdw,
            self.rbpdnf,
            self.rbpbx0,
            self.rbpbxl,
            self.rbpbxw,
            self.rbpbxnf,
            self.rbpby0,
            self.rbpbyl,
            self.rbpbyw,
            self.rbpbynf,
            self.rbsbx0,
            self.rbsby0,
            self.rbdbx0,
            self.rbdby0,
            self.rbsdbxl,
            self.rbsdbxw,
            self.rbsdbxnf,
            self.rbsdbyl,
            self.rbsdbyw,
            self.rbsdbynf,
            self.tnoia,
            self.tnoib,
            self.tnoic,
            self.rnoia,
            self.rnoib,
            self.rnoic,
            self.ntnoi,
            self.cgsl,
            self.cgdl,
            self.ckappas,
            self.ckappad,
            self.cf,
            self.vfbcv,
            self.clc,
            self.cle,
            self.dwc,
            self.dlc,
            self.xw,
            self.xl,
            self.dlcig,
            self.dlcigd,
            self.dwj,
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
            self.dmcg,
            self.dmci,
            self.dmdg,
            self.dmcgt,
            self.xgw,
            self.xgl,
            self.rshg,
            self.ngcon,
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
            self.lketac,
            self.lnsub,
            self.lndep,
            self.lnsd,
            self.lphin,
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
            self.ldvtp0,
            self.ldvtp1,
            self.ldvtp2,
            self.ldvtp3,
            self.ldvtp4,
            self.ldvtp5,
            self.llpe0,
            self.llpeb,
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
            self.lud,
            self.lud1,
            self.lup,
            self.llp,
            self.lu0,
            self.leu,
            self.lucs,
            self.lute,
            self.lucste,
            self.lvoff,
            self.ltvoff,
            self.ltnfactor,
            self.lteta0,
            self.ltvoffcv,
            self.lminv,
            self.lminvcv,
            self.ldelta,
            self.lrdsw,
            self.lrsw,
            self.lrdw,
            self.lprwg,
            self.lprwb,
            self.lprt,
            self.leta0,
            self.letab,
            self.lpclm,
            self.lpdibl1,
            self.lpdibl2,
            self.lpdiblb,
            self.lfprout,
            self.lpdits,
            self.lpditsd,
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
            self.lagidl,
            self.lbgidl,
            self.lcgidl,
            self.legidl,
            self.lfgidl,
            self.lkgidl,
            self.lrgidl,
            self.lagisl,
            self.lbgisl,
            self.lcgisl,
            self.legisl,
            self.lfgisl,
            self.lkgisl,
            self.lrgisl,
            self.laigc,
            self.lbigc,
            self.lcigc,
            self.laigsd,
            self.lbigsd,
            self.lcigsd,
            self.laigs,
            self.lbigs,
            self.lcigs,
            self.laigd,
            self.lbigd,
            self.lcigd,
            self.laigbacc,
            self.lbigbacc,
            self.lcigbacc,
            self.laigbinv,
            self.lbigbinv,
            self.lcigbinv,
            self.lnigc,
            self.lnigbacc,
            self.lnigbinv,
            self.lntox,
            self.leigbinv,
            self.lpigcd,
            self.lpoxedge,
            self.lxrcrg1,
            self.lxrcrg2,
            self.llambda,
            self.lvtl,
            self.lxn,
            self.lvfbsdoff,
            self.ltvfbsdoff,
            self.lcgsl,
            self.lcgdl,
            self.lckappas,
            self.lckappad,
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
            self.wketac,
            self.wnsub,
            self.wndep,
            self.wnsd,
            self.wphin,
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
            self.wdvtp0,
            self.wdvtp1,
            self.wdvtp2,
            self.wdvtp3,
            self.wdvtp4,
            self.wdvtp5,
            self.wlpe0,
            self.wlpeb,
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
            self.wud,
            self.wud1,
            self.wup,
            self.wlp,
            self.wu0,
            self.weu,
            self.wucs,
            self.wute,
            self.wucste,
            self.wvoff,
            self.wtvoff,
            self.wtnfactor,
            self.wteta0,
            self.wtvoffcv,
            self.wminv,
            self.wminvcv,
            self.wdelta,
            self.wrdsw,
            self.wrsw,
            self.wrdw,
            self.wprwg,
            self.wprwb,
            self.wprt,
            self.weta0,
            self.wetab,
            self.wpclm,
            self.wpdibl1,
            self.wpdibl2,
            self.wpdiblb,
            self.wfprout,
            self.wpdits,
            self.wpditsd,
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
            self.wagidl,
            self.wbgidl,
            self.wcgidl,
            self.wegidl,
            self.wfgidl,
            self.wkgidl,
            self.wrgidl,
            self.wagisl,
            self.wbgisl,
            self.wcgisl,
            self.wegisl,
            self.wfgisl,
            self.wkgisl,
            self.wrgisl,
            self.waigc,
            self.wbigc,
            self.wcigc,
            self.waigsd,
            self.wbigsd,
            self.wcigsd,
            self.waigs,
            self.wbigs,
            self.wcigs,
            self.waigd,
            self.wbigd,
            self.wcigd,
            self.waigbacc,
            self.wbigbacc,
            self.wcigbacc,
            self.waigbinv,
            self.wbigbinv,
            self.wcigbinv,
            self.wnigc,
            self.wnigbacc,
            self.wnigbinv,
            self.wntox,
            self.weigbinv,
            self.wpigcd,
            self.wpoxedge,
            self.wxrcrg1,
            self.wxrcrg2,
            self.wlambda,
            self.wvtl,
            self.wxn,
            self.wvfbsdoff,
            self.wtvfbsdoff,
            self.wcgsl,
            self.wcgdl,
            self.wckappas,
            self.wckappad,
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
            self.pketac,
            self.pnsub,
            self.pndep,
            self.pnsd,
            self.pphin,
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
            self.pdvtp0,
            self.pdvtp1,
            self.pdvtp2,
            self.pdvtp3,
            self.pdvtp4,
            self.pdvtp5,
            self.plpe0,
            self.plpeb,
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
            self.pud,
            self.pud1,
            self.pup,
            self.plp,
            self.pu0,
            self.peu,
            self.pucs,
            self.pute,
            self.pucste,
            self.pvoff,
            self.ptvoff,
            self.ptnfactor,
            self.pteta0,
            self.ptvoffcv,
            self.pminv,
            self.pminvcv,
            self.pdelta,
            self.prdsw,
            self.prsw,
            self.prdw,
            self.pprwg,
            self.pprwb,
            self.pprt,
            self.peta0,
            self.petab,
            self.ppclm,
            self.ppdibl1,
            self.ppdibl2,
            self.ppdiblb,
            self.pfprout,
            self.ppdits,
            self.ppditsd,
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
            self.pagidl,
            self.pbgidl,
            self.pcgidl,
            self.pegidl,
            self.pfgidl,
            self.pkgidl,
            self.prgidl,
            self.pagisl,
            self.pbgisl,
            self.pcgisl,
            self.pegisl,
            self.pfgisl,
            self.pkgisl,
            self.prgisl,
            self.paigc,
            self.pbigc,
            self.pcigc,
            self.paigsd,
            self.pbigsd,
            self.pcigsd,
            self.paigs,
            self.pbigs,
            self.pcigs,
            self.paigd,
            self.pbigd,
            self.pcigd,
            self.paigbacc,
            self.pbigbacc,
            self.pcigbacc,
            self.paigbinv,
            self.pbigbinv,
            self.pcigbinv,
            self.pnigc,
            self.pnigbacc,
            self.pnigbinv,
            self.pntox,
            self.peigbinv,
            self.ppigcd,
            self.ppoxedge,
            self.pxrcrg1,
            self.pxrcrg2,
            self.plambda,
            self.pvtl,
            self.pxn,
            self.pvfbsdoff,
            self.ptvfbsdoff,
            self.pcgsl,
            self.pcgdl,
            self.pckappas,
            self.pckappad,
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
            self.SjctSatCurDensity,
            self.DjctSatCurDensity,
            self.SjctSidewallSatCurDensity,
            self.DjctSidewallSatCurDensity,
            self.SjctGateSidewallSatCurDensity,
            self.DjctGateSidewallSatCurDensity,
            self.SbulkJctPotential,
            self.DbulkJctPotential,
            self.SbulkJctBotGradingCoeff,
            self.DbulkJctBotGradingCoeff,
            self.SbulkJctSideGradingCoeff,
            self.DbulkJctSideGradingCoeff,
            self.SbulkJctGateSideGradingCoeff,
            self.DbulkJctGateSideGradingCoeff,
            self.SsidewallJctPotential,
            self.DsidewallJctPotential,
            self.SGatesidewallJctPotential,
            self.DGatesidewallJctPotential,
            self.SunitAreaJctCap,
            self.DunitAreaJctCap,
            self.SunitLengthSidewallJctCap,
            self.DunitLengthSidewallJctCap,
            self.SunitLengthGateSidewallJctCap,
            self.DunitLengthGateSidewallJctCap,
            self.SjctEmissionCoeff,
            self.DjctEmissionCoeff,
            self.SjctTempExponent,
            self.DjctTempExponent,
            self.njtsstemp,
            self.njtsswstemp,
            self.njtsswgstemp,
            self.njtsdtemp,
            self.njtsswdtemp,
            self.njtsswgdtemp,
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
            self.saref,
            self.sbref,
            self.wlod,
            self.ku0,
            self.kvsat,
            self.kvth0,
            self.tku0,
            self.llodku0,
            self.wlodku0,
            self.llodvth,
            self.wlodvth,
            self.lku0,
            self.wku0,
            self.pku0,
            self.lkvth0,
            self.wkvth0,
            self.pkvth0,
            self.stk2,
            self.lodk2,
            self.steta0,
            self.lodeta0,
            self.web,
            self.wec,
            self.kvth0we,
            self.k2we,
            self.ku0we,
            self.scref,
            self.wpemod,
            self.lkvth0we,
            self.lk2we,
            self.lku0we,
            self.wkvth0we,
            self.wk2we,
            self.wku0we,
            self.pkvth0we,
            self.pk2we,
            self.pku0we,
            self.Eg0,
            self.vtm,
            self.vtm0,
            self.coxe,
            self.coxp,
            self.cof1,
            self.cof2,
            self.cof3,
            self.cof4,
            self.vcrit,
            self.factor1,
            self.PhiBS,
            self.PhiBSWS,
            self.PhiBSWGS,
            self.SjctTempSatCurDensity,
            self.SjctSidewallTempSatCurDensity,
            self.SjctGateSidewallTempSatCurDensity,
            self.PhiBD,
            self.PhiBSWD,
            self.PhiBSWGD,
            self.DjctTempSatCurDensity,
            self.DjctSidewallTempSatCurDensity,
            self.DjctGateSidewallTempSatCurDensity,
            self.SunitAreaTempJctCap,
            self.DunitAreaTempJctCap,
            self.SunitLengthSidewallTempJctCap,
            self.DunitLengthSidewallTempJctCap,
            self.SunitLengthGateSidewallTempJctCap,
            self.DunitLengthGateSidewallTempJctCap,
            self.oxideTrapDensityA,
            self.oxideTrapDensityB,
            self.oxideTrapDensityC,
            self.em,
            self.ef,
            self.af,
            self.kf,
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
            self.gidlclamp,
            self.idovvdsc,
            f64::from(self.InstCount),
            f64::from(u8::from(self.mobModGiven)),
            f64::from(u8::from(self.binUnitGiven)),
            f64::from(u8::from(self.cvchargeModGiven)),
            f64::from(u8::from(self.capModGiven)),
            f64::from(u8::from(self.dioModGiven)),
            f64::from(u8::from(self.rdsModGiven)),
            f64::from(u8::from(self.rbodyModGiven)),
            f64::from(u8::from(self.rgateModGiven)),
            f64::from(u8::from(self.perModGiven)),
            f64::from(u8::from(self.geoModGiven)),
            f64::from(u8::from(self.paramChkGiven)),
            f64::from(u8::from(self.trnqsModGiven)),
            f64::from(u8::from(self.acnqsModGiven)),
            f64::from(u8::from(self.fnoiModGiven)),
            f64::from(u8::from(self.tnoiModGiven)),
            f64::from(u8::from(self.mtrlModGiven)),
            f64::from(u8::from(self.mtrlCompatModGiven)),
            f64::from(u8::from(self.gidlModGiven)),
            f64::from(u8::from(self.igcModGiven)),
            f64::from(u8::from(self.igbModGiven)),
            f64::from(u8::from(self.tempModGiven)),
            f64::from(u8::from(self.typeGiven)),
            f64::from(u8::from(self.toxrefGiven)),
            f64::from(u8::from(self.eotGiven)),
            f64::from(u8::from(self.vddeotGiven)),
            f64::from(u8::from(self.tempeotGiven)),
            f64::from(u8::from(self.leffeotGiven)),
            f64::from(u8::from(self.weffeotGiven)),
            f64::from(u8::from(self.adosGiven)),
            f64::from(u8::from(self.bdosGiven)),
            f64::from(u8::from(self.toxeGiven)),
            f64::from(u8::from(self.toxpGiven)),
            f64::from(u8::from(self.toxmGiven)),
            f64::from(u8::from(self.dtoxGiven)),
            f64::from(u8::from(self.epsroxGiven)),
            f64::from(u8::from(self.versionGiven)),
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
            f64::from(u8::from(self.ketacGiven)),
            f64::from(u8::from(self.nsubGiven)),
            f64::from(u8::from(self.phigGiven)),
            f64::from(u8::from(self.epsrgateGiven)),
            f64::from(u8::from(self.easubGiven)),
            f64::from(u8::from(self.epsrsubGiven)),
            f64::from(u8::from(self.ni0subGiven)),
            f64::from(u8::from(self.bg0subGiven)),
            f64::from(u8::from(self.tbgasubGiven)),
            f64::from(u8::from(self.tbgbsubGiven)),
            f64::from(u8::from(self.ndepGiven)),
            f64::from(u8::from(self.nsdGiven)),
            f64::from(u8::from(self.phinGiven)),
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
            f64::from(u8::from(self.dvtp0Given)),
            f64::from(u8::from(self.dvtp1Given)),
            f64::from(u8::from(self.dvtp2Given)),
            f64::from(u8::from(self.dvtp3Given)),
            f64::from(u8::from(self.dvtp4Given)),
            f64::from(u8::from(self.dvtp5Given)),
            f64::from(u8::from(self.lpe0Given)),
            f64::from(u8::from(self.lpebGiven)),
            f64::from(u8::from(self.dvt0Given)),
            f64::from(u8::from(self.dvt1Given)),
            f64::from(u8::from(self.dvt2Given)),
            f64::from(u8::from(self.dvt0wGiven)),
            f64::from(u8::from(self.dvt1wGiven)),
            f64::from(u8::from(self.dvt2wGiven)),
            f64::from(u8::from(self.droutGiven)),
            f64::from(u8::from(self.dsubGiven)),
            f64::from(u8::from(self.vth0Given)),
            f64::from(u8::from(self.euGiven)),
            f64::from(u8::from(self.ucsGiven)),
            f64::from(u8::from(self.uaGiven)),
            f64::from(u8::from(self.ua1Given)),
            f64::from(u8::from(self.ubGiven)),
            f64::from(u8::from(self.ub1Given)),
            f64::from(u8::from(self.ucGiven)),
            f64::from(u8::from(self.uc1Given)),
            f64::from(u8::from(self.udGiven)),
            f64::from(u8::from(self.ud1Given)),
            f64::from(u8::from(self.upGiven)),
            f64::from(u8::from(self.lpGiven)),
            f64::from(u8::from(self.u0Given)),
            f64::from(u8::from(self.uteGiven)),
            f64::from(u8::from(self.ucsteGiven)),
            f64::from(u8::from(self.voffGiven)),
            f64::from(u8::from(self.tvoffGiven)),
            f64::from(u8::from(self.tnfactorGiven)),
            f64::from(u8::from(self.teta0Given)),
            f64::from(u8::from(self.tvoffcvGiven)),
            f64::from(u8::from(self.vofflGiven)),
            f64::from(u8::from(self.voffcvlGiven)),
            f64::from(u8::from(self.minvGiven)),
            f64::from(u8::from(self.minvcvGiven)),
            f64::from(u8::from(self.rdswGiven)),
            f64::from(u8::from(self.rdswminGiven)),
            f64::from(u8::from(self.rdwminGiven)),
            f64::from(u8::from(self.rswminGiven)),
            f64::from(u8::from(self.rswGiven)),
            f64::from(u8::from(self.rdwGiven)),
            f64::from(u8::from(self.prwgGiven)),
            f64::from(u8::from(self.prwbGiven)),
            f64::from(u8::from(self.prtGiven)),
            f64::from(u8::from(self.eta0Given)),
            f64::from(u8::from(self.etabGiven)),
            f64::from(u8::from(self.pclmGiven)),
            f64::from(u8::from(self.pdibl1Given)),
            f64::from(u8::from(self.pdibl2Given)),
            f64::from(u8::from(self.pdiblbGiven)),
            f64::from(u8::from(self.fproutGiven)),
            f64::from(u8::from(self.pditsGiven)),
            f64::from(u8::from(self.pditsdGiven)),
            f64::from(u8::from(self.pditslGiven)),
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
            f64::from(u8::from(self.agidlGiven)),
            f64::from(u8::from(self.bgidlGiven)),
            f64::from(u8::from(self.cgidlGiven)),
            f64::from(u8::from(self.egidlGiven)),
            f64::from(u8::from(self.fgidlGiven)),
            f64::from(u8::from(self.kgidlGiven)),
            f64::from(u8::from(self.rgidlGiven)),
            f64::from(u8::from(self.agislGiven)),
            f64::from(u8::from(self.bgislGiven)),
            f64::from(u8::from(self.cgislGiven)),
            f64::from(u8::from(self.egislGiven)),
            f64::from(u8::from(self.fgislGiven)),
            f64::from(u8::from(self.kgislGiven)),
            f64::from(u8::from(self.rgislGiven)),
            f64::from(u8::from(self.aigcGiven)),
            f64::from(u8::from(self.bigcGiven)),
            f64::from(u8::from(self.cigcGiven)),
            f64::from(u8::from(self.aigsdGiven)),
            f64::from(u8::from(self.bigsdGiven)),
            f64::from(u8::from(self.cigsdGiven)),
            f64::from(u8::from(self.aigsGiven)),
            f64::from(u8::from(self.bigsGiven)),
            f64::from(u8::from(self.cigsGiven)),
            f64::from(u8::from(self.aigdGiven)),
            f64::from(u8::from(self.bigdGiven)),
            f64::from(u8::from(self.cigdGiven)),
            f64::from(u8::from(self.aigbaccGiven)),
            f64::from(u8::from(self.bigbaccGiven)),
            f64::from(u8::from(self.cigbaccGiven)),
            f64::from(u8::from(self.aigbinvGiven)),
            f64::from(u8::from(self.bigbinvGiven)),
            f64::from(u8::from(self.cigbinvGiven)),
            f64::from(u8::from(self.nigcGiven)),
            f64::from(u8::from(self.nigbinvGiven)),
            f64::from(u8::from(self.nigbaccGiven)),
            f64::from(u8::from(self.ntoxGiven)),
            f64::from(u8::from(self.eigbinvGiven)),
            f64::from(u8::from(self.pigcdGiven)),
            f64::from(u8::from(self.poxedgeGiven)),
            f64::from(u8::from(self.ijthdfwdGiven)),
            f64::from(u8::from(self.ijthsfwdGiven)),
            f64::from(u8::from(self.ijthdrevGiven)),
            f64::from(u8::from(self.ijthsrevGiven)),
            f64::from(u8::from(self.xjbvdGiven)),
            f64::from(u8::from(self.xjbvsGiven)),
            f64::from(u8::from(self.bvdGiven)),
            f64::from(u8::from(self.bvsGiven)),
            f64::from(u8::from(self.jtssGiven)),
            f64::from(u8::from(self.jtsdGiven)),
            f64::from(u8::from(self.jtsswsGiven)),
            f64::from(u8::from(self.jtsswdGiven)),
            f64::from(u8::from(self.jtsswgsGiven)),
            f64::from(u8::from(self.jtsswgdGiven)),
            f64::from(u8::from(self.jtweffGiven)),
            f64::from(u8::from(self.njtsGiven)),
            f64::from(u8::from(self.njtsswGiven)),
            f64::from(u8::from(self.njtsswgGiven)),
            f64::from(u8::from(self.njtsdGiven)),
            f64::from(u8::from(self.njtsswdGiven)),
            f64::from(u8::from(self.njtsswgdGiven)),
            f64::from(u8::from(self.xtssGiven)),
            f64::from(u8::from(self.xtsdGiven)),
            f64::from(u8::from(self.xtsswsGiven)),
            f64::from(u8::from(self.xtsswdGiven)),
            f64::from(u8::from(self.xtsswgsGiven)),
            f64::from(u8::from(self.xtsswgdGiven)),
            f64::from(u8::from(self.tnjtsGiven)),
            f64::from(u8::from(self.tnjtsswGiven)),
            f64::from(u8::from(self.tnjtsswgGiven)),
            f64::from(u8::from(self.tnjtsdGiven)),
            f64::from(u8::from(self.tnjtsswdGiven)),
            f64::from(u8::from(self.tnjtsswgdGiven)),
            f64::from(u8::from(self.vtssGiven)),
            f64::from(u8::from(self.vtsdGiven)),
            f64::from(u8::from(self.vtsswsGiven)),
            f64::from(u8::from(self.vtsswdGiven)),
            f64::from(u8::from(self.vtsswgsGiven)),
            f64::from(u8::from(self.vtsswgdGiven)),
            f64::from(u8::from(self.vfbGiven)),
            f64::from(u8::from(self.gbminGiven)),
            f64::from(u8::from(self.rbdbGiven)),
            f64::from(u8::from(self.rbsbGiven)),
            f64::from(u8::from(self.rbpsGiven)),
            f64::from(u8::from(self.rbpdGiven)),
            f64::from(u8::from(self.rbpbGiven)),
            f64::from(u8::from(self.rbps0Given)),
            f64::from(u8::from(self.rbpslGiven)),
            f64::from(u8::from(self.rbpswGiven)),
            f64::from(u8::from(self.rbpsnfGiven)),
            f64::from(u8::from(self.rbpd0Given)),
            f64::from(u8::from(self.rbpdlGiven)),
            f64::from(u8::from(self.rbpdwGiven)),
            f64::from(u8::from(self.rbpdnfGiven)),
            f64::from(u8::from(self.rbpbx0Given)),
            f64::from(u8::from(self.rbpbxlGiven)),
            f64::from(u8::from(self.rbpbxwGiven)),
            f64::from(u8::from(self.rbpbxnfGiven)),
            f64::from(u8::from(self.rbpby0Given)),
            f64::from(u8::from(self.rbpbylGiven)),
            f64::from(u8::from(self.rbpbywGiven)),
            f64::from(u8::from(self.rbpbynfGiven)),
            f64::from(u8::from(self.rbsbx0Given)),
            f64::from(u8::from(self.rbsby0Given)),
            f64::from(u8::from(self.rbdbx0Given)),
            f64::from(u8::from(self.rbdby0Given)),
            f64::from(u8::from(self.rbsdbxlGiven)),
            f64::from(u8::from(self.rbsdbxwGiven)),
            f64::from(u8::from(self.rbsdbxnfGiven)),
            f64::from(u8::from(self.rbsdbylGiven)),
            f64::from(u8::from(self.rbsdbywGiven)),
            f64::from(u8::from(self.rbsdbynfGiven)),
            f64::from(u8::from(self.xrcrg1Given)),
            f64::from(u8::from(self.xrcrg2Given)),
            f64::from(u8::from(self.tnoiaGiven)),
            f64::from(u8::from(self.tnoibGiven)),
            f64::from(u8::from(self.tnoicGiven)),
            f64::from(u8::from(self.rnoiaGiven)),
            f64::from(u8::from(self.rnoibGiven)),
            f64::from(u8::from(self.rnoicGiven)),
            f64::from(u8::from(self.ntnoiGiven)),
            f64::from(u8::from(self.lambdaGiven)),
            f64::from(u8::from(self.vtlGiven)),
            f64::from(u8::from(self.lcGiven)),
            f64::from(u8::from(self.xnGiven)),
            f64::from(u8::from(self.vfbsdoffGiven)),
            f64::from(u8::from(self.lintnoiGiven)),
            f64::from(u8::from(self.tvfbsdoffGiven)),
            f64::from(u8::from(self.cgslGiven)),
            f64::from(u8::from(self.cgdlGiven)),
            f64::from(u8::from(self.ckappasGiven)),
            f64::from(u8::from(self.ckappadGiven)),
            f64::from(u8::from(self.cfGiven)),
            f64::from(u8::from(self.vfbcvGiven)),
            f64::from(u8::from(self.clcGiven)),
            f64::from(u8::from(self.cleGiven)),
            f64::from(u8::from(self.dwcGiven)),
            f64::from(u8::from(self.dlcGiven)),
            f64::from(u8::from(self.xwGiven)),
            f64::from(u8::from(self.xlGiven)),
            f64::from(u8::from(self.dlcigGiven)),
            f64::from(u8::from(self.dlcigdGiven)),
            f64::from(u8::from(self.dwjGiven)),
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
            f64::from(u8::from(self.dmcgGiven)),
            f64::from(u8::from(self.dmciGiven)),
            f64::from(u8::from(self.dmdgGiven)),
            f64::from(u8::from(self.dmcgtGiven)),
            f64::from(u8::from(self.xgwGiven)),
            f64::from(u8::from(self.xglGiven)),
            f64::from(u8::from(self.rshgGiven)),
            f64::from(u8::from(self.ngconGiven)),
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
            f64::from(u8::from(self.lketacGiven)),
            f64::from(u8::from(self.lnsubGiven)),
            f64::from(u8::from(self.lndepGiven)),
            f64::from(u8::from(self.lnsdGiven)),
            f64::from(u8::from(self.lphinGiven)),
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
            f64::from(u8::from(self.ldvtp0Given)),
            f64::from(u8::from(self.ldvtp1Given)),
            f64::from(u8::from(self.ldvtp2Given)),
            f64::from(u8::from(self.ldvtp3Given)),
            f64::from(u8::from(self.ldvtp4Given)),
            f64::from(u8::from(self.ldvtp5Given)),
            f64::from(u8::from(self.llpe0Given)),
            f64::from(u8::from(self.llpebGiven)),
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
            f64::from(u8::from(self.ludGiven)),
            f64::from(u8::from(self.lud1Given)),
            f64::from(u8::from(self.lupGiven)),
            f64::from(u8::from(self.llpGiven)),
            f64::from(u8::from(self.lu0Given)),
            f64::from(u8::from(self.leuGiven)),
            f64::from(u8::from(self.lucsGiven)),
            f64::from(u8::from(self.luteGiven)),
            f64::from(u8::from(self.lucsteGiven)),
            f64::from(u8::from(self.lvoffGiven)),
            f64::from(u8::from(self.ltvoffGiven)),
            f64::from(u8::from(self.ltnfactorGiven)),
            f64::from(u8::from(self.lteta0Given)),
            f64::from(u8::from(self.ltvoffcvGiven)),
            f64::from(u8::from(self.lminvGiven)),
            f64::from(u8::from(self.lminvcvGiven)),
            f64::from(u8::from(self.lrdswGiven)),
            f64::from(u8::from(self.lrswGiven)),
            f64::from(u8::from(self.lrdwGiven)),
            f64::from(u8::from(self.lprwgGiven)),
            f64::from(u8::from(self.lprwbGiven)),
            f64::from(u8::from(self.lprtGiven)),
            f64::from(u8::from(self.leta0Given)),
            f64::from(u8::from(self.letabGiven)),
            f64::from(u8::from(self.lpclmGiven)),
            f64::from(u8::from(self.lpdibl1Given)),
            f64::from(u8::from(self.lpdibl2Given)),
            f64::from(u8::from(self.lpdiblbGiven)),
            f64::from(u8::from(self.lfproutGiven)),
            f64::from(u8::from(self.lpditsGiven)),
            f64::from(u8::from(self.lpditsdGiven)),
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
            f64::from(u8::from(self.lagidlGiven)),
            f64::from(u8::from(self.lbgidlGiven)),
            f64::from(u8::from(self.lcgidlGiven)),
            f64::from(u8::from(self.legidlGiven)),
            f64::from(u8::from(self.lfgidlGiven)),
            f64::from(u8::from(self.lkgidlGiven)),
            f64::from(u8::from(self.lrgidlGiven)),
            f64::from(u8::from(self.lagislGiven)),
            f64::from(u8::from(self.lbgislGiven)),
            f64::from(u8::from(self.lcgislGiven)),
            f64::from(u8::from(self.legislGiven)),
            f64::from(u8::from(self.lfgislGiven)),
            f64::from(u8::from(self.lkgislGiven)),
            f64::from(u8::from(self.lrgislGiven)),
            f64::from(u8::from(self.laigcGiven)),
            f64::from(u8::from(self.lbigcGiven)),
            f64::from(u8::from(self.lcigcGiven)),
            f64::from(u8::from(self.laigsdGiven)),
            f64::from(u8::from(self.lbigsdGiven)),
            f64::from(u8::from(self.lcigsdGiven)),
            f64::from(u8::from(self.laigsGiven)),
            f64::from(u8::from(self.lbigsGiven)),
            f64::from(u8::from(self.lcigsGiven)),
            f64::from(u8::from(self.laigdGiven)),
            f64::from(u8::from(self.lbigdGiven)),
            f64::from(u8::from(self.lcigdGiven)),
            f64::from(u8::from(self.laigbaccGiven)),
            f64::from(u8::from(self.lbigbaccGiven)),
            f64::from(u8::from(self.lcigbaccGiven)),
            f64::from(u8::from(self.laigbinvGiven)),
            f64::from(u8::from(self.lbigbinvGiven)),
            f64::from(u8::from(self.lcigbinvGiven)),
            f64::from(u8::from(self.lnigcGiven)),
            f64::from(u8::from(self.lnigbinvGiven)),
            f64::from(u8::from(self.lnigbaccGiven)),
            f64::from(u8::from(self.lntoxGiven)),
            f64::from(u8::from(self.leigbinvGiven)),
            f64::from(u8::from(self.lpigcdGiven)),
            f64::from(u8::from(self.lpoxedgeGiven)),
            f64::from(u8::from(self.lxrcrg1Given)),
            f64::from(u8::from(self.lxrcrg2Given)),
            f64::from(u8::from(self.llambdaGiven)),
            f64::from(u8::from(self.lvtlGiven)),
            f64::from(u8::from(self.lxnGiven)),
            f64::from(u8::from(self.lvfbsdoffGiven)),
            f64::from(u8::from(self.ltvfbsdoffGiven)),
            f64::from(u8::from(self.lcgslGiven)),
            f64::from(u8::from(self.lcgdlGiven)),
            f64::from(u8::from(self.lckappasGiven)),
            f64::from(u8::from(self.lckappadGiven)),
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
            f64::from(u8::from(self.wketacGiven)),
            f64::from(u8::from(self.wnsubGiven)),
            f64::from(u8::from(self.wndepGiven)),
            f64::from(u8::from(self.wnsdGiven)),
            f64::from(u8::from(self.wphinGiven)),
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
            f64::from(u8::from(self.wdvtp0Given)),
            f64::from(u8::from(self.wdvtp1Given)),
            f64::from(u8::from(self.wdvtp2Given)),
            f64::from(u8::from(self.wdvtp3Given)),
            f64::from(u8::from(self.wdvtp4Given)),
            f64::from(u8::from(self.wdvtp5Given)),
            f64::from(u8::from(self.wlpe0Given)),
            f64::from(u8::from(self.wlpebGiven)),
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
            f64::from(u8::from(self.wudGiven)),
            f64::from(u8::from(self.wud1Given)),
            f64::from(u8::from(self.wupGiven)),
            f64::from(u8::from(self.wlpGiven)),
            f64::from(u8::from(self.wu0Given)),
            f64::from(u8::from(self.weuGiven)),
            f64::from(u8::from(self.wucsGiven)),
            f64::from(u8::from(self.wuteGiven)),
            f64::from(u8::from(self.wucsteGiven)),
            f64::from(u8::from(self.wvoffGiven)),
            f64::from(u8::from(self.wtvoffGiven)),
            f64::from(u8::from(self.wtnfactorGiven)),
            f64::from(u8::from(self.wteta0Given)),
            f64::from(u8::from(self.wtvoffcvGiven)),
            f64::from(u8::from(self.wminvGiven)),
            f64::from(u8::from(self.wminvcvGiven)),
            f64::from(u8::from(self.wrdswGiven)),
            f64::from(u8::from(self.wrswGiven)),
            f64::from(u8::from(self.wrdwGiven)),
            f64::from(u8::from(self.wprwgGiven)),
            f64::from(u8::from(self.wprwbGiven)),
            f64::from(u8::from(self.wprtGiven)),
            f64::from(u8::from(self.weta0Given)),
            f64::from(u8::from(self.wetabGiven)),
            f64::from(u8::from(self.wpclmGiven)),
            f64::from(u8::from(self.wpdibl1Given)),
            f64::from(u8::from(self.wpdibl2Given)),
            f64::from(u8::from(self.wpdiblbGiven)),
            f64::from(u8::from(self.wfproutGiven)),
            f64::from(u8::from(self.wpditsGiven)),
            f64::from(u8::from(self.wpditsdGiven)),
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
            f64::from(u8::from(self.wagidlGiven)),
            f64::from(u8::from(self.wbgidlGiven)),
            f64::from(u8::from(self.wcgidlGiven)),
            f64::from(u8::from(self.wegidlGiven)),
            f64::from(u8::from(self.wfgidlGiven)),
            f64::from(u8::from(self.wkgidlGiven)),
            f64::from(u8::from(self.wrgidlGiven)),
            f64::from(u8::from(self.wagislGiven)),
            f64::from(u8::from(self.wbgislGiven)),
            f64::from(u8::from(self.wcgislGiven)),
            f64::from(u8::from(self.wegislGiven)),
            f64::from(u8::from(self.wfgislGiven)),
            f64::from(u8::from(self.wkgislGiven)),
            f64::from(u8::from(self.wrgislGiven)),
            f64::from(u8::from(self.waigcGiven)),
            f64::from(u8::from(self.wbigcGiven)),
            f64::from(u8::from(self.wcigcGiven)),
            f64::from(u8::from(self.waigsdGiven)),
            f64::from(u8::from(self.wbigsdGiven)),
            f64::from(u8::from(self.wcigsdGiven)),
            f64::from(u8::from(self.waigsGiven)),
            f64::from(u8::from(self.wbigsGiven)),
            f64::from(u8::from(self.wcigsGiven)),
            f64::from(u8::from(self.waigdGiven)),
            f64::from(u8::from(self.wbigdGiven)),
            f64::from(u8::from(self.wcigdGiven)),
            f64::from(u8::from(self.waigbaccGiven)),
            f64::from(u8::from(self.wbigbaccGiven)),
            f64::from(u8::from(self.wcigbaccGiven)),
            f64::from(u8::from(self.waigbinvGiven)),
            f64::from(u8::from(self.wbigbinvGiven)),
            f64::from(u8::from(self.wcigbinvGiven)),
            f64::from(u8::from(self.wnigcGiven)),
            f64::from(u8::from(self.wnigbinvGiven)),
            f64::from(u8::from(self.wnigbaccGiven)),
            f64::from(u8::from(self.wntoxGiven)),
            f64::from(u8::from(self.weigbinvGiven)),
            f64::from(u8::from(self.wpigcdGiven)),
            f64::from(u8::from(self.wpoxedgeGiven)),
            f64::from(u8::from(self.wxrcrg1Given)),
            f64::from(u8::from(self.wxrcrg2Given)),
            f64::from(u8::from(self.wlambdaGiven)),
            f64::from(u8::from(self.wvtlGiven)),
            f64::from(u8::from(self.wxnGiven)),
            f64::from(u8::from(self.wvfbsdoffGiven)),
            f64::from(u8::from(self.wtvfbsdoffGiven)),
            f64::from(u8::from(self.wcgslGiven)),
            f64::from(u8::from(self.wcgdlGiven)),
            f64::from(u8::from(self.wckappasGiven)),
            f64::from(u8::from(self.wckappadGiven)),
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
            f64::from(u8::from(self.pketacGiven)),
            f64::from(u8::from(self.pnsubGiven)),
            f64::from(u8::from(self.pndepGiven)),
            f64::from(u8::from(self.pnsdGiven)),
            f64::from(u8::from(self.pphinGiven)),
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
            f64::from(u8::from(self.pdvtp0Given)),
            f64::from(u8::from(self.pdvtp1Given)),
            f64::from(u8::from(self.pdvtp2Given)),
            f64::from(u8::from(self.pdvtp3Given)),
            f64::from(u8::from(self.pdvtp4Given)),
            f64::from(u8::from(self.pdvtp5Given)),
            f64::from(u8::from(self.plpe0Given)),
            f64::from(u8::from(self.plpebGiven)),
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
            f64::from(u8::from(self.pudGiven)),
            f64::from(u8::from(self.pud1Given)),
            f64::from(u8::from(self.pupGiven)),
            f64::from(u8::from(self.plpGiven)),
            f64::from(u8::from(self.pu0Given)),
            f64::from(u8::from(self.peuGiven)),
            f64::from(u8::from(self.pucsGiven)),
            f64::from(u8::from(self.puteGiven)),
            f64::from(u8::from(self.pucsteGiven)),
            f64::from(u8::from(self.pvoffGiven)),
            f64::from(u8::from(self.ptvoffGiven)),
            f64::from(u8::from(self.ptnfactorGiven)),
            f64::from(u8::from(self.pteta0Given)),
            f64::from(u8::from(self.ptvoffcvGiven)),
            f64::from(u8::from(self.pminvGiven)),
            f64::from(u8::from(self.pminvcvGiven)),
            f64::from(u8::from(self.prdswGiven)),
            f64::from(u8::from(self.prswGiven)),
            f64::from(u8::from(self.prdwGiven)),
            f64::from(u8::from(self.pprwgGiven)),
            f64::from(u8::from(self.pprwbGiven)),
            f64::from(u8::from(self.pprtGiven)),
            f64::from(u8::from(self.peta0Given)),
            f64::from(u8::from(self.petabGiven)),
            f64::from(u8::from(self.ppclmGiven)),
            f64::from(u8::from(self.ppdibl1Given)),
            f64::from(u8::from(self.ppdibl2Given)),
            f64::from(u8::from(self.ppdiblbGiven)),
            f64::from(u8::from(self.pfproutGiven)),
            f64::from(u8::from(self.ppditsGiven)),
            f64::from(u8::from(self.ppditsdGiven)),
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
            f64::from(u8::from(self.pagidlGiven)),
            f64::from(u8::from(self.pbgidlGiven)),
            f64::from(u8::from(self.pcgidlGiven)),
            f64::from(u8::from(self.pegidlGiven)),
            f64::from(u8::from(self.pfgidlGiven)),
            f64::from(u8::from(self.pkgidlGiven)),
            f64::from(u8::from(self.prgidlGiven)),
            f64::from(u8::from(self.pagislGiven)),
            f64::from(u8::from(self.pbgislGiven)),
            f64::from(u8::from(self.pcgislGiven)),
            f64::from(u8::from(self.pegislGiven)),
            f64::from(u8::from(self.pfgislGiven)),
            f64::from(u8::from(self.pkgislGiven)),
            f64::from(u8::from(self.prgislGiven)),
            f64::from(u8::from(self.paigcGiven)),
            f64::from(u8::from(self.pbigcGiven)),
            f64::from(u8::from(self.pcigcGiven)),
            f64::from(u8::from(self.paigsdGiven)),
            f64::from(u8::from(self.pbigsdGiven)),
            f64::from(u8::from(self.pcigsdGiven)),
            f64::from(u8::from(self.paigsGiven)),
            f64::from(u8::from(self.pbigsGiven)),
            f64::from(u8::from(self.pcigsGiven)),
            f64::from(u8::from(self.paigdGiven)),
            f64::from(u8::from(self.pbigdGiven)),
            f64::from(u8::from(self.pcigdGiven)),
            f64::from(u8::from(self.paigbaccGiven)),
            f64::from(u8::from(self.pbigbaccGiven)),
            f64::from(u8::from(self.pcigbaccGiven)),
            f64::from(u8::from(self.paigbinvGiven)),
            f64::from(u8::from(self.pbigbinvGiven)),
            f64::from(u8::from(self.pcigbinvGiven)),
            f64::from(u8::from(self.pnigcGiven)),
            f64::from(u8::from(self.pnigbinvGiven)),
            f64::from(u8::from(self.pnigbaccGiven)),
            f64::from(u8::from(self.pntoxGiven)),
            f64::from(u8::from(self.peigbinvGiven)),
            f64::from(u8::from(self.ppigcdGiven)),
            f64::from(u8::from(self.ppoxedgeGiven)),
            f64::from(u8::from(self.pxrcrg1Given)),
            f64::from(u8::from(self.pxrcrg2Given)),
            f64::from(u8::from(self.plambdaGiven)),
            f64::from(u8::from(self.pvtlGiven)),
            f64::from(u8::from(self.pxnGiven)),
            f64::from(u8::from(self.pvfbsdoffGiven)),
            f64::from(u8::from(self.ptvfbsdoffGiven)),
            f64::from(u8::from(self.pcgslGiven)),
            f64::from(u8::from(self.pcgdlGiven)),
            f64::from(u8::from(self.pckappasGiven)),
            f64::from(u8::from(self.pckappadGiven)),
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
            f64::from(u8::from(self.SjctSatCurDensityGiven)),
            f64::from(u8::from(self.SjctSidewallSatCurDensityGiven)),
            f64::from(u8::from(self.SjctGateSidewallSatCurDensityGiven)),
            f64::from(u8::from(self.SbulkJctPotentialGiven)),
            f64::from(u8::from(self.SbulkJctBotGradingCoeffGiven)),
            f64::from(u8::from(self.SsidewallJctPotentialGiven)),
            f64::from(u8::from(self.SGatesidewallJctPotentialGiven)),
            f64::from(u8::from(self.SbulkJctSideGradingCoeffGiven)),
            f64::from(u8::from(self.SunitAreaJctCapGiven)),
            f64::from(u8::from(self.SunitLengthSidewallJctCapGiven)),
            f64::from(u8::from(self.SbulkJctGateSideGradingCoeffGiven)),
            f64::from(u8::from(self.SunitLengthGateSidewallJctCapGiven)),
            f64::from(u8::from(self.SjctEmissionCoeffGiven)),
            f64::from(u8::from(self.SjctTempExponentGiven)),
            f64::from(u8::from(self.DjctSatCurDensityGiven)),
            f64::from(u8::from(self.DjctSidewallSatCurDensityGiven)),
            f64::from(u8::from(self.DjctGateSidewallSatCurDensityGiven)),
            f64::from(u8::from(self.DbulkJctPotentialGiven)),
            f64::from(u8::from(self.DbulkJctBotGradingCoeffGiven)),
            f64::from(u8::from(self.DsidewallJctPotentialGiven)),
            f64::from(u8::from(self.DGatesidewallJctPotentialGiven)),
            f64::from(u8::from(self.DbulkJctSideGradingCoeffGiven)),
            f64::from(u8::from(self.DunitAreaJctCapGiven)),
            f64::from(u8::from(self.DunitLengthSidewallJctCapGiven)),
            f64::from(u8::from(self.DbulkJctGateSideGradingCoeffGiven)),
            f64::from(u8::from(self.DunitLengthGateSidewallJctCapGiven)),
            f64::from(u8::from(self.DjctEmissionCoeffGiven)),
            f64::from(u8::from(self.DjctTempExponentGiven)),
            f64::from(u8::from(self.oxideTrapDensityAGiven)),
            f64::from(u8::from(self.oxideTrapDensityBGiven)),
            f64::from(u8::from(self.oxideTrapDensityCGiven)),
            f64::from(u8::from(self.emGiven)),
            f64::from(u8::from(self.efGiven)),
            f64::from(u8::from(self.afGiven)),
            f64::from(u8::from(self.kfGiven)),
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
            f64::from(u8::from(self.sarefGiven)),
            f64::from(u8::from(self.sbrefGiven)),
            f64::from(u8::from(self.wlodGiven)),
            f64::from(u8::from(self.ku0Given)),
            f64::from(u8::from(self.kvsatGiven)),
            f64::from(u8::from(self.kvth0Given)),
            f64::from(u8::from(self.tku0Given)),
            f64::from(u8::from(self.llodku0Given)),
            f64::from(u8::from(self.wlodku0Given)),
            f64::from(u8::from(self.llodvthGiven)),
            f64::from(u8::from(self.wlodvthGiven)),
            f64::from(u8::from(self.lku0Given)),
            f64::from(u8::from(self.wku0Given)),
            f64::from(u8::from(self.pku0Given)),
            f64::from(u8::from(self.lkvth0Given)),
            f64::from(u8::from(self.wkvth0Given)),
            f64::from(u8::from(self.pkvth0Given)),
            f64::from(u8::from(self.stk2Given)),
            f64::from(u8::from(self.lodk2Given)),
            f64::from(u8::from(self.steta0Given)),
            f64::from(u8::from(self.lodeta0Given)),
            f64::from(u8::from(self.webGiven)),
            f64::from(u8::from(self.wecGiven)),
            f64::from(u8::from(self.kvth0weGiven)),
            f64::from(u8::from(self.k2weGiven)),
            f64::from(u8::from(self.ku0weGiven)),
            f64::from(u8::from(self.screfGiven)),
            f64::from(u8::from(self.wpemodGiven)),
            f64::from(u8::from(self.lkvth0weGiven)),
            f64::from(u8::from(self.lk2weGiven)),
            f64::from(u8::from(self.lku0weGiven)),
            f64::from(u8::from(self.wkvth0weGiven)),
            f64::from(u8::from(self.wk2weGiven)),
            f64::from(u8::from(self.wku0weGiven)),
            f64::from(u8::from(self.pkvth0weGiven)),
            f64::from(u8::from(self.pk2weGiven)),
            f64::from(u8::from(self.pku0weGiven)),
            f64::from(u8::from(self.gidlclampGiven)),
            f64::from(u8::from(self.idovvdscGiven)),
        ]
    }

    /// Stores numbers produced by [`Self::to_values`]; `values` must have
    /// [`Self::MEMBERS`]`.len()` entries.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub(crate) fn set_values(&mut self, values: &[f64]) {
        self.r#type = values[0] as i32;
        self.mobMod = values[1] as i32;
        self.cvchargeMod = values[2] as i32;
        self.capMod = values[3] as i32;
        self.dioMod = values[4] as i32;
        self.trnqsMod = values[5] as i32;
        self.acnqsMod = values[6] as i32;
        self.fnoiMod = values[7] as i32;
        self.tnoiMod = values[8] as i32;
        self.rdsMod = values[9] as i32;
        self.rbodyMod = values[10] as i32;
        self.rgateMod = values[11] as i32;
        self.perMod = values[12] as i32;
        self.geoMod = values[13] as i32;
        self.mtrlMod = values[14] as i32;
        self.mtrlCompatMod = values[15] as i32;
        self.gidlMod = values[16] as i32;
        self.igcMod = values[17] as i32;
        self.igbMod = values[18] as i32;
        self.tempMod = values[19] as i32;
        self.binUnit = values[20] as i32;
        self.paramChk = values[21] as i32;
        self.v48intVersion = values[22] as i32;
        self.eot = values[23];
        self.vddeot = values[24];
        self.tempeot = values[25];
        self.leffeot = values[26];
        self.weffeot = values[27];
        self.ados = values[28];
        self.bdos = values[29];
        self.toxe = values[30];
        self.toxp = values[31];
        self.toxm = values[32];
        self.dtox = values[33];
        self.epsrox = values[34];
        self.cdsc = values[35];
        self.cdscb = values[36];
        self.cdscd = values[37];
        self.cit = values[38];
        self.nfactor = values[39];
        self.xj = values[40];
        self.vsat = values[41];
        self.at = values[42];
        self.a0 = values[43];
        self.ags = values[44];
        self.a1 = values[45];
        self.a2 = values[46];
        self.keta = values[47];
        self.ketac = values[48];
        self.nsub = values[49];
        self.phig = values[50];
        self.epsrgate = values[51];
        self.easub = values[52];
        self.epsrsub = values[53];
        self.ni0sub = values[54];
        self.bg0sub = values[55];
        self.tbgasub = values[56];
        self.tbgbsub = values[57];
        self.ndep = values[58];
        self.nsd = values[59];
        self.phin = values[60];
        self.ngate = values[61];
        self.gamma1 = values[62];
        self.gamma2 = values[63];
        self.vbx = values[64];
        self.vbm = values[65];
        self.xt = values[66];
        self.k1 = values[67];
        self.kt1 = values[68];
        self.kt1l = values[69];
        self.kt2 = values[70];
        self.k2 = values[71];
        self.k3 = values[72];
        self.k3b = values[73];
        self.w0 = values[74];
        self.dvtp0 = values[75];
        self.dvtp1 = values[76];
        self.dvtp2 = values[77];
        self.dvtp3 = values[78];
        self.dvtp4 = values[79];
        self.dvtp5 = values[80];
        self.lpe0 = values[81];
        self.lpeb = values[82];
        self.dvt0 = values[83];
        self.dvt1 = values[84];
        self.dvt2 = values[85];
        self.dvt0w = values[86];
        self.dvt1w = values[87];
        self.dvt2w = values[88];
        self.drout = values[89];
        self.dsub = values[90];
        self.vth0 = values[91];
        self.eu = values[92];
        self.ucs = values[93];
        self.ua = values[94];
        self.ua1 = values[95];
        self.ub = values[96];
        self.ub1 = values[97];
        self.uc = values[98];
        self.uc1 = values[99];
        self.ud = values[100];
        self.ud1 = values[101];
        self.up = values[102];
        self.lp = values[103];
        self.u0 = values[104];
        self.ute = values[105];
        self.ucste = values[106];
        self.voff = values[107];
        self.tvoff = values[108];
        self.tnfactor = values[109];
        self.teta0 = values[110];
        self.tvoffcv = values[111];
        self.minv = values[112];
        self.minvcv = values[113];
        self.voffl = values[114];
        self.voffcvl = values[115];
        self.delta = values[116];
        self.rdsw = values[117];
        self.rdswmin = values[118];
        self.rdwmin = values[119];
        self.rswmin = values[120];
        self.rsw = values[121];
        self.rdw = values[122];
        self.prwg = values[123];
        self.prwb = values[124];
        self.prt = values[125];
        self.eta0 = values[126];
        self.etab = values[127];
        self.pclm = values[128];
        self.pdibl1 = values[129];
        self.pdibl2 = values[130];
        self.pdiblb = values[131];
        self.fprout = values[132];
        self.pdits = values[133];
        self.pditsd = values[134];
        self.pditsl = values[135];
        self.pscbe1 = values[136];
        self.pscbe2 = values[137];
        self.pvag = values[138];
        self.wr = values[139];
        self.dwg = values[140];
        self.dwb = values[141];
        self.b0 = values[142];
        self.b1 = values[143];
        self.alpha0 = values[144];
        self.alpha1 = values[145];
        self.beta0 = values[146];
        self.agidl = values[147];
        self.bgidl = values[148];
        self.cgidl = values[149];
        self.egidl = values[150];
        self.fgidl = values[151];
        self.kgidl = values[152];
        self.rgidl = values[153];
        self.agisl = values[154];
        self.bgisl = values[155];
        self.cgisl = values[156];
        self.egisl = values[157];
        self.fgisl = values[158];
        self.kgisl = values[159];
        self.rgisl = values[160];
        self.aigc = values[161];
        self.bigc = values[162];
        self.cigc = values[163];
        self.aigsd = values[164];
        self.bigsd = values[165];
        self.cigsd = values[166];
        self.aigs = values[167];
        self.bigs = values[168];
        self.cigs = values[169];
        self.aigd = values[170];
        self.bigd = values[171];
        self.cigd = values[172];
        self.aigbacc = values[173];
        self.bigbacc = values[174];
        self.cigbacc = values[175];
        self.aigbinv = values[176];
        self.bigbinv = values[177];
        self.cigbinv = values[178];
        self.nigc = values[179];
        self.nigbacc = values[180];
        self.nigbinv = values[181];
        self.ntox = values[182];
        self.eigbinv = values[183];
        self.pigcd = values[184];
        self.poxedge = values[185];
        self.toxref = values[186];
        self.ijthdfwd = values[187];
        self.ijthsfwd = values[188];
        self.ijthdrev = values[189];
        self.ijthsrev = values[190];
        self.xjbvd = values[191];
        self.xjbvs = values[192];
        self.bvd = values[193];
        self.bvs = values[194];
        self.jtss = values[195];
        self.jtsd = values[196];
        self.jtssws = values[197];
        self.jtsswd = values[198];
        self.jtsswgs = values[199];
        self.jtsswgd = values[200];
        self.jtweff = values[201];
        self.njts = values[202];
        self.njtssw = values[203];
        self.njtsswg = values[204];
        self.njtsd = values[205];
        self.njtsswd = values[206];
        self.njtsswgd = values[207];
        self.xtss = values[208];
        self.xtsd = values[209];
        self.xtssws = values[210];
        self.xtsswd = values[211];
        self.xtsswgs = values[212];
        self.xtsswgd = values[213];
        self.tnjts = values[214];
        self.tnjtssw = values[215];
        self.tnjtsswg = values[216];
        self.tnjtsd = values[217];
        self.tnjtsswd = values[218];
        self.tnjtsswgd = values[219];
        self.vtss = values[220];
        self.vtsd = values[221];
        self.vtssws = values[222];
        self.vtsswd = values[223];
        self.vtsswgs = values[224];
        self.vtsswgd = values[225];
        self.xrcrg1 = values[226];
        self.xrcrg2 = values[227];
        self.lambda = values[228];
        self.vtl = values[229];
        self.lc = values[230];
        self.xn = values[231];
        self.vfbsdoff = values[232];
        self.lintnoi = values[233];
        self.tvfbsdoff = values[234];
        self.vfb = values[235];
        self.gbmin = values[236];
        self.rbdb = values[237];
        self.rbsb = values[238];
        self.rbpb = values[239];
        self.rbps = values[240];
        self.rbpd = values[241];
        self.rbps0 = values[242];
        self.rbpsl = values[243];
        self.rbpsw = values[244];
        self.rbpsnf = values[245];
        self.rbpd0 = values[246];
        self.rbpdl = values[247];
        self.rbpdw = values[248];
        self.rbpdnf = values[249];
        self.rbpbx0 = values[250];
        self.rbpbxl = values[251];
        self.rbpbxw = values[252];
        self.rbpbxnf = values[253];
        self.rbpby0 = values[254];
        self.rbpbyl = values[255];
        self.rbpbyw = values[256];
        self.rbpbynf = values[257];
        self.rbsbx0 = values[258];
        self.rbsby0 = values[259];
        self.rbdbx0 = values[260];
        self.rbdby0 = values[261];
        self.rbsdbxl = values[262];
        self.rbsdbxw = values[263];
        self.rbsdbxnf = values[264];
        self.rbsdbyl = values[265];
        self.rbsdbyw = values[266];
        self.rbsdbynf = values[267];
        self.tnoia = values[268];
        self.tnoib = values[269];
        self.tnoic = values[270];
        self.rnoia = values[271];
        self.rnoib = values[272];
        self.rnoic = values[273];
        self.ntnoi = values[274];
        self.cgsl = values[275];
        self.cgdl = values[276];
        self.ckappas = values[277];
        self.ckappad = values[278];
        self.cf = values[279];
        self.vfbcv = values[280];
        self.clc = values[281];
        self.cle = values[282];
        self.dwc = values[283];
        self.dlc = values[284];
        self.xw = values[285];
        self.xl = values[286];
        self.dlcig = values[287];
        self.dlcigd = values[288];
        self.dwj = values[289];
        self.noff = values[290];
        self.voffcv = values[291];
        self.acde = values[292];
        self.moin = values[293];
        self.tcj = values[294];
        self.tcjsw = values[295];
        self.tcjswg = values[296];
        self.tpb = values[297];
        self.tpbsw = values[298];
        self.tpbswg = values[299];
        self.dmcg = values[300];
        self.dmci = values[301];
        self.dmdg = values[302];
        self.dmcgt = values[303];
        self.xgw = values[304];
        self.xgl = values[305];
        self.rshg = values[306];
        self.ngcon = values[307];
        self.lcdsc = values[308];
        self.lcdscb = values[309];
        self.lcdscd = values[310];
        self.lcit = values[311];
        self.lnfactor = values[312];
        self.lxj = values[313];
        self.lvsat = values[314];
        self.lat = values[315];
        self.la0 = values[316];
        self.lags = values[317];
        self.la1 = values[318];
        self.la2 = values[319];
        self.lketa = values[320];
        self.lketac = values[321];
        self.lnsub = values[322];
        self.lndep = values[323];
        self.lnsd = values[324];
        self.lphin = values[325];
        self.lngate = values[326];
        self.lgamma1 = values[327];
        self.lgamma2 = values[328];
        self.lvbx = values[329];
        self.lvbm = values[330];
        self.lxt = values[331];
        self.lk1 = values[332];
        self.lkt1 = values[333];
        self.lkt1l = values[334];
        self.lkt2 = values[335];
        self.lk2 = values[336];
        self.lk3 = values[337];
        self.lk3b = values[338];
        self.lw0 = values[339];
        self.ldvtp0 = values[340];
        self.ldvtp1 = values[341];
        self.ldvtp2 = values[342];
        self.ldvtp3 = values[343];
        self.ldvtp4 = values[344];
        self.ldvtp5 = values[345];
        self.llpe0 = values[346];
        self.llpeb = values[347];
        self.ldvt0 = values[348];
        self.ldvt1 = values[349];
        self.ldvt2 = values[350];
        self.ldvt0w = values[351];
        self.ldvt1w = values[352];
        self.ldvt2w = values[353];
        self.ldrout = values[354];
        self.ldsub = values[355];
        self.lvth0 = values[356];
        self.lua = values[357];
        self.lua1 = values[358];
        self.lub = values[359];
        self.lub1 = values[360];
        self.luc = values[361];
        self.luc1 = values[362];
        self.lud = values[363];
        self.lud1 = values[364];
        self.lup = values[365];
        self.llp = values[366];
        self.lu0 = values[367];
        self.leu = values[368];
        self.lucs = values[369];
        self.lute = values[370];
        self.lucste = values[371];
        self.lvoff = values[372];
        self.ltvoff = values[373];
        self.ltnfactor = values[374];
        self.lteta0 = values[375];
        self.ltvoffcv = values[376];
        self.lminv = values[377];
        self.lminvcv = values[378];
        self.ldelta = values[379];
        self.lrdsw = values[380];
        self.lrsw = values[381];
        self.lrdw = values[382];
        self.lprwg = values[383];
        self.lprwb = values[384];
        self.lprt = values[385];
        self.leta0 = values[386];
        self.letab = values[387];
        self.lpclm = values[388];
        self.lpdibl1 = values[389];
        self.lpdibl2 = values[390];
        self.lpdiblb = values[391];
        self.lfprout = values[392];
        self.lpdits = values[393];
        self.lpditsd = values[394];
        self.lpscbe1 = values[395];
        self.lpscbe2 = values[396];
        self.lpvag = values[397];
        self.lwr = values[398];
        self.ldwg = values[399];
        self.ldwb = values[400];
        self.lb0 = values[401];
        self.lb1 = values[402];
        self.lalpha0 = values[403];
        self.lalpha1 = values[404];
        self.lbeta0 = values[405];
        self.lvfb = values[406];
        self.lagidl = values[407];
        self.lbgidl = values[408];
        self.lcgidl = values[409];
        self.legidl = values[410];
        self.lfgidl = values[411];
        self.lkgidl = values[412];
        self.lrgidl = values[413];
        self.lagisl = values[414];
        self.lbgisl = values[415];
        self.lcgisl = values[416];
        self.legisl = values[417];
        self.lfgisl = values[418];
        self.lkgisl = values[419];
        self.lrgisl = values[420];
        self.laigc = values[421];
        self.lbigc = values[422];
        self.lcigc = values[423];
        self.laigsd = values[424];
        self.lbigsd = values[425];
        self.lcigsd = values[426];
        self.laigs = values[427];
        self.lbigs = values[428];
        self.lcigs = values[429];
        self.laigd = values[430];
        self.lbigd = values[431];
        self.lcigd = values[432];
        self.laigbacc = values[433];
        self.lbigbacc = values[434];
        self.lcigbacc = values[435];
        self.laigbinv = values[436];
        self.lbigbinv = values[437];
        self.lcigbinv = values[438];
        self.lnigc = values[439];
        self.lnigbacc = values[440];
        self.lnigbinv = values[441];
        self.lntox = values[442];
        self.leigbinv = values[443];
        self.lpigcd = values[444];
        self.lpoxedge = values[445];
        self.lxrcrg1 = values[446];
        self.lxrcrg2 = values[447];
        self.llambda = values[448];
        self.lvtl = values[449];
        self.lxn = values[450];
        self.lvfbsdoff = values[451];
        self.ltvfbsdoff = values[452];
        self.lcgsl = values[453];
        self.lcgdl = values[454];
        self.lckappas = values[455];
        self.lckappad = values[456];
        self.lcf = values[457];
        self.lclc = values[458];
        self.lcle = values[459];
        self.lvfbcv = values[460];
        self.lnoff = values[461];
        self.lvoffcv = values[462];
        self.lacde = values[463];
        self.lmoin = values[464];
        self.wcdsc = values[465];
        self.wcdscb = values[466];
        self.wcdscd = values[467];
        self.wcit = values[468];
        self.wnfactor = values[469];
        self.wxj = values[470];
        self.wvsat = values[471];
        self.wat = values[472];
        self.wa0 = values[473];
        self.wags = values[474];
        self.wa1 = values[475];
        self.wa2 = values[476];
        self.wketa = values[477];
        self.wketac = values[478];
        self.wnsub = values[479];
        self.wndep = values[480];
        self.wnsd = values[481];
        self.wphin = values[482];
        self.wngate = values[483];
        self.wgamma1 = values[484];
        self.wgamma2 = values[485];
        self.wvbx = values[486];
        self.wvbm = values[487];
        self.wxt = values[488];
        self.wk1 = values[489];
        self.wkt1 = values[490];
        self.wkt1l = values[491];
        self.wkt2 = values[492];
        self.wk2 = values[493];
        self.wk3 = values[494];
        self.wk3b = values[495];
        self.ww0 = values[496];
        self.wdvtp0 = values[497];
        self.wdvtp1 = values[498];
        self.wdvtp2 = values[499];
        self.wdvtp3 = values[500];
        self.wdvtp4 = values[501];
        self.wdvtp5 = values[502];
        self.wlpe0 = values[503];
        self.wlpeb = values[504];
        self.wdvt0 = values[505];
        self.wdvt1 = values[506];
        self.wdvt2 = values[507];
        self.wdvt0w = values[508];
        self.wdvt1w = values[509];
        self.wdvt2w = values[510];
        self.wdrout = values[511];
        self.wdsub = values[512];
        self.wvth0 = values[513];
        self.wua = values[514];
        self.wua1 = values[515];
        self.wub = values[516];
        self.wub1 = values[517];
        self.wuc = values[518];
        self.wuc1 = values[519];
        self.wud = values[520];
        self.wud1 = values[521];
        self.wup = values[522];
        self.wlp = values[523];
        self.wu0 = values[524];
        self.weu = values[525];
        self.wucs = values[526];
        self.wute = values[527];
        self.wucste = values[528];
        self.wvoff = values[529];
        self.wtvoff = values[530];
        self.wtnfactor = values[531];
        self.wteta0 = values[532];
        self.wtvoffcv = values[533];
        self.wminv = values[534];
        self.wminvcv = values[535];
        self.wdelta = values[536];
        self.wrdsw = values[537];
        self.wrsw = values[538];
        self.wrdw = values[539];
        self.wprwg = values[540];
        self.wprwb = values[541];
        self.wprt = values[542];
        self.weta0 = values[543];
        self.wetab = values[544];
        self.wpclm = values[545];
        self.wpdibl1 = values[546];
        self.wpdibl2 = values[547];
        self.wpdiblb = values[548];
        self.wfprout = values[549];
        self.wpdits = values[550];
        self.wpditsd = values[551];
        self.wpscbe1 = values[552];
        self.wpscbe2 = values[553];
        self.wpvag = values[554];
        self.wwr = values[555];
        self.wdwg = values[556];
        self.wdwb = values[557];
        self.wb0 = values[558];
        self.wb1 = values[559];
        self.walpha0 = values[560];
        self.walpha1 = values[561];
        self.wbeta0 = values[562];
        self.wvfb = values[563];
        self.wagidl = values[564];
        self.wbgidl = values[565];
        self.wcgidl = values[566];
        self.wegidl = values[567];
        self.wfgidl = values[568];
        self.wkgidl = values[569];
        self.wrgidl = values[570];
        self.wagisl = values[571];
        self.wbgisl = values[572];
        self.wcgisl = values[573];
        self.wegisl = values[574];
        self.wfgisl = values[575];
        self.wkgisl = values[576];
        self.wrgisl = values[577];
        self.waigc = values[578];
        self.wbigc = values[579];
        self.wcigc = values[580];
        self.waigsd = values[581];
        self.wbigsd = values[582];
        self.wcigsd = values[583];
        self.waigs = values[584];
        self.wbigs = values[585];
        self.wcigs = values[586];
        self.waigd = values[587];
        self.wbigd = values[588];
        self.wcigd = values[589];
        self.waigbacc = values[590];
        self.wbigbacc = values[591];
        self.wcigbacc = values[592];
        self.waigbinv = values[593];
        self.wbigbinv = values[594];
        self.wcigbinv = values[595];
        self.wnigc = values[596];
        self.wnigbacc = values[597];
        self.wnigbinv = values[598];
        self.wntox = values[599];
        self.weigbinv = values[600];
        self.wpigcd = values[601];
        self.wpoxedge = values[602];
        self.wxrcrg1 = values[603];
        self.wxrcrg2 = values[604];
        self.wlambda = values[605];
        self.wvtl = values[606];
        self.wxn = values[607];
        self.wvfbsdoff = values[608];
        self.wtvfbsdoff = values[609];
        self.wcgsl = values[610];
        self.wcgdl = values[611];
        self.wckappas = values[612];
        self.wckappad = values[613];
        self.wcf = values[614];
        self.wclc = values[615];
        self.wcle = values[616];
        self.wvfbcv = values[617];
        self.wnoff = values[618];
        self.wvoffcv = values[619];
        self.wacde = values[620];
        self.wmoin = values[621];
        self.pcdsc = values[622];
        self.pcdscb = values[623];
        self.pcdscd = values[624];
        self.pcit = values[625];
        self.pnfactor = values[626];
        self.pxj = values[627];
        self.pvsat = values[628];
        self.pat = values[629];
        self.pa0 = values[630];
        self.pags = values[631];
        self.pa1 = values[632];
        self.pa2 = values[633];
        self.pketa = values[634];
        self.pketac = values[635];
        self.pnsub = values[636];
        self.pndep = values[637];
        self.pnsd = values[638];
        self.pphin = values[639];
        self.pngate = values[640];
        self.pgamma1 = values[641];
        self.pgamma2 = values[642];
        self.pvbx = values[643];
        self.pvbm = values[644];
        self.pxt = values[645];
        self.pk1 = values[646];
        self.pkt1 = values[647];
        self.pkt1l = values[648];
        self.pkt2 = values[649];
        self.pk2 = values[650];
        self.pk3 = values[651];
        self.pk3b = values[652];
        self.pw0 = values[653];
        self.pdvtp0 = values[654];
        self.pdvtp1 = values[655];
        self.pdvtp2 = values[656];
        self.pdvtp3 = values[657];
        self.pdvtp4 = values[658];
        self.pdvtp5 = values[659];
        self.plpe0 = values[660];
        self.plpeb = values[661];
        self.pdvt0 = values[662];
        self.pdvt1 = values[663];
        self.pdvt2 = values[664];
        self.pdvt0w = values[665];
        self.pdvt1w = values[666];
        self.pdvt2w = values[667];
        self.pdrout = values[668];
        self.pdsub = values[669];
        self.pvth0 = values[670];
        self.pua = values[671];
        self.pua1 = values[672];
        self.r#pub = values[673];
        self.pub1 = values[674];
        self.puc = values[675];
        self.puc1 = values[676];
        self.pud = values[677];
        self.pud1 = values[678];
        self.pup = values[679];
        self.plp = values[680];
        self.pu0 = values[681];
        self.peu = values[682];
        self.pucs = values[683];
        self.pute = values[684];
        self.pucste = values[685];
        self.pvoff = values[686];
        self.ptvoff = values[687];
        self.ptnfactor = values[688];
        self.pteta0 = values[689];
        self.ptvoffcv = values[690];
        self.pminv = values[691];
        self.pminvcv = values[692];
        self.pdelta = values[693];
        self.prdsw = values[694];
        self.prsw = values[695];
        self.prdw = values[696];
        self.pprwg = values[697];
        self.pprwb = values[698];
        self.pprt = values[699];
        self.peta0 = values[700];
        self.petab = values[701];
        self.ppclm = values[702];
        self.ppdibl1 = values[703];
        self.ppdibl2 = values[704];
        self.ppdiblb = values[705];
        self.pfprout = values[706];
        self.ppdits = values[707];
        self.ppditsd = values[708];
        self.ppscbe1 = values[709];
        self.ppscbe2 = values[710];
        self.ppvag = values[711];
        self.pwr = values[712];
        self.pdwg = values[713];
        self.pdwb = values[714];
        self.pb0 = values[715];
        self.pb1 = values[716];
        self.palpha0 = values[717];
        self.palpha1 = values[718];
        self.pbeta0 = values[719];
        self.pvfb = values[720];
        self.pagidl = values[721];
        self.pbgidl = values[722];
        self.pcgidl = values[723];
        self.pegidl = values[724];
        self.pfgidl = values[725];
        self.pkgidl = values[726];
        self.prgidl = values[727];
        self.pagisl = values[728];
        self.pbgisl = values[729];
        self.pcgisl = values[730];
        self.pegisl = values[731];
        self.pfgisl = values[732];
        self.pkgisl = values[733];
        self.prgisl = values[734];
        self.paigc = values[735];
        self.pbigc = values[736];
        self.pcigc = values[737];
        self.paigsd = values[738];
        self.pbigsd = values[739];
        self.pcigsd = values[740];
        self.paigs = values[741];
        self.pbigs = values[742];
        self.pcigs = values[743];
        self.paigd = values[744];
        self.pbigd = values[745];
        self.pcigd = values[746];
        self.paigbacc = values[747];
        self.pbigbacc = values[748];
        self.pcigbacc = values[749];
        self.paigbinv = values[750];
        self.pbigbinv = values[751];
        self.pcigbinv = values[752];
        self.pnigc = values[753];
        self.pnigbacc = values[754];
        self.pnigbinv = values[755];
        self.pntox = values[756];
        self.peigbinv = values[757];
        self.ppigcd = values[758];
        self.ppoxedge = values[759];
        self.pxrcrg1 = values[760];
        self.pxrcrg2 = values[761];
        self.plambda = values[762];
        self.pvtl = values[763];
        self.pxn = values[764];
        self.pvfbsdoff = values[765];
        self.ptvfbsdoff = values[766];
        self.pcgsl = values[767];
        self.pcgdl = values[768];
        self.pckappas = values[769];
        self.pckappad = values[770];
        self.pcf = values[771];
        self.pclc = values[772];
        self.pcle = values[773];
        self.pvfbcv = values[774];
        self.pnoff = values[775];
        self.pvoffcv = values[776];
        self.pacde = values[777];
        self.pmoin = values[778];
        self.tnom = values[779];
        self.cgso = values[780];
        self.cgdo = values[781];
        self.cgbo = values[782];
        self.xpart = values[783];
        self.cFringOut = values[784];
        self.cFringMax = values[785];
        self.sheetResistance = values[786];
        self.SjctSatCurDensity = values[787];
        self.DjctSatCurDensity = values[788];
        self.SjctSidewallSatCurDensity = values[789];
        self.DjctSidewallSatCurDensity = values[790];
        self.SjctGateSidewallSatCurDensity = values[791];
        self.DjctGateSidewallSatCurDensity = values[792];
        self.SbulkJctPotential = values[793];
        self.DbulkJctPotential = values[794];
        self.SbulkJctBotGradingCoeff = values[795];
        self.DbulkJctBotGradingCoeff = values[796];
        self.SbulkJctSideGradingCoeff = values[797];
        self.DbulkJctSideGradingCoeff = values[798];
        self.SbulkJctGateSideGradingCoeff = values[799];
        self.DbulkJctGateSideGradingCoeff = values[800];
        self.SsidewallJctPotential = values[801];
        self.DsidewallJctPotential = values[802];
        self.SGatesidewallJctPotential = values[803];
        self.DGatesidewallJctPotential = values[804];
        self.SunitAreaJctCap = values[805];
        self.DunitAreaJctCap = values[806];
        self.SunitLengthSidewallJctCap = values[807];
        self.DunitLengthSidewallJctCap = values[808];
        self.SunitLengthGateSidewallJctCap = values[809];
        self.DunitLengthGateSidewallJctCap = values[810];
        self.SjctEmissionCoeff = values[811];
        self.DjctEmissionCoeff = values[812];
        self.SjctTempExponent = values[813];
        self.DjctTempExponent = values[814];
        self.njtsstemp = values[815];
        self.njtsswstemp = values[816];
        self.njtsswgstemp = values[817];
        self.njtsdtemp = values[818];
        self.njtsswdtemp = values[819];
        self.njtsswgdtemp = values[820];
        self.Lint = values[821];
        self.Ll = values[822];
        self.Llc = values[823];
        self.Lln = values[824];
        self.Lw = values[825];
        self.Lwc = values[826];
        self.Lwn = values[827];
        self.Lwl = values[828];
        self.Lwlc = values[829];
        self.Lmin = values[830];
        self.Lmax = values[831];
        self.Wint = values[832];
        self.Wl = values[833];
        self.Wlc = values[834];
        self.Wln = values[835];
        self.Ww = values[836];
        self.Wwc = values[837];
        self.Wwn = values[838];
        self.Wwl = values[839];
        self.Wwlc = values[840];
        self.Wmin = values[841];
        self.Wmax = values[842];
        self.saref = values[843];
        self.sbref = values[844];
        self.wlod = values[845];
        self.ku0 = values[846];
        self.kvsat = values[847];
        self.kvth0 = values[848];
        self.tku0 = values[849];
        self.llodku0 = values[850];
        self.wlodku0 = values[851];
        self.llodvth = values[852];
        self.wlodvth = values[853];
        self.lku0 = values[854];
        self.wku0 = values[855];
        self.pku0 = values[856];
        self.lkvth0 = values[857];
        self.wkvth0 = values[858];
        self.pkvth0 = values[859];
        self.stk2 = values[860];
        self.lodk2 = values[861];
        self.steta0 = values[862];
        self.lodeta0 = values[863];
        self.web = values[864];
        self.wec = values[865];
        self.kvth0we = values[866];
        self.k2we = values[867];
        self.ku0we = values[868];
        self.scref = values[869];
        self.wpemod = values[870];
        self.lkvth0we = values[871];
        self.lk2we = values[872];
        self.lku0we = values[873];
        self.wkvth0we = values[874];
        self.wk2we = values[875];
        self.wku0we = values[876];
        self.pkvth0we = values[877];
        self.pk2we = values[878];
        self.pku0we = values[879];
        self.Eg0 = values[880];
        self.vtm = values[881];
        self.vtm0 = values[882];
        self.coxe = values[883];
        self.coxp = values[884];
        self.cof1 = values[885];
        self.cof2 = values[886];
        self.cof3 = values[887];
        self.cof4 = values[888];
        self.vcrit = values[889];
        self.factor1 = values[890];
        self.PhiBS = values[891];
        self.PhiBSWS = values[892];
        self.PhiBSWGS = values[893];
        self.SjctTempSatCurDensity = values[894];
        self.SjctSidewallTempSatCurDensity = values[895];
        self.SjctGateSidewallTempSatCurDensity = values[896];
        self.PhiBD = values[897];
        self.PhiBSWD = values[898];
        self.PhiBSWGD = values[899];
        self.DjctTempSatCurDensity = values[900];
        self.DjctSidewallTempSatCurDensity = values[901];
        self.DjctGateSidewallTempSatCurDensity = values[902];
        self.SunitAreaTempJctCap = values[903];
        self.DunitAreaTempJctCap = values[904];
        self.SunitLengthSidewallTempJctCap = values[905];
        self.DunitLengthSidewallTempJctCap = values[906];
        self.SunitLengthGateSidewallTempJctCap = values[907];
        self.DunitLengthGateSidewallTempJctCap = values[908];
        self.oxideTrapDensityA = values[909];
        self.oxideTrapDensityB = values[910];
        self.oxideTrapDensityC = values[911];
        self.em = values[912];
        self.ef = values[913];
        self.af = values[914];
        self.kf = values[915];
        self.vgsMax = values[916];
        self.vgdMax = values[917];
        self.vgbMax = values[918];
        self.vdsMax = values[919];
        self.vbsMax = values[920];
        self.vbdMax = values[921];
        self.vgsrMax = values[922];
        self.vgdrMax = values[923];
        self.vgbrMax = values[924];
        self.vbsrMax = values[925];
        self.vbdrMax = values[926];
        self.gidlclamp = values[927];
        self.idovvdsc = values[928];
        self.InstCount = values[929] as i32;
        self.mobModGiven = values[930] != 0.0;
        self.binUnitGiven = values[931] != 0.0;
        self.cvchargeModGiven = values[932] != 0.0;
        self.capModGiven = values[933] != 0.0;
        self.dioModGiven = values[934] != 0.0;
        self.rdsModGiven = values[935] != 0.0;
        self.rbodyModGiven = values[936] != 0.0;
        self.rgateModGiven = values[937] != 0.0;
        self.perModGiven = values[938] != 0.0;
        self.geoModGiven = values[939] != 0.0;
        self.paramChkGiven = values[940] != 0.0;
        self.trnqsModGiven = values[941] != 0.0;
        self.acnqsModGiven = values[942] != 0.0;
        self.fnoiModGiven = values[943] != 0.0;
        self.tnoiModGiven = values[944] != 0.0;
        self.mtrlModGiven = values[945] != 0.0;
        self.mtrlCompatModGiven = values[946] != 0.0;
        self.gidlModGiven = values[947] != 0.0;
        self.igcModGiven = values[948] != 0.0;
        self.igbModGiven = values[949] != 0.0;
        self.tempModGiven = values[950] != 0.0;
        self.typeGiven = values[951] != 0.0;
        self.toxrefGiven = values[952] != 0.0;
        self.eotGiven = values[953] != 0.0;
        self.vddeotGiven = values[954] != 0.0;
        self.tempeotGiven = values[955] != 0.0;
        self.leffeotGiven = values[956] != 0.0;
        self.weffeotGiven = values[957] != 0.0;
        self.adosGiven = values[958] != 0.0;
        self.bdosGiven = values[959] != 0.0;
        self.toxeGiven = values[960] != 0.0;
        self.toxpGiven = values[961] != 0.0;
        self.toxmGiven = values[962] != 0.0;
        self.dtoxGiven = values[963] != 0.0;
        self.epsroxGiven = values[964] != 0.0;
        self.versionGiven = values[965] != 0.0;
        self.cdscGiven = values[966] != 0.0;
        self.cdscbGiven = values[967] != 0.0;
        self.cdscdGiven = values[968] != 0.0;
        self.citGiven = values[969] != 0.0;
        self.nfactorGiven = values[970] != 0.0;
        self.xjGiven = values[971] != 0.0;
        self.vsatGiven = values[972] != 0.0;
        self.atGiven = values[973] != 0.0;
        self.a0Given = values[974] != 0.0;
        self.agsGiven = values[975] != 0.0;
        self.a1Given = values[976] != 0.0;
        self.a2Given = values[977] != 0.0;
        self.ketaGiven = values[978] != 0.0;
        self.ketacGiven = values[979] != 0.0;
        self.nsubGiven = values[980] != 0.0;
        self.phigGiven = values[981] != 0.0;
        self.epsrgateGiven = values[982] != 0.0;
        self.easubGiven = values[983] != 0.0;
        self.epsrsubGiven = values[984] != 0.0;
        self.ni0subGiven = values[985] != 0.0;
        self.bg0subGiven = values[986] != 0.0;
        self.tbgasubGiven = values[987] != 0.0;
        self.tbgbsubGiven = values[988] != 0.0;
        self.ndepGiven = values[989] != 0.0;
        self.nsdGiven = values[990] != 0.0;
        self.phinGiven = values[991] != 0.0;
        self.ngateGiven = values[992] != 0.0;
        self.gamma1Given = values[993] != 0.0;
        self.gamma2Given = values[994] != 0.0;
        self.vbxGiven = values[995] != 0.0;
        self.vbmGiven = values[996] != 0.0;
        self.xtGiven = values[997] != 0.0;
        self.k1Given = values[998] != 0.0;
        self.kt1Given = values[999] != 0.0;
        self.kt1lGiven = values[1000] != 0.0;
        self.kt2Given = values[1001] != 0.0;
        self.k2Given = values[1002] != 0.0;
        self.k3Given = values[1003] != 0.0;
        self.k3bGiven = values[1004] != 0.0;
        self.w0Given = values[1005] != 0.0;
        self.dvtp0Given = values[1006] != 0.0;
        self.dvtp1Given = values[1007] != 0.0;
        self.dvtp2Given = values[1008] != 0.0;
        self.dvtp3Given = values[1009] != 0.0;
        self.dvtp4Given = values[1010] != 0.0;
        self.dvtp5Given = values[1011] != 0.0;
        self.lpe0Given = values[1012] != 0.0;
        self.lpebGiven = values[1013] != 0.0;
        self.dvt0Given = values[1014] != 0.0;
        self.dvt1Given = values[1015] != 0.0;
        self.dvt2Given = values[1016] != 0.0;
        self.dvt0wGiven = values[1017] != 0.0;
        self.dvt1wGiven = values[1018] != 0.0;
        self.dvt2wGiven = values[1019] != 0.0;
        self.droutGiven = values[1020] != 0.0;
        self.dsubGiven = values[1021] != 0.0;
        self.vth0Given = values[1022] != 0.0;
        self.euGiven = values[1023] != 0.0;
        self.ucsGiven = values[1024] != 0.0;
        self.uaGiven = values[1025] != 0.0;
        self.ua1Given = values[1026] != 0.0;
        self.ubGiven = values[1027] != 0.0;
        self.ub1Given = values[1028] != 0.0;
        self.ucGiven = values[1029] != 0.0;
        self.uc1Given = values[1030] != 0.0;
        self.udGiven = values[1031] != 0.0;
        self.ud1Given = values[1032] != 0.0;
        self.upGiven = values[1033] != 0.0;
        self.lpGiven = values[1034] != 0.0;
        self.u0Given = values[1035] != 0.0;
        self.uteGiven = values[1036] != 0.0;
        self.ucsteGiven = values[1037] != 0.0;
        self.voffGiven = values[1038] != 0.0;
        self.tvoffGiven = values[1039] != 0.0;
        self.tnfactorGiven = values[1040] != 0.0;
        self.teta0Given = values[1041] != 0.0;
        self.tvoffcvGiven = values[1042] != 0.0;
        self.vofflGiven = values[1043] != 0.0;
        self.voffcvlGiven = values[1044] != 0.0;
        self.minvGiven = values[1045] != 0.0;
        self.minvcvGiven = values[1046] != 0.0;
        self.rdswGiven = values[1047] != 0.0;
        self.rdswminGiven = values[1048] != 0.0;
        self.rdwminGiven = values[1049] != 0.0;
        self.rswminGiven = values[1050] != 0.0;
        self.rswGiven = values[1051] != 0.0;
        self.rdwGiven = values[1052] != 0.0;
        self.prwgGiven = values[1053] != 0.0;
        self.prwbGiven = values[1054] != 0.0;
        self.prtGiven = values[1055] != 0.0;
        self.eta0Given = values[1056] != 0.0;
        self.etabGiven = values[1057] != 0.0;
        self.pclmGiven = values[1058] != 0.0;
        self.pdibl1Given = values[1059] != 0.0;
        self.pdibl2Given = values[1060] != 0.0;
        self.pdiblbGiven = values[1061] != 0.0;
        self.fproutGiven = values[1062] != 0.0;
        self.pditsGiven = values[1063] != 0.0;
        self.pditsdGiven = values[1064] != 0.0;
        self.pditslGiven = values[1065] != 0.0;
        self.pscbe1Given = values[1066] != 0.0;
        self.pscbe2Given = values[1067] != 0.0;
        self.pvagGiven = values[1068] != 0.0;
        self.deltaGiven = values[1069] != 0.0;
        self.wrGiven = values[1070] != 0.0;
        self.dwgGiven = values[1071] != 0.0;
        self.dwbGiven = values[1072] != 0.0;
        self.b0Given = values[1073] != 0.0;
        self.b1Given = values[1074] != 0.0;
        self.alpha0Given = values[1075] != 0.0;
        self.alpha1Given = values[1076] != 0.0;
        self.beta0Given = values[1077] != 0.0;
        self.agidlGiven = values[1078] != 0.0;
        self.bgidlGiven = values[1079] != 0.0;
        self.cgidlGiven = values[1080] != 0.0;
        self.egidlGiven = values[1081] != 0.0;
        self.fgidlGiven = values[1082] != 0.0;
        self.kgidlGiven = values[1083] != 0.0;
        self.rgidlGiven = values[1084] != 0.0;
        self.agislGiven = values[1085] != 0.0;
        self.bgislGiven = values[1086] != 0.0;
        self.cgislGiven = values[1087] != 0.0;
        self.egislGiven = values[1088] != 0.0;
        self.fgislGiven = values[1089] != 0.0;
        self.kgislGiven = values[1090] != 0.0;
        self.rgislGiven = values[1091] != 0.0;
        self.aigcGiven = values[1092] != 0.0;
        self.bigcGiven = values[1093] != 0.0;
        self.cigcGiven = values[1094] != 0.0;
        self.aigsdGiven = values[1095] != 0.0;
        self.bigsdGiven = values[1096] != 0.0;
        self.cigsdGiven = values[1097] != 0.0;
        self.aigsGiven = values[1098] != 0.0;
        self.bigsGiven = values[1099] != 0.0;
        self.cigsGiven = values[1100] != 0.0;
        self.aigdGiven = values[1101] != 0.0;
        self.bigdGiven = values[1102] != 0.0;
        self.cigdGiven = values[1103] != 0.0;
        self.aigbaccGiven = values[1104] != 0.0;
        self.bigbaccGiven = values[1105] != 0.0;
        self.cigbaccGiven = values[1106] != 0.0;
        self.aigbinvGiven = values[1107] != 0.0;
        self.bigbinvGiven = values[1108] != 0.0;
        self.cigbinvGiven = values[1109] != 0.0;
        self.nigcGiven = values[1110] != 0.0;
        self.nigbinvGiven = values[1111] != 0.0;
        self.nigbaccGiven = values[1112] != 0.0;
        self.ntoxGiven = values[1113] != 0.0;
        self.eigbinvGiven = values[1114] != 0.0;
        self.pigcdGiven = values[1115] != 0.0;
        self.poxedgeGiven = values[1116] != 0.0;
        self.ijthdfwdGiven = values[1117] != 0.0;
        self.ijthsfwdGiven = values[1118] != 0.0;
        self.ijthdrevGiven = values[1119] != 0.0;
        self.ijthsrevGiven = values[1120] != 0.0;
        self.xjbvdGiven = values[1121] != 0.0;
        self.xjbvsGiven = values[1122] != 0.0;
        self.bvdGiven = values[1123] != 0.0;
        self.bvsGiven = values[1124] != 0.0;
        self.jtssGiven = values[1125] != 0.0;
        self.jtsdGiven = values[1126] != 0.0;
        self.jtsswsGiven = values[1127] != 0.0;
        self.jtsswdGiven = values[1128] != 0.0;
        self.jtsswgsGiven = values[1129] != 0.0;
        self.jtsswgdGiven = values[1130] != 0.0;
        self.jtweffGiven = values[1131] != 0.0;
        self.njtsGiven = values[1132] != 0.0;
        self.njtsswGiven = values[1133] != 0.0;
        self.njtsswgGiven = values[1134] != 0.0;
        self.njtsdGiven = values[1135] != 0.0;
        self.njtsswdGiven = values[1136] != 0.0;
        self.njtsswgdGiven = values[1137] != 0.0;
        self.xtssGiven = values[1138] != 0.0;
        self.xtsdGiven = values[1139] != 0.0;
        self.xtsswsGiven = values[1140] != 0.0;
        self.xtsswdGiven = values[1141] != 0.0;
        self.xtsswgsGiven = values[1142] != 0.0;
        self.xtsswgdGiven = values[1143] != 0.0;
        self.tnjtsGiven = values[1144] != 0.0;
        self.tnjtsswGiven = values[1145] != 0.0;
        self.tnjtsswgGiven = values[1146] != 0.0;
        self.tnjtsdGiven = values[1147] != 0.0;
        self.tnjtsswdGiven = values[1148] != 0.0;
        self.tnjtsswgdGiven = values[1149] != 0.0;
        self.vtssGiven = values[1150] != 0.0;
        self.vtsdGiven = values[1151] != 0.0;
        self.vtsswsGiven = values[1152] != 0.0;
        self.vtsswdGiven = values[1153] != 0.0;
        self.vtsswgsGiven = values[1154] != 0.0;
        self.vtsswgdGiven = values[1155] != 0.0;
        self.vfbGiven = values[1156] != 0.0;
        self.gbminGiven = values[1157] != 0.0;
        self.rbdbGiven = values[1158] != 0.0;
        self.rbsbGiven = values[1159] != 0.0;
        self.rbpsGiven = values[1160] != 0.0;
        self.rbpdGiven = values[1161] != 0.0;
        self.rbpbGiven = values[1162] != 0.0;
        self.rbps0Given = values[1163] != 0.0;
        self.rbpslGiven = values[1164] != 0.0;
        self.rbpswGiven = values[1165] != 0.0;
        self.rbpsnfGiven = values[1166] != 0.0;
        self.rbpd0Given = values[1167] != 0.0;
        self.rbpdlGiven = values[1168] != 0.0;
        self.rbpdwGiven = values[1169] != 0.0;
        self.rbpdnfGiven = values[1170] != 0.0;
        self.rbpbx0Given = values[1171] != 0.0;
        self.rbpbxlGiven = values[1172] != 0.0;
        self.rbpbxwGiven = values[1173] != 0.0;
        self.rbpbxnfGiven = values[1174] != 0.0;
        self.rbpby0Given = values[1175] != 0.0;
        self.rbpbylGiven = values[1176] != 0.0;
        self.rbpbywGiven = values[1177] != 0.0;
        self.rbpbynfGiven = values[1178] != 0.0;
        self.rbsbx0Given = values[1179] != 0.0;
        self.rbsby0Given = values[1180] != 0.0;
        self.rbdbx0Given = values[1181] != 0.0;
        self.rbdby0Given = values[1182] != 0.0;
        self.rbsdbxlGiven = values[1183] != 0.0;
        self.rbsdbxwGiven = values[1184] != 0.0;
        self.rbsdbxnfGiven = values[1185] != 0.0;
        self.rbsdbylGiven = values[1186] != 0.0;
        self.rbsdbywGiven = values[1187] != 0.0;
        self.rbsdbynfGiven = values[1188] != 0.0;
        self.xrcrg1Given = values[1189] != 0.0;
        self.xrcrg2Given = values[1190] != 0.0;
        self.tnoiaGiven = values[1191] != 0.0;
        self.tnoibGiven = values[1192] != 0.0;
        self.tnoicGiven = values[1193] != 0.0;
        self.rnoiaGiven = values[1194] != 0.0;
        self.rnoibGiven = values[1195] != 0.0;
        self.rnoicGiven = values[1196] != 0.0;
        self.ntnoiGiven = values[1197] != 0.0;
        self.lambdaGiven = values[1198] != 0.0;
        self.vtlGiven = values[1199] != 0.0;
        self.lcGiven = values[1200] != 0.0;
        self.xnGiven = values[1201] != 0.0;
        self.vfbsdoffGiven = values[1202] != 0.0;
        self.lintnoiGiven = values[1203] != 0.0;
        self.tvfbsdoffGiven = values[1204] != 0.0;
        self.cgslGiven = values[1205] != 0.0;
        self.cgdlGiven = values[1206] != 0.0;
        self.ckappasGiven = values[1207] != 0.0;
        self.ckappadGiven = values[1208] != 0.0;
        self.cfGiven = values[1209] != 0.0;
        self.vfbcvGiven = values[1210] != 0.0;
        self.clcGiven = values[1211] != 0.0;
        self.cleGiven = values[1212] != 0.0;
        self.dwcGiven = values[1213] != 0.0;
        self.dlcGiven = values[1214] != 0.0;
        self.xwGiven = values[1215] != 0.0;
        self.xlGiven = values[1216] != 0.0;
        self.dlcigGiven = values[1217] != 0.0;
        self.dlcigdGiven = values[1218] != 0.0;
        self.dwjGiven = values[1219] != 0.0;
        self.noffGiven = values[1220] != 0.0;
        self.voffcvGiven = values[1221] != 0.0;
        self.acdeGiven = values[1222] != 0.0;
        self.moinGiven = values[1223] != 0.0;
        self.tcjGiven = values[1224] != 0.0;
        self.tcjswGiven = values[1225] != 0.0;
        self.tcjswgGiven = values[1226] != 0.0;
        self.tpbGiven = values[1227] != 0.0;
        self.tpbswGiven = values[1228] != 0.0;
        self.tpbswgGiven = values[1229] != 0.0;
        self.dmcgGiven = values[1230] != 0.0;
        self.dmciGiven = values[1231] != 0.0;
        self.dmdgGiven = values[1232] != 0.0;
        self.dmcgtGiven = values[1233] != 0.0;
        self.xgwGiven = values[1234] != 0.0;
        self.xglGiven = values[1235] != 0.0;
        self.rshgGiven = values[1236] != 0.0;
        self.ngconGiven = values[1237] != 0.0;
        self.lcdscGiven = values[1238] != 0.0;
        self.lcdscbGiven = values[1239] != 0.0;
        self.lcdscdGiven = values[1240] != 0.0;
        self.lcitGiven = values[1241] != 0.0;
        self.lnfactorGiven = values[1242] != 0.0;
        self.lxjGiven = values[1243] != 0.0;
        self.lvsatGiven = values[1244] != 0.0;
        self.latGiven = values[1245] != 0.0;
        self.la0Given = values[1246] != 0.0;
        self.lagsGiven = values[1247] != 0.0;
        self.la1Given = values[1248] != 0.0;
        self.la2Given = values[1249] != 0.0;
        self.lketaGiven = values[1250] != 0.0;
        self.lketacGiven = values[1251] != 0.0;
        self.lnsubGiven = values[1252] != 0.0;
        self.lndepGiven = values[1253] != 0.0;
        self.lnsdGiven = values[1254] != 0.0;
        self.lphinGiven = values[1255] != 0.0;
        self.lngateGiven = values[1256] != 0.0;
        self.lgamma1Given = values[1257] != 0.0;
        self.lgamma2Given = values[1258] != 0.0;
        self.lvbxGiven = values[1259] != 0.0;
        self.lvbmGiven = values[1260] != 0.0;
        self.lxtGiven = values[1261] != 0.0;
        self.lk1Given = values[1262] != 0.0;
        self.lkt1Given = values[1263] != 0.0;
        self.lkt1lGiven = values[1264] != 0.0;
        self.lkt2Given = values[1265] != 0.0;
        self.lk2Given = values[1266] != 0.0;
        self.lk3Given = values[1267] != 0.0;
        self.lk3bGiven = values[1268] != 0.0;
        self.lw0Given = values[1269] != 0.0;
        self.ldvtp0Given = values[1270] != 0.0;
        self.ldvtp1Given = values[1271] != 0.0;
        self.ldvtp2Given = values[1272] != 0.0;
        self.ldvtp3Given = values[1273] != 0.0;
        self.ldvtp4Given = values[1274] != 0.0;
        self.ldvtp5Given = values[1275] != 0.0;
        self.llpe0Given = values[1276] != 0.0;
        self.llpebGiven = values[1277] != 0.0;
        self.ldvt0Given = values[1278] != 0.0;
        self.ldvt1Given = values[1279] != 0.0;
        self.ldvt2Given = values[1280] != 0.0;
        self.ldvt0wGiven = values[1281] != 0.0;
        self.ldvt1wGiven = values[1282] != 0.0;
        self.ldvt2wGiven = values[1283] != 0.0;
        self.ldroutGiven = values[1284] != 0.0;
        self.ldsubGiven = values[1285] != 0.0;
        self.lvth0Given = values[1286] != 0.0;
        self.luaGiven = values[1287] != 0.0;
        self.lua1Given = values[1288] != 0.0;
        self.lubGiven = values[1289] != 0.0;
        self.lub1Given = values[1290] != 0.0;
        self.lucGiven = values[1291] != 0.0;
        self.luc1Given = values[1292] != 0.0;
        self.ludGiven = values[1293] != 0.0;
        self.lud1Given = values[1294] != 0.0;
        self.lupGiven = values[1295] != 0.0;
        self.llpGiven = values[1296] != 0.0;
        self.lu0Given = values[1297] != 0.0;
        self.leuGiven = values[1298] != 0.0;
        self.lucsGiven = values[1299] != 0.0;
        self.luteGiven = values[1300] != 0.0;
        self.lucsteGiven = values[1301] != 0.0;
        self.lvoffGiven = values[1302] != 0.0;
        self.ltvoffGiven = values[1303] != 0.0;
        self.ltnfactorGiven = values[1304] != 0.0;
        self.lteta0Given = values[1305] != 0.0;
        self.ltvoffcvGiven = values[1306] != 0.0;
        self.lminvGiven = values[1307] != 0.0;
        self.lminvcvGiven = values[1308] != 0.0;
        self.lrdswGiven = values[1309] != 0.0;
        self.lrswGiven = values[1310] != 0.0;
        self.lrdwGiven = values[1311] != 0.0;
        self.lprwgGiven = values[1312] != 0.0;
        self.lprwbGiven = values[1313] != 0.0;
        self.lprtGiven = values[1314] != 0.0;
        self.leta0Given = values[1315] != 0.0;
        self.letabGiven = values[1316] != 0.0;
        self.lpclmGiven = values[1317] != 0.0;
        self.lpdibl1Given = values[1318] != 0.0;
        self.lpdibl2Given = values[1319] != 0.0;
        self.lpdiblbGiven = values[1320] != 0.0;
        self.lfproutGiven = values[1321] != 0.0;
        self.lpditsGiven = values[1322] != 0.0;
        self.lpditsdGiven = values[1323] != 0.0;
        self.lpscbe1Given = values[1324] != 0.0;
        self.lpscbe2Given = values[1325] != 0.0;
        self.lpvagGiven = values[1326] != 0.0;
        self.ldeltaGiven = values[1327] != 0.0;
        self.lwrGiven = values[1328] != 0.0;
        self.ldwgGiven = values[1329] != 0.0;
        self.ldwbGiven = values[1330] != 0.0;
        self.lb0Given = values[1331] != 0.0;
        self.lb1Given = values[1332] != 0.0;
        self.lalpha0Given = values[1333] != 0.0;
        self.lalpha1Given = values[1334] != 0.0;
        self.lbeta0Given = values[1335] != 0.0;
        self.lvfbGiven = values[1336] != 0.0;
        self.lagidlGiven = values[1337] != 0.0;
        self.lbgidlGiven = values[1338] != 0.0;
        self.lcgidlGiven = values[1339] != 0.0;
        self.legidlGiven = values[1340] != 0.0;
        self.lfgidlGiven = values[1341] != 0.0;
        self.lkgidlGiven = values[1342] != 0.0;
        self.lrgidlGiven = values[1343] != 0.0;
        self.lagislGiven = values[1344] != 0.0;
        self.lbgislGiven = values[1345] != 0.0;
        self.lcgislGiven = values[1346] != 0.0;
        self.legislGiven = values[1347] != 0.0;
        self.lfgislGiven = values[1348] != 0.0;
        self.lkgislGiven = values[1349] != 0.0;
        self.lrgislGiven = values[1350] != 0.0;
        self.laigcGiven = values[1351] != 0.0;
        self.lbigcGiven = values[1352] != 0.0;
        self.lcigcGiven = values[1353] != 0.0;
        self.laigsdGiven = values[1354] != 0.0;
        self.lbigsdGiven = values[1355] != 0.0;
        self.lcigsdGiven = values[1356] != 0.0;
        self.laigsGiven = values[1357] != 0.0;
        self.lbigsGiven = values[1358] != 0.0;
        self.lcigsGiven = values[1359] != 0.0;
        self.laigdGiven = values[1360] != 0.0;
        self.lbigdGiven = values[1361] != 0.0;
        self.lcigdGiven = values[1362] != 0.0;
        self.laigbaccGiven = values[1363] != 0.0;
        self.lbigbaccGiven = values[1364] != 0.0;
        self.lcigbaccGiven = values[1365] != 0.0;
        self.laigbinvGiven = values[1366] != 0.0;
        self.lbigbinvGiven = values[1367] != 0.0;
        self.lcigbinvGiven = values[1368] != 0.0;
        self.lnigcGiven = values[1369] != 0.0;
        self.lnigbinvGiven = values[1370] != 0.0;
        self.lnigbaccGiven = values[1371] != 0.0;
        self.lntoxGiven = values[1372] != 0.0;
        self.leigbinvGiven = values[1373] != 0.0;
        self.lpigcdGiven = values[1374] != 0.0;
        self.lpoxedgeGiven = values[1375] != 0.0;
        self.lxrcrg1Given = values[1376] != 0.0;
        self.lxrcrg2Given = values[1377] != 0.0;
        self.llambdaGiven = values[1378] != 0.0;
        self.lvtlGiven = values[1379] != 0.0;
        self.lxnGiven = values[1380] != 0.0;
        self.lvfbsdoffGiven = values[1381] != 0.0;
        self.ltvfbsdoffGiven = values[1382] != 0.0;
        self.lcgslGiven = values[1383] != 0.0;
        self.lcgdlGiven = values[1384] != 0.0;
        self.lckappasGiven = values[1385] != 0.0;
        self.lckappadGiven = values[1386] != 0.0;
        self.lcfGiven = values[1387] != 0.0;
        self.lclcGiven = values[1388] != 0.0;
        self.lcleGiven = values[1389] != 0.0;
        self.lvfbcvGiven = values[1390] != 0.0;
        self.lnoffGiven = values[1391] != 0.0;
        self.lvoffcvGiven = values[1392] != 0.0;
        self.lacdeGiven = values[1393] != 0.0;
        self.lmoinGiven = values[1394] != 0.0;
        self.wcdscGiven = values[1395] != 0.0;
        self.wcdscbGiven = values[1396] != 0.0;
        self.wcdscdGiven = values[1397] != 0.0;
        self.wcitGiven = values[1398] != 0.0;
        self.wnfactorGiven = values[1399] != 0.0;
        self.wxjGiven = values[1400] != 0.0;
        self.wvsatGiven = values[1401] != 0.0;
        self.watGiven = values[1402] != 0.0;
        self.wa0Given = values[1403] != 0.0;
        self.wagsGiven = values[1404] != 0.0;
        self.wa1Given = values[1405] != 0.0;
        self.wa2Given = values[1406] != 0.0;
        self.wketaGiven = values[1407] != 0.0;
        self.wketacGiven = values[1408] != 0.0;
        self.wnsubGiven = values[1409] != 0.0;
        self.wndepGiven = values[1410] != 0.0;
        self.wnsdGiven = values[1411] != 0.0;
        self.wphinGiven = values[1412] != 0.0;
        self.wngateGiven = values[1413] != 0.0;
        self.wgamma1Given = values[1414] != 0.0;
        self.wgamma2Given = values[1415] != 0.0;
        self.wvbxGiven = values[1416] != 0.0;
        self.wvbmGiven = values[1417] != 0.0;
        self.wxtGiven = values[1418] != 0.0;
        self.wk1Given = values[1419] != 0.0;
        self.wkt1Given = values[1420] != 0.0;
        self.wkt1lGiven = values[1421] != 0.0;
        self.wkt2Given = values[1422] != 0.0;
        self.wk2Given = values[1423] != 0.0;
        self.wk3Given = values[1424] != 0.0;
        self.wk3bGiven = values[1425] != 0.0;
        self.ww0Given = values[1426] != 0.0;
        self.wdvtp0Given = values[1427] != 0.0;
        self.wdvtp1Given = values[1428] != 0.0;
        self.wdvtp2Given = values[1429] != 0.0;
        self.wdvtp3Given = values[1430] != 0.0;
        self.wdvtp4Given = values[1431] != 0.0;
        self.wdvtp5Given = values[1432] != 0.0;
        self.wlpe0Given = values[1433] != 0.0;
        self.wlpebGiven = values[1434] != 0.0;
        self.wdvt0Given = values[1435] != 0.0;
        self.wdvt1Given = values[1436] != 0.0;
        self.wdvt2Given = values[1437] != 0.0;
        self.wdvt0wGiven = values[1438] != 0.0;
        self.wdvt1wGiven = values[1439] != 0.0;
        self.wdvt2wGiven = values[1440] != 0.0;
        self.wdroutGiven = values[1441] != 0.0;
        self.wdsubGiven = values[1442] != 0.0;
        self.wvth0Given = values[1443] != 0.0;
        self.wuaGiven = values[1444] != 0.0;
        self.wua1Given = values[1445] != 0.0;
        self.wubGiven = values[1446] != 0.0;
        self.wub1Given = values[1447] != 0.0;
        self.wucGiven = values[1448] != 0.0;
        self.wuc1Given = values[1449] != 0.0;
        self.wudGiven = values[1450] != 0.0;
        self.wud1Given = values[1451] != 0.0;
        self.wupGiven = values[1452] != 0.0;
        self.wlpGiven = values[1453] != 0.0;
        self.wu0Given = values[1454] != 0.0;
        self.weuGiven = values[1455] != 0.0;
        self.wucsGiven = values[1456] != 0.0;
        self.wuteGiven = values[1457] != 0.0;
        self.wucsteGiven = values[1458] != 0.0;
        self.wvoffGiven = values[1459] != 0.0;
        self.wtvoffGiven = values[1460] != 0.0;
        self.wtnfactorGiven = values[1461] != 0.0;
        self.wteta0Given = values[1462] != 0.0;
        self.wtvoffcvGiven = values[1463] != 0.0;
        self.wminvGiven = values[1464] != 0.0;
        self.wminvcvGiven = values[1465] != 0.0;
        self.wrdswGiven = values[1466] != 0.0;
        self.wrswGiven = values[1467] != 0.0;
        self.wrdwGiven = values[1468] != 0.0;
        self.wprwgGiven = values[1469] != 0.0;
        self.wprwbGiven = values[1470] != 0.0;
        self.wprtGiven = values[1471] != 0.0;
        self.weta0Given = values[1472] != 0.0;
        self.wetabGiven = values[1473] != 0.0;
        self.wpclmGiven = values[1474] != 0.0;
        self.wpdibl1Given = values[1475] != 0.0;
        self.wpdibl2Given = values[1476] != 0.0;
        self.wpdiblbGiven = values[1477] != 0.0;
        self.wfproutGiven = values[1478] != 0.0;
        self.wpditsGiven = values[1479] != 0.0;
        self.wpditsdGiven = values[1480] != 0.0;
        self.wpscbe1Given = values[1481] != 0.0;
        self.wpscbe2Given = values[1482] != 0.0;
        self.wpvagGiven = values[1483] != 0.0;
        self.wdeltaGiven = values[1484] != 0.0;
        self.wwrGiven = values[1485] != 0.0;
        self.wdwgGiven = values[1486] != 0.0;
        self.wdwbGiven = values[1487] != 0.0;
        self.wb0Given = values[1488] != 0.0;
        self.wb1Given = values[1489] != 0.0;
        self.walpha0Given = values[1490] != 0.0;
        self.walpha1Given = values[1491] != 0.0;
        self.wbeta0Given = values[1492] != 0.0;
        self.wvfbGiven = values[1493] != 0.0;
        self.wagidlGiven = values[1494] != 0.0;
        self.wbgidlGiven = values[1495] != 0.0;
        self.wcgidlGiven = values[1496] != 0.0;
        self.wegidlGiven = values[1497] != 0.0;
        self.wfgidlGiven = values[1498] != 0.0;
        self.wkgidlGiven = values[1499] != 0.0;
        self.wrgidlGiven = values[1500] != 0.0;
        self.wagislGiven = values[1501] != 0.0;
        self.wbgislGiven = values[1502] != 0.0;
        self.wcgislGiven = values[1503] != 0.0;
        self.wegislGiven = values[1504] != 0.0;
        self.wfgislGiven = values[1505] != 0.0;
        self.wkgislGiven = values[1506] != 0.0;
        self.wrgislGiven = values[1507] != 0.0;
        self.waigcGiven = values[1508] != 0.0;
        self.wbigcGiven = values[1509] != 0.0;
        self.wcigcGiven = values[1510] != 0.0;
        self.waigsdGiven = values[1511] != 0.0;
        self.wbigsdGiven = values[1512] != 0.0;
        self.wcigsdGiven = values[1513] != 0.0;
        self.waigsGiven = values[1514] != 0.0;
        self.wbigsGiven = values[1515] != 0.0;
        self.wcigsGiven = values[1516] != 0.0;
        self.waigdGiven = values[1517] != 0.0;
        self.wbigdGiven = values[1518] != 0.0;
        self.wcigdGiven = values[1519] != 0.0;
        self.waigbaccGiven = values[1520] != 0.0;
        self.wbigbaccGiven = values[1521] != 0.0;
        self.wcigbaccGiven = values[1522] != 0.0;
        self.waigbinvGiven = values[1523] != 0.0;
        self.wbigbinvGiven = values[1524] != 0.0;
        self.wcigbinvGiven = values[1525] != 0.0;
        self.wnigcGiven = values[1526] != 0.0;
        self.wnigbinvGiven = values[1527] != 0.0;
        self.wnigbaccGiven = values[1528] != 0.0;
        self.wntoxGiven = values[1529] != 0.0;
        self.weigbinvGiven = values[1530] != 0.0;
        self.wpigcdGiven = values[1531] != 0.0;
        self.wpoxedgeGiven = values[1532] != 0.0;
        self.wxrcrg1Given = values[1533] != 0.0;
        self.wxrcrg2Given = values[1534] != 0.0;
        self.wlambdaGiven = values[1535] != 0.0;
        self.wvtlGiven = values[1536] != 0.0;
        self.wxnGiven = values[1537] != 0.0;
        self.wvfbsdoffGiven = values[1538] != 0.0;
        self.wtvfbsdoffGiven = values[1539] != 0.0;
        self.wcgslGiven = values[1540] != 0.0;
        self.wcgdlGiven = values[1541] != 0.0;
        self.wckappasGiven = values[1542] != 0.0;
        self.wckappadGiven = values[1543] != 0.0;
        self.wcfGiven = values[1544] != 0.0;
        self.wclcGiven = values[1545] != 0.0;
        self.wcleGiven = values[1546] != 0.0;
        self.wvfbcvGiven = values[1547] != 0.0;
        self.wnoffGiven = values[1548] != 0.0;
        self.wvoffcvGiven = values[1549] != 0.0;
        self.wacdeGiven = values[1550] != 0.0;
        self.wmoinGiven = values[1551] != 0.0;
        self.pcdscGiven = values[1552] != 0.0;
        self.pcdscbGiven = values[1553] != 0.0;
        self.pcdscdGiven = values[1554] != 0.0;
        self.pcitGiven = values[1555] != 0.0;
        self.pnfactorGiven = values[1556] != 0.0;
        self.pxjGiven = values[1557] != 0.0;
        self.pvsatGiven = values[1558] != 0.0;
        self.patGiven = values[1559] != 0.0;
        self.pa0Given = values[1560] != 0.0;
        self.pagsGiven = values[1561] != 0.0;
        self.pa1Given = values[1562] != 0.0;
        self.pa2Given = values[1563] != 0.0;
        self.pketaGiven = values[1564] != 0.0;
        self.pketacGiven = values[1565] != 0.0;
        self.pnsubGiven = values[1566] != 0.0;
        self.pndepGiven = values[1567] != 0.0;
        self.pnsdGiven = values[1568] != 0.0;
        self.pphinGiven = values[1569] != 0.0;
        self.pngateGiven = values[1570] != 0.0;
        self.pgamma1Given = values[1571] != 0.0;
        self.pgamma2Given = values[1572] != 0.0;
        self.pvbxGiven = values[1573] != 0.0;
        self.pvbmGiven = values[1574] != 0.0;
        self.pxtGiven = values[1575] != 0.0;
        self.pk1Given = values[1576] != 0.0;
        self.pkt1Given = values[1577] != 0.0;
        self.pkt1lGiven = values[1578] != 0.0;
        self.pkt2Given = values[1579] != 0.0;
        self.pk2Given = values[1580] != 0.0;
        self.pk3Given = values[1581] != 0.0;
        self.pk3bGiven = values[1582] != 0.0;
        self.pw0Given = values[1583] != 0.0;
        self.pdvtp0Given = values[1584] != 0.0;
        self.pdvtp1Given = values[1585] != 0.0;
        self.pdvtp2Given = values[1586] != 0.0;
        self.pdvtp3Given = values[1587] != 0.0;
        self.pdvtp4Given = values[1588] != 0.0;
        self.pdvtp5Given = values[1589] != 0.0;
        self.plpe0Given = values[1590] != 0.0;
        self.plpebGiven = values[1591] != 0.0;
        self.pdvt0Given = values[1592] != 0.0;
        self.pdvt1Given = values[1593] != 0.0;
        self.pdvt2Given = values[1594] != 0.0;
        self.pdvt0wGiven = values[1595] != 0.0;
        self.pdvt1wGiven = values[1596] != 0.0;
        self.pdvt2wGiven = values[1597] != 0.0;
        self.pdroutGiven = values[1598] != 0.0;
        self.pdsubGiven = values[1599] != 0.0;
        self.pvth0Given = values[1600] != 0.0;
        self.puaGiven = values[1601] != 0.0;
        self.pua1Given = values[1602] != 0.0;
        self.pubGiven = values[1603] != 0.0;
        self.pub1Given = values[1604] != 0.0;
        self.pucGiven = values[1605] != 0.0;
        self.puc1Given = values[1606] != 0.0;
        self.pudGiven = values[1607] != 0.0;
        self.pud1Given = values[1608] != 0.0;
        self.pupGiven = values[1609] != 0.0;
        self.plpGiven = values[1610] != 0.0;
        self.pu0Given = values[1611] != 0.0;
        self.peuGiven = values[1612] != 0.0;
        self.pucsGiven = values[1613] != 0.0;
        self.puteGiven = values[1614] != 0.0;
        self.pucsteGiven = values[1615] != 0.0;
        self.pvoffGiven = values[1616] != 0.0;
        self.ptvoffGiven = values[1617] != 0.0;
        self.ptnfactorGiven = values[1618] != 0.0;
        self.pteta0Given = values[1619] != 0.0;
        self.ptvoffcvGiven = values[1620] != 0.0;
        self.pminvGiven = values[1621] != 0.0;
        self.pminvcvGiven = values[1622] != 0.0;
        self.prdswGiven = values[1623] != 0.0;
        self.prswGiven = values[1624] != 0.0;
        self.prdwGiven = values[1625] != 0.0;
        self.pprwgGiven = values[1626] != 0.0;
        self.pprwbGiven = values[1627] != 0.0;
        self.pprtGiven = values[1628] != 0.0;
        self.peta0Given = values[1629] != 0.0;
        self.petabGiven = values[1630] != 0.0;
        self.ppclmGiven = values[1631] != 0.0;
        self.ppdibl1Given = values[1632] != 0.0;
        self.ppdibl2Given = values[1633] != 0.0;
        self.ppdiblbGiven = values[1634] != 0.0;
        self.pfproutGiven = values[1635] != 0.0;
        self.ppditsGiven = values[1636] != 0.0;
        self.ppditsdGiven = values[1637] != 0.0;
        self.ppscbe1Given = values[1638] != 0.0;
        self.ppscbe2Given = values[1639] != 0.0;
        self.ppvagGiven = values[1640] != 0.0;
        self.pdeltaGiven = values[1641] != 0.0;
        self.pwrGiven = values[1642] != 0.0;
        self.pdwgGiven = values[1643] != 0.0;
        self.pdwbGiven = values[1644] != 0.0;
        self.pb0Given = values[1645] != 0.0;
        self.pb1Given = values[1646] != 0.0;
        self.palpha0Given = values[1647] != 0.0;
        self.palpha1Given = values[1648] != 0.0;
        self.pbeta0Given = values[1649] != 0.0;
        self.pvfbGiven = values[1650] != 0.0;
        self.pagidlGiven = values[1651] != 0.0;
        self.pbgidlGiven = values[1652] != 0.0;
        self.pcgidlGiven = values[1653] != 0.0;
        self.pegidlGiven = values[1654] != 0.0;
        self.pfgidlGiven = values[1655] != 0.0;
        self.pkgidlGiven = values[1656] != 0.0;
        self.prgidlGiven = values[1657] != 0.0;
        self.pagislGiven = values[1658] != 0.0;
        self.pbgislGiven = values[1659] != 0.0;
        self.pcgislGiven = values[1660] != 0.0;
        self.pegislGiven = values[1661] != 0.0;
        self.pfgislGiven = values[1662] != 0.0;
        self.pkgislGiven = values[1663] != 0.0;
        self.prgislGiven = values[1664] != 0.0;
        self.paigcGiven = values[1665] != 0.0;
        self.pbigcGiven = values[1666] != 0.0;
        self.pcigcGiven = values[1667] != 0.0;
        self.paigsdGiven = values[1668] != 0.0;
        self.pbigsdGiven = values[1669] != 0.0;
        self.pcigsdGiven = values[1670] != 0.0;
        self.paigsGiven = values[1671] != 0.0;
        self.pbigsGiven = values[1672] != 0.0;
        self.pcigsGiven = values[1673] != 0.0;
        self.paigdGiven = values[1674] != 0.0;
        self.pbigdGiven = values[1675] != 0.0;
        self.pcigdGiven = values[1676] != 0.0;
        self.paigbaccGiven = values[1677] != 0.0;
        self.pbigbaccGiven = values[1678] != 0.0;
        self.pcigbaccGiven = values[1679] != 0.0;
        self.paigbinvGiven = values[1680] != 0.0;
        self.pbigbinvGiven = values[1681] != 0.0;
        self.pcigbinvGiven = values[1682] != 0.0;
        self.pnigcGiven = values[1683] != 0.0;
        self.pnigbinvGiven = values[1684] != 0.0;
        self.pnigbaccGiven = values[1685] != 0.0;
        self.pntoxGiven = values[1686] != 0.0;
        self.peigbinvGiven = values[1687] != 0.0;
        self.ppigcdGiven = values[1688] != 0.0;
        self.ppoxedgeGiven = values[1689] != 0.0;
        self.pxrcrg1Given = values[1690] != 0.0;
        self.pxrcrg2Given = values[1691] != 0.0;
        self.plambdaGiven = values[1692] != 0.0;
        self.pvtlGiven = values[1693] != 0.0;
        self.pxnGiven = values[1694] != 0.0;
        self.pvfbsdoffGiven = values[1695] != 0.0;
        self.ptvfbsdoffGiven = values[1696] != 0.0;
        self.pcgslGiven = values[1697] != 0.0;
        self.pcgdlGiven = values[1698] != 0.0;
        self.pckappasGiven = values[1699] != 0.0;
        self.pckappadGiven = values[1700] != 0.0;
        self.pcfGiven = values[1701] != 0.0;
        self.pclcGiven = values[1702] != 0.0;
        self.pcleGiven = values[1703] != 0.0;
        self.pvfbcvGiven = values[1704] != 0.0;
        self.pnoffGiven = values[1705] != 0.0;
        self.pvoffcvGiven = values[1706] != 0.0;
        self.pacdeGiven = values[1707] != 0.0;
        self.pmoinGiven = values[1708] != 0.0;
        self.useFringeGiven = values[1709] != 0.0;
        self.tnomGiven = values[1710] != 0.0;
        self.cgsoGiven = values[1711] != 0.0;
        self.cgdoGiven = values[1712] != 0.0;
        self.cgboGiven = values[1713] != 0.0;
        self.xpartGiven = values[1714] != 0.0;
        self.sheetResistanceGiven = values[1715] != 0.0;
        self.SjctSatCurDensityGiven = values[1716] != 0.0;
        self.SjctSidewallSatCurDensityGiven = values[1717] != 0.0;
        self.SjctGateSidewallSatCurDensityGiven = values[1718] != 0.0;
        self.SbulkJctPotentialGiven = values[1719] != 0.0;
        self.SbulkJctBotGradingCoeffGiven = values[1720] != 0.0;
        self.SsidewallJctPotentialGiven = values[1721] != 0.0;
        self.SGatesidewallJctPotentialGiven = values[1722] != 0.0;
        self.SbulkJctSideGradingCoeffGiven = values[1723] != 0.0;
        self.SunitAreaJctCapGiven = values[1724] != 0.0;
        self.SunitLengthSidewallJctCapGiven = values[1725] != 0.0;
        self.SbulkJctGateSideGradingCoeffGiven = values[1726] != 0.0;
        self.SunitLengthGateSidewallJctCapGiven = values[1727] != 0.0;
        self.SjctEmissionCoeffGiven = values[1728] != 0.0;
        self.SjctTempExponentGiven = values[1729] != 0.0;
        self.DjctSatCurDensityGiven = values[1730] != 0.0;
        self.DjctSidewallSatCurDensityGiven = values[1731] != 0.0;
        self.DjctGateSidewallSatCurDensityGiven = values[1732] != 0.0;
        self.DbulkJctPotentialGiven = values[1733] != 0.0;
        self.DbulkJctBotGradingCoeffGiven = values[1734] != 0.0;
        self.DsidewallJctPotentialGiven = values[1735] != 0.0;
        self.DGatesidewallJctPotentialGiven = values[1736] != 0.0;
        self.DbulkJctSideGradingCoeffGiven = values[1737] != 0.0;
        self.DunitAreaJctCapGiven = values[1738] != 0.0;
        self.DunitLengthSidewallJctCapGiven = values[1739] != 0.0;
        self.DbulkJctGateSideGradingCoeffGiven = values[1740] != 0.0;
        self.DunitLengthGateSidewallJctCapGiven = values[1741] != 0.0;
        self.DjctEmissionCoeffGiven = values[1742] != 0.0;
        self.DjctTempExponentGiven = values[1743] != 0.0;
        self.oxideTrapDensityAGiven = values[1744] != 0.0;
        self.oxideTrapDensityBGiven = values[1745] != 0.0;
        self.oxideTrapDensityCGiven = values[1746] != 0.0;
        self.emGiven = values[1747] != 0.0;
        self.efGiven = values[1748] != 0.0;
        self.afGiven = values[1749] != 0.0;
        self.kfGiven = values[1750] != 0.0;
        self.vgsMaxGiven = values[1751] != 0.0;
        self.vgdMaxGiven = values[1752] != 0.0;
        self.vgbMaxGiven = values[1753] != 0.0;
        self.vdsMaxGiven = values[1754] != 0.0;
        self.vbsMaxGiven = values[1755] != 0.0;
        self.vbdMaxGiven = values[1756] != 0.0;
        self.vgsrMaxGiven = values[1757] != 0.0;
        self.vgdrMaxGiven = values[1758] != 0.0;
        self.vgbrMaxGiven = values[1759] != 0.0;
        self.vbsrMaxGiven = values[1760] != 0.0;
        self.vbdrMaxGiven = values[1761] != 0.0;
        self.LintGiven = values[1762] != 0.0;
        self.LlGiven = values[1763] != 0.0;
        self.LlcGiven = values[1764] != 0.0;
        self.LlnGiven = values[1765] != 0.0;
        self.LwGiven = values[1766] != 0.0;
        self.LwcGiven = values[1767] != 0.0;
        self.LwnGiven = values[1768] != 0.0;
        self.LwlGiven = values[1769] != 0.0;
        self.LwlcGiven = values[1770] != 0.0;
        self.LminGiven = values[1771] != 0.0;
        self.LmaxGiven = values[1772] != 0.0;
        self.WintGiven = values[1773] != 0.0;
        self.WlGiven = values[1774] != 0.0;
        self.WlcGiven = values[1775] != 0.0;
        self.WlnGiven = values[1776] != 0.0;
        self.WwGiven = values[1777] != 0.0;
        self.WwcGiven = values[1778] != 0.0;
        self.WwnGiven = values[1779] != 0.0;
        self.WwlGiven = values[1780] != 0.0;
        self.WwlcGiven = values[1781] != 0.0;
        self.WminGiven = values[1782] != 0.0;
        self.WmaxGiven = values[1783] != 0.0;
        self.sarefGiven = values[1784] != 0.0;
        self.sbrefGiven = values[1785] != 0.0;
        self.wlodGiven = values[1786] != 0.0;
        self.ku0Given = values[1787] != 0.0;
        self.kvsatGiven = values[1788] != 0.0;
        self.kvth0Given = values[1789] != 0.0;
        self.tku0Given = values[1790] != 0.0;
        self.llodku0Given = values[1791] != 0.0;
        self.wlodku0Given = values[1792] != 0.0;
        self.llodvthGiven = values[1793] != 0.0;
        self.wlodvthGiven = values[1794] != 0.0;
        self.lku0Given = values[1795] != 0.0;
        self.wku0Given = values[1796] != 0.0;
        self.pku0Given = values[1797] != 0.0;
        self.lkvth0Given = values[1798] != 0.0;
        self.wkvth0Given = values[1799] != 0.0;
        self.pkvth0Given = values[1800] != 0.0;
        self.stk2Given = values[1801] != 0.0;
        self.lodk2Given = values[1802] != 0.0;
        self.steta0Given = values[1803] != 0.0;
        self.lodeta0Given = values[1804] != 0.0;
        self.webGiven = values[1805] != 0.0;
        self.wecGiven = values[1806] != 0.0;
        self.kvth0weGiven = values[1807] != 0.0;
        self.k2weGiven = values[1808] != 0.0;
        self.ku0weGiven = values[1809] != 0.0;
        self.screfGiven = values[1810] != 0.0;
        self.wpemodGiven = values[1811] != 0.0;
        self.lkvth0weGiven = values[1812] != 0.0;
        self.lk2weGiven = values[1813] != 0.0;
        self.lku0weGiven = values[1814] != 0.0;
        self.wkvth0weGiven = values[1815] != 0.0;
        self.wk2weGiven = values[1816] != 0.0;
        self.wku0weGiven = values[1817] != 0.0;
        self.pkvth0weGiven = values[1818] != 0.0;
        self.pk2weGiven = values[1819] != 0.0;
        self.pku0weGiven = values[1820] != 0.0;
        self.gidlclampGiven = values[1821] != 0.0;
        self.idovvdscGiven = values[1822] != 0.0;
    }
}

/// `struct bsim4SizeDependParam`'s scalar members (224 of them), in declaration order.
#[derive(Clone, Default)]
pub(crate) struct SizeParam {
    pub(crate) Width: f64,
    pub(crate) Length: f64,
    pub(crate) NFinger: f64,
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
    pub(crate) ketac: f64,
    pub(crate) nsub: f64,
    pub(crate) ndep: f64,
    pub(crate) nsd: f64,
    pub(crate) phin: f64,
    pub(crate) ngate: f64,
    pub(crate) gamma1: f64,
    pub(crate) gamma2: f64,
    pub(crate) vbx: f64,
    pub(crate) vbi: f64,
    pub(crate) vbm: f64,
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
    pub(crate) dvtp0: f64,
    pub(crate) dvtp1: f64,
    pub(crate) dvtp2: f64,
    pub(crate) dvtp3: f64,
    pub(crate) dvtp4: f64,
    pub(crate) dvtp5: f64,
    pub(crate) lpe0: f64,
    pub(crate) lpeb: f64,
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
    pub(crate) ud: f64,
    pub(crate) ud1: f64,
    pub(crate) up: f64,
    pub(crate) lp: f64,
    pub(crate) u0: f64,
    pub(crate) eu: f64,
    pub(crate) ucs: f64,
    pub(crate) ute: f64,
    pub(crate) ucste: f64,
    pub(crate) voff: f64,
    pub(crate) tvoff: f64,
    pub(crate) tnfactor: f64,
    pub(crate) teta0: f64,
    pub(crate) tvoffcv: f64,
    pub(crate) minv: f64,
    pub(crate) minvcv: f64,
    pub(crate) vfb: f64,
    pub(crate) delta: f64,
    pub(crate) rdsw: f64,
    pub(crate) rds0: f64,
    pub(crate) rs0: f64,
    pub(crate) rd0: f64,
    pub(crate) rsw: f64,
    pub(crate) rdw: f64,
    pub(crate) prwg: f64,
    pub(crate) prwb: f64,
    pub(crate) prt: f64,
    pub(crate) eta0: f64,
    pub(crate) etab: f64,
    pub(crate) pclm: f64,
    pub(crate) pdibl1: f64,
    pub(crate) pdibl2: f64,
    pub(crate) pdiblb: f64,
    pub(crate) fprout: f64,
    pub(crate) pdits: f64,
    pub(crate) pditsd: f64,
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
    pub(crate) agidl: f64,
    pub(crate) bgidl: f64,
    pub(crate) cgidl: f64,
    pub(crate) egidl: f64,
    pub(crate) fgidl: f64,
    pub(crate) kgidl: f64,
    pub(crate) rgidl: f64,
    pub(crate) agisl: f64,
    pub(crate) bgisl: f64,
    pub(crate) cgisl: f64,
    pub(crate) egisl: f64,
    pub(crate) fgisl: f64,
    pub(crate) kgisl: f64,
    pub(crate) rgisl: f64,
    pub(crate) aigc: f64,
    pub(crate) bigc: f64,
    pub(crate) cigc: f64,
    pub(crate) aigsd: f64,
    pub(crate) bigsd: f64,
    pub(crate) cigsd: f64,
    pub(crate) aigs: f64,
    pub(crate) bigs: f64,
    pub(crate) cigs: f64,
    pub(crate) aigd: f64,
    pub(crate) bigd: f64,
    pub(crate) cigd: f64,
    pub(crate) aigbacc: f64,
    pub(crate) bigbacc: f64,
    pub(crate) cigbacc: f64,
    pub(crate) aigbinv: f64,
    pub(crate) bigbinv: f64,
    pub(crate) cigbinv: f64,
    pub(crate) nigc: f64,
    pub(crate) nigbacc: f64,
    pub(crate) nigbinv: f64,
    pub(crate) ntox: f64,
    pub(crate) eigbinv: f64,
    pub(crate) pigcd: f64,
    pub(crate) poxedge: f64,
    pub(crate) xrcrg1: f64,
    pub(crate) xrcrg2: f64,
    pub(crate) lambda: f64,
    pub(crate) vtl: f64,
    pub(crate) xn: f64,
    pub(crate) lc: f64,
    pub(crate) tfactor: f64,
    pub(crate) vfbsdoff: f64,
    pub(crate) tvfbsdoff: f64,
    pub(crate) ku0: f64,
    pub(crate) kvth0: f64,
    pub(crate) ku0temp: f64,
    pub(crate) rho_ref: f64,
    pub(crate) inv_od_ref: f64,
    pub(crate) kvth0we: f64,
    pub(crate) k2we: f64,
    pub(crate) ku0we: f64,
    pub(crate) cgsl: f64,
    pub(crate) cgdl: f64,
    pub(crate) ckappas: f64,
    pub(crate) ckappad: f64,
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
    pub(crate) dwj: f64,
    pub(crate) leffCV: f64,
    pub(crate) weffCV: f64,
    pub(crate) weffCJ: f64,
    pub(crate) abulkCVfactor: f64,
    pub(crate) cgso: f64,
    pub(crate) cgdo: f64,
    pub(crate) cgbo: f64,
    pub(crate) u0temp: f64,
    pub(crate) vsattemp: f64,
    pub(crate) sqrtPhi: f64,
    pub(crate) phis3: f64,
    pub(crate) Xdep0: f64,
    pub(crate) sqrtXdep0: f64,
    pub(crate) theta0vb0: f64,
    pub(crate) thetaRout: f64,
    pub(crate) mstar: f64,
    pub(crate) VgsteffVth: f64,
    pub(crate) mstarcv: f64,
    pub(crate) voffcbn: f64,
    pub(crate) voffcbncv: f64,
    pub(crate) rdswmin: f64,
    pub(crate) rdwmin: f64,
    pub(crate) rswmin: f64,
    pub(crate) vfbsd: f64,
    pub(crate) cof1: f64,
    pub(crate) cof2: f64,
    pub(crate) cof3: f64,
    pub(crate) cof4: f64,
    pub(crate) cdep0: f64,
    pub(crate) ToxRatio: f64,
    pub(crate) Aechvb: f64,
    pub(crate) Bechvb: f64,
    pub(crate) ToxRatioEdge: f64,
    pub(crate) AechvbEdgeS: f64,
    pub(crate) AechvbEdgeD: f64,
    pub(crate) BechvbEdge: f64,
    pub(crate) ldeb: f64,
    pub(crate) k1ox: f64,
    pub(crate) k2ox: f64,
    pub(crate) vfbzbfactor: f64,
    pub(crate) dvtp2factor: f64,
}

#[rustfmt::skip]
impl SizeParam {
    /// The `f64` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn real_mut(&mut self, c_name: &str) -> Option<&mut f64> {
        Some(match c_name {
            "Width" => &mut self.Width,
            "Length" => &mut self.Length,
            "NFinger" => &mut self.NFinger,
            "BSIM4cdsc" => &mut self.cdsc,
            "BSIM4cdscb" => &mut self.cdscb,
            "BSIM4cdscd" => &mut self.cdscd,
            "BSIM4cit" => &mut self.cit,
            "BSIM4nfactor" => &mut self.nfactor,
            "BSIM4xj" => &mut self.xj,
            "BSIM4vsat" => &mut self.vsat,
            "BSIM4at" => &mut self.at,
            "BSIM4a0" => &mut self.a0,
            "BSIM4ags" => &mut self.ags,
            "BSIM4a1" => &mut self.a1,
            "BSIM4a2" => &mut self.a2,
            "BSIM4keta" => &mut self.keta,
            "BSIM4ketac" => &mut self.ketac,
            "BSIM4nsub" => &mut self.nsub,
            "BSIM4ndep" => &mut self.ndep,
            "BSIM4nsd" => &mut self.nsd,
            "BSIM4phin" => &mut self.phin,
            "BSIM4ngate" => &mut self.ngate,
            "BSIM4gamma1" => &mut self.gamma1,
            "BSIM4gamma2" => &mut self.gamma2,
            "BSIM4vbx" => &mut self.vbx,
            "BSIM4vbi" => &mut self.vbi,
            "BSIM4vbm" => &mut self.vbm,
            "BSIM4xt" => &mut self.xt,
            "BSIM4phi" => &mut self.phi,
            "BSIM4litl" => &mut self.litl,
            "BSIM4k1" => &mut self.k1,
            "BSIM4kt1" => &mut self.kt1,
            "BSIM4kt1l" => &mut self.kt1l,
            "BSIM4kt2" => &mut self.kt2,
            "BSIM4k2" => &mut self.k2,
            "BSIM4k3" => &mut self.k3,
            "BSIM4k3b" => &mut self.k3b,
            "BSIM4w0" => &mut self.w0,
            "BSIM4dvtp0" => &mut self.dvtp0,
            "BSIM4dvtp1" => &mut self.dvtp1,
            "BSIM4dvtp2" => &mut self.dvtp2,
            "BSIM4dvtp3" => &mut self.dvtp3,
            "BSIM4dvtp4" => &mut self.dvtp4,
            "BSIM4dvtp5" => &mut self.dvtp5,
            "BSIM4lpe0" => &mut self.lpe0,
            "BSIM4lpeb" => &mut self.lpeb,
            "BSIM4dvt0" => &mut self.dvt0,
            "BSIM4dvt1" => &mut self.dvt1,
            "BSIM4dvt2" => &mut self.dvt2,
            "BSIM4dvt0w" => &mut self.dvt0w,
            "BSIM4dvt1w" => &mut self.dvt1w,
            "BSIM4dvt2w" => &mut self.dvt2w,
            "BSIM4drout" => &mut self.drout,
            "BSIM4dsub" => &mut self.dsub,
            "BSIM4vth0" => &mut self.vth0,
            "BSIM4ua" => &mut self.ua,
            "BSIM4ua1" => &mut self.ua1,
            "BSIM4ub" => &mut self.ub,
            "BSIM4ub1" => &mut self.ub1,
            "BSIM4uc" => &mut self.uc,
            "BSIM4uc1" => &mut self.uc1,
            "BSIM4ud" => &mut self.ud,
            "BSIM4ud1" => &mut self.ud1,
            "BSIM4up" => &mut self.up,
            "BSIM4lp" => &mut self.lp,
            "BSIM4u0" => &mut self.u0,
            "BSIM4eu" => &mut self.eu,
            "BSIM4ucs" => &mut self.ucs,
            "BSIM4ute" => &mut self.ute,
            "BSIM4ucste" => &mut self.ucste,
            "BSIM4voff" => &mut self.voff,
            "BSIM4tvoff" => &mut self.tvoff,
            "BSIM4tnfactor" => &mut self.tnfactor,
            "BSIM4teta0" => &mut self.teta0,
            "BSIM4tvoffcv" => &mut self.tvoffcv,
            "BSIM4minv" => &mut self.minv,
            "BSIM4minvcv" => &mut self.minvcv,
            "BSIM4vfb" => &mut self.vfb,
            "BSIM4delta" => &mut self.delta,
            "BSIM4rdsw" => &mut self.rdsw,
            "BSIM4rds0" => &mut self.rds0,
            "BSIM4rs0" => &mut self.rs0,
            "BSIM4rd0" => &mut self.rd0,
            "BSIM4rsw" => &mut self.rsw,
            "BSIM4rdw" => &mut self.rdw,
            "BSIM4prwg" => &mut self.prwg,
            "BSIM4prwb" => &mut self.prwb,
            "BSIM4prt" => &mut self.prt,
            "BSIM4eta0" => &mut self.eta0,
            "BSIM4etab" => &mut self.etab,
            "BSIM4pclm" => &mut self.pclm,
            "BSIM4pdibl1" => &mut self.pdibl1,
            "BSIM4pdibl2" => &mut self.pdibl2,
            "BSIM4pdiblb" => &mut self.pdiblb,
            "BSIM4fprout" => &mut self.fprout,
            "BSIM4pdits" => &mut self.pdits,
            "BSIM4pditsd" => &mut self.pditsd,
            "BSIM4pscbe1" => &mut self.pscbe1,
            "BSIM4pscbe2" => &mut self.pscbe2,
            "BSIM4pvag" => &mut self.pvag,
            "BSIM4wr" => &mut self.wr,
            "BSIM4dwg" => &mut self.dwg,
            "BSIM4dwb" => &mut self.dwb,
            "BSIM4b0" => &mut self.b0,
            "BSIM4b1" => &mut self.b1,
            "BSIM4alpha0" => &mut self.alpha0,
            "BSIM4alpha1" => &mut self.alpha1,
            "BSIM4beta0" => &mut self.beta0,
            "BSIM4agidl" => &mut self.agidl,
            "BSIM4bgidl" => &mut self.bgidl,
            "BSIM4cgidl" => &mut self.cgidl,
            "BSIM4egidl" => &mut self.egidl,
            "BSIM4fgidl" => &mut self.fgidl,
            "BSIM4kgidl" => &mut self.kgidl,
            "BSIM4rgidl" => &mut self.rgidl,
            "BSIM4agisl" => &mut self.agisl,
            "BSIM4bgisl" => &mut self.bgisl,
            "BSIM4cgisl" => &mut self.cgisl,
            "BSIM4egisl" => &mut self.egisl,
            "BSIM4fgisl" => &mut self.fgisl,
            "BSIM4kgisl" => &mut self.kgisl,
            "BSIM4rgisl" => &mut self.rgisl,
            "BSIM4aigc" => &mut self.aigc,
            "BSIM4bigc" => &mut self.bigc,
            "BSIM4cigc" => &mut self.cigc,
            "BSIM4aigsd" => &mut self.aigsd,
            "BSIM4bigsd" => &mut self.bigsd,
            "BSIM4cigsd" => &mut self.cigsd,
            "BSIM4aigs" => &mut self.aigs,
            "BSIM4bigs" => &mut self.bigs,
            "BSIM4cigs" => &mut self.cigs,
            "BSIM4aigd" => &mut self.aigd,
            "BSIM4bigd" => &mut self.bigd,
            "BSIM4cigd" => &mut self.cigd,
            "BSIM4aigbacc" => &mut self.aigbacc,
            "BSIM4bigbacc" => &mut self.bigbacc,
            "BSIM4cigbacc" => &mut self.cigbacc,
            "BSIM4aigbinv" => &mut self.aigbinv,
            "BSIM4bigbinv" => &mut self.bigbinv,
            "BSIM4cigbinv" => &mut self.cigbinv,
            "BSIM4nigc" => &mut self.nigc,
            "BSIM4nigbacc" => &mut self.nigbacc,
            "BSIM4nigbinv" => &mut self.nigbinv,
            "BSIM4ntox" => &mut self.ntox,
            "BSIM4eigbinv" => &mut self.eigbinv,
            "BSIM4pigcd" => &mut self.pigcd,
            "BSIM4poxedge" => &mut self.poxedge,
            "BSIM4xrcrg1" => &mut self.xrcrg1,
            "BSIM4xrcrg2" => &mut self.xrcrg2,
            "BSIM4lambda" => &mut self.lambda,
            "BSIM4vtl" => &mut self.vtl,
            "BSIM4xn" => &mut self.xn,
            "BSIM4lc" => &mut self.lc,
            "BSIM4tfactor" => &mut self.tfactor,
            "BSIM4vfbsdoff" => &mut self.vfbsdoff,
            "BSIM4tvfbsdoff" => &mut self.tvfbsdoff,
            "BSIM4ku0" => &mut self.ku0,
            "BSIM4kvth0" => &mut self.kvth0,
            "BSIM4ku0temp" => &mut self.ku0temp,
            "BSIM4rho_ref" => &mut self.rho_ref,
            "BSIM4inv_od_ref" => &mut self.inv_od_ref,
            "BSIM4kvth0we" => &mut self.kvth0we,
            "BSIM4k2we" => &mut self.k2we,
            "BSIM4ku0we" => &mut self.ku0we,
            "BSIM4cgsl" => &mut self.cgsl,
            "BSIM4cgdl" => &mut self.cgdl,
            "BSIM4ckappas" => &mut self.ckappas,
            "BSIM4ckappad" => &mut self.ckappad,
            "BSIM4cf" => &mut self.cf,
            "BSIM4clc" => &mut self.clc,
            "BSIM4cle" => &mut self.cle,
            "BSIM4vfbcv" => &mut self.vfbcv,
            "BSIM4noff" => &mut self.noff,
            "BSIM4voffcv" => &mut self.voffcv,
            "BSIM4acde" => &mut self.acde,
            "BSIM4moin" => &mut self.moin,
            "BSIM4dw" => &mut self.dw,
            "BSIM4dl" => &mut self.dl,
            "BSIM4leff" => &mut self.leff,
            "BSIM4weff" => &mut self.weff,
            "BSIM4dwc" => &mut self.dwc,
            "BSIM4dlc" => &mut self.dlc,
            "BSIM4dwj" => &mut self.dwj,
            "BSIM4leffCV" => &mut self.leffCV,
            "BSIM4weffCV" => &mut self.weffCV,
            "BSIM4weffCJ" => &mut self.weffCJ,
            "BSIM4abulkCVfactor" => &mut self.abulkCVfactor,
            "BSIM4cgso" => &mut self.cgso,
            "BSIM4cgdo" => &mut self.cgdo,
            "BSIM4cgbo" => &mut self.cgbo,
            "BSIM4u0temp" => &mut self.u0temp,
            "BSIM4vsattemp" => &mut self.vsattemp,
            "BSIM4sqrtPhi" => &mut self.sqrtPhi,
            "BSIM4phis3" => &mut self.phis3,
            "BSIM4Xdep0" => &mut self.Xdep0,
            "BSIM4sqrtXdep0" => &mut self.sqrtXdep0,
            "BSIM4theta0vb0" => &mut self.theta0vb0,
            "BSIM4thetaRout" => &mut self.thetaRout,
            "BSIM4mstar" => &mut self.mstar,
            "BSIM4VgsteffVth" => &mut self.VgsteffVth,
            "BSIM4mstarcv" => &mut self.mstarcv,
            "BSIM4voffcbn" => &mut self.voffcbn,
            "BSIM4voffcbncv" => &mut self.voffcbncv,
            "BSIM4rdswmin" => &mut self.rdswmin,
            "BSIM4rdwmin" => &mut self.rdwmin,
            "BSIM4rswmin" => &mut self.rswmin,
            "BSIM4vfbsd" => &mut self.vfbsd,
            "BSIM4cof1" => &mut self.cof1,
            "BSIM4cof2" => &mut self.cof2,
            "BSIM4cof3" => &mut self.cof3,
            "BSIM4cof4" => &mut self.cof4,
            "BSIM4cdep0" => &mut self.cdep0,
            "BSIM4ToxRatio" => &mut self.ToxRatio,
            "BSIM4Aechvb" => &mut self.Aechvb,
            "BSIM4Bechvb" => &mut self.Bechvb,
            "BSIM4ToxRatioEdge" => &mut self.ToxRatioEdge,
            "BSIM4AechvbEdgeS" => &mut self.AechvbEdgeS,
            "BSIM4AechvbEdgeD" => &mut self.AechvbEdgeD,
            "BSIM4BechvbEdge" => &mut self.BechvbEdge,
            "BSIM4ldeb" => &mut self.ldeb,
            "BSIM4k1ox" => &mut self.k1ox,
            "BSIM4k2ox" => &mut self.k2ox,
            "BSIM4vfbzbfactor" => &mut self.vfbzbfactor,
            "BSIM4dvtp2factor" => &mut self.dvtp2factor,
            _ => return None,
        })
    }
    /// C member names of the scalar members, in declaration order.
    #[allow(dead_code)]
    pub(crate) const MEMBERS: &[&str] = &[
        "Width",
        "Length",
        "NFinger",
        "BSIM4cdsc",
        "BSIM4cdscb",
        "BSIM4cdscd",
        "BSIM4cit",
        "BSIM4nfactor",
        "BSIM4xj",
        "BSIM4vsat",
        "BSIM4at",
        "BSIM4a0",
        "BSIM4ags",
        "BSIM4a1",
        "BSIM4a2",
        "BSIM4keta",
        "BSIM4ketac",
        "BSIM4nsub",
        "BSIM4ndep",
        "BSIM4nsd",
        "BSIM4phin",
        "BSIM4ngate",
        "BSIM4gamma1",
        "BSIM4gamma2",
        "BSIM4vbx",
        "BSIM4vbi",
        "BSIM4vbm",
        "BSIM4xt",
        "BSIM4phi",
        "BSIM4litl",
        "BSIM4k1",
        "BSIM4kt1",
        "BSIM4kt1l",
        "BSIM4kt2",
        "BSIM4k2",
        "BSIM4k3",
        "BSIM4k3b",
        "BSIM4w0",
        "BSIM4dvtp0",
        "BSIM4dvtp1",
        "BSIM4dvtp2",
        "BSIM4dvtp3",
        "BSIM4dvtp4",
        "BSIM4dvtp5",
        "BSIM4lpe0",
        "BSIM4lpeb",
        "BSIM4dvt0",
        "BSIM4dvt1",
        "BSIM4dvt2",
        "BSIM4dvt0w",
        "BSIM4dvt1w",
        "BSIM4dvt2w",
        "BSIM4drout",
        "BSIM4dsub",
        "BSIM4vth0",
        "BSIM4ua",
        "BSIM4ua1",
        "BSIM4ub",
        "BSIM4ub1",
        "BSIM4uc",
        "BSIM4uc1",
        "BSIM4ud",
        "BSIM4ud1",
        "BSIM4up",
        "BSIM4lp",
        "BSIM4u0",
        "BSIM4eu",
        "BSIM4ucs",
        "BSIM4ute",
        "BSIM4ucste",
        "BSIM4voff",
        "BSIM4tvoff",
        "BSIM4tnfactor",
        "BSIM4teta0",
        "BSIM4tvoffcv",
        "BSIM4minv",
        "BSIM4minvcv",
        "BSIM4vfb",
        "BSIM4delta",
        "BSIM4rdsw",
        "BSIM4rds0",
        "BSIM4rs0",
        "BSIM4rd0",
        "BSIM4rsw",
        "BSIM4rdw",
        "BSIM4prwg",
        "BSIM4prwb",
        "BSIM4prt",
        "BSIM4eta0",
        "BSIM4etab",
        "BSIM4pclm",
        "BSIM4pdibl1",
        "BSIM4pdibl2",
        "BSIM4pdiblb",
        "BSIM4fprout",
        "BSIM4pdits",
        "BSIM4pditsd",
        "BSIM4pscbe1",
        "BSIM4pscbe2",
        "BSIM4pvag",
        "BSIM4wr",
        "BSIM4dwg",
        "BSIM4dwb",
        "BSIM4b0",
        "BSIM4b1",
        "BSIM4alpha0",
        "BSIM4alpha1",
        "BSIM4beta0",
        "BSIM4agidl",
        "BSIM4bgidl",
        "BSIM4cgidl",
        "BSIM4egidl",
        "BSIM4fgidl",
        "BSIM4kgidl",
        "BSIM4rgidl",
        "BSIM4agisl",
        "BSIM4bgisl",
        "BSIM4cgisl",
        "BSIM4egisl",
        "BSIM4fgisl",
        "BSIM4kgisl",
        "BSIM4rgisl",
        "BSIM4aigc",
        "BSIM4bigc",
        "BSIM4cigc",
        "BSIM4aigsd",
        "BSIM4bigsd",
        "BSIM4cigsd",
        "BSIM4aigs",
        "BSIM4bigs",
        "BSIM4cigs",
        "BSIM4aigd",
        "BSIM4bigd",
        "BSIM4cigd",
        "BSIM4aigbacc",
        "BSIM4bigbacc",
        "BSIM4cigbacc",
        "BSIM4aigbinv",
        "BSIM4bigbinv",
        "BSIM4cigbinv",
        "BSIM4nigc",
        "BSIM4nigbacc",
        "BSIM4nigbinv",
        "BSIM4ntox",
        "BSIM4eigbinv",
        "BSIM4pigcd",
        "BSIM4poxedge",
        "BSIM4xrcrg1",
        "BSIM4xrcrg2",
        "BSIM4lambda",
        "BSIM4vtl",
        "BSIM4xn",
        "BSIM4lc",
        "BSIM4tfactor",
        "BSIM4vfbsdoff",
        "BSIM4tvfbsdoff",
        "BSIM4ku0",
        "BSIM4kvth0",
        "BSIM4ku0temp",
        "BSIM4rho_ref",
        "BSIM4inv_od_ref",
        "BSIM4kvth0we",
        "BSIM4k2we",
        "BSIM4ku0we",
        "BSIM4cgsl",
        "BSIM4cgdl",
        "BSIM4ckappas",
        "BSIM4ckappad",
        "BSIM4cf",
        "BSIM4clc",
        "BSIM4cle",
        "BSIM4vfbcv",
        "BSIM4noff",
        "BSIM4voffcv",
        "BSIM4acde",
        "BSIM4moin",
        "BSIM4dw",
        "BSIM4dl",
        "BSIM4leff",
        "BSIM4weff",
        "BSIM4dwc",
        "BSIM4dlc",
        "BSIM4dwj",
        "BSIM4leffCV",
        "BSIM4weffCV",
        "BSIM4weffCJ",
        "BSIM4abulkCVfactor",
        "BSIM4cgso",
        "BSIM4cgdo",
        "BSIM4cgbo",
        "BSIM4u0temp",
        "BSIM4vsattemp",
        "BSIM4sqrtPhi",
        "BSIM4phis3",
        "BSIM4Xdep0",
        "BSIM4sqrtXdep0",
        "BSIM4theta0vb0",
        "BSIM4thetaRout",
        "BSIM4mstar",
        "BSIM4VgsteffVth",
        "BSIM4mstarcv",
        "BSIM4voffcbn",
        "BSIM4voffcbncv",
        "BSIM4rdswmin",
        "BSIM4rdwmin",
        "BSIM4rswmin",
        "BSIM4vfbsd",
        "BSIM4cof1",
        "BSIM4cof2",
        "BSIM4cof3",
        "BSIM4cof4",
        "BSIM4cdep0",
        "BSIM4ToxRatio",
        "BSIM4Aechvb",
        "BSIM4Bechvb",
        "BSIM4ToxRatioEdge",
        "BSIM4AechvbEdgeS",
        "BSIM4AechvbEdgeD",
        "BSIM4BechvbEdge",
        "BSIM4ldeb",
        "BSIM4k1ox",
        "BSIM4k2ox",
        "BSIM4vfbzbfactor",
        "BSIM4dvtp2factor",
    ];

    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,
    /// a given bit as 0 or 1).
    #[allow(dead_code)]
    pub(crate) fn to_values(&self) -> Vec<f64> {
        vec![
            self.Width,
            self.Length,
            self.NFinger,
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
            self.ketac,
            self.nsub,
            self.ndep,
            self.nsd,
            self.phin,
            self.ngate,
            self.gamma1,
            self.gamma2,
            self.vbx,
            self.vbi,
            self.vbm,
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
            self.dvtp0,
            self.dvtp1,
            self.dvtp2,
            self.dvtp3,
            self.dvtp4,
            self.dvtp5,
            self.lpe0,
            self.lpeb,
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
            self.ud,
            self.ud1,
            self.up,
            self.lp,
            self.u0,
            self.eu,
            self.ucs,
            self.ute,
            self.ucste,
            self.voff,
            self.tvoff,
            self.tnfactor,
            self.teta0,
            self.tvoffcv,
            self.minv,
            self.minvcv,
            self.vfb,
            self.delta,
            self.rdsw,
            self.rds0,
            self.rs0,
            self.rd0,
            self.rsw,
            self.rdw,
            self.prwg,
            self.prwb,
            self.prt,
            self.eta0,
            self.etab,
            self.pclm,
            self.pdibl1,
            self.pdibl2,
            self.pdiblb,
            self.fprout,
            self.pdits,
            self.pditsd,
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
            self.agidl,
            self.bgidl,
            self.cgidl,
            self.egidl,
            self.fgidl,
            self.kgidl,
            self.rgidl,
            self.agisl,
            self.bgisl,
            self.cgisl,
            self.egisl,
            self.fgisl,
            self.kgisl,
            self.rgisl,
            self.aigc,
            self.bigc,
            self.cigc,
            self.aigsd,
            self.bigsd,
            self.cigsd,
            self.aigs,
            self.bigs,
            self.cigs,
            self.aigd,
            self.bigd,
            self.cigd,
            self.aigbacc,
            self.bigbacc,
            self.cigbacc,
            self.aigbinv,
            self.bigbinv,
            self.cigbinv,
            self.nigc,
            self.nigbacc,
            self.nigbinv,
            self.ntox,
            self.eigbinv,
            self.pigcd,
            self.poxedge,
            self.xrcrg1,
            self.xrcrg2,
            self.lambda,
            self.vtl,
            self.xn,
            self.lc,
            self.tfactor,
            self.vfbsdoff,
            self.tvfbsdoff,
            self.ku0,
            self.kvth0,
            self.ku0temp,
            self.rho_ref,
            self.inv_od_ref,
            self.kvth0we,
            self.k2we,
            self.ku0we,
            self.cgsl,
            self.cgdl,
            self.ckappas,
            self.ckappad,
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
            self.dwj,
            self.leffCV,
            self.weffCV,
            self.weffCJ,
            self.abulkCVfactor,
            self.cgso,
            self.cgdo,
            self.cgbo,
            self.u0temp,
            self.vsattemp,
            self.sqrtPhi,
            self.phis3,
            self.Xdep0,
            self.sqrtXdep0,
            self.theta0vb0,
            self.thetaRout,
            self.mstar,
            self.VgsteffVth,
            self.mstarcv,
            self.voffcbn,
            self.voffcbncv,
            self.rdswmin,
            self.rdwmin,
            self.rswmin,
            self.vfbsd,
            self.cof1,
            self.cof2,
            self.cof3,
            self.cof4,
            self.cdep0,
            self.ToxRatio,
            self.Aechvb,
            self.Bechvb,
            self.ToxRatioEdge,
            self.AechvbEdgeS,
            self.AechvbEdgeD,
            self.BechvbEdge,
            self.ldeb,
            self.k1ox,
            self.k2ox,
            self.vfbzbfactor,
            self.dvtp2factor,
        ]
    }

    /// Stores numbers produced by [`Self::to_values`]; `values` must have
    /// [`Self::MEMBERS`]`.len()` entries.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub(crate) fn set_values(&mut self, values: &[f64]) {
        self.Width = values[0];
        self.Length = values[1];
        self.NFinger = values[2];
        self.cdsc = values[3];
        self.cdscb = values[4];
        self.cdscd = values[5];
        self.cit = values[6];
        self.nfactor = values[7];
        self.xj = values[8];
        self.vsat = values[9];
        self.at = values[10];
        self.a0 = values[11];
        self.ags = values[12];
        self.a1 = values[13];
        self.a2 = values[14];
        self.keta = values[15];
        self.ketac = values[16];
        self.nsub = values[17];
        self.ndep = values[18];
        self.nsd = values[19];
        self.phin = values[20];
        self.ngate = values[21];
        self.gamma1 = values[22];
        self.gamma2 = values[23];
        self.vbx = values[24];
        self.vbi = values[25];
        self.vbm = values[26];
        self.xt = values[27];
        self.phi = values[28];
        self.litl = values[29];
        self.k1 = values[30];
        self.kt1 = values[31];
        self.kt1l = values[32];
        self.kt2 = values[33];
        self.k2 = values[34];
        self.k3 = values[35];
        self.k3b = values[36];
        self.w0 = values[37];
        self.dvtp0 = values[38];
        self.dvtp1 = values[39];
        self.dvtp2 = values[40];
        self.dvtp3 = values[41];
        self.dvtp4 = values[42];
        self.dvtp5 = values[43];
        self.lpe0 = values[44];
        self.lpeb = values[45];
        self.dvt0 = values[46];
        self.dvt1 = values[47];
        self.dvt2 = values[48];
        self.dvt0w = values[49];
        self.dvt1w = values[50];
        self.dvt2w = values[51];
        self.drout = values[52];
        self.dsub = values[53];
        self.vth0 = values[54];
        self.ua = values[55];
        self.ua1 = values[56];
        self.ub = values[57];
        self.ub1 = values[58];
        self.uc = values[59];
        self.uc1 = values[60];
        self.ud = values[61];
        self.ud1 = values[62];
        self.up = values[63];
        self.lp = values[64];
        self.u0 = values[65];
        self.eu = values[66];
        self.ucs = values[67];
        self.ute = values[68];
        self.ucste = values[69];
        self.voff = values[70];
        self.tvoff = values[71];
        self.tnfactor = values[72];
        self.teta0 = values[73];
        self.tvoffcv = values[74];
        self.minv = values[75];
        self.minvcv = values[76];
        self.vfb = values[77];
        self.delta = values[78];
        self.rdsw = values[79];
        self.rds0 = values[80];
        self.rs0 = values[81];
        self.rd0 = values[82];
        self.rsw = values[83];
        self.rdw = values[84];
        self.prwg = values[85];
        self.prwb = values[86];
        self.prt = values[87];
        self.eta0 = values[88];
        self.etab = values[89];
        self.pclm = values[90];
        self.pdibl1 = values[91];
        self.pdibl2 = values[92];
        self.pdiblb = values[93];
        self.fprout = values[94];
        self.pdits = values[95];
        self.pditsd = values[96];
        self.pscbe1 = values[97];
        self.pscbe2 = values[98];
        self.pvag = values[99];
        self.wr = values[100];
        self.dwg = values[101];
        self.dwb = values[102];
        self.b0 = values[103];
        self.b1 = values[104];
        self.alpha0 = values[105];
        self.alpha1 = values[106];
        self.beta0 = values[107];
        self.agidl = values[108];
        self.bgidl = values[109];
        self.cgidl = values[110];
        self.egidl = values[111];
        self.fgidl = values[112];
        self.kgidl = values[113];
        self.rgidl = values[114];
        self.agisl = values[115];
        self.bgisl = values[116];
        self.cgisl = values[117];
        self.egisl = values[118];
        self.fgisl = values[119];
        self.kgisl = values[120];
        self.rgisl = values[121];
        self.aigc = values[122];
        self.bigc = values[123];
        self.cigc = values[124];
        self.aigsd = values[125];
        self.bigsd = values[126];
        self.cigsd = values[127];
        self.aigs = values[128];
        self.bigs = values[129];
        self.cigs = values[130];
        self.aigd = values[131];
        self.bigd = values[132];
        self.cigd = values[133];
        self.aigbacc = values[134];
        self.bigbacc = values[135];
        self.cigbacc = values[136];
        self.aigbinv = values[137];
        self.bigbinv = values[138];
        self.cigbinv = values[139];
        self.nigc = values[140];
        self.nigbacc = values[141];
        self.nigbinv = values[142];
        self.ntox = values[143];
        self.eigbinv = values[144];
        self.pigcd = values[145];
        self.poxedge = values[146];
        self.xrcrg1 = values[147];
        self.xrcrg2 = values[148];
        self.lambda = values[149];
        self.vtl = values[150];
        self.xn = values[151];
        self.lc = values[152];
        self.tfactor = values[153];
        self.vfbsdoff = values[154];
        self.tvfbsdoff = values[155];
        self.ku0 = values[156];
        self.kvth0 = values[157];
        self.ku0temp = values[158];
        self.rho_ref = values[159];
        self.inv_od_ref = values[160];
        self.kvth0we = values[161];
        self.k2we = values[162];
        self.ku0we = values[163];
        self.cgsl = values[164];
        self.cgdl = values[165];
        self.ckappas = values[166];
        self.ckappad = values[167];
        self.cf = values[168];
        self.clc = values[169];
        self.cle = values[170];
        self.vfbcv = values[171];
        self.noff = values[172];
        self.voffcv = values[173];
        self.acde = values[174];
        self.moin = values[175];
        self.dw = values[176];
        self.dl = values[177];
        self.leff = values[178];
        self.weff = values[179];
        self.dwc = values[180];
        self.dlc = values[181];
        self.dwj = values[182];
        self.leffCV = values[183];
        self.weffCV = values[184];
        self.weffCJ = values[185];
        self.abulkCVfactor = values[186];
        self.cgso = values[187];
        self.cgdo = values[188];
        self.cgbo = values[189];
        self.u0temp = values[190];
        self.vsattemp = values[191];
        self.sqrtPhi = values[192];
        self.phis3 = values[193];
        self.Xdep0 = values[194];
        self.sqrtXdep0 = values[195];
        self.theta0vb0 = values[196];
        self.thetaRout = values[197];
        self.mstar = values[198];
        self.VgsteffVth = values[199];
        self.mstarcv = values[200];
        self.voffcbn = values[201];
        self.voffcbncv = values[202];
        self.rdswmin = values[203];
        self.rdwmin = values[204];
        self.rswmin = values[205];
        self.vfbsd = values[206];
        self.cof1 = values[207];
        self.cof2 = values[208];
        self.cof3 = values[209];
        self.cof4 = values[210];
        self.cdep0 = values[211];
        self.ToxRatio = values[212];
        self.Aechvb = values[213];
        self.Bechvb = values[214];
        self.ToxRatioEdge = values[215];
        self.AechvbEdgeS = values[216];
        self.AechvbEdgeD = values[217];
        self.BechvbEdge = values[218];
        self.ldeb = values[219];
        self.k1ox = values[220];
        self.k2ox = values[221];
        self.vfbzbfactor = values[222];
        self.dvtp2factor = values[223];
    }
}

/// `struct sBSIM4instance`'s scalar members (370 of them), in declaration order.
#[derive(Clone, Default)]
pub(crate) struct Instance {
    pub(crate) dNode: i32,
    pub(crate) gNodeExt: i32,
    pub(crate) sNode: i32,
    pub(crate) bNode: i32,
    pub(crate) dNodePrime: i32,
    pub(crate) gNodePrime: i32,
    pub(crate) gNodeMid: i32,
    pub(crate) sNodePrime: i32,
    pub(crate) bNodePrime: i32,
    pub(crate) dbNode: i32,
    pub(crate) sbNode: i32,
    pub(crate) qNode: i32,
    pub(crate) ueff: f64,
    pub(crate) thetavth: f64,
    pub(crate) von: f64,
    pub(crate) vdsat: f64,
    pub(crate) cgdo: f64,
    pub(crate) qgdo: f64,
    pub(crate) cgso: f64,
    pub(crate) qgso: f64,
    pub(crate) grbsb: f64,
    pub(crate) grbdb: f64,
    pub(crate) grbpb: f64,
    pub(crate) grbps: f64,
    pub(crate) grbpd: f64,
    pub(crate) vjsmFwd: f64,
    pub(crate) vjsmRev: f64,
    pub(crate) vjdmFwd: f64,
    pub(crate) vjdmRev: f64,
    pub(crate) XExpBVS: f64,
    pub(crate) XExpBVD: f64,
    pub(crate) SslpFwd: f64,
    pub(crate) SslpRev: f64,
    pub(crate) DslpFwd: f64,
    pub(crate) DslpRev: f64,
    pub(crate) IVjsmFwd: f64,
    pub(crate) IVjsmRev: f64,
    pub(crate) IVjdmFwd: f64,
    pub(crate) IVjdmRev: f64,
    pub(crate) grgeltd: f64,
    pub(crate) Pseff: f64,
    pub(crate) Pdeff: f64,
    pub(crate) Aseff: f64,
    pub(crate) Adeff: f64,
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
    pub(crate) sa: f64,
    pub(crate) sb: f64,
    pub(crate) sd: f64,
    pub(crate) sca: f64,
    pub(crate) scb: f64,
    pub(crate) scc: f64,
    pub(crate) sc: f64,
    pub(crate) rbdb: f64,
    pub(crate) rbsb: f64,
    pub(crate) rbpb: f64,
    pub(crate) rbps: f64,
    pub(crate) rbpd: f64,
    pub(crate) delvto: f64,
    pub(crate) mulu0: f64,
    pub(crate) wnflag: i32,
    pub(crate) xgw: f64,
    pub(crate) ngcon: f64,
    pub(crate) dtemp: f64,
    pub(crate) u0temp: f64,
    pub(crate) vsattemp: f64,
    pub(crate) vth0: f64,
    pub(crate) vfb: f64,
    pub(crate) vfbzb: f64,
    pub(crate) vtfbphi1: f64,
    pub(crate) vtfbphi2: f64,
    pub(crate) k2: f64,
    pub(crate) vbsc: f64,
    pub(crate) k2ox: f64,
    pub(crate) eta0: f64,
    pub(crate) toxp: f64,
    pub(crate) coxp: f64,
    pub(crate) icVDS: f64,
    pub(crate) icVGS: f64,
    pub(crate) icVBS: f64,
    pub(crate) mult_i: f64,
    pub(crate) mult_q: f64,
    pub(crate) mult_fn: f64,
    pub(crate) nf: f64,
    pub(crate) off: i32,
    pub(crate) mode: i32,
    pub(crate) trnqsMod: i32,
    pub(crate) acnqsMod: i32,
    pub(crate) rbodyMod: i32,
    pub(crate) rgateMod: i32,
    pub(crate) geoMod: i32,
    pub(crate) rgeoMod: i32,
    pub(crate) min: i32,
    pub(crate) Vgsteff: f64,
    pub(crate) vgs_eff: f64,
    pub(crate) vgd_eff: f64,
    pub(crate) dvgs_eff_dvg: f64,
    pub(crate) dvgd_eff_dvg: f64,
    pub(crate) Vdseff: f64,
    pub(crate) nstar: f64,
    pub(crate) Abulk: f64,
    pub(crate) EsatL: f64,
    pub(crate) AbovVgst2Vtm: f64,
    pub(crate) qinv: f64,
    pub(crate) cd: f64,
    pub(crate) cbs: f64,
    pub(crate) cbd: f64,
    pub(crate) csub: f64,
    pub(crate) Igidl: f64,
    pub(crate) Igisl: f64,
    pub(crate) gm: f64,
    pub(crate) gds: f64,
    pub(crate) gmbs: f64,
    pub(crate) gbd: f64,
    pub(crate) gbs: f64,
    pub(crate) noiGd0: f64,
    pub(crate) Coxeff: f64,
    pub(crate) gbbs: f64,
    pub(crate) gbgs: f64,
    pub(crate) gbds: f64,
    pub(crate) ggidld: f64,
    pub(crate) ggidlg: f64,
    pub(crate) ggidls: f64,
    pub(crate) ggidlb: f64,
    pub(crate) ggisld: f64,
    pub(crate) ggislg: f64,
    pub(crate) ggisls: f64,
    pub(crate) ggislb: f64,
    pub(crate) Igcs: f64,
    pub(crate) gIgcsg: f64,
    pub(crate) gIgcsd: f64,
    pub(crate) gIgcss: f64,
    pub(crate) gIgcsb: f64,
    pub(crate) Igcd: f64,
    pub(crate) gIgcdg: f64,
    pub(crate) gIgcdd: f64,
    pub(crate) gIgcds: f64,
    pub(crate) gIgcdb: f64,
    pub(crate) Igs: f64,
    pub(crate) gIgsg: f64,
    pub(crate) gIgss: f64,
    pub(crate) Igd: f64,
    pub(crate) gIgdg: f64,
    pub(crate) gIgdd: f64,
    pub(crate) Igb: f64,
    pub(crate) gIgbg: f64,
    pub(crate) gIgbd: f64,
    pub(crate) gIgbs: f64,
    pub(crate) gIgbb: f64,
    pub(crate) grdsw: f64,
    pub(crate) IdovVds: f64,
    pub(crate) gcrg: f64,
    pub(crate) gcrgd: f64,
    pub(crate) gcrgg: f64,
    pub(crate) gcrgs: f64,
    pub(crate) gcrgb: f64,
    pub(crate) gstot: f64,
    pub(crate) gstotd: f64,
    pub(crate) gstotg: f64,
    pub(crate) gstots: f64,
    pub(crate) gstotb: f64,
    pub(crate) gdtot: f64,
    pub(crate) gdtotd: f64,
    pub(crate) gdtotg: f64,
    pub(crate) gdtots: f64,
    pub(crate) gdtotb: f64,
    pub(crate) cggb: f64,
    pub(crate) cgdb: f64,
    pub(crate) cgsb: f64,
    pub(crate) cbgb: f64,
    pub(crate) cbdb: f64,
    pub(crate) cbsb: f64,
    pub(crate) cdgb: f64,
    pub(crate) cddb: f64,
    pub(crate) cdsb: f64,
    pub(crate) csgb: f64,
    pub(crate) csdb: f64,
    pub(crate) cssb: f64,
    pub(crate) cgbb: f64,
    pub(crate) cdbb: f64,
    pub(crate) csbb: f64,
    pub(crate) cbbb: f64,
    pub(crate) capbd: f64,
    pub(crate) capbs: f64,
    pub(crate) cqgb: f64,
    pub(crate) cqdb: f64,
    pub(crate) cqsb: f64,
    pub(crate) cqbb: f64,
    pub(crate) qgate: f64,
    pub(crate) qbulk: f64,
    pub(crate) qdrn: f64,
    pub(crate) qsrc: f64,
    pub(crate) qdef: f64,
    pub(crate) qchqs: f64,
    pub(crate) taunet: f64,
    pub(crate) gtau: f64,
    pub(crate) gtg: f64,
    pub(crate) gtd: f64,
    pub(crate) gts: f64,
    pub(crate) gtb: f64,
    pub(crate) SjctTempRevSatCur: f64,
    pub(crate) DjctTempRevSatCur: f64,
    pub(crate) SswTempRevSatCur: f64,
    pub(crate) DswTempRevSatCur: f64,
    pub(crate) SswgTempRevSatCur: f64,
    pub(crate) DswgTempRevSatCur: f64,
    pub(crate) lGiven: bool,
    pub(crate) wGiven: bool,
    pub(crate) mGiven: bool,
    pub(crate) mult_iGiven: bool,
    pub(crate) mult_qGiven: bool,
    pub(crate) mult_fnGiven: bool,
    pub(crate) nfGiven: bool,
    pub(crate) minGiven: bool,
    pub(crate) drainAreaGiven: bool,
    pub(crate) sourceAreaGiven: bool,
    pub(crate) drainSquaresGiven: bool,
    pub(crate) sourceSquaresGiven: bool,
    pub(crate) drainPerimeterGiven: bool,
    pub(crate) sourcePerimeterGiven: bool,
    pub(crate) saGiven: bool,
    pub(crate) sbGiven: bool,
    pub(crate) sdGiven: bool,
    pub(crate) scaGiven: bool,
    pub(crate) scbGiven: bool,
    pub(crate) sccGiven: bool,
    pub(crate) scGiven: bool,
    pub(crate) rbdbGiven: bool,
    pub(crate) rbsbGiven: bool,
    pub(crate) rbpbGiven: bool,
    pub(crate) rbpdGiven: bool,
    pub(crate) rbpsGiven: bool,
    pub(crate) delvtoGiven: bool,
    pub(crate) mulu0Given: bool,
    pub(crate) wnflagGiven: bool,
    pub(crate) xgwGiven: bool,
    pub(crate) ngconGiven: bool,
    pub(crate) dtempGiven: bool,
    pub(crate) icVDSGiven: bool,
    pub(crate) icVGSGiven: bool,
    pub(crate) icVBSGiven: bool,
    pub(crate) trnqsModGiven: bool,
    pub(crate) acnqsModGiven: bool,
    pub(crate) rbodyModGiven: bool,
    pub(crate) rgateModGiven: bool,
    pub(crate) geoModGiven: bool,
    pub(crate) rgeoModGiven: bool,
    pub(crate) rhsdPrime: f64,
    pub(crate) rhsgPrime: f64,
    pub(crate) rhsgExt: f64,
    pub(crate) grhsMid: f64,
    pub(crate) rhsbPrime: f64,
    pub(crate) rhssPrime: f64,
    pub(crate) rhsdb: f64,
    pub(crate) rhssb: f64,
    pub(crate) rhsd: f64,
    pub(crate) rhss: f64,
    pub(crate) rhsq: f64,
    pub(crate) _1: f64,
    pub(crate) _2: f64,
    pub(crate) _3: f64,
    pub(crate) _4: f64,
    pub(crate) _5: f64,
    pub(crate) _6: f64,
    pub(crate) _7: f64,
    pub(crate) _8: f64,
    pub(crate) _9: f64,
    pub(crate) _10: f64,
    pub(crate) _11: f64,
    pub(crate) _12: f64,
    pub(crate) _13: f64,
    pub(crate) _14: f64,
    pub(crate) _15: f64,
    pub(crate) _16: f64,
    pub(crate) _17: f64,
    pub(crate) _18: f64,
    pub(crate) _19: f64,
    pub(crate) _20: f64,
    pub(crate) _21: f64,
    pub(crate) _22: f64,
    pub(crate) _23: f64,
    pub(crate) _24: f64,
    pub(crate) _25: f64,
    pub(crate) _26: f64,
    pub(crate) _27: f64,
    pub(crate) _28: f64,
    pub(crate) _29: f64,
    pub(crate) _30: f64,
    pub(crate) _31: f64,
    pub(crate) _32: f64,
    pub(crate) _33: f64,
    pub(crate) _34: f64,
    pub(crate) _35: f64,
    pub(crate) _36: f64,
    pub(crate) _37: f64,
    pub(crate) _38: f64,
    pub(crate) _39: f64,
    pub(crate) _40: f64,
    pub(crate) _41: f64,
    pub(crate) _42: f64,
    pub(crate) _43: f64,
    pub(crate) _44: f64,
    pub(crate) _45: f64,
    pub(crate) _46: f64,
    pub(crate) _47: f64,
    pub(crate) _48: f64,
    pub(crate) _49: f64,
    pub(crate) _50: f64,
    pub(crate) _51: f64,
    pub(crate) _52: f64,
    pub(crate) _53: f64,
    pub(crate) _54: f64,
    pub(crate) _55: f64,
    pub(crate) _56: f64,
    pub(crate) _57: f64,
    pub(crate) _58: f64,
    pub(crate) _59: f64,
    pub(crate) _60: f64,
    pub(crate) _61: f64,
    pub(crate) _62: f64,
    pub(crate) _63: f64,
    pub(crate) _64: f64,
    pub(crate) _65: f64,
    pub(crate) _66: f64,
    pub(crate) _67: f64,
    pub(crate) _68: f64,
    pub(crate) _69: f64,
    pub(crate) _70: f64,
    pub(crate) _71: f64,
    pub(crate) _72: f64,
    pub(crate) _73: f64,
    pub(crate) _74: f64,
    pub(crate) _75: f64,
    pub(crate) _76: f64,
    pub(crate) _77: f64,
    pub(crate) _78: f64,
    pub(crate) _79: f64,
    pub(crate) _80: f64,
    pub(crate) _81: f64,
    pub(crate) _82: f64,
    pub(crate) _83: f64,
    pub(crate) _84: f64,
    pub(crate) _85: f64,
    pub(crate) _86: f64,
    pub(crate) _87: f64,
    pub(crate) _88: f64,
    pub(crate) _89: f64,
    pub(crate) _90: f64,
    pub(crate) _91: f64,
    pub(crate) _92: f64,
    pub(crate) _93: f64,
    pub(crate) _94: f64,
    pub(crate) _95: f64,
    pub(crate) _96: f64,
    pub(crate) _97: f64,
    pub(crate) _98: f64,
    pub(crate) _99: f64,
    pub(crate) _100: f64,
    pub(crate) _101: f64,
    pub(crate) _102: f64,
    pub(crate) _103: f64,
}

#[rustfmt::skip]
impl Instance {
    /// The `f64` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn real_mut(&mut self, c_name: &str) -> Option<&mut f64> {
        Some(match c_name {
            "BSIM4ueff" => &mut self.ueff,
            "BSIM4thetavth" => &mut self.thetavth,
            "BSIM4von" => &mut self.von,
            "BSIM4vdsat" => &mut self.vdsat,
            "BSIM4cgdo" => &mut self.cgdo,
            "BSIM4qgdo" => &mut self.qgdo,
            "BSIM4cgso" => &mut self.cgso,
            "BSIM4qgso" => &mut self.qgso,
            "BSIM4grbsb" => &mut self.grbsb,
            "BSIM4grbdb" => &mut self.grbdb,
            "BSIM4grbpb" => &mut self.grbpb,
            "BSIM4grbps" => &mut self.grbps,
            "BSIM4grbpd" => &mut self.grbpd,
            "BSIM4vjsmFwd" => &mut self.vjsmFwd,
            "BSIM4vjsmRev" => &mut self.vjsmRev,
            "BSIM4vjdmFwd" => &mut self.vjdmFwd,
            "BSIM4vjdmRev" => &mut self.vjdmRev,
            "BSIM4XExpBVS" => &mut self.XExpBVS,
            "BSIM4XExpBVD" => &mut self.XExpBVD,
            "BSIM4SslpFwd" => &mut self.SslpFwd,
            "BSIM4SslpRev" => &mut self.SslpRev,
            "BSIM4DslpFwd" => &mut self.DslpFwd,
            "BSIM4DslpRev" => &mut self.DslpRev,
            "BSIM4IVjsmFwd" => &mut self.IVjsmFwd,
            "BSIM4IVjsmRev" => &mut self.IVjsmRev,
            "BSIM4IVjdmFwd" => &mut self.IVjdmFwd,
            "BSIM4IVjdmRev" => &mut self.IVjdmRev,
            "BSIM4grgeltd" => &mut self.grgeltd,
            "BSIM4Pseff" => &mut self.Pseff,
            "BSIM4Pdeff" => &mut self.Pdeff,
            "BSIM4Aseff" => &mut self.Aseff,
            "BSIM4Adeff" => &mut self.Adeff,
            "BSIM4l" => &mut self.l,
            "BSIM4w" => &mut self.w,
            "BSIM4m" => &mut self.m,
            "BSIM4drainArea" => &mut self.drainArea,
            "BSIM4sourceArea" => &mut self.sourceArea,
            "BSIM4drainSquares" => &mut self.drainSquares,
            "BSIM4sourceSquares" => &mut self.sourceSquares,
            "BSIM4drainPerimeter" => &mut self.drainPerimeter,
            "BSIM4sourcePerimeter" => &mut self.sourcePerimeter,
            "BSIM4sourceConductance" => &mut self.sourceConductance,
            "BSIM4drainConductance" => &mut self.drainConductance,
            "BSIM4sa" => &mut self.sa,
            "BSIM4sb" => &mut self.sb,
            "BSIM4sd" => &mut self.sd,
            "BSIM4sca" => &mut self.sca,
            "BSIM4scb" => &mut self.scb,
            "BSIM4scc" => &mut self.scc,
            "BSIM4sc" => &mut self.sc,
            "BSIM4rbdb" => &mut self.rbdb,
            "BSIM4rbsb" => &mut self.rbsb,
            "BSIM4rbpb" => &mut self.rbpb,
            "BSIM4rbps" => &mut self.rbps,
            "BSIM4rbpd" => &mut self.rbpd,
            "BSIM4delvto" => &mut self.delvto,
            "BSIM4mulu0" => &mut self.mulu0,
            "BSIM4xgw" => &mut self.xgw,
            "BSIM4ngcon" => &mut self.ngcon,
            "BSIM4dtemp" => &mut self.dtemp,
            "BSIM4u0temp" => &mut self.u0temp,
            "BSIM4vsattemp" => &mut self.vsattemp,
            "BSIM4vth0" => &mut self.vth0,
            "BSIM4vfb" => &mut self.vfb,
            "BSIM4vfbzb" => &mut self.vfbzb,
            "BSIM4vtfbphi1" => &mut self.vtfbphi1,
            "BSIM4vtfbphi2" => &mut self.vtfbphi2,
            "BSIM4k2" => &mut self.k2,
            "BSIM4vbsc" => &mut self.vbsc,
            "BSIM4k2ox" => &mut self.k2ox,
            "BSIM4eta0" => &mut self.eta0,
            "BSIM4toxp" => &mut self.toxp,
            "BSIM4coxp" => &mut self.coxp,
            "BSIM4icVDS" => &mut self.icVDS,
            "BSIM4icVGS" => &mut self.icVGS,
            "BSIM4icVBS" => &mut self.icVBS,
            "BSIM4mult_i" => &mut self.mult_i,
            "BSIM4mult_q" => &mut self.mult_q,
            "BSIM4mult_fn" => &mut self.mult_fn,
            "BSIM4nf" => &mut self.nf,
            "BSIM4Vgsteff" => &mut self.Vgsteff,
            "BSIM4vgs_eff" => &mut self.vgs_eff,
            "BSIM4vgd_eff" => &mut self.vgd_eff,
            "BSIM4dvgs_eff_dvg" => &mut self.dvgs_eff_dvg,
            "BSIM4dvgd_eff_dvg" => &mut self.dvgd_eff_dvg,
            "BSIM4Vdseff" => &mut self.Vdseff,
            "BSIM4nstar" => &mut self.nstar,
            "BSIM4Abulk" => &mut self.Abulk,
            "BSIM4EsatL" => &mut self.EsatL,
            "BSIM4AbovVgst2Vtm" => &mut self.AbovVgst2Vtm,
            "BSIM4qinv" => &mut self.qinv,
            "BSIM4cd" => &mut self.cd,
            "BSIM4cbs" => &mut self.cbs,
            "BSIM4cbd" => &mut self.cbd,
            "BSIM4csub" => &mut self.csub,
            "BSIM4Igidl" => &mut self.Igidl,
            "BSIM4Igisl" => &mut self.Igisl,
            "BSIM4gm" => &mut self.gm,
            "BSIM4gds" => &mut self.gds,
            "BSIM4gmbs" => &mut self.gmbs,
            "BSIM4gbd" => &mut self.gbd,
            "BSIM4gbs" => &mut self.gbs,
            "BSIM4noiGd0" => &mut self.noiGd0,
            "BSIM4Coxeff" => &mut self.Coxeff,
            "BSIM4gbbs" => &mut self.gbbs,
            "BSIM4gbgs" => &mut self.gbgs,
            "BSIM4gbds" => &mut self.gbds,
            "BSIM4ggidld" => &mut self.ggidld,
            "BSIM4ggidlg" => &mut self.ggidlg,
            "BSIM4ggidls" => &mut self.ggidls,
            "BSIM4ggidlb" => &mut self.ggidlb,
            "BSIM4ggisld" => &mut self.ggisld,
            "BSIM4ggislg" => &mut self.ggislg,
            "BSIM4ggisls" => &mut self.ggisls,
            "BSIM4ggislb" => &mut self.ggislb,
            "BSIM4Igcs" => &mut self.Igcs,
            "BSIM4gIgcsg" => &mut self.gIgcsg,
            "BSIM4gIgcsd" => &mut self.gIgcsd,
            "BSIM4gIgcss" => &mut self.gIgcss,
            "BSIM4gIgcsb" => &mut self.gIgcsb,
            "BSIM4Igcd" => &mut self.Igcd,
            "BSIM4gIgcdg" => &mut self.gIgcdg,
            "BSIM4gIgcdd" => &mut self.gIgcdd,
            "BSIM4gIgcds" => &mut self.gIgcds,
            "BSIM4gIgcdb" => &mut self.gIgcdb,
            "BSIM4Igs" => &mut self.Igs,
            "BSIM4gIgsg" => &mut self.gIgsg,
            "BSIM4gIgss" => &mut self.gIgss,
            "BSIM4Igd" => &mut self.Igd,
            "BSIM4gIgdg" => &mut self.gIgdg,
            "BSIM4gIgdd" => &mut self.gIgdd,
            "BSIM4Igb" => &mut self.Igb,
            "BSIM4gIgbg" => &mut self.gIgbg,
            "BSIM4gIgbd" => &mut self.gIgbd,
            "BSIM4gIgbs" => &mut self.gIgbs,
            "BSIM4gIgbb" => &mut self.gIgbb,
            "BSIM4grdsw" => &mut self.grdsw,
            "BSIM4IdovVds" => &mut self.IdovVds,
            "BSIM4gcrg" => &mut self.gcrg,
            "BSIM4gcrgd" => &mut self.gcrgd,
            "BSIM4gcrgg" => &mut self.gcrgg,
            "BSIM4gcrgs" => &mut self.gcrgs,
            "BSIM4gcrgb" => &mut self.gcrgb,
            "BSIM4gstot" => &mut self.gstot,
            "BSIM4gstotd" => &mut self.gstotd,
            "BSIM4gstotg" => &mut self.gstotg,
            "BSIM4gstots" => &mut self.gstots,
            "BSIM4gstotb" => &mut self.gstotb,
            "BSIM4gdtot" => &mut self.gdtot,
            "BSIM4gdtotd" => &mut self.gdtotd,
            "BSIM4gdtotg" => &mut self.gdtotg,
            "BSIM4gdtots" => &mut self.gdtots,
            "BSIM4gdtotb" => &mut self.gdtotb,
            "BSIM4cggb" => &mut self.cggb,
            "BSIM4cgdb" => &mut self.cgdb,
            "BSIM4cgsb" => &mut self.cgsb,
            "BSIM4cbgb" => &mut self.cbgb,
            "BSIM4cbdb" => &mut self.cbdb,
            "BSIM4cbsb" => &mut self.cbsb,
            "BSIM4cdgb" => &mut self.cdgb,
            "BSIM4cddb" => &mut self.cddb,
            "BSIM4cdsb" => &mut self.cdsb,
            "BSIM4csgb" => &mut self.csgb,
            "BSIM4csdb" => &mut self.csdb,
            "BSIM4cssb" => &mut self.cssb,
            "BSIM4cgbb" => &mut self.cgbb,
            "BSIM4cdbb" => &mut self.cdbb,
            "BSIM4csbb" => &mut self.csbb,
            "BSIM4cbbb" => &mut self.cbbb,
            "BSIM4capbd" => &mut self.capbd,
            "BSIM4capbs" => &mut self.capbs,
            "BSIM4cqgb" => &mut self.cqgb,
            "BSIM4cqdb" => &mut self.cqdb,
            "BSIM4cqsb" => &mut self.cqsb,
            "BSIM4cqbb" => &mut self.cqbb,
            "BSIM4qgate" => &mut self.qgate,
            "BSIM4qbulk" => &mut self.qbulk,
            "BSIM4qdrn" => &mut self.qdrn,
            "BSIM4qsrc" => &mut self.qsrc,
            "BSIM4qdef" => &mut self.qdef,
            "BSIM4qchqs" => &mut self.qchqs,
            "BSIM4taunet" => &mut self.taunet,
            "BSIM4gtau" => &mut self.gtau,
            "BSIM4gtg" => &mut self.gtg,
            "BSIM4gtd" => &mut self.gtd,
            "BSIM4gts" => &mut self.gts,
            "BSIM4gtb" => &mut self.gtb,
            "BSIM4SjctTempRevSatCur" => &mut self.SjctTempRevSatCur,
            "BSIM4DjctTempRevSatCur" => &mut self.DjctTempRevSatCur,
            "BSIM4SswTempRevSatCur" => &mut self.SswTempRevSatCur,
            "BSIM4DswTempRevSatCur" => &mut self.DswTempRevSatCur,
            "BSIM4SswgTempRevSatCur" => &mut self.SswgTempRevSatCur,
            "BSIM4DswgTempRevSatCur" => &mut self.DswgTempRevSatCur,
            "BSIM4rhsdPrime" => &mut self.rhsdPrime,
            "BSIM4rhsgPrime" => &mut self.rhsgPrime,
            "BSIM4rhsgExt" => &mut self.rhsgExt,
            "BSIM4grhsMid" => &mut self.grhsMid,
            "BSIM4rhsbPrime" => &mut self.rhsbPrime,
            "BSIM4rhssPrime" => &mut self.rhssPrime,
            "BSIM4rhsdb" => &mut self.rhsdb,
            "BSIM4rhssb" => &mut self.rhssb,
            "BSIM4rhsd" => &mut self.rhsd,
            "BSIM4rhss" => &mut self.rhss,
            "BSIM4rhsq" => &mut self.rhsq,
            "BSIM4_1" => &mut self._1,
            "BSIM4_2" => &mut self._2,
            "BSIM4_3" => &mut self._3,
            "BSIM4_4" => &mut self._4,
            "BSIM4_5" => &mut self._5,
            "BSIM4_6" => &mut self._6,
            "BSIM4_7" => &mut self._7,
            "BSIM4_8" => &mut self._8,
            "BSIM4_9" => &mut self._9,
            "BSIM4_10" => &mut self._10,
            "BSIM4_11" => &mut self._11,
            "BSIM4_12" => &mut self._12,
            "BSIM4_13" => &mut self._13,
            "BSIM4_14" => &mut self._14,
            "BSIM4_15" => &mut self._15,
            "BSIM4_16" => &mut self._16,
            "BSIM4_17" => &mut self._17,
            "BSIM4_18" => &mut self._18,
            "BSIM4_19" => &mut self._19,
            "BSIM4_20" => &mut self._20,
            "BSIM4_21" => &mut self._21,
            "BSIM4_22" => &mut self._22,
            "BSIM4_23" => &mut self._23,
            "BSIM4_24" => &mut self._24,
            "BSIM4_25" => &mut self._25,
            "BSIM4_26" => &mut self._26,
            "BSIM4_27" => &mut self._27,
            "BSIM4_28" => &mut self._28,
            "BSIM4_29" => &mut self._29,
            "BSIM4_30" => &mut self._30,
            "BSIM4_31" => &mut self._31,
            "BSIM4_32" => &mut self._32,
            "BSIM4_33" => &mut self._33,
            "BSIM4_34" => &mut self._34,
            "BSIM4_35" => &mut self._35,
            "BSIM4_36" => &mut self._36,
            "BSIM4_37" => &mut self._37,
            "BSIM4_38" => &mut self._38,
            "BSIM4_39" => &mut self._39,
            "BSIM4_40" => &mut self._40,
            "BSIM4_41" => &mut self._41,
            "BSIM4_42" => &mut self._42,
            "BSIM4_43" => &mut self._43,
            "BSIM4_44" => &mut self._44,
            "BSIM4_45" => &mut self._45,
            "BSIM4_46" => &mut self._46,
            "BSIM4_47" => &mut self._47,
            "BSIM4_48" => &mut self._48,
            "BSIM4_49" => &mut self._49,
            "BSIM4_50" => &mut self._50,
            "BSIM4_51" => &mut self._51,
            "BSIM4_52" => &mut self._52,
            "BSIM4_53" => &mut self._53,
            "BSIM4_54" => &mut self._54,
            "BSIM4_55" => &mut self._55,
            "BSIM4_56" => &mut self._56,
            "BSIM4_57" => &mut self._57,
            "BSIM4_58" => &mut self._58,
            "BSIM4_59" => &mut self._59,
            "BSIM4_60" => &mut self._60,
            "BSIM4_61" => &mut self._61,
            "BSIM4_62" => &mut self._62,
            "BSIM4_63" => &mut self._63,
            "BSIM4_64" => &mut self._64,
            "BSIM4_65" => &mut self._65,
            "BSIM4_66" => &mut self._66,
            "BSIM4_67" => &mut self._67,
            "BSIM4_68" => &mut self._68,
            "BSIM4_69" => &mut self._69,
            "BSIM4_70" => &mut self._70,
            "BSIM4_71" => &mut self._71,
            "BSIM4_72" => &mut self._72,
            "BSIM4_73" => &mut self._73,
            "BSIM4_74" => &mut self._74,
            "BSIM4_75" => &mut self._75,
            "BSIM4_76" => &mut self._76,
            "BSIM4_77" => &mut self._77,
            "BSIM4_78" => &mut self._78,
            "BSIM4_79" => &mut self._79,
            "BSIM4_80" => &mut self._80,
            "BSIM4_81" => &mut self._81,
            "BSIM4_82" => &mut self._82,
            "BSIM4_83" => &mut self._83,
            "BSIM4_84" => &mut self._84,
            "BSIM4_85" => &mut self._85,
            "BSIM4_86" => &mut self._86,
            "BSIM4_87" => &mut self._87,
            "BSIM4_88" => &mut self._88,
            "BSIM4_89" => &mut self._89,
            "BSIM4_90" => &mut self._90,
            "BSIM4_91" => &mut self._91,
            "BSIM4_92" => &mut self._92,
            "BSIM4_93" => &mut self._93,
            "BSIM4_94" => &mut self._94,
            "BSIM4_95" => &mut self._95,
            "BSIM4_96" => &mut self._96,
            "BSIM4_97" => &mut self._97,
            "BSIM4_98" => &mut self._98,
            "BSIM4_99" => &mut self._99,
            "BSIM4_100" => &mut self._100,
            "BSIM4_101" => &mut self._101,
            "BSIM4_102" => &mut self._102,
            "BSIM4_103" => &mut self._103,
            _ => return None,
        })
    }
    /// The `i32` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn int_mut(&mut self, c_name: &str) -> Option<&mut i32> {
        Some(match c_name {
            "BSIM4dNode" => &mut self.dNode,
            "BSIM4gNodeExt" => &mut self.gNodeExt,
            "BSIM4sNode" => &mut self.sNode,
            "BSIM4bNode" => &mut self.bNode,
            "BSIM4dNodePrime" => &mut self.dNodePrime,
            "BSIM4gNodePrime" => &mut self.gNodePrime,
            "BSIM4gNodeMid" => &mut self.gNodeMid,
            "BSIM4sNodePrime" => &mut self.sNodePrime,
            "BSIM4bNodePrime" => &mut self.bNodePrime,
            "BSIM4dbNode" => &mut self.dbNode,
            "BSIM4sbNode" => &mut self.sbNode,
            "BSIM4qNode" => &mut self.qNode,
            "BSIM4wnflag" => &mut self.wnflag,
            "BSIM4off" => &mut self.off,
            "BSIM4mode" => &mut self.mode,
            "BSIM4trnqsMod" => &mut self.trnqsMod,
            "BSIM4acnqsMod" => &mut self.acnqsMod,
            "BSIM4rbodyMod" => &mut self.rbodyMod,
            "BSIM4rgateMod" => &mut self.rgateMod,
            "BSIM4geoMod" => &mut self.geoMod,
            "BSIM4rgeoMod" => &mut self.rgeoMod,
            "BSIM4min" => &mut self.min,
            _ => return None,
        })
    }
    /// The `bool` member named `c_name` in C, if there is one.
    #[allow(dead_code)]
    pub(crate) fn flag_mut(&mut self, c_name: &str) -> Option<&mut bool> {
        Some(match c_name {
            "BSIM4lGiven" => &mut self.lGiven,
            "BSIM4wGiven" => &mut self.wGiven,
            "BSIM4mGiven" => &mut self.mGiven,
            "BSIM4mult_iGiven" => &mut self.mult_iGiven,
            "BSIM4mult_qGiven" => &mut self.mult_qGiven,
            "BSIM4mult_fnGiven" => &mut self.mult_fnGiven,
            "BSIM4nfGiven" => &mut self.nfGiven,
            "BSIM4minGiven" => &mut self.minGiven,
            "BSIM4drainAreaGiven" => &mut self.drainAreaGiven,
            "BSIM4sourceAreaGiven" => &mut self.sourceAreaGiven,
            "BSIM4drainSquaresGiven" => &mut self.drainSquaresGiven,
            "BSIM4sourceSquaresGiven" => &mut self.sourceSquaresGiven,
            "BSIM4drainPerimeterGiven" => &mut self.drainPerimeterGiven,
            "BSIM4sourcePerimeterGiven" => &mut self.sourcePerimeterGiven,
            "BSIM4saGiven" => &mut self.saGiven,
            "BSIM4sbGiven" => &mut self.sbGiven,
            "BSIM4sdGiven" => &mut self.sdGiven,
            "BSIM4scaGiven" => &mut self.scaGiven,
            "BSIM4scbGiven" => &mut self.scbGiven,
            "BSIM4sccGiven" => &mut self.sccGiven,
            "BSIM4scGiven" => &mut self.scGiven,
            "BSIM4rbdbGiven" => &mut self.rbdbGiven,
            "BSIM4rbsbGiven" => &mut self.rbsbGiven,
            "BSIM4rbpbGiven" => &mut self.rbpbGiven,
            "BSIM4rbpdGiven" => &mut self.rbpdGiven,
            "BSIM4rbpsGiven" => &mut self.rbpsGiven,
            "BSIM4delvtoGiven" => &mut self.delvtoGiven,
            "BSIM4mulu0Given" => &mut self.mulu0Given,
            "BSIM4wnflagGiven" => &mut self.wnflagGiven,
            "BSIM4xgwGiven" => &mut self.xgwGiven,
            "BSIM4ngconGiven" => &mut self.ngconGiven,
            "BSIM4dtempGiven" => &mut self.dtempGiven,
            "BSIM4icVDSGiven" => &mut self.icVDSGiven,
            "BSIM4icVGSGiven" => &mut self.icVGSGiven,
            "BSIM4icVBSGiven" => &mut self.icVBSGiven,
            "BSIM4trnqsModGiven" => &mut self.trnqsModGiven,
            "BSIM4acnqsModGiven" => &mut self.acnqsModGiven,
            "BSIM4rbodyModGiven" => &mut self.rbodyModGiven,
            "BSIM4rgateModGiven" => &mut self.rgateModGiven,
            "BSIM4geoModGiven" => &mut self.geoModGiven,
            "BSIM4rgeoModGiven" => &mut self.rgeoModGiven,
            _ => return None,
        })
    }
    /// C member names of the scalar members, in declaration order.
    #[allow(dead_code)]
    pub(crate) const MEMBERS: &[&str] = &[
        "BSIM4dNode",
        "BSIM4gNodeExt",
        "BSIM4sNode",
        "BSIM4bNode",
        "BSIM4dNodePrime",
        "BSIM4gNodePrime",
        "BSIM4gNodeMid",
        "BSIM4sNodePrime",
        "BSIM4bNodePrime",
        "BSIM4dbNode",
        "BSIM4sbNode",
        "BSIM4qNode",
        "BSIM4ueff",
        "BSIM4thetavth",
        "BSIM4von",
        "BSIM4vdsat",
        "BSIM4cgdo",
        "BSIM4qgdo",
        "BSIM4cgso",
        "BSIM4qgso",
        "BSIM4grbsb",
        "BSIM4grbdb",
        "BSIM4grbpb",
        "BSIM4grbps",
        "BSIM4grbpd",
        "BSIM4vjsmFwd",
        "BSIM4vjsmRev",
        "BSIM4vjdmFwd",
        "BSIM4vjdmRev",
        "BSIM4XExpBVS",
        "BSIM4XExpBVD",
        "BSIM4SslpFwd",
        "BSIM4SslpRev",
        "BSIM4DslpFwd",
        "BSIM4DslpRev",
        "BSIM4IVjsmFwd",
        "BSIM4IVjsmRev",
        "BSIM4IVjdmFwd",
        "BSIM4IVjdmRev",
        "BSIM4grgeltd",
        "BSIM4Pseff",
        "BSIM4Pdeff",
        "BSIM4Aseff",
        "BSIM4Adeff",
        "BSIM4l",
        "BSIM4w",
        "BSIM4m",
        "BSIM4drainArea",
        "BSIM4sourceArea",
        "BSIM4drainSquares",
        "BSIM4sourceSquares",
        "BSIM4drainPerimeter",
        "BSIM4sourcePerimeter",
        "BSIM4sourceConductance",
        "BSIM4drainConductance",
        "BSIM4sa",
        "BSIM4sb",
        "BSIM4sd",
        "BSIM4sca",
        "BSIM4scb",
        "BSIM4scc",
        "BSIM4sc",
        "BSIM4rbdb",
        "BSIM4rbsb",
        "BSIM4rbpb",
        "BSIM4rbps",
        "BSIM4rbpd",
        "BSIM4delvto",
        "BSIM4mulu0",
        "BSIM4wnflag",
        "BSIM4xgw",
        "BSIM4ngcon",
        "BSIM4dtemp",
        "BSIM4u0temp",
        "BSIM4vsattemp",
        "BSIM4vth0",
        "BSIM4vfb",
        "BSIM4vfbzb",
        "BSIM4vtfbphi1",
        "BSIM4vtfbphi2",
        "BSIM4k2",
        "BSIM4vbsc",
        "BSIM4k2ox",
        "BSIM4eta0",
        "BSIM4toxp",
        "BSIM4coxp",
        "BSIM4icVDS",
        "BSIM4icVGS",
        "BSIM4icVBS",
        "BSIM4mult_i",
        "BSIM4mult_q",
        "BSIM4mult_fn",
        "BSIM4nf",
        "BSIM4off",
        "BSIM4mode",
        "BSIM4trnqsMod",
        "BSIM4acnqsMod",
        "BSIM4rbodyMod",
        "BSIM4rgateMod",
        "BSIM4geoMod",
        "BSIM4rgeoMod",
        "BSIM4min",
        "BSIM4Vgsteff",
        "BSIM4vgs_eff",
        "BSIM4vgd_eff",
        "BSIM4dvgs_eff_dvg",
        "BSIM4dvgd_eff_dvg",
        "BSIM4Vdseff",
        "BSIM4nstar",
        "BSIM4Abulk",
        "BSIM4EsatL",
        "BSIM4AbovVgst2Vtm",
        "BSIM4qinv",
        "BSIM4cd",
        "BSIM4cbs",
        "BSIM4cbd",
        "BSIM4csub",
        "BSIM4Igidl",
        "BSIM4Igisl",
        "BSIM4gm",
        "BSIM4gds",
        "BSIM4gmbs",
        "BSIM4gbd",
        "BSIM4gbs",
        "BSIM4noiGd0",
        "BSIM4Coxeff",
        "BSIM4gbbs",
        "BSIM4gbgs",
        "BSIM4gbds",
        "BSIM4ggidld",
        "BSIM4ggidlg",
        "BSIM4ggidls",
        "BSIM4ggidlb",
        "BSIM4ggisld",
        "BSIM4ggislg",
        "BSIM4ggisls",
        "BSIM4ggislb",
        "BSIM4Igcs",
        "BSIM4gIgcsg",
        "BSIM4gIgcsd",
        "BSIM4gIgcss",
        "BSIM4gIgcsb",
        "BSIM4Igcd",
        "BSIM4gIgcdg",
        "BSIM4gIgcdd",
        "BSIM4gIgcds",
        "BSIM4gIgcdb",
        "BSIM4Igs",
        "BSIM4gIgsg",
        "BSIM4gIgss",
        "BSIM4Igd",
        "BSIM4gIgdg",
        "BSIM4gIgdd",
        "BSIM4Igb",
        "BSIM4gIgbg",
        "BSIM4gIgbd",
        "BSIM4gIgbs",
        "BSIM4gIgbb",
        "BSIM4grdsw",
        "BSIM4IdovVds",
        "BSIM4gcrg",
        "BSIM4gcrgd",
        "BSIM4gcrgg",
        "BSIM4gcrgs",
        "BSIM4gcrgb",
        "BSIM4gstot",
        "BSIM4gstotd",
        "BSIM4gstotg",
        "BSIM4gstots",
        "BSIM4gstotb",
        "BSIM4gdtot",
        "BSIM4gdtotd",
        "BSIM4gdtotg",
        "BSIM4gdtots",
        "BSIM4gdtotb",
        "BSIM4cggb",
        "BSIM4cgdb",
        "BSIM4cgsb",
        "BSIM4cbgb",
        "BSIM4cbdb",
        "BSIM4cbsb",
        "BSIM4cdgb",
        "BSIM4cddb",
        "BSIM4cdsb",
        "BSIM4csgb",
        "BSIM4csdb",
        "BSIM4cssb",
        "BSIM4cgbb",
        "BSIM4cdbb",
        "BSIM4csbb",
        "BSIM4cbbb",
        "BSIM4capbd",
        "BSIM4capbs",
        "BSIM4cqgb",
        "BSIM4cqdb",
        "BSIM4cqsb",
        "BSIM4cqbb",
        "BSIM4qgate",
        "BSIM4qbulk",
        "BSIM4qdrn",
        "BSIM4qsrc",
        "BSIM4qdef",
        "BSIM4qchqs",
        "BSIM4taunet",
        "BSIM4gtau",
        "BSIM4gtg",
        "BSIM4gtd",
        "BSIM4gts",
        "BSIM4gtb",
        "BSIM4SjctTempRevSatCur",
        "BSIM4DjctTempRevSatCur",
        "BSIM4SswTempRevSatCur",
        "BSIM4DswTempRevSatCur",
        "BSIM4SswgTempRevSatCur",
        "BSIM4DswgTempRevSatCur",
        "BSIM4lGiven",
        "BSIM4wGiven",
        "BSIM4mGiven",
        "BSIM4mult_iGiven",
        "BSIM4mult_qGiven",
        "BSIM4mult_fnGiven",
        "BSIM4nfGiven",
        "BSIM4minGiven",
        "BSIM4drainAreaGiven",
        "BSIM4sourceAreaGiven",
        "BSIM4drainSquaresGiven",
        "BSIM4sourceSquaresGiven",
        "BSIM4drainPerimeterGiven",
        "BSIM4sourcePerimeterGiven",
        "BSIM4saGiven",
        "BSIM4sbGiven",
        "BSIM4sdGiven",
        "BSIM4scaGiven",
        "BSIM4scbGiven",
        "BSIM4sccGiven",
        "BSIM4scGiven",
        "BSIM4rbdbGiven",
        "BSIM4rbsbGiven",
        "BSIM4rbpbGiven",
        "BSIM4rbpdGiven",
        "BSIM4rbpsGiven",
        "BSIM4delvtoGiven",
        "BSIM4mulu0Given",
        "BSIM4wnflagGiven",
        "BSIM4xgwGiven",
        "BSIM4ngconGiven",
        "BSIM4dtempGiven",
        "BSIM4icVDSGiven",
        "BSIM4icVGSGiven",
        "BSIM4icVBSGiven",
        "BSIM4trnqsModGiven",
        "BSIM4acnqsModGiven",
        "BSIM4rbodyModGiven",
        "BSIM4rgateModGiven",
        "BSIM4geoModGiven",
        "BSIM4rgeoModGiven",
        "BSIM4rhsdPrime",
        "BSIM4rhsgPrime",
        "BSIM4rhsgExt",
        "BSIM4grhsMid",
        "BSIM4rhsbPrime",
        "BSIM4rhssPrime",
        "BSIM4rhsdb",
        "BSIM4rhssb",
        "BSIM4rhsd",
        "BSIM4rhss",
        "BSIM4rhsq",
        "BSIM4_1",
        "BSIM4_2",
        "BSIM4_3",
        "BSIM4_4",
        "BSIM4_5",
        "BSIM4_6",
        "BSIM4_7",
        "BSIM4_8",
        "BSIM4_9",
        "BSIM4_10",
        "BSIM4_11",
        "BSIM4_12",
        "BSIM4_13",
        "BSIM4_14",
        "BSIM4_15",
        "BSIM4_16",
        "BSIM4_17",
        "BSIM4_18",
        "BSIM4_19",
        "BSIM4_20",
        "BSIM4_21",
        "BSIM4_22",
        "BSIM4_23",
        "BSIM4_24",
        "BSIM4_25",
        "BSIM4_26",
        "BSIM4_27",
        "BSIM4_28",
        "BSIM4_29",
        "BSIM4_30",
        "BSIM4_31",
        "BSIM4_32",
        "BSIM4_33",
        "BSIM4_34",
        "BSIM4_35",
        "BSIM4_36",
        "BSIM4_37",
        "BSIM4_38",
        "BSIM4_39",
        "BSIM4_40",
        "BSIM4_41",
        "BSIM4_42",
        "BSIM4_43",
        "BSIM4_44",
        "BSIM4_45",
        "BSIM4_46",
        "BSIM4_47",
        "BSIM4_48",
        "BSIM4_49",
        "BSIM4_50",
        "BSIM4_51",
        "BSIM4_52",
        "BSIM4_53",
        "BSIM4_54",
        "BSIM4_55",
        "BSIM4_56",
        "BSIM4_57",
        "BSIM4_58",
        "BSIM4_59",
        "BSIM4_60",
        "BSIM4_61",
        "BSIM4_62",
        "BSIM4_63",
        "BSIM4_64",
        "BSIM4_65",
        "BSIM4_66",
        "BSIM4_67",
        "BSIM4_68",
        "BSIM4_69",
        "BSIM4_70",
        "BSIM4_71",
        "BSIM4_72",
        "BSIM4_73",
        "BSIM4_74",
        "BSIM4_75",
        "BSIM4_76",
        "BSIM4_77",
        "BSIM4_78",
        "BSIM4_79",
        "BSIM4_80",
        "BSIM4_81",
        "BSIM4_82",
        "BSIM4_83",
        "BSIM4_84",
        "BSIM4_85",
        "BSIM4_86",
        "BSIM4_87",
        "BSIM4_88",
        "BSIM4_89",
        "BSIM4_90",
        "BSIM4_91",
        "BSIM4_92",
        "BSIM4_93",
        "BSIM4_94",
        "BSIM4_95",
        "BSIM4_96",
        "BSIM4_97",
        "BSIM4_98",
        "BSIM4_99",
        "BSIM4_100",
        "BSIM4_101",
        "BSIM4_102",
        "BSIM4_103",
    ];

    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,
    /// a given bit as 0 or 1).
    #[allow(dead_code)]
    pub(crate) fn to_values(&self) -> Vec<f64> {
        vec![
            f64::from(self.dNode),
            f64::from(self.gNodeExt),
            f64::from(self.sNode),
            f64::from(self.bNode),
            f64::from(self.dNodePrime),
            f64::from(self.gNodePrime),
            f64::from(self.gNodeMid),
            f64::from(self.sNodePrime),
            f64::from(self.bNodePrime),
            f64::from(self.dbNode),
            f64::from(self.sbNode),
            f64::from(self.qNode),
            self.ueff,
            self.thetavth,
            self.von,
            self.vdsat,
            self.cgdo,
            self.qgdo,
            self.cgso,
            self.qgso,
            self.grbsb,
            self.grbdb,
            self.grbpb,
            self.grbps,
            self.grbpd,
            self.vjsmFwd,
            self.vjsmRev,
            self.vjdmFwd,
            self.vjdmRev,
            self.XExpBVS,
            self.XExpBVD,
            self.SslpFwd,
            self.SslpRev,
            self.DslpFwd,
            self.DslpRev,
            self.IVjsmFwd,
            self.IVjsmRev,
            self.IVjdmFwd,
            self.IVjdmRev,
            self.grgeltd,
            self.Pseff,
            self.Pdeff,
            self.Aseff,
            self.Adeff,
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
            self.sa,
            self.sb,
            self.sd,
            self.sca,
            self.scb,
            self.scc,
            self.sc,
            self.rbdb,
            self.rbsb,
            self.rbpb,
            self.rbps,
            self.rbpd,
            self.delvto,
            self.mulu0,
            f64::from(self.wnflag),
            self.xgw,
            self.ngcon,
            self.dtemp,
            self.u0temp,
            self.vsattemp,
            self.vth0,
            self.vfb,
            self.vfbzb,
            self.vtfbphi1,
            self.vtfbphi2,
            self.k2,
            self.vbsc,
            self.k2ox,
            self.eta0,
            self.toxp,
            self.coxp,
            self.icVDS,
            self.icVGS,
            self.icVBS,
            self.mult_i,
            self.mult_q,
            self.mult_fn,
            self.nf,
            f64::from(self.off),
            f64::from(self.mode),
            f64::from(self.trnqsMod),
            f64::from(self.acnqsMod),
            f64::from(self.rbodyMod),
            f64::from(self.rgateMod),
            f64::from(self.geoMod),
            f64::from(self.rgeoMod),
            f64::from(self.min),
            self.Vgsteff,
            self.vgs_eff,
            self.vgd_eff,
            self.dvgs_eff_dvg,
            self.dvgd_eff_dvg,
            self.Vdseff,
            self.nstar,
            self.Abulk,
            self.EsatL,
            self.AbovVgst2Vtm,
            self.qinv,
            self.cd,
            self.cbs,
            self.cbd,
            self.csub,
            self.Igidl,
            self.Igisl,
            self.gm,
            self.gds,
            self.gmbs,
            self.gbd,
            self.gbs,
            self.noiGd0,
            self.Coxeff,
            self.gbbs,
            self.gbgs,
            self.gbds,
            self.ggidld,
            self.ggidlg,
            self.ggidls,
            self.ggidlb,
            self.ggisld,
            self.ggislg,
            self.ggisls,
            self.ggislb,
            self.Igcs,
            self.gIgcsg,
            self.gIgcsd,
            self.gIgcss,
            self.gIgcsb,
            self.Igcd,
            self.gIgcdg,
            self.gIgcdd,
            self.gIgcds,
            self.gIgcdb,
            self.Igs,
            self.gIgsg,
            self.gIgss,
            self.Igd,
            self.gIgdg,
            self.gIgdd,
            self.Igb,
            self.gIgbg,
            self.gIgbd,
            self.gIgbs,
            self.gIgbb,
            self.grdsw,
            self.IdovVds,
            self.gcrg,
            self.gcrgd,
            self.gcrgg,
            self.gcrgs,
            self.gcrgb,
            self.gstot,
            self.gstotd,
            self.gstotg,
            self.gstots,
            self.gstotb,
            self.gdtot,
            self.gdtotd,
            self.gdtotg,
            self.gdtots,
            self.gdtotb,
            self.cggb,
            self.cgdb,
            self.cgsb,
            self.cbgb,
            self.cbdb,
            self.cbsb,
            self.cdgb,
            self.cddb,
            self.cdsb,
            self.csgb,
            self.csdb,
            self.cssb,
            self.cgbb,
            self.cdbb,
            self.csbb,
            self.cbbb,
            self.capbd,
            self.capbs,
            self.cqgb,
            self.cqdb,
            self.cqsb,
            self.cqbb,
            self.qgate,
            self.qbulk,
            self.qdrn,
            self.qsrc,
            self.qdef,
            self.qchqs,
            self.taunet,
            self.gtau,
            self.gtg,
            self.gtd,
            self.gts,
            self.gtb,
            self.SjctTempRevSatCur,
            self.DjctTempRevSatCur,
            self.SswTempRevSatCur,
            self.DswTempRevSatCur,
            self.SswgTempRevSatCur,
            self.DswgTempRevSatCur,
            f64::from(u8::from(self.lGiven)),
            f64::from(u8::from(self.wGiven)),
            f64::from(u8::from(self.mGiven)),
            f64::from(u8::from(self.mult_iGiven)),
            f64::from(u8::from(self.mult_qGiven)),
            f64::from(u8::from(self.mult_fnGiven)),
            f64::from(u8::from(self.nfGiven)),
            f64::from(u8::from(self.minGiven)),
            f64::from(u8::from(self.drainAreaGiven)),
            f64::from(u8::from(self.sourceAreaGiven)),
            f64::from(u8::from(self.drainSquaresGiven)),
            f64::from(u8::from(self.sourceSquaresGiven)),
            f64::from(u8::from(self.drainPerimeterGiven)),
            f64::from(u8::from(self.sourcePerimeterGiven)),
            f64::from(u8::from(self.saGiven)),
            f64::from(u8::from(self.sbGiven)),
            f64::from(u8::from(self.sdGiven)),
            f64::from(u8::from(self.scaGiven)),
            f64::from(u8::from(self.scbGiven)),
            f64::from(u8::from(self.sccGiven)),
            f64::from(u8::from(self.scGiven)),
            f64::from(u8::from(self.rbdbGiven)),
            f64::from(u8::from(self.rbsbGiven)),
            f64::from(u8::from(self.rbpbGiven)),
            f64::from(u8::from(self.rbpdGiven)),
            f64::from(u8::from(self.rbpsGiven)),
            f64::from(u8::from(self.delvtoGiven)),
            f64::from(u8::from(self.mulu0Given)),
            f64::from(u8::from(self.wnflagGiven)),
            f64::from(u8::from(self.xgwGiven)),
            f64::from(u8::from(self.ngconGiven)),
            f64::from(u8::from(self.dtempGiven)),
            f64::from(u8::from(self.icVDSGiven)),
            f64::from(u8::from(self.icVGSGiven)),
            f64::from(u8::from(self.icVBSGiven)),
            f64::from(u8::from(self.trnqsModGiven)),
            f64::from(u8::from(self.acnqsModGiven)),
            f64::from(u8::from(self.rbodyModGiven)),
            f64::from(u8::from(self.rgateModGiven)),
            f64::from(u8::from(self.geoModGiven)),
            f64::from(u8::from(self.rgeoModGiven)),
            self.rhsdPrime,
            self.rhsgPrime,
            self.rhsgExt,
            self.grhsMid,
            self.rhsbPrime,
            self.rhssPrime,
            self.rhsdb,
            self.rhssb,
            self.rhsd,
            self.rhss,
            self.rhsq,
            self._1,
            self._2,
            self._3,
            self._4,
            self._5,
            self._6,
            self._7,
            self._8,
            self._9,
            self._10,
            self._11,
            self._12,
            self._13,
            self._14,
            self._15,
            self._16,
            self._17,
            self._18,
            self._19,
            self._20,
            self._21,
            self._22,
            self._23,
            self._24,
            self._25,
            self._26,
            self._27,
            self._28,
            self._29,
            self._30,
            self._31,
            self._32,
            self._33,
            self._34,
            self._35,
            self._36,
            self._37,
            self._38,
            self._39,
            self._40,
            self._41,
            self._42,
            self._43,
            self._44,
            self._45,
            self._46,
            self._47,
            self._48,
            self._49,
            self._50,
            self._51,
            self._52,
            self._53,
            self._54,
            self._55,
            self._56,
            self._57,
            self._58,
            self._59,
            self._60,
            self._61,
            self._62,
            self._63,
            self._64,
            self._65,
            self._66,
            self._67,
            self._68,
            self._69,
            self._70,
            self._71,
            self._72,
            self._73,
            self._74,
            self._75,
            self._76,
            self._77,
            self._78,
            self._79,
            self._80,
            self._81,
            self._82,
            self._83,
            self._84,
            self._85,
            self._86,
            self._87,
            self._88,
            self._89,
            self._90,
            self._91,
            self._92,
            self._93,
            self._94,
            self._95,
            self._96,
            self._97,
            self._98,
            self._99,
            self._100,
            self._101,
            self._102,
            self._103,
        ]
    }

    /// Stores numbers produced by [`Self::to_values`]; `values` must have
    /// [`Self::MEMBERS`]`.len()` entries.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub(crate) fn set_values(&mut self, values: &[f64]) {
        self.dNode = values[0] as i32;
        self.gNodeExt = values[1] as i32;
        self.sNode = values[2] as i32;
        self.bNode = values[3] as i32;
        self.dNodePrime = values[4] as i32;
        self.gNodePrime = values[5] as i32;
        self.gNodeMid = values[6] as i32;
        self.sNodePrime = values[7] as i32;
        self.bNodePrime = values[8] as i32;
        self.dbNode = values[9] as i32;
        self.sbNode = values[10] as i32;
        self.qNode = values[11] as i32;
        self.ueff = values[12];
        self.thetavth = values[13];
        self.von = values[14];
        self.vdsat = values[15];
        self.cgdo = values[16];
        self.qgdo = values[17];
        self.cgso = values[18];
        self.qgso = values[19];
        self.grbsb = values[20];
        self.grbdb = values[21];
        self.grbpb = values[22];
        self.grbps = values[23];
        self.grbpd = values[24];
        self.vjsmFwd = values[25];
        self.vjsmRev = values[26];
        self.vjdmFwd = values[27];
        self.vjdmRev = values[28];
        self.XExpBVS = values[29];
        self.XExpBVD = values[30];
        self.SslpFwd = values[31];
        self.SslpRev = values[32];
        self.DslpFwd = values[33];
        self.DslpRev = values[34];
        self.IVjsmFwd = values[35];
        self.IVjsmRev = values[36];
        self.IVjdmFwd = values[37];
        self.IVjdmRev = values[38];
        self.grgeltd = values[39];
        self.Pseff = values[40];
        self.Pdeff = values[41];
        self.Aseff = values[42];
        self.Adeff = values[43];
        self.l = values[44];
        self.w = values[45];
        self.m = values[46];
        self.drainArea = values[47];
        self.sourceArea = values[48];
        self.drainSquares = values[49];
        self.sourceSquares = values[50];
        self.drainPerimeter = values[51];
        self.sourcePerimeter = values[52];
        self.sourceConductance = values[53];
        self.drainConductance = values[54];
        self.sa = values[55];
        self.sb = values[56];
        self.sd = values[57];
        self.sca = values[58];
        self.scb = values[59];
        self.scc = values[60];
        self.sc = values[61];
        self.rbdb = values[62];
        self.rbsb = values[63];
        self.rbpb = values[64];
        self.rbps = values[65];
        self.rbpd = values[66];
        self.delvto = values[67];
        self.mulu0 = values[68];
        self.wnflag = values[69] as i32;
        self.xgw = values[70];
        self.ngcon = values[71];
        self.dtemp = values[72];
        self.u0temp = values[73];
        self.vsattemp = values[74];
        self.vth0 = values[75];
        self.vfb = values[76];
        self.vfbzb = values[77];
        self.vtfbphi1 = values[78];
        self.vtfbphi2 = values[79];
        self.k2 = values[80];
        self.vbsc = values[81];
        self.k2ox = values[82];
        self.eta0 = values[83];
        self.toxp = values[84];
        self.coxp = values[85];
        self.icVDS = values[86];
        self.icVGS = values[87];
        self.icVBS = values[88];
        self.mult_i = values[89];
        self.mult_q = values[90];
        self.mult_fn = values[91];
        self.nf = values[92];
        self.off = values[93] as i32;
        self.mode = values[94] as i32;
        self.trnqsMod = values[95] as i32;
        self.acnqsMod = values[96] as i32;
        self.rbodyMod = values[97] as i32;
        self.rgateMod = values[98] as i32;
        self.geoMod = values[99] as i32;
        self.rgeoMod = values[100] as i32;
        self.min = values[101] as i32;
        self.Vgsteff = values[102];
        self.vgs_eff = values[103];
        self.vgd_eff = values[104];
        self.dvgs_eff_dvg = values[105];
        self.dvgd_eff_dvg = values[106];
        self.Vdseff = values[107];
        self.nstar = values[108];
        self.Abulk = values[109];
        self.EsatL = values[110];
        self.AbovVgst2Vtm = values[111];
        self.qinv = values[112];
        self.cd = values[113];
        self.cbs = values[114];
        self.cbd = values[115];
        self.csub = values[116];
        self.Igidl = values[117];
        self.Igisl = values[118];
        self.gm = values[119];
        self.gds = values[120];
        self.gmbs = values[121];
        self.gbd = values[122];
        self.gbs = values[123];
        self.noiGd0 = values[124];
        self.Coxeff = values[125];
        self.gbbs = values[126];
        self.gbgs = values[127];
        self.gbds = values[128];
        self.ggidld = values[129];
        self.ggidlg = values[130];
        self.ggidls = values[131];
        self.ggidlb = values[132];
        self.ggisld = values[133];
        self.ggislg = values[134];
        self.ggisls = values[135];
        self.ggislb = values[136];
        self.Igcs = values[137];
        self.gIgcsg = values[138];
        self.gIgcsd = values[139];
        self.gIgcss = values[140];
        self.gIgcsb = values[141];
        self.Igcd = values[142];
        self.gIgcdg = values[143];
        self.gIgcdd = values[144];
        self.gIgcds = values[145];
        self.gIgcdb = values[146];
        self.Igs = values[147];
        self.gIgsg = values[148];
        self.gIgss = values[149];
        self.Igd = values[150];
        self.gIgdg = values[151];
        self.gIgdd = values[152];
        self.Igb = values[153];
        self.gIgbg = values[154];
        self.gIgbd = values[155];
        self.gIgbs = values[156];
        self.gIgbb = values[157];
        self.grdsw = values[158];
        self.IdovVds = values[159];
        self.gcrg = values[160];
        self.gcrgd = values[161];
        self.gcrgg = values[162];
        self.gcrgs = values[163];
        self.gcrgb = values[164];
        self.gstot = values[165];
        self.gstotd = values[166];
        self.gstotg = values[167];
        self.gstots = values[168];
        self.gstotb = values[169];
        self.gdtot = values[170];
        self.gdtotd = values[171];
        self.gdtotg = values[172];
        self.gdtots = values[173];
        self.gdtotb = values[174];
        self.cggb = values[175];
        self.cgdb = values[176];
        self.cgsb = values[177];
        self.cbgb = values[178];
        self.cbdb = values[179];
        self.cbsb = values[180];
        self.cdgb = values[181];
        self.cddb = values[182];
        self.cdsb = values[183];
        self.csgb = values[184];
        self.csdb = values[185];
        self.cssb = values[186];
        self.cgbb = values[187];
        self.cdbb = values[188];
        self.csbb = values[189];
        self.cbbb = values[190];
        self.capbd = values[191];
        self.capbs = values[192];
        self.cqgb = values[193];
        self.cqdb = values[194];
        self.cqsb = values[195];
        self.cqbb = values[196];
        self.qgate = values[197];
        self.qbulk = values[198];
        self.qdrn = values[199];
        self.qsrc = values[200];
        self.qdef = values[201];
        self.qchqs = values[202];
        self.taunet = values[203];
        self.gtau = values[204];
        self.gtg = values[205];
        self.gtd = values[206];
        self.gts = values[207];
        self.gtb = values[208];
        self.SjctTempRevSatCur = values[209];
        self.DjctTempRevSatCur = values[210];
        self.SswTempRevSatCur = values[211];
        self.DswTempRevSatCur = values[212];
        self.SswgTempRevSatCur = values[213];
        self.DswgTempRevSatCur = values[214];
        self.lGiven = values[215] != 0.0;
        self.wGiven = values[216] != 0.0;
        self.mGiven = values[217] != 0.0;
        self.mult_iGiven = values[218] != 0.0;
        self.mult_qGiven = values[219] != 0.0;
        self.mult_fnGiven = values[220] != 0.0;
        self.nfGiven = values[221] != 0.0;
        self.minGiven = values[222] != 0.0;
        self.drainAreaGiven = values[223] != 0.0;
        self.sourceAreaGiven = values[224] != 0.0;
        self.drainSquaresGiven = values[225] != 0.0;
        self.sourceSquaresGiven = values[226] != 0.0;
        self.drainPerimeterGiven = values[227] != 0.0;
        self.sourcePerimeterGiven = values[228] != 0.0;
        self.saGiven = values[229] != 0.0;
        self.sbGiven = values[230] != 0.0;
        self.sdGiven = values[231] != 0.0;
        self.scaGiven = values[232] != 0.0;
        self.scbGiven = values[233] != 0.0;
        self.sccGiven = values[234] != 0.0;
        self.scGiven = values[235] != 0.0;
        self.rbdbGiven = values[236] != 0.0;
        self.rbsbGiven = values[237] != 0.0;
        self.rbpbGiven = values[238] != 0.0;
        self.rbpdGiven = values[239] != 0.0;
        self.rbpsGiven = values[240] != 0.0;
        self.delvtoGiven = values[241] != 0.0;
        self.mulu0Given = values[242] != 0.0;
        self.wnflagGiven = values[243] != 0.0;
        self.xgwGiven = values[244] != 0.0;
        self.ngconGiven = values[245] != 0.0;
        self.dtempGiven = values[246] != 0.0;
        self.icVDSGiven = values[247] != 0.0;
        self.icVGSGiven = values[248] != 0.0;
        self.icVBSGiven = values[249] != 0.0;
        self.trnqsModGiven = values[250] != 0.0;
        self.acnqsModGiven = values[251] != 0.0;
        self.rbodyModGiven = values[252] != 0.0;
        self.rgateModGiven = values[253] != 0.0;
        self.geoModGiven = values[254] != 0.0;
        self.rgeoModGiven = values[255] != 0.0;
        self.rhsdPrime = values[256];
        self.rhsgPrime = values[257];
        self.rhsgExt = values[258];
        self.grhsMid = values[259];
        self.rhsbPrime = values[260];
        self.rhssPrime = values[261];
        self.rhsdb = values[262];
        self.rhssb = values[263];
        self.rhsd = values[264];
        self.rhss = values[265];
        self.rhsq = values[266];
        self._1 = values[267];
        self._2 = values[268];
        self._3 = values[269];
        self._4 = values[270];
        self._5 = values[271];
        self._6 = values[272];
        self._7 = values[273];
        self._8 = values[274];
        self._9 = values[275];
        self._10 = values[276];
        self._11 = values[277];
        self._12 = values[278];
        self._13 = values[279];
        self._14 = values[280];
        self._15 = values[281];
        self._16 = values[282];
        self._17 = values[283];
        self._18 = values[284];
        self._19 = values[285];
        self._20 = values[286];
        self._21 = values[287];
        self._22 = values[288];
        self._23 = values[289];
        self._24 = values[290];
        self._25 = values[291];
        self._26 = values[292];
        self._27 = values[293];
        self._28 = values[294];
        self._29 = values[295];
        self._30 = values[296];
        self._31 = values[297];
        self._32 = values[298];
        self._33 = values[299];
        self._34 = values[300];
        self._35 = values[301];
        self._36 = values[302];
        self._37 = values[303];
        self._38 = values[304];
        self._39 = values[305];
        self._40 = values[306];
        self._41 = values[307];
        self._42 = values[308];
        self._43 = values[309];
        self._44 = values[310];
        self._45 = values[311];
        self._46 = values[312];
        self._47 = values[313];
        self._48 = values[314];
        self._49 = values[315];
        self._50 = values[316];
        self._51 = values[317];
        self._52 = values[318];
        self._53 = values[319];
        self._54 = values[320];
        self._55 = values[321];
        self._56 = values[322];
        self._57 = values[323];
        self._58 = values[324];
        self._59 = values[325];
        self._60 = values[326];
        self._61 = values[327];
        self._62 = values[328];
        self._63 = values[329];
        self._64 = values[330];
        self._65 = values[331];
        self._66 = values[332];
        self._67 = values[333];
        self._68 = values[334];
        self._69 = values[335];
        self._70 = values[336];
        self._71 = values[337];
        self._72 = values[338];
        self._73 = values[339];
        self._74 = values[340];
        self._75 = values[341];
        self._76 = values[342];
        self._77 = values[343];
        self._78 = values[344];
        self._79 = values[345];
        self._80 = values[346];
        self._81 = values[347];
        self._82 = values[348];
        self._83 = values[349];
        self._84 = values[350];
        self._85 = values[351];
        self._86 = values[352];
        self._87 = values[353];
        self._88 = values[354];
        self._89 = values[355];
        self._90 = values[356];
        self._91 = values[357];
        self._92 = values[358];
        self._93 = values[359];
        self._94 = values[360];
        self._95 = values[361];
        self._96 = values[362];
        self._97 = values[363];
        self._98 = values[364];
        self._99 = values[365];
        self._100 = values[366];
        self._101 = values[367];
        self._102 = values[368];
        self._103 = values[369];
    }
}
