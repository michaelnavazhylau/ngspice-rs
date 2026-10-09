//! `.disto` nonlinearities of the Gummel-Poon BJT: `bjtdset.c` (Taylor
//! coefficients at the bias point) and `bjtdisto.c` (which kernels drive
//! which branch). C's distortion model is its own simplified Gummel-Poon,
//! reproduced here as written rather than derived from `bjtload.c`:
//!
//! * junction currents of the ideal exponentials with C's old reverse law
//!   (`gbe = -IS/vbe` below `-5 NF Vt`), `ISE`/`ISC` leakage, Early effect
//!   and high injection through `qb = q1 (1 + sqrt(1 + 4 q2)) / 2` (**no
//!   `NKF`**), evaluated at `vbe = v(b') - v(e')` and `vbc = v(b) - v(c')`
//!   (the **external** base, as `bjtdset.c` reads it);
//! * the base resistance `RB`/`RBM`/`IRB` with `bjtdset.c`'s power-series
//!   inversion (whose `vbb = rbb(ib) * ib` linearizes around `ib = 0`);
//! * the diffusion charge `TF (1 + XTF exp(vbc/(1.44 VTF)) (cbe/(cbe +
//!   ITF))^2) cbe / qb` and the B-E, B-C (split by `XCJC`) and substrate
//!   depletion charges, the substrate one graded with the model's
//!   **unadjusted** `VJS`/`MJS` and always tied to the internal collector;
//! * `M` scaling folded into the parameters, with `bjtdset.c`'s extra `AREA`
//!   factor on the B-E depletion capacitance (`BJTtBEcap` already includes
//!   it);
//! * second-order coefficients signed by the polarity, third-order ones not;
//!   the depletion coefficients are never signed.
//!
//! Without a base resistance (`RB = RBM = 0`), `bjtdisto.c` forms the
//! `B-C'` (`qbx`) control from the stale excess-phase `vbe` kernel plus
//! `vbc`; that is reproduced too (it matters only with `XCJC < 1`). With a
//! base resistance, `bjtdisto.c`'s `D_F1MF2` case reads the `vb - vb'`
//! and substrate (`vs - vc'`) kernels at `f2` without conjugating them
//! (`i1hm2z` lacks its minus sign), which the base-resistance, `B-C'` and
//! substrate terms inherit in the `f1 - f2` product
//! ([`Control::with_unconjugated_in_f1_minus_f2`]). Excess
//! phase itself (`PTF`) is rejected at instantiation, so the delayed `vbe`
//! control equals `vbe`.

use crate::devices::distortion::{
    Control, DeviceDistortion, DistortionContext, DistortionTerm, Response, Series3, Taylor,
};
use crate::primitives::{Real, SpiceError, SpiceResult};

use super::Bjt;

/// The `dloadfns.c` coefficients of a `bjtdset.c` series in `p`, `q`, `r`
/// (C's `x`, `y`, `z`/`w`), the second-order ones times the polarity.
fn taylor(s: &Series3, polarity: Real) -> Taylor {
    let c = |e: [u8; 3]| s.coefficient(e);
    Taylor {
        xx: polarity * c([2, 0, 0]),
        yy: polarity * c([0, 2, 0]),
        zz: polarity * c([0, 0, 2]),
        xy: polarity * c([1, 1, 0]),
        yz: polarity * c([0, 1, 1]),
        xz: polarity * c([1, 0, 1]),
        xxx: c([3, 0, 0]),
        yyy: c([0, 3, 0]),
        zzz: c([0, 0, 3]),
        xxy: c([2, 1, 0]),
        xxz: c([2, 0, 1]),
        xyy: c([1, 2, 0]),
        yyz: c([0, 2, 1]),
        xzz: c([1, 0, 2]),
        yzz: c([0, 1, 2]),
        xyz: c([1, 1, 1]),
    }
}

/// The second- and third-order coefficients of a depletion charge with zero-
/// bias capacitance `cz`, potential `pot` and grading `m`, below the forward
/// threshold (`bjtdset.c`'s `lcap*2`/`lcap*3`).
fn depletion(cz: Real, pot: Real, m: Real, v: Real) -> (Real, Real) {
    let arg = 1. - v / pot;
    let sarg = (-m * arg.ln()).exp();
    (
        0.5 * cz * m * sarg / (arg * pot),
        cz * m * (m + 1.) * sarg / (arg * arg * pot * pot * 6.),
    )
}

impl Bjt {
    /// The `bjtdset.c` terms at the operating point (see the module
    /// documentation).
    #[allow(clippy::too_many_lines)]
    pub(super) fn distortion_terms(
        &self,
        context: &DistortionContext<'_>,
    ) -> SpiceResult<DeviceDistortion> {
        let t = self.thermal(context.model_context)?;
        let m = self.instance.multiplier;
        let pol = self.pol;
        let gmin = context.model_context.gmin;
        let n = self.nodes;
        let v = |node| context.voltage(node);
        let vt = t.vt;
        let csatbe = t.is_be * m;
        let csatbc = t.is_bc * m;
        let rbpr = t.rbm / m;
        let rbpi = t.rb / m - rbpr;
        let oik = t.inv_ikf / m;
        let c2 = t.ise * m;
        let vte = t.ne * vt;
        let oikr = t.inv_ikr / m;
        let c4 = t.isc * m;
        let vtc = t.nc * vt;
        let xjrb = t.irb * m;

        let vbe = pol * (v(n.bp) - v(n.ep));
        let vbc = pol * (v(n.b) - v(n.cp));
        let vbx = vbc;
        let vsc = pol * (v(n.s) - v(n.cp));
        let vbb = pol * (v(n.b) - v(n.bp));

        // Junction currents and their derivatives (not Taylor coefficients).
        let vtn = vt * t.nf;
        let (cbe, gbe, gbe2, gbe3, cben, gben, gben2, gben3);
        if vbe > -5. * vtn {
            let evbe = (vbe / vtn).exp();
            cbe = csatbe * (evbe - 1.) + gmin * vbe;
            gbe = csatbe * evbe / vtn + gmin;
            gbe2 = csatbe * evbe / vtn / vtn;
            gbe3 = gbe2 / vtn;
            if c2 == 0. {
                (cben, gben, gben2, gben3) = (0., 0., 0., 0.);
            } else {
                let evben = (vbe / vte).exp();
                cben = c2 * (evben - 1.);
                gben = c2 * evben / vte;
                gben2 = gben / vte;
                gben3 = gben2 / vte;
            }
        } else {
            gbe = -csatbe / vbe + gmin;
            (gbe2, gbe3, gben2, gben3) = (0., 0., 0., 0.);
            cbe = gbe * vbe;
            gben = -c2 / vbe;
            cben = gben * vbe;
        }
        let vtn = vt * t.nr;
        let (cbc, gbc, gbc2, gbc3, cbcn, gbcn, gbcn2, gbcn3);
        if vbc > -5. * vtn {
            let evbc = (vbc / vtn).exp();
            cbc = csatbc * (evbc - 1.) + gmin * vbc;
            gbc = csatbc * evbc / vtn + gmin;
            gbc2 = csatbc * evbc / vtn / vtn;
            gbc3 = gbc2 / vtn;
            if c4 == 0. {
                (cbcn, gbcn, gbcn2, gbcn3) = (0., 0., 0., 0.);
            } else {
                let evbcn = (vbc / vtc).exp();
                cbcn = c4 * (evbcn - 1.);
                gbcn = c4 * evbcn / vtc;
                gbcn2 = gbcn / vtc;
                gbcn3 = gbcn2 / vtc;
            }
        } else {
            gbc = -csatbc / vbc + gmin;
            (gbc2, gbc3) = (0., 0.);
            cbc = gbc * vbc;
            gbcn = -c4 / vbc;
            (gbcn2, gbcn3) = (0., 0.);
            cbcn = gbcn * vbc;
        }

        // Base charge qb(p = vbe, q = vbc).
        let mut early = Series3::constant(1. - t.inv_vaf * vbc - t.inv_var * vbe);
        early.set_coefficient([1, 0, 0], -t.inv_var);
        early.set_coefficient([0, 1, 0], -t.inv_vaf);
        let q1 = early.inv();
        let qb = if oik == 0. && oikr == 0. {
            q1
        } else {
            let q2 =
                Series3::univariate(0, oik * cbe + oikr * cbc, oik * gbe, oik * gbe2, oik * gbe3)
                    .plus(&Series3::univariate(
                        1,
                        0.,
                        oikr * gbc,
                        oikr * gbc2,
                        oikr * gbc3,
                    ));
            let arg = (1. + 4. * q2.value()).max(0.);
            let sqarg = if arg == 0. {
                Series3::constant(1.)
            } else {
                q2.times(4.).offset(1.).sqrt()
            };
            q1.mul(&sqarg.offset(1.)).times(0.5)
        };

        // Collector current ic(p = vbe, q = vbc, r = delayed vbe).
        let transport = Series3::univariate(2, cbe - cbc, gbe, gbe2, gbe3)
            .plus(&Series3::univariate(1, 0., -gbc, -gbc2, -gbc3));
        let ic = transport.div(&qb).plus(
            &Series3::univariate(
                1,
                cbc / t.br + cbcn,
                gbc / t.br + gbcn,
                gbc2 / t.br + gbcn2,
                gbc3 / t.br + gbcn3,
            )
            .times(-1.),
        );

        // Base resistance current ibb(p, q, r = vbb).
        let has_base_resistance = !(self.model.rbm == 0. && self.model.rb == self.model.rbm);
        let ibb = if rbpr == 0. && rbpi == 0. {
            Series3::constant(0.)
        } else {
            let cb = cbe / t.bf + cben + cbc / t.br + cbcn;
            if cb == 0. {
                // Linear in vbb only: no distortion.
                Series3::constant(0.)
            } else if xjrb != 0. && rbpi != 0. {
                // p stands for the base current here.
                let ratio = (cb / xjrb).max(1e-9);
                let low = Series3::univariate(0, ratio, 1. / xjrb, 0., 0.)
                    .sqrt()
                    .times(2.4317);
                let high = Series3::univariate(0, 1. + 14.59025 * ratio, 14.59025 / xjrb, 0., 0.)
                    .sqrt()
                    .offset(-1.);
                let z = high.div(&low);
                let tanz = z.tan();
                let numerator = tanz.plus(&z.times(-1.));
                let denominator = tanz.mul(&tanz).mul(&z);
                let rbb = numerator.div(&denominator).times(3. * rbpi).offset(rbpr);
                let vbb_of_ib = rbb.mul(&Series3::variable(0, 0.));
                let d1 = vbb_of_ib.coefficient([1, 0, 0]);
                let d2 = 2. * vbb_of_ib.coefficient([2, 0, 0]);
                let d3 = 6. * vbb_of_ib.coefficient([3, 0, 0]);
                if d1 == 0. {
                    return Err(SpiceError::Numerical {
                        context: format!("distortion of {}", self.name),
                        message: "zero base-resistance derivative (bjtdset.c d_vbb.d1_p = 0)"
                            .into(),
                    });
                }
                let gbb1 = 1. / d1;
                let gbb2 = -(d2 * 0.5) * gbb1 * gbb1;
                let gbb3 =
                    gbb1 * gbb1 * gbb1 * gbb1 * (-(d3 / 6.) + 2. * (d2 * 0.5) * (d2 * 0.5) * gbb1);
                Series3::univariate(2, cb, gbb1, 2. * gbb2, 6. * gbb3)
            } else {
                let rbb = if rbpi == 0. {
                    Series3::constant(0.)
                } else {
                    qb.inv().times(rbpi)
                }
                .offset(rbpr);
                Series3::variable(2, vbb).div(&rbb)
            }
        };

        // Base current ib(p, q).
        let ib = Series3::univariate(
            0,
            0.,
            gbe / t.bf + gben,
            gbe2 / t.bf + gben2,
            gbe3 / t.bf + gben3,
        )
        .plus(&Series3::univariate(
            1,
            0.,
            gbc / t.br + gbcn,
            gbc2 / t.br + gbcn2,
            gbc3 / t.br + gbcn3,
        ));

        // B-E charge qbe(p, q): diffusion plus depletion.
        let tf = t.tf;
        let xtf = t.xtf;
        let ovtf = t.vtf_factor;
        let xjtf = t.itf * m;
        let mut qbe = if tf != 0. && vbe > 0. {
            let cbe_series = Series3::univariate(0, cbe, gbe, gbe2, gbe3);
            let tff = if xtf == 0. {
                Series3::constant(tf)
            } else {
                let vbc_factor = if ovtf == 0. {
                    Series3::constant(1.)
                } else {
                    Series3::univariate(1, vbc * ovtf, ovtf, 0., 0.).exp()
                };
                let current_factor = if xjtf == 0. {
                    Series3::constant(1.)
                } else {
                    let ratio = cbe_series.div(&cbe_series.offset(xjtf));
                    ratio.mul(&ratio)
                };
                vbc_factor.mul(&current_factor).times(tf * xtf).offset(tf)
            };
            tff.div(&qb).mul(&cbe_series)
        } else {
            Series3::constant(0.)
        };
        let czbe = t.cje * self.instance.area * m;
        let (pe, xme) = (t.vje, t.mje);
        let (capbe2, capbe3) = if vbe < t.fc_vje {
            depletion(czbe, pe, xme, vbe)
        } else {
            (0.5 * xme * (czbe / t.f2) / pe, 0.)
        };
        qbe.set_coefficient([2, 0, 0], qbe.coefficient([2, 0, 0]) + capbe2);
        qbe.set_coefficient([3, 0, 0], qbe.coefficient([3, 0, 0]) + capbe3);

        // B-C, B-X and substrate depletion charges (unsigned coefficients).
        let ctot = t.cjc * m;
        let czbc = ctot * t.xcjc;
        let czbx = ctot - czbc;
        let (pc, xmc) = (t.vjc, t.mjc);
        let collector = |cz: Real, v: Real| {
            if v < t.fc_vjc {
                depletion(cz, pc, xmc, v)
            } else {
                (0.5 * xmc * (cz / t.f6) / pc, 0.)
            }
        };
        let (capbc2, capbc3) = collector(czbc, vbc);
        let (capbx2, capbx3) = collector(czbx, vbx);
        let czcs = t.cjs * m;
        let (ps, xms) = (self.model.vjs, self.model.mjs);
        let (capsc2, capsc3) = if vsc < 0. {
            depletion(czcs, ps, xms, vsc)
        } else {
            (czcs * 0.5 * xms / ps, 0.)
        };

        let vbe_control = || Control::between(n.bp, n.ep);
        let vbc_control = || Control::between(n.bp, n.cp);
        let mut terms = vec![
            DistortionTerm::new(
                Response::Current,
                [n.cp, n.ep],
                vec![vbe_control(), vbc_control(), vbe_control()],
                taylor(&ic, pol),
            ),
            DistortionTerm::new(
                Response::Current,
                [n.bp, n.ep],
                vec![vbe_control(), vbc_control()],
                taylor(&ib, pol),
            ),
        ];
        if has_base_resistance {
            terms.push(DistortionTerm::new(
                Response::Current,
                [n.b, n.bp],
                vec![
                    vbe_control(),
                    vbc_control(),
                    Control::sum(Vec::new()).with_unconjugated_in_f1_minus_f2(vec![[n.b, n.bp]]),
                ],
                taylor(&ibb, pol),
            ));
        }
        terms.push(DistortionTerm::new(
            Response::Charge,
            [n.bp, n.ep],
            vec![vbe_control(), vbc_control()],
            taylor(&qbe, pol),
        ));
        // bjtdisto.c: `z = vbx = vb - vc'` is formed as the previous `z` plus
        // `vbc`, the previous `z` being `vb - vb'` after the base-resistance
        // term, and the (undelayed) `vbe` kernel without one.
        let vbx_control = if has_base_resistance {
            Control::between(n.bp, n.cp).with_unconjugated_in_f1_minus_f2(vec![[n.b, n.bp]])
        } else {
            Control::sum(vec![[n.bp, n.ep], [n.bp, n.cp]])
        };
        terms.push(DistortionTerm::new(
            Response::Charge,
            [n.b, n.cp],
            vec![vbx_control],
            Taylor::single(capbx2, capbx3),
        ));
        terms.push(DistortionTerm::new(
            Response::Charge,
            [n.bp, n.cp],
            vec![vbc_control()],
            Taylor::single(capbc2, capbc3),
        ));
        terms.push(DistortionTerm::new(
            Response::Charge,
            [n.s, n.cp],
            vec![Control::sum(Vec::new()).with_unconjugated_in_f1_minus_f2(vec![[n.s, n.cp]])],
            Taylor::single(capsc2, capsc3),
        ));
        Ok(DeviceDistortion::Terms(terms))
    }
}
