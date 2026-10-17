//! HFET1, the `Z` designator at level 5 (`hfet1/`, the `hfeta` channel).
//!
//! C references, read as behaviour only: `hfet1/hfetsetup.c` (defaults and the
//! conditional internal nodes), `hfet1/hfettemp.c` (`HFETAtemp`: every
//! per-instance derived quantity), `hfet1/hfetload.c` (`HFETAload`, the `leak`
//! junction and the `hfeta` channel), `hfet1/hfetacl.c`, `hfet1/hfetpzl.c`,
//! `hfet1/hfettrunc.c`, `hfet1/hfetgetic.c`, `hfet1/hfetask.c`,
//! `hfet1/hfetparam.c` and `hfet1/hfetmpar.c`.
//!
//! # Differences from MESA, which this file must not import
//!
//! - HFET1 is **two-polarity** (`nhfet`/`phfet`): every node difference is
//!   multiplied by the type (`+1`/`-1`) before the device equations and the
//!   loaded residual is multiplied back. MESA forces NMF instead.
//! - There is **one instance temperature**, `temp`, not `td`/`ts`, and the
//!   model's `threshold` is stored normalized (`threshold *= type` in C's
//!   temperature routine).
//! - The resistances have **no temperature coefficients**: `hfettemp.c` turns
//!   the model's `rd`/`rs`/`rg`/`ri`/`rf` straight into conductances.
//! - The gate junctions are `leak()` (two-exponential, optionally with a
//!   series resistance and the `diode()` approximation) or, with
//!   `gatemod != 0`, thermionic currents with the `gmg`/`gmd` cross terms.
//! - The charges are `qgs`/`qgd` **and** a fixed-`cds` drain-source charge
//!   whose integration folds into `gds`/`cd`.
//!
//! # C quirks reproduced deliberately
//!
//! - `hfeta`'s argument is `vds > 0 ? vgs : vgd` *after* `vds` has been made
//!   positive, so it is `vgs` in inverse mode too (a slip against MESA's
//!   `inverse ? vgd : vgs`). The goldens below pin which one C does.
//! - `gmg`/`gmd` are only written when `gatemod != 0`; C's `else` branch nulls
//!   local pointers, leaving the caller's zeros.
//! - `HFETAtemp` multiplies the model's threshold by the type every time it
//!   runs; the port applies the factor in its derivation instead, which is the
//!   same value for one analysis.
//! - The `type` string setter (`type=NHFET`) is not representable in the
//!   port's scalar schema: use the `nhfet`/`phfet` flag or the card base, which
//!   C resolves the same way.

use crate::devices::initial::{InstanceInitial, split};
use crate::devices::jfet::{self, BOLTZMANN, CHARGE, p};
use crate::devices::limiting;
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::{
    AnalysisMode, Device, DeviceState, IterationPhase, LinearContext, ModelContext, ResolvedModel,
    StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// `hfetsetup.c`'s `EPSI` default, C's spelling of the GaAs permittivity.
const EPSILON_GAAS: Real = 12.244 * 8.854_18e-12;
/// C `CONSTKoverQ`.
const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
const CELSIUS_TO_KELVIN: Real = 273.15;
/// `hfeta`'s carrier-density cutoff (`nsm < 1.0e-38` turns the channel off).
const N_MIN: Real = 1e-38;

/// The state slots of `HFETAload`'s `CKTstate0`, in the port's own order. Each
/// charge is followed by its derivative, as `CKTterr`/`NIintegrate` require.
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
    pub(super) const GMG: usize = 14;
    pub(super) const GMD: usize = 15;
    pub(super) const QGS: usize = 16;
    pub(super) const CQGS: usize = 17;
    pub(super) const QGD: usize = 18;
    pub(super) const CQGD: usize = 19;
    /// `CDS * vds`: a linear drain-source charge, integrated like the gates.
    pub(super) const QDS: usize = 20;
    pub(super) const CQDS: usize = 21;
    pub(super) const COUNT: usize = 22;
}

/// `HFETAmPTable` with `hfetsetup.c`'s defaults. The type-dependent defaults
/// (`vt0`, `eta`, `mu`, `vs`) are absent here and resolved by
/// [`Model::resolve`]; `vto` is the `vt0` alias, and `vt1`/`vt2` have their own
/// defaults in `HFETAtemp`.
const MODEL: &[P] = &[
    p("vt0", U::Volt, D::Finite, None),
    p("vto", U::Volt, D::Finite, None),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.15)),
    p("rd", U::Ohm, D::NonNegative, Some(0.)),
    p("rs", U::Ohm, D::NonNegative, Some(0.)),
    p("rg", U::Ohm, D::NonNegative, Some(0.)),
    p("rdi", U::Ohm, D::NonNegative, Some(0.)),
    p("rsi", U::Ohm, D::NonNegative, Some(0.)),
    p("rgs", U::Ohm, D::Positive, Some(90.)),
    p("rgd", U::Ohm, D::Positive, Some(90.)),
    p("ri", U::Ohm, D::NonNegative, Some(0.)),
    p("rf", U::Ohm, D::NonNegative, Some(0.)),
    p("eta", U::Dimensionless, D::Finite, None),
    p("m", U::Dimensionless, D::Finite, Some(3.0)),
    p("mc", U::Dimensionless, D::Finite, Some(3.0)),
    p("gamma", U::Dimensionless, D::Finite, Some(3.0)),
    p("sigma0", U::Dimensionless, D::Finite, Some(0.057)),
    p("vsigmat", U::Volt, D::Finite, Some(0.3)),
    p("vsigma", U::Volt, D::Positive, Some(0.1)),
    p("mu", U::SquareCentimetrePerVoltSecond, D::NonNegative, None),
    p("di", U::Metre, D::Positive, Some(0.04e-6)),
    p("delta", U::Dimensionless, D::Finite, Some(3.0)),
    p("vs", U::MetrePerSecond, D::Positive, None),
    p("nmax", U::PerCubicCentimetre, D::NonNegative, Some(2e16)),
    p("deltad", U::Metre, D::NonNegative, Some(4.5e-9)),
    p("js1d", U::Ampere, D::NonNegative, Some(1.0)),
    p("js2d", U::Ampere, D::NonNegative, Some(1.15e6)),
    p("js1s", U::Ampere, D::NonNegative, Some(1.0)),
    p("js2s", U::Ampere, D::NonNegative, Some(1.15e6)),
    p("m1d", U::Dimensionless, D::Finite, Some(1.32)),
    p("m2d", U::Dimensionless, D::Finite, Some(6.9)),
    p("m1s", U::Dimensionless, D::Finite, Some(1.32)),
    p("m2s", U::Dimensionless, D::Finite, Some(6.9)),
    p("epsi", U::FaradPerMetre, D::Positive, Some(EPSILON_GAAS)),
    p("p", U::Dimensionless, D::Finite, Some(1.)),
    p("cm3", U::Dimensionless, D::Finite, Some(0.17)),
    p("a1", U::Dimensionless, D::Finite, Some(0.)),
    p("a2", U::Dimensionless, D::Finite, Some(0.)),
    p("mv1", U::Dimensionless, D::NonNegative, Some(3.)),
    p("kappa", U::Dimensionless, D::Finite, Some(0.)),
    p("delf", U::Dimensionless, D::NonNegative, Some(0.)),
    p("fgds", U::Dimensionless, D::NonNegative, Some(0.)),
    p("tf", U::Kelvin, D::NonZero, None),
    p("cds", U::Farad, D::NonNegative, Some(0.)),
    // `hfetmpar.c` stores PHIB as `value * CHARGE` (electron-volts in, joules
    // out), so the schema default is the eV figure.
    p("phib", U::Volt, D::Finite, Some(0.5)),
    p("talpha", U::Dimensionless, D::Finite, Some(1200.)),
    p("mt1", U::Dimensionless, D::NonZero, Some(3.5)),
    p("mt2", U::Dimensionless, D::NonZero, Some(9.9)),
    p("ck1", U::Dimensionless, D::Finite, Some(1.)),
    p("ck2", U::Volt, D::Finite, Some(0.)),
    p("cm1", U::Dimensionless, D::Finite, Some(3.)),
    p("cm2", U::Volt, D::Finite, Some(0.)),
    p("astar", U::Dimensionless, D::NonNegative, Some(4.0e4)),
    p("eta1", U::Dimensionless, D::Finite, Some(2.)),
    p("d1", U::Metre, D::Positive, Some(0.03e-6)),
    p("vt1", U::Volt, D::Finite, None),
    p("eta2", U::Dimensionless, D::Finite, Some(2.)),
    p("d2", U::Metre, D::Positive, Some(0.2e-6)),
    p("vt2", U::Volt, D::Finite, None),
    p("ggr", U::SiemensPerMetre, D::NonNegative, Some(40.)),
    p("del", U::Dimensionless, D::NonNegative, Some(0.04)),
    p("gatemod", U::Dimensionless, D::Finite, Some(0.)),
    p("klambda", U::Dimensionless, D::Finite, Some(0.)),
    p("kmu", U::Dimensionless, D::Finite, Some(0.)),
    p("kvto", U::Dimensionless, D::Finite, Some(0.)),
    p("level", U::Dimensionless, D::Finite, Some(5.)),
];

/// `HFETApTable`'s scalar instance setters (`hfetparam.c`).
const INSTANCE: &[P] = &[
    p("l", U::Metre, D::Positive, Some(1e-6)),
    p("w", U::Metre, D::Positive, Some(20e-6)),
    p("m", U::Dimensionless, D::Positive, Some(1.)),
    p("temp", U::Celsius, D::Temperature, None),
    p("dtemp", U::Dimensionless, D::Finite, Some(0.)),
];

/// The `IC` vector components (`hfetparam.c`): `ICVDS`, then `ICVGS`.
pub(super) const IC_COMPONENTS: [&str; 2] = ["icvds", "icvgs"];

/// The validated model card.
#[derive(Debug, Clone, Copy)]
pub(super) struct Model {
    /// `+1` for N, `-1` for P (`nhfet`/`phfet`, which C also applies to
    /// `HFETAthreshold` in its temperature routine).
    pol: Real,
    threshold: Real,
    lambda: Real,
    rd: Real,
    rs: Real,
    rg: Real,
    rdi: Real,
    rsi: Real,
    rgs: Real,
    rgd: Real,
    ri: Real,
    rf: Real,
    eta: Real,
    m: Real,
    mc: Real,
    gamma: Real,
    sigma0: Real,
    vsigmat: Real,
    vsigma: Real,
    mu: Real,
    di: Real,
    delta: Real,
    vs: Real,
    nmax: Real,
    deltad: Real,
    js1d: Real,
    js2d: Real,
    js1s: Real,
    js2s: Real,
    m1d: Real,
    m2d: Real,
    m1s: Real,
    m2s: Real,
    epsi: Real,
    pm: Real,
    cm3: Real,
    a1: Real,
    a2: Real,
    mv1: Real,
    kappa: Real,
    delf: Real,
    fgds: Real,
    /// `hfettemp.c`'s `Given` flags `hfeta` reads: the two-exponential branch
    /// needs both `eta2` and `d2`, and `n02` needs `eta2` alone.
    eta2_given: bool,
    d2_given: bool,
    /// `hfetsetup.c` defaults `tf` to `CKTtemp` (kelvin).
    tf: Option<Real>,
    cds: Real,
    phib: Real,
    talpha: Real,
    mt1: Real,
    mt2: Real,
    ck1: Real,
    ck2: Real,
    cm1: Real,
    cm2: Real,
    astar: Real,
    eta1: Real,
    d1: Real,
    vt1: Option<Real>,
    eta2: Real,
    d2: Real,
    vt2: Option<Real>,
    ggr: Real,
    del: Real,
    gatemod: Real,
    klambda: Real,
    kmu: Real,
    kvto: Real,
}

impl Model {
    /// `HFETAsetup`'s defaults, `hfetmpar.c`'s setters and `inpdomod.c`'s level
    /// check. A `phfet` base (or the `phfet` flag) makes the model p-type.
    pub(super) fn resolve<'a>(
        resolved: &'a ResolvedModel<'_>,
    ) -> SpiceResult<(Self, Vec<&'a crate::netlist::ast::ParameterAssignment>)> {
        let (pol, scalars) = jfet::typed_scalars(
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
        let r = |name: &str| jfet::required(&values, name);
        let alias = |names: &[&str]| jfet::last_setter(&values, names);
        // `hfetsetup.c` gives the threshold, `eta`, `mu` and `vs` per type.
        let typed = |explicit: Option<Real>, n: Real, p: Real| {
            explicit.unwrap_or(if pol > 0. { n } else { p })
        };
        let model = Self {
            pol,
            threshold: typed(alias(&["vt0", "vto"]), 0.15, -0.15),
            lambda: r("lambda")?,
            rd: r("rd")?,
            rs: r("rs")?,
            rg: r("rg")?,
            rdi: r("rdi")?,
            rsi: r("rsi")?,
            rgs: r("rgs")?,
            rgd: r("rgd")?,
            ri: r("ri")?,
            rf: r("rf")?,
            eta: typed(values.get("eta").map(|v| v.value), 1.28, 1.4),
            m: r("m")?,
            mc: r("mc")?,
            gamma: r("gamma")?,
            sigma0: r("sigma0")?,
            vsigmat: r("vsigmat")?,
            vsigma: r("vsigma")?,
            mu: typed(values.get("mu").map(|v| v.value), 0.4, 0.03),
            di: r("di")?,
            delta: r("delta")?,
            vs: typed(values.get("vs").map(|v| v.value), 1.5e5, 0.8e5),
            nmax: r("nmax")?,
            deltad: r("deltad")?,
            js1d: r("js1d")?,
            js2d: r("js2d")?,
            js1s: r("js1s")?,
            js2s: r("js2s")?,
            m1d: r("m1d")?,
            m2d: r("m2d")?,
            m1s: r("m1s")?,
            m2s: r("m2s")?,
            epsi: r("epsi")?,
            pm: r("p")?,
            cm3: r("cm3")?,
            a1: r("a1")?,
            a2: r("a2")?,
            mv1: r("mv1")?,
            kappa: r("kappa")?,
            delf: r("delf")?,
            fgds: r("fgds")?,
            tf: values.get("tf").map(|v| v.value),
            cds: r("cds")?,
            phib: CHARGE * r("phib")?,
            talpha: r("talpha")?,
            mt1: r("mt1")?,
            mt2: r("mt2")?,
            ck1: r("ck1")?,
            ck2: r("ck2")?,
            cm1: r("cm1")?,
            cm2: r("cm2")?,
            astar: r("astar")?,
            eta1: r("eta1")?,
            d1: r("d1")?,
            vt1: values.get("vt1").map(|v| v.value),
            eta2: r("eta2")?,
            d2: r("d2")?,
            vt2: values.get("vt2").map(|v| v.value),
            // C's `*Given` flags: a schema default is not a given setter.
            eta2_given: values.get("eta2").is_some_and(|v| v.location.is_some()),
            d2_given: values.get("d2").is_some_and(|v| v.location.is_some()),
            ggr: r("ggr")?,
            del: r("del")?,
            gatemod: r("gatemod")?,
            klambda: r("klambda")?,
            kmu: r("kmu")?,
            kvto: r("kvto")?,
        };
        if model.vsigma == 0. || model.di == 0. {
            return Err(SpiceError::circuit(format!(
                "{}: HFET1 model '{}' needs VSIGMA and DI nonzero (hfeta divides by them)",
                resolved.card().location,
                resolved.card().name
            )));
        }
        Ok((model, defaults))
    }
}

/// Validated instance setters (`hfetparam.c`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Instance {
    pub(super) l: Real,
    pub(super) w: Real,
    pub(super) m: Real,
    /// Celsius as written (`hfetparam.c` adds 273.15); `None` is
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
            l: jfet::required(&values, "l")?,
            w: jfet::required(&values, "w")?,
            m: jfet::required(&values, "m")?,
            temp: values.get("temp").map(|v| v.value),
            dtemp: jfet::required(&values, "dtemp")?,
        })
    }
}

/// `hfettemp.c`'s per-instance quantities.
#[derive(Debug, Clone, Copy)]
struct Derived {
    temp: Real,
    vt: Real,
    tlambda: Real,
    tmu: Real,
    tvto: Real,
    vt1: Real,
    vt2: Real,
    n0: Real,
    n02: Real,
    /// Whether `hfeta` may use the `n02` branch: C requires both `eta2` and
    /// `d2` to be given.
    two_exponentials: bool,
    gchi0: Real,
    cf: Real,
    imax: Real,
    is1d: Real,
    is2d: Real,
    is1s: Real,
    is2s: Real,
    iso: Real,
    ggrwl: Real,
    fgds: Real,
    delf: Real,
    delta_sqr: Real,
    gdpr: Real,
    gspr: Real,
    ggate: Real,
    tgf: Real,
    tgi: Real,
}

/// One `hfeta` evaluation: the channel, the two gate currents and the
/// `gatemod != 0` cross terms.
#[derive(Debug, Clone, Copy)]
struct Channel {
    cdrain: Real,
    gm: Real,
    gds: Real,
    capgs: Real,
    capgd: Real,
    /// `cgd`/`cgs`/`ggs` are the caller's junction currents, which `hfeta`
    /// replaces when `gatemod != 0`.
    cgd: Real,
    cgs: Real,
    ggs: Real,
    gmg: Real,
    gmd: Real,
}

/// `hfetsetup.c`'s internal nodes and the intrinsic terminals they lead to.
#[derive(Debug, Clone, Copy)]
struct Nodes {
    d: NodeId,
    g: NodeId,
    s: NodeId,
    sp: NodeId,
    dp: NodeId,
    gp: NodeId,
    spp: NodeId,
    dpp: NodeId,
}

/// HFET1, `Z` level 5.
#[derive(Debug, Clone)]
pub(crate) struct Hfet1 {
    name: String,
    model_name: String,
    terminals: Vec<NodeId>,
    nodes: Nodes,
    model: Model,
    instance: Instance,
    initial: InstanceInitial,
}

impl Hfet1 {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 5 {
            return Err(SpiceError::circuit(format!(
                "{}: HFET1 needs level 5",
                i.name
            )));
        }
        if i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: HFET1 needs three terminals",
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
        // `hfetsetup.c` creates `#source` for RS, `#drain` for RD, `#gate` for
        // RG, `#gs` for RI and `#gd` for RF, in that order, and falls back to
        // the unfurled node when a resistance is zero.
        let mut create = |resistance: Real, suffix: &str, target: &mut NodeId| -> SpiceResult<()> {
            if resistance == 0. {
                return Ok(());
            }
            let name = format!("{}#{suffix}", i.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "{}: HFET1 internal-node name collision: {name}",
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
            terminals,
            nodes: nodes_of,
            model,
            instance,
            initial,
        };
        device.derived(context.temperature)?;
        Ok(Box::new(device))
    }

    /// `hfettemp.c`. `temperature` is Celsius (`CKTtemp`), while `TEMP` is
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
        // `hfettemp.c` stores the threshold already multiplied by the type.
        let vto = model.threshold * model.pol;
        let vt = K_OVER_Q * temp;
        let tlambda = model.lambda + model.klambda * delta;
        let tmu = model.mu - model.kmu * delta;
        let tvto = vto - model.kvto * delta;
        let n0 = model.epsi * model.eta * vt / 2. / CHARGE / (model.di + model.deltad);
        let n01 = model.epsi * model.eta1 * vt / 2. / CHARGE / model.d1;
        let n02 = if model.eta2_given {
            model.epsi * model.eta2 * vt / 2. / CHARGE / model.d2
        } else {
            0.
        };
        let _ = n01;
        let gchi0 = CHARGE * instance.w * tmu / instance.l;
        let cf = 0.5 * model.epsi * instance.w;
        let imax = CHARGE * model.nmax * model.vs * instance.w;
        let area = instance.w * instance.l;
        let exp_tf = (temp / model.tf.unwrap_or(temperature + CELSIUS_TO_KELVIN)).exp();
        let derived = Derived {
            temp,
            vt,
            tlambda,
            tmu,
            tvto,
            vt1: model
                .vt1
                .unwrap_or(vto + CHARGE * model.nmax * model.di / model.epsi),
            vt2: model.vt2.unwrap_or(vto),
            n0,
            n02,
            two_exponentials: model.eta2_given && model.d2_given,
            gchi0,
            cf,
            imax,
            is1d: model.js1d * area / 2.,
            is2d: model.js2d * area / 2.,
            is1s: model.js1s * area / 2.,
            is2s: model.js2s * area / 2.,
            iso: model.astar * area / 2.,
            ggrwl: model.ggr * instance.l * instance.w / 2.,
            fgds: model.fgds * exp_tf,
            delf: model.delf * exp_tf,
            // `hfettemp.c` also derives `HFETAvcrit`, which only feeds
            // `DEVpnjlim`; HFET1's load never calls it, so it is not carried.
            delta_sqr: model.delta * model.delta,
            gdpr: if model.rd == 0. { 0. } else { 1. / model.rd },
            gspr: if model.rs == 0. { 0. } else { 1. / model.rs },
            ggate: if model.rg == 0. { 0. } else { 1. / model.rg },
            tgf: if model.rf == 0. { 0. } else { 1. / model.rf },
            tgi: if model.ri == 0. { 0. } else { 1. / model.ri },
        };
        if [
            derived.temp,
            derived.vt,
            derived.tlambda,
            derived.tmu,
            derived.tvto,
            derived.vt1,
            derived.vt2,
            derived.n0,
            derived.gchi0,
            derived.cf,
            derived.imax,
            derived.iso,
            derived.ggrwl,
            derived.imax,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || derived.vt == 0.
        {
            return Err(SpiceError::circuit(format!(
                "{}: HFET1 temperature derivation is out of range (check TF, DI, DELTAD, \
                 NMU and the instance temperature)",
                self.name
            )));
        }
        Ok(derived)
    }

    /// `hfetload.c`'s `diode()`: a smooth approximation of the junction law.
    fn diode(u: Real) -> Real {
        const U0: Real = -2.303;
        const A: Real = 2.221;
        const B: Real = 6.804;
        const C: Real = 1.685;
        let expu = u.exp();
        let it = if u <= U0 {
            expu * (1. - expu)
        } else {
            let b = 0.5 * (u - U0);
            u + A * ((U0 - u) / B).exp() - (b + (b * b + 0.25 * C * C).sqrt()).ln()
        };
        let ut = it + it.ln();
        let b = u - ut;
        let c = 1. + it;
        it * (1. + b / c + 0.5 * b * b / c / c / c)
    }

    /// `hfetload.c`'s `leak()`: the two-exponential gate junction, with a
    /// series resistance when one is given.
    #[allow(clippy::too_many_arguments)]
    fn leak(
        gmin: Real,
        vt: Real,
        v: Real,
        rs: Real,
        is1: Real,
        is2: Real,
        m1: Real,
        m2: Real,
    ) -> (Real, Real) {
        let vt1 = vt * m1;
        let vt2 = vt * m2;
        if v > -10. * vt1 {
            let vteff = vt1 + vt2;
            let iseff = is2 * (is1 / is2).powf(m1 / (m1 + m2));
            let (iaprox1, iaprox2) = if rs > 0. {
                let unorm = (v + rs * is1) / vt1 + (rs * is1 / vt1).ln();
                let first = vt1 * Self::diode(unorm) / rs - is1;
                let unorm = (v + rs * iseff) / vteff + (rs * iseff / vteff).ln();
                (first, vteff * Self::diode(unorm) / rs - iseff)
            } else {
                (
                    is1 * ((v / vt1).exp() - 1.),
                    iseff * ((v / vteff).exp() - 1.),
                )
            };
            let iaprox = if iaprox1 * iaprox2 != 0. {
                1. / (1. / iaprox1 + 1. / iaprox2)
            } else {
                0.5 * (iaprox1 + iaprox2)
            };
            let dvdi0 = rs + vt1 / (iaprox + is1) + vt2 / (iaprox + is2);
            let mut v0 = rs * iaprox;
            v0 += vt1 * (iaprox / is1 + 1.).ln() + vt2 * (iaprox / is2 + 1.).ln();
            let il = (iaprox + (v - v0) / dvdi0).max(-is1) * 0.99999;
            let gl = 1. / (rs + vt1 / (il + is1) + vt2 / (il + is2));
            (il, gl)
        } else {
            (gmin * v - is1, gmin)
        }
    }

    /// `hfeta()`: the channel, the gate currents and the `gatemod` cross
    /// terms. `vgs` is C's `vds > 0 ? vgs : vgd` argument (see the module
    /// docs); `vds` is the normalized (nonnegative) drain-source voltage.
    #[allow(clippy::too_many_arguments)]
    fn hfeta(
        &self,
        derived: &Derived,
        vgs: Real,
        vds: Real,
        mut cgd: Real,
        mut cgs: Real,
        mut ggs: Real,
        gmin: Real,
    ) -> SpiceResult<Channel> {
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
            let terms = self.gatemod_terms(derived, vgs, vds, 0., 0., 0., gmin, (cgd, cgs, ggs));
            return Ok(Channel {
                cdrain: 0.,
                gm: 0.,
                gds: 0.,
                capgs: derived.cf,
                capgd: derived.cf,
                cgd: terms.cgd,
                cgs: terms.cgs,
                ggs: terms.ggs,
                gmg: terms.gmg,
                gmd: terms.gmd,
            });
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
        let capgs = derived.cf + 2. * third * cgc * (1. - a) / (1. + p);
        let a = vsate / (2. * vsate - vdse);
        let a = a * a;
        let capgd = derived.cf + 2. * p * third * cgc * (1. - a) / (1. + p);
        // `cgd_calc`: the gatemod'd gate currents and the cross terms.
        let terms = self.gatemod_terms(
            derived,
            vgs,
            vds,
            vsate,
            delvsatevgt,
            delvgtvgs,
            gmin,
            (cgd, cgs, ggs),
        );
        cgd = terms.cgd;
        cgs = terms.cgs;
        ggs = terms.ggs;
        let mut cdrain = cdrain;
        let mut gm = gm;
        let mut gds = gds;
        // C gates the corrections on `gatemod != 0` as well as on the
        // coefficients, so an `a1`/`a2` deck without `gatemod` gets none.
        if model.gatemod != 0. && (model.a1 != 0. || model.a2 != 0.) {
            let vmax = model.cm3 * vsate;
            let a = (vds / vmax).powf(model.mv1);
            let b = (1. + a).powf(1. / model.mv1);
            let vdse = vds / b;
            let delvdsevmax = vdse * a / ((1. + a) * vmax);
            let delvdsevds = (1. - a / (1. + a)) / b;
            let dvdsevgs = delvdsevmax * model.cm3 * delvsatevgt * delvgtvgs;
            let dvdsevds = delvdsevds + delvdsevmax * model.cm3 * delvsatevgt * sigma;
            let c = vgte * vdse;
            let d = 1. + model.a2 * c;
            let e = vdse * delvgtevgt;
            let f = model.a2 * cgd;
            cdrain += model.a1 * (d * cgd - cgs);
            gds += model.a1 * (d * terms.gmd + f * (vgte * dvdsevds + e * sigma));
            gm += model.a1 * (d * terms.gmg + f * (vgte * dvdsevgs + e * delvgtvgs) - terms.ggs);
        }
        let values = [cdrain, gm, gds, capgs, capgd];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::circuit(format!(
                "{}: nonfinite hfeta channel equations",
                self.name
            )));
        }
        Ok(Channel {
            cdrain,
            gm,
            gds,
            capgs,
            capgd,
            cgd,
            cgs,
            ggs,
            gmg: terms.gmg,
            gmd: terms.gmd,
        })
    }

    /// `hfeta`'s `cgd_calc` block: with `gatemod != 0` the gate-drain and
    /// gate-source currents replace the caller's, and the `gmg`/`gmd` cross
    /// conductances exist; with `gatemod == 0` C leaves them at zero.
    #[allow(clippy::too_many_arguments)]
    fn gatemod_terms(
        &self,
        derived: &Derived,
        vgs: Real,
        vds: Real,
        vsate: Real,
        delvsatevgt: Real,
        delvgtvgs: Real,
        gmin: Real,
        junction: (Real, Real, Real),
    ) -> Channel {
        let model = &self.model;
        let (incoming_cgd, incoming_cgs, incoming_ggs) = junction;
        if model.gatemod == 0. {
            return Channel {
                cdrain: 0.,
                gm: 0.,
                gds: 0.,
                capgs: 0.,
                capgd: 0.,
                cgd: incoming_cgd,
                cgs: incoming_cgs,
                ggs: incoming_ggs,
                gmg: 0.,
                gmd: 0.,
            };
        }
        let vt = derived.vt;
        let sigma = {
            let vgt0 = vgs - derived.tvto;
            let s = ((vgt0 - model.vsigmat) / model.vsigma).exp();
            model.sigma0 / (1. + s)
        };
        let temp = derived.temp;
        let vkneet = model.ck1 * vsate + model.ck2;
        let vmax = model.cm1 * vsate + model.cm2;
        // C evaluates `pow(vds/vmax, mt2)` and `pow(vdse/vkneet, mt1)` with no
        // guard, so an off channel (`nsm < 1e-38` returns with `vsate = 0`)
        // and the default `ck2 = cm2 = 0` turn both into `0/0` and every
        // `gatemod` term into NaN. C carries that NaN into the matrix and
        // recovers through `DEVfetlim` on the next iteration (the reference
        // binary answers a `gatemod=1` deck through dynamic gmin stepping);
        // the port refuses to store non-finite state, so it takes the
        // mathematical limit instead: no channel means no gate-modulated
        // current. The converged point has `vsate > 0` whenever C's answer is
        // finite, so the fixed point is the same.
        if vkneet == 0. || vmax == 0. {
            return Channel {
                cdrain: 0.,
                gm: 0.,
                gds: 0.,
                capgs: 0.,
                capgd: 0.,
                cgd: 0.,
                cgs: 0.,
                ggs: 0.,
                gmg: 0.,
                gmd: 0.,
            };
        }
        let a = (vds / vmax).powf(model.mt2);
        let b = (1. + a).powf(1. / model.mt2);
        let vdse = vds / b;
        let c = (vdse / vkneet).powf(model.mt1);
        let d = (1. + c).powf(1. / model.mt1);
        let td = temp + model.talpha * vdse * vdse / d;
        let e = K_OVER_Q * td * model.m2d;
        let p = model.phib / (BOLTZMANN * td);
        let f = (-p).exp();
        let q = (vgs - vdse) / e;
        let g = q.exp();
        let h = derived.iso * td * td * f * g;
        let cgd = h - derived.iso * temp * temp * (-model.phib / (BOLTZMANN * temp)).exp();
        let delcgdvgs = h / e;
        let delcgdtd = h * (p - q + 2.) / td;
        let deltdvdse = model.talpha * vdse * (2. - c / (1. + c)) / d;
        let deltdvkneet = (td - temp) * c / ((1. + c) * vkneet);
        let delvdsevmax = vdse * a / ((1. + a) * vmax);
        let delvdsevds = (1. - a / (1. + a)) / b;
        let temp2 = delvsatevgt * delvgtvgs;
        let dvdsevgs = delvdsevmax * model.cm1 * temp2;
        let dtdvgs = deltdvdse * dvdsevgs + deltdvkneet * model.ck1 * temp2;
        let gmg = delcgdvgs + delcgdtd * dtdvgs;
        let temp2 = delvsatevgt * sigma;
        let dvdsevds = delvdsevds + delvdsevmax * model.cm1 * temp2;
        let dtdvds = deltdvdse * dvdsevds + deltdvkneet * model.ck1 * temp2;
        let gmd = -delcgdvgs * dvdsevds + delcgdtd * dtdvds;
        // Gate-source current.
        let vtn = vt * model.m2s;
        let csat = derived.iso * temp * temp * (-model.phib / (BOLTZMANN * temp)).exp();
        let (cgs, ggs) = if vgs <= -5. * vt {
            let ggs = -csat / vgs + gmin;
            (ggs * vgs, ggs)
        } else {
            let evgs = (vgs / vtn).exp();
            let ggs = csat * evgs / vtn + gmin;
            (csat * (evgs - 1.) + gmin * vgs, ggs)
        };
        Channel {
            cdrain: 0.,
            gm: 0.,
            gds: 0.,
            capgs: 0.,
            capgd: 0.,
            cgd,
            cgs,
            ggs,
            gmg,
            gmd,
        }
    }
}

/// `HFETAload`'s evaluated device quantities at one bias.
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
    capgs: Real,
    capgd: Real,
    gmg: Real,
    gmd: Real,
}

/// C's initialization and limiting, which for HFET1 is `DEVfetlim` alone: the
/// gate junctions are never `pnjlim`-limited, so `icheck` stays 0 and only the
/// `cghat`/`cdhat` comparison decides convergence.
#[derive(Debug, Clone, Copy)]
enum Start {
    /// The `uic` initial load.
    Conditions,
    /// A load with limiting inactive: the solution itself.
    Exact,
    /// `MODEINITJCT`/`MODEINITFIX`: always nonconvergent.
    Fixed,
    /// A held `off` load.
    HeldOff { raw: [Real; 2] },
    /// The `hfeta` channel's own `cghat`/`cdhat` test.
    Limited { cghat: Real, cdhat: Real },
}

impl Hfet1 {
    /// `HFETAload`'s initialization and limiting. `raw` and `ic` are already
    /// type-normalized (`HFETAload` multiplies every node difference by the
    /// model type).
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
            slot::GMG,
            slot::GMD,
            slot::GGSPP,
            slot::GGDPP,
        ]
        .into_iter()
        .map(|index| old(index).filter(|v| v.is_finite()))
        .collect();
        let Some(
            &[
                vgs0,
                vgd0,
                cg0,
                cd0,
                gm0,
                gds0,
                ggs0,
                ggd0,
                gmg0,
                gmd0,
                ggspp0,
                ggdpp0,
            ],
        ) = previous.as_deref()
        else {
            return ([vgs, vgd], Start::Exact);
        };
        let delvgs = vgs - vgs0;
        let delvgd = vgd - vgd0;
        let delvds = delvgs - delvgd;
        let delvgspp = raw[0] - vgs0;
        let delvgdpp = raw[1] - vgd0;
        let cghat = cg0 + gmg0 * delvgs - gmd0 * delvds
            + ggd0 * delvgd
            + ggs0 * delvgs
            + ggdpp0 * delvgdpp
            + ggspp0 * delvgspp;
        let cdhat =
            cd0 + gm0 * delvgs + gds0 * delvds - ggd0 * delvgd - (gmg0 * delvgs - gmd0 * delvds);
        // `HFETAload` limits only through `DEVfetlim`.
        let vgs = limiting::fetlim(vgs, vgs0, derived.tvto);
        let vgd = limiting::fetlim(vgd, vgd0, derived.tvto);
        ([vgs, vgd], Start::Limited { cghat, cdhat })
    }

    /// `HFETAload`'s device evaluation: the gate junctions, the `hfeta`
    /// channel and the inverse-mode normalization.
    fn point(&self, derived: &Derived, vgs: Real, vgd: Real, gmin: Real) -> SpiceResult<Point> {
        let model = &self.model;
        let vt = derived.vt;
        let (mut cgs, mut ggs, mut cgd, mut ggd);
        if model.gatemod == 0. {
            let (current, conductance) = if derived.is1s == 0. || derived.is2s == 0. {
                (0., 0.)
            } else {
                Self::leak(
                    gmin,
                    vt,
                    vgs,
                    model.rgs,
                    derived.is1s,
                    derived.is2s,
                    model.m1s,
                    model.m2s,
                )
            };
            cgs = current;
            ggs = conductance;
            let arg = -vgs * model.del / vt;
            let earg = arg.exp();
            cgs += derived.ggrwl * vgs * earg;
            ggs += derived.ggrwl * earg * (1. - arg);
            let (current, conductance) = if derived.is1d == 0. || derived.is2d == 0. {
                (0., 0.)
            } else {
                Self::leak(
                    gmin,
                    vt,
                    vgd,
                    model.rgd,
                    derived.is1d,
                    derived.is2d,
                    model.m1d,
                    model.m2d,
                )
            };
            cgd = current;
            ggd = conductance;
            let arg = -vgd * model.del / vt;
            let earg = arg.exp();
            cgd += derived.ggrwl * vgd * earg;
            ggd += derived.ggrwl * earg * (1. - arg);
        } else {
            cgs = 0.;
            ggs = 0.;
            cgd = 0.;
            ggd = 0.;
        }
        let mut vds = vgs - vgd;
        let inverse = vds < 0.;
        if inverse {
            vds = -vds;
        }
        // C's argument is `vds > 0 ? vgs : vgd` *after* the normalization
        // above, so it is `vgs` in inverse mode too.
        let channel = self.hfeta(
            derived,
            if vds > 0. { vgs } else { vgd },
            vds,
            cgd,
            cgs,
            ggs,
            gmin,
        )?;
        cgd = channel.cgd;
        cgs = channel.cgs;
        ggs = channel.ggs;
        let cg = cgs + cgd;
        let (cdrain, capgs, capgd) = if inverse {
            (-channel.cdrain, channel.capgd, channel.capgs)
        } else {
            (channel.cdrain, channel.capgs, channel.capgd)
        };
        Ok(Point {
            vgs,
            vgd,
            vds: vgs - vgd,
            cgs,
            ggs,
            cgd,
            ggd,
            cd: cdrain - cgd,
            cg,
            gm: channel.gm,
            gds: channel.gds,
            capgs,
            capgd,
            gmg: channel.gmg,
            gmd: channel.gmd,
        })
    }

    /// The total instance temperature in kelvin, `hfettemp.c`'s rule for asks.
    fn temperature(&self, temperature: Real) -> Real {
        match self.instance.temp {
            Some(v) => v + CELSIUS_TO_KELVIN,
            None => temperature + CELSIUS_TO_KELVIN + self.instance.dtemp,
        }
    }
}

impl Device for Hfet1 {
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
    /// `hfettrunc.c`: both gate charges control the timestep (`qds` does not).
    fn truncation_slots(&self) -> Vec<usize> {
        vec![slot::QGS, slot::QGD]
    }
    /// Noiseless: `hfet1init.c` leaves `DEVnoise = NULL`.
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }
    /// Linear in `.disto`: `hfet1init.c` leaves `DEVdisto = NULL`.
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(crate::devices::distortion::DeviceDistortion::Linear)
    }
    /// `hfetask.c`: `l`, `m`, the given `icvds`/`icvgs`, and `temp`/`dtemp` in
    /// Celsius (`temp` stores kelvin).
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        Ok(match key {
            "l" => Some(self.instance.l),
            // C scales WIDTH by M and leaves LENGTH alone.
            "w" => Some(self.instance.w * self.instance.m),
            "m" => Some(self.instance.m),
            "icvds" => self.initial.values[0],
            "icvgs" => self.initial.values[1],
            "temp" => Some(self.temperature(context.temperature) - CELSIUS_TO_KELVIN),
            "dtemp" => Some(self.instance.dtemp),
            _ => None,
        })
    }
    /// `hfetask.c`'s bias quantities, `m`-scaled as C stores them.
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
        // C reads `qgs`/`cqgs`/`qgd`/`cqgd` out of `CKTstate0`, accumulated
        // across accepted points; this hook sees only the converged solution.
        if matches!(key, "qgs" | "cqgs" | "qgd" | "cqgd") {
            return Err(SpiceError::not_yet_ported(
                format!("{}: HFET1 @{}[{key}]", self.name, self.name),
                "src/spicelib/devices/hfet1/hfetask.c (HFETAqgs reads CKTstate0)",
            ));
        }
        if !keys.contains(&key) {
            return Ok(None);
        }
        if transient {
            return Err(SpiceError::not_yet_ported(
                format!("{}: transient @{}[{key}]", self.name, self.name),
                "src/spicelib/devices/hfet1/hfetload.c (charge companion)",
            ));
        }
        let derived = self.derived(context.temperature)?;
        let pol = self.model.pol;
        let vgs = pol * (voltage(self.nodes.gp) - voltage(self.nodes.sp));
        let vgd = pol * (voltage(self.nodes.gp) - voltage(self.nodes.dp));
        let point = self.point(&derived, vgs, vgd, context.gmin)?;
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
            // `hfetask.c`'s `cs` is the sum of the two gate currents, in the
            // device's own polarity.
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
    /// `HFETAparam` with the instance schema's domains: a swept temperature is
    /// re-derived by the next load.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        _context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let check = |ok: bool, what: &str| -> SpiceResult<()> {
            if ok {
                Ok(())
            } else {
                Err(SpiceError::circuit(format!(
                    "{}: HFET1 {} must be {what}, got {value}",
                    self.name, parameter
                )))
            }
        };
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
            "temp" => {
                check(value.is_finite(), "finite")?;
                instance.temp = Some(value);
            }
            "dtemp" => {
                check(value.is_finite(), "finite")?;
                instance.dtemp = value;
            }
            _ => {
                // `off`, `icvds` and `icvgs` are the initial-condition setters.
                let _ = &mut initial;
                return Err(SpiceError::circuit(format!(
                    "{}: HFET1 cannot sweep {parameter}",
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
            initial,
        };
        device.derived(_context.temperature)?;
        Ok(Box::new(device))
    }

    /// `HFETAload` without bypass or production. The equations and their order
    /// are C's, including the type factor on every node difference and on the
    /// loaded residual.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit(format!(
                "{}: HFET1 AC needs small-signal assembly",
                self.name
            )));
        }
        let derived = self.derived(context.temperature)?;
        let gmin = context.gmin;
        let m = self.instance.m;
        let pol = self.model.pol;
        let voltage = |n: NodeId| context.node_voltage(n);
        let raw = [
            pol * (voltage(self.nodes.gp) - voltage(self.nodes.sp)),
            pol * (voltage(self.nodes.gp) - voltage(self.nodes.dp)),
        ];
        // `hfetgetic.c`: unset initial conditions come from the external
        // nodes, and `HFETAload` scales them by the type.
        let ic = [
            self.initial.values[0].unwrap_or_else(|| voltage(self.nodes.d) - voltage(self.nodes.s)),
            self.initial.values[1].unwrap_or_else(|| voltage(self.nodes.g) - voltage(self.nodes.s)),
        ];
        let ([vgs, vgd], start) = self.select_junctions(&derived, &context.states, raw, ic);
        let initial_transient =
            context.states.initial_transient() && context.states.phase() == IterationPhase::Predict;
        // `HFETAload` reads the prime-prime pair from the solution like the
        // prime pair (MESA's rule, with HFET1's predictor slots).
        let pp = |aliased: bool, node: NodeId, limited: Real, slot: usize| {
            if aliased {
                return limited;
            }
            let solved = || pol * (voltage(self.nodes.gp) - voltage(node));
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
        let mut point = self.point(&derived, vgs, vgd, gmin)?;
        // `HFETAload`'s charge storage, which C runs before its convergence
        // test. `qgs`/`qgd` accumulate from the accepted charges and the
        // prime-prime voltages; `qds` is the fixed `cds` across the channel.
        let mut cg = point.cg;
        let mut cd = point.cd;
        let mut gds = point.gds;
        let (mut ggspp, mut ggdpp, mut cgspp, mut cgdpp) = (0., 0., 0., 0.);
        let (mut qgs, mut qgd, mut qds) = (0., 0., 0.);
        // `HFETAcqds`: stored beside its charge because trapezoidal order 2
        // reads the accepted derivative (`state1[ccap]` in `NIintegrate`).
        let mut cqds = 0.;
        if matches!(
            context.mode,
            AnalysisMode::Transient { .. } | AnalysisMode::DcSweep
        ) {
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
            let vds1 = context
                .states
                .accepted(1, slot::VGS)
                .filter(|v| v.is_finite())
                .unwrap_or(vgs)
                - context
                    .states
                    .accepted(1, slot::VGD)
                    .filter(|v| v.is_finite())
                    .unwrap_or(vgd);
            if initial_transient {
                context
                    .states
                    .patch_state1(slot::QGS, point.capgs * vgspp)?;
                context
                    .states
                    .patch_state1(slot::QGD, point.capgd * vgdpp)?;
                context
                    .states
                    .patch_state1(slot::QDS, self.model.cds * point.vds)?;
            }
            qgs = context
                .states
                .accepted(1, slot::QGS)
                .filter(|v| v.is_finite())
                .unwrap_or(0.)
                + point.capgs * (vgspp - vgs1);
            qgd = context
                .states
                .accepted(1, slot::QGD)
                .filter(|v| v.is_finite())
                .unwrap_or(0.)
                + point.capgd * (vgdpp - vgd1);
            qds = context
                .states
                .accepted(1, slot::QDS)
                .filter(|v| v.is_finite())
                .unwrap_or(0.)
                + self.model.cds * (point.vds - vds1);
            if initial_transient {
                context.states.patch_state1(slot::QGS, qgs)?;
                context.states.patch_state1(slot::QGD, qgd)?;
                context.states.patch_state1(slot::QDS, qds)?;
            }
            if let Some(coefficients) = context.integration {
                let AnalysisMode::Transient { dt, .. } = context.mode else {
                    return Err(SpiceError::circuit(
                        "HFET1 charge companion outside transient",
                    ));
                };
                if dt != coefficients.dt() {
                    return Err(SpiceError::circuit("HFET1 charge timestep mismatch"));
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
                let (geq, cq) = junction_companion(
                    context,
                    coefficients,
                    junction(qds, self.model.cds),
                    slot::QDS,
                )?;
                gds += geq;
                cqds = cq;
                cd += cqds;
                if initial_transient {
                    context.states.patch_state1(slot::CQGS, cqgs)?;
                    context.states.patch_state1(slot::CQGD, cqgd)?;
                    context.states.patch_state1(slot::CQDS, cqds)?;
                }
            }
        }
        point = Point {
            cg,
            cd,
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
            Start::Limited { cghat, cdhat } => {
                context
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
            (slot::VGSPP, vgspp),
            (slot::VGD, point.vgd),
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
            (slot::GGDPP, ggdpp),
            (slot::GMG, point.gmg),
            (slot::GMD, point.gmd),
            (slot::QGS, qgs),
            (slot::CQGS, cgspp),
            (slot::QGD, qgd),
            (slot::CQGD, cgdpp),
            (slot::QDS, qds),
            (slot::CQDS, cqds),
        ] {
            context.states.set(index, value)?;
        }
        let n = self.nodes;
        let (ggs, ggd, gmg, gmd) = (point.ggs, point.ggd, point.gmg, point.gmd);
        let (gm, gds) = (point.gm, point.gds);
        let (gdpr, gspr, ggate, tgi, tgf) = (
            derived.gdpr,
            derived.gspr,
            derived.ggate,
            derived.tgi,
            derived.tgf,
        );
        let ceqgd = pol
            * (point.cgd + cgdpp
                - ggd * point.vgd
                - gmg * point.vgs
                - gmd * point.vds
                - ggdpp * vgdpp);
        let ceqgs = pol * (point.cgs + cgspp - ggs * point.vgs - ggspp * vgspp);
        let cdreq = pol * (point.cd + point.cgd + cgdpp - gds * point.vds - gm * point.vgs);
        context.stamp_rhs(n.gp, m * (-ceqgs - ceqgd))?;
        context.stamp_rhs(
            n.dp,
            m * (-cdreq + pol * (point.cgd - ggd * point.vgd - gmg * point.vgs - gmd * point.vds)),
        )?;
        context.stamp_rhs(n.dpp, m * pol * (cgdpp - ggdpp * vgdpp))?;
        context.stamp_rhs(n.sp, m * (cdreq + pol * (point.cgs - ggs * point.vgs)))?;
        context.stamp_rhs(n.spp, m * pol * (cgspp - ggspp * vgspp))?;
        for (row, col, value) in [
            (n.d, n.d, m * gdpr),
            (n.s, n.s, m * gspr),
            (n.gp, n.gp, m * (ggd + ggs + ggspp + ggdpp + gmg + ggate)),
            (n.dp, n.dp, m * (gds + ggd - gmd + gdpr + tgf)),
            (n.sp, n.sp, m * (gds + gm + ggs + gspr + tgi)),
            (n.spp, n.spp, m * (tgi + ggspp)),
            (n.dpp, n.dpp, m * (tgf + ggdpp)),
            (n.d, n.dp, -m * gdpr),
            (n.dp, n.d, -m * gdpr),
            (n.s, n.sp, -m * gspr),
            (n.sp, n.s, -m * gspr),
            (n.gp, n.dp, m * (-ggd + gmd)),
            (n.dp, n.gp, m * (gm - ggd - gmg)),
            (n.gp, n.sp, -m * (ggs + gmg + gmd)),
            (n.sp, n.gp, m * (-ggs - gm)),
            (n.dp, n.sp, m * (-gds - gm + gmg + gmd)),
            (n.sp, n.dp, -m * gds),
            (n.sp, n.spp, -m * tgi),
            (n.spp, n.sp, -m * tgi),
            (n.gp, n.spp, -m * ggspp),
            (n.spp, n.gp, -m * ggspp),
            (n.dp, n.dpp, -m * tgf),
            (n.dpp, n.dp, -m * tgf),
            (n.gp, n.dpp, -m * ggdpp),
            (n.dpp, n.gp, -m * ggdpp),
            (n.g, n.g, m * ggate),
            (n.g, n.gp, -m * ggate),
            (n.gp, n.g, -m * ggate),
        ] {
            context.stamp(row, col, value)?;
        }
        Ok(())
    }

    /// `hfetacl.c`: the bias conductances plus `j omega` times the gate
    /// capacitances, with C's `kappa`/`fgds`/`delf` correction on `gds`.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let derived = self.derived(context.model_context.temperature)?;
        let row = |node: NodeId| context.unknowns.node_row(node);
        let voltage = |node: NodeId| row(node).and_then(|r| bias.get(r)).unwrap_or(0.);
        let pol = self.model.pol;
        let vgs = pol * (voltage(self.nodes.gp) - voltage(self.nodes.sp));
        let vgd = pol * (voltage(self.nodes.gp) - voltage(self.nodes.dp));
        let m = self.instance.m;
        let point = self.point(&derived, vgs, vgd, context.model_context.gmin)?;
        let (gm, ggs, ggd, ggspp, ggdpp) = (point.gm, point.ggs, point.ggd, 0., 0.);
        let mut gds = point.gds;
        if self.model.kappa != 0. && derived.delf != 0. {
            let f = context.model_context.frequency;
            gds *= 1. + 0.5 * self.model.kappa * (1. + ((f - derived.fgds) / derived.delf).tanh());
        }
        let n = self.nodes;
        let (gdpr, gspr, ggate, tgi, tgf) = (
            derived.gdpr,
            derived.gspr,
            derived.ggate,
            derived.tgi,
            derived.tgf,
        );
        for (a, b, value) in [
            (n.d, n.d, m * gdpr),
            (n.s, n.s, m * gspr),
            (n.gp, n.gp, m * (ggd + ggs + ggspp + ggdpp + ggate)),
            (n.dp, n.dp, m * (gds + ggd + gdpr + tgf)),
            (n.sp, n.sp, m * (gds + gm + ggs + gspr + tgi)),
            (n.spp, n.spp, m * (tgi + ggspp)),
            (n.dpp, n.dpp, m * (tgf + ggdpp)),
            (n.d, n.dp, -m * gdpr),
            (n.dp, n.d, -m * gdpr),
            (n.s, n.sp, -m * gspr),
            (n.sp, n.s, -m * gspr),
            (n.gp, n.dp, m * -ggd),
            (n.dp, n.gp, m * (gm - ggd)),
            (n.gp, n.sp, m * -ggs),
            (n.sp, n.gp, m * (-ggs - gm)),
            (n.dp, n.sp, m * (-gds - gm)),
            (n.sp, n.dp, m * -gds),
            (n.sp, n.spp, -m * tgi),
            (n.spp, n.sp, -m * tgi),
            (n.gp, n.spp, -m * ggspp),
            (n.spp, n.gp, -m * ggspp),
            (n.dp, n.dpp, -m * tgf),
            (n.dpp, n.dp, -m * tgf),
            (n.gp, n.dpp, -m * ggdpp),
            (n.dpp, n.gp, -m * ggdpp),
            (n.g, n.g, m * ggate),
            (n.g, n.gp, -m * ggate),
            (n.gp, n.g, -m * ggate),
        ] {
            if let (Some(r), Some(c)) = (row(a), row(b)) {
                context.system.a.add(r, c, value)?;
            }
        }
        // C's `Ptr + 1` entries: `xgs = qgs omega` and `xgd = qgd omega`,
        // where a small-signal load stores the capacitances in those slots.
        context.nodal([n.gp, n.spp], m * point.capgs, true)?;
        context.nodal([n.gp, n.dpp], m * point.capgd, true)?;
        Ok(())
    }
    /// `hfetpzl.c` is `hfetacl.c` with `s` in place of `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
}
