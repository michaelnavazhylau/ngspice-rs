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
    Control, DeviceDistortion, DistortionContext, DistortionTerm, Response, Taylor, deriv::Dderivs,
};
use crate::primitives::{Real, SpiceError, SpiceResult};

use super::Bjt;

/// The `dloadfns.c` coefficients `bjtdset.c` stores from a derivative
/// structure in `p`, `q`, `r` (C's `x`, `y`, `z`/`w`): `0.5*type*d2_p2`,
/// `type*d2_pq`, `d3_p3/6.`, `0.5*d3_p2q`, `d3_pqr`.
fn taylor(d: &Dderivs, polarity: Real) -> Taylor {
    Taylor {
        xx: 0.5 * polarity * d.d2_p2,
        yy: 0.5 * polarity * d.d2_q2,
        zz: 0.5 * polarity * d.d2_r2,
        xy: polarity * d.d2_pq,
        yz: polarity * d.d2_qr,
        xz: polarity * d.d2_pr,
        xxx: d.d3_p3 / 6.,
        yyy: d.d3_q3 / 6.,
        zzz: d.d3_r3 / 6.,
        xxy: 0.5 * d.d3_p2q,
        xxz: 0.5 * d.d3_p2r,
        xyy: 0.5 * d.d3_pq2,
        yyz: 0.5 * d.d3_q2r,
        xzz: 0.5 * d.d3_pr2,
        yzz: 0.5 * d.d3_qr2,
        xyz: d.d3_pqr,
    }
}

/// C's `MAX(a, b)`.
fn c_max(a: Real, b: Real) -> Real {
    if a > b { a } else { b }
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

        // bjtdset.c's derivative structures: p = vbe, q = vbc, r = vbed (the
        // delayed vbe) for ic; r = vbb for ibb.
        let d_p = Dderivs::variable(0);
        let d_q = Dderivs::variable(1);
        let d_r = Dderivs::variable(2);

        // Base charge qb(p, q).
        let mut d_dummy = d_p;
        d_dummy.value = 1. - t.inv_vaf * vbc - t.inv_var * vbe;
        d_dummy.d1_p = -t.inv_var;
        d_dummy.d1_q = -t.inv_vaf;
        let d_q1 = d_dummy.inv();
        let d_qb = if oik == 0. && oikr == 0. {
            d_q1
        } else {
            let q2 = oik * cbe + oikr * cbc;
            let mut d_q2 = d_p;
            d_q2.value = q2;
            d_q2.d1_p = oik * gbe;
            d_q2.d1_q = oikr * gbc;
            d_q2.d2_p2 = oik * gbe2;
            d_q2.d2_q2 = oikr * gbc2;
            d_q2.d3_p3 = oik * gbe3;
            d_q2.d3_q3 = oikr * gbc3;
            let arg = c_max(0., 1. + 4. * q2);
            let d_arg = if arg == 0. {
                let mut d = d_p;
                d.d1_p = 0.;
                d
            } else {
                let mut d = d_q2.times(4.0);
                d.value += 1.;
                d
            };
            let mut d_sqarg = d_p;
            d_sqarg.value = 1.0;
            d_sqarg.d1_p = 0.0;
            if arg != 0. {
                d_sqarg = d_arg.sqrt();
            }
            let mut d_dummy = d_sqarg;
            d_dummy.value += 1.0;
            d_q1.mul(&d_dummy).times(0.5)
        };

        // Collector current ic(p, q, r).
        let mut d_dummy = d_p;
        d_dummy.d1_p = 0.0;
        d_dummy.value = cbe - cbc;
        d_dummy.d1_r = gbe;
        d_dummy.d2_r2 = gbe2;
        d_dummy.d3_r3 = gbe3;
        d_dummy.d1_q = -gbc;
        d_dummy.d2_q2 = -gbc2;
        d_dummy.d3_q3 = -gbc3;
        let mut d_ic = d_dummy.div(&d_qb);
        d_ic.value -= cbc / t.br + cbcn;
        d_ic.d1_q -= gbc / t.br + gbcn;
        d_ic.d2_q2 -= gbc2 / t.br + gbcn2;
        d_ic.d3_q3 -= gbc3 / t.br + gbcn3;

        // Base resistance current ibb.
        let has_base_resistance = !(self.model.rbm == 0. && self.model.rb == self.model.rbm);
        let d_ibb = if rbpr == 0.0 && rbpi == 0.0 {
            let mut d = d_p;
            d.d1_p = 0.0;
            d
        } else {
            let cb = cbe / t.bf + cben + cbc / t.br + cbcn;
            if cb == 0.0 {
                let mut d = d_r;
                if rbpr != 0.0 {
                    d.d1_r = 1. / rbpr;
                }
                d
            } else if xjrb != 0.0 && rbpi != 0.0 {
                // p stands for the base current here.
                let mut d_dummy = d_p;
                d_dummy.value = c_max(cb / xjrb, 1e-9);
                d_dummy.d1_p = 1. / xjrb;
                let d_dummy = d_dummy.sqrt().times(2.4317);
                let mut d_dummy2 = d_p;
                d_dummy2.value = 1. + 14.59025 * c_max(cb / xjrb, 1e-9);
                d_dummy2.d1_p = 14.59025 / xjrb;
                let mut d_dummy2 = d_dummy2.sqrt();
                d_dummy2.value -= 1.0;
                let d_z = d_dummy2.div(&d_dummy);
                let d_tanz = d_z.tan();
                let d_dummy = d_z.times(-1.0).plus(&d_tanz);
                let d_dummy2 = d_tanz.mul(&d_tanz).mul(&d_z);
                let mut d_rbb = d_dummy.div(&d_dummy2).times(3.0 * rbpi);
                d_rbb.value += rbpr;
                let d_vbb = d_rbb.mul(&d_p);
                if d_vbb.d1_p == 0. {
                    return Err(SpiceError::Numerical {
                        context: format!("distortion of {}", self.name),
                        message: "zero base-resistance derivative (bjtdset.c d_vbb.d1_p = 0)"
                            .into(),
                    });
                }
                // Power series inversion to the conductance derivatives.
                let gbb1 = 1. / d_vbb.d1_p;
                let gbb2 = -(d_vbb.d2_p2 * 0.5) * gbb1 * gbb1;
                let gbb3 = gbb1
                    * gbb1
                    * gbb1
                    * gbb1
                    * (-(d_vbb.d3_p3 / 6.0)
                        + 2. * (d_vbb.d2_p2 * 0.5) * (d_vbb.d2_p2 * 0.5) * gbb1);
                let mut d = d_r;
                d.value = cb;
                d.d1_r = gbb1;
                d.d2_r2 = 2. * gbb2;
                d.d3_r3 = 6.0 * gbb3;
                d
            } else {
                let mut d_rbb = d_p;
                d_rbb.d1_p = 0.0;
                if rbpi != 0.0 {
                    d_rbb = d_qb.inv().times(rbpi);
                }
                d_rbb.value += rbpr;
                let mut d_ibb = d_r;
                d_ibb.value = vbb;
                d_ibb.div(&d_rbb)
            }
        };

        // Base current ib(p, q).
        let mut d_ib = d_p;
        d_ib.d1_p = gbe / t.bf + gben;
        d_ib.d2_p2 = gbe2 / t.bf + gben2;
        d_ib.d3_p3 = gbe3 / t.bf + gben3;
        d_ib.d1_q = gbc / t.br + gbcn;
        d_ib.d2_q2 = gbc2 / t.br + gbcn2;
        d_ib.d3_q3 = gbc3 / t.br + gbcn3;

        // B-E charge qbe(p, q): diffusion plus depletion.
        let tf = t.tf;
        let xtf = t.xtf;
        let ovtf = t.vtf_factor;
        let xjtf = t.itf * m;
        let mut d_qbe = if tf != 0. && vbe > 0. {
            let mut d_cbe = d_p;
            d_cbe.value = cbe;
            d_cbe.d1_p = gbe;
            d_cbe.d2_p2 = gbe2;
            d_cbe.d3_p3 = gbe3;
            let d_tff = if xtf != 0. {
                let d_dummy = if ovtf != 0. {
                    let mut d = d_q;
                    d.value = vbc * ovtf;
                    d.d1_q = ovtf;
                    d.exp()
                } else {
                    let mut d = d_p;
                    d.value = 1.0;
                    d.d1_p = 0.0;
                    d
                };
                let d_dummy2 = if xjtf != 0. {
                    let mut d = d_cbe;
                    d.value += xjtf;
                    let d = d_cbe.div(&d);
                    d.mul(&d)
                } else {
                    let mut d = d_p;
                    d.value = 1.0;
                    d.d1_p = 0.0;
                    d
                };
                let mut d = d_dummy.mul(&d_dummy2).times(tf * xtf);
                d.value += tf;
                d
            } else {
                let mut d = d_p;
                d.value = tf;
                d.d1_p = 0.0;
                d
            };
            d_tff.div(&d_qb).mul(&d_cbe)
        } else {
            let mut d = d_p;
            d.value = 0.0;
            d.d1_p = 0.0;
            d
        };
        let czbe = t.cje * self.instance.area * m;
        let (pe, xme) = (t.vje, t.mje);
        let (capbe2, capbe3) = if vbe < t.fc_vje {
            depletion(czbe, pe, xme, vbe)
        } else {
            (0.5 * xme * (czbe / t.f2) / pe, 0.)
        };
        d_qbe.d2_p2 += capbe2 * 2.;
        d_qbe.d3_p3 += capbe3 * 6.;

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
                taylor(&d_ic, pol),
            ),
            DistortionTerm::new(
                Response::Current,
                [n.bp, n.ep],
                vec![vbe_control(), vbc_control()],
                taylor(&d_ib, pol),
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
                taylor(&d_ibb, pol),
            ));
        }
        terms.push(DistortionTerm::new(
            Response::Charge,
            [n.bp, n.ep],
            vec![vbe_control(), vbc_control()],
            taylor(&d_qbe, pol),
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
