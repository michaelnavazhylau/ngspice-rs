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
//! # Deliberate divergences
//!
//! - C's predictor extrapolation (`MODEINITPRED`) and bypass are not ported:
//!   a predicted load limits the iterate against the last accepted voltages
//!   (as for MOS1, see [`crate::devices::limiting`]). Any `DEVfetlim` change
//!   keeps the load nonconvergent; C only flags `DEVpnjlim` steps and its
//!   own current-prediction test.
//! - C limits `FC > 0.95` to 0.95 with a warning (`jfettemp.c`); the port
//!   rejects such a model instead of altering it.
//! - An `off` instance is held at `vgs = vgd = 0` in `MODEINITJCT` and
//!   `MODEINITFIX` through the shared [`Limiter::holds_off`] rule, with the
//!   held load judged by the gate/drain current prediction test MOS1 uses.
//!   `jfetload.c` instead marks every held `MODEINITFIX` load nonconvergent
//!   (its `icheck` starts at 1), so C leaves `MODEINITFIX` only through its
//!   continuation fallbacks; both reach the same unique operating point.
//! - The multiplicity `m` scales every stamp as in `jfetload.c`; the gate
//!   charges kept in the state vector are per-device (unscaled) values, as in
//!   C, so truncation control sees C's charges.
//!
//! `.noise`, `.disto` and `.sens` of a JFET are not ported and fail with an
//! explicit error naming `jfetnoi.c`, `jfetdist.c` and the sensitivity
//! framework rather than contributing nothing.

use crate::devices::limiting::{self, Limiter, Linearization};
use crate::devices::linear::nodal_stamp;
use crate::devices::nonlinear::junction_current;
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::devices::{
    Device, LinearContext, ModelContext, ModelFamily, ResolvedModel, StampContext,
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

/// Slots of the state vector (a subset of C's `JFETnumStates` layout, with
/// every charge directly followed by its derivative).
mod slot {
    /// Gate-source charge (C `JFETqgs`) and its derivative (`JFETcqgs`).
    pub(super) const QGS: usize = 0;
    /// Gate-drain charge (C `JFETqgd`) and its derivative (`JFETcqgd`).
    pub(super) const QGD: usize = 2;
    /// Limited `vgs` and `vgd` of the load (C `JFETvgs`, `JFETvgd`), for the
    /// next load's limiting.
    pub(super) const VGS: usize = 4;
    pub(super) const VGD: usize = 5;
    pub(super) const COUNT: usize = 6;
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
    /// `N kT/q`: the gate diodes' thermal voltage.
    vt_n: Real,
    /// C `JFETvcrit` (from the per-device saturation current).
    vcrit: Real,
    /// C `JFETtThreshold`.
    vto: Real,
    /// `JFETtBeta * area`.
    beta: Real,
    /// `JFETtSatCur * area`.
    csat: Real,
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

/// Everything one normalized bias point loads, per device (before `m`).
#[derive(Debug, Clone, Copy)]
struct Point {
    channel: Channel,
    /// Gate-source and gate-drain junction currents (with `gmin * v`) and
    /// conductances (with `gmin`).
    gs: (Real, Real),
    gd: (Real, Real),
    /// Gate-source and gate-drain depletion charges and capacitances.
    qgs: (Real, Real),
    qgd: (Real, Real),
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

    /// `jfettemp.c` for this instance, with `jfetload.c`'s area scaling.
    fn operating(&self, context: &ModelContext) -> SpiceResult<Operating> {
        let (model, instance) = (&self.model, &self.instance);
        let (tnom, temp) = temperatures(&self.name, model.tnom, instance, context)?;
        let junction = gate_junction(model.pb, tnom, temp);
        let xfc = (1. - model.fc).ln();
        let f2 = (1.5 * xfc).exp();
        let f3 = 1. - model.fc * 1.5;
        let bfac = (1. - model.b) / (model.pb - model.vto);

        let vt = temp * K_OVER_Q;
        let vt_n = vt * model.n;
        let ratio1 = temp / tnom - 1.;
        let mut saturation = model.is * (ratio1 * model.eg / vt_n).exp();
        if let Some(xti) = model.xti {
            saturation *= (ratio1 + 1.).powf(xti);
        }
        let GateJunction {
            pb,
            cjfact,
            cjfact1,
        } = junction;
        let cap = cjfact * cjfact1 * instance.area;
        let vto = match model.vtotc {
            Some(vtotc) => model.vto + vtotc * (temp - tnom),
            None => model.vto - model.tcv * (temp - tnom),
        };
        let beta = match model.betatce {
            Some(betatce) => model.beta * 1.01_f64.powf(betatce * (temp - tnom)),
            None => model.beta * (temp / tnom).powf(model.bex),
        };
        let operating = Operating {
            vt,
            vt_n,
            vcrit: limiting::critical_voltage(vt, saturation),
            vto,
            beta: beta * instance.area,
            csat: saturation * instance.area,
            pb,
            czgs: model.cgs * cap,
            czgd: model.cgd * cap,
            cor_dep_cap: model.fc * pb,
            f1: pb * (1. - (0.5 * xfc).exp()) / 0.5,
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

    /// Every per-device quantity at normalized `vgs`, `vgd` (`jfetload.c`).
    fn point(&self, op: &Operating, vgs: Real, vgd: Real, gmin: Real) -> SpiceResult<Point> {
        let junction = |v: Real| -> SpiceResult<(Real, Real)> {
            let (current, conductance) = junction_current(v, op.vt_n, op.csat)?;
            Ok((current + gmin * v, conductance + gmin))
        };
        let point = Point {
            channel: self.model.channel(op, vgs, vgd),
            gs: junction(vgs)?,
            gd: junction(vgd)?,
            qgs: op.charge(vgs, op.czgs),
            qgd: op.charge(vgd, op.czgd),
        };
        let c = point.channel;
        let values = [
            c.current,
            c.gm,
            c.gds,
            point.gs.0,
            point.gs.1,
            point.gd.0,
            point.gd.1,
            point.qgs.0,
            point.qgs.1,
            point.qgd.0,
            point.qgd.1,
        ];
        if values.iter().any(|v| !v.is_finite()) || point.qgs.1 < 0. || point.qgd.1 < 0. {
            return Err(SpiceError::Numerical {
                context: format!("JFET {}", self.name),
                message: "nonfinite channel/junction equations".into(),
            });
        }
        Ok(point)
    }

    /// Normalized `[vgs, vgd]` of `jfetload.c` for this load; see
    /// [`limit_junctions`] (`DEVfetlim` against `JFETtThreshold`).
    fn limit(
        &self,
        op: &Operating,
        limiter: &mut Limiter,
        states: &crate::devices::DeviceState<'_>,
        raw: [Real; 2],
        ic: [Real; 2],
    ) -> [Real; 2] {
        limit_junctions(
            Junctions {
                pol: self.model.pol,
                off: self.initial.off,
                vt: op.vt,
                vcrit: op.vcrit,
                vto: op.vto,
                slots: [slot::VGS, slot::VGD],
            },
            limiter,
            states,
            raw,
            ic,
        )
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

    /// Drain and source series conductances per device (`jfetload.c`
    /// `gdpr`, `gspr`: `area / R`), zero without an internal node.
    fn series(&self) -> [Real; 2] {
        series_conductances(self.instance.area, self.model.rd, self.model.rs)
    }

    fn series_ports(&self) -> [[NodeId; 2]; 2] {
        series_ports(&self.terminals, self.inner)
    }

    /// The channel partials over d', g, s' (physical and normalized partials
    /// coincide: both the current and the voltages change sign with `type`).
    fn channel_partials(&self, channel: &Channel) -> [(NodeId, Real); 3] {
        let [d, g, s] = self.inner;
        let (gm, gds) = (channel.gm, channel.gds);
        [(d, gds), (g, gm), (s, -gds - gm)]
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
        let point = self.point(&op, vgs, vgd, context.gmin)?;
        let m = self.instance.m;
        Ok(Some(match key {
            "vgs" => vgs,
            "vgd" => vgd,
            "gm" => m * point.channel.gm,
            "gds" => m * point.channel.gds,
            "ggs" => m * point.gs.1,
            "ggd" => m * point.gd.1,
            _ => m * point.gd.0,
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
    let temp = instance
        .temp
        .unwrap_or(context.temperature + instance.dtemp)
        + CELSIUS_TO_KELVIN;
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

/// What [`limit_junctions`] needs of a device: polarity, `off`, `kT/q`,
/// `vcrit`, the `DEVfetlim` threshold and the `vgs`/`vgd` state slots.
#[derive(Debug, Clone, Copy)]
pub(super) struct Junctions {
    pub(super) pol: Real,
    pub(super) off: bool,
    pub(super) vt: Real,
    pub(super) vcrit: Real,
    pub(super) vto: Real,
    pub(super) slots: [usize; 2],
}

/// Normalized `[vgs, vgd]` of `jfetload.c`/`jfet2load.c` for this load: the
/// `uic` initial load evaluates at `type * IC` (`vgd = vgs - vds`, unset
/// components from the external terminals as `jfetic.c` does), the
/// `MODEINITJCT` load at `vgs = vgd = -1` (zero for an `off` instance, which
/// is also held at zero through `MODEINITFIX`), and later loads apply
/// `DEVpnjlim` (`vt = kT/q`, `vcrit`) and then `DEVfetlim` (`vto`) to both
/// junction voltages.
pub(super) fn limit_junctions(
    j: Junctions,
    limiter: &mut Limiter,
    states: &crate::devices::DeviceState<'_>,
    raw: [Real; 2],
    ic: [Real; 2],
) -> [Real; 2] {
    let mode = limiter.mode();
    if mode == Linearization::InitialConditions {
        // C checks MODEINITJCT & MODETRANOP & MODEUIC before `off`.
        let [vds, vgs] = ic.map(|v| j.pol * v);
        return [vgs, vgs - vds];
    }
    if limiter.holds_off(states, j.off) {
        return [0.; 2];
    }
    if mode == Linearization::Initial {
        return [-1., -1.];
    }
    let [Some(vgs_old), Some(vgd_old)] = j.slots.map(|s| limiter.previous(states, s)) else {
        return raw;
    };
    let vgs = limiter.pn_junction(raw[0], Some(vgs_old), j.vt, j.vcrit);
    let vgd = limiter.pn_junction(raw[1], Some(vgd_old), j.vt, j.vcrit);
    [
        limiter.fet_gate(vgs, vgs_old, j.vto),
        limiter.fet_gate(vgd, vgd_old, j.vto),
    ]
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

/// Drain and source series conductances per device (`gdpr`, `gspr`:
/// `area / R`), zero without an internal node.
pub(super) fn series_conductances(area: Real, rd: Real, rs: Real) -> [Real; 2] {
    let conductance = |r: Real| if r == 0. { 0. } else { area / r };
    [conductance(rd), conductance(rs)]
}

/// The d-d' and s-s' ports of the series resistances.
pub(super) fn series_ports(terminals: &[NodeId], inner: [NodeId; 3]) -> [[NodeId; 2]; 2] {
    [[terminals[0], inner[0]], [terminals[2], inner[2]]]
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

/// Stamps a nonlinear current `current(v)` flowing from `output[0]` to
/// `output[1]`, linearized at the evaluated voltages.
pub(super) fn stamp_current(
    context: &mut StampContext<'_>,
    output: [NodeId; 2],
    current: Real,
    partials: &[(NodeId, Real)],
    linearized: Real,
) -> SpiceResult<()> {
    let equivalent = current - linearized;
    for (row, sign) in [(output[0], 1.), (output[1], -1.)] {
        for (col, derivative) in partials {
            context.stamp(row, *col, sign * derivative)?;
        }
        context.stamp_rhs(row, -sign * equivalent)?;
    }
    Ok(())
}

/// Stamps one gate junction from `ends[0]` (the gate) to `ends[1]` at the
/// physical evaluation voltage `v`: its current and conductance, plus in
/// transient the companion of its charge (`NIintegrate`). `current`,
/// `conductance`, `charge` and `capacitance` are per device; the charge and
/// its derivative go to `slot`/`slot + 1` unscaled, and every stamp is scaled
/// by the multiplicity `m` (`jfetload.c`).
#[allow(clippy::too_many_arguments)]
fn stamp_gate(
    context: &mut StampContext<'_>,
    ends: [NodeId; 2],
    v: Real,
    current: Real,
    conductance: Real,
    (charge, capacitance): (Real, Real),
    slot: usize,
    m: Real,
) -> SpiceResult<()> {
    let mut total = conductance;
    let mut equivalent = current - conductance * v;
    if let Some(coefficients) = context.integration {
        let crate::devices::AnalysisMode::Transient { dt, .. } = context.mode else {
            return Err(SpiceError::circuit(
                "JFET charge companion outside transient",
            ));
        };
        if dt != coefficients.dt() {
            return Err(SpiceError::circuit("JFET charge timestep mismatch"));
        }
        let mut history = vec![charge];
        for age in 1..=coefficients.charge_history_len() {
            history.push(
                context
                    .states
                    .accepted(age, slot)
                    .ok_or_else(|| SpiceError::circuit("missing accepted JFET gate charge"))?,
            );
        }
        let previous =
            if coefficients.needs_previous_derivative() {
                Some(context.states.accepted(1, slot + 1).ok_or_else(|| {
                    SpiceError::circuit("missing accepted JFET gate charge current")
                })?)
            } else {
                None
            };
        let companion = coefficients.integrate(&history, previous, capacitance)?;
        total += companion.conductance;
        equivalent += companion.derivative - companion.conductance * v;
        context.states.set(slot + 1, companion.derivative)?;
    } else {
        if context.mode.is_transient() {
            return Err(SpiceError::circuit(
                "JFET transient needs companion integration",
            ));
        }
        context.states.set(slot + 1, 0.)?;
    }
    context.states.set(slot, charge)?;
    nodal_stamp(context.matrix, context.unknowns, ends, m * total)?;
    context.stamp_rhs(ends[0], -m * equivalent)?;
    context.stamp_rhs(ends[1], m * equivalent)
}

impl Device for Jfet {
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
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("JFET AC needs small-signal assembly"));
        }
        let op = self.operating(&context.model_context())?;
        let pol = self.model.pol;
        let raw = self.normalized(self.inner.map(|n| context.node_voltage(n)));
        let mut limiter = Limiter::new(&context.states);
        let ic = self.start_conditions(|n| context.node_voltage(n));
        let [vgs, vgd] = self.limit(&op, &mut limiter, &context.states, raw, ic);
        let point = self.point(&op, vgs, vgd, context.gmin)?;
        let m = self.instance.m;
        // The intrinsic d', g, s' at the (limited) voltages, relative to s';
        // every equation below depends on voltage differences only.
        let v = [pol * (vgs - vgd), pol * vgs, 0.];
        let partials = self
            .channel_partials(&point.channel)
            .map(|(node, partial)| (node, m * partial));
        let linearized: Real = partials
            .iter()
            .zip(v)
            .map(|((_, partial), voltage)| partial * voltage)
            .sum();
        let [d, g, s] = self.inner;
        // The held-off convergence test of the shared limiter: the drain
        // (channel minus gate-drain diode) and gate currents at the held
        // voltages against their linear prediction at the iterate.
        {
            let node = |n: NodeId| context.node_voltage(n);
            let iterate = [node(d) - node(s), node(g) - node(s), 0.];
            let channel_change: Real = partials
                .iter()
                .zip(iterate)
                .map(|((_, partial), voltage)| partial * voltage)
                .sum::<Real>()
                - linearized;
            let gd_change = m * point.gd.1 * ((iterate[1] - iterate[0]) - (v[1] - v[0]));
            let gs_change = m * point.gs.1 * (iterate[1] - v[1]);
            let drain = m * pol * (point.channel.current - point.gd.0);
            let gate = m * pol * (point.gs.0 + point.gd.0);
            limiter.test_held(
                &context.states,
                &[
                    (drain, drain + channel_change - gd_change),
                    (gate, gate + gs_change + gd_change),
                ],
            );
        }
        for (ports, conductance) in self.series_ports().into_iter().zip(self.series()) {
            if conductance > 0. {
                nodal_stamp(context.matrix, context.unknowns, ports, m * conductance)?;
            }
        }
        stamp_current(
            context,
            [d, s],
            m * pol * point.channel.current,
            &partials,
            linearized,
        )?;
        let physical = |(value, derivative): (Real, Real)| (pol * value, derivative);
        let (current, conductance) = physical(point.gs);
        stamp_gate(
            context,
            [g, s],
            v[1],
            current,
            conductance,
            physical(point.qgs),
            slot::QGS,
            m,
        )?;
        let (current, conductance) = physical(point.gd);
        stamp_gate(
            context,
            [g, d],
            v[1] - v[0],
            current,
            conductance,
            physical(point.qgd),
            slot::QGD,
            m,
        )?;
        limiter.finish(&mut context.states, &[(slot::VGS, vgs), (slot::VGD, vgd)])
    }
    /// `jfetacld.c`: conductances at the bias plus `j omega` times the gate
    /// depletion capacitances, every term scaled by `m`.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let op = self.operating(context.model_context)?;
        let v = self.inner.map(|n| {
            context
                .unknowns
                .node_row(n)
                .and_then(|r| bias.get(r))
                .unwrap_or(0.)
        });
        let [vgs, vgd] = self.normalized(v);
        let point = self.point(&op, vgs, vgd, context.model_context.gmin)?;
        let m = self.instance.m;
        for (ports, conductance) in self.series_ports().into_iter().zip(self.series()) {
            if conductance > 0. {
                context.nodal(ports, m * conductance, false)?;
            }
        }
        let [d, g, s] = self.inner;
        for (row, sign) in [(d, 1.), (s, -1.)] {
            for (col, derivative) in self.channel_partials(&point.channel) {
                if let (Some(r), Some(c)) = (
                    context.unknowns.node_row(row),
                    context.unknowns.node_row(col),
                ) {
                    context.system.a.add(r, c, sign * m * derivative)?;
                }
            }
        }
        for (ports, (_, conductance), (_, capacitance)) in
            [([g, s], point.gs, point.qgs), ([g, d], point.gd, point.qgd)]
        {
            context.nodal(ports, m * conductance, false)?;
            context.nodal(ports, m * capacitance, true)?;
        }
        Ok(())
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
