//! Parker-Skellern JFET/MESFET, JFET level 2 (`jfet2`).
//!
//! C references, read as behaviour only:
//!
//! - `jfet2/jfet2.c` and `jfet2parm.h` (`JFET2pTable`, `JFET2mPTable`): the
//!   instance and model setters; `jfet2mpar.c`/`jfet2par.c` apply them in card
//!   order and `jfet2set.c` (`JFET2setup`) the defaults, internal nodes and
//!   state layout;
//! - `jfet2/jfet2temp.c` (`JFET2temp`): IS, PB (`tGatePot`), CGS/CGD scaling
//!   and `vcrit`, then `psmodel.c` `PSinstanceinit` (`xiwoo`, `za`, `alpha`,
//!   `d3`);
//! - `jfet2/psmodel.c`: `PSids` (gate diodes with breakdown, the
//!   subthreshold/dual power-law/velocity-saturation channel, the rate
//!   dependent threshold modulation through the delayed gate voltages
//!   `vgstrap`/`vtrap` and the thermal reduction through the average power
//!   `pave`), `qgg`/`PScharge` (Statz gate charge) and `PSacload`;
//! - `jfet2/jfet2load.c` (`JFET2load`): `MODEINITJCT` start voltages,
//!   `DEVpnjlim`/`DEVfetlim` limiting, inverse mode, charge integration and
//!   the stamps; `jfet2acld.c` (`JFET2acLoad`), `jfet2trun.c`
//!   (`JFET2trunc`), `jfet2ask.c` (`JFET2ask`) and `jfet2ic.c`.
//!
//! The load and the AC load follow C operation by operation through the
//! level-1 frame they share (see [`crate::devices::jfet`]), so the port is
//! bit-identical to a C build without fused multiply-adds.
//!
//! # State
//!
//! C's `JFET2numStates` layout. Besides the gate charges, the model keeps
//! three filtered quantities from one accepted point to the next (C
//! `JFET2vgstrap`, `JFET2vtrap`, `JFET2pave`): in transient each load
//! evaluates `x = h x(accepted) + (1 - h) x(now)` with
//! `h = (tau / (tau + dt / 4))^4` (`TAUG` for the gate voltages, `TAUD` for
//! the power), reading only the last *accepted* value. Outside transient
//! `h = 0`. As in C the slots follow `PSids`' arguments, so in inverse mode
//! the "gate-drain" trap holds the gate-source voltage.
//!
//! The Statz gate charge of `PScharge` is incremental in transient:
//! `qgs = qgs(accepted) + (q(vgs, vgd) - q(vgs1, vgd) + q(vgs, vgd1) -
//! q(vgs1, vgd1)) / 2` with the accepted junction voltages `vgs1`, `vgd1`
//! (and symmetrically for `qgd`). As in C a plain operating point leaves the
//! charge slots alone, while the `uic` initial load and DC sweeps store the
//! total charge in both (`PScharge` outside transient), so truncation
//! control (`jfet2trun.c`, `CKTterr` on both charges) sees C's values.
//!
//! As in C, the Newton matrix stamps each gate charge as a two-terminal
//! capacitance (no `d qgs / d vgd` cross terms) and integrates `CDS * vds`
//! into the drain current without stamping its conductance.
//!
//! # Deliberate divergences
//!
//! - As for level 1: no bypass, the held `off` rule, and `FC > 0.95` is
//!   rejected instead of clamped. `PSids` also writes `POWR_BEFORE` (`CKTstate1`) outside
//!   transient; that value is only ever multiplied by `h = 0` there and is
//!   replaced before the first transient step, so the port does not write
//!   it.
//! - C has no pole-zero load for level 2 (`DEVpzLoad = NULL`, so `.pz`
//!   silently omits the device); the port refuses `.pz` explicitly.
//! - Parameters whose zero or negative values divide by zero or take roots
//!   of negative numbers in `psmodel.c` are range-checked (`VBD`, `XI`, `P`,
//!   `Q` positive; `Z`, `TAUD`, `TAUG`, `VST`, `DELTA`, `IBD`, `CDS`
//!   nonnegative), and any nonfinite evaluation is an error.
//!
//! `.noise` (`jfet2noi.c`), `.disto` (C has no `DEVdisto` for level 2) and
//! `.sens` fail with explicit errors.

use crate::devices::jfet::{
    self, IC_COMPONENTS, Instance, Junctions, K_OVER_Q, Loaded, p, required,
};
use crate::devices::limiting;
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::{
    AnalysisMode, Device, LinearContext, ModelContext, ResolvedModel, StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeTable, Real, SpiceError, SpiceResult};

/// `psmodel.c` `FX`: below `FX` thermal voltages a junction exponential is
/// dropped (and `FX * vst` is the numerical cut-off of the channel).
const FX: Real = -10.0;
/// `psmodel.c` `MX`: the largest exponential argument.
const MX: Real = 40.0;
/// `psmodel.c` `EMX = exp(MX)`.
#[allow(clippy::excessive_precision)]
const EMX: Real = 2.353852668370199842e17;
/// `psmodel.h` `FOURTH`: the relaxation filters use a quarter step.
const FOURTH: Real = 0.25;
/// `jfet2temp.c` uses a fixed 1.11 eV band gap for IS.
const EG: Real = 1.11;

/// Slots of the state vector: C's `JFET2numStates` layout (`jfet2defs.h`),
/// level 1's followed by the drain-source charge and the filters.
mod slot {
    pub(super) use crate::devices::jfet::slot::*;
    /// Drain-source charge `CDS * vds` (C `JFET2qds`) and its derivative.
    pub(super) const QDS: usize = 13;
    /// Average power (C `JFET2pave`) and filtered gate voltages (C
    /// `JFET2vtrap`, `JFET2vgstrap`).
    pub(super) const PAVE: usize = 15;
    pub(super) const VGDTRAP: usize = 16;
    pub(super) const VGSTRAP: usize = 17;
    /// C `JFET2unknown`: allocated, never written.
    pub(super) const UNKNOWN: usize = 18;
    pub(super) const COUNT: usize = 19;
}

/// Model setters of `jfet2parm.h` with the defaults of `jfet2set.c`. The two
/// spellings of VTO (`vt0`, `vto`) and of PB (`vbi`, `pb`) and `hfgam`
/// (default: `lfgam`) have no schema default.
const MODEL: &[P] = &[
    p("acgam", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
    p("beta", U::AmperePerVoltSquared, D::NonNegative, Some(1e-4)),
    p("cds", U::Farad, D::NonNegative, Some(0.)),
    p("cgd", U::Farad, D::NonNegative, Some(0.)),
    p("cgs", U::Farad, D::NonNegative, Some(0.)),
    p("delta", U::Dimensionless, D::NonNegative, Some(0.)),
    p("hfeta", U::Dimensionless, D::Finite, Some(0.)),
    p("hfe1", U::InverseVolt, D::Finite, Some(0.)),
    p("hfe2", U::InverseVolt, D::Finite, Some(0.)),
    p("hfg1", U::InverseVolt, D::Finite, Some(0.)),
    p("hfg2", U::InverseVolt, D::Finite, Some(0.)),
    p("mvst", U::InverseVolt, D::Finite, Some(0.)),
    p("mxi", U::Dimensionless, D::Finite, Some(0.)),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("ibd", U::Ampere, D::NonNegative, Some(0.)),
    p("is", U::Ampere, D::Positive, Some(1e-14)),
    // jfet2noi.c input: accepted for model compatibility; `.noise` itself is
    // refused until jfet2noi.c is ported.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.)),
    p("lfgam", U::Dimensionless, D::Finite, Some(0.)),
    p("lfg1", U::InverseVolt, D::Finite, Some(0.)),
    p("lfg2", U::InverseVolt, D::Finite, Some(0.)),
    p("n", U::Dimensionless, D::Positive, Some(1.)),
    p("p", U::Dimensionless, D::Positive, Some(2.)),
    p("vbi", U::Volt, D::Positive, None),
    p("pb", U::Volt, D::Positive, None),
    p("q", U::Dimensionless, D::Positive, Some(2.)),
    p("rd", U::Ohm, D::NonNegative, Some(0.)),
    p("rs", U::Ohm, D::NonNegative, Some(0.)),
    p("taud", U::Second, D::NonNegative, Some(0.)),
    p("taug", U::Second, D::NonNegative, Some(0.)),
    p("vbd", U::Volt, D::Positive, Some(1.)),
    // "version number of PS model": stored but read nowhere in jfet2/.
    p("ver", U::Dimensionless, D::Finite, Some(0.)),
    p("vst", U::Volt, D::NonNegative, Some(0.)),
    p("vt0", U::Volt, D::Finite, None),
    p("vto", U::Volt, D::Finite, None),
    p("xc", U::Dimensionless, D::Finite, Some(0.)),
    p("xi", U::Dimensionless, D::Positive, Some(1000.)),
    p("z", U::Dimensionless, D::NonNegative, Some(1.)),
    p("hfgam", U::Dimensionless, D::Finite, None),
    p("tnom", U::Celsius, D::Temperature, None),
];

/// Validated model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
struct Model {
    /// C `JFET2type`: +1 for NJF, -1 for PJF.
    pol: Real,
    acgam: Real,
    beta: Real,
    cds: Real,
    cgd: Real,
    cgs: Real,
    delta: Real,
    hfeta: Real,
    hfe1: Real,
    hfe2: Real,
    hfg1: Real,
    hfg2: Real,
    hfgam: Real,
    mvst: Real,
    mxi: Real,
    fc: Real,
    ibd: Real,
    is: Real,
    lambda: Real,
    lfgam: Real,
    lfg1: Real,
    lfg2: Real,
    n: Real,
    p: Real,
    /// C `JFET2phi` (`vbi`/`pb`).
    pb: Real,
    q: Real,
    rd: Real,
    rs: Real,
    taud: Real,
    taug: Real,
    vbd: Real,
    vst: Real,
    vto: Real,
    xc: Real,
    xi: Real,
    z: Real,
    tnom: Option<Real>,
}

/// Every temperature- and area-dependent quantity of one load
/// (`jfet2temp.c`, `PSinstanceinit` and the area factors of `psmodel.c`).
#[derive(Debug, Clone, Copy)]
struct Operating {
    /// `kT/q` at the device temperature (limiting and `vcrit`).
    vt: Real,
    /// `N kT/q` (`psmodel.h` `NVT`).
    nvt: Real,
    /// C `JFET2vcrit` (from the per-device saturation current).
    vcrit: Real,
    /// `JFET2tSatCur * area`, `IBD * area`, `BETA * area`, `DELTA / area`.
    isat: Real,
    ibd: Real,
    beta: Real,
    delta: Real,
    /// C `JFET2tGatePot` (`VBI`).
    pb: Real,
    /// `JFET2tCGS * area`, `JFET2tCGD * area`, `CDS * area`.
    czgs: Real,
    czgd: Real,
    capds: Real,
    /// C `JFET2corDepCap` (`VMAX`), `JFET2xiwoo`, model `JFET2za`,
    /// `JFET2alpha`, `JFET2d3`.
    vmax: Real,
    xi_woo: Real,
    za: Real,
    alpha: Real,
    d3: Real,
}

/// The accepted history a transient load filters against (`psmodel.h`
/// `VGSTRAP_BEFORE`, `VGDTRAP_BEFORE`, `POWR_BEFORE`) and the step
/// (`STEP = CKTdelta`).
#[derive(Debug, Clone, Copy)]
struct Memory {
    step: Real,
    vgstrap: Real,
    vgdtrap: Real,
    powr: Real,
}

/// `PSids` at its argument voltages: the drain current, its partials with
/// respect to `vgs` at fixed `vds` and `vds` at fixed `vgs`, both gate diode
/// currents and conductances, and the new filtered state.
#[derive(Debug, Clone, Copy)]
struct Ids {
    current: Real,
    gm: Real,
    gds: Real,
    igs: Real,
    igd: Real,
    ggs: Real,
    ggd: Real,
    vgstrap: Real,
    vgdtrap: Real,
    pave: Real,
}

/// Everything one normalized bias point loads, per device (before `m`), in
/// `jfet2load.c`'s orientation after the inverse-mode exchange.
#[derive(Debug, Clone, Copy)]
struct Point {
    /// Channel current from d' to s' and its partials (`cd + cgd`, `gm`,
    /// `gds` of `jfet2load.c`).
    channel: (Real, Real, Real),
    /// Gate-source and gate-drain diode currents and conductances (with
    /// `gmin`).
    gs: (Real, Real),
    gd: (Real, Real),
    /// The filtered state to store (`vgstrap`, `vtrap`, `pave`).
    vgstrap: Real,
    vgdtrap: Real,
    pave: Real,
}

/// `psmodel.c` `qgg`: Statz gate charge and its partials `(q, cgs, cgd)`.
#[allow(clippy::too_many_arguments)]
fn qgg(
    vgs: Real,
    vgd: Real,
    gamma: Real,
    pb: Real,
    alpha: Real,
    vto: Real,
    vmax: Real,
    xc: Real,
    cgso: Real,
    cgdo: Real,
) -> (Real, Real, Real) {
    let vds = vgs - vgd;
    let d1_xc = 1. - xc;
    let vert = (vds * vds + alpha).sqrt();
    let veff = 0.5 * (vgs + vgd + vert) + gamma * vds;
    let vnr = d1_xc * (veff - vto);
    let vnrt = (vnr * vnr + 0.04).sqrt();
    let vnew = veff + 0.5 * (vnrt - vnr);
    let (ext, qrt, cgso_eff);
    if vnew < vmax {
        ext = 0.;
        qrt = (1. - vnew / pb).sqrt();
        cgso_eff = 0.5 * cgso / qrt * (1. + xc + d1_xc * vnr / vnrt);
    } else {
        let vx = 0.5 * (vnew - vmax);
        let par = 1. + vx / (pb - vmax);
        qrt = (1. - vmax / pb).sqrt();
        ext = vx * (1. + par) / qrt;
        cgso_eff = 0.5 * cgso / qrt * (1. + xc + d1_xc * vnr / vnrt) * par;
    }
    let cpm = vds / vert;
    let cplus = 0.5 * (1. + cpm);
    let cminus = cplus - cpm;
    let cgs = cgso_eff * (cplus + gamma) + cgdo * (cminus + gamma);
    let cgd = cgso_eff * (cminus - gamma) + cgdo * (cplus - gamma);
    (
        cgso * ((pb + pb) * (1. - qrt) + ext) + cgdo * (veff - vert),
        cgs,
        cgd,
    )
}

/// One forward gate diode of `PSids` (`isat`, `N Vt`, `gmin`): current and
/// conductance.
fn forward_diode(v: Real, isat: Real, nvt: Real, gmin: Real) -> (Real, Real) {
    let arg = v / nvt;
    if arg > FX {
        if arg < MX {
            let zz = isat * arg.exp();
            (zz - isat + gmin * v, zz / nvt + gmin)
        } else {
            let zz = isat * EMX;
            (zz * (arg - MX + 1.) - isat + gmin * v, zz / nvt + gmin)
        }
    } else {
        (-isat + gmin * v, gmin)
    }
}

/// Adds `PSids`' reverse "breakdown" conduction (`IBD`, `VBD`) to a diode.
fn breakdown((mut i, mut g): (Real, Real), v: Real, ibd: Real, vbd: Real) -> (Real, Real) {
    let arg = -v / vbd;
    if arg > FX {
        if arg < MX {
            let zz = ibd * arg.exp();
            g += zz / vbd;
            i -= zz - ibd;
        } else {
            let zz = ibd * EMX;
            g += zz / vbd;
            i -= zz * ((arg - MX) + 1.) - ibd;
        }
    } else {
        i += ibd;
    }
    (i, g)
}

/// `(tau / (tau + step / 4))^4` (`psmodel.c`).
fn relaxation(tau: Real, step: Real) -> Real {
    let mut h = tau / (tau + step * FOURTH);
    h *= h;
    h *= h;
    h
}

impl Model {
    /// `psmodel.c` `PSids` at (`vgs`, `vgd`) with `vgs >= vgd`; `memory` is
    /// the accepted filter state of a transient load (`TRAN_ANAL`).
    #[allow(clippy::many_single_char_names, clippy::similar_names)]
    fn ids(
        &self,
        op: &Operating,
        vgs: Real,
        vgd: Real,
        gmin: Real,
        memory: Option<&Memory>,
    ) -> Ids {
        // Gate junction diodes.
        let (igs, ggs) = breakdown(
            forward_diode(vgs, op.isat, op.nvt, gmin),
            vgs,
            op.ibd,
            self.vbd,
        );
        let (igd, ggd) = breakdown(
            forward_diode(vgd, op.isat, op.nvt, gmin),
            vgd,
            op.ibd,
            self.vbd,
        );

        // Drain current and derivatives.
        let vdst = vgs - vgd;
        // Rate-dependent threshold modulation.
        let (h, vgdtrap, vgstrap) = match memory {
            Some(memory) => {
                let h = relaxation(self.taug, memory.step);
                (
                    h,
                    h * memory.vgdtrap + (1. - h) * vgd,
                    h * memory.vgstrap + (1. - h) * vgs,
                )
            }
            None => (0., vgd, vgs),
        };
        let (lfg, lfg1, lfg2) = (self.lfgam, self.lfg1, self.lfg2);
        let (hfg, hfg1, hfg2) = (self.hfgam, self.hfg1, self.hfg2);
        let (hfe, hfe1, hfe2) = (self.hfeta, self.hfe1, self.hfe2);
        let mut vgst = vgs - self.vto;
        vgst -= (lfg - lfg1 * vgstrap + lfg2 * vgdtrap) * vgdtrap;
        let eta = hfe - hfe1 * vgdtrap + hfe2 * vgstrap;
        let dvgs = vgstrap - vgs;
        vgst += eta * dvgs;
        let gam = hfg - hfg1 * vgstrap + hfg2 * vgdtrap;
        let dvgd = vgdtrap - vgd;
        vgst += gam * dvgd;
        let (mut idrain, mut gm, mut gds);
        {
            // Exponential subthreshold effect ids(vgst, vdst).
            let mvst = self.mvst;
            let vst = self.vst * (1. + mvst * vdst);
            if vgst > FX * vst {
                let (vgt, subfac);
                let large = MX * vst;
                if vgst > large {
                    // Numerically large.
                    subfac = EMX + 1.;
                    vgt = (EMX / subfac) * (vgst - large) + large;
                } else {
                    // Limit gate bias exponentially.
                    subfac = 1. + (vgst / vst).exp();
                    vgt = vst * subfac.ln();
                }
                // Dual power-law ids(vgt, vdst).
                let m_q = self.q;
                let p_m_q = self.p - m_q;
                let dvpd_dvdst = op.d3 * vgt.powf(p_m_q);
                let vdp = vdst * dvpd_dvdst;
                // Early saturation effect ids(vgt, vdp).
                let za = op.za;
                let mxi = self.mxi;
                let vsat_fac = vgt / (mxi * vgt + op.xi_woo);
                let vsat = vgt / (1. + vsat_fac);
                let aa = za * vdp + vsat / 2.0;
                let a_aa = aa - vsat;
                let knee = vsat * vsat * self.z / 4.0;
                let rpt = (aa * aa + knee).sqrt();
                let a_rpt = (a_aa * a_aa + knee).sqrt();
                let vdt = rpt - a_rpt;
                let dvdt_dvdp = za * (aa / rpt - a_aa / a_rpt);
                let dvdt_dvgt = (vdt - vdp * dvdt_dvdp) * (1. + mxi * vsat_fac * vsat_fac)
                    / (1. + vsat_fac)
                    / vgt;
                // Intrinsic Q-law FET equation ids(vgt, vdt).
                gds = (vgt - vdt).powf(m_q - 1.);
                gm = vgt.powf(m_q - 1.) - gds;
                idrain = vdt * gds + vgt * gm;
                gds *= m_q;
                gm *= m_q;
                gm += gds * dvdt_dvgt;
                gds *= dvdt_dvdp;
                gm += gds * p_m_q * vdp / vgt;
                gds *= dvpd_dvdst;
                let arg = 1. - 1. / subfac;
                if vst != 0. {
                    gds += gm * self.vst * mvst * (vgt - vgst * arg) / vst;
                }
                gm *= arg;
            } else {
                // In extreme cut-off (numerically).
                idrain = 0.;
                gm = 0.;
                gds = 0.;
            }
        }
        let arg = h * gam
            + (1. - h) * (hfe1 * dvgs - hfg2 * dvgd + 2. * lfg2 * vgdtrap - lfg1 * vgstrap + lfg);
        gds += gm * arg;
        gm *= 1. - h * eta + (1. - h) * (hfe2 * dvgs - hfg1 * dvgd + lfg1 * vgdtrap) - arg;

        // Channel length modulation and beta scaling.
        let arg = op.beta * (1. + self.lambda * vdst);
        gm *= arg;
        gds = op.beta * self.lambda * idrain + gds * arg;
        idrain *= arg;

        // Thermal reduction of drain current.
        let delta = op.delta;
        let (h, pave, powr_before) = match memory {
            Some(memory) => {
                let h = relaxation(self.taud, memory.step);
                let pave = h * memory.powr + (1. - h) * vdst * idrain;
                (h, pave, memory.powr)
            }
            // `POWR_NOW = POWR_BEFORE = pAverage`.
            None => {
                let pave = vdst * idrain;
                (0., pave, pave)
            }
        };
        let pfac = 1. + pave * delta;
        idrain /= pfac;
        let arg = (h * delta * powr_before + 1.) / pfac / pfac;
        Ids {
            current: idrain,
            gm: gm * arg,
            gds: gds * arg - (1. - h) * delta * idrain * idrain,
            igs,
            igd,
            ggs,
            ggd,
            vgstrap,
            vgdtrap,
            pave,
        }
    }

    /// `PScharge`'s `qgg` with this device's parameters.
    fn qgg(&self, op: &Operating, vgs: Real, vgd: Real) -> (Real, Real, Real) {
        qgg(
            vgs, vgd, self.acgam, op.pb, op.alpha, self.vto, op.vmax, self.xc, op.czgs, op.czgd,
        )
    }

    /// `psmodel.c` `PSacload`: the AC transconductance and output
    /// conductance `(Gm, xGm, Gds, xGds)` at angular frequency `omega` from
    /// the operating point's `gm`, `gds` and drain current `ids` (C's stored
    /// `JFET2cd`).
    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    fn acload(
        &self,
        op: &Operating,
        vgs: Real,
        vgd: Real,
        ids: Real,
        omega: Real,
        gm: Real,
        gds: Real,
    ) -> (Real, Real, Real, Real) {
        let vds = vgs - vgd;
        let lfgam = self.lfgam;
        let lfg1 = self.lfg1;
        let lfg2 = self.lfg2 * vgd;
        let hfg1 = self.hfg1;
        let hfg2 = self.hfg2 * vgd;
        let hfeta = self.hfeta;
        let hfe1 = self.hfe1;
        let hfe2 = self.hfe2 * vgs;
        let hfgam = self.hfgam - hfg1 * vgs + hfg2;
        let eta = hfeta - hfe1 * vgd + hfe2;
        let lfga = lfgam - lfg1 * vgs + lfg2 + lfg2;
        let gmo = gm / (1. - lfga + lfg1 * vgd);

        let wtg = self.taug * omega;
        let wtgdet = 1. + wtg * wtg;
        let gwtgdet = gmo / wtgdet;

        let arg = hfgam - lfga;
        let gdsi = arg * gwtgdet;
        let gdsr = arg * gmo - gdsi;
        let gmi = (eta + lfg1 * vgd) * gwtgdet + gdsi;

        let xgds = wtg * gdsi;
        let gds = gds + gdsr;
        let xgm = -wtg * gmi;
        let gm = gmi + gmo * (1. - eta - hfgam);

        let delta = op.delta;
        let wtd = self.taud * omega;
        let wtddet = 1. + wtd * wtd;
        let fac = delta * ids;
        let del = 1. / (1. - fac * vds);
        let dd = (del - 1.) / wtddet;
        let dr = del - dd;
        let di = wtd * dd;

        let cdsqr = fac * ids * del * wtd / wtddet;
        (
            dr * gm - di * xgm,
            di * gm + dr * xgm,
            dr * gds - di * xgds + cdsqr * wtd,
            di * gds + dr * xgds + cdsqr,
        )
    }
}

/// Parker-Skellern JFET with series resistance, Statz gate charge, a
/// drain-source capacitance and temperature scaling.
#[derive(Debug)]
pub struct Jfet2 {
    name: String,
    /// External d, g, s followed by any internal source/drain nodes.
    terminals: Vec<NodeId>,
    /// d', g, s': the nodes the intrinsic device sees.
    inner: [NodeId; 3],
    model: Model,
    instance: Instance,
    /// `OFF` and `IC-VDS`/`IC-VGS` (C `JFET2off`, `JFET2icVDS`, `JFET2icVGS`).
    initial: crate::devices::initial::InstanceInitial,
}

impl Jfet2 {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 2 || i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: JFET2 needs level 2 and three terminals",
                i.name
            )));
        }
        let (initial, setters) = crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)?;
        let (pol, scalars) = jfet::card_scalars(resolved);
        // `inpgmod.c::create_model`: see `Jfet::instantiate`.
        let (scalars, model_defaults) = crate::devices::schema::split_model_setters(
            scalars,
            &ScalarSchema { parameters: MODEL },
            &ScalarSchema {
                parameters: crate::devices::jfet::INSTANCE,
            },
        );
        let m = ScalarSchema { parameters: MODEL }.validate(scalars, &resolved.card().location)?;
        let r = |name: &str| required(&m, name);
        // JFET2_MOD_VTO (`vt0`, alias `vto`) and JFET2_MOD_PB (`vbi`, alias
        // `pb`): setters apply in card order, so the later spelling wins.
        let vto = jfet::last_setter(&m, &["vt0", "vto"]).unwrap_or(-2.);
        let pb = jfet::last_setter(&m, &["vbi", "pb"]).unwrap_or(1.);
        let lfgam = r("lfgam")?;
        let model = Model {
            pol,
            acgam: r("acgam")?,
            beta: r("beta")?,
            cds: r("cds")?,
            cgd: r("cgd")?,
            cgs: r("cgs")?,
            delta: r("delta")?,
            hfeta: r("hfeta")?,
            hfe1: r("hfe1")?,
            hfe2: r("hfe2")?,
            hfg1: r("hfg1")?,
            hfg2: r("hfg2")?,
            // jfet2parm.h: HFGAM defaults to the model's LFGAM.
            hfgam: m.get("hfgam").map_or(lfgam, |v| v.value),
            mvst: r("mvst")?,
            mxi: r("mxi")?,
            fc: r("fc")?,
            ibd: r("ibd")?,
            is: r("is")?,
            lambda: r("lambda")?,
            lfgam,
            lfg1: r("lfg1")?,
            lfg2: r("lfg2")?,
            n: r("n")?,
            p: r("p")?,
            pb,
            q: r("q")?,
            rd: r("rd")?,
            rs: r("rs")?,
            taud: r("taud")?,
            taug: r("taug")?,
            vbd: r("vbd")?,
            vst: r("vst")?,
            vto,
            xc: r("xc")?,
            xi: r("xi")?,
            z: r("z")?,
            tnom: m.get("tnom").map(|v| v.value),
        };
        jfet::check_fc(&i.name, model.fc)?;
        let instance = jfet::instance_setters(&model_defaults, &setters, &i.location)?;
        let mut device = Self {
            name: i.name.clone(),
            terminals: vec![],
            inner: [NodeId::GROUND; 3],
            model,
            instance,
            initial,
        };
        // Validate every derivation before interning nodes.
        device.operating(context)?;
        (device.terminals, device.inner) = jfet::intern_nodes(i, nodes, model.rd, model.rs)?;
        Ok(Box::new(device))
    }

    /// `jfet2temp.c` and `PSinstanceinit` for this instance, with
    /// `psmodel.c`'s area scaling.
    fn operating(&self, context: &ModelContext) -> SpiceResult<Operating> {
        let (model, instance) = (&self.model, &self.instance);
        let (tnom, temp) = jfet::temperatures(&self.name, model.tnom, instance, context)?;
        let junction = jfet::gate_junction(model.pb, tnom, temp);
        let vt = temp * K_OVER_Q;
        let ratio1 = temp / tnom - 1.;
        let saturation = model.is * (ratio1 * EG / vt).exp();
        let pb = junction.pb;
        let tcgs = model.cgs * junction.cjfact * junction.cjfact1;
        let tcgd = model.cgd * junction.cjfact * junction.cjfact1;
        // PSinstanceinit.
        let woo = pb - model.vto;
        let xi_woo = model.xi * woo;
        let area = instance.area;
        let operating = Operating {
            vt,
            nvt: temp * K_OVER_Q * model.n,
            vcrit: limiting::critical_voltage(vt, saturation),
            isat: saturation * area,
            ibd: model.ibd * area,
            beta: model.beta * area,
            delta: model.delta / area,
            pb,
            czgs: tcgs * area,
            czgd: tcgd * area,
            capds: model.cds * area,
            vmax: model.fc * pb,
            xi_woo,
            za: (1. + model.z).sqrt() / 2.,
            alpha: xi_woo * xi_woo / (model.xi + 1.) / (model.xi + 1.) / 4.,
            d3: model.p / model.q / woo.powf(model.p - model.q),
        };
        let finite = [
            operating.nvt,
            operating.vcrit,
            operating.isat,
            operating.beta,
            operating.delta,
            operating.czgs,
            operating.czgd,
            operating.vmax,
            operating.xi_woo,
            operating.alpha,
            operating.d3,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !(finite && pb.is_finite() && pb > 0.)
            || operating.isat <= 0.
            || operating.czgs < 0.
            || operating.czgd < 0.
        {
            return Err(SpiceError::circuit(format!(
                "{}: JFET2 parameters are out of range at {temp:.2} K (nonfinite value, \
                 VBI - VTO <= 0 with P != Q, or nonpositive PB/IS after temperature scaling)",
                self.name
            )));
        }
        Ok(operating)
    }

    /// `jfet2load.c`'s DC evaluation at normalized `vgs`, `vgd`: `PSids` in
    /// normal mode, or with the junctions exchanged in inverse mode (`gds +=
    /// gm`, `gm = -gm`, the current reversed).
    fn point(
        &self,
        op: &Operating,
        vgs: Real,
        vgd: Real,
        gmin: Real,
        memory: Option<&Memory>,
    ) -> SpiceResult<Point> {
        let point = if vgs - vgd < 0. {
            let r = self.model.ids(op, vgd, vgs, gmin, memory);
            Point {
                channel: (-r.current, -r.gm, r.gds + r.gm),
                gs: (r.igd, r.ggd),
                gd: (r.igs, r.ggs),
                vgstrap: r.vgstrap,
                vgdtrap: r.vgdtrap,
                pave: r.pave,
            }
        } else {
            let r = self.model.ids(op, vgs, vgd, gmin, memory);
            Point {
                channel: (r.current, r.gm, r.gds),
                gs: (r.igs, r.ggs),
                gd: (r.igd, r.ggd),
                vgstrap: r.vgstrap,
                vgdtrap: r.vgdtrap,
                pave: r.pave,
            }
        };
        let values = [
            point.channel.0,
            point.channel.1,
            point.channel.2,
            point.gs.0,
            point.gs.1,
            point.gd.0,
            point.gd.1,
            point.vgstrap,
            point.vgdtrap,
            point.pave,
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: format!("JFET2 {}", self.name),
                message: "nonfinite Parker-Skellern channel/junction equations".into(),
            });
        }
        Ok(point)
    }

    /// Normalized `[vgs, vgd]` from physical node voltages of d', g, s'.
    fn normalized(&self, v: [Real; 3]) -> [Real; 2] {
        let pol = self.model.pol;
        [pol * (v[1] - v[2]), pol * (v[1] - v[0])]
    }

    fn numerical(&self, message: &str) -> SpiceError {
        SpiceError::Numerical {
            context: format!("JFET2 {}", self.name),
            message: message.into(),
        }
    }

    /// The bias-dependent ask quantities of `jfet2ask.c` at the solution
    /// `voltage` (no limiting, DC filters): `vgs`, `vgd`, the
    /// multiplicity-scaled `gm`, `gds`, `ggs`, `ggd`, `igd`, and `vtrap`,
    /// `vpave` (normalized polarity, as C stores them). In transient every
    /// quantity but `vgs`/`vgd` depends on the accepted filter state or the
    /// charge companion, so those are refused there.
    fn operating_ask(
        &self,
        key: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        transient: bool,
    ) -> SpiceResult<Option<Real>> {
        if !matches!(
            key,
            "vgs" | "vgd" | "gm" | "gds" | "ggs" | "ggd" | "igd" | "vtrap" | "vpave"
        ) {
            return Ok(None);
        }
        let [vgs, vgd] = self.normalized(self.inner.map(voltage));
        if transient && !matches!(key, "vgs" | "vgd") {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: transient @{}[{key}] (C reads the filtered state and charge companion)",
                    self.name, self.name
                ),
                "src/spicelib/devices/jfet2/jfet2ask.c, psmodel.c",
            ));
        }
        let op = self.operating(context)?;
        let point = self.point(&op, vgs, vgd, context.gmin, None)?;
        let m = self.instance.m;
        Ok(Some(match key {
            "vgs" => vgs,
            "vgd" => vgd,
            "gm" => m * point.channel.1,
            "gds" => m * point.channel.2,
            "ggs" => m * point.gs.1,
            "ggd" => m * point.gd.1,
            "igd" => m * point.gd.0,
            "vtrap" => point.vgdtrap,
            _ => point.pave,
        }))
    }
}

/// Rewrites `slot` with its value before this load (C leaves CKTstate0
/// untouched there): the previous load's, else the accepted or carried one,
/// else zero.
fn carry(states: &mut crate::devices::DeviceState<'_>, slot: usize) -> SpiceResult<()> {
    let value = states
        .iterate(slot)
        .or_else(|| states.accepted(1, slot))
        .or_else(|| states.carried(slot))
        .filter(|v| v.is_finite())
        .unwrap_or(0.);
    states.set(slot, value)
}

impl Device for Jfet2 {
    /// `jfet2set.c`: C's device type and `TSTALLOC` order.
    fn c_device_type(&self) -> crate::devices::setup::CDeviceType {
        crate::devices::setup::CDeviceType::Jfet2
    }

    /// `jfet2set.c` `TSTALLOC` order.
    fn setup_matrix(
        &self,
        setup: &mut crate::devices::setup::MatrixSetup<'_>,
    ) -> SpiceResult<bool> {
        let [d, g, s] = [self.terminals[0], self.terminals[1], self.terminals[2]];
        let [dp, _, sp] = self.inner;
        for (row, col) in [
            (d, dp),
            (g, dp),
            (g, sp),
            (s, sp),
            (dp, d),
            (dp, g),
            (dp, sp),
            (sp, g),
            (sp, s),
            (sp, dp),
            (d, d),
            (g, g),
            (s, s),
            (dp, dp),
            (sp, sp),
        ] {
            setup.nodes(row, col);
        }
        Ok(true)
    }
    /// `jfet2ask.c` scalar asks: `area` (C reports `area * m`), `m`, `temp`,
    /// `dtemp`, `ic-vds` and `ic-vgs` (when given).
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        Ok(jfet::observation_parameter(
            &self.instance,
            &self.initial,
            key,
            context,
        ))
    }
    fn observation_operating(
        &self,
        key: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        transient: bool,
    ) -> SpiceResult<Option<Real>> {
        self.operating_ask(key, context, voltage, transient)
    }
    /// `jfet2ask.c` reports `id`, `ig` and `is` in the device's normalized
    /// polarity (`JFET2cd`, `JFET2cg` are stored before the `type` factor).
    fn observation_current_sign(&self) -> Real {
        self.model.pol
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'j'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn has_start_settings(&self) -> bool {
        self.initial.off
    }
    /// With `TAUG` or `TAUD` the small-signal load of `PSacload` is not
    /// affine in `j omega`, so AC re-assembles it at every frequency (the
    /// operating point itself does not depend on it).
    fn small_signal_depends_on_frequency(&self) -> bool {
        self.model.taug != 0. || self.model.taud != 0.
    }
    fn state_count(&self) -> usize {
        slot::COUNT
    }
    /// `jfet2trun.c`: both gate charges control the timestep (not `qds`).
    fn truncation_slots(&self) -> Vec<usize> {
        vec![slot::QGS, slot::QGD]
    }
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        jfet::instance_parameter(keyword)
    }
    /// `JFET2param` then `JFET2temp`, with the instance schema domains.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let (instance, initial) =
            jfet::swept_instance(&self.name, &self.instance, &self.initial, parameter, value)?;
        let device = Self {
            name: self.name.clone(),
            terminals: self.terminals.clone(),
            inner: self.inner,
            model: self.model,
            instance,
            initial,
        };
        device.operating(context)?;
        Ok(Box::new(device))
    }
    /// `JFET2load` (`jfet2load.c`) without bypass, which ngspice disables by
    /// default. As for level 1 the load's own `icheck`/`cghat`/`cdhat` test
    /// is the convergence test (`DEVconvTest = NULL`).
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("JFET2 AC needs small-signal assembly"));
        }
        let op = self.operating(&context.model_context())?;
        let pol = self.model.pol;
        let raw = self.normalized(self.inner.map(|n| context.node_voltage(n)));
        let ic =
            jfet::start_conditions(&self.terminals, &self.initial, |n| context.node_voltage(n));
        let integration = context.integration;
        // C `xfact = CKTdelta / CKTdeltaOld[1]`; one in a `.dc` sweep.
        let xfact = integration.map_or(1., crate::maths::integrator::Coefficients::predictor_ratio);
        let ([vgs, vgd], start) = jfet::select_junctions(
            Junctions {
                pol,
                off: self.initial.off,
                vt: op.vt,
                vcrit: op.vcrit,
                vto: self.model.vto,
            },
            &context.states,
            raw,
            ic,
            xfact,
        );
        let transient = context.mode.is_transient();
        if transient && integration.is_none() {
            return Err(SpiceError::circuit(
                "JFET2 transient needs companion integration",
            ));
        }
        let state1 = |states: &crate::devices::DeviceState<'_>, s: usize, what: &str| {
            states
                .accepted(1, s)
                .ok_or_else(|| self.numerical(&format!("missing accepted {what}")))
        };
        // psmodel.h: TRAN_ANAL is MODETRAN, the `BEFORE` values CKTstate1
        // and STEP `CKTdelta`.
        let memory = if let AnalysisMode::Transient { dt, .. } = context.mode {
            Some(Memory {
                step: dt,
                vgstrap: state1(&context.states, slot::VGSTRAP, "vgstrap")?,
                vgdtrap: state1(&context.states, slot::VGDTRAP, "vtrap")?,
                powr: state1(&context.states, slot::PAVE, "pave")?,
            })
        } else {
            None
        };
        let point = self.point(&op, vgs, vgd, context.gmin, memory.as_ref())?;
        let vds = vgs - vgd;
        let (cdrain, gm, gds) = point.channel;
        let ((cgs, ggs), (cgd, ggd)) = (point.gs, point.gd);
        let mut l = Loaded {
            vgs,
            vgd,
            vds,
            cg: cgs + cgd,
            cgd,
            cd: cdrain - cgd,
            gm,
            gds,
            ggs,
            ggd,
        };
        let initial_transient = context.states.phase() == crate::devices::IterationPhase::Predict
            && context.states.initial_transient();

        // Charge storage: C evaluates the charges in transient, `.dc` sweeps,
        // MODEINITSMSIG and the `uic` operating point, and integrates them in
        // transient only. Elsewhere the slots keep their previous values, as
        // C's CKTstate0 does.
        if transient || context.mode == AnalysisMode::DcSweep || context.states.initial_conditions()
        {
            let capds = op.capds;
            let (qgs, qgd, capgs, capgd) = if transient {
                // PScharge in transient: the Statz charge incremented from
                // the accepted point (VGS1, VGD1, QGS_BEFORE, QGD_BEFORE).
                let vgs1 = state1(&context.states, slot::VGS, "vgs")?;
                let vgd1 = state1(&context.states, slot::VGD, "vgd")?;
                let qgs1 = state1(&context.states, slot::QGS, "qgs")?;
                let qgd1 = state1(&context.states, slot::QGD, "qgd")?;
                let (qgga, cgsna, cgdna) = self.model.qgg(&op, vgs, vgd);
                let (qggb, _, cgdnb) = self.model.qgg(&op, vgs1, vgd);
                let (qggc, cgsnc, _) = self.model.qgg(&op, vgs, vgd1);
                let (qggd, _, _) = self.model.qgg(&op, vgs1, vgd1);
                (
                    qgs1 + 0.5 * (qgga - qggb + qggc - qggd),
                    qgd1 + 0.5 * (qgga - qggc + qggb - qggd),
                    0.5 * (cgsna + cgsnc),
                    0.5 * (cgdna + cgdnb),
                )
            } else {
                // PScharge outside transient: the total charge in both slots.
                let (q, capgs, capgd) = self.model.qgg(&op, vgs, vgd);
                (q, q, capgs, capgd)
            };
            let qds = capds * vds;
            if [qgs, qgd, capgs, capgd, qds].iter().any(|v| !v.is_finite()) {
                return Err(self.numerical("nonfinite Statz gate charge"));
            }
            if let Some(coefficients) = integration {
                let AnalysisMode::Transient { dt, .. } = context.mode else {
                    return Err(SpiceError::circuit(
                        "JFET2 charge companion outside transient",
                    ));
                };
                if dt != coefficients.dt() {
                    return Err(SpiceError::circuit("JFET2 charge timestep mismatch"));
                }
                if initial_transient {
                    context.states.patch_state1(slot::QGS, qgs)?;
                    context.states.patch_state1(slot::QGD, qgd)?;
                    context.states.patch_state1(slot::QDS, qds)?;
                }
                let point = |charge, capacitance| JunctionPoint {
                    current: 0.,
                    conductance: 0.,
                    charge,
                    capacitance,
                };
                let (geq, cqgs) =
                    junction_companion(context, coefficients, point(qgs, capgs), slot::QGS)?;
                l.ggs += geq;
                l.cg += cqgs;
                let (geq, cqgd) =
                    junction_companion(context, coefficients, point(qgd, capgd), slot::QGD)?;
                l.ggd += geq;
                l.cg += cqgd;
                l.cd -= cqgd;
                l.cgd += cqgd;
                // jfet2load.c integrates CDS * vds into the drain current
                // but leaves its conductance out of the matrix.
                let (_, cqds) =
                    junction_companion(context, coefficients, point(qds, capds), slot::QDS)?;
                l.cd += cqds;
                if initial_transient {
                    context.states.patch_state1(slot::QGS + 1, cqgs)?;
                    context.states.patch_state1(slot::QGD + 1, cqgd)?;
                    context.states.patch_state1(slot::QDS + 1, cqds)?;
                }
            } else {
                for s in [slot::QGS + 1, slot::QGD + 1, slot::QDS + 1] {
                    carry(&mut context.states, s)?;
                }
            }
            context.states.set(slot::QGS, qgs)?;
            context.states.set(slot::QGD, qgd)?;
            context.states.set(slot::QDS, qds)?;
        } else {
            for s in [
                slot::QGS,
                slot::QGS + 1,
                slot::QGD,
                slot::QGD + 1,
                slot::QDS,
                slot::QDS + 1,
            ] {
                carry(&mut context.states, s)?;
            }
        }
        carry(&mut context.states, slot::UNKNOWN)?;
        for (s, value) in [
            (slot::VGSTRAP, point.vgstrap),
            (slot::VGDTRAP, point.vgdtrap),
            (slot::PAVE, point.pave),
        ] {
            context.states.set(s, value)?;
        }
        jfet::finish_load(
            context,
            start,
            &l,
            &self.terminals,
            self.inner,
            pol,
            self.instance.m,
            jfet::series_conductances(self.instance.area, self.model.rd, self.model.rs),
        )
    }
    /// `jfet2acld.c` after C's `MODEINITSMSIG` load: `PSacload`'s
    /// frequency-dependent transconductance and output conductance at the
    /// stored `gm`, `gds` and `cd`, `CDS` added to the imaginary output
    /// conductance, then the conductances and gate capacitances as for
    /// level 1, every stamp scaled by `m` in C's order and grouping.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let op = self.operating(context.model_context)?;
        let unknowns = context.unknowns;
        let voltage = |n: NodeId| unknowns.node_row(n).and_then(|r| bias.get(r)).unwrap_or(0.);
        let [vgs, vgd] = self.normalized(self.inner.map(voltage));
        let point = self.point(&op, vgs, vgd, context.model_context.gmin, None)?;
        let (_, capgs, capgd) = self.model.qgg(&op, vgs, vgd);
        let omega = 2.0 * std::f64::consts::PI * context.model_context.frequency;
        // C passes the stored `JFET2cd` (channel minus gate-drain diode).
        let (cdrain, gm, gds) = point.channel;
        let cd = cdrain - point.gd.0;
        let (gm, xgm, gds, xgds) = self.model.acload(&op, vgs, vgd, cd, omega, gm, gds);
        if [capgs, capgd, gm, xgm, gds, xgds]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(self.numerical("nonfinite small-signal load"));
        }
        let m = self.instance.m;
        let [dp, g, sp] = self.inner;
        if self.small_signal_depends_on_frequency() {
            // xgds += qds * CKTomega, with MODEINITSMSIG's qds = CDS * area.
            let xgds = xgds + op.capds * omega;
            for (row, col, terms, scale) in [
                (dp, dp, &[xgds][..], m),
                (sp, sp, &[xgds, xgm][..], m),
                (dp, g, &[xgm][..], m),
                (dp, sp, &[xgds, xgm][..], -m),
                (sp, g, &[xgm][..], -m),
                (sp, dp, &[xgds][..], -m),
            ] {
                context.prescaled_dynamic_sum(row, col, terms, scale)?;
            }
        } else {
            // Without TAUG/TAUD, PSacload's imaginary parts are signed zeros,
            // so `xgds + qds * CKTomega` is `CDS * area * omega` exactly.
            for (row, col, scale) in [(dp, dp, m), (sp, sp, m), (dp, sp, -m), (sp, dp, -m)] {
                context.scaled_dynamic_sum(row, col, &[op.capds], scale)?;
            }
        }
        jfet::ac_gate_capacitances(context, self.inner, m, capgs, capgd)?;
        jfet::ac_conductances(
            context,
            &self.terminals,
            self.inner,
            m,
            jfet::series_conductances(self.instance.area, self.model.rd, self.model.rs),
            [gm, gds, point.gs.1, point.gd.1],
        )
    }
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(SpiceError::not_yet_ported(
            format!("noise analysis of JFET2 {}", self.name),
            "src/spicelib/devices/jfet2/jfet2noi.c",
        ))
    }
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(SpiceError::not_yet_ported(
            format!("distortion analysis of JFET2 {}", self.name),
            "src/spicelib/devices/jfet2/ (no DEVdisto: C omits the device from .disto)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model {
            pol: 1.,
            acgam: 0.1,
            beta: 1.3e-3,
            cds: 0.5e-12,
            cgd: 1e-12,
            cgs: 2e-12,
            delta: 0.3,
            hfeta: 0.02,
            hfe1: 0.01,
            hfe2: 0.015,
            hfg1: 0.012,
            hfg2: 0.008,
            hfgam: 0.04,
            mvst: 0.2,
            mxi: 0.1,
            fc: 0.5,
            ibd: 1e-9,
            is: 1e-14,
            lambda: 0.03,
            lfgam: 0.05,
            lfg1: 0.02,
            lfg2: 0.01,
            n: 1.2,
            p: 2.3,
            pb: 0.9,
            q: 2.1,
            rd: 0.,
            rs: 0.,
            taud: 1e-7,
            taug: 2e-7,
            vbd: 2.,
            vst: 0.06,
            vto: -2.,
            xc: 0.2,
            xi: 8.,
            z: 0.6,
            tnom: None,
        }
    }

    fn device(model: Model) -> (Jfet2, Operating) {
        let device = Jfet2 {
            name: "j1".into(),
            terminals: vec![],
            inner: [NodeId::GROUND; 3],
            model,
            instance: Instance {
                area: 1.5,
                m: 1.,
                temp: None,
                dtemp: 0.,
            },
            initial: crate::devices::initial::InstanceInitial::default(),
        };
        let op = device.operating(&ModelContext::default()).unwrap();
        (device, op)
    }

    /// gm = dI/dvgs at fixed vds and gds = dI/dvds at fixed vgs, as well as
    /// the diode conductances, in every region of both modes, with and
    /// without the transient filters.
    #[test]
    fn ids_partials_are_the_current_derivatives() {
        let (device, op) = device(model());
        let memory = Memory {
            step: 3e-8,
            vgstrap: -0.7,
            vgdtrap: -2.4,
            powr: 4e-3,
        };
        for memory in [None, Some(&memory)] {
            for vgs in [-3.2, -2.3, -1.9, -1., 0., 0.4] {
                for vds in [-3., -1.2, -0.4, -0.05, 0.05, 0.4, 1.2, 3.] {
                    let at = |vgs: Real, vds: Real| {
                        device.point(&op, vgs, vgs - vds, 1e-12, memory).unwrap()
                    };
                    let c = at(vgs, vds);
                    let h = 1e-6;
                    let gm = (at(vgs + h, vds).channel.0 - at(vgs - h, vds).channel.0) / (2. * h);
                    let gds = (at(vgs, vds + h).channel.0 - at(vgs, vds - h).channel.0) / (2. * h);
                    let scale = c.channel.1.abs().max(c.channel.2.abs()).max(1e-9);
                    let tag = format!("{vgs} {vds} {}", memory.is_some());
                    assert!(
                        (gm - c.channel.1).abs() <= 1e-5 * scale,
                        "{tag}: gm {gm} {:?}",
                        c.channel
                    );
                    assert!(
                        (gds - c.channel.2).abs() <= 1e-5 * scale,
                        "{tag}: gds {gds} {:?}",
                        c.channel
                    );
                    // d igs / d vgs at fixed vgd = ggs.
                    let ggs = (device
                        .point(&op, vgs + h, vgs - vds, 1e-12, memory)
                        .unwrap()
                        .gs
                        .0
                        - device
                            .point(&op, vgs - h, vgs - vds, 1e-12, memory)
                            .unwrap()
                            .gs
                            .0)
                        / (2. * h);
                    assert!(
                        (ggs - c.gs.1).abs() <= 1e-5 * c.gs.1.abs() + 1e-15,
                        "{tag}: ggs"
                    );
                }
            }
        }
    }

    #[test]
    fn the_channel_is_antisymmetric_without_filters() {
        let (device, op) = device(model());
        for (vgs, vgd) in [(-1., -1.5), (0.2, -2.), (-1.9, -1.95)] {
            let forward = device.point(&op, vgs, vgd, 0., None).unwrap().channel.0;
            let reverse = device.point(&op, vgd, vgs, 0., None).unwrap().channel.0;
            assert!((forward + reverse).abs() <= 1e-15, "{vgs} {vgd}");
        }
    }

    #[test]
    fn statz_charge_partials_are_its_derivatives() {
        let (device, op) = device(model());
        for vgs in [-4., -2.1, -1., 0., 0.3, 0.6, 0.85] {
            for vgd in [-5., -2., -0.5, 0.2, 0.7] {
                let (_, cgs, cgd) = device.model.qgg(&op, vgs, vgd);
                let h = 1e-7;
                let q = |a: Real, b: Real| device.model.qgg(&op, a, b).0;
                let ngs = (q(vgs + h, vgd) - q(vgs - h, vgd)) / (2. * h);
                let ngd = (q(vgs, vgd + h) - q(vgs, vgd - h)) / (2. * h);
                let scale = cgs.abs().max(cgd.abs());
                assert!(
                    (ngs - cgs).abs() <= 1e-6 * scale,
                    "{vgs} {vgd}: cgs {ngs} {cgs}"
                );
                assert!(
                    (ngd - cgd).abs() <= 1e-6 * scale,
                    "{vgs} {vgd}: cgd {ngd} {cgd}"
                );
            }
        }
    }

    #[test]
    fn ac_load_reduces_to_the_dc_conductances_without_time_constants() {
        let mut m = model();
        m.taug = 0.;
        m.taud = 0.;
        let (device, op) = device(m);
        let p = device.point(&op, -0.8, -3., 1e-12, None).unwrap();
        let (gm, xgm, gds, xgds) = device.model.acload(
            &op,
            -0.8,
            -3.,
            p.channel.0 - p.gd.0,
            1e7,
            p.channel.1,
            p.channel.2,
        );
        assert!(
            (gm - p.channel.1).abs() <= 1e-12 * gm.abs(),
            "{gm} {:?}",
            p.channel
        );
        assert!(
            (gds - p.channel.2).abs() <= 1e-12 * gds.abs(),
            "{gds} {:?}",
            p.channel
        );
        assert_eq!((xgm, xgds), (0., 0.));
    }
}
