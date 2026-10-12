//! `.disto` nonlinearities of the MOS1 device: `mos1dset.c` (Taylor
//! coefficients at the bias point) and `mos1dist.c` (which kernels drive
//! which branch), reproduced as written:
//!
//! * the Shichman-Hodges drain current in `x = vgs`, `y = vbs`, `z = vds`
//!   (gate and bulk at their external nodes, drain and source at the internal
//!   ones), with `mos1dset.c`'s source-drain interchange of the coefficients
//!   in inverse mode;
//! * the bulk diodes' ideal exponentials (forward bias only, no `gmin`);
//! * the bulk depletion charges, whose second-order coefficient carries the
//!   polarity below `FC * PB` but not on the linear continuation above it;
//! * Meyer's gate capacitances as **single-variable** charges of `vgs`,
//!   `vgd` and `vgb` (C's own comment calls these expressions incorrect;
//!   the port keeps them for parity), the gate-drain/gate-source pair swapped
//!   in inverse mode;
//! * in the `2f1 - f2` product, `mos1dist.c` reads the `H2(f1, f1)` kernel
//!   from the first-order `H1(f1)` vector (a C defect, reproduced through
//!   [`DistortionTerm::with_first_order_im3_kernel`]).

use crate::devices::distortion::{
    Control, DeviceDistortion, DistortionContext, DistortionTerm, Response, Taylor,
};
use crate::primitives::{Real, SpiceResult};

use super::Mos1;
use crate::devices::mos::MAX_EXP_ARG;

/// `exp(-grading * ln(arg))` with `mos1dset.c`'s square-root shortcut.
fn grading_power(arg: Real, grading: Real) -> Real {
    if grading == 0.5 {
        1. / arg.sqrt()
    } else {
        (-grading * arg.ln()).exp()
    }
}

impl Mos1 {
    /// The `mos1dset.c` terms at the operating point (see the module
    /// documentation).
    #[allow(clippy::too_many_lines, clippy::similar_names)]
    pub(super) fn distortion_terms(
        &self,
        context: &DistortionContext<'_>,
    ) -> SpiceResult<DeviceDistortion> {
        let op = self.operating(context.model_context)?;
        let pol = self.model.common.pol;
        let gmin = context.model_context.gmin;
        let [dp, g, sp, b] = self.inner;
        let v = |node| context.voltage(node);
        let vt = op.vt;
        let vbs = pol * (v(b) - v(sp));
        let vgs = pol * (v(g) - v(sp));
        let vds = pol * (v(dp) - v(sp));
        let vbd = vbs - vds;
        let vgd = vgs - vds;

        // Bulk diodes.
        let diode = |v: Real, saturation: Real| {
            if v <= 0. {
                (0., 0.)
            } else {
                let e = (v / vt).min(MAX_EXP_ARG).exp();
                let g = saturation * e / vt + gmin;
                let g2 = pol * 0.5 * (g - gmin) / vt;
                (g2, pol * g2 / (vt * 3.))
            }
        };
        let (gbs2, gbs3) = diode(vbs, op.source.saturation);
        let (gbd2, gbd3) = diode(vbd, op.drain.saturation);

        let mode: Real = if vds >= 0. { 1. } else { -1. };
        let normal = mode > 0.;
        let gamma = op.gamma;
        let lambda = op.params.lambda;
        let beta = op.params.beta;
        let phi = op.phi;
        let vbx = if normal { vbs } else { vbd };
        let (sarg, dvon, d2von, d3von);
        if vbx <= 0. {
            sarg = (phi - vbx).sqrt();
            if -gamma != 0. {
                let d1 = -gamma * 0.5 / sarg;
                let d2 = -d1 * 0.5 / (sarg * sarg);
                (dvon, d2von, d3von) = (d1, d2, 1.5 * d2 / (sarg * sarg));
            } else {
                (dvon, d2von, d3von) = (0., 0., 0.);
            }
        } else {
            let root = phi.sqrt();
            let d1 = if gamma != 0. {
                -gamma / (root + root)
            } else {
                0.
            };
            let s = (root - vbx / (root + root)).max(0.);
            sarg = s;
            dvon = if s <= 0. { 0. } else { d1 };
            (d2von, d3von) = (0., 0.);
        }
        let von = op.vbi * pol + gamma * sarg;
        let vgst = (if normal { vgs } else { vgd }) - von;
        let vdsat = vgst.max(0.);
        let vdsm = vds * mode;
        // (gm, gb, gds derivatives up to third order), C's names.
        let (mut gm2, mut gb2, mut gds2, mut gmds, mut gbds, mut gmb) = (0., 0., 0., 0., 0., 0.);
        let (mut gm3, mut gb3, mut gds3) = (0., 0., 0.);
        let (mut gm2ds, mut gmds2, mut gm2b, mut gmb2, mut gb2ds, mut gbds2, mut gmbds) =
            (0., 0., 0., 0., 0., 0., 0.);
        if vgst > 0. {
            let betap = beta * (1. + lambda * vdsm);
            if vgst <= vdsm {
                // Saturation.
                let gm = betap * vgst;
                gm2 = betap;
                gds2 = 0.;
                gb2 = -(gm * d2von - betap * dvon * dvon);
                gmds = vgst * lambda * beta;
                gbds = -gmds * dvon;
                gmb = -betap * dvon;
                gm3 = 0.;
                gb3 = -(gmb * d2von + gm * d3von - betap * 2. * dvon * d2von);
                gds3 = 0.;
                gm2ds = beta * lambda;
                gm2b = 0.;
                gmb2 = -betap * d2von;
                gb2ds = -(gmds * d2von - dvon * dvon * beta * lambda);
                gmds2 = 0.;
                gbds2 = 0.;
                gmbds = -beta * lambda * dvon;
            } else {
                // Linear region.
                let gm = betap * vdsm;
                gm2 = 0.;
                gb2 = -(gm * d2von);
                gds2 = 2. * beta * lambda * (vgst - vdsm) - betap;
                gmds = beta * lambda * vdsm + betap;
                gbds = -gmds * dvon;
                gmb = 0.;
                gm3 = 0.;
                gb3 = -gm * d3von;
                gds3 = -beta * lambda * 3.;
                (gm2ds, gm2b, gmb2) = (0., 0., 0.);
                gmds2 = 2. * lambda * beta;
                gb2ds = -(gmds * d2von);
                gbds2 = -gmds2 * dvon;
                gmbds = 0.;
            }
        }

        // Bulk depletion charges.
        let bulk = |v: Real, junction: &crate::devices::mos::Junction| {
            let pb = junction.potential;
            let (mj, mjsw) = (junction.mj, junction.mjsw);
            if v < junction.fc * pb {
                let arg = 1. - v / pb;
                let sarg = grading_power(arg, mj);
                let sargsw = grading_power(arg, mjsw);
                let c2 = pol * 0.5 / pb
                    * (junction.bottom * mj * sarg / arg + junction.sidewall * mjsw * sargsw / arg);
                let c3 = (junction.bottom * sarg * mj * (mj + 1.)
                    + junction.sidewall * sargsw * mjsw * (mjsw + 1.))
                    / (6. * pb * pb * arg * arg);
                (c2, c3)
            } else {
                // mos1temp.c's f3: the slope of the linear continuation.
                let arg = 1. - junction.fc;
                let sarg = (-mj * arg.ln()).exp();
                let sargsw = (-mjsw * arg.ln()).exp();
                let f3 = junction.bottom * mj * sarg / arg / pb
                    + junction.sidewall * mjsw * sargsw / arg / pb;
                (0.5 * f3, 0.)
            }
        };
        let (capbs2, capbs3) = bulk(vbs, &op.source);
        let (capbd2, capbd3) = bulk(vbd, &op.drain);

        // Meyer gate charges.
        let cox = op.oxide;
        let (mut capgb2, mut capgs2, mut capgs3, mut capgd2, mut capgd3) = (0., 0., 0., 0., 0.);
        if vgst <= -phi {
        } else if vgst <= -phi / 2. {
            capgb2 = -cox / (4. * phi);
        } else if vgst <= 0. {
            capgb2 = -cox / (4. * phi);
            capgs2 = cox / (3. * phi);
        } else if vdsat > vdsm {
            let vddif = 2.0 * vdsat - vdsm;
            let vddif1 = vdsat - vdsm;
            let vddif2 = vddif * vddif;
            capgd2 = -vdsat * vdsm * cox / (3. * vddif * vddif2);
            capgd3 = -vdsm * cox * (vddif - 6. * vdsat) / (9. * vddif2 * vddif2);
            capgs2 = -vddif1 * vdsm * cox / (3. * vddif * vddif2);
            capgs3 = -vdsm * cox * (vddif - 6. * vddif1) / (9. * vddif2 * vddif2);
        }
        let capgb3 = 0.;

        let mut cdr = if normal {
            Taylor {
                xx: gm2,
                yy: gb2,
                zz: gds2,
                xy: gmb,
                yz: gbds,
                xz: gmds,
                xxx: gm3,
                yyy: gb3,
                zzz: gds3,
                xxz: gm2ds,
                xxy: gm2b,
                yyz: gb2ds,
                xyy: gmb2,
                xzz: gmds2,
                yzz: gbds2,
                xyz: gmbds,
            }
        } else {
            Taylor {
                xx: -gm2,
                yy: -gb2,
                zz: -(gm2 + gb2 + gds2 + 2. * (gmb + gmds + gbds)),
                xy: -gmb,
                yz: gmb + gb2 + gbds,
                xz: gm2 + gmb + gmds,
                xxx: -gm3,
                yyy: -gb3,
                zzz: gm3
                    + gb3
                    + gds3
                    + 3. * (gm2b + gm2ds + gmb2 + gb2ds + gmds2 + gbds2)
                    + 6. * gmbds,
                xxz: gm3 + gm2b + gm2ds,
                xxy: -gm2b,
                yyz: gmb2 + gb3 + gb2ds,
                xyy: -gmb2,
                xzz: -(gm3 + 2. * (gm2b + gm2ds + gmbds) + gmb2 + gmds2),
                yzz: -(gb3 + 2. * (gmb2 + gb2ds + gmbds) + gm2b + gbds2),
                xyz: gm2b + gmb2 + gmbds,
            }
        };
        // Polarity and the 1/2!, 1/3! Taylor factors (mos1dset.c).
        cdr.xx *= 0.5 * pol;
        cdr.yy *= 0.5 * pol;
        cdr.zz *= 0.5 * pol;
        cdr.xy *= pol;
        cdr.yz *= pol;
        cdr.xz *= pol;
        cdr.xxx /= 6.;
        cdr.yyy /= 6.;
        cdr.zzz /= 6.;
        cdr.xxz *= 0.5;
        cdr.xxy *= 0.5;
        cdr.yyz *= 0.5;
        cdr.xyy *= 0.5;
        cdr.xzz *= 0.5;
        cdr.yzz *= 0.5;
        let (gs, gd) = if normal {
            ((pol * capgs2, capgs3), (pol * capgd2, capgd3))
        } else {
            ((pol * capgd2, capgd3), (pol * capgs2, capgs3))
        };

        let vgs_control = || Control::between(g, sp);
        let vbs_control = || Control::between(b, sp);
        let vds_control = || Control::between(dp, sp);
        let current = |nodes, control: Control, c2, c3| {
            DistortionTerm::new(
                Response::Current,
                nodes,
                vec![control],
                Taylor::single(c2, c3),
            )
        };
        let charge = |nodes, control: Control, c2, c3| {
            DistortionTerm::new(
                Response::Charge,
                nodes,
                vec![control],
                Taylor::single(c2, c3),
            )
        };
        let terms = vec![
            DistortionTerm::new(
                Response::Current,
                [dp, sp],
                vec![vgs_control(), vbs_control(), vds_control()],
                cdr,
            ),
            current([b, sp], vbs_control(), gbs2, gbs3),
            current([b, dp], Control::between(b, dp), gbd2, gbd3),
            charge([g, sp], vgs_control(), gs.0, gs.1),
            charge([g, dp], Control::between(g, dp), gd.0, gd.1),
            charge([g, b], Control::between(g, b), pol * capgb2, capgb3),
            charge([b, sp], vbs_control(), capbs2, capbs3),
            charge([b, dp], Control::between(b, dp), capbd2, capbd3),
        ];
        // mos1dist.c's D_2F1MF2 reads H1(f1) where H2(f1, f1) is meant.
        Ok(DeviceDistortion::Terms(
            terms
                .into_iter()
                .map(DistortionTerm::with_first_order_im3_kernel)
                .collect(),
        ))
    }
}
