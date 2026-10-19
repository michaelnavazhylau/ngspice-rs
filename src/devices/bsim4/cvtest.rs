//! `b4cvtest.c` (`BSIM4convTest`): the device convergence test, followed
//! statement by statement over C's struct members.
#![allow(non_snake_case)]
use super::fields::{Instance, Model};
use super::host::{mode, state};

/// C's `MAX` for the tolerance bases.
fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// `BSIM4convTest` for one instance: `true` when C would increment
/// `CKTnoncon`. `rhs_old` is the new iterate C reads as `CKTrhsOld`,
/// `state0` the last load's state vector, `mode` C's `CKTmode`.
#[allow(clippy::too_many_lines)] // One C function, kept in C's shape.
pub(crate) fn conv_test(
    m: &Model,
    h: &Instance,
    rhs_old: &dyn Fn(i32) -> f64,
    state0: &dyn Fn(usize) -> f64,
    mode: u32,
    reltol: f64,
    abstol: f64,
) -> bool {
    let ty = f64::from(m.r#type);
    let vds = ty * (rhs_old(h.dNodePrime) - rhs_old(h.sNodePrime));
    let vgs = ty * (rhs_old(h.gNodePrime) - rhs_old(h.sNodePrime));
    let vbs = ty * (rhs_old(h.bNodePrime) - rhs_old(h.sNodePrime));
    let vdbs = ty * (rhs_old(h.dbNode) - rhs_old(h.sNodePrime));
    let vsbs = ty * (rhs_old(h.sbNode) - rhs_old(h.sNodePrime));
    let vses = ty * (rhs_old(h.sNode) - rhs_old(h.sNodePrime));
    let vdes = ty * (rhs_old(h.dNode) - rhs_old(h.sNodePrime));

    let vgdo = state0(state::VGS) - state0(state::VDS);
    let vbd = vbs - vds;
    let vdbd = vdbs - vds;
    let vgd = vgs - vds;

    let delvbd = vbd - state0(state::VBD);
    let delvdbd = vdbd - state0(state::VDBD);
    let delvgd = vgd - vgdo;

    let delvds = vds - state0(state::VDS);
    let delvgs = vgs - state0(state::VGS);
    let delvbs = vbs - state0(state::VBS);
    let delvsbs = vsbs - state0(state::VSBS);

    let delvses = vses - (state0(state::VSES));
    let vdedo = state0(state::VDES) - state0(state::VDS);
    let delvdes = vdes - state0(state::VDES);
    let delvded = vdes - vds - vdedo;
    let _ = delvdes;

    let delvbd_jct = if h.rbodyMod == 0 { delvbd } else { delvdbd };
    let delvbs_jct = if h.rbodyMod == 0 { delvbs } else { delvsbs };

    let (Idtot, cdhat, Igstot, cgshat, Igdtot, cgdhat, Igbtot, cgbhat);
    if h.mode >= 0 {
        Idtot = h.cd + h.csub - h.cbd + h.Igidl;
        cdhat = Idtot - h.gbd * delvbd_jct
            + (h.gmbs + h.gbbs + h.ggidlb) * delvbs
            + (h.gm + h.gbgs + h.ggidlg) * delvgs
            + (h.gds + h.gbds + h.ggidld) * delvds;
        Igstot = h.Igs + h.Igcs;
        cgshat = Igstot + (h.gIgsg + h.gIgcsg) * delvgs + h.gIgcsd * delvds + h.gIgcsb * delvbs;
        Igdtot = h.Igd + h.Igcd;
        cgdhat =
            Igdtot + h.gIgdg * delvgd + h.gIgcdg * delvgs + h.gIgcdd * delvds + h.gIgcdb * delvbs;
        Igbtot = h.Igb;
        cgbhat = h.Igb + h.gIgbg * delvgs + h.gIgbd * delvds + h.gIgbb * delvbs;
    } else {
        // C's own "bugfix" spelling of the reverse-mode drain current.
        Idtot = h.cd + h.cbd - h.Igidl;
        cdhat = Idtot + h.gbd * delvbd_jct + h.gmbs * delvbd + h.gm * delvgd
            - (h.gds + h.ggidls) * delvds
            - h.ggidlg * delvgs
            - h.ggidlb * delvbs;
        Igstot = h.Igs + h.Igcd;
        cgshat =
            Igstot + h.gIgsg * delvgs + h.gIgcdg * delvgd - h.gIgcdd * delvds + h.gIgcdb * delvbd;
        Igdtot = h.Igd + h.Igcs;
        cgdhat = Igdtot + (h.gIgdg + h.gIgcsg) * delvgd - h.gIgcsd * delvds + h.gIgcsb * delvbd;
        Igbtot = h.Igb;
        cgbhat = h.Igb + h.gIgbg * delvgd - h.gIgbd * delvds + h.gIgbb * delvbd;
    }

    let Isestot = h.gstot * (state0(state::VSES));
    let cseshat =
        Isestot + h.gstot * delvses + h.gstotd * delvds + h.gstotg * delvgs + h.gstotb * delvbs;
    let Idedtot = h.gdtot * vdedo;
    let cdedhat =
        Idedtot + h.gdtot * delvded + h.gdtotd * delvds + h.gdtotg * delvgs + h.gdtotb * delvbs;

    // Check convergence
    if (h.off == 0) || ((mode & mode::MODEINITFIX) == 0) {
        let tol0 = reltol * max(cdhat.abs(), Idtot.abs()) + abstol;
        let tol1 = reltol * max(cseshat.abs(), Isestot.abs()) + abstol;
        let tol2 = reltol * max(cdedhat.abs(), Idedtot.abs()) + abstol;
        let tol3 = reltol * max(cgshat.abs(), Igstot.abs()) + abstol;
        let tol4 = reltol * max(cgdhat.abs(), Igdtot.abs()) + abstol;
        let tol5 = reltol * max(cgbhat.abs(), Igbtot.abs()) + abstol;
        if ((cdhat - Idtot).abs() >= tol0)
            || ((cseshat - Isestot).abs() >= tol1)
            || ((cdedhat - Idedtot).abs() >= tol2)
        {
            return true;
        }
        if ((cgshat - Igstot).abs() >= tol3)
            || ((cgdhat - Igdtot).abs() >= tol4)
            || ((cgbhat - Igbtot).abs() >= tol5)
        {
            return true;
        }
        let Ibtot = h.cbs + h.cbd - h.Igidl - h.Igisl - h.csub;
        let cbhat = if h.mode >= 0 {
            Ibtot + h.gbd * delvbd_jct + h.gbs * delvbs_jct
                - (h.gbbs + h.ggidlb) * delvbs
                - (h.gbgs + h.ggidlg) * delvgs
                - (h.gbds + h.ggidld) * delvds
                - h.ggislg * delvgd
                - h.ggislb * delvbd
                + h.ggisls * delvds
        } else {
            Ibtot + h.gbs * delvbs_jct + h.gbd * delvbd_jct
                - (h.gbbs + h.ggislb) * delvbd
                - (h.gbgs + h.ggislg) * delvgd
                + (h.gbds + h.ggisld - h.ggidls) * delvds
                - h.ggidlg * delvgs
                - h.ggidlb * delvbs
        };
        let tol6 = reltol * max(cbhat.abs(), Ibtot.abs()) + abstol;
        if (cbhat - Ibtot).abs() > tol6 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::acld::tests::shaped;
    use super::super::host::{mode, node, state};
    use super::{Model, conv_test};

    fn run(h: &super::Instance, rhs: &[f64; node::COUNT], s0: &[f64; state::COUNT]) -> bool {
        let m = Model {
            r#type: 1,
            ..Model::default()
        };
        let rhs_old = |n: i32| usize::try_from(n).map_or(0.0, |n| rhs[n]);
        let state0 = |k: usize| s0[k];
        conv_test(&m, h, &rhs_old, &state0, mode::MODEINITFLOAT, 1e-3, 1e-12)
    }

    #[test]
    fn converged_when_the_iterate_matches_the_load() {
        let mut h = shaped(0, 0, 0);
        h.cd = 1e-3;
        h.gm = 1e-3;
        h.gds = 1e-5;
        let mut rhs = [0.0; node::COUNT];
        rhs[node::DP as usize] = 1.0;
        rhs[node::D as usize] = 1.0;
        rhs[node::G_EXT as usize] = 0.8;
        let mut s0 = [0.0; state::COUNT];
        s0[state::VDS] = 1.0;
        s0[state::VGS] = 0.8;
        s0[state::VDES] = 1.0;
        s0[state::VBD] = -1.0;
        s0[state::VDBD] = -1.0;
        assert!(!run(&h, &rhs, &s0));
        // A large gate step predicts a current change well beyond reltol.
        rhs[node::G_EXT as usize] = 1.8;
        assert!(run(&h, &rhs, &s0));
    }

    #[test]
    fn an_off_instance_is_not_tested_in_initfix() {
        let mut h = shaped(0, 0, 0);
        h.off = 1;
        h.gm = 1.0;
        let mut rhs = [0.0; node::COUNT];
        rhs[node::G_EXT as usize] = 5.0;
        let s0 = [0.0; state::COUNT];
        let m = Model {
            r#type: 1,
            ..Model::default()
        };
        let rhs_old = |n: i32| usize::try_from(n).map_or(0.0, |n| rhs[n]);
        let state0 = |k: usize| s0[k];
        let fix = mode::MODEDCOP | mode::MODEINITFIX;
        assert!(!conv_test(&m, &h, &rhs_old, &state0, fix, 1e-3, 1e-12));
        assert!(conv_test(
            &m,
            &h,
            &rhs_old,
            &state0,
            mode::MODEINITFLOAT,
            1e-3,
            1e-12
        ));
    }
}
