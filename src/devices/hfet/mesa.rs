//! MESA, `Z` level 2 (`mesa1`).
//!
//! C references, read as behaviour only: `mesa/mesasetup.c` (`MESAsetup`: the
//! internal nodes and the instance defaults), `mesa/mesatemp.c`
//! (`MESAtemp`: every per-instance derived quantity), `mesa/mesaload.c`
//! (`MESAload` and the `mesa1` channel), `mesa/mesaparam.c` and
//! `mesa/mesaask.c`.
//!
//! This is H1b's first slice: `.op`, `.dc` and the small-signal assembly
//! (`.ac`, `.sp`, `.tf`) with every resistance path `mesasetup.c` and
//! `mesatemp.c` define - `rd`/`rs`, `rg`, `ri`/`rf` and the `rdi`/`rsi`
//! series pair. The charge storage and `NIintegrate` path (`mesatrunc.c`) and
//! the pole-zero load (`mesapzl.c`) still fail with explicit errors.
//!
//! # Deliberate divergences
//!
//! - C's bypass test and `MODEINITPRED` extrapolation are omitted, as for the
//!   JFET and MESFET.
//! - Where `mesa1` overflows or takes the root of a negative number the port
//!   reports a numerical error instead of continuing with NaN.
//! - `mesaload.c` reads the prime-prime pair straight from the solution, so it
//!   is never limited; the port does the same.

use crate::devices::hfet::{Instance, Model};
use crate::devices::initial::InstanceInitial;
use crate::devices::jfet::{BOLTZMANN, CHARGE};
use crate::devices::limiting;
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::sensitivity::{
    DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityParameter, SensitivityRecord,
};
use crate::devices::{
    AnalysisMode, Device, DeviceState, IterationPhase, LinearContext, ModelContext, ResolvedModel,
    StampContext, sweep::check_swept,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// `mesaload.c`'s `#define EPSILONGAAS (12.244*8.85418e-12)`, the same value
/// `mesasetup.c` defaults `MESAepsi` to. The modern `eps0`
/// (`8.854_187_817e-12`) is 8.8e-7 larger and scales `beta`, `n0`, `isatb0` and
/// `cf` with it (and `vpo` against it), which was the whole of the ~1.2e-6
/// drain-current residual the MESA goldens used to show.
const EPSILON_GAAS: Real = 12.244 * 8.854_18e-12;
/// C `CONSTKoverQ`: the thermal voltage per kelvin.
const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
const CELSIUS_TO_KELVIN: Real = 273.15;
/// `mesaload.c`'s `ns` cutoff, below which the channel is treated as off.
const N_MIN: Real = 1e-38;

/// `MESApTable`'s settable reals (`dctrcurv.c` `DCTsetInstParam`).
const SWEEPABLE: [&str; 8] = ["l", "w", "m", "icvds", "icvgs", "td", "ts", "dtemp"];

/// The state slots of `MESAload`'s `CKTstate0`, in the port's own order (C's
/// `MESAnumStates` has a different one, which nothing here needs). Each charge
/// is followed by its derivative, as `CKTterr` and `NIintegrate` require.
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
    pub(super) const GGDPP: usize = 13;
    /// C `MESAqgs`/`MESAcqgs` and `MESAqgd`/`MESAcqgd`: the accumulated gate
    /// charges and their integrated derivatives.
    pub(super) const QGS: usize = 14;
    pub(super) const CQGS: usize = 15;
    pub(super) const QGD: usize = 16;
    pub(super) const CQGD: usize = 17;
    pub(super) const COUNT: usize = 18;
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
    /// `here->MESAimax = CHARGE*NMAX*VS*W` (level 4 only).
    imax: Real,
    csatfs: Real,
    csatfd: Real,
    ggrwl: Real,
    vcrits: Real,
    vcritd: Real,
    gdpr: Real,
    gspr: Real,
    ggate: Real,
    /// `mesatemp.c`'s `tGi`/`tGf`: the conductances of `ri`/`rf`, stamped
    /// between the prime and prime-prime nodes.
    tgi: Real,
    tgf: Real,
    /// `mesatemp.c`'s `MESAtRdi`/`MESAtRsi`: the channel's series resistances
    /// (`mesa1`'s `RS`/`RD`), which create no node.
    trdi: Real,
    trsi: Real,
    /// `model->MESAvpo`: level 2's device depth term, or `vpou + vpod` for
    /// levels 3 and 4 (`mesatemp.c`'s else branch).
    vpo: Real,
    /// `vpou`/`vpod`, which only `mesa2` reads.
    vpou: Real,
    vpod: Real,
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
    /// `mesasetup.c`/`inpdomod.c`'s level: 2 (`mesa1`), 3 (`mesa2`) or 4
    /// (`mesa3`). The model card and every load are shared; the level selects
    /// the channel and three `mesatemp.c` derivations. C stores it in a
    /// **`double`** (`mesadefs.h`) and tests `== 2`/`== 3`/`== 4`, so a value
    /// between them matches no branch: `mesaload.c` then leaves the channel at
    /// zero and `mesatemp.c` takes its `else` arms. `.sens` perturbs the field,
    /// so the port keeps C's representation.
    level: Real,
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
        let level = resolved.levels().selector;
        if !matches!(level, 2..=4) {
            return Err(SpiceError::circuit(format!(
                "{}: MESA needs level 2, 3 or 4",
                i.name
            )));
        }
        if i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: MESA needs three terminals",
                i.name
            )));
        }
        let (model, model_defaults) = Model::resolve(resolved)?;
        let (initial, setters) = Instance::resolve(i)?;
        let instance = Instance::validate(&model_defaults, &setters, &i.location)?;
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
        // `mesasetup.c` creates `#source` for RS, `#drain` for RD, `#gate` for
        // RG, `#gs` for RI and `#gd` for RF, in that order, and falls back to
        // the unfurled node when a resistance is zero: the prime-prime pair
        // becomes the prime pair. `rdi`/`rsi` create no node at all;
        // `mesatemp.c` folds them into the channel's series terms. The
        // reference binary's rawfile omits `#source`/`#drain` (`outitf.c`'s
        // skip list), not the model: `show` reports `dprimenode`/`sprimenode`
        // and `rd=1e6 rs=1e6` collapses the current to `0.5/(rd+rs)`.
        let mut create = |resistance: Real, suffix: &str, target: &mut NodeId| -> SpiceResult<()> {
            if resistance == 0. {
                return Ok(());
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
            *target = internal;
            Ok(())
        };
        create(model.rs, "source", &mut nodes_of.sp)?;
        create(model.rd, "drain", &mut nodes_of.dp)?;
        create(model.rg, "gate", &mut nodes_of.gp)?;
        create(model.ri, "gs", &mut nodes_of.spp)?;
        if model.ri == 0. {
            nodes_of.spp = nodes_of.sp;
        }
        create(model.rf, "gd", &mut nodes_of.dpp)?;
        if model.rf == 0. {
            nodes_of.dpp = nodes_of.dp;
        }
        *nodes = staged;
        let device = Self {
            name: i.name.clone(),
            model_name: resolved.card().name.clone(),
            level: Real::from(level),
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
        // `mesatemp.c`: level 2 has one depth term, levels 3-4 split it into
        // the two-population `vpou`/`vpod`, which `mesa2` selects between.
        let vpou = CHARGE * model.ndu * model.du * model.du / 2. / EPSILON_GAAS;
        let vpod =
            CHARGE * model.ndelta * model.th * (2. * model.du + model.th) / 2. / EPSILON_GAAS;
        let vpo = if self.level == 2. {
            CHARGE * model.nd * model.d * model.d / 2. / EPSILON_GAAS
        } else {
            vpou + vpod
        };
        // Levels 3 and 4 scale the channel prefactor by the mobility, and
        // level 3 replaces the depth by `du` while level 4 switches the
        // permittivity to the model's `epsi`.
        let gchi0 = if self.level == 2. {
            CHARGE * instance.w / instance.l
        } else {
            CHARGE * instance.w / instance.l * tmu
        };
        let beta = 2. * EPSILON_GAAS * model.vs * model.zeta * instance.w / model.d;
        let depth = if self.level == 3. { model.du } else { model.d };
        let permittivity = if self.level == 4. {
            model.epsi
        } else {
            EPSILON_GAAS
        };
        // `mesatemp.c:78-81`: only level 4 halves the channel charge term;
        // levels 1-3 use the same expression without the 2.
        let n0 = if self.level == 4. {
            permittivity * teta * vts / 2. / CHARGE / depth
        } else {
            permittivity * teta * vts / CHARGE / depth
        };
        let nsb0 = EPSILON_GAAS * teta * vts / CHARGE / (model.du + model.th);
        let isatb0 = CHARGE * n0 * vts * instance.w / instance.l;
        let cf = 0.5 * permittivity * instance.w;
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
        let tri = scalar(model.ri, ts);
        let trf = scalar(model.rf, td);
        // `mesatemp.c`: RD, RDI and RF are scaled at `td`, RS, RSI, RG and RI
        // at `ts`.
        let trdi = scalar(model.rdi, td);
        let trsi = scalar(model.rsi, ts);
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
            imax: CHARGE * model.nmax * model.vs * instance.w,
            csatfs,
            csatfd,
            ggrwl,
            vcrits: vcrit(vts, csatfs),
            vcritd: vcrit(vtd, csatfd),
            gdpr: conductance(trd),
            gspr: conductance(trs),
            ggate: conductance(trg),
            tgi: conductance(tri),
            tgf: conductance(trf),
            trdi,
            trsi,
            vpo,
            vpou,
            vpod,
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

    /// The level's channel: `mesa1` (level 2), `mesa2` (level 3) or `mesa3`
    /// (level 4). `mesaload.c` selects between them on `model->MESAlevel`, and
    /// everything around them - the junctions, the charge storage, the load
    /// and the small-signal assembly - is shared.
    #[allow(clippy::too_many_arguments)]
    fn channel(
        &self,
        derived: &Derived,
        lambda: Real,
        vgs: Real,
        vds: Real,
        von: Real,
    ) -> SpiceResult<(Real, Real, Real, Real, Real)> {
        if self.level == 2. {
            self.mesa1(derived, lambda, vgs, vds, von)
        } else if self.level == 3. {
            self.mesa2(derived, lambda, vgs, vds, von)
        } else if self.level == 4. {
            self.mesa3(derived, lambda, vgs, vds, von)
        } else {
            // C's chain has no `else`: the channel variables keep the zeros
            // they were initialized with, so only the gate junctions load.
            Ok((0., 0., 0., 0., 0.))
        }
    }

    /// `mesa2` (`mesaload.c:690`): level 3's two-population channel. The
    /// expressions and their order are C's, including the four regions of
    /// `nsa`'s `vpo` selection and the `ca`/`cb` capacitance split.
    fn mesa2(
        &self,
        derived: &Derived,
        lambda: Real,
        vgs: Real,
        vds: Real,
        von: Real,
    ) -> SpiceResult<(Real, Real, Real, Real, Real)> {
        let model = &self.model;
        let vt = derived.vts;
        let etavth = derived.teta * vt;
        let rt = derived.trsi + derived.trdi;
        let vgt0 = vgs - von;
        let s = ((vgt0 - model.vsigmat) / model.vsigma).exp();
        let sigma = model.sigma0 / (1. + s);
        let vgt = vgt0 + sigma * vds;
        let t = vgt / vt - 1.;
        let q = (derived.delta_sqr + t * t).sqrt();
        let vgte = 0.5 * vt * (2. + t + q);
        let a = 2. * model.beta * vgte;
        // `mesa2` selects the donor density by where `vgte` sits relative to
        // the two threshold terms.
        let (nsa, ca, delnsavgte) = if vgt > derived.vpod {
            if vgte > derived.vpo {
                (
                    model.ndelta * model.th + model.ndu * model.du,
                    EPSILON_GAAS / model.du,
                    0.,
                )
            } else {
                let r = ((derived.vpo - vgte) / derived.vpou).sqrt();
                (
                    model.ndelta * model.th + model.ndu * model.du * (1. - r),
                    EPSILON_GAAS / model.du / r,
                    model.ndu * model.du / derived.vpou / 2. / r,
                )
            }
        } else if derived.vpod - vgte < 0. {
            (
                model.ndelta * model.th * (1. - model.du / model.th),
                EPSILON_GAAS / model.du,
                0.,
            )
        } else {
            let r = (1. + model.ndu / model.ndelta * (derived.vpod - vgte) / derived.vpou).sqrt();
            (
                model.ndelta * model.th * (1. - model.du / model.th * (r - 1.)),
                EPSILON_GAAS / model.du / r,
                model.du * model.ndu / 2. / derived.vpou / r,
            )
        };
        let b = (vgt / etavth).exp();
        let cb = EPSILON_GAAS / (model.du + model.th) * b;
        let nsb = derived.nsb0 * b;
        let delnsbvgt = nsb / etavth;
        let ns = nsa * nsb / (nsa + nsb);
        if ns < N_MIN {
            return Ok((0., 0., 0., derived.cf, derived.cf));
        }
        let gchi = derived.gchi0 * ns;
        let gch = gchi / (1. + gchi * rt);
        let f = (1. + 2. * a * model.rsi).sqrt();
        let d = 1. + a * model.rsi + f;
        let e = 1. + model.tc * vgte;
        let isata = a * vgte / d / e;
        let isatb = derived.isatb0 * b;
        let isat = isata * isatb / (isata + isatb);
        let vsate = isat / gch;
        let vdse = vds * (1. + (vds / vsate).powf(model.mc)).powf(-1. / model.mc);
        let g = (vds / vsate).powf(model.m);
        let h = (1. + g).powf(1. / model.m);
        let delidgch0 = vds / h;
        let delidgch = delidgch0 * (1. + lambda * vds);
        let cdrain = gch * delidgch;
        let cgc = self.instance.w * self.instance.l * ca * cb / (ca + cb);
        let c = (vsate - vdse) / (2. * vsate - vdse);
        let c = c * c;
        let capgs = derived.cf + 2. / 3. * cgc * (1. - c);
        let c = vsate / (2. * vsate - vdse);
        let c = c * c;
        let capgd = derived.cf + 2. / 3. * cgc * (1. - c);
        let delvgtevgt = 0.5 * (1. + t / q);
        let delidvds0 = gch / h;
        let delidvds1 = if vds != 0. {
            cdrain * (vds / vsate).powf(model.m - 1.) / vsate / (1. + g)
        } else {
            0.
        };
        let delidvds = delidvds0 * (1. + 2. * lambda * vds) - delidvds1;
        let delgchgchi = 1. / (1. + gchi * rt) / (1. + gchi * rt);
        let delgchins = derived.gchi0;
        let r = nsa + nsb;
        let r = r * r;
        let delnsvgt = (nsb * nsb * delvgtevgt * delnsavgte + nsa * nsa * delnsbvgt) / r;
        let delidvsate = cdrain * g / vsate / (1. + g);
        let delvsateisat = 1. / gch;
        let r = isata + isatb;
        let r = r * r;
        let delisatisata = isatb * isatb / r;
        let ddevgte = 2. * model.beta * model.rsi * (1. + 1. / f) * e + d * model.tc;
        let delisatavgte = (2. * a * d * e - a * vgte * ddevgte) / d / d / e / e;
        let delisatisatb = isata * isata / r;
        let delisatbvgt = isatb / etavth;
        let delvsategch = -vsate / gch;
        let delvgtvgs = 1. - model.sigma0 * vds * s / model.vsigma / (1. + s) / (1. + s);
        let p = delgchgchi * delgchins * delnsvgt;
        let delvsatevgt = delvsateisat
            * (delisatisata * delisatavgte * delvgtevgt + delisatisatb * delisatbvgt)
            + delvsategch * p;
        let gm1 = delidvsate * delvsatevgt;
        let g = delidgch * p + gm1;
        let gm = g * delvgtvgs;
        let gds = delidvds + g * sigma;
        let values = [cdrain, gm, gds, capgs, capgd];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(self.numerical("nonfinite mesa2 channel equations"));
        }
        Ok((cdrain, gm, gds, capgs, capgd))
    }

    /// `mesa3` (`mesaload.c:858`): level 4's `nmax`/`gamma` channel, the same
    /// shape as `hfeta2` but with the `cas`/`cbs` capacitance split.
    fn mesa3(
        &self,
        derived: &Derived,
        lambda: Real,
        vgs: Real,
        vds: Real,
        von: Real,
    ) -> SpiceResult<(Real, Real, Real, Real, Real)> {
        let model = &self.model;
        let vt = derived.vts;
        let etavth = derived.teta * vt;
        let vl = model.vs / derived.tmu * self.instance.l;
        let rt = derived.trsi + derived.trdi;
        let vgt0 = vgs - von;
        let s = ((vgt0 - model.vsigmat) / model.vsigma).exp();
        let sigma = model.sigma0 / (1. + s);
        let vgt = vgt0 + sigma * vds;
        let u = 0.5 * vgt / vt - 1.;
        let t = (derived.delta_sqr + u * u).sqrt();
        let vgte = vt * (2. + u + t);
        let b = (vgt / etavth).exp();
        let nsm = 2. * derived.n0 * (1. + 0.5 * b).ln();
        if nsm < N_MIN {
            return Ok((0., 0., 0., derived.cf, derived.cf));
        }
        let c = (nsm / model.nmax).powf(model.gamma);
        let q = (1. + c).powf(1. / model.gamma);
        let ns = nsm / q;
        let gchi = derived.gchi0 * ns;
        let gch = gchi / (1. + gchi * rt);
        let gchim = derived.gchi0 * nsm;
        let h = (1. + 2. * gchim * model.rsi + vgte * vgte / (vl * vl)).sqrt();
        let p = 1. + gchim * model.rsi + h;
        let isatm = gchim * vgte / p;
        let g = (isatm / derived.imax).powf(model.gamma);
        let isat = isatm / (1. + g).powf(1. / model.gamma);
        let vsate = isat / gch;
        let vdse = vds * (1. + (vds / vsate).powf(model.mc)).powf(-1. / model.mc);
        let d = (vds / vsate).powf(model.m);
        let e = (1. + d).powf(1. / model.m);
        let delidgch = vds * (1. + lambda * vds) / e;
        let cdrain = gch * delidgch;
        // `mesa3`'s capacitance split, which is the only place `cas`/`cbs`
        // and the model's `epsi` enter the channel.
        let cgcm = 1.
            / (1. / model.cas * model.d / model.epsi
                + 1. / model.cbs * etavth / CHARGE / derived.n0 * (-vgt / etavth).exp());
        let cgc = self.instance.w * self.instance.l * cgcm / (1. + c).powf(1. + 1. / model.gamma);
        let third = 2.0 / 3.0;
        let a = (vsate - vdse) / (2. * vsate - vdse);
        let a = a * a;
        let capgs = derived.cf + third * cgc * (1. - a);
        let a = vsate / (2. * vsate - vdse);
        let a = a * a;
        let capgd = derived.cf + third * cgc * (1. - a);
        let delidvsate = cdrain * d / vsate / (1. + d);
        let delidvds = gch * (1. + 2. * lambda * vds) / e
            - cdrain * (vds / vsate).powf(model.m - 1.) / (vsate * (1. + d));
        let a = 1. + gchi * rt;
        let delgchgchi = 1. / (a * a);
        let delgchins = derived.gchi0;
        let delnsnsm = ns / nsm * (1. - c / (1. + c));
        let delnsmvgt = derived.n0 / etavth / (1. / b + 0.5);
        let delvgtevgt = 0.5 * (1. + u / t);
        let delvsateisat = 1. / gch;
        let delisatisatm = isat / isatm * (1. - g / (1. + g));
        let delisatmvgte = gchim * (p - vgte * vgte / (vl * vl * h)) / (p * p);
        let delvsategch = -vsate / gch;
        let delisatmgchim = vgte * (p - gchim * model.rsi * (1. + 1. / h)) / (p * p);
        let delvgtvgs = 1. - vds * model.sigma0 / model.vsigma * s / ((1. + s) * (1. + s));
        let p = delgchgchi * delgchins * delnsnsm * delnsmvgt;
        let delvsatevgt = delvsateisat
            * delisatisatm
            * (delisatmvgte * delvgtevgt + delisatmgchim * derived.gchi0 * delnsmvgt)
            + delvsategch * p;
        let g = delidgch * p + delidvsate * delvsatevgt;
        let gm = g * delvgtvgs;
        let gds = delidvds + g * sigma;
        let values = [cdrain, gm, gds, capgs, capgd];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(self.numerical("nonfinite mesa3 channel equations"));
        }
        Ok((cdrain, gm, gds, capgs, capgd))
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
        let rt = derived.trsi + derived.trdi;
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
        // `mesaload.c`'s `1.0/ND/D/(1-sqrt1)`, i.e. `1/(ND D (1 - sqrt1))`;
        // the `b/n0` term dominates in subthreshold, where the two spellings
        // agree, and it is the whole difference in strong inversion.
        let denom = 1. / model.nd / model.d / (1. - sqrt1);
        let ns = 1. / (denom + 1. / derived.n0 * b);
        if ns < N_MIN {
            return Ok((0., 0., 0., derived.cf, derived.cf));
        }
        let gchi = derived.gchi0 * mu * ns;
        let gch = gchi / (1. + gchi * rt);
        let f = (1. + 2. * a * derived.trsi).sqrt();
        let d = 1. + a * derived.trsi + f;
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
        let temp = 1. + gchi * rt;
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
        let ddevgte = 2. * beta * derived.trsi * v * e + d * model.tc;
        let temp = d * d * e * e;
        let delisatavgte = (2. * a * d * e - a * vgte * ddevgte) / temp;
        let delisatabeta = 2. * vgte * vgte * (d * e - a * e * derived.trsi * v) / temp;
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
        let (cdrain, gm, gds, capgs, capgd) = self.channel(
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
            self.channel(derived, lambda, if inverse { vgd } else { vgs }, vds, von)?;
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

    /// The DC stamp cannot serve an AC-mode load: AC, `.sp` and `.tf` go
    /// through [`Device::assemble_small_signal`], which implements
    /// `mesaacl.c` (the `m10_mesa_ac` fixture verifies it against C). Same
    /// guard shape as the JFET's.
    fn is_ac(&self) -> SpiceError {
        SpiceError::circuit(format!(
            "{}: MESA AC needs small-signal assembly",
            self.name
        ))
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
    /// `mesatrunc.c`: both gate charges control the timestep.
    fn truncation_slots(&self) -> Vec<usize> {
        vec![slot::QGS, slot::QGD]
    }
    /// Noiseless: `mesainit.c` leaves `DEVnoise = NULL`, so C's `.noise` runs
    /// the card with no contribution at all (its sibling `mes/` has
    /// `MESnoise`; MESA and both HFET levels do not). The measured reference
    /// behaviour is a run with `onoise_spectrum` zero on a resistor-only
    /// circuit, not an error.
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }
    /// Linear in `.disto`: `mesainit.c` leaves `DEVdisto = NULL`, so the card
    /// enters only through its small-signal matrix.
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(crate::devices::distortion::DeviceDistortion::Linear)
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
        // `mesaask.c` also reports `qgs`/`cqgs`/`qgd`/`cqgd` from `CKTstate0`.
        // C accumulates them across accepted points (a `.dc` sweep carries the
        // previous point's charge), and this hook sees only the converged
        // solution, so they are refused rather than recomputed from the bias
        // and reported as if they were C's.
        if matches!(key, "qgs" | "cqgs" | "qgd" | "cqgd") {
            return Err(SpiceError::not_yet_ported(
                format!("{}: MESA @{}[{key}]", self.name, self.name),
                "src/spicelib/devices/mesa/mesaask.c (MESAqgs/MESAcqgs read CKTstate0)",
            ));
        }
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
    /// `mesa.c`'s `IFparm` tables under C's `sgen` walk (#164). Levels 2 and 4
    /// are verified against C; level 3's `mesa2` path still differs on the
    /// `DU`/`TH` rows (see `docs/port/SENSITIVITY.md`), so it is refused.
    fn sensitivity(&self, context: &ModelContext) -> SpiceResult<Box<dyn DeviceSensitivity + '_>> {
        if !matches!(self.level, l if l == 2. || l == 4.) {
            return Err(SpiceError::not_yet_ported(
                format!("{}: MESA level {} sensitivity", self.name, self.level),
                "src/spicelib/devices/mesa/mesatemp.c, mesaload.c (mesa2/mesa3)",
            ));
        }
        // A level-4 instance whose temperature is shifted by TS/DTEMP still
        // differs from C on `rtc1`/`rtc2`: measured at `ts=40` on a level-4 card
        // the port answers -4.459e-3/-5.796e-2 against C's -5.519e-3/-7.174e-2
        // (the same 0.808 ratio on both), while `.op`/`.dc`/`.tran` agree to
        // 1e-9 and every other column is inside 1e-6. Refuse rather than
        // report those two rows. Level 2 verifies with `dtemp` set
        // (`sens_mesa`'s `z2`).
        if self.level == 4. && (self.instance.ts.is_some() || self.instance.dtemp != 0.) {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: MESA level 4 sensitivity at a shifted temperature",
                    self.name
                ),
                "src/spicelib/devices/mesa/mesatemp.c (the RTC1/RTC2 scaling of \
                 trdi/trsi/trs/trd when TS/DTEMP move the instance temperature)",
            ));
        }
        Ok(Box::new(MesaSensitivity::new(self, context)))
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
            level: self.level,
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
        // `mesaload.c`'s `vgspp`/`vgdpp`: read from the solution, carried
        // forward unextrapolated under prediction (`state0[pp] = state1[pp]`,
        // where the main pair extrapolates with `xfact`), and replaced by the
        // limited junction value while the prime-prime node aliases its prime
        // node. C never limits the pair itself: `ri`/`rf` make it a linear
        // node between the two. The `MODEINITJCT` branches set a fresh 0
        // instead; that differs on the first iteration only, and the port's
        // DC goldens pin the converged result either way.
        let pp = |aliased: bool, node: NodeId, limited: Real, slot: usize| {
            if aliased {
                return limited;
            }
            let solved = || voltage(self.nodes.gp) - voltage(node);
            if context.states.phase() == IterationPhase::Predict {
                return context
                    .states
                    .accepted(1, slot)
                    .filter(|v| v.is_finite())
                    .unwrap_or_else(solved);
            }
            solved()
        };
        let vgspp = pp(
            self.nodes.spp == self.nodes.sp,
            self.nodes.spp,
            vgs,
            slot::VGSPP,
        );
        let vgdpp = pp(
            self.nodes.dpp == self.nodes.dp,
            self.nodes.dpp,
            vgd,
            slot::VGDPP,
        );
        let von = derived.tvto + self.model.ks * (voltage(self.nodes.sp) - self.model.vsg);
        let point = self.point(&derived, vgs, vgd, von, gmin)?;
        // `mesaload.c`'s charge storage, which C runs before its convergence
        // test because the test compares `cghat` with the *integrated* `cg`.
        // The trial charge accumulates from the accepted charge and the
        // prime-prime voltage - macspice's incremental form, where the JFET
        // computes a closed-form charge - and only a transient load
        // integrates it. `.dc` still accumulates (C's `MODEDCTRANCURVE`), so
        // `@z1[qgs]` reports it mid-sweep without changing a stamp.
        let initial_transient =
            context.states.initial_transient() && context.states.phase() == IterationPhase::Predict;
        let mut cg = point.cg;
        let mut cd = point.cd;
        let (mut ggspp, mut ggdpp, mut cgspp, mut cgdpp) = (0., 0., 0., 0.);
        let (mut qgs, mut qgd) = (0., 0.);
        if matches!(
            context.mode,
            AnalysisMode::Transient { .. } | AnalysisMode::DcSweep
        ) {
            // C reads `state1[MESAvgspp]`/`state1[MESAvgdpp]` first, patches
            // `state1[MESAqgs]` for `MODEINITTRAN`, and only then accumulates.
            let vgs1 = context
                .states
                .accepted(1, slot::VGSPP)
                .filter(|v| v.is_finite())
                .unwrap_or(vgspp);
            let vgd1 = context
                .states
                .accepted(1, slot::VGDPP)
                .filter(|v| v.is_finite())
                .unwrap_or(vgdpp);
            if initial_transient {
                context
                    .states
                    .patch_state1(slot::QGS, point.capgs * vgspp)?;
                context
                    .states
                    .patch_state1(slot::QGD, point.capgd * vgdpp)?;
            }
            let qgs1 = context
                .states
                .accepted(1, slot::QGS)
                .filter(|v| v.is_finite())
                .unwrap_or(0.);
            let qgd1 = context
                .states
                .accepted(1, slot::QGD)
                .filter(|v| v.is_finite())
                .unwrap_or(0.);
            qgs = qgs1 + point.capgs * (vgspp - vgs1);
            qgd = qgd1 + point.capgd * (vgdpp - vgd1);
            if initial_transient {
                // C copies the accumulated charge into `CKTstate1` before
                // `NIintegrate`, so the integration history starts there.
                context.states.patch_state1(slot::QGS, qgs)?;
                context.states.patch_state1(slot::QGD, qgd)?;
            }
            if let Some(coefficients) = context.integration {
                let AnalysisMode::Transient { dt, .. } = context.mode else {
                    return Err(SpiceError::circuit(
                        "MESA charge companion outside transient",
                    ));
                };
                if dt != coefficients.dt() {
                    return Err(SpiceError::circuit("MESA charge timestep mismatch"));
                }
                let junction = |charge, capacitance| JunctionPoint {
                    current: 0.,
                    conductance: 0.,
                    charge,
                    capacitance,
                };
                let (geq, cqgs) = junction_companion(
                    context,
                    coefficients,
                    junction(qgs, point.capgs),
                    slot::QGS,
                )?;
                ggspp = geq;
                cgspp = cqgs;
                cg += cqgs;
                let (geq, cqgd) = junction_companion(
                    context,
                    coefficients,
                    junction(qgd, point.capgd),
                    slot::QGD,
                )?;
                ggdpp = geq;
                cgdpp = cqgd;
                cg += cqgd;
                cd -= cqgd;
                if initial_transient {
                    context.states.patch_state1(slot::CQGS, cqgs)?;
                    context.states.patch_state1(slot::CQGD, cqgd)?;
                }
            }
        }
        // The convergence test compares the predicted currents with the
        // integrated ones (C's `cg`/`cd` after the charge block).
        let point = Point { cg, cd, ..point };
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
            (slot::CQGS, cgspp),
            (slot::GGDPP, ggdpp),
            (slot::CQGD, cgdpp),
            (slot::QGS, qgs),
            (slot::QGD, qgd),
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
        // `mesaacl.c`/`mesapzl.c` build their conductances from `gm0`/`gm1`/
        // `gm2`, `gds0` and `delidgch0`/`delidvds0`/`delidvds1`, which `mesa1`
        // and `mesa2` store and **`mesa3` never writes**. A level-4 device
        // therefore answers `.ac` with those fields at their zeroed initial
        // values: no transconductance and no output conductance at all.
        // Measured on a level-4 card that carries -1.55e-4 A in DC, the
        // reference binary prints 1.07e-8 A at 1 kHz and the same at 1 MHz.
        // Refused rather than reproduced, as `y`/TXL's AC is.
        //
        // `state` is `Some` exactly on the swept small-signal path (`.ac`,
        // `.sp` and `.pz`), and `None` elsewhere - on the probe a `.dc` sweep
        // assembles to resolve a swept source, and on `.tf`, which linearises
        // the converged DC Jacobian (C's `CKTtf` does not go through
        // `CKTacLoad`). Those two probes need the *correct* linearisation,
        // which the port computes below; only the swept path would have to
        // reproduce C's zeroed fields, and that is refused.
        if self.level == 4. && context.states.is_some() {
            return Err(SpiceError::not_yet_ported(
                format!("{}: MESA level 4 AC/SP/pole-zero", self.name),
                "src/spicelib/devices/mesa/mesaacl.c reads the `gm0`/`gds0`/`delidgch0` \
                 fields that mesaload.c's mesa3 never stores (`.dc` sweeps, `.tf` and the \
                 transient still work: they need only the correct linearisation)",
            ));
        }
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
    /// `mesapzl.c` is `mesaacl.c` with `s` in place of `j omega`: identical
    /// stamps, with `xgs`/`xgd` the gate capacitances themselves (`Ptr + 1`
    /// becomes the `s` multiple the pole-zero pencil applies). The port
    /// assembles the same dynamic terms, which its pole-zero analysis
    /// multiplies by `s`.
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
}

// ---------------------------------------------------------------------------
// `.sens` (#164)
// ---------------------------------------------------------------------------

/// `MESApTable` entries `set_param()` accepts, in C's table order (`off` is a
/// flag and the `*node`/bias asks are not settable).
const SENSITIVITY_INSTANCE: &[SensitivityParameter] = &[
    SensitivityParameter::dc("l"),
    SensitivityParameter::dc("w"),
    SensitivityParameter::dc("m"),
    SensitivityParameter::dc("icvds"),
    SensitivityParameter::dc("icvgs"),
    SensitivityParameter::dc("td"),
    SensitivityParameter::dc("ts"),
    SensitivityParameter::dc("dtemp"),
];

/// `MESAmPTable` in C's order: `vt0`, `tphib`, `tggr` and `alphat` are `IOPR`
/// (redundant aliases) and `gd`/`gs`/`vcrit` are ask-only.
const SENSITIVITY_MODEL: &[SensitivityParameter] = &[
    SensitivityParameter::dc("vto"),
    SensitivityParameter::dc("lambda"),
    SensitivityParameter::dc("lambdahf"),
    SensitivityParameter::dc("beta"),
    SensitivityParameter::dc("vs"),
    SensitivityParameter::dc("rd"),
    SensitivityParameter::dc("rs"),
    SensitivityParameter::dc("rg"),
    SensitivityParameter::dc("ri"),
    SensitivityParameter::dc("rf"),
    SensitivityParameter::dc("rdi"),
    SensitivityParameter::dc("rsi"),
    SensitivityParameter::dc("phib"),
    SensitivityParameter::dc("phib1"),
    SensitivityParameter::dc("astar"),
    SensitivityParameter::dc("ggr"),
    SensitivityParameter::dc("del"),
    SensitivityParameter::dc("xchi"),
    SensitivityParameter::dc("n"),
    SensitivityParameter::dc("eta"),
    SensitivityParameter::dc("m"),
    SensitivityParameter::dc("mc"),
    SensitivityParameter::dc("alpha"),
    SensitivityParameter::dc("sigma0"),
    SensitivityParameter::dc("vsigmat"),
    SensitivityParameter::dc("vsigma"),
    SensitivityParameter::dc("mu"),
    SensitivityParameter::dc("theta"),
    SensitivityParameter::dc("mu1"),
    SensitivityParameter::dc("mu2"),
    SensitivityParameter::dc("d"),
    SensitivityParameter::dc("nd"),
    SensitivityParameter::dc("du"),
    SensitivityParameter::dc("ndu"),
    SensitivityParameter::dc("th"),
    SensitivityParameter::dc("ndelta"),
    SensitivityParameter::dc("delta"),
    SensitivityParameter::dc("tc"),
    SensitivityParameter::dc("tvto"),
    SensitivityParameter::dc("tlambda"),
    SensitivityParameter::dc("teta0"),
    SensitivityParameter::dc("teta1"),
    SensitivityParameter::dc("tmu"),
    SensitivityParameter::dc("xtm0"),
    SensitivityParameter::dc("xtm1"),
    SensitivityParameter::dc("xtm2"),
    SensitivityParameter::dc("ks"),
    SensitivityParameter::dc("vsg"),
    SensitivityParameter::dc("tf"),
    SensitivityParameter::dc("flo"),
    SensitivityParameter::dc("delfo"),
    SensitivityParameter::dc("ag"),
    SensitivityParameter::dc("rtc1"),
    SensitivityParameter::dc("rtc2"),
    SensitivityParameter::dc("zeta"),
    SensitivityParameter::dc("level"),
    SensitivityParameter::dc("nmax"),
    SensitivityParameter::dc("gamma"),
    SensitivityParameter::dc("epsi"),
    SensitivityParameter::dc("cas"),
    SensitivityParameter::dc("cbs"),
];

/// A MESA's `.sens` description: `mesa.c`'s tables, `mesaparam.c`/
/// `mesamparam.c` setters, `mesasetup.c`'s defaults and `mesatemp.c`'s
/// temperature derivations. The load is the port's own [`Mesa`] rebuilt from
/// C's records.
#[derive(Debug)]
pub(super) struct MesaSensitivity<'a> {
    device: &'a Mesa,
    circuit_kelvin: Real,
}

impl<'a> MesaSensitivity<'a> {
    pub(super) fn new(device: &'a Mesa, context: &ModelContext) -> Self {
        Self {
            device,
            circuit_kelvin: context.temperature + CELSIUS_TO_KELVIN,
        }
    }
}

impl DeviceSensitivity for MesaSensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        SensitivityFamily::Mesa
    }

    fn model(&self) -> Option<&str> {
        Some(&self.device.model_name)
    }

    fn model_parameters(&self) -> &'static [SensitivityParameter] {
        SENSITIVITY_MODEL
    }

    fn instance_parameters(&self) -> &'static [SensitivityParameter] {
        SENSITIVITY_INSTANCE
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let (m, i) = (&self.device.model, &self.device.instance);
        let mut model = SensitivityRecord::new();
        for (keyword, value, given) in [
            ("vto", m.threshold, true),
            ("lambda", m.lambda, true),
            (
                "lambdahf",
                m.lambdahf.unwrap_or(m.lambda),
                m.lambdahf.is_some(),
            ),
            ("beta", m.beta, true),
            ("vs", m.vs, true),
            ("rd", m.rd, true),
            ("rs", m.rs, true),
            ("rg", m.rg, true),
            ("ri", m.ri, true),
            ("rf", m.rf, true),
            ("rdi", m.rdi, true),
            ("rsi", m.rsi, true),
            ("phib", m.phib, true),
            ("phib1", m.phib1, true),
            ("astar", m.astar, true),
            ("ggr", m.ggr, true),
            ("del", m.del, true),
            ("xchi", m.xchi, true),
            ("n", m.n, true),
            ("eta", m.eta, true),
            ("m", m.m, true),
            ("mc", m.mc, true),
            ("alpha", m.alpha, true),
            ("sigma0", m.sigma0, true),
            ("vsigmat", m.vsigmat, true),
            ("vsigma", m.vsigma, true),
            ("mu", m.mu, true),
            ("theta", m.theta, true),
            ("mu1", m.mu1, true),
            ("mu2", m.mu2, true),
            ("d", m.d, true),
            ("nd", m.nd, true),
            ("du", m.du, true),
            ("ndu", m.ndu, true),
            ("th", m.th, true),
            ("ndelta", m.ndelta, true),
            ("delta", m.delta, true),
            ("tc", m.tc, true),
            ("tvto", m.tvto, true),
            ("tlambda", m.tlambda, true),
            ("teta0", m.teta0, true),
            ("teta1", m.teta1, true),
            ("tmu", m.tmu, true),
            ("xtm0", m.xtm0, true),
            ("xtm1", m.xtm1, true),
            ("xtm2", m.xtm2, true),
            ("ks", m.ks, true),
            ("vsg", m.vsg, true),
            (
                "tf",
                m.tf.map_or(self.circuit_kelvin, |c| c + CELSIUS_TO_KELVIN),
                m.tf.is_some(),
            ),
            ("flo", m.flo, true),
            ("delfo", m.delfo, true),
            ("ag", m.ag, true),
            ("rtc1", m.tc1, true),
            ("rtc2", m.tc2, true),
            ("zeta", m.zeta, true),
            ("level", Real::from(self.device.level), true),
            ("nmax", m.nmax, true),
            ("gamma", m.gamma, true),
            ("epsi", m.epsi, true),
            ("cas", m.cas, true),
            ("cbs", m.cbs, true),
        ] {
            model = model.with(keyword, value, given);
        }
        let mut instance = SensitivityRecord::new();
        for (keyword, value, given) in [
            ("l", i.l, true),
            ("w", i.w, true),
            ("m", i.m, true),
            ("icvds", self.device.initial.values[0].unwrap_or(0.), true),
            ("icvgs", self.device.initial.values[1].unwrap_or(0.), true),
            (
                "td",
                i.td.map_or(self.circuit_kelvin + i.dtemp, |c| c + CELSIUS_TO_KELVIN),
                i.td.is_some(),
            ),
            (
                "ts",
                i.ts.map_or(self.circuit_kelvin + i.dtemp, |c| c + CELSIUS_TO_KELVIN),
                i.ts.is_some(),
            ),
            ("dtemp", i.dtemp, true),
        ] {
            instance = instance.with(keyword, value, given);
        }
        self.setup(&mut model, &mut instance)?;
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }

    /// `MESAmMask` returns `MESAphib`/`MESAphib1` **raw** (joules) while their
    /// setters multiply by `CHARGE` (`mesamparam.c`), so C's perturbation *and*
    /// its restore apply the factor once more: after the `phib` column is
    /// processed the stored barrier is `phib * CHARGE` (about 1e-38 J), and
    /// every later column is computed with it. The reference binary does exactly
    /// that - `show` reports `@mesmod2[phib] = 1.283485e-38` after a `.sens`
    /// run - so the replay reproduces it instead of "fixing" it.
    fn set(
        &self,
        scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        match (scope, keyword) {
            (ParameterScope::Model, "phib" | "phib1") => model.set(keyword, value * CHARGE),
            // `mesamparam.c` stores `tlambda`, `teta0`, `teta1`, `tmu` and
            // `tf` as `value + CONSTCtoK`, so every perturbation of them is
            // offset by 273.15 K and restoring the asked value re-adds it:
            // after the `teta1` row the model holds 273.15, and the `tmu` row
            // leaves 573.3, which is what makes the later `xtm0` row non-zero
            // in C (`pow(ts/tmu, xtm0)` is exactly 1 while `tmu` is 300.15).
            (ParameterScope::Model, "tlambda" | "teta0" | "teta1" | "tmu" | "tf") => {
                model.set(keyword, value + CELSIUS_TO_KELVIN)
            }
            (ParameterScope::Model, _) => model.set(keyword, value),
            (ParameterScope::Instance, _) => instance.set(keyword, value),
        }
    }

    /// `MESAsetup`: `tf` defaults to the circuit temperature and `level` to 2.
    /// Every other default is already in the model the port resolved, and none
    /// of them feeds a *given*-flag-dependent derivation except `lambdahf`,
    /// which `mesatemp.c` re-derives in [`Self::temperature`].
    fn setup(
        &self,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if !model.given("tf") {
            model.store("tf", self.circuit_kelvin)?;
        }
        if !model.given("level") {
            model.store("level", 2.)?;
        }
        if !instance.given("td") {
            let dtemp = instance.value("dtemp")?;
            instance.store("td", self.circuit_kelvin + dtemp)?;
        }
        if !instance.given("ts") {
            let dtemp = instance.value("dtemp")?;
            instance.store("ts", self.circuit_kelvin + dtemp)?;
        }
        Ok(())
    }

    /// `mesatemp.c`: `lambdahf` follows `lambda` while it is not given. The
    /// level-dependent `vpo`/`vpou`/`vpod` and every temperature coefficient
    /// are recomputed by the port's load from these records.
    fn temperature(
        &self,
        model: &mut SensitivityRecord,
        _instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        if !model.given("lambdahf") {
            let lambda = model.value("lambda")?;
            model.store("lambdahf", lambda)?;
        }
        Ok(())
    }

    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        if mode == SensitivityMode::Ac {
            return Err(SpiceError::not_yet_ported(
                format!("{}: MESA AC sensitivity", self.device.name),
                "src/spicelib/devices/mesa/mesaacl.c",
            ));
        }
        let (m, i) = (current.model, current.instance);
        let celsius = |record: &SensitivityRecord, keyword: &str| -> SpiceResult<Option<Real>> {
            Ok(record
                .given(keyword)
                .then(|| record.value(keyword))
                .transpose()?
                .map(|kelvin| kelvin - CELSIUS_TO_KELVIN))
        };
        let model = Model {
            threshold: m.value("vto")?,
            lambda: m.value("lambda")?,
            lambdahf: m
                .given("lambdahf")
                .then(|| m.value("lambdahf"))
                .transpose()?,
            vs: m.value("vs")?,
            rd: m.value("rd")?,
            rs: m.value("rs")?,
            rg: m.value("rg")?,
            ri: m.value("ri")?,
            rf: m.value("rf")?,
            rdi: m.value("rdi")?,
            rsi: m.value("rsi")?,
            phib: m.value("phib")?,
            phib1: m.value("phib1")?,
            astar: m.value("astar")?,
            ggr: m.value("ggr")?,
            del: m.value("del")?,
            xchi: m.value("xchi")?,
            n: m.value("n")?,
            eta: m.value("eta")?,
            m: m.value("m")?,
            mc: m.value("mc")?,
            alpha: m.value("alpha")?,
            sigma0: m.value("sigma0")?,
            vsigmat: m.value("vsigmat")?,
            vsigma: m.value("vsigma")?,
            mu: m.value("mu")?,
            theta: m.value("theta")?,
            mu1: m.value("mu1")?,
            mu2: m.value("mu2")?,
            d: m.value("d")?,
            nd: m.value("nd")?,
            beta: m.value("beta")?,
            ndu: m.value("ndu")?,
            ndelta: m.value("ndelta")?,
            nmax: m.value("nmax")?,
            gamma: m.value("gamma")?,
            epsi: m.value("epsi")?,
            cas: m.value("cas")?,
            cbs: m.value("cbs")?,
            du: m.value("du")?,
            th: m.value("th")?,
            delta: m.value("delta")?,
            tc: m.value("tc")?,
            tvto: m.value("tvto")?,
            tlambda: m.value("tlambda")?,
            teta0: m.value("teta0")?,
            teta1: m.value("teta1")?,
            tmu: m.value("tmu")?,
            xtm0: m.value("xtm0")?,
            xtm1: m.value("xtm1")?,
            xtm2: m.value("xtm2")?,
            ks: m.value("ks")?,
            vsg: m.value("vsg")?,
            tf: celsius(m, "tf")?,
            flo: m.value("flo")?,
            delfo: m.value("delfo")?,
            ag: m.value("ag")?,
            tc1: m.value("rtc1")?,
            tc2: m.value("rtc2")?,
            zeta: m.value("zeta")?,
        };
        let instance = Instance {
            l: i.value("l")?,
            w: i.value("w")?,
            m: i.value("m")?,
            td: celsius(i, "td")?,
            ts: celsius(i, "ts")?,
            dtemp: i.value("dtemp")?,
        };
        // `MESAmParam` writes the level straight into C's `double` field, so a
        // perturbation of 1 ppm leaves a value that matches none of the
        // `== 2/3/4` tests; the load then carries no channel, as in C.
        let level = m.value("level")?;
        let device = Mesa {
            name: self.device.name.clone(),
            model_name: self.device.model_name.clone(),
            level,
            terminals: self.device.terminals.clone(),
            nodes: self.device.nodes,
            model,
            instance,
            initial: self.device.initial.clone(),
        };
        Ok(SensitivityLoad::Replacement(Box::new(device)))
    }
}
