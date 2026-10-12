//! Shichman-Hodges JFET level 1 with the Sydney University doping-tail term
//! (`jfet`).
//!
//! C references, read as behaviour only:
//!
//! - `jfet/jfet.c` (`JFETpTable`, `JFETmPTable`): the instance and model
//!   setters; `jfet/jfetpar.c`/`jfetmpar.c` apply them in card order;
//! - `jfet/jfetset.c` (`JFETsetup`): model/instance defaults and the internal
//!   drain/source nodes created for nonzero RD/RS;
//! - `jfet/jfettemp.c` (`JFETtemp`): temperature scaling of IS (`XTI`, `EG`),
//!   PB, CGS/CGD, VTO (`TCV`/`VTOTC`) and BETA (`BEX`/`BETATCE`), the `f1`,
//!   `f2`, `f3` depletion-charge coefficients, `B`'s `bFac` and `vcrit`;
//! - `jfet/jfetload.c` (`JFETload`): gate-source/gate-drain diodes with C's
//!   cubic reverse continuation below `-3 N Vt`, the Sydney channel current in
//!   normal and inverse mode, gate depletion charges, `MODEINITJCT` start
//!   voltages and `DEVpnjlim`/`DEVfetlim` limiting;
//! - `jfet/jfetacld.c` (`JFETacLoad`), `jfet/jfetpzld.c` (`JFETpzLoad`) and
//!   `jfet/jfettrun.c` (`JFETtrunc`);
//! - `jfet/jfetask.c` (`JFETask`) for the observable quantities.
//!
//! The load, the AC load and the temperature update follow C operation by
//! operation (operand grouping, `m` scaling, stamp order), so the port is
//! bit-identical to a C build without fused multiply-adds.
//!
//! # Deliberate divergences
//!
//! - Bypass (`.options bypass`, off by default in ngspice) is not ported.
//! - An `off` instance held at zero in `MODEINITFIX` is judged by the
//!   current-prediction test of the devices with a `DEVconvTest`, where
//!   `jfetload.c` always reports it nonconvergent and C reaches the
//!   operating point only through `OPtran` (`optran.c`, not ported); see
//!   [`Start::HeldOff`].
//! - C limits `FC > 0.95` to 0.95 with a warning (`jfettemp.c`); the port
//!   rejects such a model instead of altering it.
//! - The AC and pole-zero loads evaluate the `MODEINITSMSIG` conductances
//!   and capacitances at the bias point, which equals C's `CKTstate0`
//!   voltages when the converged load limited nothing (as for the BJT and
//!   MOS1). `jfetpzld.c` adds the conductance and `s` times the
//!   capacitance to the real part in two steps; the port's pole-zero load
//!   reuses the AC grouping.
//!
//! `.noise`, `.disto` and `.sens` of a JFET are not ported and fail with an
//! explicit error naming `jfetnoi.c`, `jfetdist.c` and the sensitivity
//! framework rather than contributing nothing.

use crate::devices::limiting;
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::devices::{
    AnalysisMode, Device, DeviceState, IterationPhase, LinearContext, ModelContext, ModelFamily,
    ResolvedModel, StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ParameterAssignment, ParameterKind};
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// C `CONSTboltz` (J/K).
pub(super) const BOLTZMANN: Real = 1.38064852e-23;
/// C `CHARGE` (C).
pub(super) const CHARGE: Real = 1.6021766208e-19;
/// C `CONSTKoverQ`.
pub(super) const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// C `CONSTCtoK`.
pub(super) const CELSIUS_TO_KELVIN: Real = 273.15;
/// C `REFTEMP` (27 degrees Celsius).
pub(super) const REFTEMP: Real = 27. + CELSIUS_TO_KELVIN;
/// `jfettemp.c` limits FC to this value (the port rejects larger values).
pub(super) const MAX_FC: Real = 0.95;

/// Slots of the state vector: C's `JFETnumStates` layout (`jfetdefs.h`),
/// which `JFET2numStates` (`jfet2defs.h`) extends.
pub(super) mod slot {
    /// Limited `vgs` and `vgd` of the load (C `JFETvgs`, `JFETvgd`).
    pub(in crate::devices) const VGS: usize = 0;
    pub(in crate::devices) const VGD: usize = 1;
    /// Gate current, drain current (minus the gate-drain diode) and
    /// gate-drain current of the load (C `JFETcg`, `JFETcd`, `JFETcgd`).
    pub(in crate::devices) const CG: usize = 2;
    pub(in crate::devices) const CD: usize = 3;
    pub(in crate::devices) const CGD: usize = 4;
    /// Channel and gate conductances (C `JFETgm`, `JFETgds`, `JFETggs`,
    /// `JFETggd`), including the charge companions in transient.
    pub(in crate::devices) const GM: usize = 5;
    pub(in crate::devices) const GDS: usize = 6;
    pub(in crate::devices) const GGS: usize = 7;
    pub(in crate::devices) const GGD: usize = 8;
    /// Gate-source charge (C `JFETqgs`) and its derivative (`JFETcqgs`).
    pub(in crate::devices) const QGS: usize = 9;
    /// Gate-drain charge (C `JFETqgd`) and its derivative (`JFETcqgd`).
    pub(in crate::devices) const QGD: usize = 11;
    /// Level 1's slot count (`JFETnumStates`).
    pub(in crate::devices) const COUNT: usize = 13;
}

pub(super) const fn p(name: &'static str, unit: U, domain: D, default: Option<Real>) -> P {
    P {
        name,
        unit,
        domain,
        default,
    }
}

/// Model setters of `JFETmPTable` with the defaults of `jfetset.c`. Setters
/// whose presence changes `jfettemp.c` (`XTI`, `VTOTC`, `BETATCE`) and the
/// two spellings of VTO have no schema default.
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("beta", U::AmperePerVoltSquared, D::NonNegative, Some(1e-4)),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.)),
    p("rd", U::Ohm, D::NonNegative, Some(0.)),
    p("rs", U::Ohm, D::NonNegative, Some(0.)),
    p("cgs", U::Farad, D::NonNegative, Some(0.)),
    p("cgd", U::Farad, D::NonNegative, Some(0.)),
    p("pb", U::Volt, D::Positive, Some(1.)),
    p("is", U::Ampere, D::Positive, Some(1e-14)),
    p("n", U::Dimensionless, D::Positive, Some(1.)),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("b", U::Dimensionless, D::Finite, Some(1.)),
    p("tnom", U::Celsius, D::Temperature, None),
    p("tcv", U::VoltPerKelvin, D::Finite, Some(0.)),
    p("vtotc", U::VoltPerKelvin, D::Finite, None),
    p("bex", U::Dimensionless, D::Finite, Some(0.)),
    p("betatce", U::Dimensionless, D::Finite, None),
    p("xti", U::Dimensionless, D::Finite, None),
    p("eg", U::ElectronVolt, D::Finite, Some(1.11)),
    // jfetnoi.c inputs: accepted for model compatibility; `.noise` itself is
    // refused until jfetnoi.c is ported.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
    p("nlev", U::Dimensionless, D::Finite, Some(2.)),
    p("gdsnoi", U::Dimensionless, D::Finite, Some(1.)),
];

/// Scalar instance setters of `JFETpTable` (`jfetset.c`/`jfettemp.c`
/// defaults). `ic-vds`/`ic-vgs` and `off` are split off first.
pub(super) const INSTANCE: &[P] = &[
    p("area", U::Dimensionless, D::Positive, Some(1.)),
    p("m", U::Dimensionless, D::Positive, Some(1.)),
    p("temp", U::Celsius, D::Temperature, None),
    p("dtemp", U::Celsius, D::Finite, Some(0.)),
];

/// The `IC` vector components (`jfetpar.c`): `IC-VDS`, then `IC-VGS`.
pub(super) const IC_COMPONENTS: [&str; 2] = ["ic-vds", "ic-vgs"];

pub(super) fn required(values: &ScalarValues, name: &str) -> SpiceResult<Real> {
    values
        .get(name)
        .map(|v| v.value)
        .ok_or_else(|| SpiceError::circuit(format!("missing JFET schema default {name}")))
}

/// Validated model card. `None` records C's "not given".
#[derive(Debug, Clone, Copy)]
struct Model {
    /// C `JFETtype`: +1 for NJF, -1 for PJF.
    pol: Real,
    vto: Real,
    beta: Real,
    lambda: Real,
    rd: Real,
    rs: Real,
    cgs: Real,
    cgd: Real,
    pb: Real,
    is: Real,
    n: Real,
    fc: Real,
    b: Real,
    tnom: Option<Real>,
    tcv: Real,
    vtotc: Option<Real>,
    bex: Real,
    betatce: Option<Real>,
    xti: Option<Real>,
    eg: Real,
}

/// Validated instance setters.
#[derive(Debug, Clone, Copy)]
pub(super) struct Instance {
    pub(super) area: Real,
    pub(super) m: Real,
    /// Instance temperature in Celsius, when given.
    pub(super) temp: Option<Real>,
    pub(super) dtemp: Real,
}

/// Every temperature- and area-dependent quantity of one load
/// (`jfettemp.c` and the start of `jfetload.c`).
#[derive(Debug, Clone, Copy)]
struct Operating {
    /// `kT/q` at the device temperature (limiting and `vcrit`).
    vt: Real,
    /// `N kT/q`: the gate diodes' thermal voltage (`vt_temp`).
    vt_n: Real,
    /// C `JFETvcrit` (from the per-device saturation current).
    vcrit: Real,
    /// C `JFETtThreshold`.
    vto: Real,
    /// `JFETtBeta * area`.
    beta: Real,
    /// `JFETtSatCur * area`.
    csat: Real,
    /// `JFETdrainConduct * area`, `JFETsourceConduct * area` (`gdpr`, `gspr`).
    gdpr: Real,
    gspr: Real,
    /// C `JFETtGatePot`.
    pb: Real,
    /// `JFETtCGS * area`, `JFETtCGD * area`.
    czgs: Real,
    czgd: Real,
    /// C `JFETcorDepCap`, `JFETf1`, model `JFETf2`, `JFETf3` and `JFETbFac`.
    cor_dep_cap: Real,
    f1: Real,
    f2: Real,
    f3: Real,
    bfac: Real,
}

/// The channel at one normalized bias (`jfetload.c`, Sydney University
/// formulation): drain current from d' to s' and its partials with respect
/// to `vgs` (at fixed `vds`) and `vds` (at fixed `vgs`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Channel {
    current: Real,
    gm: Real,
    gds: Real,
}

/// `jfetload.c`'s per-device quantities at normalized `vgs`, `vgd`, before
/// any charge integration, in C's names.
#[derive(Debug, Clone, Copy)]
struct Eval {
    vgs: Real,
    vgd: Real,
    vds: Real,
    /// Total gate current `cg = cgs + cgd` (with `gmin`).
    cg: Real,
    /// Gate-drain diode current (with `gmin`).
    cgd: Real,
    ggs: Real,
    ggd: Real,
    cdrain: Real,
    gm: Real,
    gds: Real,
    /// `cd = cdrain - cgd`.
    cd: Real,
    qgs: Real,
    capgs: Real,
    qgd: Real,
    capgd: Real,
}

/// One gate diode of `jfetload.c`: current and conductance at `v`, with
/// C's cubic reverse continuation below `-3 vt_temp` and `gmin`.
fn gate_diode(v: Real, vt_temp: Real, csat: Real, gmin: Real) -> (Real, Real) {
    if v < -3. * vt_temp {
        let mut arg = 3. * vt_temp / (v * std::f64::consts::E);
        arg = arg * arg * arg;
        (-csat * (1. + arg) + gmin * v, csat * 3. * arg / v + gmin)
    } else {
        let ev = (v / vt_temp).exp();
        (csat * (ev - 1.) + gmin * v, csat * ev / vt_temp + gmin)
    }
}

impl Model {
    /// `jfetload.c`'s channel current and partials at normalized voltages.
    fn channel(&self, op: &Operating, vgs: Real, vgd: Real) -> Channel {
        let (b, lambda, beta) = (self.b, self.lambda, op.beta);
        let vds = vgs - vgd;
        let off = Channel {
            current: 0.,
            gm: 0.,
            gds: 0.,
        };
        if vds >= 0. {
            let vgst = vgs - op.vto;
            if vgst <= 0. {
                return off;
            }
            let betap = beta * (1. + lambda * vds);
            if vgst >= vds {
                // Normal mode, linear region.
                let apart = 2. * b + 3. * op.bfac * (vgst - vds);
                let cpart = vds * (vds * (op.bfac * vds - b) + vgst * apart);
                Channel {
                    current: betap * cpart,
                    gm: betap * vds * (apart + 3. * op.bfac * vgst),
                    gds: betap * (vgst - vds) * apart + beta * lambda * cpart,
                }
            } else {
                // Normal mode, saturation region.
                let bfac = vgst * op.bfac;
                let cpart = vgst * vgst * (b + bfac);
                Channel {
                    current: betap * cpart,
                    gm: betap * vgst * (2. * b + 3. * bfac),
                    gds: lambda * beta * cpart,
                }
            }
        } else {
            let vgdt = vgd - op.vto;
            if vgdt <= 0. {
                return off;
            }
            let betap = beta * (1. - lambda * vds);
            if vgdt + vds >= 0. {
                // Inverse mode, linear region.
                let apart = 2. * b + 3. * op.bfac * (vgdt + vds);
                let cpart = vds * (-vds * (-op.bfac * vds - b) + vgdt * apart);
                let gm = betap * vds * (apart + 3. * op.bfac * vgdt);
                Channel {
                    current: betap * cpart,
                    gm,
                    gds: betap * (vgdt + vds) * apart - beta * lambda * cpart - gm,
                }
            } else {
                // Inverse mode, saturation region.
                let bfac = vgdt * op.bfac;
                let gm = -betap * vgdt * (2. * b + 3. * bfac);
                let cpart = vgdt * vgdt * (b + bfac);
                Channel {
                    current: -betap * cpart,
                    gm,
                    gds: lambda * beta * cpart - gm,
                }
            }
        }
    }
}

impl Operating {
    /// Gate depletion charge and capacitance at a normalized junction
    /// voltage for zero-bias capacitance `cz` (`jfetload.c`, grading 1/2 with
    /// linear continuation above `FC * PB`).
    fn charge(&self, v: Real, cz: Real) -> (Real, Real) {
        let twop = 2. * self.pb;
        if v < self.cor_dep_cap {
            let sarg = (1. - v / self.pb).sqrt();
            (twop * cz * (1. - sarg), cz / sarg)
        } else {
            let czf2 = cz / self.f2;
            let fcpb2 = self.cor_dep_cap * self.cor_dep_cap;
            (
                cz * self.f1
                    + czf2 * (self.f3 * (v - self.cor_dep_cap) + (v * v - fcpb2) / (twop + twop)),
                czf2 * (self.f3 + v / twop),
            )
        }
    }
}

/// JFET level 1 with series resistance, gate depletion charge and
/// temperature scaling.
#[derive(Debug)]
pub struct Jfet {
    name: String,
    /// External d, g, s followed by any internal source/drain nodes.
    terminals: Vec<NodeId>,
    /// d', g, s': the nodes the intrinsic device sees.
    inner: [NodeId; 3],
    model: Model,
    instance: Instance,
    /// `OFF` and `IC-VDS`/`IC-VGS` (C `JFEToff`, `JFETicVDS`, `JFETicVGS`).
    initial: crate::devices::initial::InstanceInitial,
}

impl Jfet {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 1 || i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: JFET needs level 1 and three terminals",
                i.name
            )));
        }
        let (initial, setters) = crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)?;
        let (pol, scalars) = card_scalars(resolved);
        let m = ScalarSchema { parameters: MODEL }.validate(scalars, &resolved.card().location)?;
        let get = |name: &str| m.get(name).map(|v| v.value);
        // JFET_MOD_VTO has the IOP spelling `vt0` and the IOPR alias `vto`;
        // setters apply in card order, so the later one wins.
        let vto = last_setter(&m, &["vto", "vt0"]).unwrap_or(-2.);
        let model = Model {
            pol,
            vto,
            beta: required(&m, "beta")?,
            lambda: required(&m, "lambda")?,
            rd: required(&m, "rd")?,
            rs: required(&m, "rs")?,
            cgs: required(&m, "cgs")?,
            cgd: required(&m, "cgd")?,
            pb: required(&m, "pb")?,
            is: required(&m, "is")?,
            n: required(&m, "n")?,
            fc: required(&m, "fc")?,
            b: required(&m, "b")?,
            tnom: get("tnom"),
            tcv: required(&m, "tcv")?,
            vtotc: get("vtotc"),
            bex: required(&m, "bex")?,
            betatce: get("betatce"),
            xti: get("xti"),
            eg: required(&m, "eg")?,
        };
        check_fc(&i.name, model.fc)?;
        let instance = instance_setters(&setters, &i.location)?;
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
        (device.terminals, device.inner) = intern_nodes(i, nodes, model.rd, model.rs)?;
        Ok(Box::new(device))
    }

    /// `jfettemp.c` for this instance, with `jfetload.c`'s area scaling, in
    /// C's operation order.
    fn operating(&self, context: &ModelContext) -> SpiceResult<Operating> {
        let (model, instance) = (&self.model, &self.instance);
        let (tnom, temp) = temperatures(&self.name, model.tnom, instance, context)?;
        let junction = gate_junction(model.pb, tnom, temp);
        let xfc = (1. - model.fc).ln();
        let f2 = ((1. + 0.5) * xfc).exp();
        let f3 = 1. - model.fc * (1. + 0.5);
        let bfac = (1. - model.b) / (model.pb - model.vto);

        let vt = temp * K_OVER_Q;
        let vtn = vt * model.n;
        let ratio1 = temp / tnom - 1.;
        let saturation = match model.xti {
            Some(xti) => model.is * (ratio1 * model.eg / vtn).exp() * (ratio1 + 1.).powf(xti),
            None => model.is * (ratio1 * model.eg / vtn).exp(),
        };
        let GateJunction {
            pb,
            cjfact,
            cjfact1,
        } = junction;
        let mut tcgs = model.cgs * cjfact;
        let mut tcgd = model.cgd * cjfact;
        tcgs *= cjfact1;
        tcgd *= cjfact1;
        let vto = match model.vtotc {
            Some(vtotc) => model.vto + vtotc * (temp - tnom),
            None => model.vto - model.tcv * (temp - tnom),
        };
        let beta = match model.betatce {
            Some(betatce) => model.beta * 1.01_f64.powf(betatce * (temp - tnom)),
            None => model.beta * (temp / tnom).powf(model.bex),
        };
        let area = instance.area;
        let [gdpr, gspr] = series_conductances(area, model.rd, model.rs);
        let operating = Operating {
            vt,
            vt_n: vtn,
            vcrit: limiting::critical_voltage(vt, saturation),
            vto,
            beta: beta * area,
            csat: saturation * area,
            gdpr,
            gspr,
            pb,
            czgs: tcgs * area,
            czgd: tcgd * area,
            cor_dep_cap: model.fc * pb,
            f1: pb * (1. - ((1. - 0.5) * xfc).exp()) / (1. - 0.5),
            f2,
            f3,
            bfac,
        };
        let finite = [
            operating.vt_n,
            operating.vcrit,
            operating.vto,
            operating.beta,
            operating.csat,
            operating.czgs,
            operating.czgd,
            operating.cor_dep_cap,
            operating.f1,
            operating.f2,
            operating.f3,
            operating.bfac,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !(finite && pb.is_finite() && pb > 0.)
            || operating.csat <= 0.
            || operating.beta < 0.
            || operating.czgs < 0.
            || operating.czgd < 0.
        {
            return Err(SpiceError::circuit(format!(
                "{}: JFET parameters are out of range at {temp:.2} K (nonfinite value, \
                 PB equal to VTO, or nonpositive PB/IS after temperature scaling)",
                self.name
            )));
        }
        Ok(operating)
    }

    /// `jfetload.c` from `vds = vgs - vgd` to the charges, at normalized
    /// `vgs`, `vgd`, before any charge integration. Every expression keeps
    /// C's operation order.
    fn evaluate(&self, op: &Operating, vgs: Real, vgd: Real, gmin: Real) -> SpiceResult<Eval> {
        let vds = vgs - vgd;
        let (cgs, ggs) = gate_diode(vgs, op.vt_n, op.csat, gmin);
        let (cgd, ggd) = gate_diode(vgd, op.vt_n, op.csat, gmin);
        let cg = cgs + cgd;
        let channel = self.model.channel(op, vgs, vgd);
        let (qgs, capgs) = op.charge(vgs, op.czgs);
        let (qgd, capgd) = op.charge(vgd, op.czgd);
        let e = Eval {
            vgs,
            vgd,
            vds,
            cg,
            cgd,
            ggs,
            ggd,
            cdrain: channel.current,
            gm: channel.gm,
            gds: channel.gds,
            cd: channel.current - cgd,
            qgs,
            capgs,
            qgd,
            capgd,
        };
        let values = [
            e.cg, e.cgd, e.ggs, e.ggd, e.cdrain, e.gm, e.gds, e.qgs, e.capgs, e.qgd, e.capgd,
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(SpiceError::Numerical {
                context: format!("JFET {}", self.name),
                message: "nonfinite channel/junction equations".into(),
            });
        }
        Ok(e)
    }

    /// The `uic` initial conditions `[IC-VDS, IC-VGS]`; see
    /// [`start_conditions`].
    fn start_conditions(&self, voltage: impl Fn(NodeId) -> Real) -> [Real; 2] {
        start_conditions(&self.terminals, &self.initial, voltage)
    }

    /// Normalized `[vgs, vgd]` from physical node voltages of d', g, s'.
    fn normalized(&self, v: [Real; 3]) -> [Real; 2] {
        let pol = self.model.pol;
        [pol * (v[1] - v[2]), pol * (v[1] - v[0])]
    }

    /// The bias-dependent ask quantities of `jfetask.c` at the solution
    /// `voltage` (no limiting): `vgs`, `vgd` and the multiplicity-scaled
    /// `gm`, `gds`, `ggs`, `ggd` and `igd` (normalized polarity, as C stores
    /// them). In transient C adds the integration companion to `ggs`, `ggd`
    /// and `igd`, so those are refused there.
    fn operating_ask(
        &self,
        key: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        transient: bool,
    ) -> SpiceResult<Option<Real>> {
        if !matches!(key, "vgs" | "vgd" | "gm" | "gds" | "ggs" | "ggd" | "igd") {
            return Ok(None);
        }
        if transient && matches!(key, "ggs" | "ggd" | "igd") {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: transient @{}[{key}] (C adds the charge companion)",
                    self.name, self.name
                ),
                "src/spicelib/devices/jfet/jfetask.c, jfetload.c",
            ));
        }
        let op = self.operating(context)?;
        let [vgs, vgd] = self.normalized(self.inner.map(voltage));
        let e = self.evaluate(&op, vgs, vgd, context.gmin)?;
        let m = self.instance.m;
        Ok(Some(match key {
            "vgs" => vgs,
            "vgd" => vgd,
            "gm" => m * e.gm,
            "gds" => m * e.gds,
            "ggs" => m * e.ggs,
            "ggd" => m * e.ggd,
            _ => m * e.cgd,
        }))
    }
}

// Pieces shared with the Parker-Skellern level 2 (`jfet2/`), whose setup,
// temperature, instance and load skeleton C copied from `jfet/`.

/// The polarity and the scalar setters of a JFET model card: the
/// `JFET_MOD_NJF`/`JFET_MOD_PJF` (`JFET2_MOD_NJF`/`PJF`) tail flags set the
/// type in card order, starting from the base (`.model jm njf(pjf)` is a
/// PJF); `level` is consumed by the resolver.
pub(super) fn card_scalars<'a>(
    resolved: &'a ResolvedModel<'_>,
) -> (Real, Vec<&'a ParameterAssignment>) {
    let mut pol = if resolved.family() == ModelFamily::Pjf {
        -1.
    } else {
        1.
    };
    let mut scalars = Vec::new();
    for parameter in &resolved.card().parameters {
        match (
            parameter.kind == ParameterKind::Flag,
            parameter.name.as_str(),
        ) {
            (_, name) if name.eq_ignore_ascii_case("level") => {}
            (true, "njf") => pol = 1.,
            (true, "pjf") => pol = -1.,
            _ => scalars.push(parameter),
        }
    }
    (pol, scalars)
}

/// The value of whichever of the alias spellings `names` (one C parameter
/// id) was set last in card order, `None` when none was given.
pub(super) fn last_setter(values: &ScalarValues, names: &[&str]) -> Option<Real> {
    names
        .iter()
        .filter_map(|name| values.get(name))
        .max_by_key(|v| v.location.as_ref().map(|l| (l.line, l.column)))
        .map(|v| v.value)
}

/// `jfettemp.c`/`jfet2temp.c` limit FC to 0.95 with a warning; the port
/// rejects such a model instead of altering it.
pub(super) fn check_fc(name: &str, fc: Real) -> SpiceResult<()> {
    if fc > MAX_FC {
        return Err(SpiceError::circuit(format!(
            "{name}: JFET FC={fc} exceeds {MAX_FC} (jfettemp.c would clamp it with a warning)"
        )));
    }
    Ok(())
}

/// The validated scalar instance setters (`AREA`, `M`, `TEMP`, `DTEMP`).
pub(super) fn instance_setters(
    setters: &[ParameterAssignment],
    owner: &crate::primitives::SourceLoc,
) -> SpiceResult<Instance> {
    let v = ScalarSchema {
        parameters: INSTANCE,
    }
    .validate(setters, owner)?;
    Ok(Instance {
        area: required(&v, "area")?,
        m: required(&v, "m")?,
        temp: v.get("temp").map(|v| v.value),
        dtemp: required(&v, "dtemp")?,
    })
}

/// Interns the external d, g, s and the internal prime nodes of a nonzero
/// `RS`, then `RD` (`jfetset.c`/`jfet2set.c` create the source prime node
/// first): returns the terminal list and the intrinsic d', g, s'.
pub(super) fn intern_nodes(
    i: &DeviceInstance,
    nodes: &mut NodeTable,
    rd: Real,
    rs: Real,
) -> SpiceResult<(Vec<NodeId>, [NodeId; 3])> {
    let mut staged = nodes.clone();
    let external: Vec<NodeId> = i.nodes.iter().map(|n| staged.intern(n)).collect();
    let mut terminals = external.clone();
    let mut inner = [external[0], external[1], external[2]];
    for (index, resistance, suffix) in [(2, rs, "source"), (0, rd, "drain")] {
        if resistance != 0. {
            let name = format!("{}#{suffix}", i.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "JFET internal-node name collision: {name}"
                )));
            }
            let prime = staged.intern(&name);
            staged.set_kind(prime, NodeKind::Internal);
            terminals.push(prime);
            inner[index] = prime;
        }
    }
    *nodes = staged;
    Ok((terminals, inner))
}

/// The model's nominal and the instance's temperature in Kelvin: `TNOM`
/// else the circuit nominal temperature, `TEMP` else the circuit
/// temperature plus `DTEMP`.
pub(super) fn temperatures(
    name: &str,
    tnom: Option<Real>,
    instance: &Instance,
    context: &ModelContext,
) -> SpiceResult<(Real, Real)> {
    let tnom = tnom.unwrap_or(context.nominal_temperature) + CELSIUS_TO_KELVIN;
    // C stores TEMP in kelvin (`jfetpar.c`) and adds DTEMP to the kelvin
    // `CKTtemp` (`jfettemp.c`).
    let temp = match instance.temp {
        Some(t) => t + CELSIUS_TO_KELVIN,
        None => (context.temperature + CELSIUS_TO_KELVIN) + instance.dtemp,
    };
    if !(tnom.is_finite() && tnom > 0. && temp.is_finite() && temp > 0.) {
        return Err(SpiceError::circuit(format!(
            "{name}: invalid JFET temperature"
        )));
    }
    Ok((tnom, temp))
}

/// The temperature-scaled gate junction of `jfettemp.c`/`jfet2temp.c`.
#[derive(Debug, Clone, Copy)]
pub(super) struct GateJunction {
    /// C `tGatePot` at the device temperature.
    pub(super) pb: Real,
    /// `cjfact` (from TNOM) and `cjfact1` (to the device temperature): the
    /// zero-bias capacitances scale by their product.
    pub(super) cjfact: Real,
    pub(super) cjfact1: Real,
}

/// `jfettemp.c`'s band-gap laws for the gate potential `pb` (given at
/// `tnom`) and the capacitance factors at `temp` (both Kelvin).
pub(super) fn gate_junction(pb: Real, tnom: Real, temp: Real) -> GateJunction {
    let band_gap = |kelvin: Real| 1.16 - (7.02e-4 * kelvin * kelvin) / (kelvin + 1108.);
    let reference = 1.1150877 / (BOLTZMANN * (REFTEMP + REFTEMP));
    let vtnom = K_OVER_Q * tnom;
    let fact1 = tnom / REFTEMP;
    let kt1 = BOLTZMANN * tnom;
    let arg1 = -band_gap(tnom) / (kt1 + kt1) + reference;
    let pbfact1 = -2. * vtnom * (1.5 * fact1.ln() + CHARGE * arg1);
    let pbo = (pb - pbfact1) / fact1;
    let gmaold = (pb - pbo) / pbo;
    let cjfact = 1. / (1. + 0.5 * (4e-4 * (tnom - REFTEMP) - gmaold));
    let vt = temp * K_OVER_Q;
    let fact2 = temp / REFTEMP;
    let kt = BOLTZMANN * temp;
    let arg = -band_gap(temp) / (kt + kt) + reference;
    let pbfact = -2. * vt * (1.5 * fact2.ln() + CHARGE * arg);
    let pb = fact2 * pbo + pbfact;
    let gmanew = (pb - pbo) / pbo;
    let cjfact1 = 1. + 0.5 * (4e-4 * (temp - REFTEMP) - gmanew);
    GateJunction {
        pb,
        cjfact,
        cjfact1,
    }
}

/// How `jfetload.c`/`jfet2load.c` obtained the voltages of one load.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Start {
    /// The `uic` initial load (`MODETRANOP | MODEUIC | MODEINITJCT`).
    Conditions,
    /// A load outside a device-limited Newton solve: the supplied solution.
    Exact,
    /// `MODEINITJCT` or `MODEINITTRAN`: fixed voltages with C's `icheck`
    /// still set, so always nonconvergent.
    Fixed,
    /// An `off` instance held at zero in `MODEINITFIX`, with the iterate's
    /// normalized `[vgs, vgd]`. C marks every such load nonconvergent and
    /// leaves `MODEINITFIX` only through its continuation fallbacks, ending
    /// in `OPtran` (`optran.c`), which is not ported. The port instead
    /// judges the held load as the devices with a `DEVconvTest` do: it
    /// converges once the currents at the held voltages agree with their
    /// linear prediction at the iterate.
    HeldOff { raw: [Real; 2] },
    /// `MODEINITPRED`, `MODEINITFLOAT` or `MODEINITFIX`: limited against the
    /// previous state, with C's `icheck` and current predictions.
    Limited {
        icheck: bool,
        cghat: Real,
        cdhat: Real,
    },
}

/// What [`select_junctions`] needs of a device: polarity, `off`, `kT/q`,
/// `vcrit` and the `DEVfetlim` threshold (`JFETtThreshold`, or the model
/// `VTO` for level 2).
#[derive(Debug, Clone, Copy)]
pub(super) struct Junctions {
    pub(super) pol: Real,
    pub(super) off: bool,
    pub(super) vt: Real,
    pub(super) vcrit: Real,
    pub(super) vto: Real,
}

/// `jfetload.c`/`jfet2load.c`'s normalized `[vgs, vgd]` for this load and
/// how they were obtained.
///
/// `raw` is the iterate's (C `CKTrhsOld`) and `ic` the `uic` initial
/// conditions `[IC-VDS, IC-VGS]`. `MODEINITJCT` starts at `-1` (zero for an
/// `off` instance, which is also held at zero in `MODEINITFIX`),
/// `MODEINITTRAN` at the accepted (`CKTstate1`) voltages. `MODEINITPRED`
/// extrapolates the accepted voltages with `DEVpred`'s
/// `(1 + xfact) state1 - xfact state2` and compares with `CKTstate1` (C
/// copies it into `CKTstate0`); other loads compare the iterate with the
/// previous load. The comparison yields C's `cghat` and `cdhat`, then
/// `DEVpnjlim` and `DEVfetlim` limit both voltages.
pub(super) fn select_junctions(
    j: Junctions,
    states: &DeviceState<'_>,
    raw: [Real; 2],
    ic: [Real; 2],
    xfact: Real,
) -> ([Real; 2], Start) {
    if states.initial_conditions() {
        let vds = j.pol * ic[0];
        let vgs = j.pol * ic[1];
        return ([vgs, vgs - vds], Start::Conditions);
    }
    if !states.device_limiting() {
        return (raw, Start::Exact);
    }
    let phase = states.phase();
    let new = match phase {
        IterationPhase::Junction => {
            let v = if j.off { 0. } else { -1. };
            return ([v, v], Start::Fixed);
        }
        IterationPhase::Fix if j.off => return ([0.; 2], Start::HeldOff { raw }),
        IterationPhase::Predict if states.initial_transient() => {
            let state1 = |slot: usize, fallback: Real| states.accepted(1, slot).unwrap_or(fallback);
            return (
                [state1(slot::VGS, raw[0]), state1(slot::VGD, raw[1])],
                Start::Fixed,
            );
        }
        IterationPhase::Predict => [
            states.predicted(slot::VGS, xfact),
            states.predicted(slot::VGD, xfact),
        ],
        IterationPhase::Fix | IterationPhase::Float => raw.map(Some),
    };
    let old = |slot: usize| match phase {
        IterationPhase::Predict => states.accepted(1, slot),
        _ => states.iterate(slot),
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
    .map(|slot| old(slot).filter(|v| v.is_finite()))
    .collect();
    let Some(&[vgs0, vgd0, cg0, cd0, gm0, gds0, ggs0, ggd0]) = previous.as_deref() else {
        return ([vgs, vgd], Start::Exact);
    };
    let delvgs = vgs - vgs0;
    let delvgd = vgd - vgd0;
    let delvds = delvgs - delvgd;
    let cghat = cg0 + ggd0 * delvgd + ggs0 * delvgs;
    let cdhat = cd0 + gm0 * delvgs + gds0 * delvds - ggd0 * delvgd;
    let gs = limiting::pnjlim(vgs, vgs0, j.vt, j.vcrit);
    let gd = limiting::pnjlim(vgd, vgd0, j.vt, j.vcrit);
    let vgs = limiting::fetlim(gs.value, vgs0, j.vto);
    let vgd = limiting::fetlim(gd.value, vgd0, j.vto);
    (
        [vgs, vgd],
        Start::Limited {
            icheck: gs.limited || gd.limited,
            cghat,
            cdhat,
        },
    )
}

/// The values `jfetload.c`/`jfet2load.c` store and stamp, per device,
/// after the charge companions: C's `vgs`, `vgd`, `vds`, `cg`, `cgd`, `cd`,
/// `gm`, `gds`, `ggs`, `ggd`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Loaded {
    pub(super) vgs: Real,
    pub(super) vgd: Real,
    pub(super) vds: Real,
    pub(super) cg: Real,
    pub(super) cgd: Real,
    pub(super) cd: Real,
    pub(super) gm: Real,
    pub(super) gds: Real,
    pub(super) ggs: Real,
    pub(super) ggd: Real,
}

/// The end of `jfetload.c`/`jfet2load.c`: the convergence check (`icheck`,
/// then the gate and drain currents against their linear prediction from
/// the previous load), the `CKTstate0` writes and the current vector and Y
/// matrix in C's order and arithmetic, every stamp scaled by `m`.
/// `terminals` are the external d, g, s and `inner` the intrinsic d', g, s'.
#[allow(clippy::too_many_arguments)]
pub(super) fn finish_load(
    context: &mut StampContext<'_>,
    start: Start,
    l: &Loaded,
    terminals: &[NodeId],
    inner: [NodeId; 3],
    pol: Real,
    m: Real,
    [gdpr, gspr]: [Real; 2],
) -> SpiceResult<()> {
    let nonconvergent = match start {
        Start::Fixed => true,
        Start::Conditions | Start::Exact => false,
        Start::HeldOff { raw } => {
            let delvgs = raw[0] - l.vgs;
            let delvgd = raw[1] - l.vgd;
            let delvds = delvgs - delvgd;
            let cghat = l.cg + l.ggd * delvgd + l.ggs * delvgs;
            let cdhat = l.cd + l.gm * delvgs + l.gds * delvds - l.ggd * delvgd;
            context
                .states
                .convergence_tolerances()
                .is_some_and(|(reltol, abstol)| {
                    (cghat - l.cg).abs() > reltol * cghat.abs().max(l.cg.abs()) + abstol
                        || (cdhat - l.cd).abs() > reltol * cdhat.abs().max(l.cd.abs()) + abstol
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
                        (cghat - l.cg).abs() >= reltol * cghat.abs().max(l.cg.abs()) + abstol
                            || (cdhat - l.cd).abs() > reltol * cdhat.abs().max(l.cd.abs()) + abstol
                    })
        }
    };
    if nonconvergent {
        context.states.report_nonconvergence()?;
    }
    for (slot, value) in [
        (slot::VGS, l.vgs),
        (slot::VGD, l.vgd),
        (slot::CG, l.cg),
        (slot::CD, l.cd),
        (slot::CGD, l.cgd),
        (slot::GM, l.gm),
        (slot::GDS, l.gds),
        (slot::GGS, l.ggs),
        (slot::GGD, l.ggd),
    ] {
        context.states.set(slot, value)?;
    }
    let t = pol;
    let Loaded {
        vgs,
        vgd,
        vds,
        cg,
        cgd,
        cd,
        gm,
        gds,
        ggs,
        ggd,
    } = *l;
    let ceqgd = t * (cgd - ggd * vgd);
    let ceqgs = t * ((cg - cgd) - ggs * vgs);
    let cdreq = t * ((cd + cgd) - gds * vds - gm * vgs);
    let [d, g, s] = [0, 1, 2].map(|k| terminals[k]);
    let [dp, _, sp] = inner;
    context.stamp_rhs(g, m * (-ceqgs - ceqgd))?;
    context.stamp_rhs(dp, m * (-cdreq + ceqgd))?;
    context.stamp_rhs(sp, m * (cdreq + ceqgs))?;
    for (row, col, value) in [
        (d, dp, -gdpr),
        (g, dp, -ggd),
        (g, sp, -ggs),
        (s, sp, -gspr),
        (dp, d, -gdpr),
        (dp, g, gm - ggd),
        (dp, sp, -gds - gm),
        (sp, g, -ggs - gm),
        (sp, s, -gspr),
        (sp, dp, -gds),
        (d, d, gdpr),
        (g, g, ggd + ggs),
        (s, s, gspr),
        (dp, dp, gdpr + gds + ggd),
        (sp, sp, gspr + gds + gm + ggs),
    ] {
        context.stamp(row, col, m * value)?;
    }
    Ok(())
}

/// The real part of `jfetacld.c`/`jfet2acld.c`: the conductances in C's
/// order and grouping, every stamp scaled by `m`.
#[allow(clippy::too_many_arguments)]
pub(super) fn ac_conductances(
    context: &mut LinearContext<'_>,
    terminals: &[NodeId],
    inner: [NodeId; 3],
    m: Real,
    [gdpr, gspr]: [Real; 2],
    [gm, gds, ggs, ggd]: [Real; 4],
) -> SpiceResult<()> {
    let [d, g, s] = [0, 1, 2].map(|k| terminals[k]);
    let [dp, _, sp] = inner;
    for (row, col, value) in [
        (d, d, m * gdpr),
        (g, g, m * (ggd + ggs)),
        (s, s, m * gspr),
        (dp, dp, m * (gdpr + gds + ggd)),
        (sp, sp, m * (gspr + gds + gm + ggs)),
        (d, dp, -(m * gdpr)),
        (g, dp, -(m * ggd)),
        (g, sp, -(m * ggs)),
        (s, sp, -(m * gspr)),
        (dp, d, -(m * gdpr)),
        (dp, g, m * (-ggd + gm)),
        (dp, sp, m * (-gds - gm)),
        (sp, g, m * (-ggs - gm)),
        (sp, s, -(m * gspr)),
        (sp, dp, -(m * gds)),
    ] {
        if let (Some(r), Some(c)) = (
            context.unknowns.node_row(row),
            context.unknowns.node_row(col),
        ) {
            context.system.a.add(r, c, value)?;
        }
    }
    Ok(())
}

/// The gate capacitances' imaginary part of `jfetacld.c`/`jfet2acld.c`
/// (`xgs = capgs * omega`, `xgd = capgd * omega`), in C's order.
pub(super) fn ac_gate_capacitances(
    context: &mut LinearContext<'_>,
    inner: [NodeId; 3],
    m: Real,
    capgs: Real,
    capgd: Real,
) -> SpiceResult<()> {
    let [dp, g, sp] = inner;
    for (row, col, terms, scale) in [
        (g, g, &[capgd, capgs][..], m),
        (dp, dp, &[capgd][..], m),
        (sp, sp, &[capgs][..], m),
        (g, dp, &[capgd][..], -m),
        (g, sp, &[capgs][..], -m),
        (dp, g, &[capgd][..], -m),
        (sp, g, &[capgs][..], -m),
    ] {
        context.scaled_dynamic_sum(row, col, terms, scale)?;
    }
    Ok(())
}

/// The `uic` initial conditions `[IC-VDS, IC-VGS]`: unset components are
/// the external terminal voltages of the solution (`jfetic.c`,
/// `jfet2ic.c`).
pub(super) fn start_conditions(
    terminals: &[NodeId],
    initial: &crate::devices::initial::InstanceInitial,
    voltage: impl Fn(NodeId) -> Real,
) -> [Real; 2] {
    let [d, g, s] = [0, 1, 2].map(|k| terminals[k]);
    [
        initial.values[0].unwrap_or_else(|| voltage(d) - voltage(s)),
        initial.values[1].unwrap_or_else(|| voltage(g) - voltage(s)),
    ]
}

/// Drain and source series conductances per device (`gdpr`, `gspr`): the
/// model's `1 / R` (`jfettemp.c`) times `area` (`jfetload.c`), zero without
/// an internal node.
pub(super) fn series_conductances(area: Real, rd: Real, rs: Real) -> [Real; 2] {
    let conductance = |r: Real| if r == 0. { 0. } else { (1. / r) * area };
    [conductance(rd), conductance(rs)]
}

/// `jfetask.c`/`jfet2ask.c` scalar asks: `area` (C reports `area * m`),
/// `m`, `temp`, `dtemp`, `ic-vds` and `ic-vgs` (when given).
pub(super) fn observation_parameter(
    p: &Instance,
    initial: &crate::devices::initial::InstanceInitial,
    key: &str,
    context: &ModelContext,
) -> Option<Real> {
    match key {
        "area" => Some(p.area * p.m),
        "m" => Some(p.m),
        "temp" => Some(p.temp.unwrap_or(context.temperature + p.dtemp)),
        "dtemp" => Some(p.dtemp),
        "ic-vds" => initial.values[0],
        "ic-vgs" => initial.values[1],
        _ => None,
    }
}

/// `JFETparam`/`JFET2param` of a swept instance setter, with the instance
/// schema domains; the caller re-derives the temperature dependence.
pub(super) fn swept_instance(
    name: &str,
    instance: &Instance,
    initial: &crate::devices::initial::InstanceInitial,
    parameter: &str,
    value: Real,
) -> SpiceResult<(Instance, crate::devices::initial::InstanceInitial)> {
    use crate::devices::sweep::check_swept;
    let check = |ok: bool, what: &str| check_swept(name, parameter, value, ok, what);
    let mut instance = *instance;
    let mut initial = initial.clone();
    match parameter {
        "area" => {
            check(value > 0., "positive")?;
            instance.area = value;
        }
        "m" => {
            check(value > 0., "positive")?;
            instance.m = value;
        }
        "temp" => {
            check(value + CELSIUS_TO_KELVIN > 0., "above absolute zero")?;
            instance.temp = Some(value);
        }
        "dtemp" => {
            check(true, "finite")?;
            instance.dtemp = value;
        }
        "ic-vds" | "ic-vgs" => {
            check(true, "finite")?;
            initial.values[usize::from(parameter == "ic-vgs")] = Some(value);
        }
        _ => {
            return Err(SpiceError::circuit(format!(
                "{name}: JFET parameter {parameter} cannot be swept"
            )));
        }
    }
    Ok((instance, initial))
}

/// `JFETpTable`/`JFET2pTable` settable reals that `dctrcurv.c`
/// `DCTsetInstParam` re-derives through `JFETtemp`/`JFET2temp`.
pub(super) fn instance_parameter(keyword: &str) -> Option<&'static str> {
    ["area", "m", "temp", "dtemp", "ic-vds", "ic-vgs"]
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(keyword))
}

impl Device for Jfet {
    /// `jfetset.c`: C's device type and `TSTALLOC` order.
    fn c_device_type(&self) -> crate::devices::setup::CDeviceType {
        crate::devices::setup::CDeviceType::Jfet
    }

    /// `jfetset.c` `TSTALLOC` order.
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
    /// `jfetask.c` scalar asks: `area` (C reports `area * m`), `m`, `temp`,
    /// `dtemp`, `ic-vds` and `ic-vgs` (when given).
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        Ok(observation_parameter(
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
    /// `jfetask.c` reports `id`, `ig` and `is` in the device's normalized
    /// polarity (`JFETcd`, `JFETcg` are stored before the `type` factor).
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
    fn state_count(&self) -> usize {
        slot::COUNT
    }
    /// `jfettrun.c`: both gate charges control the timestep.
    fn truncation_slots(&self) -> Vec<usize> {
        vec![slot::QGS, slot::QGD]
    }
    /// `JFETpTable`'s settable reals that `dctrcurv.c` `DCTsetInstParam`
    /// re-derives through `JFETtemp`.
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        instance_parameter(keyword)
    }
    /// `JFETparam` then `JFETtemp`, with the instance schema domains.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let (instance, initial) =
            swept_instance(&self.name, &self.instance, &self.initial, parameter, value)?;
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
    /// `JFETload` (`jfetload.c`) without bypass, which ngspice disables by
    /// default. JFET has no `DEVconvTest`: the load's own `icheck`/`cghat`/
    /// `cdhat` test is its convergence test.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("JFET AC needs small-signal assembly"));
        }
        let op = self.operating(&context.model_context())?;
        let raw = self.normalized(self.inner.map(|n| context.node_voltage(n)));
        let ic = self.start_conditions(|n| context.node_voltage(n));
        let integration = context.integration;
        // C `xfact = CKTdelta / CKTdeltaOld[1]`; one in a `.dc` sweep.
        let xfact = integration.map_or(1., crate::maths::integrator::Coefficients::predictor_ratio);
        let ([vgs, vgd], start) = select_junctions(
            Junctions {
                pol: self.model.pol,
                off: self.initial.off,
                vt: op.vt,
                vcrit: op.vcrit,
                vto: op.vto,
            },
            &context.states,
            raw,
            ic,
            xfact,
        );
        let mut e = self.evaluate(&op, vgs, vgd, context.gmin)?;
        let initial_transient =
            context.states.phase() == IterationPhase::Predict && context.states.initial_transient();

        // Charge storage: C evaluates the charges in transient, `.dc` sweeps,
        // MODEINITSMSIG and the `uic` operating point, and integrates them in
        // transient only; storing them in every load changes no stamp.
        if let Some(coefficients) = integration {
            let AnalysisMode::Transient { dt, .. } = context.mode else {
                return Err(SpiceError::circuit(
                    "JFET charge companion outside transient",
                ));
            };
            if dt != coefficients.dt() {
                return Err(SpiceError::circuit("JFET charge timestep mismatch"));
            }
            if initial_transient {
                context.states.patch_state1(slot::QGS, e.qgs)?;
                context.states.patch_state1(slot::QGD, e.qgd)?;
            }
            let point = |charge, capacitance| JunctionPoint {
                current: 0.,
                conductance: 0.,
                charge,
                capacitance,
            };
            let (geq, cqgs) =
                junction_companion(context, coefficients, point(e.qgs, e.capgs), slot::QGS)?;
            e.ggs += geq;
            e.cg += cqgs;
            let (geq, cqgd) =
                junction_companion(context, coefficients, point(e.qgd, e.capgd), slot::QGD)?;
            e.ggd += geq;
            e.cg += cqgd;
            e.cd -= cqgd;
            e.cgd += cqgd;
            if initial_transient {
                context.states.patch_state1(slot::QGS + 1, cqgs)?;
                context.states.patch_state1(slot::QGD + 1, cqgd)?;
            }
        } else {
            if context.mode.is_transient() {
                return Err(SpiceError::circuit(
                    "JFET transient needs companion integration",
                ));
            }
            context.states.set(slot::QGS + 1, 0.)?;
            context.states.set(slot::QGD + 1, 0.)?;
        }
        context.states.set(slot::QGS, e.qgs)?;
        context.states.set(slot::QGD, e.qgd)?;

        finish_load(
            context,
            start,
            &Loaded {
                vgs: e.vgs,
                vgd: e.vgd,
                vds: e.vds,
                cg: e.cg,
                cgd: e.cgd,
                cd: e.cd,
                gm: e.gm,
                gds: e.gds,
                ggs: e.ggs,
                ggd: e.ggd,
            },
            &self.terminals,
            self.inner,
            self.model.pol,
            self.instance.m,
            [op.gdpr, op.gspr],
        )
    }
    /// `jfetacld.c` after C's `MODEINITSMSIG` load: the conductances and
    /// gate capacitances at the bias, every stamp scaled by `m` and grouped
    /// as C groups it.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let op = self.operating(context.model_context)?;
        let unknowns = context.unknowns;
        let voltage = |n: NodeId| unknowns.node_row(n).and_then(|r| bias.get(r)).unwrap_or(0.);
        let [vgs, vgd] = self.normalized(self.inner.map(voltage));
        let e = self.evaluate(&op, vgs, vgd, context.model_context.gmin)?;
        let m = self.instance.m;
        ac_gate_capacitances(context, self.inner, m, e.capgs, e.capgd)?;
        ac_conductances(
            context,
            &self.terminals,
            self.inner,
            m,
            [op.gdpr, op.gspr],
            [e.gm, e.gds, e.ggs, e.ggd],
        )
    }
    /// `jfetpzld.c` equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(SpiceError::not_yet_ported(
            format!("noise analysis of JFET {}", self.name),
            "src/spicelib/devices/jfet/jfetnoi.c",
        ))
    }
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(SpiceError::not_yet_ported(
            format!("distortion analysis of JFET {}", self.name),
            "src/spicelib/devices/jfet/jfetdset.c, jfetdist.c",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operating(b: Real) -> (Model, Operating) {
        let model = Model {
            pol: 1.,
            vto: -2.,
            beta: 1e-3,
            lambda: 0.02,
            rd: 0.,
            rs: 0.,
            cgs: 2e-12,
            cgd: 1e-12,
            pb: 0.8,
            is: 1e-14,
            n: 1.2,
            fc: 0.5,
            b,
            tnom: None,
            tcv: 0.,
            vtotc: None,
            bex: 0.,
            betatce: None,
            xti: None,
            eg: 1.11,
        };
        let device = Jfet {
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
        (model, device.operating(&ModelContext::default()).unwrap())
    }

    /// gm = dI/dvgs at fixed vds and gds = dI/dvds at fixed vgs, in every
    /// region of both modes, with and without the doping tail.
    #[test]
    fn channel_partials_are_the_current_derivatives() {
        for b in [1., 0.6, 1.4] {
            let (model, op) = operating(b);
            let current = |vgs: Real, vds: Real| model.channel(&op, vgs, vgs - vds).current;
            for vgs in [-2.5, -1.7, -1., 0., 0.3] {
                for vds in [-3., -1.2, -0.4, -0.05, 0.05, 0.4, 1.2, 3.] {
                    let c = model.channel(&op, vgs, vgs - vds);
                    let h = 1e-6;
                    let gm = (current(vgs + h, vds) - current(vgs - h, vds)) / (2. * h);
                    let gds = (current(vgs, vds + h) - current(vgs, vds - h)) / (2. * h);
                    let scale = c.gm.abs().max(c.gds.abs()).max(1e-9);
                    assert!((gm - c.gm).abs() <= 1e-6 * scale, "b={b} {vgs} {vds}: gm");
                    assert!(
                        (gds - c.gds).abs() <= 1e-6 * scale,
                        "b={b} {vgs} {vds}: gds"
                    );
                }
            }
        }
    }

    #[test]
    fn the_channel_is_antisymmetric_under_drain_source_exchange() {
        let (model, op) = operating(0.8);
        for (vgs, vgd) in [(-1., -1.5), (0.2, -2.), (-1.9, -1.95)] {
            let forward = model.channel(&op, vgs, vgd).current;
            let reverse = model.channel(&op, vgd, vgs).current;
            assert!((forward + reverse).abs() <= 1e-15, "{vgs} {vgd}");
        }
    }

    #[test]
    fn gate_charge_is_continuous_and_its_derivative_is_the_capacitance() {
        let (_, op) = operating(1.);
        for v in [-5., -1., 0., 0.3, op.cor_dep_cap, 0.6, 1.] {
            let h = 1e-7;
            let (_, c) = op.charge(v, 2e-12);
            let numerical = (op.charge(v + h, 2e-12).0 - op.charge(v - h, 2e-12).0) / (2. * h);
            assert!((numerical - c).abs() <= 1e-6 * c, "{v}: {numerical} {c}");
        }
        let edge = op.cor_dep_cap;
        let (below, above) = (op.charge(edge - 1e-12, 2e-12), op.charge(edge, 2e-12));
        assert!((below.0 - above.0).abs() <= 1e-9 * above.0.abs());
        assert!((below.1 - above.1).abs() <= 1e-9 * above.1);
    }
}
