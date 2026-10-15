//! Statz MESFET, `Z` level 1 (`mes`).
//!
//! C references, read as behaviour only:
//!
//! - `parser/inp2z.c` and `parser/inpdomod.c`: the `Z` card and the
//!   `nmf`/`pmf` level map (0 or 1 `mes/`, 2-4 `mesa/`, 5 `hfet1/`, 6
//!   `hfet2/`; only level 1 is ported);
//! - `mes/mes.c` (`MESpTable`, `MESmPTable`), `mesparam.c`, `mesmpar.c`: the
//!   instance and model setters, applied in card order;
//! - `mes/messetup.c` (`MESsetup`): model/instance defaults and the internal
//!   source then drain nodes created for nonzero RS/RD;
//! - `mes/mestemp.c` (`MEStemp`): the series conductances and `vcrit`; the
//!   model has no temperature dependence (`CONSTvt0` is the fixed 27 C thermal
//!   voltage);
//! - `mes/mesload.c` (`MESload`): gate-source/gate-drain diodes with C's cubic
//!   reverse continuation below `-3 Vt`, the Statz channel current
//!   `beta (1 + lambda vds) vgst^2 / (1 + b vgst)` times the cubic
//!   saturation polynomial `1 - (1 - alpha vds / 3)^3` (C's approximation of
//!   the `tanh` law) in normal and inverse mode, the Statz gate charge
//!   `qggnew`, `MODEINITJCT` start voltages and `DEVpnjlim`/`DEVfetlim`
//!   limiting;
//! - `mes/mesacl.c` (`MESacLoad`), `mespzld.c` (`MESpzLoad`),
//!   `mestrunc.c` (`MEStrunc`), `mesgetic.c` (`MESgetic`), `mesask.c`
//!   (`MESask`) and `mesnoise.c` (`MESnoise`).
//!
//! # Gate charge
//!
//! As in `jfet2/` (whose charge code C copied from `mes/`), the gate charge is
//! incremental in transient: `qgs = qgs(accepted) + (q(vgs, vgd) -
//! q(vgs1, vgd) + q(vgs, vgd1) - q(vgs1, vgd1)) / 2` with the accepted junction
//! voltages `vgs1`, `vgd1`, symmetrically for `qgd`. Every DC load stores
//! the total charge `q(vgs, vgd)` in both slots: `mesload.c` does so in DC
//! sweeps and the `uic` load, and its `MODEINITTRAN` load writes it into
//! both accepted charges from the operating point. `CKTterr` (`mestrunc.c`)
//! scales its charge tolerance with `|q|`, so the offsets matter for
//! timestep control (`jfet2load.c` has no such assignment, so `jfet2/`
//! stores zero there).
//!
//! # Deliberate divergences
//!
//! - As for the JFET (see [`crate::devices::jfet`]): no `MODEINITPRED`
//!   extrapolation or bypass, any limited step keeps the load nonconvergent,
//!   and `off` uses the shared held-load rule.
//! - The transient Newton matrix is the exact Jacobian of C's equations: it
//!   includes the cross derivatives of the incremental gate charges
//!   (`d qgs / d vgd`, `d qgd / d vgs`) where `mesload.c` stamps each charge
//!   as a two-terminal capacitance `qggnew` reports at the iterate. The
//!   equations and their solution are C's; only the iteration differs.
//! - `mespzld.c` scales some diagonal conductances without the multiplicity
//!   (`m * ggd + ggs`: an operator-precedence slip); the port's pole-zero
//!   load is the AC load, every term scaled by `m`. Both agree for `m = 1`.
//! - An unlabeled leading value on the card is refused by the parser
//!   ([`crate::netlist`]): INP2Z discards it for MES models.
//! - `ALPHA` must be positive (`3 / ALPHA` and `1 / ALPHA` divide by it),
//!   `B` nonnegative (keeping `1 + B vgst` positive). `qggnew` takes
//!   `sqrt(1 - v / PB)` of its effective forward voltage `v`, at most
//!   `vmax = 0.5 V`: with `PB <= 0.5 V` a forward-biased gate makes C's
//!   charge NaN, which the port reports as a numerical error in transient
//!   and AC (outside transient the charge is only stored; see the stamp).
//! - `FC` only feeds `mestemp.c`'s `depl_cap`/`f1`-`f3`, which `mesload.c`
//!   never reads: it is validated as finite and has no effect, as in C.
//!
//! `.disto` (`mesdset.c`, `mesdisto.c`) and `.sens` fail with explicit
//! errors.

use crate::devices::jfet::{self, BOLTZMANN, CHARGE, REFTEMP, p};
use crate::devices::limiting;
use crate::devices::noise::{
    DeviceNoise, FlickerLaw, NoiseContext, NoiseFamily, NoiseKind, NoiseSource,
};
use crate::devices::nonlinear::{JunctionPoint, junction_companion, junction_current};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::{
    Device, LinearContext, ModelContext, ModelFamily, ResolvedModel, StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeTable, Real, SpiceError, SpiceResult};

/// C `CONSTvt0`: the thermal voltage at `REFTEMP`, `CONSTboltz * REFTEMP /
/// CHARGE` (`main.c`). MES uses it whatever the circuit temperature.
const VT0: Real = BOLTZMANN * REFTEMP / CHARGE;
/// `qggnew`'s smoothing parameter `del`.
const DEL: Real = 0.2;
/// `qggnew`'s `vmax`: the largest effective forward voltage of the
/// depletion charge, beyond which it continues linearly.
const VMAX: Real = 0.5;
/// C `N_MINLOG` (`noisedef.h`).
const N_MINLOG: Real = 1e-38;

// The state vector follows the shared JFET-family layout
// ([`crate::devices::jfet::slot`]) -- `VGS`/`VGD`, the load currents and
// conductances, then the gate charges -- so that `mesload.c`'s port shares
// `jfet::select_junctions` and `jfet::finish_load` with the JFETs. C's
// `MESnumStates` carries the same slots in a different order, which the port
// does not need to reproduce.
use crate::devices::jfet::slot;

/// Model setters of `MESmPTable` with the defaults of `messetup.c`. The two
/// spellings of VTO (`vt0` and its `IOPR` alias `vto`) have no schema
/// default.
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("alpha", U::InverseVolt, D::Positive, Some(2.)),
    p(
        "beta",
        U::AmperePerVoltSquared,
        D::NonNegative,
        Some(2.5e-3),
    ),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.)),
    p("b", U::InverseVolt, D::NonNegative, Some(0.3)),
    p("rd", U::Ohm, D::NonNegative, Some(0.)),
    p("rs", U::Ohm, D::NonNegative, Some(0.)),
    p("cgs", U::Farad, D::NonNegative, Some(0.)),
    p("cgd", U::Farad, D::NonNegative, Some(0.)),
    p("pb", U::Volt, D::Positive, Some(1.)),
    p("is", U::Ampere, D::Positive, Some(1e-14)),
    // Only mestemp.c's unused depletion coefficients read FC.
    p("fc", U::Dimensionless, D::Finite, Some(0.5)),
    // mesnoise.c flicker coefficient and exponent.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
];

/// Scalar instance setters of `MESpTable` (`messetup.c` defaults); `icvds`,
/// `icvgs` and `off` are split off first.
const INSTANCE: &[P] = &[
    p("area", U::Dimensionless, D::Positive, Some(1.)),
    p("m", U::Dimensionless, D::Positive, Some(1.)),
];

/// The `IC` vector components (`mesparam.c`): `ICVDS`, then `ICVGS`.
const IC_COMPONENTS: [&str; 2] = ["icvds", "icvgs"];

/// Validated model card.
#[derive(Debug, Clone, Copy)]
struct Model {
    /// C `MEStype`: +1 for NMF, -1 for PMF.
    pol: Real,
    vto: Real,
    alpha: Real,
    beta: Real,
    lambda: Real,
    b: Real,
    rd: Real,
    rs: Real,
    cgs: Real,
    cgd: Real,
    pb: Real,
    is: Real,
    kf: Real,
    af: Real,
}

/// Validated instance setters.
#[derive(Debug, Clone, Copy)]
struct Instance {
    area: Real,
    m: Real,
}

/// The area-dependent quantities of one load (start of `mesload.c`).
#[derive(Debug, Clone, Copy)]
struct Operating {
    /// C `MESvcrit` (from the model's, not the area-scaled, saturation
    /// current).
    vcrit: Real,
    /// `MESbeta * area`, `MESgateSatCurrent * area`.
    beta: Real,
    csat: Real,
    /// `MEScapGS * area`, `MEScapGD * area`.
    czgs: Real,
    czgd: Real,
}

/// The channel at one normalized bias: drain current from d' to s' and its
/// partials with respect to `vgs` (at fixed `vds`) and `vds` (at fixed
/// `vgs`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Channel {
    current: Real,
    gm: Real,
    gds: Real,
}

/// The DC quantities of one normalized bias point, per device (before `m`).
#[derive(Debug, Clone, Copy)]
struct Point {
    channel: Channel,
    /// Gate-source and gate-drain junction currents (with `gmin * v`) and
    /// conductances (with `gmin`).
    gs: (Real, Real),
    gd: (Real, Real),
}

/// `qggnew` of `mesload.c`: the Statz gate charge at (`vgs`, `vgd`) and its
/// partials `(q, d q / d vgs, d q / d vgd)`.
fn qggnew(
    vgs: Real,
    vgd: Real,
    phib: Real,
    vcap: Real,
    vto: Real,
    cgs: Real,
    cgd: Real,
) -> (Real, Real, Real) {
    let veroot = ((vgs - vgd) * (vgs - vgd) + vcap * vcap).sqrt();
    let veff1 = 0.5 * (vgs + vgd + veroot);
    let veff2 = veff1 - veroot;
    let vnroot = ((veff1 - vto) * (veff1 - vto) + DEL * DEL).sqrt();
    let mut vnew1 = 0.5 * (veff1 + vto + vnroot);
    let vnew3 = vnew1;
    let ext = if vnew1 < VMAX {
        0.
    } else {
        vnew1 = VMAX;
        (vnew3 - VMAX) / (1. - VMAX / phib).sqrt()
    };
    let qroot = (1. - vnew1 / phib).sqrt();
    let qggval = cgs * (2. * phib * (1. - qroot) + ext) + cgd * veff2;
    let par1 = 0.5 * (1. + (veff1 - vto) / vnroot);
    let cfact = (vgs - vgd) / veroot;
    let cplus = 0.5 * (1. + cfact);
    let cminus = cplus - cfact;
    (
        qggval,
        cgs / qroot * par1 * cplus + cgd * cminus,
        cgs / qroot * par1 * cminus + cgd * cplus,
    )
}

impl Model {
    /// `mesload.c`'s channel current and partials at normalized voltages.
    fn channel(&self, op: &Operating, vgs: Real, vgd: Real) -> Channel {
        let (alpha, b, lambda, beta) = (self.alpha, self.b, self.lambda, op.beta);
        let vds = vgs - vgd;
        let off = Channel {
            current: 0.,
            gm: 0.,
            gds: 0.,
        };
        if vds >= 0. {
            let vgst = vgs - self.vto;
            if vgst <= 0. {
                return off;
            }
            let prod = 1. + lambda * vds;
            let betap = beta * prod;
            let denom = 1. + b * vgst;
            let invdenom = 1. / denom;
            if vds >= 3. / alpha {
                // Normal mode, saturation region.
                Channel {
                    current: betap * vgst * vgst * invdenom,
                    gm: betap * vgst * (1. + denom) * invdenom * invdenom,
                    gds: lambda * beta * vgst * vgst * invdenom,
                }
            } else {
                // Normal mode, linear region.
                let afact = 1. - alpha * vds / 3.;
                let lfact = 1. - afact * afact * afact;
                Channel {
                    current: betap * vgst * vgst * invdenom * lfact,
                    gm: betap * vgst * (1. + denom) * invdenom * invdenom * lfact,
                    gds: beta
                        * vgst
                        * vgst
                        * invdenom
                        * (alpha * afact * afact * prod + lfact * lambda),
                }
            }
        } else {
            let vgdt = vgd - self.vto;
            if vgdt <= 0. {
                return off;
            }
            let prod = 1. - lambda * vds;
            let betap = beta * prod;
            let denom = 1. + b * vgdt;
            let invdenom = 1. / denom;
            if -vds >= 3. / alpha {
                // Inverse mode, saturation region.
                let gm = -betap * vgdt * (1. + denom) * invdenom * invdenom;
                Channel {
                    current: -betap * vgdt * vgdt * invdenom,
                    gm,
                    gds: lambda * beta * vgdt * vgdt * invdenom - gm,
                }
            } else {
                // Inverse mode, linear region.
                let afact = 1. + alpha * vds / 3.;
                let lfact = 1. - afact * afact * afact;
                let gm = -betap * vgdt * (1. + denom) * invdenom * invdenom * lfact;
                Channel {
                    current: -betap * vgdt * vgdt * invdenom * lfact,
                    gm,
                    gds: beta
                        * vgdt
                        * vgdt
                        * invdenom
                        * (alpha * afact * afact * prod + lfact * lambda)
                        - gm,
                }
            }
        }
    }

    /// `qggnew` with this model's `PB`, `1 / ALPHA`, `VTO` and the
    /// area-scaled zero-bias capacitances.
    fn qgg(&self, op: &Operating, vgs: Real, vgd: Real) -> (Real, Real, Real) {
        qggnew(
            vgs,
            vgd,
            self.pb,
            1. / self.alpha,
            self.vto,
            op.czgs,
            op.czgd,
        )
    }
}

/// MESFET level 1 with series resistance and Statz gate charge.
#[derive(Debug)]
pub struct Mesfet {
    name: String,
    /// The `.model` name (noise visiting order).
    model_name: String,
    /// External d, g, s followed by any internal source/drain nodes.
    terminals: Vec<NodeId>,
    /// d', g, s': the nodes the intrinsic device sees.
    inner: [NodeId; 3],
    model: Model,
    instance: Instance,
    /// `OFF` and `ICVDS`/`ICVGS` (C `MESoff`, `MESicVDS`, `MESicVGS`).
    initial: crate::devices::initial::InstanceInitial,
}

impl Mesfet {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        resolved: &ResolvedModel<'_>,
        _context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if resolved.levels().selector != 1 || i.nodes.len() != 3 {
            return Err(SpiceError::circuit(format!(
                "{}: MESFET needs level 1 and three terminals",
                i.name
            )));
        }
        let (initial, setters) = crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)?;
        let (pol, scalars) = jfet::typed_scalars(
            resolved,
            resolved.family() == ModelFamily::Pmf,
            ["nmf", "pmf"],
        );
        let m = ScalarSchema { parameters: MODEL }.validate(scalars, &resolved.card().location)?;
        let r = |name: &str| jfet::required(&m, name);
        // MES_MOD_VTO has the IOP spelling `vt0` and the IOPR alias `vto`;
        // setters apply in card order, so the later one wins.
        let model = Model {
            pol,
            vto: jfet::last_setter(&m, &["vto", "vt0"]).unwrap_or(-2.),
            alpha: r("alpha")?,
            beta: r("beta")?,
            lambda: r("lambda")?,
            b: r("b")?,
            rd: r("rd")?,
            rs: r("rs")?,
            cgs: r("cgs")?,
            cgd: r("cgd")?,
            pb: r("pb")?,
            is: r("is")?,
            kf: r("kf")?,
            af: r("af")?,
        };
        let v = ScalarSchema {
            parameters: INSTANCE,
        }
        .validate(&setters, &i.location)?;
        let instance = Instance {
            area: jfet::required(&v, "area")?,
            m: jfet::required(&v, "m")?,
        };
        let mut device = Self {
            name: i.name.clone(),
            model_name: resolved.card().name.clone(),
            terminals: vec![],
            inner: [NodeId::GROUND; 3],
            model,
            instance,
            initial,
        };
        // Validate every derivation before interning nodes.
        device.operating()?;
        (device.terminals, device.inner) = jfet::intern_nodes(i, nodes, model.rd, model.rs)?;
        Ok(Box::new(device))
    }

    /// `mestemp.c` and the area scaling at the start of `mesload.c`.
    fn operating(&self) -> SpiceResult<Operating> {
        let (model, area) = (&self.model, self.instance.area);
        let op = Operating {
            vcrit: limiting::critical_voltage(VT0, model.is),
            beta: model.beta * area,
            csat: model.is * area,
            czgs: model.cgs * area,
            czgd: model.cgd * area,
        };
        if [op.vcrit, op.beta, op.csat, op.czgs, op.czgd]
            .iter()
            .any(|v| !v.is_finite())
            || op.csat <= 0.
        {
            return Err(SpiceError::circuit(format!(
                "{}: MESFET parameters are out of range (nonfinite area-scaled value)",
                self.name
            )));
        }
        Ok(op)
    }

    fn numerical(&self, message: &str) -> SpiceError {
        SpiceError::Numerical {
            context: format!("MESFET {}", self.name),
            message: message.into(),
        }
    }

    /// The DC quantities at normalized `vgs`, `vgd` (`mesload.c`).
    fn point(&self, op: &Operating, vgs: Real, vgd: Real, gmin: Real) -> SpiceResult<Point> {
        let junction = |v: Real| -> SpiceResult<(Real, Real)> {
            let (current, conductance) = junction_current(v, VT0, op.csat)?;
            Ok((current + gmin * v, conductance + gmin))
        };
        let point = Point {
            channel: self.model.channel(op, vgs, vgd),
            gs: junction(vgs)?,
            gd: junction(vgd)?,
        };
        let c = point.channel;
        if [
            c.current, c.gm, c.gds, point.gs.0, point.gs.1, point.gd.0, point.gd.1,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err(self.numerical("nonfinite channel/junction equations"));
        }
        Ok(point)
    }

    /// The Statz charge and its partials, refusing `qggnew`'s nonfinite
    /// cases (a forward gate with `PB <= 0.5 V`).
    fn charge(&self, op: &Operating, vgs: Real, vgd: Real) -> SpiceResult<(Real, Real, Real)> {
        let q = self.model.qgg(op, vgs, vgd);
        if [q.0, q.1, q.2].iter().any(|v| !v.is_finite()) {
            return Err(self.numerical(&format!(
                "nonfinite Statz gate charge (qggnew's effective gate voltage reaches \
                 PB = {} V; it needs PB > {VMAX} V for a forward-biased gate)",
                self.model.pb
            )));
        }
        Ok(q)
    }

    /// Normalized `[vgs, vgd]` from physical node voltages of d', g, s'.
    fn normalized(&self, v: [Real; 3]) -> [Real; 2] {
        let pol = self.model.pol;
        [pol * (v[1] - v[2]), pol * (v[1] - v[0])]
    }

    /// Normalized `[vgs, vgd]` at a solution vector (no limiting).
    fn bias(&self, unknowns: &crate::devices::MnaUnknowns, bias: &Vector) -> [Real; 2] {
        self.normalized(
            self.inner
                .map(|n| unknowns.node_row(n).and_then(|r| bias.get(r)).unwrap_or(0.)),
        )
    }

    /// The bias-dependent ask quantities of `mesask.c` at the solution
    /// `voltage` (no limiting): `vgs`, `vgd` and the multiplicity-scaled
    /// `gm`, `gds`, `ggs`, `ggd` and gate-drain current `cgd` (normalized
    /// polarity, as C stores them). In transient C adds the integration
    /// companion to `ggs`, `ggd` and `cgd`, so those are refused there.
    fn operating_ask(
        &self,
        key: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        transient: bool,
    ) -> SpiceResult<Option<Real>> {
        if !matches!(key, "vgs" | "vgd" | "gm" | "gds" | "ggs" | "ggd" | "cgd") {
            return Ok(None);
        }
        if transient && matches!(key, "ggs" | "ggd" | "cgd") {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: transient @{}[{key}] (C adds the charge companion)",
                    self.name, self.name
                ),
                "src/spicelib/devices/mes/mesask.c, mesload.c",
            ));
        }
        let op = self.operating()?;
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

impl Device for Mesfet {
    /// `mesask.c` scalar asks: `area` (C reports `area * m`) and the given
    /// `icvds`/`icvgs`. C answers no `m` ask (`MES_M` is missing from
    /// `MESask`).
    fn observation_parameter(
        &self,
        key: &str,
        _context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        Ok(match key {
            "area" => Some(self.instance.area * self.instance.m),
            "icvds" => self.initial.values[0],
            "icvgs" => self.initial.values[1],
            _ => None,
        })
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
    /// `mesask.c` reports `cd`, `cg` and `is` in the device's normalized
    /// polarity (`MEScd`, `MEScg` are stored before the `type` factor).
    fn observation_current_sign(&self) -> Real {
        self.model.pol
    }
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
    /// `mestrunc.c`: both gate charges control the timestep.
    fn truncation_slots(&self) -> Vec<usize> {
        vec![slot::QGS, slot::QGD]
    }
    /// `MESpTable`'s settable reals (`dctrcurv.c` `DCTsetInstParam`; MES has
    /// no temperature dependence to re-derive).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        ["area", "m", "icvds", "icvgs"]
            .into_iter()
            .find(|name| name.eq_ignore_ascii_case(keyword))
    }
    /// `MESparam` with the instance schema domains.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        _context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        use crate::devices::sweep::check_swept;
        let check = |ok: bool, what: &str| check_swept(&self.name, parameter, value, ok, what);
        let mut instance = self.instance;
        let mut initial = self.initial.clone();
        match parameter {
            "area" => {
                check(value > 0., "positive")?;
                instance.area = value;
            }
            "m" => {
                check(value > 0., "positive")?;
                instance.m = value;
            }
            "icvds" | "icvgs" => {
                check(true, "finite")?;
                initial.values[usize::from(parameter == "icvgs")] = Some(value);
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "{}: MESFET parameter {parameter} cannot be swept",
                    self.name
                )));
            }
        }
        let device = Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            terminals: self.terminals.clone(),
            inner: self.inner,
            model: self.model,
            instance,
            initial,
        };
        device.operating()?;
        Ok(Box::new(device))
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("MESFET AC needs small-signal assembly"));
        }
        let op = self.operating()?;
        let pol = self.model.pol;
        let raw = self.normalized(self.inner.map(|n| context.node_voltage(n)));
        // mesgetic.c: unset initial conditions come from the external nodes.
        let ic =
            jfet::start_conditions(&self.terminals, &self.initial, |n| context.node_voltage(n));
        let integration = context.integration;
        // C `xfact = CKTdelta / CKTdeltaOld[1]`; one in a `.dc` sweep.
        let xfact = integration.map_or(1., crate::maths::integrator::Coefficients::predictor_ratio);
        let ([vgs, vgd], start) = jfet::select_junctions(
            jfet::Junctions {
                pol,
                off: self.initial.off,
                vt: VT0,
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
                "MESFET transient needs companion integration",
            ));
        }
        let point = self.point(&op, vgs, vgd, context.gmin)?;
        let vds = vgs - vgd;
        let (cdrain, gm, gds) = (point.channel.current, point.channel.gm, point.channel.gds);
        let ((cgs, ggs), (cgd, ggd)) = (point.gs, point.gd);
        let mut l = jfet::Loaded {
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
        // The incremental Statz charges with their capacitances: (charge,
        // d/dvgs, d/dvgd) for qgs and qgd.
        let (qgs, qgd, capgs, capgd) = if transient {
            let accepted = |s: usize, what: &str| {
                context
                    .states
                    .accepted(1, s)
                    .ok_or_else(|| self.numerical(&format!("missing accepted {what}")))
            };
            let (vgs1, vgd1) = (accepted(slot::VGS, "vgs")?, accepted(slot::VGD, "vgd")?);
            let (qgs1, qgd1) = (accepted(slot::QGS, "qgs")?, accepted(slot::QGD, "qgd")?);
            let (qa, cgsna, cgdna) = self.charge(&op, vgs, vgd)?;
            let (qb, _, _) = self.charge(&op, vgs1, vgd)?;
            let (qc, _, _) = self.charge(&op, vgs, vgd1)?;
            let (qd, _, _) = self.charge(&op, vgs1, vgd1)?;
            // mesload.c takes the incremental charges from CKTstate1 and the
            // capacitances of the *first* evaluation (`capgs = cgsna`,
            // `capgd = cgdna`), not an average of the four calls.
            (
                qgs1 + 0.5 * (qa - qb + qc - qd),
                qgd1 + 0.5 * (qa - qc + qb - qd),
                cgsna,
                cgdna,
            )
        } else {
            // Outside transient the total charge goes to both slots: what
            // mesload.c stores in DC sweeps and the `uic` load, and what its
            // MODEINITTRAN load writes into CKTstate1 for both charges from
            // the operating point. CKTterr's charge tolerance (mestrunc.c)
            // scales with |q|, so the first steps need C's offset. Nothing
            // integrates it here, so C's NaN for a forward gate with
            // PB <= 0.5 V is stored as zero rather than refusing a DC
            // analysis C completes (a transient then fails in `charge`).
            let q = self.model.qgg(&op, vgs, vgd).0;
            let q = if q.is_finite() { q } else { 0. };
            (q, q, 0., 0.)
        };
        if transient {
            let coefficients = integration.ok_or_else(|| {
                SpiceError::circuit("MESFET transient needs companion integration")
            })?;
            let companion = |charge, capacitance| JunctionPoint {
                current: 0.,
                conductance: 0.,
                charge,
                capacitance,
            };
            let (geq, cqgs) =
                junction_companion(context, coefficients, companion(qgs, capgs), slot::QGS)?;
            l.ggs += geq;
            l.cg += cqgs;
            let (geq, cqgd) =
                junction_companion(context, coefficients, companion(qgd, capgd), slot::QGD)?;
            l.ggd += geq;
            l.cg += cqgd;
            l.cd -= cqgd;
            l.cgd += cqgd;
            context.states.set(slot::QGS, qgs)?;
            context.states.set(slot::QGD, qgd)?;
        } else {
            for (s, q) in [(slot::QGS, qgs), (slot::QGD, qgd)] {
                context.states.set(s, q)?;
                context.states.set(s + 1, 0.)?;
            }
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
    /// `mesacl.c`: conductances at the bias plus `j omega` times `qggnew`'s
    /// gate-source and gate-drain capacitances (each a two-terminal element,
    /// as in C), every term scaled by `m`.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let op = self.operating()?;
        let [vgs, vgd] = self.bias(context.unknowns, bias);
        let point = self.point(&op, vgs, vgd, context.model_context.gmin)?;
        let (_, capgs, capgd) = self.charge(&op, vgs, vgd)?;
        let m = self.instance.m;
        for (ports, conductance) in jfet::series_ports(&self.terminals, self.inner)
            .into_iter()
            .zip(jfet::series_conductances(
                self.instance.area,
                self.model.rd,
                self.model.rs,
            ))
        {
            if conductance > 0. {
                context.nodal(ports, m * conductance, false)?;
            }
        }
        let [d, g, s] = self.inner;
        let c = point.channel;
        for (row, sign) in [(d, 1.), (s, -1.)] {
            for (col, derivative) in self.inner.iter().zip([c.gds, c.gm, -c.gds - c.gm]) {
                if let (Some(r), Some(col)) = (
                    context.unknowns.node_row(row),
                    context.unknowns.node_row(*col),
                ) {
                    context.system.a.add(r, col, sign * m * derivative)?;
                }
            }
        }
        for (ports, conductance, capacitance) in
            [([g, s], point.gs.1, capgs), ([g, d], point.gd.1, capgd)]
        {
            context.nodal(ports, m * conductance, false)?;
            context.nodal(ports, m * capacitance, true)?;
        }
        Ok(())
    }
    /// `mespzld.c` is the AC load with `s` for `j omega` (see the module
    /// docs for its multiplicity slip).
    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }
    /// `mesnoise.c`: thermal noise of RD and RS (`NevalSrc`, circuit
    /// temperature), channel thermal noise `4 k T (2/3) |gm|` and flicker
    /// noise `KF |cd|^AF / f` between d' and s', every source scaled by `m`.
    /// `cd` is C's `MEScd`: the channel minus the gate-drain diode current.
    fn noise(&self, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let op = self.operating()?;
        let [vgs, vgd] = self.bias(context.unknowns, context.bias);
        let point = self.point(&op, vgs, vgd, context.model_context.gmin)?;
        let m = self.instance.m;
        let temperature = context.circuit_kelvin();
        let [gdpr, gspr] =
            jfet::series_conductances(self.instance.area, self.model.rd, self.model.rs);
        let cd = point.channel.current - point.gd.0;
        let flicker = m * self.model.kf * (self.model.af * cd.abs().max(N_MINLOG).ln()).exp();
        let [d, _, s] = self.inner;
        let thermal = |conductance: Real| NoiseKind::Thermal {
            conductance,
            temperature,
        };
        Ok(DeviceNoise::Sources {
            family: NoiseFamily::Mes,
            model: Some(self.model_name.clone()),
            total: true,
            sources: vec![
                NoiseSource::new("_rd", [d, self.terminals[0]], thermal(gdpr * m)),
                NoiseSource::new("_rs", [s, self.terminals[2]], thermal(gspr * m)),
                NoiseSource::new(
                    "_id",
                    [d, s],
                    thermal(2.0 / 3.0 * m * point.channel.gm.abs()),
                ),
                NoiseSource::new(
                    "_1overf",
                    [d, s],
                    NoiseKind::Flicker {
                        numerator: flicker,
                        law: FlickerLaw::PerHertz,
                    },
                ),
            ],
        })
    }
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(SpiceError::not_yet_ported(
            format!("distortion analysis of MESFET {}", self.name),
            "src/spicelib/devices/mes/mesdset.c, mesdisto.c",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(b: Real, lambda: Real) -> (Model, Operating) {
        let model = Model {
            pol: 1.,
            vto: -1.5,
            alpha: 2.5,
            beta: 2e-3,
            lambda,
            b,
            rd: 0.,
            rs: 0.,
            cgs: 2e-12,
            cgd: 0.4e-12,
            pb: 0.8,
            is: 1e-14,
            kf: 0.,
            af: 1.,
        };
        let device = Mesfet {
            name: "z1".into(),
            model_name: "zm".into(),
            terminals: vec![],
            inner: [NodeId::GROUND; 3],
            model,
            instance: Instance { area: 1.5, m: 1. },
            initial: crate::devices::initial::InstanceInitial::default(),
        };
        (model, device.operating().unwrap())
    }

    /// gm = dI/dvgs at fixed vds and gds = dI/dvds at fixed vgs, in every
    /// region of both modes.
    #[test]
    fn channel_partials_are_the_current_derivatives() {
        for (b, lambda) in [(0.3, 0.), (0., 0.05), (1.2, 0.02)] {
            let (model, op) = device(b, lambda);
            let current = |vgs: Real, vds: Real| model.channel(&op, vgs, vgs - vds).current;
            for vgs in [-2., -1.2, -0.5, 0., 0.4] {
                for vds in [-3., -1.5, -0.6, -0.05, 0.05, 0.6, 1.5, 3.] {
                    let c = model.channel(&op, vgs, vgs - vds);
                    let h = 1e-6;
                    let gm = (current(vgs + h, vds) - current(vgs - h, vds)) / (2. * h);
                    let gds = (current(vgs, vds + h) - current(vgs, vds - h)) / (2. * h);
                    let scale = c.gm.abs().max(c.gds.abs()).max(1e-9);
                    assert!((gm - c.gm).abs() <= 1e-6 * scale, "{b} {vgs} {vds}: gm");
                    assert!((gds - c.gds).abs() <= 1e-6 * scale, "{b} {vgs} {vds}: gds");
                }
            }
        }
    }

    #[test]
    fn the_channel_is_antisymmetric_under_drain_source_exchange() {
        let (model, op) = device(0.3, 0.04);
        for (vgs, vgd) in [(-1., -1.5), (0.2, -2.), (-1.4, -1.45), (0.3, -0.5)] {
            let forward = model.channel(&op, vgs, vgd).current;
            let reverse = model.channel(&op, vgd, vgs).current;
            assert!((forward + reverse).abs() <= 1e-18, "{vgs} {vgd}");
        }
    }

    /// `qggnew`'s capacitances are its charge's partial derivatives, on both
    /// sides of `vmax` and in both modes.
    #[test]
    fn statz_capacitances_are_the_charge_partials() {
        let (model, op) = device(0.3, 0.);
        for vgs in [-3., -1.4, -0.5, 0., 0.3, 0.6] {
            for vgd in [-4., -1.6, -0.2, 0.2, 0.7] {
                let (_, cgs, cgd) = model.qgg(&op, vgs, vgd);
                let h = 1e-7;
                let q = |a: Real, b: Real| model.qgg(&op, a, b).0;
                let ngs = (q(vgs + h, vgd) - q(vgs - h, vgd)) / (2. * h);
                let ngd = (q(vgs, vgd + h) - q(vgs, vgd - h)) / (2. * h);
                let scale = cgs.abs().max(cgd.abs());
                assert!((ngs - cgs).abs() <= 1e-6 * scale, "{vgs} {vgd}: cgs");
                assert!((ngd - cgd).abs() <= 1e-6 * scale, "{vgs} {vgd}: cgd");
            }
        }
    }

    #[test]
    fn qggnew_needs_pb_above_vmax() {
        let (model, op) = device(0.3, 0.);
        let low = Model { pb: 0.4, ..model };
        let q = low.qgg(&op, 0.8, 0.8);
        assert!(!q.0.is_finite() || !q.1.is_finite());
    }
}
