//! HFET2, the `Z` designator at level 6 (`hfet2/`, the `hfeta2` channel).
//!
//! C references, read as behaviour only: `hfet2/hfet2setup.c` (defaults and the
//! conditional internal nodes), `hfet2/hfet2temp.c` (`HFET2temp`),
//! `hfet2/hfet2load.c` (`HFET2load` and `hfeta2`), `hfet2/hfet2acl.c`,
//! `hfet2/hfet2pzl.c`, `hfet2/hfet2trunc.c`, `hfet2/hfet2getic.c`,
//! `hfet2/hfet2ask.c`, `hfet2/hfet2param.c` and `hfet2/hfet2mpar.c`.
//!
//! # Where it sits between the other two levels
//!
//! HFET2 is the smallest of the three lineage copies: 13 state slots, no
//! `#gate` node, no prime-prime pair, no `gatemod`, no `a1`/`a2` corrections
//! and no `cds` charge. Its load is the JFET-shaped one: `pnjlim` *and*
//! `fetlim` limiting (HFET1 has only the latter), a single-exponential gate
//! junction from `JS`/`GGR`, and a transient that folds the integration
//! companion into `ggs`/`ggd` directly (HFET1 keeps it in `ggspp`/`ggdpp`).
//! Like HFET1 it is two-polarity: every node difference is scaled by the type
//! and the loaded residual is scaled back.
//!
//! # C quirks reproduced deliberately
//!
//! - `HFET2temp` scales `vto` into `TVTO` but leaves `HFET2_VT1`/`VT2` on the
//!   *raw* threshold, and `hfeta2`'s `nsc` branch mixes the two
//!   (`exp((vgt + TVTO - VT2)/(ETA2*vt))`). Both are reproduced as written.
//! - `vcrit` is derived from the constant `1e-11` rather than a junction
//!   current (`hfet2temp.c`), so `pnjlim`'s critical voltage is fixed.
//! - `N01` is computed in C's temperature routine and read nowhere; the port
//!   does not carry it.
//! - The C identifiers for three matrix entries are mangled by a bad
//!   substitution (`HFET2sourcePriHFET2ourcePrimePtr`); they still map to
//!   `(sourcePrime, sourcePrime)`, `(sourcePrime, source)` and
//!   `(drainPrime, sourcePrime)` in `hfet2setup.c` and `hfet2load.c` alike,
//!   which is what the port stamps.
//! - `d2` and `eta2` gate the `nsc` branch through their *given* flags, which
//!   nothing ever clears, so `.sens` reports a topology jump for them unless
//!   the card gave both; `hfet2ask.c` has no `HFET2_M` case, so C reports no
//!   multiplier column.

use crate::devices::initial::{InstanceInitial, split};
use crate::devices::jfet::{BOLTZMANN, CHARGE, p};
use crate::devices::limiting;
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::sensitivity::{
    DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityParameter, SensitivityRecord,
};
use crate::devices::{
    AnalysisMode, Device, DeviceState, IterationPhase, LinearContext, ModelContext, ResolvedModel,
    StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// C `CONSTKoverQ`.
const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
const CELSIUS_TO_KELVIN: Real = 273.15;
/// `hfeta2`'s carrier-density cutoff.
const N_MIN: Real = 1e-38;
/// `hfet2temp.c`'s fixed critical-current constant for `vcrit`.
const VCRIT_CURRENT: Real = 1e-11;

/// The state slots of `HFET2load`'s `CKTstate0`, in the port's own order. Each
/// charge is followed by its derivative.
mod slot {
    pub(super) const VGS: usize = 0;
    pub(super) const VGD: usize = 1;
    pub(super) const CG: usize = 2;
    pub(super) const CD: usize = 3;
    pub(super) const CGD: usize = 4;
    pub(super) const GM: usize = 5;
    pub(super) const GDS: usize = 6;
    pub(super) const GGS: usize = 7;
    pub(super) const GGD: usize = 8;
    pub(super) const QGS: usize = 9;
    pub(super) const CQGS: usize = 10;
    pub(super) const QGD: usize = 11;
    pub(super) const CQGD: usize = 12;
    pub(super) const COUNT: usize = 13;
}

/// `HFET2mPTable` with `hfet2setup.c`'s defaults. The type-dependent defaults
/// (`vt0`, `eta`, `mu`, `vs`) are resolved by [`Model::resolve`], and `vt1`/
/// `vt2` have theirs in `HFET2temp`.
const MODEL: &[P] = &[
    p("vt0", U::Volt, D::Finite, None),
    p("vto", U::Volt, D::Finite, None),
    p("cf", U::Farad, D::NonNegative, Some(0.)),
    p("d1", U::Metre, D::Positive, Some(0.03e-6)),
    p("d2", U::Metre, D::Positive, Some(0.2e-6)),
    p("del", U::Dimensionless, D::NonNegative, Some(0.04)),
    p("delta", U::Dimensionless, D::Finite, Some(3.0)),
    p("deltad", U::Metre, D::NonNegative, Some(4.5e-9)),
    p("di", U::Metre, D::Positive, Some(0.04e-6)),
    p(
        "epsi",
        U::FaradPerMetre,
        D::Positive,
        Some(12.244 * 8.854_18e-12),
    ),
    p("eta", U::Dimensionless, D::Finite, None),
    p("eta1", U::Dimensionless, D::Finite, Some(2.)),
    p("eta2", U::Dimensionless, D::Finite, Some(2.)),
    p("gamma", U::Dimensionless, D::Finite, Some(3.0)),
    p("ggr", U::SiemensPerMetre, D::NonNegative, Some(0.)),
    p("js", U::AmperePerSquareMetre, D::NonNegative, Some(0.)),
    p("klambda", U::Dimensionless, D::Finite, Some(0.)),
    p("kmu", U::Dimensionless, D::Finite, Some(0.)),
    p("knmax", U::Dimensionless, D::Finite, Some(0.)),
    p("kvto", U::Dimensionless, D::Finite, Some(0.)),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.15)),
    p("m", U::Dimensionless, D::Finite, Some(3.0)),
    p("mc", U::Dimensionless, D::Finite, Some(3.0)),
    p("mu", U::SquareCentimetrePerVoltSecond, D::NonNegative, None),
    p("n", U::Dimensionless, D::Finite, Some(5.0)),
    p("nmax", U::PerCubicCentimetre, D::NonNegative, Some(2e16)),
    p("p", U::Dimensionless, D::Finite, Some(1.)),
    p("rd", U::Ohm, D::NonNegative, Some(0.)),
    p("rs", U::Ohm, D::NonNegative, Some(0.)),
    p("rdi", U::Ohm, D::NonNegative, Some(0.)),
    p("rsi", U::Ohm, D::NonNegative, Some(0.)),
    p("sigma0", U::Dimensionless, D::Finite, Some(0.057)),
    p("vs", U::MetrePerSecond, D::Positive, None),
    p("vsigma", U::Volt, D::Positive, Some(0.1)),
    p("vsigmat", U::Volt, D::Finite, Some(0.3)),
    p("vt1", U::Volt, D::Finite, None),
    p("vt2", U::Volt, D::Finite, None),
    p("level", U::Dimensionless, D::Finite, Some(6.)),
];

/// `HFET2pTable`'s scalar instance setters (`hfet2param.c`).
const INSTANCE: &[P] = &[
    p("l", U::Metre, D::Positive, Some(1e-6)),
    p("w", U::Metre, D::Positive, Some(20e-6)),
    p("m", U::Dimensionless, D::Positive, Some(1.)),
    p("temp", U::Celsius, D::Temperature, None),
    p("dtemp", U::Dimensionless, D::Finite, Some(0.)),
];

/// The `IC` vector components (`hfet2param.c`): `ICVDS`, then `ICVGS`.
pub(super) const IC_COMPONENTS: [&str; 2] = ["icvds", "icvgs"];

/// The validated model card.
#[derive(Debug, Clone, Copy)]
pub(super) struct Model {
    /// `+1` for N, `-1` for P; `hfet2temp.c` stores `TYPE*VTO` in `TVTO`.
    pol: Real,
    threshold: Real,
    cf: Real,
    d1: Real,
    d2: Real,
    del: Real,
    delta: Real,
    deltad: Real,
    di: Real,
    epsi: Real,
    eta: Real,
    eta1: Real,
    eta2: Real,
    gamma: Real,
    ggr: Real,
    js: Real,
    klambda: Real,
    kmu: Real,
    knmax: Real,
    kvto: Real,
    lambda: Real,
    m: Real,
    mc: Real,
    mu: Real,
    n: Real,
    nmax: Real,
    pm: Real,
    rd: Real,
    rs: Real,
    rdi: Real,
    rsi: Real,
    sigma0: Real,
    vs: Real,
    vsigma: Real,
    vsigmat: Real,
    vt1: Option<Real>,
    vt2: Option<Real>,
    /// `hfet2setup.c`'s `Given` flags that `hfeta2` reads.
    eta2_given: bool,
    d2_given: bool,
}

impl Model {
    /// `HFET2setup`'s defaults, `hfet2mpar.c`'s setters and `inpdomod.c`'s
    /// level check. A `phfet` base (or the `phfet` flag) makes it p-type.
    pub(super) fn resolve<'a>(
        resolved: &'a ResolvedModel<'_>,
    ) -> SpiceResult<(Self, Vec<&'a crate::netlist::ast::ParameterAssignment>)> {
        let (pol, scalars) = crate::devices::jfet::typed_scalars(
            resolved,
            resolved.family() == crate::devices::ModelFamily::Pmf,
            ["nhfet", "phfet"],
        );
        // `inpgmod.c::create_model`: a setter outside the model table that is
        // a settable instance parameter becomes a model-level instance
        // default, applied before the instance card's own setters.
        let (scalars, defaults) = crate::devices::schema::split_model_setters(
            scalars,
            &ScalarSchema { parameters: MODEL },
            &ScalarSchema {
                parameters: INSTANCE,
            },
        );
        let values =
            ScalarSchema { parameters: MODEL }.validate(scalars, &resolved.card().location)?;
        let r = |name: &str| crate::devices::jfet::required(&values, name);
        let alias = |names: &[&str]| crate::devices::jfet::last_setter(&values, names);
        let typed = |explicit: Option<Real>, n: Real, p: Real| {
            explicit.unwrap_or(if pol > 0. { n } else { p })
        };
        let model = Self {
            pol,
            threshold: typed(alias(&["vt0", "vto"]), 0.15, -0.15),
            cf: r("cf")?,
            d1: r("d1")?,
            d2: r("d2")?,
            del: r("del")?,
            delta: r("delta")?,
            deltad: r("deltad")?,
            di: r("di")?,
            epsi: r("epsi")?,
            eta: typed(values.get("eta").map(|v| v.value), 1.28, 1.4),
            eta1: r("eta1")?,
            eta2: r("eta2")?,
            gamma: r("gamma")?,
            ggr: r("ggr")?,
            js: r("js")?,
            klambda: r("klambda")?,
            kmu: r("kmu")?,
            knmax: r("knmax")?,
            kvto: r("kvto")?,
            lambda: r("lambda")?,
            m: r("m")?,
            mc: r("mc")?,
            mu: typed(values.get("mu").map(|v| v.value), 0.4, 0.03),
            n: r("n")?,
            nmax: r("nmax")?,
            pm: r("p")?,
            rd: r("rd")?,
            rs: r("rs")?,
            rdi: r("rdi")?,
            rsi: r("rsi")?,
            sigma0: r("sigma0")?,
            vs: typed(values.get("vs").map(|v| v.value), 1.5e5, 0.8e5),
            vsigma: r("vsigma")?,
            vsigmat: r("vsigmat")?,
            vt1: values.get("vt1").map(|v| v.value),
            vt2: values.get("vt2").map(|v| v.value),
            eta2_given: values.get("eta2").is_some_and(|v| v.location.is_some()),
            d2_given: values.get("d2").is_some_and(|v| v.location.is_some()),
        };
        if model.vsigma == 0. || model.di + model.deltad == 0. {
            return Err(SpiceError::circuit(format!(
                "{}: HFET2 model '{}' needs VSIGMA and DI nonzero (hfeta2 divides by them)",
                resolved.card().location,
                resolved.card().name
            )));
        }
        Ok((model, defaults))
    }
}

/// Validated instance setters (`hfet2param.c`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Instance {
    pub(super) l: Real,
    pub(super) w: Real,
    pub(super) m: Real,
    /// Celsius as written (`hfet2param.c` adds 273.15); `None` is
    /// `CKTtemp + dtemp`, used as kelvin.
    pub(super) temp: Option<Real>,
    pub(super) dtemp: Real,
}

impl Instance {
    pub(super) fn resolve(
        i: &DeviceInstance,
    ) -> SpiceResult<(
        InstanceInitial,
        Vec<crate::netlist::ast::ParameterAssignment>,
    )> {
        split(&i.parameters, &IC_COMPONENTS)
    }

    pub(super) fn validate(
        defaults: &[&crate::netlist::ast::ParameterAssignment],
        setters: &[crate::netlist::ast::ParameterAssignment],
        owner: &crate::primitives::SourceLoc,
    ) -> SpiceResult<Self> {
        let values = crate::devices::schema::with_defaults(
            &ScalarSchema {
                parameters: INSTANCE,
            },
            defaults,
            setters.iter(),
            owner,
        )?;
        Ok(Self {
            l: crate::devices::jfet::required(&values, "l")?,
            w: crate::devices::jfet::required(&values, "w")?,
            m: crate::devices::jfet::required(&values, "m")?,
            temp: values.get("temp").map(|v| v.value),
            dtemp: crate::devices::jfet::required(&values, "dtemp")?,
        })
    }
}

/// `hfet2temp.c`'s per-instance quantities.
#[derive(Debug, Clone, Copy)]
struct Derived {
    temp: Real,
    vt: Real,
    tlambda: Real,
    tmu: Real,
    tnmax: Real,
    tvto: Real,
    /// `HFET2_VT1`: the *raw* threshold plus the channel charge term.
    vt1: Real,
    /// `VT2`: the raw threshold.
    vt2: Real,
    n0: Real,
    n02: Real,
    two_exponentials: bool,
    jslw: Real,
    ggrlw: Real,
    gchi0: Real,
    imax: Real,
    vcrit: Real,
    delta_sqr: Real,
    gdpr: Real,
    gspr: Real,
}

/// `hfeta2`'s evaluation: the channel and the two gate capacitances.
#[derive(Debug, Clone, Copy)]
struct Channel {
    cdrain: Real,
    gm: Real,
    gds: Real,
    capgs: Real,
    capgd: Real,
}

/// `hfet2setup.c`'s internal nodes.
#[derive(Debug, Clone, Copy)]
struct Nodes {
    d: NodeId,
    g: NodeId,
    s: NodeId,
    sp: NodeId,
    dp: NodeId,
}

/// HFET2, `Z` level 6.
#[derive(Debug, Clone)]
pub(crate) struct Hfet2 {
    name: String,
    model_name: String,
    terminals: Vec<NodeId>,
    nodes: Nodes,
    model: Model,
    instance: Instance,
    initial: InstanceInitial,
}

impl Hfet2 {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 6 {
            return Err(SpiceError::circuit(format!(
                "{}: HFET2 needs level 6",
                i.name
            )));
        }
        if i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: HFET2 needs three terminals",
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
        };
        // `hfet2setup.c` creates `#source` for RS and `#drain` for RD, in that
        // order, and falls back to the external node when a resistance is
        // zero. There is no gate or prime-prime node at this level.
        let mut create = |resistance: Real, suffix: &str, target: &mut NodeId| -> SpiceResult<()> {
            if resistance == 0. {
                return Ok(());
            }
            let name = format!("{}#{suffix}", i.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "{}: HFET2 internal-node name collision: {name}",
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
        device.derived(context.temperature)?;
        Ok(Box::new(device))
    }

    /// `hfet2temp.c`. `temperature` is Celsius (`CKTtemp`), while `TEMP` is
    /// kelvin.
    fn derived(&self, temperature: Real) -> SpiceResult<Derived> {
        let model = &self.model;
        let instance = &self.instance;
        let temp = match instance.temp {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + instance.dtemp,
        };
        let nominal = temperature + CELSIUS_TO_KELVIN;
        let delta = temp - nominal;
        let vt = K_OVER_Q * temp;
        let area = instance.w * instance.l;
        let derived = Derived {
            temp,
            vt,
            tlambda: model.lambda + model.klambda * delta,
            tmu: model.mu - model.kmu * delta,
            tnmax: model.nmax - model.knmax * delta,
            tvto: model.pol * model.threshold - model.kvto * delta,
            // C leaves `HFET2_VT1`/`VT2` on the raw threshold, unlike `TVTO`.
            vt1: model
                .vt1
                .unwrap_or(model.threshold + CHARGE * model.nmax * model.di / model.epsi),
            vt2: model.vt2.unwrap_or(model.threshold),
            n0: model.epsi * model.eta * vt / 2. / CHARGE / (model.di + model.deltad),
            n02: if model.eta2_given {
                model.epsi * model.eta2 * vt / 2. / CHARGE / model.d2
            } else {
                0.
            },
            two_exponentials: model.eta2_given && model.d2_given,
            jslw: model.js * instance.l * instance.w / 2.,
            ggrlw: model.ggr * instance.l * instance.w / 2.,
            gchi0: CHARGE * instance.w * (model.mu - model.kmu * delta) / instance.l,
            imax: CHARGE * (model.nmax - model.knmax * delta) * model.vs * instance.w,
            vcrit: vt * (vt / (std::f64::consts::SQRT_2 * VCRIT_CURRENT)).ln(),
            delta_sqr: model.delta * model.delta,
            gdpr: if model.rd == 0. { 0. } else { 1. / model.rd },
            gspr: if model.rs == 0. { 0. } else { 1. / model.rs },
        };
        let _ = area;
        if [
            derived.temp,
            derived.vt,
            derived.tlambda,
            derived.tmu,
            derived.tnmax,
            derived.tvto,
            derived.vt1,
            derived.vt2,
            derived.n0,
            derived.gchi0,
            derived.imax,
            derived.vcrit,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || derived.vt == 0.
        {
            return Err(SpiceError::circuit(format!(
                "{}: HFET2 temperature derivation is out of range (check DI, DELTAD, N, \
                 NMU and the instance temperature)",
                self.name
            )));
        }
        Ok(derived)
    }

    /// `hfeta2()`: the channel, its derivatives and the two gate
    /// capacitances. `vgs` is C's `vds > 0 ? vgs : vgd` argument; `vds` is the
    /// normalized (nonnegative) drain-source voltage.
    fn hfeta2(&self, derived: &Derived, vgs: Real, vds: Real) -> SpiceResult<Channel> {
        let model = &self.model;
        let vt = derived.vt;
        let etavth = model.eta * vt;
        let vl = model.vs / derived.tmu * self.instance.l;
        let rt = model.rsi + model.rdi;
        let vgt0 = vgs - derived.tvto;
        let s = ((vgt0 - model.vsigmat) / model.vsigma).exp();
        let sigma = model.sigma0 / (1. + s);
        let vgt = vgt0 + sigma * vds;
        let u = 0.5 * vgt / vt - 1.;
        let t = (derived.delta_sqr + u * u).sqrt();
        let vgte = vt * (2. + u + t);
        let b = (vgt / etavth).exp();
        let nsn = 2. * derived.n0 * (1. + 0.5 * b).ln();
        let nsm = if derived.two_exponentials {
            let nsc = derived.n02 * ((vgt + derived.tvto - derived.vt2) / (model.eta2 * vt)).exp();
            nsn * nsc / (nsn + nsc)
        } else {
            nsn
        };
        if nsm < N_MIN {
            return Ok(Channel {
                cdrain: 0.,
                gm: 0.,
                gds: 0.,
                capgs: model.cf,
                capgd: model.cf,
            });
        }
        let c = (nsm / derived.tnmax).powf(model.gamma);
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
        let d = (vds / vsate).powf(model.m);
        let e = (1. + d).powf(1. / model.m);
        let delidgch = vds * (1. + derived.tlambda * vds) / e;
        let cdrain = gch * delidgch;
        let delidvsate = cdrain * d / vsate / (1. + d);
        let delidvds = gch * (1. + 2. * derived.tlambda * vds) / e
            - cdrain * (vds / vsate).powf(model.m - 1.) / (vsate * (1. + d));
        let a = 1. + gchi * rt;
        let delgchgchi = 1. / (a * a);
        let delgchins = derived.gchi0;
        let delnsnsm = ns / nsm * (1. - c / (1. + c));
        let delvgtevgt = 0.5 * (1. + u / t);
        let mut delnsmvgt = derived.n0 / etavth / (1. / b + 0.5);
        if derived.two_exponentials {
            let nsc = derived.n02 * ((vgt + derived.tvto - derived.vt2) / (model.eta2 * vt)).exp();
            delnsmvgt = nsc * (nsc * delnsmvgt + nsn * nsn / (model.eta2 * vt))
                / ((nsc + nsn) * (nsc + nsn));
        }
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
        // Capacitances.
        let temp = model.eta1 * vt;
        let cg1 = 1. / (model.d1 / model.epsi + temp * (-(vgs - derived.vt1) / temp).exp());
        let cgc =
            self.instance.w * self.instance.l * (CHARGE * delnsnsm * delnsmvgt * delvgtvgs + cg1);
        let vdse = vds * (1. + (vds / vsate).powf(model.mc)).powf(-1. / model.mc);
        let a = (vsate - vdse) / (2. * vsate - vdse);
        let a = a * a;
        let third = 2.0 / 3.0;
        let p = model.pm + (1. - model.pm) * (-vds / vsate).exp();
        let capgs = model.cf + 2. * third * cgc * (1. - a) / (1. + p);
        let a = vsate / (2. * vsate - vdse);
        let a = a * a;
        let capgd = model.cf + 2. * p * third * cgc * (1. - a) / (1. + p);
        let values = [cdrain, gm, gds, capgs, capgd];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::circuit(format!(
                "{}: nonfinite hfeta2 channel equations",
                self.name
            )));
        }
        Ok(Channel {
            cdrain,
            gm,
            gds,
            capgs,
            capgd,
        })
    }
}

/// `HFET2load`'s evaluated device quantities at one bias.
#[derive(Debug, Clone, Copy)]
struct Point {
    vgs: Real,
    vgd: Real,
    vds: Real,
    cg: Real,
    cd: Real,
    cgd: Real,
    gm: Real,
    gds: Real,
    ggs: Real,
    ggd: Real,
    capgs: Real,
    capgd: Real,
}

/// C's initialization and limiting. `HFET2load` limits with `DEVpnjlim` *and*
/// `DEVfetlim`, so `icheck` is live (unlike HFET1, which has no junction
/// limiting).
#[derive(Debug, Clone, Copy)]
enum Start {
    Conditions,
    Exact,
    Fixed,
    HeldOff {
        raw: [Real; 2],
    },
    Limited {
        icheck: bool,
        cghat: Real,
        cdhat: Real,
    },
}

impl Hfet2 {
    /// `HFET2load`'s initialization and limiting. `raw` and `ic` are already
    /// type-normalized.
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
        // C's `icheck = 1` then `pnjlim`/`fetlim`; `fetlim` cannot set it.
        let gs = limiting::pnjlim(vgs, vgs0, K_OVER_Q * derived.temp, derived.vcrit);
        let gd = limiting::pnjlim(vgd, vgd0, K_OVER_Q * derived.temp, derived.vcrit);
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

    /// `HFET2load`'s device evaluation: the single-exponential gate junctions
    /// and the `hfeta2` channel.
    fn point(&self, derived: &Derived, vgs: Real, vgd: Real) -> SpiceResult<Point> {
        let model = &self.model;
        let vt = derived.vt;
        let vtn = model.n * vt;
        let arg = -vgs * model.del / vt;
        let earg = arg.exp();
        let expe = (vgs / vtn).exp();
        let ggs = derived.jslw * expe / vtn + derived.ggrlw * earg * (1. - arg);
        let mut cg = derived.jslw * (expe - 1.) + derived.ggrlw * vgs * earg;
        let arg = -vgd * model.del / vt;
        let earg = arg.exp();
        let expe = (vgd / vtn).exp();
        let ggd = derived.jslw * expe / vtn + derived.ggrlw * earg * (1. - arg);
        let cgd = derived.jslw * (expe - 1.) + derived.ggrlw * vgd * earg;
        cg += cgd;
        let mut vds = vgs - vgd;
        let inverse = vds < 0.;
        if inverse {
            vds = -vds;
        }
        // C's argument is `vds > 0 ? vgs : vgd` after the normalization.
        let channel = self.hfeta2(derived, if vds > 0. { vgs } else { vgd }, vds)?;
        let (cdrain, capgs, capgd) = if inverse {
            (-channel.cdrain, channel.capgd, channel.capgs)
        } else {
            (channel.cdrain, channel.capgs, channel.capgd)
        };
        Ok(Point {
            vgs,
            vgd,
            vds: vgs - vgd,
            cg,
            cd: cdrain - cgd,
            cgd,
            gm: channel.gm,
            gds: channel.gds,
            ggs,
            ggd,
            capgs,
            capgd,
        })
    }

    /// The total instance temperature in kelvin.
    fn temperature(&self, temperature: Real) -> Real {
        match self.instance.temp {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + self.instance.dtemp,
        }
    }
}

impl Device for Hfet2 {
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
    /// `hfet2trunc.c`: both gate charges control the timestep.
    fn truncation_slots(&self) -> Vec<usize> {
        vec![slot::QGS, slot::QGD]
    }
    /// Noiseless: `hfet2init.c` leaves `DEVnoise = NULL`.
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }
    /// Linear in `.disto`: `hfet2init.c` leaves `DEVdisto = NULL`.
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(crate::devices::distortion::DeviceDistortion::Linear)
    }
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        Ok(match key {
            "l" => Some(self.instance.l),
            "w" => Some(self.instance.w * self.instance.m),
            "m" => Some(self.instance.m),
            "icvds" => self.initial.values[0],
            "icvgs" => self.initial.values[1],
            "temp" => Some(self.temperature(context.temperature) - CELSIUS_TO_KELVIN),
            "dtemp" => Some(self.instance.dtemp),
            _ => None,
        })
    }
    /// `hfet2ask.c`'s bias quantities, `m`-scaled as C stores them.
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
        if matches!(key, "qgs" | "cqgs" | "qgd" | "cqgd") {
            return Err(SpiceError::not_yet_ported(
                format!("{}: HFET2 @{}[{key}]", self.name, self.name),
                "src/spicelib/devices/hfet2/hfet2ask.c (HFET2qgs reads CKTstate0)",
            ));
        }
        if !keys.contains(&key) {
            return Ok(None);
        }
        if transient {
            return Err(SpiceError::not_yet_ported(
                format!("{}: transient @{}[{key}]", self.name, self.name),
                "src/spicelib/devices/hfet2/hfet2load.c (charge companion)",
            ));
        }
        let derived = self.derived(context.temperature)?;
        let pol = self.model.pol;
        let vgs = pol * (voltage(self.nodes.g) - voltage(self.nodes.sp));
        let vgd = pol * (voltage(self.nodes.g) - voltage(self.nodes.dp));
        let point = self.point(&derived, vgs, vgd)?;
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
        ["l", "w", "m", "temp", "dtemp"]
            .into_iter()
            .find(|name| name.eq_ignore_ascii_case(keyword))
    }
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let check = |ok: bool, what: &str| -> SpiceResult<()> {
            if ok {
                Ok(())
            } else {
                Err(SpiceError::circuit(format!(
                    "{}: HFET2 {} must be {what}, got {value}",
                    self.name, parameter
                )))
            }
        };
        let mut instance = self.instance;
        match parameter {
            "l" | "w" | "m" => {
                check(value > 0., "positive")?;
                match parameter {
                    "l" => instance.l = value,
                    "w" => instance.w = value,
                    _ => instance.m = value,
                }
            }
            "temp" => {
                check(value.is_finite(), "finite")?;
                instance.temp = Some(value);
            }
            "dtemp" => {
                check(value.is_finite(), "finite")?;
                instance.dtemp = value;
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "{}: HFET2 cannot sweep {parameter}",
                    self.name
                )));
            }
        }
        let device = Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            terminals: self.terminals.clone(),
            nodes: self.nodes,
            model: self.model,
            instance,
            initial: self.initial.clone(),
        };
        device.derived(context.temperature)?;
        Ok(Box::new(device))
    }

    /// `HFET2load`: the equations, their order and the stamps are C's.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit(format!(
                "{}: HFET2 AC needs small-signal assembly",
                self.name
            )));
        }
        let derived = self.derived(context.temperature)?;
        let m = self.instance.m;
        let pol = self.model.pol;
        let voltage = |n: NodeId| context.node_voltage(n);
        let raw = [
            pol * (voltage(self.nodes.g) - voltage(self.nodes.sp)),
            pol * (voltage(self.nodes.g) - voltage(self.nodes.dp)),
        ];
        let ic = [
            self.initial.values[0].unwrap_or_else(|| voltage(self.nodes.d) - voltage(self.nodes.s)),
            self.initial.values[1].unwrap_or_else(|| voltage(self.nodes.g) - voltage(self.nodes.s)),
        ];
        let ([vgs, vgd], start) = self.select_junctions(&derived, &context.states, raw, ic);
        let initial_transient =
            context.states.initial_transient() && context.states.phase() == IterationPhase::Predict;
        let mut point = self.point(&derived, vgs, vgd)?;
        let (mut cg, mut cd, mut ggs, mut ggd) = (point.cg, point.cd, point.ggs, point.ggd);
        let gds = point.gds;
        let mut cgd = point.cgd;
        let (mut cgspp, mut cgdpp) = (0., 0.);
        let (mut qgs, mut qgd) = (0., 0.);
        if matches!(
            context.mode,
            AnalysisMode::Transient { .. } | AnalysisMode::DcSweep
        ) {
            let vgs1 = context
                .states
                .accepted(1, slot::VGS)
                .filter(|v| v.is_finite())
                .unwrap_or(vgs);
            let vgd1 = context
                .states
                .accepted(1, slot::VGD)
                .filter(|v| v.is_finite())
                .unwrap_or(vgd);
            if initial_transient {
                context.states.patch_state1(slot::QGS, point.capgs * vgs)?;
                context.states.patch_state1(slot::QGD, point.capgd * vgd)?;
            }
            qgs = context
                .states
                .accepted(1, slot::QGS)
                .filter(|v| v.is_finite())
                .unwrap_or(0.)
                + point.capgs * (vgs - vgs1);
            qgd = context
                .states
                .accepted(1, slot::QGD)
                .filter(|v| v.is_finite())
                .unwrap_or(0.)
                + point.capgd * (vgd - vgd1);
            if initial_transient {
                context.states.patch_state1(slot::QGS, qgs)?;
                context.states.patch_state1(slot::QGD, qgd)?;
            }
            if let Some(coefficients) = context.integration {
                let AnalysisMode::Transient { dt, .. } = context.mode else {
                    return Err(SpiceError::circuit(
                        "HFET2 charge companion outside transient",
                    ));
                };
                if dt != coefficients.dt() {
                    return Err(SpiceError::circuit("HFET2 charge timestep mismatch"));
                }
                let junction = |charge, capacitance| JunctionPoint {
                    current: 0.,
                    conductance: 0.,
                    charge,
                    capacitance,
                };
                // `HFET2load` folds the companions into `ggs`/`ggd` directly.
                let (geq, cqgs) = junction_companion(
                    context,
                    coefficients,
                    junction(qgs, point.capgs),
                    slot::QGS,
                )?;
                ggs += geq;
                cgspp = cqgs;
                cg += cqgs;
                let (geq, cqgd) = junction_companion(
                    context,
                    coefficients,
                    junction(qgd, point.capgd),
                    slot::QGD,
                )?;
                ggd += geq;
                cgdpp = cqgd;
                cg += cqgd;
                cd -= cqgd;
                cgd += cqgd;
                if initial_transient {
                    context.states.patch_state1(slot::CQGS, cqgs)?;
                    context.states.patch_state1(slot::CQGD, cqgd)?;
                }
            }
        }
        point = Point {
            cg,
            cd,
            cgd,
            ggs,
            ggd,
            gds,
            ..point
        };
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
            (slot::VGS, point.vgs),
            (slot::VGD, point.vgd),
            (slot::CG, point.cg),
            (slot::CD, point.cd),
            (slot::CGD, point.cgd),
            (slot::GM, point.gm),
            (slot::GDS, point.gds),
            (slot::GGS, point.ggs),
            (slot::GGD, point.ggd),
            (slot::QGS, qgs),
            (slot::CQGS, cgspp),
            (slot::QGD, qgd),
            (slot::CQGD, cgdpp),
        ] {
            context.states.set(index, value)?;
        }
        let n = self.nodes;
        let (ggs, ggd) = (point.ggs, point.ggd);
        let (gm, gds, cgd, cg) = (point.gm, point.gds, point.cgd, point.cg);
        let (gdpr, gspr) = (derived.gdpr, derived.gspr);
        let ceqgd = pol * (cgd - ggd * point.vgd);
        let ceqgs = pol * ((cg - cgd) - ggs * point.vgs);
        let cdreq = pol * ((point.cd + cgd) - gds * point.vds - gm * point.vgs);
        context.stamp_rhs(n.g, m * (-ceqgs - ceqgd))?;
        context.stamp_rhs(n.dp, m * (-cdreq + ceqgd))?;
        context.stamp_rhs(n.sp, m * (cdreq + ceqgs))?;
        for (row, col, value) in [
            (n.d, n.dp, -m * gdpr),
            (n.g, n.dp, -m * ggd),
            (n.g, n.sp, -m * ggs),
            (n.s, n.sp, -m * gspr),
            (n.dp, n.d, -m * gdpr),
            (n.dp, n.g, m * (gm - ggd)),
            (n.dp, n.sp, m * (-gds - gm)),
            (n.sp, n.g, m * (-ggs - gm)),
            (n.sp, n.s, -m * gspr),
            (n.sp, n.dp, -m * gds),
            (n.d, n.d, m * gdpr),
            (n.g, n.g, m * (ggd + ggs)),
            (n.s, n.s, m * gspr),
            (n.dp, n.dp, m * (gdpr + gds + ggd)),
            (n.sp, n.sp, m * (gspr + gds + gm + ggs)),
        ] {
            context.stamp(row, col, value)?;
        }
        Ok(())
    }

    /// `hfet2.c`'s `IFparm` tables under C's `sgen` walk (#164): the record
    /// replay of `hfet2temp.c`/`hfet2mpar.c`/`hfet2param.c` with the load being
    /// this device rebuilt from those records.
    ///
    /// Three C behaviours the replay reproduces, all verified against the
    /// reference binary:
    ///
    /// - `d2` and `eta2` gate the `nsc` topology branch through their *given*
    ///   flags (see [`MODEL_PARAMETERS`]), so their columns are honest
    ///   derivatives only when the card gave both, and given-flag jumps
    ///   otherwise.
    /// - `hfet2ask.c` answers the instance temperature in Celsius while
    ///   `hfet2param.c` stores it in kelvin.
    /// - `HFET2_VT1`/`VT2` are model fields that follow the raw `VTO` (not the
    ///   type-scaled `TVTO`) whenever they are not given.
    fn sensitivity(&self, context: &ModelContext) -> SpiceResult<Box<dyn DeviceSensitivity + '_>> {
        Ok(Box::new(Hfet2Sensitivity::new(self, context)))
    }

    /// `hfet2acl.c`: the bias conductances plus `j omega` times the gate
    /// capacitances.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let derived = self.derived(context.model_context.temperature)?;
        let row = |node: NodeId| context.unknowns.node_row(node);
        let voltage = |node: NodeId| row(node).and_then(|r| bias.get(r)).unwrap_or(0.);
        let pol = self.model.pol;
        let vgs = pol * (voltage(self.nodes.g) - voltage(self.nodes.sp));
        let vgd = pol * (voltage(self.nodes.g) - voltage(self.nodes.dp));
        let m = self.instance.m;
        let point = self.point(&derived, vgs, vgd)?;
        let (gm, gds, ggs, ggd) = (point.gm, point.gds, point.ggs, point.ggd);
        let (gdpr, gspr) = (derived.gdpr, derived.gspr);
        let n = self.nodes;
        for (a, b, value) in [
            (n.d, n.d, m * gdpr),
            (n.g, n.g, m * (ggd + ggs)),
            (n.s, n.s, m * gspr),
            (n.dp, n.dp, m * (gdpr + gds + ggd)),
            (n.sp, n.sp, m * (gspr + gds + gm + ggs)),
            (n.d, n.dp, -m * gdpr),
            (n.g, n.dp, -m * ggd),
            (n.g, n.sp, -m * ggs),
            (n.s, n.sp, -m * gspr),
            (n.dp, n.d, -m * gdpr),
            (n.dp, n.g, m * (gm - ggd)),
            (n.dp, n.sp, m * (-gds - gm)),
            (n.sp, n.g, m * (-ggs - gm)),
            (n.sp, n.s, -m * gspr),
            (n.sp, n.dp, -m * gds),
        ] {
            if let (Some(r), Some(c)) = (row(a), row(b)) {
                context.system.a.add(r, c, value)?;
            }
        }
        context.nodal([n.g, n.sp], m * point.capgs, true)?;
        context.nodal([n.g, n.dp], m * point.capgd, true)?;
        Ok(())
    }
    /// `hfet2pzl.c` is `hfet2acl.c` with `s` in place of `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
}

// ---- `.sens` (#164) -------------------------------------------------------

/// `hfet2.c`'s `HFET2mPTable`, in C's order, filtered the way `set_param()`
/// (`cktsgen.c`) and `sens_getp()` filter it: `type` is `IF_STRING`,
/// `nhfet`/`phfet` are `IF_FLAG` and `vto` is the `IOPR` alias of `vt0`, so
/// `(IF_SET|IF_ASK|IF_REAL)` drops all four.
///
/// `d2` and `eta2` are ordinary entries here, but C's setters mark them
/// *given*, and that flag is a topology switch rather than a value:
/// `hfet2temp.c` fills `N02` under `eta2Given` alone, while `hfet2load.c` reads
/// the `nsc` branch under `eta2Given && d2Given`. The walk never clears a
/// given flag, so perturbing either of them can change which model the
/// perturbed load evaluates. See [`Hfet2Sensitivity::load`].
const MODEL_PARAMETERS: &[SensitivityParameter] = &[
    SensitivityParameter::dc("cf"),
    SensitivityParameter::dc("d1"),
    SensitivityParameter::dc("d2"),
    SensitivityParameter::dc("del"),
    SensitivityParameter::dc("delta"),
    SensitivityParameter::dc("deltad"),
    SensitivityParameter::dc("di"),
    SensitivityParameter::dc("epsi"),
    SensitivityParameter::dc("eta"),
    SensitivityParameter::dc("eta1"),
    SensitivityParameter::dc("eta2"),
    SensitivityParameter::dc("gamma"),
    SensitivityParameter::dc("ggr"),
    SensitivityParameter::dc("js"),
    SensitivityParameter::dc("klambda"),
    SensitivityParameter::dc("kmu"),
    SensitivityParameter::dc("knmax"),
    SensitivityParameter::dc("kvto"),
    SensitivityParameter::dc("lambda"),
    SensitivityParameter::dc("m"),
    SensitivityParameter::dc("mc"),
    SensitivityParameter::dc("mu"),
    SensitivityParameter::dc("n"),
    SensitivityParameter::dc("nmax"),
    SensitivityParameter::dc("p"),
    SensitivityParameter::dc("rd"),
    SensitivityParameter::dc("rdi"),
    SensitivityParameter::dc("rs"),
    SensitivityParameter::dc("rsi"),
    SensitivityParameter::dc("sigma0"),
    SensitivityParameter::dc("vs"),
    SensitivityParameter::dc("vsigma"),
    SensitivityParameter::dc("vsigmat"),
    SensitivityParameter::dc("vt0"),
    SensitivityParameter::dc("vt1"),
    SensitivityParameter::dc("vt2"),
];

/// `hfet2.c`'s `HFET2pTable` under the same filter: `off` is `IF_FLAG` and the
/// probe parameters are `OP` (ask only).
///
/// `m` passes the table filter but not the ask: `hfet2ask.c` has no
/// `HFET2_M` case (`hfetask.c` does have `HFETA_M`), so `sgen` drops the column
/// C never reports.
const INSTANCE_PARAMETERS: &[SensitivityParameter] = &[
    SensitivityParameter::dc("l"),
    SensitivityParameter::dc("w"),
    SensitivityParameter::dc("icvds"),
    SensitivityParameter::dc("icvgs"),
    SensitivityParameter::dc("temp"),
    SensitivityParameter::dc("dtemp"),
];

/// `hfet2temp.c`/`hfet2mpar.c`/`hfet2setup.c` replayed on C's records, with
/// the load being the port's own [`Hfet2`] rebuilt from them.
#[derive(Debug)]
pub(super) struct Hfet2Sensitivity<'a> {
    device: &'a Hfet2,
    circuit_kelvin: Real,
}

impl<'a> Hfet2Sensitivity<'a> {
    pub(super) fn new(device: &'a Hfet2, context: &ModelContext) -> Self {
        Self {
            device,
            circuit_kelvin: context.temperature + CELSIUS_TO_KELVIN,
        }
    }
}

impl DeviceSensitivity for Hfet2Sensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        SensitivityFamily::Hfet2
    }

    fn model(&self) -> Option<&str> {
        Some(&self.device.model_name)
    }

    fn model_parameters(&self) -> &'static [SensitivityParameter] {
        MODEL_PARAMETERS
    }

    fn instance_parameters(&self) -> &'static [SensitivityParameter] {
        INSTANCE_PARAMETERS
    }

    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)> {
        let (m, i) = (&self.device.model, &self.device.instance);
        let mut model = SensitivityRecord::new();
        for (keyword, value, given) in [
            ("cf", m.cf, true),
            ("d1", m.d1, true),
            ("d2", m.d2, m.d2_given),
            ("del", m.del, true),
            ("delta", m.delta, true),
            ("deltad", m.deltad, true),
            ("di", m.di, true),
            ("epsi", m.epsi, true),
            ("eta", m.eta, true),
            ("eta1", m.eta1, true),
            ("eta2", m.eta2, m.eta2_given),
            ("gamma", m.gamma, true),
            ("ggr", m.ggr, true),
            ("js", m.js, true),
            ("klambda", m.klambda, true),
            ("kmu", m.kmu, true),
            ("knmax", m.knmax, true),
            ("kvto", m.kvto, true),
            ("lambda", m.lambda, true),
            ("m", m.m, true),
            ("mc", m.mc, true),
            ("mu", m.mu, true),
            ("n", m.n, true),
            ("nmax", m.nmax, true),
            ("p", m.pm, true),
            ("rd", m.rd, true),
            ("rdi", m.rdi, true),
            ("rs", m.rs, true),
            ("rsi", m.rsi, true),
            ("sigma0", m.sigma0, true),
            ("vs", m.vs, true),
            ("vsigma", m.vsigma, true),
            ("vsigmat", m.vsigmat, true),
            // `vt0` is C's `VTO`: raw, never scaled by the type in place
            // (`hfet2temp.c` writes `TVTO` instead). `vt1`/`vt2` are the model
            // fields `hfet2setup.c` zeroes when not given, and `temperature`
            // below fills them the way `HFET2temp` does.
            ("vt0", m.threshold, true),
            ("vt1", m.vt1.unwrap_or(0.), m.vt1.is_some()),
            ("vt2", m.vt2.unwrap_or(0.), m.vt2.is_some()),
        ] {
            model = model.with(keyword, value, given);
        }
        let mut instance = SensitivityRecord::new();
        for (keyword, value, given) in [
            ("l", i.l, true),
            ("w", i.w, true),
            ("m", i.m, true),
            (
                "icvds",
                self.device.initial.values[0].unwrap_or(0.),
                self.device.initial.values[0].is_some(),
            ),
            (
                "icvgs",
                self.device.initial.values[1].unwrap_or(0.),
                self.device.initial.values[1].is_some(),
            ),
            // `hfet2param.c` stores `temp` in kelvin.
            (
                "temp",
                i.temp
                    .map_or(self.circuit_kelvin + i.dtemp, |c| c + CELSIUS_TO_KELVIN),
                i.temp.is_some(),
            ),
            ("dtemp", i.dtemp, true),
        ] {
            instance = instance.with(keyword, value, given);
        }
        self.setup(&mut model, &mut instance)?;
        self.temperature(&mut model, &mut instance)?;
        Ok((model, instance))
    }

    fn ask(
        &self,
        scope: ParameterScope,
        keyword: &str,
        records: RecordPair<'_>,
    ) -> SpiceResult<Real> {
        // `hfet2ask.c` answers the instance temperature in Celsius while
        // `hfet2param.c` stores it in kelvin, so `.sens` steps the Celsius
        // value and the setter then adds 273.15.
        if scope == ParameterScope::Instance && keyword == "temp" {
            return Ok(records.instance.value("temp")? - CELSIUS_TO_KELVIN);
        }
        match scope {
            ParameterScope::Model => records.model.value(keyword),
            ParameterScope::Instance => records.instance.value(keyword),
        }
    }

    fn set(
        &self,
        scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        match (scope, keyword) {
            // `hfet2param.c` stores `TEMP = value + CONSTCtoK`; the walk's
            // restore adds the offset a second time, as in C.
            (ParameterScope::Instance, "temp") => instance.set(keyword, value + CELSIUS_TO_KELVIN),
            (ParameterScope::Model, _) => model.set(keyword, value),
            (ParameterScope::Instance, _) => instance.set(keyword, value),
        }
    }

    /// `HFET2setup`'s record-visible part. Its model defaults are already in
    /// the records ([`Hfet2::records`] resolves the card), and its
    /// `HFET2_VT1 = 0`/`VT2 = 0` placeholders are overwritten by the
    /// `temperature` call that follows every setup, so nothing is left to do.
    fn setup(
        &self,
        _model: &mut SensitivityRecord,
        _instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        Ok(())
    }

    /// `hfet2temp.c`'s model-side arithmetic, which is the part that persists.
    ///
    /// `HFET2_VT1` and `VT2` are model fields (despite the `here->`-looking
    /// macros): when not given they follow the *raw* `VTO`, and the channel
    /// mixes them with the type-scaled `TVTO`. Everything else the routine
    /// writes is per instance and re-derived by the rebuilt device.
    fn temperature(
        &self,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        let threshold = model.value("vt0")?;
        if !model.given("vt1") {
            let added = CHARGE * model.value("nmax")? * model.value("di")? / model.value("epsi")?;
            model.store("vt1", threshold + added)?;
        }
        if !model.given("vt2") {
            model.store("vt2", threshold)?;
        }
        let _ = instance;
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
                format!("{}: HFET2 AC sensitivity", self.device.name),
                "src/spicelib/devices/hfet2/hfet2acl.c",
            ));
        }
        let (m, i) = (current.model, current.instance);
        let r = |keyword: &str| m.value(keyword);
        let v = |keyword: &str| i.value(keyword);
        let celsius = |keyword: &str| -> SpiceResult<Option<Real>> {
            Ok(i.given(keyword)
                .then(|| i.value(keyword))
                .transpose()?
                .map(|kelvin| kelvin - CELSIUS_TO_KELVIN))
        };
        // `eta2_given`/`d2_given` come from the *records*: the walk's setters
        // flip them and never restore them, and `hfeta2`'s `nsc` branch tests
        // them.
        let model = Model {
            pol: self.device.model.pol,
            threshold: r("vt0")?,
            cf: r("cf")?,
            d1: r("d1")?,
            d2: r("d2")?,
            del: r("del")?,
            delta: r("delta")?,
            deltad: r("deltad")?,
            di: r("di")?,
            epsi: r("epsi")?,
            eta: r("eta")?,
            eta1: r("eta1")?,
            eta2: r("eta2")?,
            gamma: r("gamma")?,
            ggr: r("ggr")?,
            js: r("js")?,
            klambda: r("klambda")?,
            kmu: r("kmu")?,
            knmax: r("knmax")?,
            kvto: r("kvto")?,
            lambda: r("lambda")?,
            m: r("m")?,
            mc: r("mc")?,
            mu: r("mu")?,
            n: r("n")?,
            nmax: r("nmax")?,
            pm: r("p")?,
            rd: r("rd")?,
            rs: r("rs")?,
            rdi: r("rdi")?,
            rsi: r("rsi")?,
            sigma0: r("sigma0")?,
            vs: r("vs")?,
            vsigma: r("vsigma")?,
            vsigmat: r("vsigmat")?,
            vt1: Some(r("vt1")?),
            vt2: Some(r("vt2")?),
            eta2_given: m.given("eta2"),
            d2_given: m.given("d2"),
        };
        let instance = Instance {
            l: v("l")?,
            w: v("w")?,
            m: v("m")?,
            temp: celsius("temp")?,
            dtemp: v("dtemp")?,
        };
        let device = Hfet2 {
            name: self.device.name.clone(),
            model_name: self.device.model_name.clone(),
            terminals: self.device.terminals.clone(),
            nodes: self.device.nodes,
            model,
            instance,
            initial: self.device.initial.clone(),
        };
        Ok(SensitivityLoad::Replacement(Box::new(device)))
    }
}
