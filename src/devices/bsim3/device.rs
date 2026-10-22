//! The BSIM3 device: `b3set.c`/`b3temp.c` setup and the `b3ld.c`,
//! `b3acld.c`, `b3pzld.c`, `b3noi.c` and `b3cvtest.c` routines wired into
//! the solver.
//!
//! The ported C routines see the circuit through [`super::host`]; this file
//! implements those traits over [`StampContext`] and [`LinearContext`] and
//! maps the port's analysis phases onto C's `CKTmode` bits:
//!
//! | Port | C `CKTmode` |
//! | --- | --- |
//! | `uic` initial load | `MODETRANOP \| MODEUIC \| MODEINITJCT` |
//! | `.op` phases Junction/Fix/Float | `MODEDCOP \| MODEINITJCT/FIX/FLOAT` |
//! | `.dc` sweep phases (Predict = warm start) | `MODEDCTRANCURVE \| ...` (`MODEINITPRED`) |
//! | `.tran` first point / later points | `MODETRAN \| MODEINITTRAN` / `MODEINITPRED`, then `MODEINITFLOAT` |
//! | small-signal and asks | `MODEDCOP \| MODEINITSMSIG` |
//!
//! C keeps one `BSIM3instance` whose fields each load overwrites and the
//! convergence test, the AC load and the asks read back. The device keeps
//! the same struct in a cell (the "live" instance): every load updates it,
//! as C's does. Small-signal assemblies and asks run C's `MODEINITSMSIG`
//! load on a copy, so they never disturb a running solve.
//!
//! `capMod` 0, 1 and 2 and the NQS models are translated with the rest of
//! `b3ld.c` but not yet verified against C, so every analysis whose result
//! depends on the charges (transient, `uic`, AC, pole-zero, noise and the
//! charge asks) refuses them (#190, #191); DC results do not depend on them.
use std::cell::RefCell;
use std::rc::Rc;

use super::fields::{Instance, Model, SizeParam};
use super::host::{LoadCkt, SmallSignalCkt, mode, node, state};
use super::{Inputs, ModelParameters, acld, apply, cvtest, elements, load, noi, pzld, set, temp};
use crate::devices::linear::LinearContext;
use crate::devices::models::{ModelContext, ResolvedModel};
use crate::devices::schema::temperature_kelvin;
use crate::devices::state::IterationPhase;
use crate::devices::traits::{AnalysisMode, ConvergenceContext, Device, StampContext};
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SourceLoc, SpiceError, SpiceResult};

/// One temperature's setup: the model after `b3set.c` and `b3temp.c`, this
/// instance after both, and its size-dependent parameters.
pub(crate) struct Prepared {
    pub(crate) temperature: Real,
    pub(crate) nominal: Real,
    /// The `.dc @inst[param]` values applied (field, value).
    pub(crate) swept: Vec<(&'static str, Real)>,
    pub(crate) model: Model,
    pub(crate) instance: Instance,
    pub(crate) size: SizeParam,
    /// `b3set.c`/`b3temp.c`/`b3check.c` messages, in C order. Kept for
    /// inspection: the port has no channel that reports device warnings
    /// yet (C prints them).
    #[allow(dead_code)]
    pub(crate) warnings: Vec<String>,
}

/// `BSIM3setup`'s result: the model, the instance and the messages so far.
pub(crate) struct SetUp {
    pub(crate) model: Model,
    pub(crate) instance: Instance,
    pub(crate) warnings: Vec<String>,
    /// Internal nodes C creates, `(local index, suffix)` in order.
    pub(crate) created: Vec<(i32, &'static str)>,
}

/// `b3mpar.c`/`b3par.c` setters and `BSIM3setup` at this nominal
/// temperature (C's `CKTnomTemp`, the TNOM default).
///
/// # Errors
/// Setter plumbing failures and `acmMod > 0`.
pub(crate) fn setup(
    params: &ModelParameters,
    inputs: &Inputs,
    nominal: Real,
    location: &SourceLoc,
) -> SpiceResult<SetUp> {
    let mut warnings = Vec::new();
    let mut model = apply::model(params)?;
    set::setup_model(
        &mut model,
        temperature_kelvin(nominal, location)?,
        &mut warnings,
    );
    let mut here = apply::instance(inputs)?;
    here.dNode = node::D;
    here.gNode = node::G;
    here.sNode = node::S;
    here.bNode = node::B;
    let created = set::setup_instance(&model, &mut here, &mut warnings)?;
    Ok(SetUp {
        model,
        instance: here,
        warnings,
        created,
    })
}

/// One BSIM3 instance.
pub(crate) struct Bsim3 {
    name: String,
    model_name: String,
    /// External drain, gate, source, bulk, then the internal nodes in
    /// `b3set.c`'s creation order.
    terminals: Vec<NodeId>,
    /// Local node index ([`node`]) to circuit node. Uncreated internal
    /// indices are never looked up: the instance's node fields alias them
    /// to external indices, as in C.
    nodes: [NodeId; node::COUNT],
    params: ModelParameters,
    inputs: Inputs,
    location: SourceLoc,
    /// `b3set.c`'s created nodes, fixed at instantiation.
    created: Vec<(i32, &'static str)>,
    /// `.dc @inst[param]` values (C's `DCTsetInstParam`): `BSIM3param`
    /// stores them after `BSIM3setup`, and `BSIM3temp` follows. Each is the
    /// setter's C field and given bit with its value.
    swept: Vec<(&'static str, Option<&'static str>, Real)>,
    /// C's structs, shared with this instance's `.dc @inst[param]`
    /// stand-ins (which C's `DCTsetInstParam` writes into the same struct).
    shared: Rc<RefCell<Shared>>,
}

/// The C structs of one BSIM3 instance.
#[derive(Default)]
struct Shared {
    /// Recent setups by temperature, nominal temperature and swept values.
    setups: Vec<Rc<Prepared>>,
    /// C's instance struct as the last load left it, and its setup.
    live: Option<(Rc<Prepared>, Instance)>,
}

/// How many setups [`Shared`] keeps.
const SETUPS_KEPT: usize = 8;

impl std::fmt::Debug for Bsim3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bsim3")
            .field("name", &self.name)
            .field("model", &self.model_name)
            .finish_non_exhaustive()
    }
}

impl Bsim3 {
    /// Build an instance, running C's setup and temperature passes before
    /// interning any node, so a fatal `b3check.c` result creates nothing.
    ///
    /// # Errors
    /// Invalid input, setup or temperature failures, node name collisions.
    pub(crate) fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if instance.nodes.len() != 4 {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "BSIM3 needs four terminals: drain, gate, source, bulk",
            ));
        }
        let params = super::model_parameters(model)?;
        let inputs = params.instance_parameters(instance, 1.)?;
        let mut device = Self {
            name: instance.name.clone(),
            model_name: model.card().name.clone(),
            terminals: Vec::new(),
            nodes: [NodeId::GROUND; node::COUNT],
            params,
            inputs,
            location: instance.location.clone(),
            created: Vec::new(),
            swept: Vec::new(),
            shared: Rc::new(RefCell::new(Shared::default())),
        };
        device.created = setup(
            &device.params,
            &device.inputs,
            context.nominal_temperature,
            &device.location,
        )?
        .created;
        let prepared = device.prepare(context.temperature, context.nominal_temperature)?;
        let mut staged = nodes.clone();
        let external: Vec<NodeId> = instance.nodes.iter().map(|n| staged.intern(n)).collect();
        device.terminals.clone_from(&external);
        for (local, id) in [node::D, node::G, node::S, node::B]
            .into_iter()
            .zip(external)
        {
            device.nodes[local as usize] = id;
        }
        for &(local, suffix) in &device.created {
            let name = format!("{}#{suffix}", instance.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "{}: internal node name collision: {name}",
                    instance.name
                )));
            }
            let id = staged.intern(&name);
            staged.set_kind(id, NodeKind::Internal);
            let index = usize::try_from(local)
                .ok()
                .filter(|&i| i < node::COUNT)
                .ok_or_else(|| SpiceError::circuit("BSIM3 setup created an invalid node"))?;
            device.nodes[index] = id;
            device.terminals.push(id);
        }
        *nodes = staged;
        device.shared.borrow_mut().setups.push(Rc::new(prepared));
        Ok(Box::new(device))
    }

    /// `BSIM3temp` at these temperatures (Celsius) on C's structs.
    ///
    /// The first call runs `BSIM3setup` and `BSIM3temp`. C keeps one model
    /// and one instance struct and runs `BSIM3setup` once, while each later
    /// `BSIM3temp` (a new analysis temperature, `.dc temp`, a
    /// `.dc @inst[param]` point) works on those structs as the previous pass
    /// left them. So later calls start from the first call's model and
    /// instance and apply the swept values. Every pass rebuilds the
    /// size-dependent knots, and `b3check.c` checks each new knot, as C does.
    fn prepare(&self, temperature: Real, nominal: Real) -> SpiceResult<Prepared> {
        let first = self
            .shared
            .try_borrow()
            .ok()
            .and_then(|shared| shared.setups.first().cloned());
        let (mut model, mut instance, mut warnings) = match first {
            None => {
                let SetUp {
                    model,
                    instance,
                    warnings,
                    created,
                } = setup(&self.params, &self.inputs, nominal, &self.location)?;
                if created != self.created && !self.created.is_empty() {
                    return Err(SpiceError::circuit(format!(
                        "{}: BSIM3 internal nodes changed between setups",
                        self.name
                    )));
                }
                (model, instance, warnings)
            }
            Some(first) => (first.model.clone(), first.instance.clone(), Vec::new()),
        };
        for &(field, given, value) in &self.swept {
            let slot = instance.real_mut(field).ok_or_else(|| {
                SpiceError::circuit(format!("BSIM3 swept field {field} has no member"))
            })?;
            *slot = value;
            if let Some(flag) = given.and_then(|given| instance.flag_mut(given)) {
                *flag = true;
            }
        }
        let kelvin = temperature_kelvin(temperature, &self.location)?;
        let locals = temp::temperature_model(&mut model, kelvin, &mut warnings);
        let size = temp::temperature_instance(
            &mut model,
            &mut instance,
            &locals,
            &mut temp::SizeCache::new(),
            false,
            &self.model_name,
            &self.name,
            &mut warnings,
        )
        .map_err(|e| match e {
            SpiceError::Circuit { message } => {
                SpiceError::parse(self.location.clone(), format!("{}: {message}", self.name))
            }
            other => other,
        })?;
        Ok(Prepared {
            temperature,
            nominal,
            swept: self
                .swept
                .iter()
                .map(|&(field, _, value)| (field, value))
                .collect(),
            model,
            instance,
            size,
            warnings,
        })
    }

    /// The setup for these temperatures (and this device's swept values),
    /// rebuilt when C would re-run `BSIM3temp`.
    fn ensure(&self, temperature: Real, nominal: Real) -> SpiceResult<Rc<Prepared>> {
        let same = |p: &Prepared| {
            p.temperature == temperature
                && p.nominal == nominal
                && p.swept.len() == self.swept.len()
                && p.swept
                    .iter()
                    .zip(&self.swept)
                    .all(|(a, b)| a.0 == b.0 && a.1.to_bits() == b.2.to_bits())
        };
        {
            let shared = self.borrow_shared()?;
            if let Some(prepared) = shared.setups.iter().find(|p| same(p)) {
                return Ok(Rc::clone(prepared));
            }
        }
        let prepared = Rc::new(self.prepare(temperature, nominal)?);
        let mut shared = self.borrow_shared_mut()?;
        // The first setup is C's struct as `BSIM3setup` and the first
        // `BSIM3temp` left it; later passes start from it, so keep it.
        if shared.setups.len() >= SETUPS_KEPT {
            shared.setups.remove(1);
        }
        shared.setups.push(Rc::clone(&prepared));
        Ok(prepared)
    }

    fn borrow_shared(&self) -> SpiceResult<std::cell::Ref<'_, Shared>> {
        self.shared
            .try_borrow()
            .map_err(|_| SpiceError::circuit(format!("{}: reentrant BSIM3 access", self.name)))
    }

    fn borrow_shared_mut(&self) -> SpiceResult<std::cell::RefMut<'_, Shared>> {
        self.shared
            .try_borrow_mut()
            .map_err(|_| SpiceError::circuit(format!("{}: reentrant BSIM3 access", self.name)))
    }

    /// Any setup, for what does not depend on temperature (nodes, matrix
    /// elements).
    fn any_setup(&self) -> Option<Rc<Prepared>> {
        self.shared.try_borrow().ok()?.setups.last().cloned()
    }

    /// The circuit node of a local node index; `None` for ground.
    fn node_id(&self, local: i32) -> Option<NodeId> {
        if local == node::GROUND {
            return None;
        }
        usize::try_from(local)
            .ok()
            .and_then(|i| self.nodes.get(i))
            .copied()
    }

    /// C's instance struct for `prepared`: the live one when it already
    /// belongs to it, else `prepared`'s with the members the last load wrote
    /// carried over (C re-runs `BSIM3temp` on its one struct).
    fn live_copy(&self, prepared: &Rc<Prepared>) -> Instance {
        let Ok(shared) = self.shared.try_borrow() else {
            return prepared.instance.clone();
        };
        match &shared.live {
            Some((owner, h)) if Rc::ptr_eq(owner, prepared) => h.clone(),
            Some((_, h)) => {
                let mut fresh = prepared.instance.clone();
                carry_load_members(h, &mut fresh);
                fresh
            }
            None => prepared.instance.clone(),
        }
    }

    /// C's `MODEINITSMSIG` load at `state0` (the operating point's state
    /// vector), leaving the small-signal values on the returned instance.
    fn small_signal_point(
        &self,
        prepared: &Rc<Prepared>,
        state0: [Real; state::COUNT],
        voltage: &dyn Fn(NodeId) -> Real,
        gmin: Real,
    ) -> SpiceResult<(Instance, [Real; state::COUNT])> {
        let mut h = self.live_copy(prepared);
        let mut probe = Probe {
            nodes: &self.nodes,
            voltage,
            mode: mode::MODEDCOP | mode::MODEINITSMSIG,
            limiting: false,
            gmin,
            temperature: prepared.model_temperature(),
            s0: state0,
        };
        load::load(&prepared.model, &mut h, &prepared.size, &mut probe)?;
        Ok((h, probe.s0))
    }

    /// The `state0` C's last operating-point load leaves, reproduced at a
    /// solution: an unlimited `MODEINITFLOAT` load there. The analyses hand
    /// devices the accepted-history vector (C's `CKTstate1`), not
    /// `CKTstate0`; C's `CKTstate0` after `CKTop` is what its last load
    /// wrote at the iterate the analysis linearises around.
    fn state_at(
        &self,
        prepared: &Rc<Prepared>,
        voltage: &dyn Fn(NodeId) -> Real,
        gmin: Real,
    ) -> SpiceResult<[Real; state::COUNT]> {
        let mut h = self.live_copy(prepared);
        let mut probe = Probe {
            nodes: &self.nodes,
            voltage,
            mode: mode::MODEDCOP | mode::MODEINITFLOAT,
            limiting: false,
            gmin,
            temperature: prepared.model_temperature(),
            s0: [0.; state::COUNT],
        };
        load::load(&prepared.model, &mut h, &prepared.size, &mut probe)?;
        Ok(probe.s0)
    }

    /// The charge models the port has verified against C: `capMod = 3`
    /// without NQS. Analyses whose results depend on the charges refuse the
    /// others.
    fn require_verified_charges(&self, prepared: &Prepared) -> SpiceResult<()> {
        let (model, here) = (&prepared.model, &prepared.instance);
        if model.capMod != 3 {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: BSIM3 capMod={} charge model (only capMod=3 is verified; \
                     set capMod=3 or omit it)",
                    self.name, model.capMod
                ),
                "src/spicelib/devices/bsim3/b3ld.c (capMod 0/1/2, #190)",
            ));
        }
        if here.nqsMod != 0 || here.acnqsMod != 0 {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: BSIM3 NQS model (nqsMod={}, acnqsMod={})",
                    self.name, here.nqsMod, here.acnqsMod
                ),
                "src/spicelib/devices/bsim3/b3ld.c (nqsMod/acnqsMod, #191)",
            ));
        }
        Ok(())
    }

    /// `b3acld.c` or `b3pzld.c` after the `MODEINITSMSIG` load.
    fn assemble(
        &self,
        context: &mut LinearContext<'_>,
        bias: &crate::maths::Vector,
        pole_zero: bool,
    ) -> SpiceResult<()> {
        let model_context = *context.model_context;
        let prepared = self.ensure(model_context.temperature, model_context.nominal_temperature)?;
        self.require_verified_charges(&prepared)?;
        let unknowns = context.unknowns;
        let voltage = |node: NodeId| {
            unknowns
                .node_row(node)
                .and_then(|row| bias.get(row))
                .unwrap_or(0.)
        };
        let s0 = self.state_at(&prepared, &voltage, model_context.gmin)?;
        let (h, s0) = self.small_signal_point(&prepared, s0, &voltage, model_context.gmin)?;
        let omega = 2.0 * std::f64::consts::PI * model_context.frequency;
        let mut host = SmallSignalHost {
            context,
            nodes: &self.nodes,
            omega,
            s0,
            error: None,
        };
        // `CKTpzSetup` re-runs `BSIM3setup` after `pzan.c`'s `MODEINITSMSIG`
        // load; `b3set.c` keeps a created prime (its field is no longer 0)
        // and re-aliases only what it aliased before, so the nodes are
        // unchanged.
        if pole_zero {
            pzld::pz_load(&prepared.model, &h, &prepared.size, &mut host)?;
        } else {
            acld::ac_load(&prepared.model, &h, &prepared.size, &mut host)?;
        }
        host.error.map_or(Ok(()), Err)
    }

    /// `b3ask.c` at an operating point, on the instance and state the
    /// `MODEINITSMSIG` load leaves.
    fn ask(&self, keyword: &str, h: &Instance, s0: &[Real; state::COUNT]) -> Option<Real> {
        let p = super::descriptor(super::parameters::INSTANCE, keyword)?;
        let m = h.m;
        Some(match p.c_id {
            "BSIM3_SOURCECONDUCT" => h.sourceConductance * m,
            "BSIM3_DRAINCONDUCT" => h.drainConductance * m,
            "BSIM3_VBD" => s0[state::VBD],
            "BSIM3_VBS" => s0[state::VBS],
            "BSIM3_VGS" => s0[state::VGS],
            "BSIM3_VDS" => s0[state::VDS],
            "BSIM3_CD" => h.cd * m,
            "BSIM3_CBS" => h.cbs * m,
            "BSIM3_CBD" => h.cbd * m,
            "BSIM3_GM" => h.gm * m,
            "BSIM3_GDS" => h.gds * m,
            "BSIM3_GMBS" => h.gmbs * m,
            "BSIM3_GBD" => h.gbd * m,
            "BSIM3_GBS" => h.gbs * m,
            "BSIM3_QB" => s0[state::QB] * m,
            "BSIM3_CQB" => s0[state::CQB] * m,
            "BSIM3_QG" => s0[state::QG] * m,
            "BSIM3_CQG" => s0[state::CQG] * m,
            "BSIM3_QD" => s0[state::QD] * m,
            "BSIM3_CQD" => s0[state::CQD] * m,
            "BSIM3_CGG" => h.cggb * m,
            "BSIM3_CGD" => h.cgdb * m,
            "BSIM3_CGS" => h.cgsb * m,
            "BSIM3_CDG" => h.cdgb * m,
            "BSIM3_CDD" => h.cddb * m,
            "BSIM3_CDS" => h.cdsb * m,
            "BSIM3_CBG" => h.cbgb * m,
            "BSIM3_CBDB" => h.cbdb * m,
            "BSIM3_CBSB" => h.cbsb * m,
            "BSIM3_CAPBD" => h.capbd * m,
            "BSIM3_CAPBS" => h.capbs * m,
            "BSIM3_VON" => h.von,
            "BSIM3_VDSAT" => h.vdsat,
            "BSIM3_QBS" => s0[state::QBS] * m,
            "BSIM3_QBD" => s0[state::QBD] * m,
            _ => return parameter_ask(p.c_id, h),
        })
    }
}

/// `b3ask.c`'s bias-independent instance values.
fn parameter_ask(c_id: &str, h: &Instance) -> Option<Real> {
    Some(match c_id {
        "BSIM3_L" => h.l,
        "BSIM3_W" => h.w,
        "BSIM3_M" => h.m,
        "BSIM3_AS" => h.sourceArea,
        "BSIM3_AD" => h.drainArea,
        "BSIM3_PS" => h.sourcePerimeter,
        "BSIM3_PD" => h.drainPerimeter,
        "BSIM3_NRS" => h.sourceSquares,
        "BSIM3_NRD" => h.drainSquares,
        "BSIM3_OFF" => Real::from(h.off),
        "BSIM3_NQSMOD" => Real::from(h.nqsMod),
        "BSIM3_ACNQSMOD" => Real::from(h.acnqsMod),
        "BSIM3_GEO" => Real::from(h.geo),
        "BSIM3_DELVTO" => h.delvto,
        "BSIM3_MULU0" => h.mulu0,
        "BSIM3_IC_VBS" => h.icVBS,
        "BSIM3_IC_VDS" => h.icVDS,
        "BSIM3_IC_VGS" => h.icVGS,
        _ => return None,
    })
}

/// `b3ask.c` keywords that read the charges or their derivatives.
const CHARGE_ASKS: [&str; 19] = [
    "BSIM3_QB",
    "BSIM3_CQB",
    "BSIM3_QG",
    "BSIM3_CQG",
    "BSIM3_QD",
    "BSIM3_CQD",
    "BSIM3_CGG",
    "BSIM3_CGD",
    "BSIM3_CGS",
    "BSIM3_CDG",
    "BSIM3_CDD",
    "BSIM3_CDS",
    "BSIM3_CBG",
    "BSIM3_CBDB",
    "BSIM3_CBSB",
    "BSIM3_CAPBD",
    "BSIM3_CAPBS",
    "BSIM3_QBS",
    "BSIM3_QBD",
];

impl Prepared {
    /// `CKTtemp` in kelvin for the loads.
    fn model_temperature(&self) -> Real {
        self.temperature + 273.15
    }
}

/// Every `here->` member `b3ld.c` writes and `b3temp.c` does not (`load.rs`),
/// copied from C's instance struct as the last load left it onto a freshly
/// temperature-processed one: C runs `BSIM3temp` on its single struct, which
/// rewrites the temperature-derived members (including `cgdo`/`cgso`) and
/// leaves these (the next load reads `BSIM3von` for `DEVfetlim`).
fn carry_load_members(from: &Instance, to: &mut Instance) {
    to.AbovVgst2Vtm = from.AbovVgst2Vtm;
    to.Abulk = from.Abulk;
    to.Vdseff = from.Vdseff;
    to.Vgsteff = from.Vgsteff;
    to.capbd = from.capbd;
    to.capbs = from.capbs;
    to.cbd = from.cbd;
    to.cbdb = from.cbdb;
    to.cbgb = from.cbgb;
    to.cbs = from.cbs;
    to.cbsb = from.cbsb;
    to.cd = from.cd;
    to.cddb = from.cddb;
    to.cdgb = from.cdgb;
    to.cdsb = from.cdsb;
    to.cgdb = from.cgdb;
    to.cggb = from.cggb;
    to.cgsb = from.cgsb;
    to.cqbb = from.cqbb;
    to.cqdb = from.cqdb;
    to.cqgb = from.cqgb;
    to.cqsb = from.cqsb;
    to.csub = from.csub;
    to.gbbs = from.gbbs;
    to.gbd = from.gbd;
    to.gbds = from.gbds;
    to.gbgs = from.gbgs;
    to.gbs = from.gbs;
    to.gds = from.gds;
    to.gm = from.gm;
    to.gmbs = from.gmbs;
    to.gtau = from.gtau;
    to.gtb = from.gtb;
    to.gtd = from.gtd;
    to.gtg = from.gtg;
    to.gts = from.gts;
    to.mode = from.mode;
    to.qbulk = from.qbulk;
    to.qdrn = from.qdrn;
    to.qgate = from.qgate;
    to.qinv = from.qinv;
    to.rds = from.rds;
    to.taunet = from.taunet;
    to.thetavth = from.thetavth;
    to.ueff = from.ueff;
    to.vdsat = from.vdsat;
    to.von = from.von;
}

/// C's `CKTmode` for a load, and whether limiting is allowed.
fn load_mode(context: &StampContext<'_>) -> SpiceResult<(u32, bool)> {
    let states = &context.states;
    if states.initial_conditions() {
        return Ok((mode::MODETRANOP | mode::MODEUIC | mode::MODEINITJCT, true));
    }
    let uic = if states.uic() { mode::MODEUIC } else { 0 };
    let base = match context.mode {
        AnalysisMode::OperatingPoint => mode::MODEDCOP,
        AnalysisMode::DcSweep => mode::MODEDCTRANCURVE,
        AnalysisMode::Transient { .. } => mode::MODETRAN,
        AnalysisMode::Ac { .. } => {
            return Err(SpiceError::circuit(
                "BSIM3 AC is assembled by the small-signal load, not stamped",
            ));
        }
    };
    if !states.device_limiting() {
        return Ok((uic | base | mode::MODEINITFLOAT, false));
    }
    let init = match states.phase() {
        IterationPhase::Junction => mode::MODEINITJCT,
        IterationPhase::Fix => mode::MODEINITFIX,
        IterationPhase::Float => mode::MODEINITFLOAT,
        IterationPhase::Predict if base == mode::MODETRAN && states.initial_transient() => {
            mode::MODEINITTRAN
        }
        IterationPhase::Predict => mode::MODEINITPRED,
    };
    Ok((uic | base | init, true))
}

/// [`LoadCkt`] over a Newton/transient load.
struct StampHost<'h, 'c> {
    context: &'h mut StampContext<'c>,
    nodes: &'h [NodeId; node::COUNT],
    mode: u32,
    limiting: bool,
    temperature: Real,
    s0: [Real; state::COUNT],
    error: Option<SpiceError>,
}

impl StampHost<'_, '_> {
    fn id(&self, local: i32) -> Option<NodeId> {
        if local == node::GROUND {
            return None;
        }
        usize::try_from(local)
            .ok()
            .and_then(|i| self.nodes.get(i))
            .copied()
    }

    fn fail(&mut self, error: SpiceError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    fn accepted(&mut self, age: usize, slot: usize) -> Real {
        if let Some(value) = self.context.states.accepted(age, slot) {
            return value;
        }
        self.fail(SpiceError::circuit(format!(
            "BSIM3 load read CKTstate{age} slot {slot} with no accepted point"
        )));
        0.
    }

    /// Commit `CKTstate0` to the trial.
    fn finish(self) -> SpiceResult<()> {
        if let Some(error) = self.error {
            return Err(error);
        }
        for (slot, value) in self.s0.into_iter().enumerate() {
            self.context.states.set(slot, value)?;
        }
        Ok(())
    }
}

impl LoadCkt for StampHost<'_, '_> {
    fn mode(&self) -> u32 {
        self.mode
    }
    fn limiting(&self) -> bool {
        self.limiting
    }
    fn gmin(&self) -> f64 {
        self.context.gmin
    }
    fn temp(&self) -> f64 {
        self.temperature
    }
    fn ag(&self, i: usize) -> f64 {
        self.context
            .integration
            .and_then(|c| c.ag().get(i).copied())
            .unwrap_or(0.)
    }
    fn delta(&self) -> f64 {
        // dctrcurv.c leaves CKTdelta equal to every CKTdeltaOld, so DEVpred's
        // ratio is one in a sweep.
        self.context.integration.map_or(1., |c| c.dt())
    }
    fn delta_old(&self, i: usize) -> f64 {
        self.context
            .integration
            .map_or(Some(1.), |c| c.delta_old(i))
            .unwrap_or(0.)
    }
    fn rhs_old(&self, node: i32) -> f64 {
        self.id(node).map_or(0., |id| self.context.node_voltage(id))
    }
    fn state0(&self, slot: usize) -> f64 {
        self.s0.get(slot).copied().unwrap_or(0.)
    }
    fn set_state0(&mut self, slot: usize, value: f64) {
        if let Some(entry) = self.s0.get_mut(slot) {
            *entry = value;
        }
    }
    fn state1(&self, slot: usize) -> f64 {
        self.context.states.accepted(1, slot).unwrap_or(f64::NAN)
    }
    fn set_state1(&mut self, slot: usize, value: f64) {
        if let Err(error) = self.context.states.patch_state1(slot, value) {
            self.fail(error);
        }
    }
    fn state2(&self, slot: usize) -> f64 {
        // dctran.c copies state1 into state2 after the first step.
        self.context
            .states
            .accepted(2, slot)
            .or_else(|| self.context.states.accepted(1, slot))
            .unwrap_or(f64::NAN)
    }
    fn integrate(&mut self, cap: f64, slot: usize) -> SpiceResult<(f64, f64)> {
        let Some(coefficients) = self.context.integration else {
            // Outside transient C still integrates (all charges in the `uic`
            // initial load) with the zero `CKTag` of a fresh circuit
            // (`dctran.c` clears it before the first step).
            let q1 = self.context.states.accepted(1, slot).unwrap_or(0.);
            let (geq, ceq, derivative) = zero_coefficient_integrate(cap, self.state0(slot), q1);
            self.set_state0(slot + 1, derivative);
            return Ok((geq, ceq));
        };
        let q0 = self.state0(slot);
        let mut history = crate::maths::integrator::ChargeHistory::new(q0);
        for age in 1..=coefficients.charge_history_len() {
            let q = self.accepted(age, slot);
            history.push(q)?;
        }
        let previous = if coefficients.needs_previous_derivative() {
            Some(self.accepted(1, slot + 1))
        } else {
            None
        };
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        let companion = coefficients.integrate(&history, previous, cap)?;
        self.set_state0(slot + 1, companion.derivative);
        Ok((companion.conductance, companion.current))
    }
    fn noncon(&mut self) {
        if let Err(error) = self.context.states.report_nonconvergence() {
            self.fail(error);
        }
    }
    fn matrix(&mut self, row: i32, col: i32, value: f64) {
        let (Some(r), Some(c)) = (self.id(row), self.id(col)) else {
            return;
        };
        if let Err(error) = self.context.stamp(r, c, value) {
            self.fail(error);
        }
    }
    fn rhs(&mut self, node: i32, value: f64) {
        let Some(n) = self.id(node) else {
            return;
        };
        if let Err(error) = self.context.stamp_rhs(n, value) {
            self.fail(error);
        }
    }
}

/// `NIintegrate` (`niinteg.c`, trapezoidal order 1, C's defaults) with
/// `CKTag[0] = CKTag[1] = 0`: returns `(geq, ceq, state0[ccap])`. C keeps
/// whatever `CKTag` the last transient left; a fresh circuit's is zero,
/// which is what every non-transient analysis of a deck without an earlier
/// `.tran` sees.
fn zero_coefficient_integrate(cap: f64, q0: f64, q1: f64) -> (f64, f64, f64) {
    let ag = [0.0_f64; 2];
    let derivative = ag[0] * q0 + ag[1] * q1;
    (ag[0] * cap, derivative - ag[0] * q0, derivative)
}

/// [`LoadCkt`] for C's evaluation-only loads (`MODEINITSMSIG`, and the
/// unlimited evaluation that reproduces an operating point's `state0`):
/// stamps are discarded, nothing is integrated.
struct Probe<'p> {
    nodes: &'p [NodeId; node::COUNT],
    voltage: &'p dyn Fn(NodeId) -> Real,
    mode: u32,
    limiting: bool,
    gmin: Real,
    temperature: Real,
    s0: [Real; state::COUNT],
}

impl LoadCkt for Probe<'_> {
    fn mode(&self) -> u32 {
        self.mode
    }
    fn limiting(&self) -> bool {
        self.limiting
    }
    fn gmin(&self) -> f64 {
        self.gmin
    }
    fn temp(&self) -> f64 {
        self.temperature
    }
    fn ag(&self, _i: usize) -> f64 {
        0.
    }
    fn delta(&self) -> f64 {
        1.
    }
    fn delta_old(&self, _i: usize) -> f64 {
        1.
    }
    fn rhs_old(&self, node: i32) -> f64 {
        if node == node::GROUND {
            return 0.;
        }
        usize::try_from(node)
            .ok()
            .and_then(|i| self.nodes.get(i))
            .map_or(0., |&id| (self.voltage)(id))
    }
    fn state0(&self, slot: usize) -> f64 {
        self.s0.get(slot).copied().unwrap_or(0.)
    }
    fn set_state0(&mut self, slot: usize, value: f64) {
        if let Some(entry) = self.s0.get_mut(slot) {
            *entry = value;
        }
    }
    fn state1(&self, slot: usize) -> f64 {
        self.state0(slot)
    }
    fn set_state1(&mut self, _slot: usize, _value: f64) {}
    fn state2(&self, slot: usize) -> f64 {
        self.state0(slot)
    }
    fn integrate(&mut self, cap: f64, slot: usize) -> SpiceResult<(f64, f64)> {
        let q = self.state0(slot);
        let (geq, ceq, derivative) = zero_coefficient_integrate(cap, q, q);
        self.set_state0(slot + 1, derivative);
        Ok((geq, ceq))
    }
    fn noncon(&mut self) {}
    fn matrix(&mut self, _row: i32, _col: i32, _value: f64) {}
    fn rhs(&mut self, _node: i32, _value: f64) {}
}

/// [`SmallSignalCkt`] over a small-signal or pole-zero assembly.
struct SmallSignalHost<'h, 'c> {
    context: &'h mut LinearContext<'c>,
    nodes: &'h [NodeId; node::COUNT],
    omega: Real,
    s0: [Real; state::COUNT],
    error: Option<SpiceError>,
}

impl SmallSignalHost<'_, '_> {
    fn rows(&self, row: i32, col: i32) -> Option<(usize, usize)> {
        let id = |local: i32| {
            if local == node::GROUND {
                return None;
            }
            usize::try_from(local)
                .ok()
                .and_then(|i| self.nodes.get(i))
                .copied()
        };
        let (r, c) = (id(row)?, id(col)?);
        Some((
            self.context.unknowns.node_row(r)?,
            self.context.unknowns.node_row(c)?,
        ))
    }

    fn record(&mut self, result: SpiceResult<()>) {
        if let Err(error) = result
            && self.error.is_none()
        {
            self.error = Some(error);
        }
    }
}

impl SmallSignalCkt for SmallSignalHost<'_, '_> {
    fn omega(&self) -> f64 {
        self.omega
    }
    fn state0(&self, slot: usize) -> f64 {
        self.s0.get(slot).copied().unwrap_or(0.)
    }
    fn real(&mut self, row: i32, col: i32, value: f64) {
        if let Some((r, c)) = self.rows(row, col) {
            if let Some(statements) = self.context.pole_zero_statements.as_deref_mut() {
                statements.push(crate::maths::pencil::PencilTerm {
                    row: r,
                    col: c,
                    conductance: Some(value),
                    capacitance: None,
                });
            }
            let result = self.context.system.a.add(r, c, value);
            self.record(result);
        }
    }
    fn imag(&mut self, row: i32, col: i32, value: f64) {
        if let Some((r, c)) = self.rows(row, col) {
            let result = self.context.dynamic_prescaled(r, c, value);
            self.record(result);
        }
    }
    fn cap(&mut self, row: i32, col: i32, value: f64) {
        if let Some((r, c)) = self.rows(row, col) {
            if let Some(statements) = self.context.pole_zero_statements.as_deref_mut() {
                statements.push(crate::maths::pencil::PencilTerm {
                    row: r,
                    col: c,
                    conductance: None,
                    capacitance: Some(value),
                });
            }
            let result = self.context.dynamic_group(r, c, &[value], 1., 1.);
            self.record(result);
        }
    }
}

impl Device for Bsim3 {
    fn name(&self) -> &str {
        &self.name
    }

    fn model_name(&self) -> Option<&str> {
        Some(&self.model_name)
    }

    fn designator(&self) -> char {
        'm'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn is_nonlinear(&self) -> bool {
        true
    }

    fn has_start_settings(&self) -> bool {
        self.inputs
            .applied()
            .iter()
            .any(|a| matches!(a.parameter.canonical, "off" | "icvds" | "icvgs" | "icvbs"))
    }

    fn state_count(&self) -> usize {
        state::COUNT
    }

    fn c_device_type(&self) -> crate::devices::setup::CDeviceType {
        crate::devices::setup::CDeviceType::Bsim
    }

    /// `BSIM3trunc` (`b3trunc.c`): `CKTterr` on qb, qg and qd.
    fn truncation_slots(&self) -> Vec<usize> {
        vec![state::QB, state::QG, state::QD]
    }

    fn setup_matrix(
        &self,
        setup: &mut crate::devices::setup::MatrixSetup<'_>,
    ) -> SpiceResult<bool> {
        let Some(prepared) = self.any_setup() else {
            return Ok(false);
        };
        for (row, col) in elements::matrix_elements(&prepared.instance) {
            if let (Some(r), Some(c)) = (self.node_id(row), self.node_id(col)) {
                setup.nodes(r, c);
            }
        }
        Ok(true)
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let prepared = self.ensure(context.temperature, context.nominal_temperature)?;
        let (mode, limiting) = load_mode(context)?;
        if (mode & (mode::MODETRAN | mode::MODEUIC)) != 0 {
            self.require_verified_charges(&prepared)?;
        }
        let current = self
            .borrow_shared()?
            .live
            .as_ref()
            .is_some_and(|(owner, _)| Rc::ptr_eq(owner, &prepared));
        if !current {
            let instance = self.live_copy(&prepared);
            self.borrow_shared_mut()?.live = Some((Rc::clone(&prepared), instance));
        }
        let mut shared = self.borrow_shared_mut()?;
        let Some((_, h)) = shared.live.as_mut() else {
            return Err(SpiceError::circuit("BSIM3 live instance missing"));
        };
        if mode & mode::MODEUIC != 0 && context.states.initial_conditions() {
            // b3getic.c: unset IC components are the solution's terminal
            // voltages (CKTic runs only under MODEUIC).
            let v = |local: i32| {
                self.node_id(local)
                    .map_or(0., |id| context.node_voltage(id))
            };
            if !h.icVBSGiven {
                h.icVBS = v(h.bNode) - v(h.sNode);
            }
            if !h.icVDSGiven {
                h.icVDS = v(h.dNode) - v(h.sNode);
            }
            if !h.icVGSGiven {
                h.icVGS = v(h.gNode) - v(h.sNode);
            }
        }
        let mut s0 = [0.; state::COUNT];
        for (slot, value) in s0.iter_mut().enumerate() {
            if let Some(previous) = context
                .states
                .iterate(slot)
                .or_else(|| context.states.carried(slot))
                .filter(|v| v.is_finite())
            {
                *value = previous;
            }
        }
        let temperature = prepared.model_temperature();
        let mut host = StampHost {
            context,
            nodes: &self.nodes,
            mode,
            limiting,
            temperature,
            s0,
            error: None,
        };
        load::load(&prepared.model, h, &prepared.size, &mut host)?;
        host.finish()
    }

    /// `BSIM3convTest` on the live instance the last load left, reading
    /// `CKTrhsOld` (the iterate that load used) as C does. `NIiter` runs it
    /// in `MODEINITFIX` too, where an `off` instance is not tested.
    fn convergence_test(&self, context: &ConvergenceContext<'_>) -> SpiceResult<bool> {
        let shared = self.borrow_shared()?;
        let Some((prepared, h)) = shared.live.as_ref() else {
            return Ok(true);
        };
        let rhs_old = |local: i32| {
            self.node_id(local)
                .and_then(|id| context.unknowns.node_row(id))
                .and_then(|row| context.previous.get(row))
                .unwrap_or(0.)
        };
        let state0 = |slot: usize| context.states.get(slot).copied().unwrap_or(0.);
        Ok(cvtest::conv_test(
            &prepared.model,
            h,
            &rhs_old,
            &state0,
            match context.phase {
                IterationPhase::Fix => mode::MODEINITFIX,
                _ => mode::MODEINITFLOAT,
            },
            context.reltol,
            context.abstol,
        ))
    }

    /// Every real instance setter of `b3.c`'s table (`DCTfindInstParam`
    /// accepts any settable instance parameter).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        let p = super::descriptor(super::parameters::INSTANCE, keyword)?;
        (p.input_type == super::InputType::Real
            && p.access != super::Access::Output
            && p.c_field.is_some())
        .then_some(p.canonical)
    }

    /// `BSIM3param` then `BSIM3temp` (`dctrcurv.c` `DCTsetInstParam`): the
    /// value is stored after `BSIM3setup`, so setup-derived quantities
    /// (instance defaults, the internal nodes) keep their setup values, as
    /// in C.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let p = super::descriptor(super::parameters::INSTANCE, parameter)
            .filter(|p| self.instance_parameter(p.name).is_some())
            .ok_or_else(|| {
                SpiceError::circuit(format!(
                    "{}: instance parameter {parameter} cannot be swept",
                    self.name
                ))
            })?;
        crate::devices::sweep::check_swept(&self.name, parameter, value, true, "finite")?;
        let field = p.c_field.unwrap_or_default();
        let mut swept = self.swept.clone();
        swept.push((field, p.c_given, value));
        let device = Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            terminals: self.terminals.clone(),
            nodes: self.nodes,
            params: self.params.clone(),
            inputs: self.inputs.clone(),
            location: self.location.clone(),
            created: self.created.clone(),
            swept,
            shared: Rc::clone(&self.shared),
        };
        device.ensure(context.temperature, context.nominal_temperature)?;
        Ok(Box::new(device))
    }

    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        let Some(p) = super::descriptor(super::parameters::INSTANCE, key) else {
            return Ok(None);
        };
        let prepared = self.ensure(context.temperature, context.nominal_temperature)?;
        let h = &prepared.instance;
        Ok(match p.c_id {
            "BSIM3_SOURCECONDUCT" => Some(h.sourceConductance * h.m),
            "BSIM3_DRAINCONDUCT" => Some(h.drainConductance * h.m),
            id => parameter_ask(id, h),
        })
    }

    fn observation_operating(
        &self,
        keyword: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        _transient: bool,
    ) -> SpiceResult<Option<Real>> {
        let prepared = self.ensure(context.temperature, context.nominal_temperature)?;
        if super::descriptor(super::parameters::INSTANCE, keyword)
            .is_some_and(|p| CHARGE_ASKS.contains(&p.c_id))
        {
            self.require_verified_charges(&prepared)?;
        }
        let s0 = self.state_at(&prepared, voltage, context.gmin)?;
        let (h, s0) = self.small_signal_point(&prepared, s0, voltage, context.gmin)?;
        Ok(self.ask(keyword, &h, &s0))
    }

    /// `b3acld.c` reassembled at every frequency: its `xc* = cap * omega`
    /// terms (and the AC NQS terms) are computed with C's `CKTomega`.
    fn small_signal_depends_on_frequency(&self) -> bool {
        true
    }

    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        self.assemble(context, bias, false)
    }

    fn assemble_pole_zero(
        &self,
        context: &mut LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        self.assemble(context, bias, true)
    }

    /// C's BSIM3 has no `DEVdisto` (`bsim3init.c`): `.disto` sees only its
    /// AC load.
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(crate::devices::distortion::DeviceDistortion::Linear)
    }

    /// `b3noi.c` at the analysis frequency, on the instance C's
    /// `MODEINITSMSIG` load leaves: `.rd`, `.rs`, `.id` and `.1overf`, then
    /// the instance total.
    fn noise(
        &self,
        context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        use crate::devices::bsim4::NoiseSource as Source;
        use crate::devices::noise::{DeviceNoise, NoiseFamily, NoiseKind, NoiseSource};
        let model_context = *context.model_context;
        let prepared = self.ensure(model_context.temperature, model_context.nominal_temperature)?;
        self.require_verified_charges(&prepared)?;
        let voltage = |node: NodeId| context.voltage(node);
        let s0 = self.state_at(&prepared, &voltage, model_context.gmin)?;
        let (h, s0) = self.small_signal_point(&prepared, s0, &voltage, model_context.gmin)?;
        let temperature = context.circuit_kelvin();
        let generators = noi::generators(
            &prepared.model,
            &h,
            &prepared.size,
            &|slot| s0.get(slot).copied().unwrap_or(0.),
            model_context.frequency,
            temperature,
        )?;
        let id = |local: i32| self.node_id(local).unwrap_or(NodeId::GROUND);
        let sources = generators
            .into_iter()
            .zip(noi::NAMES)
            .map(|(generator, suffix)| {
                let (nodes, kind) = match generator {
                    Source::Thermal(a, b, conductance) => (
                        [id(a), id(b)],
                        NoiseKind::Thermal {
                            conductance,
                            temperature,
                        },
                    ),
                    Source::Gain(a, b, factor) => ([id(a), id(b)], NoiseKind::Gain { factor }),
                    Source::Shot(a, b, current) => ([id(a), id(b)], NoiseKind::Shot { current }),
                    Source::Zero | Source::Correlated { .. } => (
                        [id(h.dNodePrime), id(h.sNodePrime)],
                        NoiseKind::Gain { factor: 0. },
                    ),
                };
                NoiseSource::new(suffix, nodes, kind)
            })
            .collect();
        Ok(DeviceNoise::Sources {
            family: NoiseFamily::Bsim3,
            model: Some(self.model_name.clone()),
            total: true,
            sources,
        })
    }
}
