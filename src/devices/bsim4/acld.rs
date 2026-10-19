//! `b4acld.c` (`BSIM4acLoad`): the small-signal AC load, followed statement
//! by statement over C's struct members.
#![allow(non_snake_case)]
use super::fields::{Instance, Model, SizeParam};
use super::host::{SmallSignalCkt, state};
use crate::primitives::SpiceResult;

/// `BSIM4acLoad` for one instance, from the values the `MODEINITSMSIG`
/// load left on `h`.
///
/// # Errors
/// C's error returns (none: `BSIM4acLoad` always returns `OK`).
#[allow(clippy::too_many_lines)] // One C function, kept in C's shape.
pub(crate) fn ac_load(
    m: &Model,
    h: &Instance,
    p: &SizeParam,
    ckt: &mut dyn SmallSignalCkt,
) -> SpiceResult<()> {
    let omega = ckt.omega();
    let cgso = h.cgso;
    let cgdo = h.cgdo;

    let Csd = -(h.cddb + h.cgdb + h.cbdb);
    let Csg = -(h.cdgb + h.cggb + h.cbgb);
    let Css = -(h.cdsb + h.cgsb + h.cbsb);

    let mut T0 = 0.0;
    let (gmr, gmbsr, gdsr, gmi, gmbsi, gdsi);
    let (Cddr, Cdgr, Cdsr, Cdbr, Cddi, Cdgi, Cdsi, Cdbi);
    let (Csdr, Csgr, Cssr, Csbr, Csdi, Csgi, Cssi, Csbi);
    let (Cgdr, Cggr, Cgsr, Cgdi, Cggi, Cgsi, Cgbi);
    if h.acnqsMod != 0 {
        T0 = omega * h.taunet;
        let T1 = T0 * T0;
        let T2 = 1.0 / (1.0 + T1);
        let T3 = T0 * T2;

        gmr = h.gm * T2;
        gmbsr = h.gmbs * T2;
        gdsr = h.gds * T2;

        gmi = -h.gm * T3;
        gmbsi = -h.gmbs * T3;
        gdsi = -h.gds * T3;

        Cddr = h.cddb * T2;
        Cdgr = h.cdgb * T2;
        Cdsr = h.cdsb * T2;
        Cdbr = -(Cddr + Cdgr + Cdsr);

        // WDLiu: Cxyi multiplied by jomega below, and actually to be of
        // conductance.
        Cddi = h.cddb * T3 * omega;
        Cdgi = h.cdgb * T3 * omega;
        Cdsi = h.cdsb * T3 * omega;
        Cdbi = -(Cddi + Cdgi + Cdsi);

        Csdr = Csd * T2;
        Csgr = Csg * T2;
        Cssr = Css * T2;
        Csbr = -(Csdr + Csgr + Cssr);

        Csdi = Csd * T3 * omega;
        Csgi = Csg * T3 * omega;
        Cssi = Css * T3 * omega;
        Csbi = -(Csdi + Csgi + Cssi);

        Cgdr = -(Cddr + Csdr + h.cbdb);
        Cggr = -(Cdgr + Csgr + h.cbgb);
        Cgsr = -(Cdsr + Cssr + h.cbsb);
        // C's Cgbr = -(Cgdr + Cggr + Cgsr) is never read.

        Cgdi = -(Cddi + Csdi);
        Cggi = -(Cdgi + Csgi);
        Cgsi = -(Cdsi + Cssi);
        Cgbi = -(Cgdi + Cggi + Cgsi);
    } else {
        // QS
        gmr = h.gm;
        gmbsr = h.gmbs;
        gdsr = h.gds;
        gmi = 0.0;
        gmbsi = 0.0;
        gdsi = 0.0;

        Cddr = h.cddb;
        Cdgr = h.cdgb;
        Cdsr = h.cdsb;
        Cdbr = -(Cddr + Cdgr + Cdsr);
        Cddi = 0.0;
        Cdgi = 0.0;
        Cdsi = 0.0;
        Cdbi = 0.0;

        Csdr = Csd;
        Csgr = Csg;
        Cssr = Css;
        Csbr = -(Csdr + Csgr + Cssr);
        Csdi = 0.0;
        Csgi = 0.0;
        Cssi = 0.0;
        Csbi = 0.0;

        Cgdr = h.cgdb;
        Cggr = h.cggb;
        Cgsr = h.cgsb;
        Cgdi = 0.0;
        Cggi = 0.0;
        Cgsi = 0.0;
        Cgbi = 0.0;
    }
    let _ = Csbr;

    let (Gmr, Gmbsr, FwdSumr, RevSumr, Gmi, Gmbsi, FwdSumi, RevSumi);
    let (gbbdp, gbbsp, gbdpg, gbdpdp, gbdpb, gbdpsp, gbspdp, gbspg, gbspb, gbspsp);
    let (gIstotg, gIstotd, gIstots, gIstotb, gIdtotg, gIdtotd, gIdtots, gIdtotb);
    let (gIbtotg, gIbtotd, gIbtots, gIbtotb, gIgtotg, gIgtotd, gIgtots, gIgtotb);
    let (gcrg, gcrgd, gcrgg, gcrgs, gcrgb);
    let mut xcgmgmb = 0.0;
    let mut xcgmdb = 0.0;
    let mut xcgmsb = 0.0;
    let mut xcgmbb = 0.0;
    let mut xcsbsb = 0.0;
    let (xcdgmb, xcsgmb, xcbgmb, xcggbr, xcgdbr, xcgsbr, xcgbbr, xcdgbr, xcsgbr, xcbgb);
    let (xcddbr, xcdsbr, xcsdbr, xcssbr, xcdbbr, xcsbbr, xcbdb, xcbsb, xcdbdb, xcbbb);
    let (xcdgbi, xcsgbi, xcddbi, xcdsbi, xcsdbi, xcssbi, xcdbbi, xcsbbi);
    let (xcggbi, xcgdbi, xcgsbi, xcgbbi);
    if h.mode >= 0 {
        Gmr = gmr;
        Gmbsr = gmbsr;
        FwdSumr = Gmr + Gmbsr;
        RevSumr = 0.0;
        Gmi = gmi;
        Gmbsi = gmbsi;
        FwdSumi = Gmi + Gmbsi;
        RevSumi = 0.0;

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

        if h.rgateMod == 3 {
            xcgmgmb = (cgdo + cgso + p.cgbo) * omega;
            xcgmdb = -cgdo * omega;
            xcgmsb = -cgso * omega;
            xcgmbb = -p.cgbo * omega;

            xcdgmb = xcgmdb;
            xcsgmb = xcgmsb;
            xcbgmb = xcgmbb;

            xcggbr = Cggr * omega;
            xcgdbr = Cgdr * omega;
            xcgsbr = Cgsr * omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);

            xcdgbr = Cdgr * omega;
            xcsgbr = Csgr * omega;
            xcbgb = h.cbgb * omega;
        } else {
            xcggbr = (Cggr + cgdo + cgso + p.cgbo) * omega;
            xcgdbr = (Cgdr - cgdo) * omega;
            xcgsbr = (Cgsr - cgso) * omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);

            xcdgbr = (Cdgr - cgdo) * omega;
            xcsgbr = (Csgr - cgso) * omega;
            xcbgb = (h.cbgb - p.cgbo) * omega;

            (xcdgmb, xcsgmb, xcbgmb) = (0.0, 0.0, 0.0);
        }
        xcddbr = (Cddr + h.capbd + cgdo) * omega;
        xcdsbr = Cdsr * omega;
        xcsdbr = Csdr * omega;
        xcssbr = (h.capbs + cgso + Cssr) * omega;

        if h.rbodyMod == 0 {
            xcdbbr = -(xcdgbr + xcddbr + xcdsbr + xcdgmb);
            xcsbbr = -(xcsgbr + xcsdbr + xcssbr + xcsgmb);

            xcbdb = (h.cbdb - h.capbd) * omega;
            xcbsb = (h.cbsb - h.capbs) * omega;
            xcdbdb = 0.0;
        } else {
            xcdbbr = Cdbr * omega;
            xcsbbr = -(xcsgbr + xcsdbr + xcssbr + xcsgmb) + h.capbs * omega;

            xcbdb = h.cbdb * omega;
            xcbsb = h.cbsb * omega;

            xcdbdb = -h.capbd * omega;
            xcsbsb = -h.capbs * omega;
        }
        xcbbb = -(xcbdb + xcbgb + xcbsb + xcbgmb);

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
        // Reverse mode
        Gmr = -gmr;
        Gmbsr = -gmbsr;
        FwdSumr = 0.0;
        RevSumr = -(Gmr + Gmbsr);
        Gmi = -gmi;
        Gmbsi = -gmbsi;
        FwdSumi = 0.0;
        RevSumi = -(Gmi + Gmbsi);

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

        if h.rgateMod == 3 {
            xcgmgmb = (cgdo + cgso + p.cgbo) * omega;
            xcgmdb = -cgdo * omega;
            xcgmsb = -cgso * omega;
            xcgmbb = -p.cgbo * omega;

            xcdgmb = xcgmdb;
            xcsgmb = xcgmsb;
            xcbgmb = xcgmbb;

            xcggbr = Cggr * omega;
            xcgdbr = Cgsr * omega;
            xcgsbr = Cgdr * omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);

            xcdgbr = Csgr * omega;
            xcsgbr = Cdgr * omega;
            xcbgb = h.cbgb * omega;
        } else {
            xcggbr = (Cggr + cgdo + cgso + p.cgbo) * omega;
            xcgdbr = (Cgsr - cgdo) * omega;
            xcgsbr = (Cgdr - cgso) * omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);

            xcdgbr = (Csgr - cgdo) * omega;
            xcsgbr = (Cdgr - cgso) * omega;
            xcbgb = (h.cbgb - p.cgbo) * omega;

            (xcdgmb, xcsgmb, xcbgmb) = (0.0, 0.0, 0.0);
        }
        xcddbr = (h.capbd + cgdo + Cssr) * omega;
        xcdsbr = Csdr * omega;
        xcsdbr = Cdsr * omega;
        xcssbr = (Cddr + h.capbs + cgso) * omega;

        if h.rbodyMod == 0 {
            xcdbbr = -(xcdgbr + xcddbr + xcdsbr + xcdgmb);
            xcsbbr = -(xcsgbr + xcsdbr + xcssbr + xcsgmb);

            xcbdb = (h.cbsb - h.capbd) * omega;
            xcbsb = (h.cbdb - h.capbs) * omega;
            xcdbdb = 0.0;
        } else {
            xcdbbr = -(xcdgbr + xcddbr + xcdsbr + xcdgmb) + h.capbd * omega;
            xcsbbr = Cdbr * omega;

            xcbdb = h.cbsb * omega;
            xcbsb = h.cbdb * omega;
            xcdbdb = -h.capbd * omega;
            xcsbsb = -h.capbs * omega;
        }
        xcbbb = -(xcbgb + xcbdb + xcbsb + xcbgmb);

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

    // Loading AC matrix
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

        ckt.imag(gp, gp, mult_q * xcggbr);
        ckt.real(gp, gp, mult_i * (geltd + gIgtotg) + mult_q * xcggbi);
        ckt.imag(gp, dp, mult_q * xcgdbr);
        ckt.real(gp, dp, mult_q * xcgdbi + mult_i * gIgtotd);
        ckt.imag(gp, sp, mult_q * xcgsbr);
        ckt.real(gp, sp, mult_q * xcgsbi + mult_i * gIgtots);
        ckt.imag(gp, bp, mult_q * xcgbbr);
        ckt.real(gp, bp, mult_q * xcgbbi + mult_i * gIgtotb);
    } else if h.rgateMod == 2 {
        // WDLiu: gcrg already subtracted from all gcrgg below
        ckt.real(ge, ge, mult_i * gcrg);
        ckt.real(ge, gp, mult_i * gcrgg);
        ckt.real(ge, dp, mult_i * gcrgd);
        ckt.real(ge, sp, mult_i * gcrgs);
        ckt.real(ge, bp, mult_i * gcrgb);

        ckt.real(gp, ge, -(mult_i * gcrg));
        ckt.imag(gp, gp, mult_q * xcggbr);
        ckt.real(gp, gp, -(mult_i * (gcrgg - gIgtotg) - mult_q * xcggbi));
        ckt.imag(gp, dp, mult_q * xcgdbr);
        ckt.real(gp, dp, -(mult_i * (gcrgd - gIgtotd) - mult_q * xcgdbi));
        ckt.imag(gp, sp, mult_q * xcgsbr);
        ckt.real(gp, sp, -(mult_i * (gcrgs - gIgtots) - mult_q * xcgsbi));
        ckt.imag(gp, bp, mult_q * xcgbbr);
        ckt.real(gp, bp, -(mult_i * (gcrgb - gIgtotb) - mult_q * xcgbbi));
    } else if h.rgateMod == 3 {
        ckt.real(ge, ge, mult_i * geltd);
        ckt.real(ge, gm, -(mult_i * geltd));
        ckt.real(gm, ge, -(mult_i * geltd));
        ckt.real(gm, gm, mult_i * (geltd + gcrg));
        ckt.imag(gm, gm, mult_q * xcgmgmb);

        ckt.real(gm, dp, mult_i * gcrgd);
        ckt.imag(gm, dp, mult_q * xcgmdb);
        ckt.real(gm, gp, mult_i * gcrgg);
        ckt.real(gm, sp, mult_i * gcrgs);
        ckt.imag(gm, sp, mult_q * xcgmsb);
        ckt.real(gm, bp, mult_i * gcrgb);
        ckt.imag(gm, bp, mult_q * xcgmbb);

        ckt.imag(dp, gm, mult_q * xcdgmb);
        ckt.real(gp, gm, -(mult_i * gcrg));
        ckt.imag(sp, gm, mult_q * xcsgmb);
        ckt.imag(bp, gm, mult_q * xcbgmb);

        ckt.real(gp, gp, -(mult_i * (gcrgg - gIgtotg) - mult_q * xcggbi));
        ckt.imag(gp, gp, mult_q * xcggbr);
        ckt.real(gp, dp, -(mult_i * (gcrgd - gIgtotd) - mult_q * xcgdbi));
        ckt.imag(gp, dp, mult_q * xcgdbr);
        ckt.real(gp, sp, -(mult_i * (gcrgs - gIgtots) - mult_q * xcgsbi));
        ckt.imag(gp, sp, mult_q * xcgsbr);
        ckt.real(gp, bp, -(mult_i * (gcrgb - gIgtotb) - mult_q * xcgbbi));
        ckt.imag(gp, bp, mult_q * xcgbbr);
    } else {
        ckt.imag(gp, gp, mult_q * xcggbr);
        ckt.real(gp, gp, mult_q * xcggbi + mult_i * gIgtotg);
        ckt.imag(gp, dp, mult_q * xcgdbr);
        ckt.real(gp, dp, mult_q * xcgdbi + mult_i * gIgtotd);
        ckt.imag(gp, sp, mult_q * xcgsbr);
        ckt.real(gp, sp, mult_q * xcgsbi + mult_i * gIgtots);
        ckt.imag(gp, bp, mult_q * xcgbbr);
        ckt.real(gp, bp, mult_q * xcgbbi + mult_i * gIgtotb);
    }

    if m.rdsMod != 0 {
        ckt.real(d, gp, mult_i * gdtotg);
        ckt.real(d, sp, mult_i * gdtots);
        ckt.real(d, bp, mult_i * gdtotb);
        ckt.real(s, dp, mult_i * gstotd);
        ckt.real(s, gp, mult_i * gstotg);
        ckt.real(s, bp, mult_i * gstotb);
    }

    ckt.imag(dp, dp, mult_q * xcddbr + mult_i * (gdsi + RevSumi));
    ckt.real(
        dp,
        dp,
        mult_i * (gdpr + gdsr + h.gbd - gdtotd + RevSumr + gbdpdp - gIdtotd) + mult_q * xcddbi,
    );
    ckt.real(dp, d, -(mult_i * (gdpr + gdtot)));
    ckt.imag(dp, gp, mult_q * xcdgbr + mult_i * Gmi);
    ckt.real(
        dp,
        gp,
        mult_i * (Gmr - gdtotg + gbdpg - gIdtotg) + mult_q * xcdgbi,
    );
    ckt.imag(dp, sp, mult_q * xcdsbr - mult_i * (gdsi + FwdSumi));
    ckt.real(
        dp,
        sp,
        -(mult_i * (gdsr + FwdSumr + gdtots - gbdpsp + gIdtots) - mult_q * xcdsbi),
    );
    ckt.imag(dp, bp, mult_q * xcdbbr + mult_i * Gmbsi);
    ckt.real(
        dp,
        bp,
        -(mult_i * (gjbd + gdtotb - Gmbsr - gbdpb + gIdtotb) - mult_q * xcdbbi),
    );

    ckt.real(d, dp, -(mult_i * (gdpr - gdtotd)));
    ckt.real(d, d, mult_i * (gdpr + gdtot));

    ckt.imag(sp, dp, mult_q * xcsdbr - mult_i * (gdsi + RevSumi));
    ckt.real(
        sp,
        dp,
        -(mult_i * (gdsr + gstotd + RevSumr - gbspdp + gIstotd) - mult_q * xcsdbi),
    );
    ckt.imag(sp, gp, mult_q * xcsgbr - mult_i * Gmi);
    ckt.real(
        sp,
        gp,
        -(mult_i * (Gmr + gstotg - gbspg + gIstotg) - mult_q * xcsgbi),
    );
    ckt.imag(sp, sp, mult_q * xcssbr + mult_i * (gdsi + FwdSumi));
    ckt.real(
        sp,
        sp,
        mult_i * (gspr + gdsr + h.gbs - gstots + FwdSumr + gbspsp - gIstots) + mult_q * xcssbi,
    );
    ckt.real(sp, s, -(mult_i * (gspr + gstot)));
    ckt.imag(sp, bp, mult_q * xcsbbr - mult_i * Gmbsi);
    ckt.real(
        sp,
        bp,
        -(mult_i * (gjbs + gstotb + Gmbsr - gbspb + gIstotb) - mult_q * xcsbbi),
    );

    ckt.real(s, sp, -(mult_i * (gspr - gstots)));
    ckt.real(s, s, mult_i * (gspr + gstot));

    ckt.imag(bp, dp, mult_q * xcbdb);
    ckt.real(bp, dp, -(mult_i * (gjbd - gbbdp + gIbtotd)));
    ckt.imag(bp, gp, mult_q * xcbgb);
    ckt.real(bp, gp, -(mult_i * (h.gbgs + gIbtotg)));
    ckt.imag(bp, sp, mult_q * xcbsb);
    ckt.real(bp, sp, -(mult_i * (gjbs - gbbsp + gIbtots)));
    ckt.imag(bp, bp, mult_q * xcbbb);
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
        ckt.imag(dp, db, mult_q * xcdbdb);
        ckt.real(dp, db, -(mult_i * h.gbd));
        ckt.imag(sp, sb, mult_q * xcsbsb);
        ckt.real(sp, sb, -(mult_i * h.gbs));

        ckt.imag(db, dp, mult_q * xcdbdb);
        ckt.real(db, dp, -(mult_i * h.gbd));
        ckt.imag(db, db, -(mult_q * xcdbdb));
        ckt.real(db, db, mult_i * (h.gbd + h.grbpd + h.grbdb));
        ckt.real(db, bp, -(mult_i * h.grbpd));
        ckt.real(db, b, -(mult_i * h.grbdb));

        ckt.real(bp, db, -(mult_i * h.grbpd));
        ckt.real(bp, b, -(mult_i * h.grbpb));
        ckt.real(bp, sb, -(mult_i * h.grbps));
        ckt.real(bp, bp, mult_i * (h.grbpd + h.grbps + h.grbpb));
        // WDLiu: (-here->BSIM4gbbs) already added to BPbpPtr

        ckt.imag(sb, sp, mult_q * xcsbsb);
        ckt.real(sb, sp, -(mult_i * h.gbs));
        ckt.real(sb, bp, -(mult_i * h.grbps));
        ckt.real(sb, b, -(mult_i * h.grbsb));
        ckt.imag(sb, sb, -(mult_q * xcsbsb));
        ckt.real(sb, sb, mult_i * (h.gbs + h.grbps + h.grbsb));

        ckt.real(b, db, -(mult_i * h.grbdb));
        ckt.real(b, bp, -(mult_i * h.grbpb));
        ckt.real(b, sb, -(mult_i * h.grbsb));
        ckt.real(b, b, mult_i * (h.grbsb + h.grbdb + h.grbpb));
    }

    // WDLiu: The internal charge node generated for transient NQS is not
    // needed for AC NQS. The following is not doing a real job, but we have
    // to keep it; otherwise a singular AC NQS matrix may occur if the
    // transient NQS is on. The charge node is isolated from the instance.
    if h.trnqsMod != 0 {
        ckt.real(q, q, mm * 1.0);
        ckt.real(q, gp, 0.0);
        ckt.real(q, dp, 0.0);
        ckt.real(q, sp, 0.0);
        ckt.real(q, bp, 0.0);

        ckt.real(dp, q, 0.0);
        ckt.real(sp, q, 0.0);
        ckt.real(gp, q, 0.0);
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::host::{SmallSignalCkt, node};
    use super::{Instance, Model, SizeParam};
    use std::collections::BTreeSet;

    /// Records every stamp, by kind.
    #[derive(Default)]
    pub(crate) struct Recorder {
        pub(crate) omega: f64,
        pub(crate) state: Vec<f64>,
        pub(crate) real: Vec<(i32, i32, f64)>,
        pub(crate) imag: Vec<(i32, i32, f64)>,
        pub(crate) cap: Vec<(i32, i32, f64)>,
    }

    impl SmallSignalCkt for Recorder {
        fn omega(&self) -> f64 {
            self.omega
        }
        fn state0(&self, slot: usize) -> f64 {
            self.state.get(slot).copied().unwrap_or(0.0)
        }
        fn real(&mut self, row: i32, col: i32, value: f64) {
            self.real.push((row, col, value));
        }
        fn imag(&mut self, row: i32, col: i32, value: f64) {
            self.imag.push((row, col, value));
        }
        fn cap(&mut self, row: i32, col: i32, value: f64) {
            self.cap.push((row, col, value));
        }
    }

    impl Recorder {
        pub(crate) fn positions(&self) -> BTreeSet<(i32, i32)> {
            self.real
                .iter()
                .chain(&self.imag)
                .chain(&self.cap)
                .map(|&(r, c, _)| (r, c))
                .collect()
        }
    }

    /// An instance with node fields as `b4set.c` leaves them.
    pub(crate) fn shaped(rgate: i32, rbody: i32, trnqs: i32) -> Instance {
        let created = rbody != 0;
        Instance {
            dNode: node::D,
            gNodeExt: node::G_EXT,
            sNode: node::S,
            bNode: node::B,
            dNodePrime: node::DP,
            sNodePrime: node::SP,
            gNodePrime: if rgate > 0 { node::GP } else { node::G_EXT },
            gNodeMid: if rgate == 3 { node::GM } else { node::G_EXT },
            dbNode: if created { node::DB } else { node::B },
            bNodePrime: if created { node::BP } else { node::B },
            sbNode: if created { node::SB } else { node::B },
            qNode: if trnqs != 0 { node::Q } else { node::GROUND },
            rgateMod: rgate,
            rbodyMod: rbody,
            trnqsMod: trnqs,
            m: 1.0,
            mult_i: 1.0,
            mult_q: 1.0,
            ..Instance::default()
        }
    }

    /// Every stamp lands on an element `b4set.c` allocates.
    pub(crate) fn check_shapes(
        run: impl Fn(&Model, &Instance, &mut Recorder),
        expect_cap: bool,
        charge_node: bool,
    ) {
        for rgate in 0..=3 {
            for rbody in [0, 1] {
                for trnqs in [0, 1] {
                    for rds in [0, 1] {
                        let h = shaped(rgate, rbody, trnqs);
                        let m = Model {
                            rdsMod: rds,
                            ..Model::default()
                        };
                        let allowed: BTreeSet<_> = super::super::elements::matrix_elements(&m, &h)
                            .into_iter()
                            .collect();
                        let mut ckt = Recorder {
                            omega: 1e6,
                            ..Recorder::default()
                        };
                        run(&m, &h, &mut ckt);
                        let used = ckt.positions();
                        assert!(
                            used.is_subset(&allowed),
                            "rgate {rgate} rbody {rbody}: {:?}",
                            used.difference(&allowed).collect::<Vec<_>>()
                        );
                        assert_eq!(ckt.cap.is_empty(), !expect_cap);
                        assert_eq!(ckt.imag.is_empty(), expect_cap);
                        assert!(used.contains(&(h.gNodePrime, h.gNodePrime)));
                        assert_eq!(used.contains(&(node::DB, node::DB)), rbody != 0);
                        assert_eq!(
                            used.contains(&(node::Q, node::Q)),
                            charge_node && trnqs != 0
                        );
                        assert_eq!(used.contains(&(node::GM, node::GM)), rgate == 3);
                        assert_eq!(used.contains(&(h.dNode, h.gNodePrime)), rds != 0);
                    }
                }
            }
        }
    }

    #[test]
    fn ac_stamps_land_on_allocated_elements() {
        check_shapes(
            |m, h, ckt| super::ac_load(m, h, &SizeParam::default(), ckt).unwrap(),
            false,
            true,
        );
    }

    #[test]
    fn ac_gate_capacitance_is_scaled_by_omega() {
        let mut h = shaped(0, 0, 0);
        h.cggb = 2e-15;
        h.cgdo = 1e-16;
        let mut ckt = Recorder {
            omega: 1e6,
            ..Recorder::default()
        };
        super::ac_load(&Model::default(), &h, &SizeParam::default(), &mut ckt).unwrap();
        let gg = ckt
            .imag
            .iter()
            .find(|&&(r, c, _)| (r, c) == (h.gNodePrime, h.gNodePrime))
            .unwrap()
            .2;
        assert_eq!(gg, (2e-15 + 1e-16 + 0.0 + 0.0) * 1e6);
    }
}
