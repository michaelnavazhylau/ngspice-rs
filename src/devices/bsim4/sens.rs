//! `.sens` for BSIM4: `cktsens.c`'s generic perturbation replayed on C's
//! structs.
//!
//! C's BSIM4 registers no `DEVsen*` routine, so `sens_sens()` perturbs every
//! model and instance parameter `sgen` enumerates (`IF_SET|IF_ASK|IF_REAL`,
//! not redundant, and askable: `b4mask.c`/`b4ask.c` must answer it, see
//! [`super::asks`]) through `BSIM4mParam`/`BSIM4param`, `BSIM4setup`,
//! `BSIM4temp` and `BSIM4load`/`BSIM4acLoad` on the isolated instance. The
//! records are C's structs themselves: every scalar member of `sBSIM4model`,
//! and of `sBSIM4instance` followed by its `bsim4SizeDependParam` (C's
//! `pParam`), by C member name, so the ports of those routines run on them
//! unchanged.
//!
//! C behaviours the replay reproduces:
//!
//! * The asks read the stored member, and the setters convert again: TNOM is
//!   asked in kelvin and stored `+ 273.15`, so after its row the model's
//!   TNOM stays 273.15 K higher; NDEP above 1e20 and NSD/NGATE above 1e23
//!   (and their L/W/P coefficients) are stored `* 1e-6`, so a perturbation
//!   across the threshold jumps.
//! * `BSIM4setup` runs again before every parameter and is not idempotent:
//!   it creates an internal node only while its member is still zero, and
//!   otherwise aliases it. So from the first parameter on, the drain,
//!   source, gate and mid-gate primes it created alias their external nodes
//!   and `qNode` is ground (the body nodes are kept); the loads and
//!   `delta_Y`'s elements use those aliases. Where an alias is ground (a
//!   grounded source with series resistance, `trnqsMod`), the next re-run
//!   would create a node, which `cktsens.c` treats as a fatal internal error
//!   (`controlled_exit`); the port fails there with C's message.
//! * `BSIM4checkModel` does not run: its `static` model name was set by the
//!   deck's own temperature pass.
//! * AC: `CKTunsetup` resets the node members before each frequency's
//!   `CKTsetup`, which creates the nodes again. The reset load (zero
//!   voltages, no charge computation) writes the DC small-signal members and
//!   zeroes the C-V derivatives; `capbd`/`capbs`, which only a charge load
//!   writes, hold what an earlier analysis's `MODEINITSMSIG` load left (zero
//!   when none ran). Each parameter's AC load reads those, the reset's
//!   `CKTstate0`, and the setup/temperature members of the present records.
#![allow(non_snake_case)] // C's member names.

use std::rc::Rc;
use std::sync::OnceLock;

use super::device::{Bsim4, ac_stamp, reset_load};
use super::fields::{Instance, Model, SizeParam};
use super::host::node;
use super::{Access, InputType, asks, elements, parameters, set, temp};
use crate::devices::schema::temperature_kelvin;
use crate::devices::sensitivity::{
    AcReset, DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityParameter as P, SensitivityRecord as Record,
};
use crate::devices::{Device, LinearContext, ModelContext, StampContext};
use crate::primitives::{NodeId, Real, SpiceError, SpiceResult};

/// The instance record's next-to-last field: set by `DEVunsetup` (the node
/// members are zero, so the next `BSIM4setup` creates nodes), cleared by
/// setup.
const UNSET: &str = "unsetup";

/// The instance record's last field: a bit per internal node (in
/// [`INTERNAL`] order) that the last `CKTsetup` created, which
/// `BSIM4unsetup` must delete again.
const CREATED: &str = "created";

/// The internal nodes as `(own index, member)`: drain, source, gate and
/// mid-gate primes, the three body nodes and the NQS node.
const INTERNAL: [i32; 8] = [
    node::DP,
    node::SP,
    node::GP,
    node::GM,
    node::DB,
    node::BP,
    node::SB,
    node::Q,
];

/// The instance's internal node members in [`INTERNAL`] order.
fn internal_members(h: &Instance) -> [i32; 8] {
    [
        h.dNodePrime,
        h.sNodePrime,
        h.gNodePrime,
        h.gNodeMid,
        h.dbNode,
        h.bNodePrime,
        h.sbNode,
        h.qNode,
    ]
}

/// The [`CREATED`] bits for the members that hold their own node.
fn created_bits(h: &Instance) -> u32 {
    internal_members(h)
        .into_iter()
        .zip(INTERNAL)
        .enumerate()
        .filter(|(_, (member, own))| member == own)
        .fold(0, |bits, (k, _)| bits | (1 << k))
}

/// Members `b4ld.c` writes that neither `b4set.c` nor `b4temp.c` writes
/// (`cgdo`/`cgso` are written by both): a per-parameter AC load reads them
/// as the frequency's reset load left them.
const LOAD_ONLY: &[&str] = &[
    "BSIM4AbovVgst2Vtm",
    "BSIM4Abulk",
    "BSIM4Coxeff",
    "BSIM4EsatL",
    "BSIM4IdovVds",
    "BSIM4Igb",
    "BSIM4Igcd",
    "BSIM4Igcs",
    "BSIM4Igd",
    "BSIM4Igidl",
    "BSIM4Igisl",
    "BSIM4Igs",
    "BSIM4Vdseff",
    "BSIM4Vgsteff",
    "BSIM4capbd",
    "BSIM4capbs",
    "BSIM4cbbb",
    "BSIM4cbd",
    "BSIM4cbdb",
    "BSIM4cbgb",
    "BSIM4cbs",
    "BSIM4cbsb",
    "BSIM4cd",
    "BSIM4cdbb",
    "BSIM4cddb",
    "BSIM4cdgb",
    "BSIM4cdsb",
    "BSIM4cgbb",
    "BSIM4cgdb",
    "BSIM4cggb",
    "BSIM4cgsb",
    "BSIM4cqbb",
    "BSIM4cqdb",
    "BSIM4cqgb",
    "BSIM4cqsb",
    "BSIM4csbb",
    "BSIM4csdb",
    "BSIM4csgb",
    "BSIM4cssb",
    "BSIM4csub",
    "BSIM4dvgd_eff_dvg",
    "BSIM4dvgs_eff_dvg",
    "BSIM4gIgbb",
    "BSIM4gIgbd",
    "BSIM4gIgbg",
    "BSIM4gIgbs",
    "BSIM4gIgcdb",
    "BSIM4gIgcdd",
    "BSIM4gIgcdg",
    "BSIM4gIgcds",
    "BSIM4gIgcsb",
    "BSIM4gIgcsd",
    "BSIM4gIgcsg",
    "BSIM4gIgcss",
    "BSIM4gIgdd",
    "BSIM4gIgdg",
    "BSIM4gIgsg",
    "BSIM4gIgss",
    "BSIM4gbbs",
    "BSIM4gbd",
    "BSIM4gbds",
    "BSIM4gbgs",
    "BSIM4gbs",
    "BSIM4gcrg",
    "BSIM4gcrgb",
    "BSIM4gcrgd",
    "BSIM4gcrgg",
    "BSIM4gcrgs",
    "BSIM4gds",
    "BSIM4gdtot",
    "BSIM4gdtotb",
    "BSIM4gdtotd",
    "BSIM4gdtotg",
    "BSIM4gdtots",
    "BSIM4ggidlb",
    "BSIM4ggidld",
    "BSIM4ggidlg",
    "BSIM4ggidls",
    "BSIM4ggislb",
    "BSIM4ggisld",
    "BSIM4ggislg",
    "BSIM4ggisls",
    "BSIM4gm",
    "BSIM4gmbs",
    "BSIM4grdsw",
    "BSIM4gstot",
    "BSIM4gstotb",
    "BSIM4gstotd",
    "BSIM4gstotg",
    "BSIM4gstots",
    "BSIM4gtau",
    "BSIM4gtb",
    "BSIM4gtd",
    "BSIM4gtg",
    "BSIM4gts",
    "BSIM4mode",
    "BSIM4noiGd0",
    "BSIM4nstar",
    "BSIM4qbulk",
    "BSIM4qchqs",
    "BSIM4qdrn",
    "BSIM4qgate",
    "BSIM4qgdo",
    "BSIM4qgso",
    "BSIM4qinv",
    "BSIM4qsrc",
    "BSIM4taunet",
    "BSIM4thetavth",
    "BSIM4ueff",
    "BSIM4vdsat",
    "BSIM4vgd_eff",
    "BSIM4vgs_eff",
    "BSIM4von",
];

/// `sgen`'s parameters of one table: real, settable and askable entries
/// that are not redundant, in table order.
fn table(rows: &'static [super::Parameter], asks: &[(&str, &str)]) -> Vec<P> {
    rows.iter()
        .filter(|p| {
            p.input_type == InputType::Real
                && p.access == Access::ReadWrite
                && !p.redundant
                && asks.iter().any(|(id, _)| *id == p.c_id)
        })
        .map(|p| P::dc(p.name))
        .collect()
}

fn model_table() -> &'static [P] {
    static TABLE: OnceLock<Vec<P>> = OnceLock::new();
    TABLE.get_or_init(|| table(parameters::MODEL, asks::MODEL))
}

fn instance_table() -> &'static [P] {
    static TABLE: OnceLock<Vec<P>> = OnceLock::new();
    TABLE.get_or_init(|| table(parameters::INSTANCE, asks::INSTANCE))
}

/// One BSIM4 instance as `.sens` sees it.
pub(crate) struct Bsim4Sensitivity<'a> {
    device: &'a Bsim4,
    context: ModelContext,
    /// The model's VERSION string (not a numeric member).
    version: String,
    model: Model,
    instance: Instance,
    size: SizeParam,
}

impl std::fmt::Debug for Bsim4Sensitivity<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bsim4Sensitivity")
            .field("device", &self.device.name)
            .finish_non_exhaustive()
    }
}

fn record(members: &[&'static str], values: &[Real]) -> Record {
    members
        .iter()
        .zip(values)
        .fold(Record::new(), |record, (&name, &value)| {
            record.with(name, value, false)
        })
}

impl<'a> Bsim4Sensitivity<'a> {
    /// The device's structs as the deck's own setup and temperature pass
    /// left them.
    ///
    /// # Errors
    /// The device's setup failing.
    pub(crate) fn new(device: &'a Bsim4, context: &ModelContext) -> SpiceResult<Self> {
        let prepared = device.ensure(context.temperature, context.nominal_temperature)?;
        Ok(Self {
            device,
            context: *context,
            version: prepared.model.version.clone(),
            model: prepared.model.clone(),
            instance: device.live_copy(&prepared),
            size: prepared.size.clone(),
        })
    }

    fn model_of(&self, record: &Record) -> Model {
        let mut model = Model {
            version: self.version.clone(),
            ..Model::default()
        };
        model.set_values(&record.values());
        model
    }

    /// The instance, its size-dependent parameters and the unsetup flag.
    fn instance_of(record: &Record) -> (Instance, SizeParam, bool) {
        let values = record.values();
        let (instance_values, rest) = values.split_at(Instance::MEMBERS.len());
        let (size_values, flags) = rest.split_at(SizeParam::MEMBERS.len());
        let mut instance = Instance::default();
        instance.set_values(instance_values);
        let mut size = SizeParam::default();
        size.set_values(size_values);
        (instance, size, flags.first().is_some_and(|v| *v != 0.))
    }

    fn store_instance(
        record: &mut Record,
        instance: &Instance,
        size: &SizeParam,
        unset: bool,
    ) -> SpiceResult<()> {
        let created = record.value(CREATED)?;
        let mut values = instance.to_values();
        values.extend(size.to_values());
        values.push(if unset { 1. } else { 0. });
        values.push(created);
        record.store_values(&values)
    }

    fn kelvin(&self, celsius: Real) -> SpiceResult<Real> {
        temperature_kelvin(celsius, &self.device.location)
    }

    /// C node number 0: ground, or an external node tied to ground.
    fn grounded(&self, local: i32) -> bool {
        local == node::GROUND || self.device.node_id(local) == Some(NodeId::GROUND)
    }

    /// The structs of `pair` as one load sees them.
    fn structs(&self, pair: RecordPair<'_>) -> (Model, Instance, SizeParam) {
        let (instance, size, _) = Self::instance_of(pair.instance);
        (self.model_of(pair.model), instance, size)
    }
}

fn descriptor(scope: ParameterScope, keyword: &str) -> SpiceResult<&'static super::Parameter> {
    let table = match scope {
        ParameterScope::Model => parameters::MODEL,
        ParameterScope::Instance => parameters::INSTANCE,
    };
    super::descriptor(table, keyword)
        .ok_or_else(|| SpiceError::circuit(format!("BSIM4 has no parameter {keyword}")))
}

impl DeviceSensitivity for Bsim4Sensitivity<'_> {
    fn family(&self) -> SensitivityFamily {
        SensitivityFamily::Bsim4
    }

    fn model(&self) -> Option<&str> {
        Some(&self.device.model_name)
    }

    fn model_parameters(&self) -> &'static [P] {
        model_table()
    }

    fn instance_parameters(&self) -> &'static [P] {
        instance_table()
    }

    fn records(&self) -> SpiceResult<(Record, Record)> {
        let mut values = self.instance.to_values();
        values.extend(self.size.to_values());
        values.push(0.);
        values.push(Real::from(created_bits(&self.instance)));
        let mut members: Vec<&'static str> = Instance::MEMBERS.to_vec();
        members.extend(SizeParam::MEMBERS);
        members.push(UNSET);
        members.push(CREATED);
        Ok((
            record(Model::MEMBERS, &self.model.to_values()),
            record(&members, &values),
        ))
    }

    /// `BSIM4mAsk`/`BSIM4ask`: the member the ask reads, unconverted.
    fn ask(
        &self,
        scope: ParameterScope,
        keyword: &str,
        records: RecordPair<'_>,
    ) -> SpiceResult<Real> {
        let p = descriptor(scope, keyword)?;
        let (asks, record) = match scope {
            ParameterScope::Model => (asks::MODEL, records.model),
            ParameterScope::Instance => (asks::INSTANCE, records.instance),
        };
        let member = asks
            .iter()
            .find(|(id, _)| *id == p.c_id)
            .map(|(_, member)| *member)
            .ok_or_else(|| SpiceError::circuit(format!("BSIM4 cannot ask {keyword}")))?;
        record.value(member)
    }

    /// `BSIM4mParam`/`BSIM4param`: the setter's conversion, then the member
    /// and its given bit.
    fn set(
        &self,
        scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut Record,
        instance: &mut Record,
    ) -> SpiceResult<()> {
        let p = descriptor(scope, keyword)?;
        let field = p
            .c_field
            .ok_or_else(|| SpiceError::circuit(format!("BSIM4 has no setter for {keyword}")))?;
        let (record, value) = match scope {
            ParameterScope::Model => (
                model,
                super::bsim4_model_real(p, value, &self.device.location)?,
            ),
            // b4par.c scales geometry by `.options scale`, which is 1 here.
            ParameterScope::Instance => (instance, value),
        };
        record.store(field, value)?;
        if let Some(given) = p.c_given {
            record.store(given, 1.)?;
        }
        Ok(())
    }

    /// `BSIM4unsetup`: it deletes an internal node only while its member
    /// still differs from the external node, then zeroes every member.
    /// `CKTunsetup` exits when a node the last `CKTsetup` created survives,
    /// which happens once a `.sens` setup re-run has aliased it.
    fn unsetup(&self, _model: &mut Record, instance: &mut Record) -> SpiceResult<()> {
        let (mut h, size, _) = Self::instance_of(instance);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let created = instance.value(CREATED)? as u32;
        let survives = internal_members(&h)
            .into_iter()
            .zip(INTERNAL)
            .enumerate()
            .any(|(k, (member, own))| created & (1 << k) != 0 && member != own);
        if survives {
            return Err(SpiceError::circuit(format!(
                "{}: Internal Error: incomplete CKTunsetup(), this will cause serious \
                 problems (cktsens.c's next frequency: BSIM4unsetup keeps the internal \
                 nodes a .sens setup re-run aliased, and C exits here)",
                self.device.name
            )));
        }
        instance.store(CREATED, 0.)?;
        h.dNodePrime = node::GROUND;
        h.sNodePrime = node::GROUND;
        h.gNodePrime = node::GROUND;
        h.gNodeMid = node::GROUND;
        h.dbNode = node::GROUND;
        h.bNodePrime = node::GROUND;
        h.sbNode = node::GROUND;
        h.qNode = node::GROUND;
        Self::store_instance(instance, &h, &size, true)
    }

    /// `BSIM4setup` on the isolated model and instance, run again: the
    /// defaults, then the node members as C's re-run leaves them.
    fn setup(&self, model: &mut Record, instance: &mut Record) -> SpiceResult<()> {
        let mut warnings = Vec::new();
        let mut m = self.model_of(model);
        set::setup_model(
            &mut m,
            self.kelvin(self.context.nominal_temperature)?,
            &mut warnings,
        )?;
        let (mut h, size, unset) = Self::instance_of(instance);
        let previous = [
            h.dNodePrime,
            h.sNodePrime,
            h.gNodePrime,
            h.gNodeMid,
            h.qNode,
        ];
        set::setup_instance(&m, &mut h, self.device.noise_analysis, &mut warnings)?;
        // `if (create && field == 0) CKTmkVolt(...) else field = alias;`
        let primes = [
            (node::DP, h.dNode),
            (node::SP, h.sNode),
            (node::GP, h.gNodeExt),
            (node::GM, h.gNodeExt),
            (node::Q, node::GROUND),
        ];
        let fresh = [
            h.dNodePrime,
            h.sNodePrime,
            h.gNodePrime,
            h.gNodeMid,
            h.qNode,
        ];
        let mut result = [0; 5];
        for (k, ((own, alias), (was, now))) in primes
            .into_iter()
            .zip(previous.into_iter().zip(fresh))
            .enumerate()
        {
            let create = now == own;
            result[k] = if create && (unset || self.grounded(was)) {
                if !unset {
                    return Err(SpiceError::circuit(format!(
                        "{}: Internal Error: node allocation in DEVsetup() during \
                         sensitivity analysis (cktsens.c exits here): b4set.c would create \
                         an internal node again because its alias is ground",
                        self.device.name
                    )));
                }
                own
            } else {
                alias
            };
        }
        [
            h.dNodePrime,
            h.sNodePrime,
            h.gNodePrime,
            h.gNodeMid,
            h.qNode,
        ] = result;
        model.store_values(&m.to_values())?;
        if unset {
            instance.store(CREATED, Real::from(created_bits(&h)))?;
        }
        Self::store_instance(instance, &h, &size, false)
    }

    /// `BSIM4temp` on the isolated model and instance, without
    /// `BSIM4checkModel` (its `static` model name is already set).
    fn temperature(&self, model: &mut Record, instance: &mut Record) -> SpiceResult<()> {
        let mut warnings = Vec::new();
        let mut m = self.model_of(model);
        let (mut h, _, unset) = Self::instance_of(instance);
        let locals = temp::temperature_model(
            &mut m,
            self.kelvin(self.context.temperature)?,
            &mut warnings,
        );
        let before = h.clone();
        let size = match temp::temperature_instance(
            &mut m,
            &mut h,
            &locals,
            false,
            &self.device.model_name,
            &self.device.name,
            &mut warnings,
        ) {
            Ok(size) => size,
            // `sens_temp()` ignores BSIM4temp's E_BADPARM (`(void)`): the
            // model pass stands, the instance keeps its members, and its
            // `pParam` is the freshly allocated (zeroed) set with only the
            // geometry assigned before the failing check.
            Err(SpiceError::NotYetPorted { .. }) | Err(SpiceError::Unsupported { .. }) => {
                return Err(SpiceError::circuit(format!(
                    "{}: BSIM4 temperature pass failed under .sens",
                    self.device.name
                )));
            }
            Err(_) => {
                h = before;
                rejected_size(&m, &h)
            }
        };
        model.store_values(&m.to_values())?;
        Self::store_instance(instance, &h, &size, unset)
    }

    fn setup_elements(&self, setup: RecordPair<'_>) -> Option<Vec<(NodeId, NodeId)>> {
        let (m, h, _) = self.structs(setup);
        Some(
            elements::matrix_elements(&m, &h)
                .into_iter()
                .filter_map(|(row, col)| {
                    Some((self.device.node_id(row)?, self.device.node_id(col)?))
                })
                .collect(),
        )
    }

    /// `BSIM4load` (`MODEDCOP | MODEINITFLOAT`) on the present structs.
    fn load(
        &self,
        _setup: RecordPair<'_>,
        current: RecordPair<'_>,
        mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad> {
        if mode == SensitivityMode::Ac {
            return Err(SpiceError::circuit(
                "BSIM4 AC sensitivity loads go through the reset load",
            ));
        }
        let (m, h, size) = self.structs(current);
        Ok(SensitivityLoad::Replacement(Box::new(
            self.device.stand_in(m, h, size, &self.context),
        )))
    }

    fn stale_ac_fields(&self) -> Option<&'static str> {
        Some("src/spicelib/devices/bsim4/b4acld.c (capbd/capbs)")
    }

    /// `BSIM4acLoad` after `cktsens.c`'s reset load (see the module notes).
    fn reset_ac_load(
        &self,
        _setup: RecordPair<'_>,
        reset: AcReset<'_>,
        current: RecordPair<'_>,
    ) -> SpiceResult<SensitivityLoad> {
        if reset.transient.is_some() {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: BSIM4 .sens ac after .tran (the final transient load's capbd/capbs)",
                    self.device.name
                ),
                "src/spicelib/analysis/cktsens.c and src/spicelib/devices/bsim4/b4acld.c",
            ));
        }
        let (rm, rh, rsize) = self.structs(reset.state);
        let (m, h, size) = self.structs(current);
        Ok(SensitivityLoad::Replacement(Box::new(ResetBsim4 {
            device: self.device.stand_in(m, h, size, &self.context),
            original: self.device.stand_in(
                self.model.clone(),
                self.instance.clone(),
                self.size.clone(),
                &self.context,
            ),
            reset: (rm, rh, rsize),
            small_signal: reset.small_signal.cloned(),
            omega: reset.omega,
            temperature: self.kelvin(self.context.temperature)?,
            gmin: self.context.gmin,
        })))
    }
}

/// The `pParam` `b4temp.c` leaves when one of its effective-geometry checks
/// fails (lines after `if (Size_Not_Found)`): a zeroed allocation holding the
/// members assigned before the first failing check, in C's arithmetic.
fn rejected_size(m: &Model, h: &Instance) -> SizeParam {
    let mut p = SizeParam {
        Length: h.l,
        Width: h.w,
        NFinger: h.nf,
        ..SizeParam::default()
    };
    let Lnew = h.l + m.xl;
    let Wnew = h.w / h.nf + m.xw;
    let T0 = Lnew.powf(m.Lln);
    let T1 = Wnew.powf(m.Lwn);
    let tmp1 = m.Ll / T0 + m.Lw / T1 + m.Lwl / (T0 * T1);
    p.dl = m.Lint + tmp1;
    let tmp2 = m.Llc / T0 + m.Lwc / T1 + m.Lwlc / (T0 * T1);
    p.dlc = m.dlc + tmp2;
    let T2 = Lnew.powf(m.Wln);
    let T3 = Wnew.powf(m.Wwn);
    let tmp1 = m.Wl / T2 + m.Ww / T3 + m.Wwl / (T2 * T3);
    p.dw = m.Wint + tmp1;
    let tmp2 = m.Wlc / T2 + m.Wwc / T3 + m.Wwlc / (T2 * T3);
    p.dwc = m.dwc + tmp2;
    p.dwj = m.dwj + tmp2;
    p.leff = Lnew - 2.0 * p.dl;
    if p.leff <= 0.0 {
        return p;
    }
    p.weff = Wnew - 2.0 * p.dw;
    if p.weff <= 0.0 {
        return p;
    }
    p.leffCV = Lnew - 2.0 * p.dlc;
    if p.leffCV <= 0.0 {
        return p;
    }
    p.weffCV = Wnew - 2.0 * p.dwc;
    if p.weffCV <= 0.0 {
        return p;
    }
    p.weffCJ = Wnew - 2.0 * p.dwj;
    p
}

/// A `.sens` AC stand-in: `cktsens.c`'s reset load on the reset structs,
/// then `b4acld.c` on C's one struct as the later setup and temperature
/// calls left it.
struct ResetBsim4 {
    device: Bsim4,
    /// The deck's own structs, for an earlier small-signal load.
    original: Bsim4,
    reset: (Model, Instance, SizeParam),
    /// The operating point of an earlier `MODEINITSMSIG` load, if any.
    small_signal: Option<crate::maths::Vector>,
    omega: Real,
    temperature: Real,
    gmin: Real,
}

impl std::fmt::Debug for ResetBsim4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResetBsim4")
            .field("device", &self.device.name)
            .finish_non_exhaustive()
    }
}

impl Device for ResetBsim4 {
    fn name(&self) -> &str {
        self.device.name()
    }

    fn designator(&self) -> char {
        'm'
    }

    fn terminals(&self) -> &[NodeId] {
        self.device.terminals()
    }

    fn stamp(&self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::circuit(format!(
            "{}: an AC sensitivity stand-in has no DC load",
            self.device.name()
        )))
    }

    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        _bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        let model_context = *context.model_context;
        let (rm, mut rh, rsize) = (
            self.reset.0.clone(),
            self.reset.1.clone(),
            self.reset.2.clone(),
        );
        if let Some(bias) = &self.small_signal {
            // capbd/capbs as the earlier analysis's MODEINITSMSIG load left
            // them, at the deck's operating point on the deck's structs.
            let unknowns = context.unknowns;
            let voltage = |id: NodeId| {
                unknowns
                    .node_row(id)
                    .and_then(|row| bias.get(row))
                    .unwrap_or(0.)
            };
            let prepared = self
                .original
                .ensure(model_context.temperature, model_context.nominal_temperature)?;
            let s0 = self.original.state_at(&prepared, &voltage, self.gmin)?;
            let (h, _) = self
                .original
                .small_signal_point(&prepared, s0, &voltage, self.gmin)?;
            rh.capbd = h.capbd;
            rh.capbs = h.capbs;
        }
        let s0 = reset_load(
            &self.device.nodes,
            &rm,
            &mut rh,
            &rsize,
            self.temperature,
            self.gmin,
        )?;
        let prepared = self
            .device
            .ensure(model_context.temperature, model_context.nominal_temperature)?;
        let prepared: &Rc<_> = &prepared;
        let mut merged = prepared.instance.to_values();
        let from_reset = rh.to_values();
        for (index, name) in Instance::MEMBERS.iter().enumerate() {
            if LOAD_ONLY.contains(name) {
                merged[index] = from_reset[index];
            }
        }
        let mut h = Instance::default();
        h.set_values(&merged);
        ac_stamp(
            context,
            &self.device.nodes,
            self.omega,
            s0,
            &prepared.model,
            &h,
            &prepared.size,
        )
    }
}
