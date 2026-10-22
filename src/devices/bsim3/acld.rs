//! `b3acld.c` (`BSIM3acLoad`): the small-signal load at `CKTomega`,
//! followed statement by statement over C's struct members. C's `xc*` terms
//! already include `omega`; they go to the imaginary part unscaled.
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

/// `BSIM3acLoad` for one instance, from the values the `MODEINITSMSIG` load
/// left on `here`.
///
/// # Errors
/// None: C's `BSIM3acLoad` always returns `OK`.
#[allow(clippy::unnecessary_wraps)] // Uniform stage signature.
pub(crate) fn ac_load(
    model: &Model,
    here: &Instance,
    pParam: &SizeParam,
    ckt: &mut dyn SmallSignalCkt,
) -> SpiceResult<()> {
    let mut xcggb: f64 = 0.0;
    let mut xcgdb: f64 = 0.0;
    let mut xcgsb: f64 = 0.0;
    let mut xcbgb: f64 = 0.0;
    let mut xcbdb: f64 = 0.0;
    let mut xcbsb: f64 = 0.0;
    let mut xcddb: f64 = 0.0;
    let mut xcssb: f64 = 0.0;
    let mut xcdgb: f64 = 0.0;
    let mut gdpr: f64 = 0.0;
    let mut gspr: f64 = 0.0;
    let mut gds: f64 = 0.0;
    let mut gbd: f64 = 0.0;
    let mut gbs: f64 = 0.0;
    let mut capbd: f64 = 0.0;
    let mut capbs: f64 = 0.0;
    let mut xcsgb: f64 = 0.0;
    let mut xcdsb: f64 = 0.0;
    let mut xcsdb: f64 = 0.0;
    let mut cggb: f64 = 0.0;
    let mut cgdb: f64 = 0.0;
    let mut cgsb: f64 = 0.0;
    let mut cbgb: f64 = 0.0;
    let mut cbdb: f64 = 0.0;
    let mut cbsb: f64 = 0.0;
    let mut cddb: f64 = 0.0;
    let mut cdgb: f64 = 0.0;
    let mut cdsb: f64 = 0.0;
    let mut omega: f64 = 0.0;
    let mut GSoverlapCap: f64 = 0.0;
    let mut GDoverlapCap: f64 = 0.0;
    let mut GBoverlapCap: f64 = 0.0;
    let mut FwdSum: f64 = 0.0;
    let mut RevSum: f64 = 0.0;
    let mut Gm: f64 = 0.0;
    let mut Gmbs: f64 = 0.0;
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
    let mut T0: f64 = 0.0;
    let mut T2: f64 = 0.0;
    let mut T3: f64 = 0.0;
    let mut gmr: f64 = 0.0;
    let mut gmbsr: f64 = 0.0;
    let mut gmi: f64 = 0.0;
    let mut gmbsi: f64 = 0.0;
    let mut gdsi: f64 = 0.0;
    let mut Cddr: f64 = 0.0;
    let mut Cdgr: f64 = 0.0;
    let mut Cdsr: f64 = 0.0;
    let mut Csdr: f64 = 0.0;
    let mut Csgr: f64 = 0.0;
    let mut Cssr: f64 = 0.0;
    let mut Cgdr: f64 = 0.0;
    let mut Cggr: f64 = 0.0;
    let mut Cgsr: f64 = 0.0;
    let mut Cddi: f64 = 0.0;
    let mut Cdgi: f64 = 0.0;
    let mut Cdsi: f64 = 0.0;
    let mut Cdbi: f64 = 0.0;
    let mut Csdi: f64 = 0.0;
    let mut Csgi: f64 = 0.0;
    let mut Cssi: f64 = 0.0;
    let mut Csbi: f64 = 0.0;
    let mut Cgdi: f64 = 0.0;
    let mut Cggi: f64 = 0.0;
    let mut Cgsi: f64 = 0.0;
    let mut Cgbi: f64 = 0.0;
    let mut Gmi: f64 = 0.0;
    let mut Gmbsi: f64 = 0.0;
    let mut FwdSumi: f64 = 0.0;
    let mut RevSumi: f64 = 0.0;
    let mut xcdgbi: f64 = 0.0;
    let mut xcsgbi: f64 = 0.0;
    let mut xcddbi: f64 = 0.0;
    let mut xcdsbi: f64 = 0.0;
    let mut xcsdbi: f64 = 0.0;
    let mut xcssbi: f64 = 0.0;
    let mut xcdbbi: f64 = 0.0;
    let mut xcsbbi: f64 = 0.0;
    let mut xcggbi: f64 = 0.0;
    let mut xcgdbi: f64 = 0.0;
    let mut xcgsbi: f64 = 0.0;
    let mut xcgbbi: f64 = 0.0;
    let mut m: f64 = 0.0;
    omega = ckt.omega();
    Csd = -(here.cddb + here.cgdb + here.cbdb);
    Csg = -(here.cdgb + here.cggb + here.cbgb);
    Css = -(here.cdsb + here.cgsb + here.cbsb);
    if here.acnqsMod != 0 {
        T0 = omega * here.taunet;
        T1 = T0 * T0;
        T2 = 1.0 / (1.0 + T1);
        T3 = T0 * T2;
        gmr = here.gm * T2;
        gmbsr = here.gmbs * T2;
        gds = here.gds * T2;
        gmi = -here.gm * T3;
        gmbsi = -here.gmbs * T3;
        gdsi = -here.gds * T3;
        Cddr = here.cddb * T2;
        Cdgr = here.cdgb * T2;
        Cdsr = here.cdsb * T2;
        Cddi = here.cddb * T3 * omega;
        Cdgi = here.cdgb * T3 * omega;
        Cdsi = here.cdsb * T3 * omega;
        Cdbi = -(Cddi + Cdgi + Cdsi);
        Csdr = Csd * T2;
        Csgr = Csg * T2;
        Cssr = Css * T2;
        Csdi = Csd * T3 * omega;
        Csgi = Csg * T3 * omega;
        Cssi = Css * T3 * omega;
        Csbi = -(Csdi + Csgi + Cssi);
        Cgdr = -(Cddr + Csdr + here.cbdb);
        Cggr = -(Cdgr + Csgr + here.cbgb);
        Cgsr = -(Cdsr + Cssr + here.cbsb);
        Cgdi = -(Cddi + Csdi);
        Cggi = -(Cdgi + Csgi);
        Cgsi = -(Cdsi + Cssi);
        Cgbi = -(Cgdi + Cggi + Cgsi);
    } else {
        gmr = here.gm;
        gmbsr = here.gmbs;
        gds = here.gds;
        gdsi = 0.0;
        gmbsi = gdsi;
        gmi = gmbsi;
        Cddr = here.cddb;
        Cdgr = here.cdgb;
        Cdsr = here.cdsb;
        Cdbi = 0.0;
        Cdsi = Cdbi;
        Cdgi = Cdsi;
        Cddi = Cdgi;
        Csdr = Csd;
        Csgr = Csg;
        Cssr = Css;
        Csbi = 0.0;
        Cssi = Csbi;
        Csgi = Cssi;
        Csdi = Csgi;
        Cgdr = here.cgdb;
        Cggr = here.cggb;
        Cgsr = here.cgsb;
        Cgbi = 0.0;
        Cgsi = Cgbi;
        Cggi = Cgsi;
        Cgdi = Cggi;
    }
    if here.mode >= 0 {
        Gm = gmr;
        Gmbs = gmbsr;
        FwdSum = Gm + Gmbs;
        RevSum = 0.0;
        Gmi = gmi;
        Gmbsi = gmbsi;
        FwdSumi = Gmi + Gmbsi;
        RevSumi = 0.0;
        gbbdp = -here.gbds;
        gbbsp = here.gbds + here.gbgs + here.gbbs;
        gbdpg = here.gbgs;
        gbdpb = here.gbbs;
        gbdpdp = here.gbds;
        gbdpsp = -(gbdpg + gbdpb + gbdpdp);
        gbspdp = 0.0;
        gbspg = 0.0;
        gbspb = 0.0;
        gbspsp = 0.0;
        if here.nqsMod == 0 || here.acnqsMod == 1 {
            cggb = Cggr;
            cgsb = Cgsr;
            cgdb = Cgdr;
            cbgb = here.cbgb;
            cbsb = here.cbsb;
            cbdb = here.cbdb;
            cdgb = Cdgr;
            cdsb = Cdsr;
            cddb = Cddr;
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
            xcqgb = here.cqgb * omega;
            xcqdb = here.cqdb * omega;
            xcqsb = here.cqsb * omega;
            xcqbb = here.cqbb * omega;
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
        xcdgbi = Cdgi;
        xcsgbi = Csgi;
        xcddbi = Cddi;
        xcdsbi = Cdsi;
        xcsdbi = Csdi;
        xcssbi = Cssi;
        xcdbbi = Cdbi;
        xcsbbi = Csbi;
        xcggbi = Cggi;
        xcgdbi = Cgdi;
        xcgsbi = Cgsi;
        xcgbbi = Cgbi;
    } else {
        Gm = -gmr;
        Gmbs = -gmbsr;
        FwdSum = 0.0;
        RevSum = -(Gm + Gmbs);
        Gmi = -gmi;
        Gmbsi = -gmbsi;
        FwdSumi = 0.0;
        RevSumi = -(Gmi + Gmbsi);
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
        if here.nqsMod == 0 || here.acnqsMod == 1 {
            cggb = Cggr;
            cgsb = Cgdr;
            cgdb = Cgsr;
            cbgb = here.cbgb;
            cbsb = here.cbdb;
            cbdb = here.cbsb;
            cdgb = -(Cdgr + cggb + cbgb);
            cdsb = -(Cddr + cgsb + cbsb);
            cddb = -(Cdsr + cgdb + cbdb);
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
            xcqgb = here.cqgb * omega;
            xcqdb = here.cqsb * omega;
            xcqsb = here.cqdb * omega;
            xcqbb = here.cqbb * omega;
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
        xcdgbi = Csgi;
        xcsgbi = Cdgi;
        xcddbi = Cssi;
        xcdsbi = Csdi;
        xcsdbi = Cdsi;
        xcssbi = Cddi;
        xcdbbi = Csbi;
        xcsbbi = Cdbi;
        xcggbi = Cggi;
        xcgdbi = Cgsi;
        xcgsbi = Cgdi;
        xcgbbi = Cgbi;
    }
    T1 = ckt.state0(state::QDEF) * here.gtau;
    gdpr = here.drainConductance;
    gspr = here.sourceConductance;
    gbd = here.gbd;
    gbs = here.gbs;
    capbd = here.capbd;
    capbs = here.capbs;
    GSoverlapCap = here.cgso;
    GDoverlapCap = here.cgdo;
    GBoverlapCap = pParam.cgbo;
    xcdgb = (cdgb - GDoverlapCap) * omega;
    xcddb = (cddb + capbd + GDoverlapCap) * omega;
    xcdsb = cdsb * omega;
    xcsgb = -(cggb + cbgb + cdgb + GSoverlapCap) * omega;
    xcsdb = -(cgdb + cbdb + cddb) * omega;
    xcssb = (capbs + GSoverlapCap - (cgsb + cbsb + cdsb)) * omega;
    xcggb = (cggb + GDoverlapCap + GSoverlapCap + GBoverlapCap) * omega;
    xcgdb = (cgdb - GDoverlapCap) * omega;
    xcgsb = (cgsb - GSoverlapCap) * omega;
    xcbgb = (cbgb - GBoverlapCap) * omega;
    xcbdb = (cbdb - capbd) * omega;
    xcbsb = (cbsb - capbs) * omega;
    m = here.m;
    ckt.imag(here.gNode, here.gNode, m * xcggb);
    ckt.imag(here.bNode, here.bNode, -(m * (xcbgb + xcbdb + xcbsb)));
    ckt.imag(
        here.dNodePrime,
        here.dNodePrime,
        m * (xcddb + gdsi + RevSumi),
    );
    ckt.imag(
        here.sNodePrime,
        here.sNodePrime,
        m * (xcssb + gdsi + FwdSumi),
    );
    ckt.imag(here.gNode, here.bNode, -(m * (xcggb + xcgdb + xcgsb)));
    ckt.imag(here.gNode, here.dNodePrime, m * xcgdb);
    ckt.imag(here.gNode, here.sNodePrime, m * xcgsb);
    ckt.imag(here.bNode, here.gNode, m * xcbgb);
    ckt.imag(here.bNode, here.dNodePrime, m * xcbdb);
    ckt.imag(here.bNode, here.sNodePrime, m * xcbsb);
    ckt.imag(here.dNodePrime, here.gNode, m * (xcdgb + Gmi));
    ckt.imag(
        here.dNodePrime,
        here.bNode,
        -(m * (xcdgb + xcddb + xcdsb + Gmbsi)),
    );
    ckt.imag(
        here.dNodePrime,
        here.sNodePrime,
        m * (xcdsb - gdsi - FwdSumi),
    );
    ckt.imag(here.sNodePrime, here.gNode, m * (xcsgb - Gmi));
    ckt.imag(
        here.sNodePrime,
        here.bNode,
        -(m * (xcsgb + xcsdb + xcssb - Gmbsi)),
    );
    ckt.imag(
        here.sNodePrime,
        here.dNodePrime,
        m * (xcsdb - gdsi - RevSumi),
    );
    ckt.real(here.dNode, here.dNode, m * gdpr);
    ckt.real(here.sNode, here.sNode, m * gspr);
    ckt.real(here.bNode, here.bNode, m * (gbd + gbs - here.gbbs));
    ckt.real(
        here.dNodePrime,
        here.dNodePrime,
        m * (gdpr + gds + gbd + RevSum + xcddbi + dxpart * xgtd + T1 * ddxpart_dVd + gbdpdp),
    );
    ckt.real(
        here.sNodePrime,
        here.sNodePrime,
        m * (gspr + gds + gbs + FwdSum + xcssbi + sxpart * xgts + T1 * dsxpart_dVs + gbspsp),
    );
    ckt.real(here.dNode, here.dNodePrime, -(m * gdpr));
    ckt.real(here.sNode, here.sNodePrime, -(m * gspr));
    ckt.real(here.bNode, here.gNode, -(m * here.gbgs));
    ckt.real(here.bNode, here.dNodePrime, -(m * (gbd - gbbdp)));
    ckt.real(here.bNode, here.sNodePrime, -(m * (gbs - gbbsp)));
    ckt.real(here.dNodePrime, here.dNode, -(m * gdpr));
    ckt.real(
        here.dNodePrime,
        here.gNode,
        m * (Gm + dxpart * xgtg + T1 * ddxpart_dVg + gbdpg + xcdgbi),
    );
    ckt.real(
        here.dNodePrime,
        here.bNode,
        -(m * (gbd - Gmbs - dxpart * xgtb - T1 * ddxpart_dVb - gbdpb - xcdbbi)),
    );
    ckt.real(
        here.dNodePrime,
        here.sNodePrime,
        -(m * (gds + FwdSum - dxpart * xgts - T1 * ddxpart_dVs - gbdpsp - xcdsbi)),
    );
    ckt.real(
        here.sNodePrime,
        here.gNode,
        -(m * (Gm - sxpart * xgtg - T1 * dsxpart_dVg - gbspg - xcsgbi)),
    );
    ckt.real(here.sNodePrime, here.sNode, -(m * gspr));
    ckt.real(
        here.sNodePrime,
        here.bNode,
        -(m * (gbs + Gmbs - sxpart * xgtb - T1 * dsxpart_dVb - gbspb - xcsbbi)),
    );
    ckt.real(
        here.sNodePrime,
        here.dNodePrime,
        -(m * (gds + RevSum - sxpart * xgtd - T1 * dsxpart_dVd - gbspdp - xcsdbi)),
    );
    ckt.real(here.gNode, here.gNode, -(m * (xgtg - xcggbi)));
    ckt.real(here.gNode, here.bNode, -(m * (xgtb - xcgbbi)));
    ckt.real(here.gNode, here.dNodePrime, -(m * (xgtd - xcgdbi)));
    ckt.real(here.gNode, here.sNodePrime, -(m * (xgts - xcgsbi)));
    if here.nqsMod != 0 {
        if here.acnqsMod != 0 {
            ckt.real(here.qNode, here.qNode, m * 1.0);
            ckt.real(here.qNode, here.gNode, 0.0);
            ckt.real(here.qNode, here.dNodePrime, 0.0);
            ckt.real(here.qNode, here.sNodePrime, 0.0);
            ckt.real(here.qNode, here.bNode, 0.0);
            ckt.real(here.dNodePrime, here.qNode, 0.0);
            ckt.real(here.sNodePrime, here.qNode, 0.0);
            ckt.real(here.gNode, here.qNode, 0.0);
        } else {
            ckt.imag(here.qNode, here.qNode, m * omega * ScalingFactor);
            ckt.imag(here.qNode, here.gNode, -(m * xcqgb));
            ckt.imag(here.qNode, here.dNodePrime, -(m * xcqdb));
            ckt.imag(here.qNode, here.sNodePrime, -(m * xcqsb));
            ckt.imag(here.qNode, here.bNode, -(m * xcqbb));
            ckt.real(here.qNode, here.qNode, m * here.gtau);
            ckt.real(here.dNodePrime, here.qNode, m * dxpart * here.gtau);
            ckt.real(here.sNodePrime, here.qNode, m * sxpart * here.gtau);
            ckt.real(here.gNode, here.qNode, -(m * here.gtau));
            ckt.real(here.qNode, here.gNode, m * xgtg);
            ckt.real(here.qNode, here.dNodePrime, m * xgtd);
            ckt.real(here.qNode, here.sNodePrime, m * xgts);
            ckt.real(here.qNode, here.bNode, m * xgtb);
        }
    }
    // C label `stray-close-brace:`.
    // C label `stray-close-brace:`.
    Ok(())
}
