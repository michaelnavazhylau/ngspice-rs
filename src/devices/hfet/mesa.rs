//! MESA, `Z` level 2 (`mesa1`).
//!
//! C references, read as behaviour only: `mesa/mesasetup.c` (`MESAsetup`: the
//! internal nodes and the instance defaults), `mesa/mesatemp.c`
//! (`MESAtemp`: every per-instance derived quantity), `mesa/mesaload.c`
//! (`MESAload` and the `mesa1` channel), `mesa/mesaparam.c` and
//! `mesa/mesaask.c`.
//!
//! This is H1a: `.op` and `.dc` only. The charge storage and `NIintegrate`
//! path (`mesatrunc.c`), the AC load (`mesaacl.c`), the pole-zero load
//! (`mesapzl.c`) and the `ri`/`rf`/`rdi`/`rsi` (prime-prime and `#gs`/`#gd`)
//! branches are H1b and fail with explicit errors.
//!
//! # Deliberate divergences
//!
//! - C's bypass test and `MODEINITPRED` extrapolation are omitted, as for the
//!   JFET and MESFET.
//! - Where `mesa1` overflows or takes the root of a negative number the port
//!   reports a numerical error instead of continuing with NaN.
//! - `ri`, `rf`, `rdi` and `rsi` are refused rather than aliased: C creates
//!   the `#gs`/`#gd` and prime-prime nodes for them, and their conductances
//!   (`tGi`, `tGf`) enter the load. Refusing keeps the stamps that remain
//!   identical to C's, which zero those terms.

use crate::devices::hfet::{Instance, Model};
use crate::devices::initial::InstanceInitial;
use crate::devices::jfet::{BOLTZMANN, CHARGE};
use crate::devices::limiting;
use crate::devices::{
    Device, DeviceState, IterationPhase, LinearContext, ModelContext, ResolvedModel, StampContext,
    sweep::check_swept,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// C `EPSILONGAAS` (`mesaload.c`).
const EPSILON_GAAS: Real = 12.244 * 8.854_187_817e-12;
/// C `CONSTKoverQ`: the thermal voltage per kelvin.
const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
const CELSIUS_TO_KELVIN: Real = 273.15;
/// `mesaload.c`'s `ns` cutoff, below which the channel is treated as off.
const N_MIN: Real = 1e-38;

/// `MESApTable`'s settable reals (`dctrcurv.c` `DCTsetInstParam`).
const SWEEPABLE: [&str; 8] = ["l", "w", "m", "icvds", "icvgs", "td", "ts", "dtemp"];

/// The state slots of `MESAload`'s `CKTstate0`, in the port's own order (C's
/// `MESAnumStates` has a different one, which nothing here needs).
mod slot {
    pub(super) const VGS: usize = 0;
    pub(super) const VGSPP: usize = 1;
    pub(super) const VGD: usize = 2;
    pub(super) const VGDPP: usize = 3;
    pub(super) const CG: usize = 4;
    pub(super) const CD: usize = 5;
    pub(super) const CGD: usize = 6;
    pub(super) const CGS: usize = 7;
    pub(super) const GM: usize = 8;
    pub(super) const GDS: usize = 9;
    pub(super) const GGS: usize = 10;
    pub(super) const GGD: usize = 11;
    pub(super) const GGSPP: usize = 12;
    pub(super) const CGSPP: usize = 13;
    pub(super) const GGDPP: usize = 14;
    pub(super) const CGDPP: usize = 15;
    pub(super) const COUNT: usize = 16;
}

/// `mesatemp.c`'s per-instance quantities.
#[derive(Debug, Clone, Copy)]
struct Derived {
    /// `vt` at `ts` and `td`, and `MESAn` times them.
    vts: Real,
    vtd: Real,
    vtes: Real,
    vted: Real,
    tmu: Real,
    tphib: Real,
    tvto: Real,
    gchi0: Real,
    /// `here->MESAbeta = 2 eps vs zeta w / d` (level 2).
    beta: Real,
    teta: Real,
    tlambda: Real,
    /// `tLambdahf` and `mesaacl.c`'s `fl`/`delf` blend window.
    tlambdahf: Real,
    fl: Real,
    delf: Real,
    n0: Real,
    nsb0: Real,
    isatb0: Real,
    cf: Real,
    csatfs: Real,
    csatfd: Real,
    ggrwl: Real,
    vcrits: Real,
    vcritd: Real,
    gdpr: Real,
    gspr: Real,
    ggate: Real,
    tgi: Real,
    tgf: Real,
    /// `model->MESAvpo` (level 2) and `delta²`.
    vpo: Real,
    delta_sqr: Real,
}

/// `mesaacl.c`'s conductances and capacitances at the bias.
#[derive(Debug, Clone, Copy)]
struct AcValues {
    gm: Real,
    gds: Real,
    capgs: Real,
    capgd: Real,
    ggs: Real,
    ggd: Real,
    ggspp: Real,
    ggdpp: Real,
}

/// One load's junction, channel and capacitance values (`mesaload.c`).
#[derive(Debug, Clone, Copy)]
struct Point {
    vgs: Real,
    vgd: Real,
    vds: Real,
    cgs: Real,
    ggs: Real,
    cgd: Real,
    ggd: Real,
    cd: Real,
    cg: Real,
    gm: Real,
    gds: Real,
    /// The gate capacitances `mesaacl.c` reports as `qgs`/`qgd` in a
    /// small-signal load.
    capgs: Real,
    capgd: Real,
}

/// `mesaload.c`'s start states, and the linear prediction C compares the
/// converged currents against.
#[derive(Debug, Clone, Copy)]
enum Start {
    /// The `uic` initial load.
    Conditions,
    /// A load with limiting inactive: the solution itself.
    Exact,
    /// `MODEINITJCT`/`MODEINITFIX`: always nonconvergent.
    Fixed,
    /// A held `off` load (the shared rule of the port's other nonlinear
    /// devices).
    HeldOff { raw: [Real; 2] },
    /// C's `DEVpnjlim`/`DEVfetlim` limited the step.
    Limited {
        icheck: bool,
        cghat: Real,
        cdhat: Real,
    },
}

/// `mesasetup.c`'s internal nodes and the intrinsic terminals they lead to.
#[derive(Debug, Clone, Copy)]
struct Nodes {
    /// External drain, gate and source.
    d: NodeId,
    g: NodeId,
    s: NodeId,
    /// `#source`, `#drain` and `#gate`, or the external node when its
    /// resistance is zero.
    sp: NodeId,
    dp: NodeId,
    gp: NodeId,
    /// The prime-prime nodes (H1b; the prime node while `rdi`/`rsi` are zero).
    spp: NodeId,
    dpp: NodeId,
}

/// MESA level 2.
#[derive(Debug, Clone)]
pub(crate) struct Mesa {
    name: String,
    model_name: String,
    /// External terminals then the internal nodes, in `mesasetup.c`'s order.
    terminals: Vec<NodeId>,
    nodes: Nodes,
    model: Model,
    instance: Instance,
    initial: InstanceInitial,
}

impl Mesa {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 2 {
            return Err(SpiceError::circuit(format!(
                "{}: MESA needs level 2",
                i.name
            )));
        }
        if i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: MESA needs three terminals",
                i.name
            )));
        }
        let model = Model::resolve(resolved)?;
        let (initial, setters) = Instance::resolve(i)?;
        let instance = Instance::validate(&setters, &i.location)?;
        // `mesasetup.c` creates `#gs` for RI, `#gd` for RF and the prime-prime
        // pair for RSI/RDI; their conductances enter the load, so H1a refuses
        // them rather than dropping terms.
        for (value, setter, reference) in [
            (model.ri, "ri", "mesasetup.c, mesaload.c (MESAtGi)"),
            (model.rf, "rf", "mesasetup.c, mesaload.c (MESAtGf)"),
            (
                model.rdi,
                "rdi",
                "mesasetup.c, mesaload.c (MESAdrainPrmPrmNode)",
            ),
            (
                model.rsi,
                "rsi",
                "mesasetup.c, mesaload.c (MESAsourcePrmPrmNode)",
            ),
        ] {
            if value != 0. {
                return Err(SpiceError::not_yet_ported(
                    format!(
                        "{}: MESA model '{}' {setter}={value}",
                        i.location,
                        resolved.card().name
                    ),
                    reference,
                ));
            }
        }
        let mut staged = nodes.clone();
        let external: Vec<NodeId> = i.nodes.iter().map(|n| staged.intern(n)).collect();
        let mut terminals = external.clone();
        let mut nodes_of = Nodes {
            d: external[0],
            g: external[1],
            s: external[2],
            sp: external[2],
            dp: external[0],
            gp: external[1],
            spp: external[2],
            dpp: external[0],
        };
        // `MESAsetup` creates `#source` for RS, then `#drain` for RD, then
        // `#gate` for RG; each falls back to the external node.
        for (resistance, suffix) in [
            (model.rs, "source"),
            (model.rd, "drain"),
            (model.rg, "gate"),
        ] {
            if resistance == 0. {
                continue;
            }
            let name = format!("{}#{suffix}", i.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "{}: MESA internal-node name collision: {name}",
                    i.name
                )));
            }
            let internal = staged.intern(&name);
            staged.set_kind(internal, NodeKind::Internal);
            terminals.push(internal);
            match suffix {
                "source" => nodes_of.sp = internal,
                "drain" => nodes_of.dp = internal,
                _ => nodes_of.gp = internal,
            }
        }
        // `rsi`/`rdi` are refused above, so the prime-prime nodes are the
        // prime nodes and `vgspp == vgs`, as in C.
        nodes_of.spp = nodes_of.sp;
        nodes_of.dpp = nodes_of.dp;
        *nodes = staged;
        let device = Self {
            name: i.name.clone(),
            model_name: resolved.card().name.clone(),
            terminals,
            nodes: nodes_of,
            model,
            instance,
            initial,
        };
        // Validate the derivations before the device joins the circuit.
        device.derived(context.temperature, context.nominal_temperature)?;
        Ok(Box::new(device))
    }

    /// `mesatemp.c`. `temperature` and `nominal` are Celsius
    /// (`CKTtemp`/`CKTnomTemp`), while the instance temperatures are kelvin
    /// after `mesaparam.c`'s conversion.
    fn derived(&self, temperature: Real, nominal: Real) -> SpiceResult<Derived> {
        let model = &self.model;
        let instance = &self.instance;
        // C's `CKTtemp`/`CKTnomTemp` are kelvin; the port's `ModelContext`
        // carries Celsius. `mesaparam.c` stores an explicit TD/TS as
        // `value + CONSTCtoK`, so it is Celsius in the deck, and
        // `mesasetup.c`'s default `CKTtemp + DTEMP` is kelvin.
        let nominal_kelvin = nominal + CELSIUS_TO_KELVIN;
        let ts = match instance.ts {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + instance.dtemp,
        };
        let td = match instance.td {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + instance.dtemp,
        };
        let scalar = |resistance: Real, kelvin: Real| {
            if resistance == 0. {
                0.
            } else {
                let delta = kelvin - nominal_kelvin;
                resistance * (1. + model.tc1 * delta + model.tc2 * delta * delta)
            }
        };
        let tmu = if model.mu1 == 0. && model.mu2 == 0. {
            model.mu * (ts / model.tmu).powf(model.xtm0)
        } else {
            let muimp = model.mu * (ts / model.tmu).powf(model.xtm0);
            let mupo = model.mu1 * (model.tmu / ts).powf(model.xtm1)
                + model.mu2 * (model.tmu / ts).powf(model.xtm2);
            1. / (1. / muimp + 1. / mupo)
        };
        let teta = model.eta * (1. + ts / model.teta0) + model.teta1 / ts;
        let tlambda = model.lambda * (1. - ts / model.tlambda);
        let tlambdahf = model.lambdahf.unwrap_or(model.lambda) * (1. - ts / model.tlambda);
        // `mesasetup.c` defaults TF to `CKTtemp` (kelvin).
        let exp_tf = (ts / model.tf.unwrap_or(temperature + CELSIUS_TO_KELVIN)).exp();
        let fl = model.flo * exp_tf;
        let delf = model.delfo * exp_tf;
        let vts = K_OVER_Q * ts;
        let vtd = K_OVER_Q * td;
        let tphib = model.phib - model.phib1 * (ts - nominal_kelvin);
        let tvto = model.threshold - model.tvto * (ts - nominal_kelvin);
        let vpo = CHARGE * model.nd * model.d * model.d / 2. / EPSILON_GAAS;
        let gchi0 = CHARGE * instance.w / instance.l;
        let beta = 2. * EPSILON_GAAS * model.vs * model.zeta * instance.w / model.d;
        let n0 = EPSILON_GAAS * teta * vts / CHARGE / model.d;
        let nsb0 = EPSILON_GAAS * teta * vts / CHARGE / (model.du + model.th);
        let isatb0 = CHARGE * n0 * vts * instance.w / instance.l;
        let cf = 0.5 * EPSILON_GAAS * instance.w;
        let csatfs = 0.5
            * model.astar
            * ts
            * ts
            * (-tphib / (BOLTZMANN * ts)).exp()
            * instance.l
            * instance.w;
        let csatfd = 0.5
            * model.astar
            * td
            * td
            * (-tphib / (BOLTZMANN * td)).exp()
            * instance.l
            * instance.w;
        let ggrwl =
            model.ggr * instance.l * instance.w * (model.xchi * (ts - nominal_kelvin)).exp();
        let vcrit = |vt: Real, csat: Real| {
            if csat == 0. {
                Real::MAX
            } else {
                vt * (vt / (std::f64::consts::SQRT_2 * csat)).ln()
            }
        };
        let trd = scalar(model.rd, td);
        let trs = scalar(model.rs, ts);
        let trg = scalar(model.rg, ts);
        let conductance = |resistance: Real| {
            if resistance == 0. {
                0.
            } else {
                1. / resistance
            }
        };
        let derived = Derived {
            vts,
            vtd,
            vtes: model.n * vts,
            vted: model.n * vtd,
            tmu,
            tphib,
            tvto,
            gchi0,
            beta,
            teta,
            tlambda,
            tlambdahf,
            fl,
            delf,
            n0,
            nsb0,
            isatb0,
            cf,
            csatfs,
            csatfd,
            ggrwl,
            vcrits: vcrit(vts, csatfs),
            vcritd: vcrit(vtd, csatfd),
            gdpr: conductance(trd),
            gspr: conductance(trs),
            ggate: conductance(trg),
            tgi: 0.,
            tgf: 0.,
            vpo,
            delta_sqr: model.delta * model.delta,
        };
        if [
            derived.vts,
            derived.vtd,
            derived.vtes,
            derived.vted,
            derived.tmu,
            derived.tphib,
            derived.tvto,
            derived.gchi0,
            derived.beta,
            derived.teta,
            derived.tlambda,
            derived.n0,
            derived.nsb0,
            derived.isatb0,
            derived.cf,
            derived.csatfs,
            derived.csatfd,
            derived.ggrwl,
            derived.vpo,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || derived.vtes == 0.
            || derived.vted == 0.
        {
            return Err(SpiceError::circuit(format!(
                "{}: MESA temperature derivation is out of range (check N, TF, TETA0/TETA1, \
                 TMU and the instance temperatures)",
                self.name
            )));
        }
        Ok(derived)
    }

    /// `mesaload.c`'s initialization and limiting, per junction: `MESA` uses
    /// `vtes`/`vcrits` for the gate-source and `vted`/`vcritd` for the
    /// gate-drain junction, where the JFET shared code uses one pair.
    fn select_junctions(
        &self,
        derived: &Derived,
        states: &DeviceState<'_>,
        raw: [Real; 2],
        ic: [Real; 2],
    ) -> ([Real; 2], Start) {
        if states.initial_conditions() {
            let vds = ic[0];
            let vgs = ic[1];
            return ([vgs, vgs - vds], Start::Conditions);
        }
        if !states.device_limiting() {
            return (raw, Start::Exact);
        }
        let new = match states.phase() {
            IterationPhase::Junction => {
                let v = if self.initial.off { 0. } else { -1. };
                return ([v, v], Start::Fixed);
            }
            IterationPhase::Fix if self.initial.off => return ([0.; 2], Start::HeldOff { raw }),
            IterationPhase::Predict => [
                states.predicted(slot::VGS, 1.),
                states.predicted(slot::VGD, 1.),
            ],
            IterationPhase::Fix | IterationPhase::Float => raw.map(Some),
        };
        let old = |index: usize| match states.phase() {
            IterationPhase::Predict => states.accepted(1, index),
            _ => states.iterate(index),
        };
        let [Some(vgs), Some(vgd)] = new else {
            return (raw, Start::Exact);
        };
        let previous: Option<Vec<Real>> = [
            slot::VGS,
            slot::VGD,
            slot::CG,
            slot::CD,
            slot::GM,
            slot::GDS,
            slot::GGS,
            slot::GGD,
        ]
        .into_iter()
        .map(|index| old(index).filter(|v| v.is_finite()))
        .collect();
        let Some(&[vgs0, vgd0, cg0, cd0, gm0, gds0, ggs0, ggd0]) = previous.as_deref() else {
            return ([vgs, vgd], Start::Exact);
        };
        let delvgs = vgs - vgs0;
        let delvgd = vgd - vgd0;
        let delvds = delvgs - delvgd;
        let cghat = cg0 + ggd0 * delvgd + ggs0 * delvgs;
        let cdhat = cd0 + gm0 * delvgs + gds0 * delvds - ggd0 * delvgd;
        let gs = limiting::pnjlim(vgs, vgs0, derived.vtes, derived.vcrits);
        let gd = limiting::pnjlim(vgd, vgd0, derived.vted, derived.vcritd);
        let vgs = limiting::fetlim(gs.value, vgs0, derived.tvto);
        let vgd = limiting::fetlim(gd.value, vgd0, derived.tvto);
        (
            [vgs, vgd],
            Start::Limited {
                icheck: gs.limited || gd.limited,
                cghat,
                cdhat,
            },
        )
    }

    /// The gate junction of `mesaload.c`: a Schottky diode
    /// `csat (exp(v/vt) - 1)` plus the reverse term
    /// `ggrwl e^{-v del/vt} (1 + v del/vt)` and `gmin`.
    fn junction(
        &self,
        derived: &Derived,
        v: Real,
        vt: Real,
        vte: Real,
        csat: Real,
        gmin: Real,
    ) -> (Real, Real) {
        let arg = -v * self.model.del / vt;
        let earg = arg.exp();
        let ev = (v / vte).exp();
        let conductance = csat * ev / vte + derived.ggrwl * earg * (1. - arg) + gmin;
        let current = csat * (ev - 1.) + derived.ggrwl * v * earg + gmin * v;
        (current, conductance)
    }

    /// `mesa1` (`mesaload.c:499`): the drain current and its derivatives. The
    /// expressions and their order are C's.
    fn mesa1(
        &self,
        derived: &Derived,
        lambda: Real,
        vgs: Real,
        vds: Real,
        von: Real,
    ) -> SpiceResult<(Real, Real, Real, Real, Real)> {
        let model = &self.model;
        let eta = derived.teta;
        let vt = derived.vts;
        let etavth = eta * vt;
        let rt = 0.; // RS + RD, both refused at setup
        let vgt0 = vgs - von;
        let s = ((vgt0 - model.vsigmat) / model.vsigma).exp();
        let sigma = model.sigma0 / (1. + s);
        let vgt = vgt0 + sigma * vds;
        let mu = derived.tmu + model.theta * vgt;
        let vl = model.vs / mu * self.instance.l;
        let beta = derived.beta / (derived.vpo + 3. * vl);
        let u = vgt / vt - 1.;
        let t = (derived.delta_sqr + u * u).sqrt();
        let vgte = 0.5 * vt * (2. + u + t);
        let a = 2. * beta * vgte;
        let b = (-vgt / etavth).exp();
        let sqrt1 = if vgte > derived.vpo {
            0.
        } else {
            (1. - vgte / derived.vpo).sqrt()
        };
        let denom = 1. / model.nd / model.d * (1. - sqrt1);
        let ns = 1. / (denom + 1. / derived.n0 * b);
        if ns < N_MIN {
            return Ok((0., 0., 0., derived.cf, derived.cf));
        }
        let gchi = derived.gchi0 * mu * ns;
        let gch = gchi / (1. + gchi * rt);
        let f = (1. + 2. * a * 0.).sqrt(); // sqrt(1 + 2 a RS), RS = 0
        let d = 1. + a * 0. + f;
        let e = 1. + model.tc * vgte;
        let isata = a * vgte / (d * e);
        let isatb = derived.isatb0 * mu * (vgt / etavth).exp();
        let isat = isata * isatb / (isata + isatb);
        let vsate = isat / gch;
        let vdse = vds * (1. + (vds / vsate).powf(model.mc)).powf(-1. / model.mc);
        let m = model.m + model.alpha * vgte;
        let g = (vds / vsate).powf(m);
        let h = (1. + g).powf(1. / m);
        let delidgch0 = vds / h;
        let delidgch = delidgch0 * (1. + lambda * vds);
        let cdrain = gch * delidgch;
        let temp = if vgt > derived.vpo {
            0.
        } else {
            (1. - vgt / derived.vpo).sqrt()
        };
        let cgc = self.instance.w * self.instance.l * EPSILON_GAAS / (temp + b) / model.d;
        let c = (vsate - vdse) / (2. * vsate - vdse);
        let c = c * c;
        let capgs = derived.cf + 2. / 3. * cgc * (1. - c);
        let c = vsate / (2. * vsate - vdse);
        let c = c * c;
        let capgd = derived.cf + 2. / 3. * cgc * (1. - c);
        let temp = 1. + 0. * rt; // 1 + gchi RS
        let delgchgchi = 1. / (temp * temp);
        let delgchins = derived.gchi0 * mu;
        let delnsvgt = ns * ns * 1. / derived.n0 / etavth * b;
        let q = 1. - sqrt1;
        let delnsvgte = if sqrt1 == 0. {
            0.
        } else {
            0.5 * ns * ns / (derived.vpo * model.nd * model.d * sqrt1 * q * q)
        };
        let delvgtevgt = 0.5 * (1. + u / t);
        let delidvds0 = gch / h;
        let delidvds1 = if vds != 0. {
            cdrain * (vds / vsate).powf(m - 1.) / (vsate * (1. + g))
        } else {
            0.
        };
        let delidvds = delidvds0 * (1. + 2. * lambda * vds) - delidvds1;
        let delidvsate = cdrain * g / (vsate * (1. + g));
        let delvsateisat = 1. / gch;
        let r = isata + isatb;
        let r = r * r;
        let delisatisata = isatb * isatb / r;
        let v = 1. + 1. / f;
        let ddevgte = 2. * beta * 0. * v * e + d * model.tc; // 2 beta RS v e + d TC, RS = 0
        let temp = d * d * e * e;
        let delisatavgte = (2. * a * d * e - a * vgte * ddevgte) / temp;
        let delisatabeta = 2. * vgte * vgte * (d * e - a * e * 0. * v) / temp;
        let delisatisatb = isata * isata / r;
        let delvsategch = -vsate / gch;
        let dvgtvgs = 1. - model.sigma0 * vds * s / model.vsigma / ((1. + s) * (1. + s));
        let temp = derived.gchi0 * ns * model.theta;
        let dgchivgt = delgchins * (delnsvgte * delvgtevgt + delnsvgt) + temp;
        let dvgtevds = delvgtevgt * sigma;
        let dgchivds = delgchins * (delnsvgte * dvgtevds + delnsvgt * sigma) + temp * sigma;
        let temp2 = delisatabeta * 3. * beta * vl * model.theta / (mu * (derived.vpo + 3. * vl));
        let disatavgt = delisatavgte * delvgtevgt + temp2;
        let disatavds = delisatavgte * dvgtevds + temp2 * sigma;
        let disatbvgt = isatb / etavth + isatb / mu * model.theta;
        let p = delgchgchi * dgchivgt;
        let w = delgchgchi * dgchivds;
        let dvsatevgt =
            delvsateisat * (delisatisata * disatavgt + delisatisatb * disatbvgt) + delvsategch * p;
        let dvsatevds = delvsateisat
            * (delisatisata * disatavds + delisatisatb * disatbvgt * sigma)
            + delvsategch * w;
        let (gmmadd, gdsmadd) = if model.alpha != 0. {
            let gmmadd = if vds == 0. {
                0.
            } else {
                cdrain
                    * ((1. + g).ln() / (m * m) - g * (vds / vsate).ln() / (m * (1. + g)))
                    * model.alpha
                    * delvgtevgt
            };
            (gmmadd, gmmadd * sigma)
        } else {
            (0., 0.)
        };
        let g = delidgch * p + delidvsate * dvsatevgt;
        let gm = (g + gmmadd) * dvgtvgs;
        let gds = delidvds + delidvsate * dvsatevds + delidgch * w + gdsmadd;
        let values = [cdrain, gm, gds, capgs, capgd];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(self.numerical("nonfinite mesa1 channel equations"));
        }
        Ok((cdrain, gm, gds, capgs, capgd))
    }

    fn numerical(&self, message: &str) -> SpiceError {
        SpiceError::Numerical {
            context: format!("MESA {}", self.name),
            message: message.into(),
        }
    }

    /// `mesaload.c`'s DC evaluation at `vgs`/`vgd`.
    fn point(
        &self,
        derived: &Derived,
        vgs: Real,
        vgd: Real,
        von: Real,
        gmin: Real,
    ) -> SpiceResult<Point> {
        let (cgs, ggs) = self.junction(
            derived,
            vgs,
            derived.vts,
            derived.vtes,
            derived.csatfs,
            gmin,
        );
        let (cgd, ggd) = self.junction(
            derived,
            vgd,
            derived.vtd,
            derived.vted,
            derived.csatfd,
            gmin,
        );
        let vds = vgs - vgd;
        let (inverse, vds) = if vds < 0. { (true, -vds) } else { (false, vds) };
        let (cdrain, gm, gds, capgs, capgd) = self.mesa1(
            derived,
            derived.tlambda,
            if inverse { vgd } else { vgs },
            vds,
            von,
        )?;
        // `mesaload.c` negates the drain current and swaps the gate
        // capacitances in inverse mode (`mesaacl.c` reads them back from the
        // state as they are stored here).
        let (cdrain, capgs, capgd) = if inverse {
            (-cdrain, capgd, capgs)
        } else {
            (cdrain, capgs, capgd)
        };
        let _ = inverse;
        let values = [cgs, ggs, cgd, ggd, cdrain];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(self.numerical("nonfinite gate junction equations"));
        }
        Ok(Point {
            vgs,
            vgd,
            vds: vgs - vgd,
            cgs,
            ggs,
            cgd,
            ggd,
            cd: cdrain - cgd,
            cg: cgs + cgd,
            gm,
            gds,
            capgs,
            capgd,
        })
    }

    /// The instance temperatures in kelvin, `mesatemp.c`'s rule for asks.
    fn temperatures(&self, temperature: Real) -> (Real, Real) {
        let ts = match self.instance.ts {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + self.instance.dtemp,
        };
        let td = match self.instance.td {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + self.instance.dtemp,
        };
        (ts, td)
    }

    /// `mesaacl.c`: `gm`/`gds` are re-derived at the bias with the
    /// frequency-blended `lambda`, and the gate capacitances come from the
    /// operating point.
    #[allow(clippy::too_many_arguments)]
    fn ac_point(
        &self,
        derived: &Derived,
        vgs: Real,
        vgd: Real,
        von: Real,
        gmin: Real,
        frequency: Real,
    ) -> SpiceResult<AcValues> {
        let lambda = if derived.delf == 0. {
            derived.tlambda
        } else {
            derived.tlambda
                + 0.5
                    * (derived.tlambdahf - derived.tlambda)
                    * (1. + ((frequency - derived.fl) / derived.delf).tanh())
        };
        if !lambda.is_finite() {
            return Err(self.numerical("nonfinite mesaacl.c lambda blend"));
        }
        let point = self.point(derived, vgs, vgd, von, gmin)?;
        let vds = vgs - vgd;
        let (inverse, vds) = if vds < 0. { (true, -vds) } else { (false, vds) };
        let (_, gm, gds, _, _) =
            self.mesa1(derived, lambda, if inverse { vgd } else { vgs }, vds, von)?;
        // `ggspp`/`ggdpp` come from the charge integration, which only a
        // transient load fills; a DC or small-signal load leaves them zero.
        Ok(AcValues {
            gm,
            gds,
            capgs: point.capgs,
            capgd: point.capgd,
            ggs: point.ggs,
            ggd: point.ggd,
            ggspp: 0.,
            ggdpp: 0.,
        })
    }

    /// `MESApTable`'s settable reals (`dctrcurv.c` `DCTsetInstParam`).
    fn sweepable(parameter: &str) -> Option<&'static str> {
        SWEEPABLE
            .into_iter()
            .find(|name| name.eq_ignore_ascii_case(parameter))
    }

    fn is_ac(&self) -> SpiceError {
        SpiceError::not_yet_ported(
            format!("{}: MESA level 2 AC/SP analysis", self.name),
            "src/spicelib/devices/mesa/mesaacl.c (H1b)",
        )
    }
}

impl Device for Mesa {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'z'
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
    fn state_count(&self) -> usize {
        slot::COUNT
    }
    /// `mesaask.c`: `l`, `m`, the given `icvds`/`icvgs`, and the instance
    /// temperatures in Celsius (`td`/`ts` store kelvin) plus `dtemp`.
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        let (ts, td) = self.temperatures(context.temperature);
        Ok(match key {
            "l" => Some(self.instance.l),
            // C scales WIDTH by M and leaves LENGTH alone.
            "w" => Some(self.instance.w * self.instance.m),
            "m" => Some(self.instance.m),
            "icvds" => self.initial.values[0],
            "icvgs" => self.initial.values[1],
            "td" => Some(td - CELSIUS_TO_KELVIN),
            "ts" => Some(ts - CELSIUS_TO_KELVIN),
            "dtemp" => Some(self.instance.dtemp),
            _ => None,
        })
    }
    /// `mesaask.c`'s bias quantities, `m`-scaled as C stores them. `cs` and
    /// the power dissipation need the external terminal voltages.
    fn observation_operating(
        &self,
        key: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        transient: bool,
    ) -> SpiceResult<Option<Real>> {
        let keys = [
            "vgs", "vgd", "cg", "cd", "cgd", "gm", "gds", "ggs", "ggd", "cs", "p",
        ];
        if !keys.contains(&key) {
            return Ok(None);
        }
        if transient {
            return Err(SpiceError::not_yet_ported(
                format!("{}: transient @{}[{key}]", self.name, self.name),
                "src/spicelib/devices/mesa/mesaload.c (charge storage, H1b)",
            ));
        }
        let derived = self.derived(context.temperature, context.nominal_temperature)?;
        let vgs = voltage(self.nodes.gp) - voltage(self.nodes.sp);
        let vgd = voltage(self.nodes.gp) - voltage(self.nodes.dp);
        let von = derived.tvto + self.model.ks * (voltage(self.nodes.sp) - self.model.vsg);
        let point = self.point(&derived, vgs, vgd, von, context.gmin)?;
        let m = self.instance.m;
        Ok(Some(match key {
            "vgs" => point.vgs,
            "vgd" => point.vgd,
            "cg" => m * point.cg,
            "cd" => m * point.cd,
            "cgd" => m * point.cgd,
            "gm" => m * point.gm,
            "gds" => m * point.gds,
            "ggs" => m * point.ggs,
            "ggd" => m * point.ggd,
            "cs" => m * (-point.cd - point.cg),
            _ => {
                let v = |n: NodeId| voltage(n);
                m * (point.cd * v(self.nodes.d) + point.cg * v(self.nodes.g)
                    - (point.cd + point.cg) * v(self.nodes.s))
            }
        }))
    }
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        Self::sweepable(keyword)
    }
    /// `MESAparam` with the instance schema's domains: a swept temperature is
    /// re-derived by the next load.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        _context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let check = |ok: bool, what: &str| check_swept(&self.name, parameter, value, ok, what);
        let mut instance = self.instance;
        let mut initial = self.initial.clone();
        match parameter {
            "l" | "w" | "m" => {
                check(value > 0., "positive")?;
                match parameter {
                    "l" => instance.l = value,
                    "w" => instance.w = value,
                    _ => instance.m = value,
                }
            }
            "td" | "ts" | "dtemp" => {
                check(value.is_finite(), "finite")?;
                match parameter {
                    "td" => instance.td = Some(value),
                    "ts" => instance.ts = Some(value),
                    _ => instance.dtemp = value,
                }
            }
            "icvds" | "icvgs" => {
                check(value.is_finite(), "finite")?;
                initial.values[usize::from(parameter == "icvgs")] = Some(value);
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "{}: MESA parameter {parameter} cannot be swept",
                    self.name
                )));
            }
        }
        Ok(Box::new(Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            terminals: self.terminals.clone(),
            nodes: self.nodes,
            model: self.model,
            instance,
            initial,
        }))
    }
    /// `MESAload`'s DC path.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(self.is_ac());
        }
        if context.mode.is_transient() {
            return Err(SpiceError::not_yet_ported(
                format!("{}: MESA level 2 transient analysis", self.name),
                "src/spicelib/devices/mesa/mesaload.c (charge storage), mesatrunc.c (H1b)",
            ));
        }
        let derived = self.derived(context.temperature, context.nominal_temperature)?;
        let gmin = context.gmin;
        let m = self.instance.m;
        let voltage = |n: NodeId| context.node_voltage(n);
        let raw = [
            voltage(self.nodes.gp) - voltage(self.nodes.sp),
            voltage(self.nodes.gp) - voltage(self.nodes.dp),
        ];
        // `mesgetic.c`: unset initial conditions come from the external nodes.
        let ic = [
            self.initial.values[0].unwrap_or_else(|| voltage(self.nodes.d) - voltage(self.nodes.s)),
            self.initial.values[1].unwrap_or_else(|| voltage(self.nodes.g) - voltage(self.nodes.s)),
        ];
        let ([vgs, vgd], start) = self.select_junctions(&derived, &context.states, raw, ic);
        // `rsi`/`rdi` are zero, so the prime-prime nodes are the prime nodes.
        let (vgspp, vgdpp) = (vgs, vgd);
        let von = derived.tvto + self.model.ks * (voltage(self.nodes.sp) - self.model.vsg);
        let point = self.point(&derived, vgs, vgd, von, gmin)?;
        let nonconvergent = match start {
            Start::Fixed => true,
            Start::Conditions | Start::Exact => false,
            Start::HeldOff { raw } => {
                let delvgs = raw[0] - point.vgs;
                let delvgd = raw[1] - point.vgd;
                let delvds = delvgs - delvgd;
                let cghat = point.cg + point.ggd * delvgd + point.ggs * delvgs;
                let cdhat = point.cd + point.gm * delvgs + point.gds * delvds - point.ggd * delvgd;
                context
                    .states
                    .convergence_tolerances()
                    .is_some_and(|(reltol, abstol)| {
                        (cghat - point.cg).abs() > reltol * cghat.abs().max(point.cg.abs()) + abstol
                            || (cdhat - point.cd).abs()
                                > reltol * cdhat.abs().max(point.cd.abs()) + abstol
                    })
            }
            Start::Limited {
                icheck,
                cghat,
                cdhat,
            } => {
                icheck
                    || context
                        .states
                        .convergence_tolerances()
                        .is_some_and(|(reltol, abstol)| {
                            (cghat - point.cg).abs()
                                >= reltol * cghat.abs().max(point.cg.abs()) + abstol
                                || (cdhat - point.cd).abs()
                                    > reltol * cdhat.abs().max(point.cd.abs()) + abstol
                        })
            }
        };
        if nonconvergent {
            context.states.report_nonconvergence()?;
        }
        // In DC the prime-prime conductances and their RHS terms are zero
        // (`mesaload.c` only fills them from `NIintegrate`).
        let (ggspp, ggdpp, cgspp, cgdpp) = (0., 0., 0., 0.);
        for (index, value) in [
            (slot::VGS, vgs),
            (slot::VGSPP, vgspp),
            (slot::VGD, vgd),
            (slot::VGDPP, vgdpp),
            (slot::CG, point.cg),
            (slot::CD, point.cd),
            (slot::CGD, point.cgd),
            (slot::CGS, point.cgs),
            (slot::GM, point.gm),
            (slot::GDS, point.gds),
            (slot::GGS, point.ggs),
            (slot::GGD, point.ggd),
            (slot::GGSPP, ggspp),
            (slot::CGSPP, cgspp),
            (slot::GGDPP, ggdpp),
            (slot::CGDPP, cgdpp),
        ] {
            context.states.set(index, value)?;
        }
        let ccorr = self.model.ag * (point.cgs - point.cgd);
        let ceqgd = point.cgd + cgdpp - point.ggd * point.vgd - ggdpp * vgdpp;
        let ceqgs = point.cgs + cgspp - point.ggs * point.vgs - ggspp * vgspp;
        let cdreq = (point.cd + point.cgd + cgdpp) - point.gds * point.vds - point.gm * point.vgs;
        // C's order, with the prime-prime terms added to the same rows while
        // those nodes alias the prime nodes.
        context.stamp_rhs(self.nodes.gp, m * (-ceqgs - ceqgd))?;
        context.stamp_rhs(
            self.nodes.dp,
            m * (-cdreq + (point.cgd - point.ggd * point.vgd) + ccorr),
        )?;
        context.stamp_rhs(self.nodes.dpp, cgdpp - ggdpp * vgdpp)?;
        context.stamp_rhs(
            self.nodes.sp,
            m * (cdreq + (point.cgs - point.ggs * point.vgs) - ccorr),
        )?;
        context.stamp_rhs(self.nodes.spp, cgspp - ggspp * vgspp)?;
        let (gds, gm) = (point.gds, point.gm);
        let (ggs, ggd) = (point.ggs, point.ggd);
        let (gdpr, gspr, ggate) = (derived.gdpr, derived.gspr, derived.ggate);
        let (tgi, tgf) = (derived.tgi, derived.tgf);
        let n = self.nodes;
        for (row, col, value) in [
            (n.d, n.d, m * gdpr),
            (n.s, n.s, m * gspr),
            (n.g, n.g, m * ggate),
            (n.spp, n.spp, m * (tgi + ggspp)),
            (n.dpp, n.dpp, m * (tgf + ggdpp)),
            (n.gp, n.gp, m * (ggd + ggs + ggate + ggspp + ggdpp)),
            (n.dp, n.dp, m * (gds + ggd + gdpr + tgf)),
            (n.sp, n.sp, m * (gds + gm + ggs + gspr + tgi)),
            (n.d, n.dp, -m * gdpr),
            (n.dp, n.d, -m * gdpr),
            (n.s, n.sp, -m * gspr),
            (n.sp, n.s, -m * gspr),
            (n.g, n.gp, -m * ggate),
            (n.gp, n.g, -m * ggate),
            (n.gp, n.dp, -m * ggd),
            (n.gp, n.sp, -m * ggs),
            (n.dp, n.gp, m * (gm - ggd)),
            (n.dp, n.sp, m * (-gds - gm)),
            (n.sp, n.gp, m * (-ggs - gm)),
            (n.sp, n.dp, -m * gds),
            (n.sp, n.spp, -m * tgi),
            (n.spp, n.sp, -m * tgi),
            (n.gp, n.spp, -m * ggspp),
            (n.spp, n.gp, -m * ggspp),
            (n.dp, n.dpp, -m * tgf),
            (n.dpp, n.dp, -m * tgf),
            (n.gp, n.dpp, -m * ggdpp),
            (n.dpp, n.gp, -m * ggdpp),
        ] {
            context.stamp(row, col, value)?;
        }
        Ok(())
    }
    /// `mesaacl.c`: the bias conductances plus `j omega` times the gate
    /// capacitances (each a two-terminal element, as in C), every term scaled
    /// by `m`.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let derived = self.derived(
            context.model_context.temperature,
            context.model_context.nominal_temperature,
        )?;
        let row = |node: NodeId| context.unknowns.node_row(node);
        let voltage = |node: NodeId| row(node).and_then(|r| bias.get(r)).unwrap_or(0.);
        let vgs = voltage(self.nodes.gp) - voltage(self.nodes.sp);
        let vgd = voltage(self.nodes.gp) - voltage(self.nodes.dp);
        let von = derived.tvto + self.model.ks * (voltage(self.nodes.sp) - self.model.vsg);
        let m = self.instance.m;
        let AcValues {
            gm,
            gds,
            capgs,
            capgd,
            ggs,
            ggd,
            ggspp,
            ggdpp,
        } = self.ac_point(
            &derived,
            vgs,
            vgd,
            von,
            context.model_context.gmin,
            context.model_context.frequency,
        )?;
        let n = self.nodes;
        let (gdpr, gspr, ggate) = (derived.gdpr, derived.gspr, derived.ggate);
        let (tgi, tgf) = (derived.tgi, derived.tgf);
        for (a, b, value) in [
            (n.d, n.d, m * gdpr),
            (n.s, n.s, m * gspr),
            (n.g, n.g, m * ggate),
            (n.spp, n.spp, m * (tgi + ggspp)),
            (n.dpp, n.dpp, m * (tgf + ggdpp)),
            (n.d, n.dp, -m * gdpr),
            (n.dp, n.d, -m * gdpr),
            (n.s, n.sp, -m * gspr),
            (n.sp, n.s, -m * gspr),
            (n.g, n.gp, -m * ggate),
            (n.gp, n.g, -m * ggate),
            (n.gp, n.dp, m * -ggd),
            (n.gp, n.sp, m * -ggs),
            (n.dp, n.gp, m * (gm - ggd)),
            (n.dp, n.sp, m * (-gds - gm)),
            (n.sp, n.gp, m * (-ggs - gm)),
            (n.sp, n.dp, m * -gds),
            (n.gp, n.gp, m * (ggd + ggs + ggate + ggspp + ggdpp)),
            (n.dp, n.dp, m * (gds + ggd + gdpr + tgf)),
            (n.sp, n.sp, m * (gds + gm + ggs + gspr + tgi)),
            (n.sp, n.spp, -m * tgi),
            (n.spp, n.sp, -m * tgi),
            (n.gp, n.spp, -m * ggspp),
            (n.spp, n.gp, -m * ggspp),
            (n.dp, n.dpp, -m * tgf),
            (n.dpp, n.dp, -m * tgf),
            (n.gp, n.dpp, -m * ggdpp),
            (n.dpp, n.gp, -m * ggdpp),
        ] {
            if let (Some(r), Some(c)) = (row(a), row(b)) {
                context.system.a.add(r, c, value)?;
            }
        }
        // `mesaacl.c`'s `Ptr + 1` entries: `xgs = qgs omega` and
        // `xgd = qgd omega`, where a small-signal load stores the
        // capacitances in those slots.
        context.nodal([n.gp, n.spp], m * capgs, true)?;
        context.nodal([n.gp, n.dpp], m * capgd, true)?;
        Ok(())
    }
    /// `mesapzl.c` is H1b.
    fn assemble_pole_zero(
        &self,
        _context: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            format!("{}: MESA level 2 pole-zero analysis", self.name),
            "src/spicelib/devices/mesa/mesapzl.c (H1b)",
        ))
    }
}
