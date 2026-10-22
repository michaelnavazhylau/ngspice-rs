//! `b3pzld.c` (`BSIM3pzLoad`): the pole-zero load, followed statement by
//! statement over C's struct members.
//!
//! C adds `m * (x * s)` as `*(ptr) += m * (x * s->real); *(ptr + 1) +=
//! m * (x * s->imag)`; the port passes `m * x` to [`SmallSignalCkt::cap`],
//! the coefficient of `s` in the pencil `A + s E`, which rounds as C does for
//! the multiplicity `m = 1` (the default) and may differ in the last bit
//! otherwise.
#![allow(non_snake_case)] // C's member and local names.
#![allow(unused_assignments, unused_variables, unused_mut)] // C assigns locals that some paths never read.
#![allow(unused_parens)] // C's parenthesisation is kept verbatim.
#![allow(clippy::double_parens)] // C's parenthesisation is kept verbatim.
#![allow(clippy::excessive_precision)] // C's literals.
#![allow(clippy::float_cmp)] // C compares floating point exactly.
#![allow(clippy::nonminimal_bool)] // C's conditions.
#![allow(clippy::too_many_lines)] // One C function.
#![allow(clippy::neg_multiply)] // C's `-1 * x` forms.
#![allow(clippy::precedence)] // C's expressions are kept verbatim.

use super::fields::{Instance, Model, SizeParam};
use super::host::{SmallSignalCkt, state};
use crate::primitives::SpiceResult;

fn fabs(x: f64) -> f64 {
    x.abs()
}

/// `BSIM3pzLoad` for one instance, from the values the `MODEINITSMSIG` load
/// left on `here`.
///
/// # Errors
/// None: C's `BSIM3pzLoad` always returns `OK`.
#[allow(clippy::unnecessary_wraps)] // Uniform stage signature.
pub(crate) fn pz_load(
    model: &Model,
    here: &Instance,
    pParam: &SizeParam,
    ckt: &mut dyn SmallSignalCkt,
) -> SpiceResult<()> {
    let mut xcggb: f64 = 0.0;
    let mut xcgdb: f64 = 0.0;
    let mut xcgsb: f64 = 0.0;
    let mut xcgbb: f64 = 0.0;
    let mut xcbgb: f64 = 0.0;
    let mut xcbdb: f64 = 0.0;
    let mut xcbsb: f64 = 0.0;
    let mut xcbbb: f64 = 0.0;
    let mut xcdgb: f64 = 0.0;
    let mut xcddb: f64 = 0.0;
    let mut xcdsb: f64 = 0.0;
    let mut xcdbb: f64 = 0.0;
    let mut xcsgb: f64 = 0.0;
    let mut xcsdb: f64 = 0.0;
    let mut xcssb: f64 = 0.0;
    let mut xcsbb: f64 = 0.0;
    let mut gdpr: f64 = 0.0;
    let mut gspr: f64 = 0.0;
    let mut gds: f64 = 0.0;
    let mut gbd: f64 = 0.0;
    let mut gbs: f64 = 0.0;
    let mut capbd: f64 = 0.0;
    let mut capbs: f64 = 0.0;
    let mut FwdSum: f64 = 0.0;
    let mut RevSum: f64 = 0.0;
    let mut Gm: f64 = 0.0;
    let mut Gmbs: f64 = 0.0;
    let mut cggb: f64 = 0.0;
    let mut cgdb: f64 = 0.0;
    let mut cgsb: f64 = 0.0;
    let mut cbgb: f64 = 0.0;
    let mut cbdb: f64 = 0.0;
    let mut cbsb: f64 = 0.0;
    let mut cddb: f64 = 0.0;
    let mut cdgb: f64 = 0.0;
    let mut cdsb: f64 = 0.0;
    let mut GSoverlapCap: f64 = 0.0;
    let mut GDoverlapCap: f64 = 0.0;
    let mut GBoverlapCap: f64 = 0.0;
    let mut dxpart: f64 = 0.0;
    let mut sxpart: f64 = 0.0;
    let mut xgtg: f64 = 0.0;
    let mut xgtd: f64 = 0.0;
    let mut xgts: f64 = 0.0;
    let mut xgtb: f64 = 0.0;
    let mut xcqgb: f64 = 0.0;
    let mut xcqdb: f64 = 0.0;
    let mut xcqsb: f64 = 0.0;
    let mut xcqbb: f64 = 0.0;
    let mut gbspsp: f64 = 0.0;
    let mut gbbdp: f64 = 0.0;
    let mut gbbsp: f64 = 0.0;
    let mut gbspg: f64 = 0.0;
    let mut gbspb: f64 = 0.0;
    let mut gbspdp: f64 = 0.0;
    let mut gbdpdp: f64 = 0.0;
    let mut gbdpg: f64 = 0.0;
    let mut gbdpb: f64 = 0.0;
    let mut gbdpsp: f64 = 0.0;
    let mut ddxpart_dVd: f64 = 0.0;
    let mut ddxpart_dVg: f64 = 0.0;
    let mut ddxpart_dVb: f64 = 0.0;
    let mut ddxpart_dVs: f64 = 0.0;
    let mut dsxpart_dVd: f64 = 0.0;
    let mut dsxpart_dVg: f64 = 0.0;
    let mut dsxpart_dVb: f64 = 0.0;
    let mut dsxpart_dVs: f64 = 0.0;
    let mut T1: f64 = 0.0;
    let mut CoxWL: f64 = 0.0;
    let mut qcheq: f64 = 0.0;
    let mut Cdg: f64 = 0.0;
    let mut Cdd: f64 = 0.0;
    let mut Cds: f64 = 0.0;
    let mut Csg: f64 = 0.0;
    let mut Csd: f64 = 0.0;
    let mut Css: f64 = 0.0;
    let mut ScalingFactor: f64 = 1.0e-9;
    let mut m: f64 = 0.0;
    if here.mode >= 0 {
        Gm = here.gm;
        Gmbs = here.gmbs;
        FwdSum = Gm + Gmbs;
        RevSum = 0.0;
        gbbdp = -here.gbds;
        gbbsp = here.gbds + here.gbgs + here.gbbs;
        gbdpg = here.gbgs;
        gbdpdp = here.gbds;
        gbdpb = here.gbbs;
        gbdpsp = -(gbdpg + gbdpdp + gbdpb);
        gbspg = 0.0;
        gbspdp = 0.0;
        gbspb = 0.0;
        gbspsp = 0.0;
        if here.nqsMod == 0 && here.acnqsMod == 0 {
            cggb = here.cggb;
            cgsb = here.cgsb;
            cgdb = here.cgdb;
            cbgb = here.cbgb;
            cbsb = here.cbsb;
            cbdb = here.cbdb;
            cdgb = here.cdgb;
            cdsb = here.cdsb;
            cddb = here.cddb;
            xgtb = 0.0;
            xgts = xgtb;
            xgtd = xgts;
            xgtg = xgtd;
            sxpart = 0.6;
            dxpart = 0.4;
            ddxpart_dVs = 0.0;
            ddxpart_dVb = ddxpart_dVs;
            ddxpart_dVg = ddxpart_dVb;
            ddxpart_dVd = ddxpart_dVg;
            dsxpart_dVs = 0.0;
            dsxpart_dVb = dsxpart_dVs;
            dsxpart_dVg = dsxpart_dVb;
            dsxpart_dVd = dsxpart_dVg;
        } else {
            cgsb = 0.0;
            cgdb = cgsb;
            cggb = cgdb;
            cbsb = 0.0;
            cbdb = cbsb;
            cbgb = cbdb;
            cdsb = 0.0;
            cddb = cdsb;
            cdgb = cddb;
            xgtg = here.gtg;
            xgtd = here.gtd;
            xgts = here.gts;
            xgtb = here.gtb;
            xcqgb = here.cqgb;
            xcqdb = here.cqdb;
            xcqsb = here.cqsb;
            xcqbb = here.cqbb;
            CoxWL = model.cox * pParam.weffCV * pParam.leffCV;
            qcheq = -(here.qgate + here.qbulk);
            if fabs(qcheq) <= 1.0e-5 * CoxWL {
                if model.xpart < 0.5 {
                    dxpart = 0.4;
                } else if model.xpart > 0.5 {
                    dxpart = 0.0;
                } else {
                    dxpart = 0.5;
                }
                ddxpart_dVs = 0.0;
                ddxpart_dVb = ddxpart_dVs;
                ddxpart_dVg = ddxpart_dVb;
                ddxpart_dVd = ddxpart_dVg;
            } else {
                dxpart = here.qdrn / qcheq;
                Cdd = here.cddb;
                Csd = -(here.cgdb + here.cddb + here.cbdb);
                ddxpart_dVd = (Cdd - dxpart * (Cdd + Csd)) / qcheq;
                Cdg = here.cdgb;
                Csg = -(here.cggb + here.cdgb + here.cbgb);
                ddxpart_dVg = (Cdg - dxpart * (Cdg + Csg)) / qcheq;
                Cds = here.cdsb;
                Css = -(here.cgsb + here.cdsb + here.cbsb);
                ddxpart_dVs = (Cds - dxpart * (Cds + Css)) / qcheq;
                ddxpart_dVb = -(ddxpart_dVd + ddxpart_dVg + ddxpart_dVs);
            }
            sxpart = 1.0 - dxpart;
            dsxpart_dVd = -ddxpart_dVd;
            dsxpart_dVg = -ddxpart_dVg;
            dsxpart_dVs = -ddxpart_dVs;
            dsxpart_dVb = -(dsxpart_dVd + dsxpart_dVg + dsxpart_dVs);
        }
    } else {
        Gm = -here.gm;
        Gmbs = -here.gmbs;
        FwdSum = 0.0;
        RevSum = -(Gm + Gmbs);
        gbbsp = -here.gbds;
        gbbdp = here.gbds + here.gbgs + here.gbbs;
        gbdpg = 0.0;
        gbdpsp = 0.0;
        gbdpb = 0.0;
        gbdpdp = 0.0;
        gbspg = here.gbgs;
        gbspsp = here.gbds;
        gbspb = here.gbbs;
        gbspdp = -(gbspg + gbspsp + gbspb);
        if here.nqsMod == 0 && here.acnqsMod == 0 {
            cggb = here.cggb;
            cgsb = here.cgdb;
            cgdb = here.cgsb;
            cbgb = here.cbgb;
            cbsb = here.cbdb;
            cbdb = here.cbsb;
            cdgb = -(here.cdgb + cggb + cbgb);
            cdsb = -(here.cddb + cgsb + cbsb);
            cddb = -(here.cdsb + cgdb + cbdb);
            xgtb = 0.0;
            xgts = xgtb;
            xgtd = xgts;
            xgtg = xgtd;
            sxpart = 0.4;
            dxpart = 0.6;
            ddxpart_dVs = 0.0;
            ddxpart_dVb = ddxpart_dVs;
            ddxpart_dVg = ddxpart_dVb;
            ddxpart_dVd = ddxpart_dVg;
            dsxpart_dVs = 0.0;
            dsxpart_dVb = dsxpart_dVs;
            dsxpart_dVg = dsxpart_dVb;
            dsxpart_dVd = dsxpart_dVg;
        } else {
            cgsb = 0.0;
            cgdb = cgsb;
            cggb = cgdb;
            cbsb = 0.0;
            cbdb = cbsb;
            cbgb = cbdb;
            cdsb = 0.0;
            cddb = cdsb;
            cdgb = cddb;
            xgtg = here.gtg;
            xgtd = here.gts;
            xgts = here.gtd;
            xgtb = here.gtb;
            xcqgb = here.cqgb;
            xcqdb = here.cqsb;
            xcqsb = here.cqdb;
            xcqbb = here.cqbb;
            CoxWL = model.cox * pParam.weffCV * pParam.leffCV;
            qcheq = -(here.qgate + here.qbulk);
            if fabs(qcheq) <= 1.0e-5 * CoxWL {
                if model.xpart < 0.5 {
                    sxpart = 0.4;
                } else if model.xpart > 0.5 {
                    sxpart = 0.0;
                } else {
                    sxpart = 0.5;
                }
                dsxpart_dVs = 0.0;
                dsxpart_dVb = dsxpart_dVs;
                dsxpart_dVg = dsxpart_dVb;
                dsxpart_dVd = dsxpart_dVg;
            } else {
                sxpart = here.qdrn / qcheq;
                Css = here.cddb;
                Cds = -(here.cgdb + here.cddb + here.cbdb);
                dsxpart_dVs = (Css - sxpart * (Css + Cds)) / qcheq;
                Csg = here.cdgb;
                Cdg = -(here.cggb + here.cdgb + here.cbgb);
                dsxpart_dVg = (Csg - sxpart * (Csg + Cdg)) / qcheq;
                Csd = here.cdsb;
                Cdd = -(here.cgsb + here.cdsb + here.cbsb);
                dsxpart_dVd = (Csd - sxpart * (Csd + Cdd)) / qcheq;
                dsxpart_dVb = -(dsxpart_dVd + dsxpart_dVg + dsxpart_dVs);
            }
            dxpart = 1.0 - sxpart;
            ddxpart_dVd = -dsxpart_dVd;
            ddxpart_dVg = -dsxpart_dVg;
            ddxpart_dVs = -dsxpart_dVs;
            ddxpart_dVb = -(ddxpart_dVd + ddxpart_dVg + ddxpart_dVs);
        }
    }
    T1 = ckt.state0(state::QDEF) * here.gtau;
    gdpr = here.drainConductance;
    gspr = here.sourceConductance;
    gds = here.gds;
    gbd = here.gbd;
    gbs = here.gbs;
    capbd = here.capbd;
    capbs = here.capbs;
    GSoverlapCap = here.cgso;
    GDoverlapCap = here.cgdo;
    GBoverlapCap = pParam.cgbo;
    xcdgb = (cdgb - GDoverlapCap);
    xcddb = (cddb + capbd + GDoverlapCap);
    xcdsb = cdsb;
    xcdbb = -(xcdgb + xcddb + xcdsb);
    xcsgb = -(cggb + cbgb + cdgb + GSoverlapCap);
    xcsdb = -(cgdb + cbdb + cddb);
    xcssb = (capbs + GSoverlapCap - (cgsb + cbsb + cdsb));
    xcsbb = -(xcsgb + xcsdb + xcssb);
    xcggb = (cggb + GDoverlapCap + GSoverlapCap + GBoverlapCap);
    xcgdb = (cgdb - GDoverlapCap);
    xcgsb = (cgsb - GSoverlapCap);
    xcgbb = -(xcggb + xcgdb + xcgsb);
    xcbgb = (cbgb - GBoverlapCap);
    xcbdb = (cbdb - capbd);
    xcbsb = (cbsb - capbs);
    xcbbb = -(xcbgb + xcbdb + xcbsb);
    m = here.m;
    ckt.cap(here.gNode, here.gNode, m * xcggb);
    ckt.cap(here.bNode, here.bNode, m * xcbbb);
    ckt.cap(here.dNodePrime, here.dNodePrime, m * xcddb);
    ckt.cap(here.sNodePrime, here.sNodePrime, m * xcssb);
    ckt.cap(here.gNode, here.bNode, m * xcgbb);
    ckt.cap(here.gNode, here.dNodePrime, m * xcgdb);
    ckt.cap(here.gNode, here.sNodePrime, m * xcgsb);
    ckt.cap(here.bNode, here.gNode, m * xcbgb);
    ckt.cap(here.bNode, here.dNodePrime, m * xcbdb);
    ckt.cap(here.bNode, here.sNodePrime, m * xcbsb);
    ckt.cap(here.dNodePrime, here.gNode, m * xcdgb);
    ckt.cap(here.dNodePrime, here.bNode, m * xcdbb);
    ckt.cap(here.dNodePrime, here.sNodePrime, m * xcdsb);
    ckt.cap(here.sNodePrime, here.gNode, m * xcsgb);
    ckt.cap(here.sNodePrime, here.bNode, m * xcsbb);
    ckt.cap(here.sNodePrime, here.dNodePrime, m * xcsdb);
    ckt.real(here.dNode, here.dNode, m * gdpr);
    ckt.real(here.dNode, here.dNodePrime, -(m * gdpr));
    ckt.real(here.dNodePrime, here.dNode, -(m * gdpr));
    ckt.real(here.sNode, here.sNode, m * gspr);
    ckt.real(here.sNode, here.sNodePrime, -(m * gspr));
    ckt.real(here.sNodePrime, here.sNode, -(m * gspr));
    ckt.real(here.bNode, here.gNode, -(m * here.gbgs));
    ckt.real(here.bNode, here.bNode, m * (gbd + gbs - here.gbbs));
    ckt.real(here.bNode, here.dNodePrime, -(m * (gbd - gbbdp)));
    ckt.real(here.bNode, here.sNodePrime, -(m * (gbs - gbbsp)));
    ckt.real(
        here.dNodePrime,
        here.gNode,
        m * (Gm + dxpart * xgtg + T1 * ddxpart_dVg + gbdpg),
    );
    ckt.real(
        here.dNodePrime,
        here.dNodePrime,
        m * (gdpr + gds + gbd + RevSum + dxpart * xgtd + T1 * ddxpart_dVd + gbdpdp),
    );
    ckt.real(
        here.dNodePrime,
        here.sNodePrime,
        -(m * (gds + FwdSum - dxpart * xgts - T1 * ddxpart_dVs - gbdpsp)),
    );
    ckt.real(
        here.dNodePrime,
        here.bNode,
        -(m * (gbd - Gmbs - dxpart * xgtb - T1 * ddxpart_dVb - gbdpb)),
    );
    ckt.real(
        here.sNodePrime,
        here.gNode,
        -(m * (Gm - sxpart * xgtg - T1 * dsxpart_dVg - gbspg)),
    );
    ckt.real(
        here.sNodePrime,
        here.sNodePrime,
        m * (gspr + gds + gbs + FwdSum + sxpart * xgts + T1 * dsxpart_dVs + gbspsp),
    );
    ckt.real(
        here.sNodePrime,
        here.bNode,
        -(m * (gbs + Gmbs - sxpart * xgtb - T1 * dsxpart_dVb - gbspb)),
    );
    ckt.real(
        here.sNodePrime,
        here.dNodePrime,
        -(m * (gds + RevSum - sxpart * xgtd - T1 * dsxpart_dVd - gbspdp)),
    );
    ckt.real(here.gNode, here.gNode, -(m * xgtg));
    ckt.real(here.gNode, here.bNode, -(m * xgtb));
    ckt.real(here.gNode, here.dNodePrime, -(m * xgtd));
    ckt.real(here.gNode, here.sNodePrime, -(m * xgts));
    if (here.nqsMod != 0) || (here.acnqsMod != 0) {
        ckt.cap(here.qNode, here.qNode, m * ScalingFactor);
        ckt.cap(here.qNode, here.gNode, -(m * xcqgb));
        ckt.cap(here.qNode, here.dNodePrime, -(m * xcqdb));
        ckt.cap(here.qNode, here.bNode, -(m * xcqbb));
        ckt.cap(here.qNode, here.sNodePrime, -(m * xcqsb));
        ckt.real(here.gNode, here.qNode, -(m * (here.gtau)));
        ckt.real(here.dNodePrime, here.qNode, m * (dxpart * here.gtau));
        ckt.real(here.sNodePrime, here.qNode, m * (sxpart * here.gtau));
        ckt.real(here.qNode, here.qNode, m * (here.gtau));
        ckt.real(here.qNode, here.gNode, m * xgtg);
        ckt.real(here.qNode, here.dNodePrime, m * xgtd);
        ckt.real(here.qNode, here.bNode, m * xgtb);
        ckt.real(here.qNode, here.sNodePrime, m * xgts);
    }
    // C label `stray-close-brace:`.
    // C label `stray-close-brace:`.
    Ok(())
}
