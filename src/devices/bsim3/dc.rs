//! BSIM3v3.3 DC evaluation, a port of the bias-dependent part of `b3ld.c`.
//!
//! This is the physics core only: it takes terminal voltages and returns the
//! DC currents, conductances and observables C stores on the instance. It does
//! not limit voltages (`DEVfetlim`, `DEVpnjlim`, `DEVlimvds`), bypass points or
//! assemble matrix stamps — those belong to the solver loop and the device
//! implementation. Charges (`capMod`), NQS, AC and noise are not evaluated.
use super::defaults::ModelDefaults;
use super::setup::InstanceSetup;
use super::temperature::TemperatureModel;
use crate::primitives::{Real, SpiceResult};

const EXP_THRESHOLD: Real = 34.0;
const MIN_EXP: Real = 1.713_908_431e-15;
const MAX_EXP: Real = 5.834_617_425e14;
const EPS_SI: Real = 1.035_94e-10;
const CHARGE_Q: Real = 1.602_19e-19;

/// Terminal bias applied to the intrinsic device (the `dNodePrime`/
/// `sNodePrime` reference C uses), already multiplied by the model polarity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bias {
    /// Body-source voltage.
    pub vbs: Real,
    /// Gate-source voltage.
    pub vgs: Real,
    /// Drain-source voltage.
    pub vds: Real,
}

/// DC currents, conductances and observables from one `b3ld.c` evaluation.
///
/// Currents are the C instance values (`here->BSIM3cd`, `cbs`, ...) before the
/// multiplicity multiplier, which `*ask.c` applies separately.
#[derive(Debug, Clone, PartialEq)]
pub struct DcEvaluation {
    /// Drain current (C `here->BSIM3cd`).
    pub ids: Real,
    /// Transconductance (C `here->BSIM3gm`).
    pub gm: Real,
    /// Output conductance (C `here->BSIM3gds`).
    pub gds: Real,
    /// Body transconductance (C `here->BSIM3gmbs`).
    pub gmbs: Real,
    /// Substrate current (C `here->BSIM3csub`).
    pub isub: Real,
    /// Substrate-current derivatives (C `gbgs`, `gbds`, `gbbs`).
    pub gbgs: Real,
    /// Substrate-current drain derivative.
    pub gbds: Real,
    /// Substrate-current body derivative.
    pub gbbs: Real,
    /// Source junction current (C `here->BSIM3cbs`).
    pub cbs: Real,
    /// Drain junction current (C `here->BSIM3cbd`).
    pub cbd: Real,
    /// Source junction conductance, including gmin.
    pub gbs: Real,
    /// Drain junction conductance, including gmin.
    pub gbd: Real,
    /// Source-body voltage used, after mode selection.
    pub vbs: Real,
    /// Drain-body voltage computed as `vbs - vds`.
    pub vbd: Real,
    /// Gate-source voltage used, after mode selection.
    pub vgs: Real,
    /// Drain-source voltage used, after mode selection.
    pub vds: Real,
    /// Normal (+1) or inverse (-1) mode.
    pub mode: i8,
    /// Threshold voltage reported by `@m1[vth]` (C `here->BSIM3von`).
    pub von: Real,
    /// Effective gate overdrive.
    pub vgsteff: Real,
    /// Effective drain voltage.
    pub vdseff: Real,
    /// Saturation voltage.
    pub vdsat: Real,
    /// Bulk-charge factor.
    pub abulk: Real,
    /// Effective mobility.
    pub ueff: Real,
    /// Series resistance used.
    pub rds: Real,
    /// C `here->BSIM3thetavth`.
    pub thetavth: Real,
    /// C `here->BSIM3AbovVgst2Vtm`.
    pub abulk_over_vgst2vtm: Real,
}

/// The BSIM3 DC core for one defaulted model and one set-up instance.
#[derive(Debug, Clone, Copy)]
pub struct DcModel<'a> {
    /// The defaulted model (`b3set.c`).
    pub model: &'a ModelDefaults,
    /// The model-level temperature pass (`b3temp.c`).
    pub temperature: &'a TemperatureModel,
    /// This instance's effective parameters (`b3temp.c`).
    pub setup: &'a InstanceSetup,
}

impl DcModel<'_> {
    /// Evaluate the DC currents and conductances at `bias`.
    ///
    /// `gmin` is C's `CKTgmin`, added to both junction conductances.
    ///
    /// # Errors
    /// Missing internal parameters, or a nonfinite intermediate that C would
    /// silently propagate into the matrix.
    pub fn evaluate(&self, bias: Bias, gmin: Real) -> SpiceResult<DcEvaluation> {
        let p = self.setup;
        let m = self.model;
        let t = self.temperature;
        let model = |name: &str| -> Real {
            m.get(name)
                .unwrap_or_else(|| panic!("BSIM3 DC model field {name}"))
        };
        let param = |name: &str| -> Real {
            p.pparam(name)
                .unwrap_or_else(|| panic!("BSIM3 DC instance parameter {name}"))
        };
        let type_ = Real::from(p.polarity);
        let cox = m.oxide_capacitance;
        let tox = model("tox");
        let vtm = t.get("vtm").expect("vtm");
        let temp = t.get("temp").expect("temp");
        let tnom = t.get("tnom").expect("tnom");
        let factor1 = t.get("factor1").expect("factor1");

        // Source/drain junction diode DC model begins.
        let nvtm = vtm * model("nj");
        let source_sat_current = p.source_sat_current;
        let drain_sat_current = p.drain_sat_current;
        let vbd_in = bias.vbs - bias.vds;

        let junction = |sat: Real, v: Real, vjsm: Real, isevjsm: Real| -> (Real, Real) {
            if sat <= 0.0 {
                return (gmin, gmin * v);
            }
            // IJTH==0 and below-breakdown share C's exponential branch.
            if model("ijth") == 0.0 || v < vjsm {
                let ev = (v / nvtm).exp();
                (sat * ev / nvtm + gmin, sat * (ev - 1.0) + gmin * v)
            } else {
                let t0 = isevjsm / nvtm;
                (t0 + gmin, isevjsm - sat + t0 * (v - vjsm) + gmin * v)
            }
        };
        let (gbs, cbs) = junction(source_sat_current, bias.vbs, p.vjsm, p.isevjsm);
        let (gbd, cbd) = junction(drain_sat_current, vbd_in, p.vjdm, p.isevjdm);

        // Normal versus inverse mode. The inverse case mirrors gate/drain.
        let (mode, vds, vgs, vbs) = if bias.vds >= 0.0 {
            (1, bias.vds, bias.vgs, bias.vbs)
        } else {
            (-1, -bias.vds, bias.vgs - bias.vds, bias.vbs - bias.vds)
        };

        // Body-effect and depletion charge.
        let vbsc = param("vbsc");
        let phi = param("phi");
        let t0 = vbs - vbsc - 0.001;
        let t1 = (t0 * t0 - 0.004 * vbsc).sqrt();
        let vbseff_raw = vbsc + 0.5 * (t0 + t1);
        let dvbseff_dvb = 0.5 * (1.0 + t0 / t1);
        let vbseff = if vbseff_raw < vbs { vbs } else { vbseff_raw };
        let (sqrt_phis, dsqrt_phis_dvb);
        if vbseff > 0.0 {
            let t0 = phi / (phi + vbseff);
            let _phis = phi * t0;
            sqrt_phis = param("phis3") / (phi + 0.5 * vbseff);
            dsqrt_phis_dvb = -0.5 * sqrt_phis * sqrt_phis / param("phis3");
        } else {
            sqrt_phis = (phi - vbseff).sqrt();
            dsqrt_phis_dvb = -0.5 / sqrt_phis;
        }
        let xdep = param("xdep0") * sqrt_phis / param("sqrtphi");
        let dxdep_dvb = (param("xdep0") / param("sqrtphi")) * dsqrt_phis_dvb;

        let leff = param("leff");
        let v0 = param("vbi") - phi;
        let t3 = xdep.sqrt();

        // dvt2 body-bias dependence of the threshold.
        let (t1, t2) = {
            let t0 = param("dvt2") * vbseff;
            if t0 >= -0.5 {
                (1.0 + t0, param("dvt2"))
            } else {
                let t4 = 1.0 / (3.0 + 8.0 * t0);
                ((1.0 + 3.0 * t0) * t4, param("dvt2") * t4 * t4)
            }
        };
        let lt1 = factor1 * t3 * t1;
        let dlt1_dvb = factor1 * (0.5 / t3 * t1 * dxdep_dvb + t3 * t2);
        let (t1, t2) = {
            let t0 = param("dvt2w") * vbseff;
            if t0 >= -0.5 {
                (1.0 + t0, param("dvt2w"))
            } else {
                let t4 = 1.0 / (3.0 + 8.0 * t0);
                ((1.0 + 3.0 * t0) * t4, param("dvt2w") * t4 * t4)
            }
        };
        let ltw = factor1 * t3 * t1;
        let dltw_dvb = factor1 * (0.5 / t3 * t1 * dxdep_dvb + t3 * t2);

        // Narrow/short-channel threshold shifts.
        let (theta0, dtheta0_dvb) = {
            let t0 = -0.5 * param("dvt1") * leff / lt1;
            if t0 > -EXP_THRESHOLD {
                let t1 = t0.exp();
                let dt1_dvb = -t0 / lt1 * t1 * dlt1_dvb;
                (t1 * (1.0 + 2.0 * t1), (1.0 + 4.0 * t1) * dt1_dvb)
            } else {
                let t1 = MIN_EXP;
                (t1 * (1.0 + 2.0 * t1), 0.0)
            }
        };
        let thetavth = param("dvt0") * theta0;
        let delt_vth = thetavth * v0;
        let ddelt_vth_dvb = param("dvt0") * dtheta0_dvb * v0;

        let (t2w, dt2w_dvb) = {
            let t0 = -0.5 * param("dvt1w") * param("weff") * leff / ltw;
            if t0 > -EXP_THRESHOLD {
                let t1 = t0.exp();
                let dt1_dvb = -t0 / ltw * t1 * dltw_dvb;
                (t1 * (1.0 + 2.0 * t1), (1.0 + 4.0 * t1) * dt1_dvb)
            } else {
                let t1 = MIN_EXP;
                (t1 * (1.0 + 2.0 * t1), 0.0)
            }
        };
        let t0 = param("dvt0w") * t2w;
        let t2 = t0 * v0;
        let dt2_dvb = param("dvt0w") * dt2w_dvb * v0;

        let temp_ratio = temp / tnom - 1.0;
        let t0 = (1.0 + param("nlx") / leff).sqrt();
        let t1 = param("k1ox") * (t0 - 1.0) * param("sqrtphi")
            + (param("kt1") + param("kt1l") / leff + param("kt2") * vbseff) * temp_ratio;
        let tmp2 = tox * phi / (param("weff") + param("w0"));

        let t3eta = param("eta0") + param("etab") * vbseff;
        let (t3eta, t4) = if t3eta < 1.0e-4 {
            let t9 = 1.0 / (3.0 - 2.0e4 * t3eta);
            ((2.0e-4 - t3eta) * t9, t9 * t9)
        } else {
            (t3eta, 1.0)
        };
        let ddibl_sft_dvd = t3eta * param("theta0vb0");
        let dibl_sft = ddibl_sft_dvd * vds;

        // `here->BSIM3vth0` is the instance value after DELVTO.
        let vth = type_ * p.vth0 - param("k1") * param("sqrtphi") + param("k1ox") * sqrt_phis
            - param("k2ox") * vbseff
            - delt_vth
            - t2
            + (param("k3") + param("k3b") * vbseff) * tmp2
            + t1
            - dibl_sft;
        let von = vth;
        let dvth_dvb = param("k1ox") * dsqrt_phis_dvb - param("k2ox") - ddelt_vth_dvb - dt2_dvb
            + param("k3b") * tmp2
            - param("etab") * vds * param("theta0vb0") * t4
            + param("kt2") * temp_ratio;
        let dvth_dvd = -ddibl_sft_dvd;

        // Subthreshold slope factor n.
        let tmp2n = param("nfactor") * EPS_SI / xdep;
        let tmp3n = param("cdsc") + param("cdscb") * vbseff + param("cdscd") * vds;
        let tmp4 = (tmp2n + tmp3n * theta0 + param("cit")) / cox;
        let (n, dn_dvb, dn_dvd) = if tmp4 >= -0.5 {
            (
                1.0 + tmp4,
                (-tmp2n / xdep * dxdep_dvb + tmp3n * dtheta0_dvb + param("cdscb") * theta0) / cox,
                param("cdscd") * theta0 / cox,
            )
        } else {
            let t0 = 1.0 / (3.0 + 8.0 * tmp4);
            let factor = t0 * t0;
            (
                (1.0 + 3.0 * tmp4) * t0,
                (-tmp2n / xdep * dxdep_dvb + tmp3n * dtheta0_dvb + param("cdscb") * theta0) / cox
                    * factor,
                param("cdscd") * theta0 / cox * factor,
            )
        };

        // Poly-gate depletion.
        let t0 = p.vfb + phi;
        let (vgs_eff, dvgseff_dvg) =
            if param("ngate") > 1.0e18 && param("ngate") < 1.0e25 && vgs > t0 {
                let t1 = 1.0e6 * CHARGE_Q * EPS_SI * param("ngate") / (cox * cox);
                let t4 = (1.0 + 2.0 * (vgs - t0) / t1).sqrt();
                let t2 = t1 * (t4 - 1.0);
                let t3 = 0.5 * t2 * t2 / t1;
                let t7 = 1.12 - t3 - 0.05;
                let t6 = (t7 * t7 + 0.224).sqrt();
                let t5 = 1.12 - 0.5 * (t7 + t6);
                (vgs - t5, 1.0 - (0.5 - 0.5 / t4) * (1.0 + t7 / t6))
            } else {
                (vgs, 1.0)
            };
        let vgst = vgs_eff - vth;

        // Effective gate overdrive (Vgsteff), with C's three regimes.
        let t10 = 2.0 * n * vtm;
        let vgst_nvt = vgst / t10;
        let exp_arg = (2.0 * param("voff") - vgst) / t10;
        let (vgsteff, mut dvgsteff_dvg, dvgsteff_dvd, dvgsteff_dvb);
        if vgst_nvt > EXP_THRESHOLD {
            vgsteff = vgst;
            dvgsteff_dvg = dvgseff_dvg;
            dvgsteff_dvd = -dvth_dvd;
            dvgsteff_dvb = -dvth_dvb;
        } else if exp_arg > EXP_THRESHOLD {
            let t0 = (vgst - param("voff")) / (n * vtm);
            let exp_vgst = t0.exp();
            vgsteff = vtm * param("cdep0") / cox * exp_vgst;
            dvgsteff_dvg = vgsteff / (n * vtm);
            dvgsteff_dvd = -dvgsteff_dvg * (dvth_dvd + t0 * vtm * dn_dvd);
            dvgsteff_dvb = -dvgsteff_dvg * (dvth_dvb + t0 * vtm * dn_dvb);
            dvgsteff_dvg *= dvgseff_dvg;
        } else {
            let exp_vgst = vgst_nvt.exp();
            let t1 = t10 * (1.0 + exp_vgst).ln();
            let dt1_dvg = exp_vgst / (1.0 + exp_vgst);
            let dt1_dvb = -dt1_dvg * (dvth_dvb + vgst / n * dn_dvb) + t1 / n * dn_dvb;
            let dt1_dvd = -dt1_dvg * (dvth_dvd + vgst / n * dn_dvd) + t1 / n * dn_dvd;

            let dt2_dvg = -cox / (vtm * param("cdep0")) * exp_arg.exp();
            let t2v = 1.0 - t10 * dt2_dvg;
            let dt2_dvd =
                -dt2_dvg * (dvth_dvd - 2.0 * vtm * exp_arg * dn_dvd) + (t2v - 1.0) / n * dn_dvd;
            let dt2_dvb =
                -dt2_dvg * (dvth_dvb - 2.0 * vtm * exp_arg * dn_dvb) + (t2v - 1.0) / n * dn_dvb;

            vgsteff = t1 / t2v;
            let t3 = t2v * t2v;
            dvgsteff_dvg = (t2v * dt1_dvg - t1 * dt2_dvg) / t3 * dvgseff_dvg;
            dvgsteff_dvd = (t2v * dt1_dvd - t1 * dt2_dvd) / t3;
            dvgsteff_dvb = (t2v * dt1_dvb - t1 * dt2_dvb) / t3;
        }

        // Effective channel width.
        let t9 = sqrt_phis - param("sqrtphi");
        let mut weff = param("weff") - 2.0 * (param("dwg") * vgsteff + param("dwb") * t9);
        let mut dweff_dvg = -2.0 * param("dwg");
        let mut dweff_dvb = -2.0 * param("dwb") * dsqrt_phis_dvb;
        if weff < 2.0e-8 {
            let t0 = 1.0 / (6.0e-8 - 2.0 * weff);
            weff = 2.0e-8 * (4.0e-8 - weff) * t0;
            let factor = t0 * t0 * 4.0e-16;
            dweff_dvg *= factor;
            dweff_dvb *= factor;
        }

        // Series resistance.
        let t0 = param("prwg") * vgsteff + param("prwb") * t9;
        let (rds, drds_dvg, drds_dvb) = if t0 >= -0.9 {
            (
                param("rds0") * (1.0 + t0),
                param("rds0") * param("prwg"),
                param("rds0") * param("prwb") * dsqrt_phis_dvb,
            )
        } else {
            let t1 = 1.0 / (17.0 + 20.0 * t0);
            let t1s = t1 * t1;
            (
                param("rds0") * (0.8 + t0) * t1,
                param("rds0") * param("prwg") * t1s,
                param("rds0") * param("prwb") * dsqrt_phis_dvb * t1s,
            )
        };

        // Bulk-charge factor.
        let t1 = 0.5 * param("k1ox") / sqrt_phis;
        let dt1_dvb = -t1 / sqrt_phis * dsqrt_phis_dvb;
        let t9 = (param("xj") * xdep).sqrt();
        let tmp1 = leff + 2.0 * t9;
        let t5 = leff / tmp1;
        let tmp2a = param("a0") * t5;
        let tmp3a = param("weff") + param("b1");
        let tmp4a = param("b0") / tmp3a;
        let t2a = tmp2a + tmp4a;
        let dt2_dvb = -t9 / tmp1 / xdep * dxdep_dvb;
        let t6 = t5 * t5;
        let t7 = t5 * t6;
        let mut abulk0 = 1.0 + t1 * t2a;
        let mut dabulk0_dvb = t1 * tmp2a * dt2_dvb + t2a * dt1_dvb;
        let t8 = param("ags") * param("a0") * t7;
        let mut dabulk_dvg = -t1 * t8;
        let mut abulk = abulk0 + dabulk_dvg * vgsteff;
        let mut dabulk_dvb = dabulk0_dvb - t8 * vgsteff * (dt1_dvb + 3.0 * t1 * dt2_dvb);
        if abulk0 < 0.1 {
            let t9c = 1.0 / (3.0 - 20.0 * abulk0);
            abulk0 = (0.2 - abulk0) * t9c;
            dabulk0_dvb *= t9c * t9c;
        }
        if abulk < 0.1 {
            let t9c = 1.0 / (3.0 - 20.0 * abulk);
            abulk = (0.2 - abulk) * t9c;
            let t10c = t9c * t9c;
            dabulk_dvb *= t10c;
            dabulk_dvg *= t10c;
        }
        let t2k = param("keta") * vbseff;
        let (t0k, dt0_dvb) = if t2k >= -0.9 {
            let t0k = 1.0 / (1.0 + t2k);
            (t0k, -param("keta") * t0k * t0k)
        } else {
            let t1k = 1.0 / (0.8 + t2k);
            ((17.0 + 20.0 * t2k) * t1k, -param("keta") * t1k * t1k)
        };
        dabulk_dvg *= t0k;
        dabulk_dvb = dabulk_dvb * t0k + abulk * dt0_dvb;
        dabulk0_dvb = dabulk0_dvb * t0k + abulk0 * dt0_dvb;
        abulk *= t0k;
        let _ = (abulk0, dabulk0_dvb);

        // Mobility, by MOBMOD.
        let mut denomi_dvg;
        let mut denomi_dvd;
        let mut denomi_dvb;
        let t5m;
        if model("mobmod") == 1.0 {
            let t0 = vgsteff + vth + vth;
            let t2 = param("ua") + param("uc") * vbseff;
            let t3 = t0 / tox;
            t5m = t3 * (t2 + param("ub") * t3);
            denomi_dvg = (t2 + 2.0 * param("ub") * t3) / tox;
            denomi_dvd = denomi_dvg * 2.0 * dvth_dvd;
            denomi_dvb = denomi_dvg * 2.0 * dvth_dvb + param("uc") * t3;
        } else if model("mobmod") == 2.0 {
            t5m =
                vgsteff / tox * (param("ua") + param("uc") * vbseff + param("ub") * vgsteff / tox);
            denomi_dvg =
                (param("ua") + param("uc") * vbseff + 2.0 * param("ub") * vgsteff / tox) / tox;
            denomi_dvd = 0.0;
            denomi_dvb = vgsteff * param("uc") / tox;
        } else {
            let t0 = vgsteff + vth + vth;
            let t2 = 1.0 + param("uc") * vbseff;
            let t3 = t0 / tox;
            let t4 = t3 * (param("ua") + param("ub") * t3);
            t5m = t4 * t2;
            denomi_dvg = (param("ua") + 2.0 * param("ub") * t3) * t2 / tox;
            denomi_dvd = denomi_dvg * 2.0 * dvth_dvd;
            denomi_dvb = denomi_dvg * 2.0 * dvth_dvb + param("uc") * t4;
        }
        let denomi = if t5m >= -0.8 {
            1.0 + t5m
        } else {
            let t9c = 1.0 / (7.0 + 10.0 * t5m);
            let factor = t9c * t9c;
            denomi_dvg *= factor;
            denomi_dvd *= factor;
            denomi_dvb *= factor;
            (0.6 + t5m) * t9c
        };
        let u0temp = p.u0temp;
        let ueff = u0temp / denomi;
        let t9c = -ueff / denomi;
        let dueff_dvg = t9c * denomi_dvg;
        let dueff_dvd = t9c * denomi_dvd;
        let dueff_dvb = t9c * denomi_dvb;

        // Saturation drain voltage.
        let wvcox = weff * param("vsattemp") * cox;
        let wvcoxrds = wvcox * rds;
        let esat = 2.0 * param("vsattemp") / ueff;
        let esatl = esat * leff;
        let t0e = -esatl / ueff;
        let desatl_dvg = t0e * dueff_dvg;
        let desatl_dvd = t0e * dueff_dvd;
        let desatl_dvb = t0e * dueff_dvb;

        let a1v = param("a1");
        let a2v = param("a2");
        let (lambda, dlambda_dvg) = if a1v == 0.0 {
            (a2v, 0.0)
        } else if a1v > 0.0 {
            let t0 = 1.0 - a2v;
            let t1 = t0 - a1v * vgsteff - 0.0001;
            let t2 = (t1 * t1 + 0.0004 * t0).sqrt();
            (a2v + t0 - 0.5 * (t1 + t2), 0.5 * a1v * (1.0 + t1 / t2))
        } else {
            let t1 = a2v + a1v * vgsteff - 0.0001;
            let t2 = (t1 * t1 + 0.0004 * a2v).sqrt();
            (0.5 * (t1 + t2), 0.5 * a1v * (1.0 + t1 / t2))
        };

        let vgst2vtm = vgsteff + 2.0 * vtm;
        let abulk_over_vgst2vtm = abulk / vgst2vtm;
        let (tmp2v, tmp3v) = if rds > 0.0 {
            (
                drds_dvg / rds + dweff_dvg / weff,
                drds_dvb / rds + dweff_dvb / weff,
            )
        } else {
            (dweff_dvg / weff, dweff_dvb / weff)
        };

        let (vdsat, dvdsat_dvg, dvdsat_dvd, dvdsat_dvb, tmp1v);
        if rds == 0.0 && lambda == 1.0 {
            let t0 = 1.0 / (abulk * esatl + vgst2vtm);
            tmp1v = 0.0;
            let t1v = t0 * t0;
            let t2 = vgst2vtm * t0;
            let t3 = esatl * vgst2vtm;
            vdsat = t3 * t0;
            let dt0_dvg = -(abulk * desatl_dvg + esatl * dabulk_dvg + 1.0) * t1v;
            let dt0_dvd = -(abulk * desatl_dvd) * t1v;
            let dt0_dvb = -(abulk * desatl_dvb + dabulk_dvb * esatl) * t1v;
            dvdsat_dvg = t3 * dt0_dvg + t2 * desatl_dvg + esatl * t0;
            dvdsat_dvd = t3 * dt0_dvd + t2 * desatl_dvd;
            dvdsat_dvb = t3 * dt0_dvb + t2 * desatl_dvb;
        } else {
            tmp1v = dlambda_dvg / (lambda * lambda);
            let t9 = abulk * wvcoxrds;
            let t8 = abulk * t9;
            let t7 = vgst2vtm * t9;
            let t6 = vgst2vtm * wvcoxrds;
            let t0 = 2.0 * abulk * (t9 - 1.0 + 1.0 / lambda);
            let dt0_dvg =
                2.0 * (t8 * tmp2v - abulk * tmp1v + (2.0 * t9 + 1.0 / lambda - 1.0) * dabulk_dvg);
            let dt0_dvb =
                2.0 * (t8 * (2.0 / abulk * dabulk_dvb + tmp3v) + (1.0 / lambda - 1.0) * dabulk_dvb);
            let _dt0_dvd = 0.0;
            let t1v = vgst2vtm * (2.0 / lambda - 1.0) + abulk * esatl + 3.0 * t7;
            let dt1_dvg = (2.0 / lambda - 1.0) - 2.0 * vgst2vtm * tmp1v
                + abulk * desatl_dvg
                + esatl * dabulk_dvg
                + 3.0 * (t9 + t7 * tmp2v + t6 * dabulk_dvg);
            let dt1_dvb =
                abulk * desatl_dvb + esatl * dabulk_dvb + 3.0 * (t6 * dabulk_dvb + t7 * tmp3v);
            let dt1_dvd = abulk * desatl_dvd;
            let t2v = vgst2vtm * (esatl + 2.0 * t6);
            let dt2_dvg = esatl + vgst2vtm * desatl_dvg + t6 * (4.0 + 2.0 * vgst2vtm * tmp2v);
            let dt2_dvb = vgst2vtm * (desatl_dvb + 2.0 * t6 * tmp3v);
            let dt2_dvd = vgst2vtm * desatl_dvd;
            let t3 = (t1v * t1v - 2.0 * t0 * t2v).sqrt();
            vdsat = (t1v - t3) / t0;
            dvdsat_dvg =
                (dt1_dvg - (t1v * dt1_dvg - dt0_dvg * t2v - t0 * dt2_dvg) / t3 - vdsat * dt0_dvg)
                    / t0;
            dvdsat_dvb =
                (dt1_dvb - (t1v * dt1_dvb - dt0_dvb * t2v - t0 * dt2_dvb) / t3 - vdsat * dt0_dvb)
                    / t0;
            dvdsat_dvd = (dt1_dvd - (t1v * dt1_dvd - t0 * dt2_dvd) / t3) / t0;
        }

        // Effective drain voltage.
        let delta = param("delta");
        let t1d = vdsat - vds - delta;
        let dt1_dvg = dvdsat_dvg;
        let dt1_dvd = dvdsat_dvd - 1.0;
        let dt1_dvb = dvdsat_dvb;
        let t2d = (t1d * t1d + 4.0 * delta * vdsat).sqrt();
        let t0d = t1d / t2d;
        let t3d = 2.0 * delta / t2d;
        let dt2_dvg = t0d * dt1_dvg + t3d * dvdsat_dvg;
        let dt2_dvd = t0d * dt1_dvd + t3d * dvdsat_dvd;
        let dt2_dvb = t0d * dt1_dvb + t3d * dvdsat_dvb;
        let mut vdseff = vdsat - 0.5 * (t1d + t2d);
        let mut dvdseff_dvg = dvdsat_dvg - 0.5 * (dt1_dvg + dt2_dvg);
        let dvdseff_dvd = dvdsat_dvd - 0.5 * (dt1_dvd + dt2_dvd);
        let mut dvdseff_dvb = dvdsat_dvb - 0.5 * (dt1_dvb + dt2_dvb);
        if vds == 0.0 {
            vdseff = 0.0;
            dvdseff_dvg = 0.0;
            dvdseff_dvb = 0.0;
        }

        // Early voltage: saturation, CLM and DIBL components.
        let tmp4v = 1.0 - 0.5 * abulk * vdsat / vgst2vtm;
        let t9v = wvcoxrds * vgsteff;
        let t8v = t9v / vgst2vtm;
        let t0v = esatl + vdsat + 2.0 * t9v * tmp4v;
        let t7v = 2.0 * wvcoxrds * tmp4v;
        let dt0_dvg = desatl_dvg + dvdsat_dvg + t7v * (1.0 + tmp2v * vgsteff)
            - t8v * (abulk * dvdsat_dvg - abulk * vdsat / vgst2vtm + vdsat * dabulk_dvg);
        let dt0_dvb = desatl_dvb + dvdsat_dvb + t7v * tmp3v * vgsteff
            - t8v * (dabulk_dvb * vdsat + abulk * dvdsat_dvb);
        let dt0_dvd = desatl_dvd + dvdsat_dvd - t8v * abulk * dvdsat_dvd;
        let t9e = wvcoxrds * abulk;
        let t1e = 2.0 / lambda - 1.0 + t9e;
        let dt1_dvg = -2.0 * tmp1v + wvcoxrds * (abulk * tmp2v + dabulk_dvg);
        let dt1_dvb = dabulk_dvb * wvcoxrds + t9e * tmp3v;
        let vasat = t0v / t1e;
        let dvasat_dvg = (dt0_dvg - vasat * dt1_dvg) / t1e;
        let dvasat_dvb = (dt0_dvb - vasat * dt1_dvb) / t1e;
        let dvasat_dvd = dt0_dvd / t1e;
        if vdseff > vds {
            vdseff = vds;
        }
        let diff_vds = vds - vdseff;

        let (vaclm, dvaclm_dvg, dvaclm_dvd, dvaclm_dvb) =
            if param("pclm") > 0.0 && diff_vds > 1.0e-10 {
                let t0 = 1.0 / (param("pclm") * abulk * param("litl"));
                let dt0_dvb = -t0 / abulk * dabulk_dvb;
                let dt0_dvg = -t0 / abulk * dabulk_dvg;
                let t2 = vgsteff / esatl;
                let t1 = leff * (abulk + t2);
                let dt1_dvg = leff * ((1.0 - t2 * desatl_dvg) / esatl + dabulk_dvg);
                let dt1_dvb = leff * (dabulk_dvb - t2 * desatl_dvb / esatl);
                // b3ld.c divides by Esat (not EsatL) here; keep that.
                let dt1_dvd = -t2 * desatl_dvd / esat;
                let t9 = t0 * t1;
                let vaclm = t9 * diff_vds;
                (
                    vaclm,
                    t0 * dt1_dvg * diff_vds - t9 * dvdseff_dvg + t1 * diff_vds * dt0_dvg,
                    t0 * dt1_dvd * diff_vds + t9 * (1.0 - dvdseff_dvd),
                    (dt0_dvb * t1 + t0 * dt1_dvb) * diff_vds - t9 * dvdseff_dvb,
                )
            } else {
                (MAX_EXP, 0.0, 0.0, 0.0)
            };

        let (vadibl, dvadibl_dvg, dvadibl_dvd, dvadibl_dvb) = if param("thetarout") > 0.0 {
            let t8 = abulk * vdsat;
            let t0 = vgst2vtm * t8;
            let dt0_dvg = vgst2vtm * abulk * dvdsat_dvg + t8 + vgst2vtm * vdsat * dabulk_dvg;
            let dt0_dvb = vgst2vtm * (dabulk_dvb * vdsat + abulk * dvdsat_dvb);
            let dt0_dvd = vgst2vtm * abulk * dvdsat_dvd;
            let t1 = vgst2vtm + t8;
            let dt1_dvg = 1.0 + abulk * dvdsat_dvg + vdsat * dabulk_dvg;
            let dt1_dvb = abulk * dvdsat_dvb + dabulk_dvb * vdsat;
            let dt1_dvd = abulk * dvdsat_dvd;
            let t9 = t1 * t1;
            let t2 = param("thetarout");
            let mut vadibl = (vgst2vtm - t0 / t1) / t2;
            let mut dvadibl_dvg = (1.0 - dt0_dvg / t1 + t0 * dt1_dvg / t9) / t2;
            let mut dvadibl_dvb = (-dt0_dvb / t1 + t0 * dt1_dvb / t9) / t2;
            let mut dvadibl_dvd = (-dt0_dvd / t1 + t0 * dt1_dvd / t9) / t2;
            let t7 = param("pdiblb") * vbseff;
            if t7 >= -0.9 {
                let t3 = 1.0 / (1.0 + t7);
                vadibl *= t3;
                dvadibl_dvg *= t3;
                dvadibl_dvb = (dvadibl_dvb - vadibl * param("pdiblb")) * t3;
                dvadibl_dvd *= t3;
            } else {
                let t4 = 1.0 / (0.8 + t7);
                let t3 = (17.0 + 20.0 * t7) * t4;
                dvadibl_dvg *= t3;
                dvadibl_dvb = dvadibl_dvb * t3 - vadibl * param("pdiblb") * t4 * t4;
                dvadibl_dvd *= t3;
                vadibl *= t3;
            }
            (vadibl, dvadibl_dvg, dvadibl_dvd, dvadibl_dvb)
        } else {
            (MAX_EXP, 0.0, 0.0, 0.0)
        };

        let t8a = param("pvag") / esatl;
        let t9a = t8a * vgsteff;
        let (t0a, dt0_dvg, dt0_dvd, dt0_dvb);
        if t9a > -0.9 {
            t0a = 1.0 + t9a;
            dt0_dvg = t8a * (1.0 - vgsteff * desatl_dvg / esatl);
            dt0_dvb = -t9a * desatl_dvb / esatl;
            dt0_dvd = -t9a * desatl_dvd / esatl;
        } else {
            let mut t1a = 1.0 / (17.0 + 20.0 * t9a);
            t0a = (0.8 + t9a) * t1a;
            t1a *= t1a;
            dt0_dvg = t8a * (1.0 - vgsteff * desatl_dvg / esatl) * t1a;
            let t9b = t9a * t1a / esatl;
            dt0_dvb = -t9b * desatl_dvb;
            dt0_dvd = -t9b * desatl_dvd;
        }

        let tmp1a = vaclm * vaclm;
        let tmp2a2 = vadibl * vadibl;
        let mut tmp3a2 = vaclm + vadibl;
        let t1a = vaclm * vadibl / tmp3a2;
        tmp3a2 *= tmp3a2;
        let dt1_dvg = (tmp1a * dvadibl_dvg + tmp2a2 * dvaclm_dvg) / tmp3a2;
        let dt1_dvd = (tmp1a * dvadibl_dvd + tmp2a2 * dvaclm_dvd) / tmp3a2;
        let dt1_dvb = (tmp1a * dvadibl_dvb + tmp2a2 * dvaclm_dvb) / tmp3a2;
        let va = vasat + t0a * t1a;
        let dva_dvg = dvasat_dvg + t1a * dt0_dvg + t0a * dt1_dvg;
        let dva_dvd = dvasat_dvd + t1a * dt0_dvd + t0a * dt1_dvd;
        let dva_dvb = dvasat_dvb + t1a * dt0_dvb + t0a * dt1_dvb;

        let (vascbe, dvascbe_dvg, dvascbe_dvd, dvascbe_dvb) = if param("pscbe2") > 0.0 {
            if diff_vds > param("pscbe1") * param("litl") / EXP_THRESHOLD {
                let t0 = param("pscbe1") * param("litl") / diff_vds;
                let vascbe = leff * t0.exp() / param("pscbe2");
                let t1 = t0 * vascbe / diff_vds;
                (
                    vascbe,
                    t1 * dvdseff_dvg,
                    -t1 * (1.0 - dvdseff_dvd),
                    t1 * dvdseff_dvb,
                )
            } else {
                (MAX_EXP * leff / param("pscbe2"), 0.0, 0.0, 0.0)
            }
        } else {
            (MAX_EXP, 0.0, 0.0, 0.0)
        };

        // Drain current and its derivatives.
        let coxwovl = cox * weff / leff;
        let beta = ueff * coxwovl;
        let dbeta_dvg = coxwovl * dueff_dvg + beta * dweff_dvg / weff;
        let dbeta_dvd = coxwovl * dueff_dvd;
        let dbeta_dvb = coxwovl * dueff_dvb + beta * dweff_dvb / weff;

        let t0 = 1.0 - 0.5 * abulk * vdseff / vgst2vtm;
        let dt0_dvg = -0.5
            * (abulk * dvdseff_dvg - abulk * vdseff / vgst2vtm + vdseff * dabulk_dvg)
            / vgst2vtm;
        let dt0_dvd = -0.5 * abulk * dvdseff_dvd / vgst2vtm;
        let dt0_dvb = -0.5 * (abulk * dvdseff_dvb + dabulk_dvb * vdseff) / vgst2vtm;

        let fgche1 = vgsteff * t0;
        let dfgche1_dvg = vgsteff * dt0_dvg + t0;
        let dfgche1_dvd = vgsteff * dt0_dvd;
        let dfgche1_dvb = vgsteff * dt0_dvb;

        let t9f = vdseff / esatl;
        let fgche2 = 1.0 + t9f;
        let dfgche2_dvg = (dvdseff_dvg - t9f * desatl_dvg) / esatl;
        let dfgche2_dvd = (dvdseff_dvd - t9f * desatl_dvd) / esatl;
        let dfgche2_dvb = (dvdseff_dvb - t9f * desatl_dvb) / esatl;

        let gche = beta * fgche1 / fgche2;
        let dgche_dvg = (beta * dfgche1_dvg + fgche1 * dbeta_dvg - gche * dfgche2_dvg) / fgche2;
        let dgche_dvd = (beta * dfgche1_dvd + fgche1 * dbeta_dvd - gche * dfgche2_dvd) / fgche2;
        let dgche_dvb = (beta * dfgche1_dvb + fgche1 * dbeta_dvb - gche * dfgche2_dvb) / fgche2;

        let t0i = 1.0 + gche * rds;
        let t9i = vdseff / t0i;
        let idl = gche * t9i;
        let didl_dvg = (gche * dvdseff_dvg + t9i * dgche_dvg) / t0i - idl * gche / t0i * drds_dvg;
        let didl_dvd = (gche * dvdseff_dvd + t9i * dgche_dvd) / t0i;
        let didl_dvb = (gche * dvdseff_dvb + t9i * dgche_dvb - idl * drds_dvb * gche) / t0i;

        let t9s = diff_vds / va;
        let t0s = 1.0 + t9s;
        let idsa = idl * t0s;
        let didsa_dvg = t0s * didl_dvg - idl * (dvdseff_dvg + t9s * dva_dvg) / va;
        let didsa_dvd = t0s * didl_dvd + idl * (1.0 - dvdseff_dvd - t9s * dva_dvd) / va;
        let didsa_dvb = t0s * didl_dvb - idl * (dvdseff_dvb + t9s * dva_dvb) / va;

        let t9c2 = diff_vds / vascbe;
        let t0c = 1.0 + t9c2;
        let ids = idsa * t0c;
        let mut gds = t0c * didsa_dvd + idsa * (1.0 - dvdseff_dvd - t9c2 * dvascbe_dvd) / vascbe;
        let mut gmbs = t0c * didsa_dvb - idsa * (dvdseff_dvb + t9c2 * dvascbe_dvb) / vascbe;
        let mut gm = t0c * didsa_dvg - idsa * (dvdseff_dvg + t9c2 * dvascbe_dvg) / vascbe;

        gds += gm * dvgsteff_dvd;
        gmbs += gm * dvgsteff_dvb;
        gm *= dvgsteff_dvg;
        gmbs *= dvbseff_dvb;

        // Substrate current.
        let tmp = param("alpha0") + param("alpha1") * leff;
        let (isub, gbgs, gbds, gbbs) = if tmp <= 0.0 || param("beta0") <= 0.0 {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            let t2 = tmp / leff;
            let (t1, dt1_dvg, dt1_dvd, dt1_dvb) = if diff_vds > param("beta0") / EXP_THRESHOLD {
                let t0 = -param("beta0") / diff_vds;
                let t1 = t2 * diff_vds * t0.exp();
                let t3 = t1 / diff_vds * (t0 - 1.0);
                (
                    t1,
                    t3 * dvdseff_dvg,
                    t3 * (dvdseff_dvd - 1.0),
                    t3 * dvdseff_dvb,
                )
            } else {
                let t3 = t2 * MIN_EXP;
                (
                    t3 * diff_vds,
                    -t3 * dvdseff_dvg,
                    t3 * (1.0 - dvdseff_dvd),
                    -t3 * dvdseff_dvb,
                )
            };
            let isub = t1 * idsa;
            let mut gbg = t1 * didsa_dvg + idsa * dt1_dvg;
            let mut gbd = t1 * didsa_dvd + idsa * dt1_dvd;
            let mut gbb = t1 * didsa_dvb + idsa * dt1_dvb;
            gbd += gbg * dvgsteff_dvd;
            gbb += gbg * dvgsteff_dvb;
            gbg *= dvgsteff_dvg;
            gbb *= dvbseff_dvb;
            (isub, gbg, gbd, gbb)
        };

        let result = DcEvaluation {
            ids,
            gm,
            gds,
            gmbs,
            isub,
            gbgs,
            gbds,
            gbbs,
            cbs,
            cbd,
            gbs,
            gbd,
            vbs,
            vbd: vbs - vds,
            vgs,
            vds,
            mode,
            von,
            vgsteff,
            vdseff,
            vdsat,
            abulk,
            ueff,
            rds,
            thetavth,
            abulk_over_vgst2vtm,
        };
        for (name, value) in [
            ("ids", result.ids),
            ("gm", result.gm),
            ("gds", result.gds),
            ("gmbs", result.gmbs),
            ("von", result.von),
            ("ueff", result.ueff),
            ("vdseff", result.vdseff),
            ("cbs", result.cbs),
            ("cbd", result.cbd),
        ] {
            if !value.is_finite() {
                return Err(crate::primitives::SpiceError::Numerical {
                    context: "BSIM3 DC evaluation (bsim3/b3ld.c)".into(),
                    message: format!("{name} is not finite"),
                });
            }
        }
        Ok(result)
    }
}

/// `@inst[param]` values C's `b3ask.c` reads from the instance after a load.
impl DcEvaluation {
    /// The value of a DC operating-point ask keyword, before multiplicity.
    /// These are C's IFparm keywords; `cbs`/`cbd` are capacitances in C and are
    /// therefore absent, while `ibs`/`ibd` are the junction currents. Charge,
    /// NQS, noise and gate-current asks are not part of this core.
    #[must_use]
    pub fn ask(&self, keyword: &str) -> Option<Real> {
        match keyword.to_ascii_lowercase().as_str() {
            "id" => Some(self.ids),
            "gm" => Some(self.gm),
            "gds" => Some(self.gds),
            "gmbs" => Some(self.gmbs),
            "gbd" => Some(self.gbds),
            "gbs" => Some(self.gbs),
            "vth" => Some(self.von),
            "vbs" => Some(self.vbs),
            "vgs" => Some(self.vgs),
            "vds" => Some(self.vds),
            "vdsat" => Some(self.vdsat),
            "ibs" => Some(self.cbs),
            "ibd" => Some(self.cbd),
            _ => None,
        }
    }
}
