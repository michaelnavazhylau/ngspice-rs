//! BSIM3v3.3 charge (C-V) model, a port of `b3ld.c`'s charge block.
//!
//! This ports the `capMod == 3` path (C's default, the "charge thickness"
//! model): the C-V preamble (`VbseffCV`, `CoxWL`, the `noff`/`voffcv`
//! `Vgsteff`), the CTM charge/capacitance block, the bias-dependent `capMod
//! >= 2` overlap capacitances and the source/drain junction C-V. `capMod` 0,
//! 1 and 2 and every NQS mode are explicit refusals: each is a different
//! equation set, not a tuning of this one.
//!
//! Every output keeps C's sign convention: charges on the gate, bulk and
//! drain and the nine `c*xb` capacitance derivatives, before the multiplicity
//! multiplier and before the overlap-charge adjustment the caller performs.
use super::dc::{Bias, DcEvaluation};
use super::defaults::ModelDefaults;
use super::setup::InstanceSetup;
use super::temperature::TemperatureModel;
use crate::primitives::{Real, SpiceError, SpiceResult};

const EXP_THRESHOLD: Real = 34.0;
const MIN_EXP: Real = 1.713_908_431e-15;
const MAX_EXP: Real = 5.834_617_425e14;
/// `b3ld.c` `EPSSI` (rounded, not the platform's permittivity).
const EPS_SI: Real = 1.035_94e-10;
/// `b3ld.c` `DELTA_1`.
const DELTA_1: Real = 0.02;
/// `b3ld.c` `DELTA_3`.
const DELTA_3: Real = 0.02;
/// `b3ld.c` `DELTA_4`.
const DELTA_4: Real = 0.02;

/// Intrinsic charges, the nine capacitance derivatives and the junction C-V
/// of one `b3ld.c` charge evaluation at a fixed bias.
///
/// Charges are C's instance values before the multiplicity multiplier and
/// before the overlap-charge (`qgdo`/`qgso`/`cgbo`) adjustment: `qgate`,
/// `qbulk` and `qdrn` are the CTM results, exactly as C has them when the
/// `capMod == 3` block ends. `qbs`/`qbd` are C's `CKTstate0` junction charges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeEvaluation {
    /// Intrinsic gate charge (C `qgate` at the end of the CTM block).
    pub qgate: Real,
    /// Intrinsic bulk charge (C `qbulk`).
    pub qbulk: Real,
    /// Intrinsic drain charge (C `qdrn`).
    pub qdrn: Real,
    /// Intrinsic source charge (C `qsrc`, from the XPART partition).
    pub qsrc: Real,
    /// C `here->BSIM3cggb`.
    pub cggb: Real,
    /// C `here->BSIM3cgdb`.
    pub cgdb: Real,
    /// C `here->BSIM3cgsb`.
    pub cgsb: Real,
    /// C `here->BSIM3cdgb`.
    pub cdgb: Real,
    /// C `here->BSIM3cddb`.
    pub cddb: Real,
    /// C `here->BSIM3cdsb`.
    pub cdsb: Real,
    /// C `here->BSIM3cbgb`.
    pub cbgb: Real,
    /// C `here->BSIM3cbdb`.
    pub cbdb: Real,
    /// C `here->BSIM3cbsb`.
    pub cbsb: Real,
    /// Bias-dependent gate-drain overlap capacitance (C `here->BSIM3cgdo`).
    pub cgdo: Real,
    /// Bias-dependent gate-source overlap capacitance (C `here->BSIM3cgso`).
    pub cgso: Real,
    /// Gate-bulk overlap capacitance (C `pParam->BSIM3cgbo`).
    pub cgbo: Real,
    /// Gate-drain overlap charge (C `qgdo`), for the gate/drain adjustment.
    pub qgdo: Real,
    /// Gate-source overlap charge (C `qgso`).
    pub qgso: Real,
    /// Source junction capacitance (C `here->BSIM3capbs`).
    pub capbs: Real,
    /// Drain junction capacitance (C `here->BSIM3capbd`).
    pub capbd: Real,
    /// Source junction charge (C `CKTstate0[here->BSIM3qbs]`).
    pub qbs: Real,
    /// Drain junction charge (C `CKTstate0[here->BSIM3qbd]`).
    pub qbd: Real,
    /// Inversion charge for the noise model (C `here->BSIM3qinv`).
    pub qinv: Real,
}

impl ChargeEvaluation {
    /// C's overlap-charge adjustment and `qd`/`qb` derivation, in C's order.
    ///
    /// `mode` is the DC mode (>= 0 normal, < 0 inverse). Returns
    /// `CKTstate0`'s `qg`, `qd` and `qb` (C's `qgate`/`qdrn` are updated in
    /// place because the terminal currents read them).
    #[must_use]
    pub fn with_overlap(&self, mode: i8, vgb: Real) -> OverlapCharges {
        let qgd = self.qgdo;
        let qgs = self.qgso;
        let qgb = self.cgbo * vgb;
        let qgate = self.qgate + qgd + qgs + qgb;
        let qbulk = self.qbulk - qgb;
        let (qdrn, qsrc) = if mode >= 0 {
            let qdrn = self.qdrn - qgd;
            (qdrn, -(qgate + qbulk + qdrn))
        } else {
            let qsrc = self.qdrn - qgs;
            (-(qgate + qbulk + qsrc), qsrc)
        };
        OverlapCharges {
            qgate,
            qbulk,
            qdrn,
            qsrc,
            // C stores the state after the adjustment; the junction charges
            // are already on the bulk/drain nodes.
            qg: qgate,
            qd: qdrn - self.qbd,
            qb: qbulk + self.qbd + self.qbs,
        }
    }
}

/// C's `qgate`/`qbulk`/`qdrn`/`qsrc` after the overlap-charge adjustment and
/// the `CKTstate0` charge values `b3ask.c` reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlapCharges {
    /// Gate charge including overlaps and gate-bulk.
    pub qgate: Real,
    /// Bulk charge including gate-bulk.
    pub qbulk: Real,
    /// Drain charge including the gate-drain overlap.
    pub qdrn: Real,
    /// Source charge (C solves it from the node-charge sum).
    pub qsrc: Real,
    /// C `CKTstate0[here->BSIM3qg]`.
    pub qg: Real,
    /// C `CKTstate0[here->BSIM3qd]`.
    pub qd: Real,
    /// C `CKTstate0[here->BSIM3qb]`.
    pub qb: Real,
}

/// The BSIM3 charge core for one defaulted model and one set-up instance.
#[derive(Debug, Clone, Copy)]
pub struct ChargeModel<'a> {
    /// The defaulted model (`b3set.c`).
    pub model: &'a ModelDefaults,
    /// The model-level temperature pass (`b3temp.c`).
    pub temperature: &'a TemperatureModel,
    /// This instance's effective parameters (`b3temp.c`).
    pub setup: &'a InstanceSetup,
}

impl ChargeModel<'_> {
    /// C's `capMod` mode after `b3set.c` defaults.
    fn cap_mod(&self) -> i32 {
        self.model.get("capmod").unwrap_or(3.).round() as i32
    }

    /// Evaluate the charge model at `bias`, reusing the DC evaluation's
    /// intermediates.
    ///
    /// # Errors
    /// A `capMod` other than 3 or any NQS mode (`NotYetPorted` naming the C
    /// file), or a nonfinite result.
    ///
    /// # Panics
    /// If a field or parameter this port names is absent from the model or
    /// instance setup: a schema bug in the port, not a deck error.
    pub fn evaluate(&self, dc: &DcEvaluation, bias: Bias) -> SpiceResult<ChargeEvaluation> {
        let cap_mod = self.cap_mod();
        if cap_mod != 3 {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "BSIM3 capMod={cap_mod} charge model (only capMod=3 is ported; \
                     set capMod=3 or omit it)"
                ),
                "src/spicelib/devices/bsim3/b3ld.c (capMod 0/1/2 charge blocks)",
            ));
        }
        if self.setup.nqs_mod != 0 || self.setup.acnqs_mod != 0 {
            return Err(SpiceError::not_yet_ported(
                "BSIM3 NQS charge model (nqsMod/acnqsMod)",
                "src/spicelib/devices/bsim3/b3ld.c (NQS charge partition and qdef)",
            ));
        }
        let p = self.setup;
        let m = self.model;
        let t = self.temperature;
        let number = |name: &str| -> Real {
            m.get(name)
                .unwrap_or_else(|| panic!("BSIM3 charge model field {name}"))
        };
        let param = |name: &str| -> Real {
            p.pparam(name)
                .unwrap_or_else(|| panic!("BSIM3 charge instance parameter {name}"))
        };
        let cv = &dc.cv;
        // C's junction C-V and overlap charges use the raw terminal bias;
        // the CTM block uses the mode-selected magnitudes (`Vgs`, `Vds`).
        let vbs = bias.vbs;
        let vgs = bias.vgs;
        let vds = bias.vds;
        let vds_mode = dc.vds;

        // ---- b3ld.c C-V preamble: VbseffCV, CoxWL and the noff VgsteffCV.
        let (vbseff_cv, dvbseff_cv_dvb) = if cv.vbseff < 0.0 {
            (cv.vbseff, 1.0)
        } else {
            // C `VbseffCV = phi - Phis`.
            (cv.phi - cv.phis, -cv.d_phis_dvb)
        };
        let cox = m.oxide_capacitance;
        let cox_wl = cox * p.weff_cv * p.leff_cv;

        let noff = cv.n * param("noff");
        let dnoff_dvd = param("noff") * cv.d_n_dvd;
        let dnoff_dvb = param("noff") * cv.d_n_dvb;
        let t0 = cv.vtm * noff;
        let voffcv = param("voffcv");
        let vgst_nvt = (cv.vgst - voffcv) / t0;
        let (vgsteff, dvgsteff_dvg, dvgsteff_dvd, dvgsteff_dvb) = if vgst_nvt > EXP_THRESHOLD {
            (
                cv.vgst - voffcv,
                cv.d_vgs_eff_dvg,
                -cv.d_vth_dvd,
                -cv.d_vth_dvb,
            )
        } else if vgst_nvt < -EXP_THRESHOLD {
            let vgsteff = t0 * (1.0 + MIN_EXP).ln();
            let dvd = vgsteff / noff;
            (vgsteff, 0.0, dvd * dnoff_dvd, dvd * dnoff_dvb)
        } else {
            let exp_vgst = vgst_nvt.exp();
            let vgsteff = t0 * (1.0 + exp_vgst).ln();
            let dvg = exp_vgst / (1.0 + exp_vgst);
            let dvd = -dvg * (cv.d_vth_dvd + (cv.vgst - voffcv) / noff * dnoff_dvd)
                + vgsteff / noff * dnoff_dvd;
            let dvb = -dvg * (cv.d_vth_dvb + (cv.vgst - voffcv) / noff * dnoff_dvb)
                + vgsteff / noff * dnoff_dvb;
            (vgsteff, dvg * cv.d_vgs_eff_dvg, dvd, dvb)
        };

        // ---- capMod == 3, the charge-thickness model (CTM).
        let vfbzb = p.vfbzb;
        let v3 = vfbzb - cv.vgs_eff + vbseff_cv - DELTA_3;
        let t0 = if vfbzb <= 0.0 {
            (v3 * v3 - 4.0 * DELTA_3 * vfbzb).sqrt()
        } else {
            (v3 * v3 + 4.0 * DELTA_3 * vfbzb).sqrt()
        };
        let t1 = 0.5 * (1.0 + v3 / t0);
        let vfbeff = vfbzb - 0.5 * (v3 + t0);
        let dvfbeff_dvg = t1 * cv.d_vgs_eff_dvg;
        let dvfbeff_dvb = -t1 * dvbseff_cv_dvb;

        let tox = number("tox");
        // C `Tox = 1.0e8 * model->BSIM3tox` (centimetres for the ACDE exponent).
        let mut tox_cm = 1.0e8 * tox;
        let t0 = (cv.vgs_eff - vbseff_cv - vfbzb) / tox_cm;
        let dt0_dvg = cv.d_vgs_eff_dvg / tox_cm;
        let dt0_dvb = -dvbseff_cv_dvb / tox_cm;

        let acde = param("acde");
        let ldeb = param("ldeb");
        let tmp = t0 * acde;
        let (tcen, mut dtcen_dvg, mut dtcen_dvb) = if tmp > -EXP_THRESHOLD && tmp < EXP_THRESHOLD {
            let tcen = ldeb * tmp.exp();
            let mut dvg = acde * tcen;
            let dvb = dvg * dt0_dvb;
            dvg *= dt0_dvg;
            (tcen, dvg, dvb)
        } else if tmp <= -EXP_THRESHOLD {
            (ldeb * MIN_EXP, 0.0, 0.0)
        } else {
            (ldeb * MAX_EXP, 0.0, 0.0)
        };

        let link = 1.0e-3 * tox;
        // C reuses V3/V4/T1 here, after the Vfb-eff computation.
        let v3 = ldeb - tcen - link;
        let v4 = (v3 * v3 + 4.0 * link * ldeb).sqrt();
        let tcen = ldeb - 0.5 * (v3 + v4);
        let t1 = 0.5 * (1.0 + v3 / v4);
        dtcen_dvg *= t1;
        dtcen_dvb *= t1;

        let ccen = EPS_SI / tcen;
        let t2 = cox / (cox + ccen);
        let coxeff = t2 * ccen;
        let t3 = -ccen / tcen;
        let mut dcoxeff_dvg = t2 * t2 * t3;
        let dcoxeff_dvb = dcoxeff_dvg * dtcen_dvb;
        dcoxeff_dvg *= dtcen_dvg;
        let cox_wl_cen = cox_wl * coxeff / cox;

        let qac0 = cox_wl_cen * (vfbeff - vfbzb);
        let qov_cox = qac0 / coxeff;
        let dqac0_dvg = cox_wl_cen * dvfbeff_dvg + qov_cox * dcoxeff_dvg;
        let dqac0_dvb = cox_wl_cen * dvfbeff_dvb + qov_cox * dcoxeff_dvb;

        // Poly-gate depletion share of the bulk charge.
        let k1ox = param("k1ox");
        let t0 = 0.5 * k1ox;
        let t3 = cv.vgs_eff - vfbeff - vbseff_cv - vgsteff;
        let (t1, t2) = if k1ox == 0.0 {
            (0.0, 0.0)
        } else if t3 < 0.0 {
            (t0 + t3 / k1ox, cox_wl_cen)
        } else {
            let t1 = (t0 * t0 + t3).sqrt();
            (t1, cox_wl_cen * t0 / t1)
        };

        let qsub0 = cox_wl_cen * k1ox * (t1 - t0);
        let qov_cox = qsub0 / coxeff;
        let dqsub0_dvg =
            t2 * (cv.d_vgs_eff_dvg - dvfbeff_dvg - dvgsteff_dvg) + qov_cox * dcoxeff_dvg;
        let dqsub0_dvd = -t2 * dvgsteff_dvd;
        let dqsub0_dvb =
            -t2 * (dvfbeff_dvb + dvbseff_cv_dvb + dvgsteff_dvb) + qov_cox * dcoxeff_dvb;

        // Gate-bias-dependent surface potential (C's "Delta Phis").
        let (denomi, t0) = if k1ox <= 0.0 {
            (0.25 * param("moin") * cv.vtm, 0.5 * param("sqrtphi"))
        } else {
            (
                param("moin") * cv.vtm * k1ox * k1ox,
                k1ox * param("sqrtphi"),
            )
        };
        let t1 = 2.0 * t0 + vgsteff;
        let delta_phi = cv.vtm * (1.0 + t1 * vgsteff / denomi).ln();
        let ddelta_phi_dvg = 2.0 * cv.vtm * (t1 - t0) / (denomi + t1 * vgsteff);

        // Effective gate overdrive, `VgDP`.
        let t0 = vgsteff - delta_phi - 0.001;
        let dt0_dvg = 1.0 - ddelta_phi_dvg;
        let t1 = (t0 * t0 + vgsteff * 0.004).sqrt();
        let vgdp = 0.5 * (t0 + t1);
        let dvgdp_dvg = 0.5 * (dt0_dvg + (t0 * dt0_dvg + 0.002) / t1);

        // Bias-dependent channel thickness for the inversion charge.
        let t3 = 4.0 * (cv.vth - vfbzb - cv.phi);
        tox_cm += tox_cm;
        let (t0, dt0_dvd, dt0_dvb) = if t3 >= 0.0 {
            (
                (vgsteff + t3) / tox_cm,
                (dvgsteff_dvd + 4.0 * cv.d_vth_dvd) / tox_cm,
                (dvgsteff_dvb + 4.0 * cv.d_vth_dvb) / tox_cm,
            )
        } else {
            (
                (vgsteff + 1.0e-20) / tox_cm,
                dvgsteff_dvd / tox_cm,
                dvgsteff_dvb / tox_cm,
            )
        };
        let tmp = (0.7 * t0.ln()).exp();
        let t1 = 1.0 + tmp;
        let t2 = 0.7 * tmp / (t0 * tox_cm);
        let tcen = 1.9e-9 / t1;
        let mut dtcen_dvg2 = -1.9e-9 * t2 / t1 / t1;
        let mut dtcen_dvd = tox_cm * dtcen_dvg2;
        let dtcen_dvb2 = dtcen_dvd * dt0_dvb;
        dtcen_dvd *= dt0_dvd;
        dtcen_dvg2 *= dvgsteff_dvg;

        let ccen = EPS_SI / tcen;
        let t0 = cox / (cox + ccen);
        let coxeff = t0 * ccen;
        let t1 = -ccen / tcen;
        let mut dcoxeff_dvg = t0 * t0 * t1;
        let dcoxeff_dvd = dcoxeff_dvg * dtcen_dvd;
        let dcoxeff_dvb = dcoxeff_dvg * dtcen_dvb2;
        dcoxeff_dvg *= dtcen_dvg2;
        let cox_wl_cen = cox_wl * coxeff / cox;

        // Effective drain voltage for C-V.
        let abulk_cv = cv.abulk0 * param("abulkcvfactor");
        let dabulk_cv_dvb = param("abulkcvfactor") * cv.d_abulk0_dvb;
        let vdsat_cv = vgdp / abulk_cv;
        let t0 = vdsat_cv - vds_mode - DELTA_4;
        let dt0_dvg = dvgdp_dvg / abulk_cv;
        let dt0_dvb = -vdsat_cv * dabulk_cv_dvb / abulk_cv;
        let t1 = (t0 * t0 + 4.0 * DELTA_4 * vdsat_cv).sqrt();
        let dt1_dvg0 = (t0 + DELTA_4 + DELTA_4) / t1;
        let dt1_dvd = -t0 / t1;
        let dt1_dvb0 = dt1_dvg0 * dt0_dvb;
        let dt1_dvg = dt1_dvg0 * dt0_dvg;
        let (mut vdseff_cv, mut dvdseff_cv_dvg, dvdseff_cv_dvd, mut dvdseff_cv_dvb) = if t0 >= 0.0 {
            (
                vdsat_cv - 0.5 * (t0 + t1),
                0.5 * (dt0_dvg - dt1_dvg),
                0.5 * (1.0 - dt1_dvd),
                0.5 * (dt0_dvb - dt1_dvb0),
            )
        } else {
            let t3 = (DELTA_4 + DELTA_4) / (t1 - t0);
            let t4 = 1.0 - t3;
            let t5 = vdsat_cv * t3 / (t1 - t0);
            (
                vdsat_cv * t4,
                dt0_dvg * t4 + t5 * (dt1_dvg - dt0_dvg),
                t5 * (dt1_dvd + 1.0),
                dt0_dvb * (1.0 - t5) + t5 * dt1_dvb0,
            )
        };
        // C zeroes VdseffCV's gate/body derivatives at Vds = 0 (not the drain
        // one, which the published code leaves alone).
        if vds_mode == 0.0 {
            vdseff_cv = 0.0;
            dvdseff_cv_dvg = 0.0;
            dvdseff_cv_dvb = 0.0;
        }

        // Gate charge and its derivatives.
        let t0 = abulk_cv * vdseff_cv;
        let t1 = vgdp;
        let t2 = 12.0 * (t1 - 0.5 * t0 + 1.0e-20);
        let t3 = t0 / t2;
        let t4 = 1.0 - 12.0 * t3 * t3;
        let t5 = abulk_cv * (6.0 * t0 * (4.0 * t1 - t0) / (t2 * t2) - 0.5);
        let t6 = t5 * vdseff_cv / abulk_cv;

        let qgate = cox_wl_cen * (t1 - t0 * (0.5 - t3));
        let qinoi = qgate;
        let qov_cox = qgate / coxeff;
        let mut cgg1 = cox_wl_cen * (t4 * dvgdp_dvg + t5 * dvdseff_cv_dvg);
        let cgd1 = cox_wl_cen * t5 * dvdseff_cv_dvd + cgg1 * dvgsteff_dvd + qov_cox * dcoxeff_dvd;
        let cgb1 = cox_wl_cen * (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb)
            + cgg1 * dvgsteff_dvb
            + qov_cox * dcoxeff_dvb;
        cgg1 = cgg1 * dvgsteff_dvg + qov_cox * dcoxeff_dvg;

        // Bulk charge and its derivatives.
        let t7 = 1.0 - abulk_cv;
        let t8 = t2 * t2;
        let t9 = 12.0 * t7 * t0 * t0 / (t8 * abulk_cv);
        let t10 = t9 * dvgdp_dvg;
        let t11 = -t7 * t5 / abulk_cv;
        let t12 = -(t9 * t1 / abulk_cv + vdseff_cv * (0.5 - t0 / t2));

        let qbulk = cox_wl_cen * t7 * (0.5 * vdseff_cv - t0 * vdseff_cv / t2);
        let qov_cox = qbulk / coxeff;
        let mut cbg1 = cox_wl_cen * (t10 + t11 * dvdseff_cv_dvg);
        let cbd1 = cox_wl_cen * t11 * dvdseff_cv_dvd + cbg1 * dvgsteff_dvd + qov_cox * dcoxeff_dvd;
        let cbb1 = cox_wl_cen * (t11 * dvdseff_cv_dvb + t12 * dabulk_cv_dvb)
            + cbg1 * dvgsteff_dvb
            + qov_cox * dcoxeff_dvb;
        cbg1 = cbg1 * dvgsteff_dvg + qov_cox * dcoxeff_dvg;

        // Source charge partition (XPART: > 0.5 = 0/100, < 0.5 = 40/60).
        let xpart = number("xpart");
        let (qsrc, csg, csd, mut csb);
        if xpart > 0.5 {
            let qsrc0 = -cox_wl_cen * (t1 / 2.0 + t0 / 4.0 - 0.5 * t0 * t0 / t2);
            let qov_cox = qsrc0 / coxeff;
            let t2 = t2 + t2;
            let t3 = t2 * t2;
            let t7p = -(0.25 - 12.0 * t0 * (4.0 * t1 - t0) / t3);
            let t4p = -(0.5 + 24.0 * t0 * t0 / t3) * dvgdp_dvg;
            let t5p = t7p * abulk_cv;
            let t6p = t7p * vdseff_cv;
            let csg0 = cox_wl_cen * (t4p + t5p * dvdseff_cv_dvg);
            let csd0 =
                cox_wl_cen * t5p * dvdseff_cv_dvd + csg0 * dvgsteff_dvd + qov_cox * dcoxeff_dvd;
            let csb0 = cox_wl_cen * (t5p * dvdseff_cv_dvb + t6p * dabulk_cv_dvb)
                + csg0 * dvgsteff_dvb
                + qov_cox * dcoxeff_dvb;
            let csg1 = csg0 * dvgsteff_dvg + qov_cox * dcoxeff_dvg;
            qsrc = qsrc0;
            csg = csg1;
            csd = csd0;
            csb = csb0;
        } else if xpart < 0.5 {
            // 40/60 partition, C's default (`b3set.c` xpart = 0).
            let t2 = t2 / 12.0;
            let t3 = 0.5 * cox_wl_cen / (t2 * t2);
            let t4 =
                t1 * (2.0 * t0 * t0 / 3.0 + t1 * (t1 - 4.0 * t0 / 3.0)) - 2.0 * t0 * t0 * t0 / 15.0;
            let qsrc0 = -t3 * t4;
            let qov_cox = qsrc0 / coxeff;
            let t8 = 4.0 / 3.0 * t1 * (t1 - t0) + 0.4 * t0 * t0;
            let t5 =
                -2.0 * qsrc0 / t2 - t3 * (t1 * (3.0 * t1 - 8.0 * t0 / 3.0) + 2.0 * t0 * t0 / 3.0);
            let t6 = abulk_cv * (qsrc0 / t2 + t3 * t8);
            let t7 = t6 * vdseff_cv / abulk_cv;
            let csg0 = t5 * dvgdp_dvg + t6 * dvdseff_cv_dvg;
            let csd0 = csg0 * dvgsteff_dvd + t6 * dvdseff_cv_dvd + qov_cox * dcoxeff_dvd;
            let csb0 = csg0 * dvgsteff_dvb
                + t6 * dvdseff_cv_dvb
                + t7 * dabulk_cv_dvb
                + qov_cox * dcoxeff_dvb;
            let csg1 = csg0 * dvgsteff_dvg + qov_cox * dcoxeff_dvg;
            qsrc = qsrc0;
            csg = csg1;
            csd = csd0;
            csb = csb0;
        } else {
            let qsrc0 = -0.5 * qgate;
            qsrc = qsrc0;
            csg = -0.5 * cgg1;
            csd = -0.5 * cgd1;
            csb = -0.5 * cgb1;
        }

        // Fold the accumulation/subthreshold charges into gate and bulk.
        let qgate = qgate + qac0 + qsub0 - qbulk;
        let qbulk = qbulk - (qac0 + qsub0);
        let qdrn = -(qgate + qbulk + qsrc);

        let cbg = cbg1 - dqac0_dvg - dqsub0_dvg;
        let cbd = cbd1 - dqsub0_dvd;
        let mut cbb = cbb1 - dqac0_dvb - dqsub0_dvb;

        let cgg = cgg1 - cbg;
        let cgd = cgd1 - cbd;
        let mut cgb = cgb1 - cbb;
        cgb *= cv.d_vbseff_dvb;
        cbb *= cv.d_vbseff_dvb;
        csb *= cv.d_vbseff_dvb;

        let cggb = cgg;
        let cgsb = -(cgg + cgd + cgb);
        let cgdb = cgd;
        let cdgb = -(cgg + cbg + csg);
        let cdsb = cgg + cgd + cgb + cbg + cbd + cbb + csg + csd + csb;
        let cddb = -(cgd + cbd + csd);
        let cbgb = cbg;
        let cbsb = -(cbg + cbd + cbb);
        let cbdb = cbd;

        // ---- Overlap capacitances (capMod 3 uses the capMod >= 2 branch).
        let cgdo_param = param("cgdo");
        let cgso_param = param("cgso");
        let ck = param("ckappa");
        let vgd = vgs - vds;

        let t0 = vgd + DELTA_1;
        let t1 = (t0 * t0 + 4.0 * DELTA_1).sqrt();
        let t2 = 0.5 * (t0 - t1);
        let t3 = p.weff_cv * param("cgdl");
        let t4 = (1.0 - 4.0 * t2 / ck).sqrt();
        let cgdo = cgdo_param + t3 - t3 * (1.0 - 1.0 / t4) * (0.5 - 0.5 * t0 / t1);
        let qgdo = (cgdo_param + t3) * vgd - t3 * (t2 + 0.5 * ck * (t4 - 1.0));

        let t0 = vgs + DELTA_1;
        let t1 = (t0 * t0 + 4.0 * DELTA_1).sqrt();
        let t2 = 0.5 * (t0 - t1);
        let t3 = p.weff_cv * param("cgsl");
        let t4 = (1.0 - 4.0 * t2 / ck).sqrt();
        let cgso = cgso_param + t3 - t3 * (1.0 - 1.0 / t4) * (0.5 - 0.5 * t0 / t1);
        let qgso = (cgso_param + t3) * vgs - t3 * (t2 + 0.5 * ck * (t4 - 1.0));

        let cgbo = param("cgbo");

        // ---- Junction C-V (b3ld.c "bulk-drain and bulk-source depletion
        // capacitances"). C uses the un-temperature-scaled gate-sidewall field
        // (`BSIM3unitLengthGateSidewallJctCap`, not `...TempJctCap`); that
        // quirk is reproduced.
        let (czbd, czbdsw, czbdswg) =
            junction_zero_bias(t, m, p.drain_area, p.drain_perimeter, param("weff"));
        let (czbs, czbssw, czbsswg) =
            junction_zero_bias(t, m, p.source_area, p.source_perimeter, param("weff"));
        let mj = number("mj");
        let mjsw = number("mjsw");
        let mjswg = number("mjswg");
        let phib = t.get("phi_b").expect("phi_b");
        let phibsw = t.get("phi_bsw").expect("phi_bsw");
        let phibswg = t.get("phi_bswg").expect("phi_bswg");

        let vbd = vbs - vds;
        let (qbs, capbs) = junction_cv(
            vbs, czbs, czbssw, czbsswg, mj, mjsw, mjswg, phib, phibsw, phibswg,
        );
        let (qbd, capbd) = junction_cv(
            vbd, czbd, czbdsw, czbdswg, mj, mjsw, mjswg, phib, phibsw, phibswg,
        );

        let result = ChargeEvaluation {
            qgate,
            qbulk,
            qdrn,
            qsrc,
            cggb,
            cgdb,
            cgsb,
            cdgb,
            cddb,
            cdsb,
            cbgb,
            cbdb,
            cbsb,
            cgdo,
            cgso,
            cgbo,
            qgdo,
            qgso,
            capbs,
            capbd,
            qbs,
            qbd,
            qinv: -qinoi,
        };
        for (name, value) in [
            ("qgate", result.qgate),
            ("qbulk", result.qbulk),
            ("qdrn", result.qdrn),
            ("qsrc", result.qsrc),
            ("qbs", result.qbs),
            ("qbd", result.qbd),
            ("capbs", result.capbs),
            ("capbd", result.capbd),
            ("cggb", result.cggb),
            ("cgdb", result.cgdb),
            ("cgsb", result.cgsb),
            ("cdgb", result.cdgb),
            ("cddb", result.cddb),
            ("cdsb", result.cdsb),
            ("cbgb", result.cbgb),
            ("cbdb", result.cbdb),
            ("cbsb", result.cbsb),
            ("cgdo", result.cgdo),
            ("cgso", result.cgso),
        ] {
            if !value.is_finite() {
                return Err(SpiceError::Numerical {
                    context: "BSIM3 charge evaluation (bsim3/b3ld.c)".into(),
                    message: format!("{name} is not finite"),
                });
            }
        }
        Ok(result)
    }
}

/// `b3ld.c`'s zero-bias junction capacitances for one junction, split into the
/// bottom, sidewall and gate-sidewall parts.
fn junction_zero_bias(
    temperature: &TemperatureModel,
    model: &ModelDefaults,
    area: Real,
    perimeter: Real,
    weff: Real,
) -> (Real, Real, Real) {
    let czb = temperature
        .get("unit_area_temp_jct_cap")
        .expect("unit_area_temp_jct_cap")
        * area;
    let sidewall = temperature
        .get("unit_length_sidewall_temp_jct_cap")
        .expect("unit_length_sidewall_temp_jct_cap");
    // C reads the *model* field here (no CJSWG temperature scaling).
    let gate_sidewall = model.get("cjswg").expect("cjswg");
    if perimeter < weff {
        (czb, 0.0, gate_sidewall * perimeter)
    } else {
        (czb, sidewall * (perimeter - weff), gate_sidewall * weff)
    }
}

/// `b3ld.c`'s source/drain bulk junction C-V for one junction, returning
/// `(charge, capacitance)`.
#[allow(clippy::too_many_arguments)] // C's three junction regions.
fn junction_cv(
    v: Real,
    czb: Real,
    czsw: Real,
    czswg: Real,
    mj: Real,
    mjsw: Real,
    mjswg: Real,
    phib: Real,
    phibsw: Real,
    phibswg: Real,
) -> (Real, Real) {
    if v == 0.0 {
        (0.0, czb + czsw + czswg)
    } else if v < 0.0 {
        let mut charge = 0.0;
        let mut cap = 0.0;
        for (cz, m, phi) in [
            (czb, mj, phib),
            (czsw, mjsw, phibsw),
            (czswg, mjswg, phibswg),
        ] {
            if cz > 0.0 {
                let arg = 1.0 - v / phi;
                let sarg = if m == 0.5 {
                    1.0 / arg.sqrt()
                } else {
                    (-m * arg.ln()).exp()
                };
                charge += phi * cz * (1.0 - arg * sarg) / (1.0 - m);
                cap += cz * sarg;
            }
        }
        (charge, cap)
    } else {
        let t0 = czb + czsw + czswg;
        let t1 = v * (czb * mj / phib + czsw * mjsw / phibsw + czswg * mjswg / phibswg);
        (v * (t0 + 0.5 * t1), t0 + t1)
    }
}
