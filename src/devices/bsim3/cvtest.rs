//! `b3cvtest.c` (`BSIM3convTest`): the device convergence test `NIiter`
//! runs after a solve, on the instance the last load left.
#![allow(non_snake_case)] // C's member names.

use super::fields::{Instance, Model};
use super::host::{mode, state};

/// C's `MAX` macro.
fn MAX(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// `BSIM3convTest` for one instance: `true` when C would increment
/// `CKTnoncon` (not converged). `rhs_old` reads `CKTrhsOld` at a local node,
/// `state0` reads `CKTstate0`, and `ckt_mode` is `CKTmode`.
pub(crate) fn conv_test(
    model: &Model,
    here: &Instance,
    rhs_old: &dyn Fn(i32) -> f64,
    state0: &dyn Fn(usize) -> f64,
    ckt_mode: u32,
    reltol: f64,
    abstol: f64,
) -> bool {
    let type_ = f64::from(model.r#type);
    let vbs = type_ * (rhs_old(here.bNode) - rhs_old(here.sNodePrime));
    let vgs = type_ * (rhs_old(here.gNode) - rhs_old(here.sNodePrime));
    let vds = type_ * (rhs_old(here.dNodePrime) - rhs_old(here.sNodePrime));
    let vbd = vbs - vds;
    let vgd = vgs - vds;
    let vgdo = state0(state::VGS) - state0(state::VDS);
    let delvbs = vbs - state0(state::VBS);
    let delvbd = vbd - state0(state::VBD);
    let delvgs = vgs - state0(state::VGS);
    let delvds = vds - state0(state::VDS);
    let delvgd = vgd - vgdo;

    let mut cd = here.cd - here.cbd;
    let cdhat = if here.mode >= 0 {
        cd += here.csub;
        cd - here.gbd * delvbd
            + (here.gmbs + here.gbbs) * delvbs
            + (here.gm + here.gbgs) * delvgs
            + (here.gds + here.gbds) * delvds
    } else {
        cd + (here.gmbs - here.gbd) * delvbd + here.gm * delvgd - here.gds * delvds
    };

    if (here.off == 0) || ((ckt_mode & mode::MODEINITFIX) == 0) {
        let tol = reltol * MAX(cdhat.abs(), cd.abs()) + abstol;
        if (cdhat - cd).abs() >= tol {
            return true;
        }
        let cbs = here.cbs;
        let cbd = here.cbd;
        let cbhat = if here.mode >= 0 {
            cbs + cbd - here.csub + here.gbd * delvbd + (here.gbs - here.gbbs) * delvbs
                - here.gbgs * delvgs
                - here.gbds * delvds
        } else {
            cbs + cbd - here.csub + here.gbs * delvbs + (here.gbd - here.gbbs) * delvbd
                - here.gbgs * delvgd
                + here.gbds * delvds
        };
        let tol = reltol * MAX(cbhat.abs(), (cbs + cbd - here.csub).abs()) + abstol;
        if (cbhat - (cbs + cbd - here.csub)).abs() > tol {
            return true;
        }
    }
    false
}
