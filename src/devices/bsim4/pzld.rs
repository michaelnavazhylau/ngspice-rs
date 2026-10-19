//! `b4pzld.c` (`BSIM4pzLoad`): the pole-zero load, followed statement by
//! statement over C's struct members.
//!
//! C adds `x * s` as `*(ptr) += x * s->real; *(ptr + 1) += x * s->imag`;
//! the port passes `x` to [`SmallSignalCkt::cap`], the coefficient of `s`
//! in the pencil `A + s E`. Every `b4pzld.c` term is affine in `s`. One
//! grouping differs from C: the AC NQS charge-node term
//! `m * s->real * ScalingFactor` is the coefficient `m * ScalingFactor`
//! times `s`, which rounds differently from C's left-to-right product.
#![allow(non_snake_case)]
use super::fields::{Instance, Model, SizeParam};
use super::host::{SmallSignalCkt, state};
use crate::primitives::SpiceResult;

/// `BSIM4pzLoad` for one instance, from the values the `MODEINITSMSIG`
/// load left on `h`.
///
/// # Errors
/// C's error returns (none: `BSIM4pzLoad` always returns `OK`).
#[allow(clippy::too_many_lines)] // One C function, kept in C's shape.
pub(crate) fn pz_load(
    m: &Model,
    h: &Instance,
    p: &SizeParam,
    ckt: &mut dyn SmallSignalCkt,
) -> SpiceResult<()> {
    let ScalingFactor = 1.0e-9;
    let cgso = h.cgso;
    let cgdo = h.cgdo;

    let mut T0 = 0.0;
    let mut xcdbdb = 0.0;
    let mut xcsbsb = 0.0;
    let mut xcgmgmb = 0.0;
    let mut xcgmdb = 0.0;
    let mut xcgmsb = 0.0;
    let mut xcdgmb = 0.0;
    let mut xcsgmb = 0.0;
    let mut xcgmbb = 0.0;
    let mut xcbgmb = 0.0;
    let mut xcqgb = 0.0;
    let mut xcqdb = 0.0;
    let mut xcqsb = 0.0;
    let mut xcqbb = 0.0;

    let (Gm, Gmbs, FwdSum, RevSum);
    let (gbbdp, gbbsp, gbdpg, gbdpdp, gbdpb, gbdpsp, gbspdp, gbspg, gbspb, gbspsp);
    let (gIstotg, gIstotd, gIstots, gIstotb, gIdtotg, gIdtotd, gIdtots, gIdtotb);
    let (gIbtotg, gIbtotd, gIbtots, gIbtotb, gIgtotg, gIgtotd, gIgtots, gIgtotb);
    let (gcrg, gcrgd, gcrgg, gcrgs, gcrgb);
    let (xcggb, xcgdb, xcgsb, xcgbb, xcbgb, xcbdb, xcbsb, xcbbb);
    let (xcdgb, xcddb, xcdsb, xcdbb, xcsgb, xcsdb, xcssb, xcsbb);
    let (xgtg, xgtd, xgts, xgtb, sxpart, dxpart);
    let (ddxpart_dVd, ddxpart_dVg, ddxpart_dVb, ddxpart_dVs);
    let (dsxpart_dVd, dsxpart_dVg, dsxpart_dVb, dsxpart_dVs);

    if h.mode >= 0 {
        Gm = h.gm;
        Gmbs = h.gmbs;
        FwdSum = Gm + Gmbs;
        RevSum = 0.0;

        gbbdp = -(h.gbds);
        gbbsp = h.gbds + h.gbgs + h.gbbs;
        gbdpg = h.gbgs;
        gbdpdp = h.gbds;
        gbdpb = h.gbbs;
        gbdpsp = -(gbdpg + gbdpdp + gbdpb);

        gbspdp = 0.0;
        gbspg = 0.0;
        gbspb = 0.0;
        gbspsp = 0.0;

        if m.igcMod != 0 {
            gIstotg = h.gIgsg + h.gIgcsg;
            gIstotd = h.gIgcsd;
            gIstots = h.gIgss + h.gIgcss;
            gIstotb = h.gIgcsb;

            gIdtotg = h.gIgdg + h.gIgcdg;
            gIdtotd = h.gIgdd + h.gIgcdd;
            gIdtots = h.gIgcds;
            gIdtotb = h.gIgcdb;
        } else {
            (gIstotg, gIstotd, gIstots, gIstotb) = (0.0, 0.0, 0.0, 0.0);
            (gIdtotg, gIdtotd, gIdtots, gIdtotb) = (0.0, 0.0, 0.0, 0.0);
        }

        if m.igbMod != 0 {
            gIbtotg = h.gIgbg;
            gIbtotd = h.gIgbd;
            gIbtots = h.gIgbs;
            gIbtotb = h.gIgbb;
        } else {
            (gIbtotg, gIbtotd, gIbtots, gIbtotb) = (0.0, 0.0, 0.0, 0.0);
        }

        if (m.igcMod != 0) || (m.igbMod != 0) {
            gIgtotg = gIstotg + gIdtotg + gIbtotg;
            gIgtotd = gIstotd + gIdtotd + gIbtotd;
            gIgtots = gIstots + gIdtots + gIbtots;
            gIgtotb = gIstotb + gIdtotb + gIbtotb;
        } else {
            (gIgtotg, gIgtotd, gIgtots, gIgtotb) = (0.0, 0.0, 0.0, 0.0);
        }

        if h.rgateMod == 2 {
            T0 = ckt.state0(state::VGES) - ckt.state0(state::VGS);
        } else if h.rgateMod == 3 {
            T0 = ckt.state0(state::VGMS) - ckt.state0(state::VGS);
        }
        if h.rgateMod > 1 {
            gcrgd = h.gcrgd * T0;
            gcrgg = h.gcrgg * T0 - h.gcrg;
            gcrgs = h.gcrgs * T0;
            gcrgb = h.gcrgb * T0;
            gcrg = h.gcrg;
        } else {
            (gcrg, gcrgd, gcrgg, gcrgs, gcrgb) = (0.0, 0.0, 0.0, 0.0, 0.0);
        }

        if h.acnqsMod == 0 {
            if h.rgateMod == 3 {
                xcgmgmb = cgdo + cgso + p.cgbo;
                xcgmdb = -cgdo;
                xcgmsb = -cgso;
                xcgmbb = -p.cgbo;

                xcdgmb = xcgmdb;
                xcsgmb = xcgmsb;
                xcbgmb = xcgmbb;

                xcggb = h.cggb;
                xcgdb = h.cgdb;
                xcgsb = h.cgsb;
                xcgbb = -(xcggb + xcgdb + xcgsb);

                xcdgb = h.cdgb;
                xcsgb = -(h.cggb + h.cbgb + h.cdgb);
                xcbgb = h.cbgb;
            } else {
                xcggb = h.cggb + cgdo + cgso + p.cgbo;
                xcgdb = h.cgdb - cgdo;
                xcgsb = h.cgsb - cgso;
                xcgbb = -(xcggb + xcgdb + xcgsb);

                xcdgb = h.cdgb - cgdo;
                xcsgb = -(h.cggb + h.cbgb + h.cdgb + cgso);
                xcbgb = h.cbgb - p.cgbo;

                (xcdgmb, xcsgmb, xcbgmb) = (0.0, 0.0, 0.0);
            }
            xcddb = h.cddb + h.capbd + cgdo;
            xcdsb = h.cdsb;

            xcsdb = -(h.cgdb + h.cbdb + h.cddb);
            xcssb = h.capbs + cgso - (h.cgsb + h.cbsb + h.cdsb);

            if h.rbodyMod == 0 {
                xcdbb = -(xcdgb + xcddb + xcdsb + xcdgmb);
                xcsbb = -(xcsgb + xcsdb + xcssb + xcsgmb);
                xcbdb = h.cbdb - h.capbd;
                xcbsb = h.cbsb - h.capbs;
                xcdbdb = 0.0;
            } else {
                xcdbb = -(h.cddb + h.cdgb + h.cdsb);
                xcsbb = -(xcsgb + xcsdb + xcssb + xcsgmb) + h.capbs;
                xcbdb = h.cbdb;
                xcbsb = h.cbsb;

                xcdbdb = -h.capbd;
                xcsbsb = -h.capbs;
            }
            xcbbb = -(xcbdb + xcbgb + xcbsb + xcbgmb);

            (xgtg, xgtd, xgts, xgtb) = (0.0, 0.0, 0.0, 0.0);
            sxpart = 0.6;
            dxpart = 0.4;
            (ddxpart_dVd, ddxpart_dVg, ddxpart_dVb, ddxpart_dVs) = (0.0, 0.0, 0.0, 0.0);
            (dsxpart_dVd, dsxpart_dVg, dsxpart_dVb, dsxpart_dVs) = (0.0, 0.0, 0.0, 0.0);
        } else {
            (xcggb, xcgdb, xcgsb, xcgbb) = (0.0, 0.0, 0.0, 0.0);
            (xcbgb, xcbdb, xcbsb, xcbbb) = (0.0, 0.0, 0.0, 0.0);
            (xcdgb, xcddb, xcdsb, xcdbb) = (0.0, 0.0, 0.0, 0.0);
            (xcsgb, xcsdb, xcssb, xcsbb) = (0.0, 0.0, 0.0, 0.0);

            xgtg = h.gtg;
            xgtd = h.gtd;
            xgts = h.gts;
            xgtb = h.gtb;

            xcqgb = h.cqgb;
            xcqdb = h.cqdb;
            xcqsb = h.cqsb;
            xcqbb = h.cqbb;

            let CoxWL = m.coxe * p.weffCV * h.nf * p.leffCV;
            let qcheq = -(h.qgate + h.qbulk);
            if qcheq.abs() <= 1.0e-5 * CoxWL {
                dxpart = if m.xpart < 0.5 {
                    0.4
                } else if m.xpart > 0.5 {
                    0.0
                } else {
                    0.5
                };
                (ddxpart_dVd, ddxpart_dVg, ddxpart_dVb, ddxpart_dVs) = (0.0, 0.0, 0.0, 0.0);
            } else {
                dxpart = h.qdrn / qcheq;
                let Cdd = h.cddb;
                let Csd = -(h.cgdb + h.cddb + h.cbdb);
                ddxpart_dVd = (Cdd - dxpart * (Cdd + Csd)) / qcheq;
                let Cdg = h.cdgb;
                let Csg = -(h.cggb + h.cdgb + h.cbgb);
                ddxpart_dVg = (Cdg - dxpart * (Cdg + Csg)) / qcheq;

                let Cds = h.cdsb;
                let Css = -(h.cgsb + h.cdsb + h.cbsb);
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
        Gm = -h.gm;
        Gmbs = -h.gmbs;
        FwdSum = 0.0;
        RevSum = -(Gm + Gmbs);

        gbbsp = -(h.gbds);
        gbbdp = h.gbds + h.gbgs + h.gbbs;

        gbdpg = 0.0;
        gbdpsp = 0.0;
        gbdpb = 0.0;
        gbdpdp = 0.0;

        gbspg = h.gbgs;
        gbspsp = h.gbds;
        gbspb = h.gbbs;
        gbspdp = -(gbspg + gbspsp + gbspb);

        if m.igcMod != 0 {
            gIstotg = h.gIgsg + h.gIgcdg;
            gIstotd = h.gIgcds;
            gIstots = h.gIgss + h.gIgcdd;
            gIstotb = h.gIgcdb;

            gIdtotg = h.gIgdg + h.gIgcsg;
            gIdtotd = h.gIgdd + h.gIgcss;
            gIdtots = h.gIgcsd;
            gIdtotb = h.gIgcsb;
        } else {
            (gIstotg, gIstotd, gIstots, gIstotb) = (0.0, 0.0, 0.0, 0.0);
            (gIdtotg, gIdtotd, gIdtots, gIdtotb) = (0.0, 0.0, 0.0, 0.0);
        }

        if m.igbMod != 0 {
            gIbtotg = h.gIgbg;
            gIbtotd = h.gIgbs;
            gIbtots = h.gIgbd;
            gIbtotb = h.gIgbb;
        } else {
            (gIbtotg, gIbtotd, gIbtots, gIbtotb) = (0.0, 0.0, 0.0, 0.0);
        }

        if (m.igcMod != 0) || (m.igbMod != 0) {
            gIgtotg = gIstotg + gIdtotg + gIbtotg;
            gIgtotd = gIstotd + gIdtotd + gIbtotd;
            gIgtots = gIstots + gIdtots + gIbtots;
            gIgtotb = gIstotb + gIdtotb + gIbtotb;
        } else {
            (gIgtotg, gIgtotd, gIgtots, gIgtotb) = (0.0, 0.0, 0.0, 0.0);
        }

        if h.rgateMod == 2 {
            T0 = ckt.state0(state::VGES) - ckt.state0(state::VGS);
        } else if h.rgateMod == 3 {
            T0 = ckt.state0(state::VGMS) - ckt.state0(state::VGS);
        }
        if h.rgateMod > 1 {
            // C swaps the drain and source derivatives in reverse mode.
            gcrgd = h.gcrgs * T0;
            gcrgg = h.gcrgg * T0 - h.gcrg;
            gcrgs = h.gcrgd * T0;
            gcrgb = h.gcrgb * T0;
            gcrg = h.gcrg;
        } else {
            (gcrg, gcrgd, gcrgg, gcrgs, gcrgb) = (0.0, 0.0, 0.0, 0.0, 0.0);
        }

        if h.acnqsMod == 0 {
            if h.rgateMod == 3 {
                xcgmgmb = cgdo + cgso + p.cgbo;
                xcgmdb = -cgdo;
                xcgmsb = -cgso;
                xcgmbb = -p.cgbo;

                xcdgmb = xcgmdb;
                xcsgmb = xcgmsb;
                xcbgmb = xcgmbb;

                xcggb = h.cggb;
                xcgdb = h.cgsb;
                xcgsb = h.cgdb;
                xcgbb = -(xcggb + xcgdb + xcgsb);

                xcdgb = -(h.cggb + h.cbgb + h.cdgb);
                xcsgb = h.cdgb;
                xcbgb = h.cbgb;
            } else {
                xcggb = h.cggb + cgdo + cgso + p.cgbo;
                xcgdb = h.cgsb - cgdo;
                xcgsb = h.cgdb - cgso;
                xcgbb = -(xcggb + xcgdb + xcgsb);

                xcdgb = -(h.cggb + h.cbgb + h.cdgb + cgdo);
                xcsgb = h.cdgb - cgso;
                xcbgb = h.cbgb - p.cgbo;

                (xcdgmb, xcsgmb, xcbgmb) = (0.0, 0.0, 0.0);
            }
            xcddb = h.capbd + cgdo - (h.cgsb + h.cbsb + h.cdsb);
            xcdsb = -(h.cgdb + h.cbdb + h.cddb);

            xcsdb = h.cdsb;
            xcssb = h.cddb + h.capbs + cgso;

            if h.rbodyMod == 0 {
                xcdbb = -(xcdgb + xcddb + xcdsb + xcdgmb);
                xcsbb = -(xcsgb + xcsdb + xcssb + xcsgmb);
                xcbdb = h.cbsb - h.capbd;
                xcbsb = h.cbdb - h.capbs;
                xcdbdb = 0.0;
            } else {
                xcdbb = -(xcdgb + xcddb + xcdsb + xcdgmb) + h.capbd;
                xcsbb = -(h.cddb + h.cdgb + h.cdsb);
                xcbdb = h.cbsb;
                xcbsb = h.cbdb;
                xcdbdb = -h.capbd;
                xcsbsb = -h.capbs;
            }
            xcbbb = -(xcbgb + xcbdb + xcbsb + xcbgmb);

            (xgtg, xgtd, xgts, xgtb) = (0.0, 0.0, 0.0, 0.0);
            sxpart = 0.4;
            dxpart = 0.6;
            (ddxpart_dVd, ddxpart_dVg, ddxpart_dVb, ddxpart_dVs) = (0.0, 0.0, 0.0, 0.0);
            (dsxpart_dVd, dsxpart_dVg, dsxpart_dVb, dsxpart_dVs) = (0.0, 0.0, 0.0, 0.0);
        } else {
            (xcggb, xcgdb, xcgsb, xcgbb) = (0.0, 0.0, 0.0, 0.0);
            (xcbgb, xcbdb, xcbsb, xcbbb) = (0.0, 0.0, 0.0, 0.0);
            (xcdgb, xcddb, xcdsb, xcdbb) = (0.0, 0.0, 0.0, 0.0);
            (xcsgb, xcsdb, xcssb, xcsbb) = (0.0, 0.0, 0.0, 0.0);

            xgtg = h.gtg;
            xgtd = h.gts;
            xgts = h.gtd;
            xgtb = h.gtb;

            xcqgb = h.cqgb;
            xcqdb = h.cqsb;
            xcqsb = h.cqdb;
            xcqbb = h.cqbb;

            let CoxWL = m.coxe * p.weffCV * h.nf * p.leffCV;
            let qcheq = -(h.qgate + h.qbulk);
            if qcheq.abs() <= 1.0e-5 * CoxWL {
                sxpart = if m.xpart < 0.5 {
                    0.4
                } else if m.xpart > 0.5 {
                    0.0
                } else {
                    0.5
                };
                (dsxpart_dVd, dsxpart_dVg, dsxpart_dVb, dsxpart_dVs) = (0.0, 0.0, 0.0, 0.0);
            } else {
                sxpart = h.qdrn / qcheq;
                let Css = h.cddb;
                let Cds = -(h.cgdb + h.cddb + h.cbdb);
                dsxpart_dVs = (Css - sxpart * (Css + Cds)) / qcheq;
                let Csg = h.cdgb;
                let Cdg = -(h.cggb + h.cdgb + h.cbgb);
                dsxpart_dVg = (Csg - sxpart * (Csg + Cdg)) / qcheq;

                let Csd = h.cdsb;
                let Cdd = -(h.cgsb + h.cdsb + h.cbsb);
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

    let (gstot, gstotd, gstotg, gstots, gstotb);
    let (gdtot, gdtotd, gdtotg, gdtots, gdtotb);
    if m.rdsMod == 1 {
        gstot = h.gstot;
        gstotd = h.gstotd;
        gstotg = h.gstotg;
        gstots = h.gstots - gstot;
        gstotb = h.gstotb;

        gdtot = h.gdtot;
        gdtotd = h.gdtotd - gdtot;
        gdtotg = h.gdtotg;
        gdtots = h.gdtots;
        gdtotb = h.gdtotb;
    } else {
        (gstot, gstotd, gstotg, gstots, gstotb) = (0.0, 0.0, 0.0, 0.0, 0.0);
        (gdtot, gdtotd, gdtotg, gdtots, gdtotb) = (0.0, 0.0, 0.0, 0.0, 0.0);
    }

    let T1 = ckt.state0(state::QDEF) * h.gtau;
    let gds = h.gds;

    // Loading PZ matrix
    let mm = h.m;
    let mult_i = h.mult_i * h.m;
    let mult_q = h.mult_q * h.m;

    let (gdpr, gspr) = if m.rdsMod == 0 {
        (h.drainConductance, h.sourceConductance)
    } else {
        (0.0, 0.0)
    };

    let (gjbd, gjbs) = if h.rbodyMod == 0 {
        (h.gbd, h.gbs)
    } else {
        (0.0, 0.0)
    };

    let geltd = h.grgeltd;

    let (dp, gp, gm, sp, bp) = (
        h.dNodePrime,
        h.gNodePrime,
        h.gNodeMid,
        h.sNodePrime,
        h.bNodePrime,
    );
    let (d, ge, s, b) = (h.dNode, h.gNodeExt, h.sNode, h.bNode);
    let (db, sb, q) = (h.dbNode, h.sbNode, h.qNode);

    if h.rgateMod == 1 {
        ckt.real(ge, ge, mult_i * geltd);
        ckt.real(gp, ge, -(mult_i * geltd));
        ckt.real(ge, gp, -(mult_i * geltd));

        ckt.cap(gp, gp, mult_q * xcggb);
        ckt.real(gp, gp, mult_i * (geltd + gIgtotg) - mult_q * xgtg);
        ckt.cap(gp, dp, mult_q * xcgdb);
        ckt.real(gp, dp, -(mult_q * xgtd - mult_i * gIgtotd));
        ckt.cap(gp, sp, mult_q * xcgsb);
        ckt.real(gp, sp, -(mult_q * xgts - mult_i * gIgtots));
        ckt.cap(gp, bp, mult_q * xcgbb);
        ckt.real(gp, bp, -(mult_q * xgtb - mult_i * gIgtotb));
    } else if h.rgateMod == 2 {
        ckt.real(ge, ge, mult_i * gcrg);
        ckt.real(ge, gp, mult_i * gcrgg);
        ckt.real(ge, dp, mult_i * gcrgd);
        ckt.real(ge, sp, mult_i * gcrgs);
        ckt.real(ge, bp, mult_i * gcrgb);

        ckt.real(gp, ge, -(mult_i * gcrg));
        ckt.cap(gp, gp, mult_q * xcggb);
        ckt.real(gp, gp, -(mult_i * (gcrgg - gIgtotg) + mult_q * xgtg));
        ckt.cap(gp, dp, mult_q * xcgdb);
        ckt.real(gp, dp, -(mult_i * (gcrgd - gIgtotd) + mult_q * xgtd));
        ckt.cap(gp, sp, mult_q * xcgsb);
        ckt.real(gp, sp, -(mult_i * (gcrgs - gIgtots) + mult_q * xgts));
        ckt.cap(gp, bp, mult_q * xcgbb);
        ckt.real(gp, bp, -(mult_i * (gcrgb - gIgtotb) + mult_q * xgtb));
    } else if h.rgateMod == 3 {
        ckt.real(ge, ge, mult_i * geltd);
        ckt.real(ge, gm, -(mult_i * geltd));
        ckt.real(gm, ge, -(mult_i * geltd));
        ckt.real(gm, gm, mult_i * (geltd + gcrg));
        ckt.cap(gm, gm, mult_q * xcgmgmb);

        ckt.real(gm, dp, mult_i * gcrgd);
        ckt.cap(gm, dp, mult_q * xcgmdb);
        ckt.real(gm, gp, mult_i * gcrgg);
        ckt.real(gm, sp, mult_i * gcrgs);
        ckt.cap(gm, sp, mult_q * xcgmsb);
        ckt.real(gm, bp, mult_i * gcrgb);
        ckt.cap(gm, bp, mult_q * xcgmbb);

        ckt.cap(dp, gm, mult_q * xcdgmb);
        ckt.real(gp, gm, -(mult_i * gcrg));
        ckt.cap(sp, gm, mult_q * xcsgmb);
        ckt.cap(bp, gm, mult_q * xcbgmb);

        ckt.real(gp, gp, -(mult_i * (gcrgg - gIgtotg) + mult_q * xgtg));
        ckt.cap(gp, gp, mult_q * xcggb);
        ckt.real(gp, dp, -(mult_i * (gcrgd - gIgtotd) + mult_q * xgtd));
        ckt.cap(gp, dp, mult_q * xcgdb);
        ckt.real(gp, sp, -(mult_i * (gcrgs - gIgtots) + mult_q * xgts));
        ckt.cap(gp, sp, mult_q * xcgsb);
        ckt.real(gp, bp, -(mult_i * (gcrgb - gIgtotb) + mult_q * xgtb));
        ckt.cap(gp, bp, mult_q * xcgbb);
    } else {
        ckt.cap(gp, dp, mult_q * xcgdb);
        ckt.real(gp, dp, -(mult_q * xgtd - mult_i * gIgtotd));
        ckt.cap(gp, gp, mult_q * xcggb);
        ckt.real(gp, gp, -(mult_q * xgtg - mult_i * gIgtotg));
        ckt.cap(gp, sp, mult_q * xcgsb);
        ckt.real(gp, sp, -(mult_q * xgts - mult_i * gIgtots));
        ckt.cap(gp, bp, mult_q * xcgbb);
        ckt.real(gp, bp, -(mult_q * xgtb - mult_i * gIgtotb));
    }

    if m.rdsMod != 0 {
        ckt.real(d, gp, mult_i * gdtotg);
        ckt.real(d, sp, mult_i * gdtots);
        ckt.real(d, bp, mult_i * gdtotb);
        ckt.real(s, dp, mult_i * gstotd);
        ckt.real(s, gp, mult_i * gstotg);
        ckt.real(s, bp, mult_i * gstotb);
    }

    ckt.cap(dp, dp, mult_q * xcddb);
    ckt.real(
        dp,
        dp,
        mult_i * (gdpr + gds + h.gbd - gdtotd + RevSum + gbdpdp - gIdtotd)
            + mult_q * (dxpart * xgtd + T1 * ddxpart_dVd),
    );
    ckt.real(dp, d, -(mult_i * (gdpr + gdtot)));
    ckt.cap(dp, gp, mult_q * xcdgb);
    ckt.real(
        dp,
        gp,
        mult_i * (Gm - gdtotg + gbdpg - gIdtotg) + mult_q * (T1 * ddxpart_dVg + dxpart * xgtg),
    );
    ckt.cap(dp, sp, mult_q * xcdsb);
    ckt.real(
        dp,
        sp,
        -(mult_i * (gds + FwdSum + gdtots - gbdpsp + gIdtots)
            - mult_q * (T1 * ddxpart_dVs + dxpart * xgts)),
    );
    ckt.cap(dp, bp, mult_q * xcdbb);
    ckt.real(
        dp,
        bp,
        -(mult_i * (gjbd + gdtotb - Gmbs - gbdpb + gIdtotb)
            - mult_q * T1 * (ddxpart_dVb + dxpart * xgtb)),
    );

    ckt.real(d, dp, -(mult_i * (gdpr - gdtotd)));
    ckt.real(d, d, mult_i * (gdpr + gdtot));

    ckt.cap(sp, dp, mult_q * xcsdb);
    ckt.real(
        sp,
        dp,
        -(mult_i * (gds + gstotd + RevSum - gbspdp + gIstotd)
            - mult_q * (T1 * dsxpart_dVd + sxpart * xgtd)),
    );
    ckt.cap(sp, gp, mult_q * xcsgb);
    ckt.real(
        sp,
        gp,
        -(mult_i * (Gm + gstotg - gbspg + gIstotg) - mult_q * (T1 * dsxpart_dVg + sxpart * xgtg)),
    );
    ckt.cap(sp, sp, mult_q * xcssb);
    ckt.real(
        sp,
        sp,
        mult_i * (gspr + gds + h.gbs - gIstots - gstots + FwdSum + gbspsp)
            + mult_q * (sxpart * xgts + T1 * dsxpart_dVs),
    );
    ckt.real(sp, s, -(mult_i * (gspr + gstot)));
    ckt.cap(sp, bp, mult_q * xcsbb);
    ckt.real(
        sp,
        bp,
        -(mult_i * (gjbs + gstotb + Gmbs - gbspb + gIstotb)
            - mult_q * (T1 * dsxpart_dVb + sxpart * xgtb)),
    );

    ckt.real(s, sp, -(mult_i * (gspr - gstots)));
    ckt.real(s, s, mult_i * (gspr + gstot));

    ckt.cap(bp, dp, mult_q * xcbdb);
    ckt.real(bp, dp, -(mult_i * (gjbd - gbbdp + gIbtotd)));
    ckt.cap(bp, gp, mult_q * xcbgb);
    ckt.real(bp, gp, -(mult_i * (h.gbgs + gIbtotg)));
    ckt.cap(bp, sp, mult_q * xcbsb);
    ckt.real(bp, sp, -(mult_i * (gjbs - gbbsp + gIbtots)));
    ckt.cap(bp, bp, mult_q * xcbbb);
    ckt.real(bp, bp, mult_i * (gjbd + gjbs - h.gbbs - gIbtotb));
    let ggidld = h.ggidld;
    let ggidlg = h.ggidlg;
    let ggidlb = h.ggidlb;
    let ggislg = h.ggislg;
    let ggisls = h.ggisls;
    let ggislb = h.ggislb;

    // stamp gidl
    ckt.real(dp, dp, mult_i * ggidld);
    ckt.real(dp, gp, mult_i * ggidlg);
    ckt.real(dp, sp, -(mult_i * ((ggidlg + ggidld) + ggidlb)));
    ckt.real(dp, bp, mult_i * ggidlb);
    ckt.real(bp, dp, -(mult_i * ggidld));
    ckt.real(bp, gp, -(mult_i * ggidlg));
    ckt.real(bp, sp, mult_i * ((ggidlg + ggidld) + ggidlb));
    ckt.real(bp, bp, -(mult_i * ggidlb));
    // stamp gisl
    ckt.real(sp, dp, -(mult_i * ((ggisls + ggislg) + ggislb)));
    ckt.real(sp, gp, mult_i * ggislg);
    ckt.real(sp, sp, mult_i * ggisls);
    ckt.real(sp, bp, mult_i * ggislb);
    ckt.real(bp, dp, mult_i * ((ggislg + ggisls) + ggislb));
    ckt.real(bp, gp, -(mult_i * ggislg));
    ckt.real(bp, sp, -(mult_i * ggisls));
    ckt.real(bp, bp, -(mult_i * ggislb));

    if h.rbodyMod != 0 {
        ckt.cap(dp, db, mult_q * xcdbdb);
        ckt.real(dp, db, -(mult_i * h.gbd));
        ckt.cap(sp, sb, mult_q * xcsbsb);
        ckt.real(sp, sb, -(mult_i * h.gbs));

        ckt.cap(db, dp, mult_q * xcdbdb);
        ckt.real(db, dp, -(mult_i * h.gbd));
        ckt.cap(db, db, -(mult_q * xcdbdb));
        ckt.real(db, db, mult_i * (h.gbd + h.grbpd + h.grbdb));
        ckt.real(db, bp, -(mult_i * h.grbpd));
        ckt.real(db, b, -(mult_i * h.grbdb));

        ckt.real(bp, db, -(mult_i * h.grbpd));
        ckt.real(bp, b, -(mult_i * h.grbpb));
        ckt.real(bp, sb, -(mult_i * h.grbps));
        ckt.real(bp, bp, mult_i * (h.grbpd + h.grbps + h.grbpb));
        // WDL: (-here->BSIM4gbbs) already added to BPbpPtr

        ckt.cap(sb, sp, mult_q * xcsbsb);
        ckt.real(sb, sp, -(mult_i * h.gbs));
        ckt.real(sb, bp, -(mult_i * h.grbps));
        ckt.real(sb, b, -(mult_i * h.grbsb));
        ckt.cap(sb, sb, -(mult_q * xcsbsb));
        ckt.real(sb, sb, mult_i * (h.gbs + h.grbps + h.grbsb));

        ckt.real(b, db, -(mult_i * h.grbdb));
        ckt.real(b, bp, -(mult_i * h.grbpb));
        ckt.real(b, sb, -(mult_i * h.grbsb));
        ckt.real(b, b, mult_i * (h.grbsb + h.grbdb + h.grbpb));
    }

    if h.acnqsMod != 0 {
        // C: `*(QqPtr) += m * s->real * ScalingFactor` (see the module note).
        ckt.cap(q, q, mm * ScalingFactor);
        ckt.cap(q, gp, -(mm * xcqgb));
        ckt.cap(q, dp, -(mm * xcqdb));
        ckt.cap(q, bp, -(mm * xcqbb));
        ckt.cap(q, sp, -(mm * xcqsb));

        ckt.real(gp, q, -(mm * h.gtau));
        ckt.real(dp, q, mm * dxpart * h.gtau);
        ckt.real(sp, q, mm * sxpart * h.gtau);

        ckt.real(q, q, mm * h.gtau);
        ckt.real(q, gp, mm * xgtg);
        ckt.real(q, dp, mm * xgtd);
        ckt.real(q, bp, mm * xgtb);
        ckt.real(q, sp, mm * xgts);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::acld::tests::check_shapes;
    use super::SizeParam;

    #[test]
    fn pz_stamps_land_on_allocated_elements() {
        check_shapes(
            |m, h, ckt| super::pz_load(m, h, &SizeParam::default(), ckt).unwrap(),
            true,
            false,
        );
    }
}
