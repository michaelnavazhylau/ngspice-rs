//! Device parameter descriptions for `.sens` sensitivity analysis.
//!
//! C reference (behaviour only, reimplemented): `sens_sens()` in
//! `src/spicelib/analysis/cktsens.c` and the parameter generator `sgen_init()`/
//! `sgen_next()` in `cktsgen.c`. ngspice's `.sens` does **not** use the
//! SPICE2 adjoint `*sload.c` routines: for every real instance and model
//! parameter it can set and ask, it perturbs the parameter in place
//! (`DEVparam`/`DEVmodParam` followed by `DEVtemperature`), reloads the one
//! device (`DEVload` for DC, `DEVacLoad` for AC) and solves
//! `Y dx = dI - dY x` with the operating-point (or AC) matrix `Y`.
//!
//! The perturbation acts on C's live device structures and is "restored" by
//! calling the setter again with the value it asked before. Setters mark the
//! parameter *given*, some setters are not idempotent, and `DEVsetup` runs
//! once per parameter before the perturbation (so quantities it derives do
//! not follow the perturbed value). Those side effects persist from one
//! parameter to the next, from one frequency to the next and across the
//! instances of a model, and they shape C's numbers. The port therefore
//! models each device as C's own record of fields and *given* flags
//! ([`SensitivityRecord`]) and replays C's sequence of setter, temperature
//! update, setup and load calls on it ([`DeviceSensitivity`]); the analysis
//! (`crate::analysis` `.sens`) owns the order, the bookkeeping and the solves.
//!
//! A device describes itself through [`crate::devices::Device::sensitivity`],
//! whose default is an explicit [`SpiceError::NotYetPorted`]: a device whose
//! C parameters the port cannot reproduce is refused, never silently left
//! out of the parameter list. See `docs/port/SENSITIVITY.md`.

use std::fmt;

mod controlled;
mod misc;
mod reactive;
mod res;
mod source;
pub(crate) use controlled::ControlledSensitivity;
pub(crate) use misc::{BehaviouralSensitivity, MutualSensitivity, SwitchSensitivity};
pub(crate) use reactive::{ReactiveInputs, ReactiveKind, ReactiveSensitivity};
pub(crate) use res::{ResistorInputs, ResistorSensitivity};
pub use source::SourceInputs;
pub(crate) use source::SourceSensitivity;

use crate::devices::{Device, LinearContext, StampContext};
use crate::primitives::{NodeId, Real, SpiceError, SpiceResult};

/// The C device types in the order of C's `DEVices[]` table
/// (`src/spicelib/devices/dev.c`), which is the order `sgen_next()` visits
/// them and therefore the order of the `.sens` output columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SensitivityFamily {
    /// `asrc`: B sources.
    Asrc,
    /// `bjt`.
    Bjt,
    /// `cap`: capacitors.
    Capacitor,
    /// `cccs`: F sources.
    Cccs,
    /// `ccvs`: H sources.
    Ccvs,
    /// `csw`: W switches.
    CurrentSwitch,
    /// `dio`: diodes.
    Diode,
    /// `hfeta`: HFET1 (`z` level 5).
    Hfet1,
    /// `hfet2`: HFET2 (`z` level 6).
    Hfet2,
    /// `ind`: inductors.
    Inductor,
    /// `mut`: K mutual inductance.
    Mutual,
    /// `isrc`: independent current sources.
    CurrentSource,
    /// `jfet`: JFET level 1.
    Jfet,
    /// `jfet2`: JFET level 2 (Parker-Skellern).
    Jfet2,
    /// `mes`: MESFET level 1 (Statz).
    Mes,
    /// `mesa`: MESA levels 2-4.
    Mesa,
    /// `mos1`.
    Mos1,
    /// `mos2`.
    Mos2,
    /// `mos3`.
    Mos3,
    /// `mos6`.
    Mos6,
    /// `mos9`.
    Mos9,
    /// `res`: resistors.
    Resistor,
    /// `sw`: S switches.
    VoltageSwitch,
    /// `vccs`: G sources.
    Vccs,
    /// `vcvs`: E sources.
    Vcvs,
    /// `vsrc`: independent voltage sources.
    VoltageSource,
}

/// Whether a parameter belongs to the model or to the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParameterScope {
    /// A model parameter (`DEVmodParam`), named `<instance>:<keyword>`.
    Model,
    /// An instance parameter (`DEVparam`), named `<instance>_<keyword>`, or
    /// `<instance>` alone for the first principal parameter.
    Instance,
}

/// One entry of a C `IFparm` table that `set_param()` (`cktsgen.c`) accepts:
/// flagged `IF_SET|IF_ASK|IF_REAL`, neither `IF_VECTOR`, `IF_REDUNDANT` nor
/// `IF_NONSENSE`, and asked successfully by the device's `DEVask`. Tables list
/// them in C's table order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitivityParameter {
    /// The C keyword, e.g. `rsh`, `tc1`, `dc`.
    pub keyword: &'static str,
    /// `IF_PRINCIPAL`: the first such parameter of an instance is named after
    /// the instance alone.
    pub principal: bool,
    /// `IF_AC` or `IF_AC_ONLY`: perturbed only by an AC sensitivity analysis.
    pub ac: bool,
}

impl SensitivityParameter {
    /// A parameter that every analysis perturbs.
    #[must_use]
    pub const fn dc(keyword: &'static str) -> Self {
        Self {
            keyword,
            principal: false,
            ac: false,
        }
    }

    /// An `IF_AC`/`IF_AC_ONLY` parameter.
    #[must_use]
    pub const fn ac(keyword: &'static str) -> Self {
        Self {
            keyword,
            principal: false,
            ac: true,
        }
    }

    /// An `IF_PRINCIPAL` parameter, `ac` when it is also `IF_AC`/`IF_AC_ONLY`.
    #[must_use]
    pub const fn principal(keyword: &'static str, ac: bool) -> Self {
        Self {
            keyword,
            principal: true,
            ac,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RecordEntry {
    keyword: &'static str,
    value: Real,
    given: bool,
}

/// C's view of one model or instance structure: named fields with their
/// values and *given* flags. Besides the parameters it holds the derived
/// fields C's temperature routine writes and its load reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SensitivityRecord {
    entries: Vec<RecordEntry>,
}

fn missing(keyword: &str) -> SpiceError {
    SpiceError::circuit(format!("sensitivity record has no field '{keyword}'"))
}

impl SensitivityRecord {
    /// An empty record.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The record with one more field.
    #[must_use]
    pub fn with(mut self, keyword: &'static str, value: Real, given: bool) -> Self {
        self.entries.push(RecordEntry {
            keyword,
            value,
            given,
        });
        self
    }

    fn entry(&self, keyword: &str) -> SpiceResult<&RecordEntry> {
        self.entries
            .iter()
            .find(|entry| entry.keyword == keyword)
            .ok_or_else(|| missing(keyword))
    }

    fn entry_mut(&mut self, keyword: &str) -> SpiceResult<&mut RecordEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.keyword == keyword)
            .ok_or_else(|| missing(keyword))
    }

    /// A field's value.
    ///
    /// # Errors
    /// The record has no such field.
    pub fn value(&self, keyword: &str) -> SpiceResult<Real> {
        Ok(self.entry(keyword)?.value)
    }

    /// A field's *given* flag (`false` for a field the record lacks).
    #[must_use]
    pub fn given(&self, keyword: &str) -> bool {
        self.entry(keyword).is_ok_and(|entry| entry.given)
    }

    /// C's plain setter: stores `value` and marks the field given.
    ///
    /// # Errors
    /// The record has no such field.
    pub fn set(&mut self, keyword: &str, value: Real) -> SpiceResult<()> {
        let entry = self.entry_mut(keyword)?;
        entry.value = value;
        entry.given = true;
        Ok(())
    }

    /// Stores a value without touching the *given* flag (a derived field, or
    /// a default C's setup or temperature routine fills in).
    ///
    /// # Errors
    /// The record has no such field.
    pub fn store(&mut self, keyword: &str, value: Real) -> SpiceResult<()> {
        self.entry_mut(keyword)?.value = value;
        Ok(())
    }

    /// Changes only the *given* flag.
    ///
    /// # Errors
    /// The record has no such field.
    pub fn set_given(&mut self, keyword: &str, given: bool) -> SpiceResult<()> {
        self.entry_mut(keyword)?.given = given;
        Ok(())
    }
}

/// A model record and an instance record, as one device load sees them.
#[derive(Debug, Clone, Copy)]
pub struct RecordPair<'a> {
    /// The model's record (shared by every instance of the model).
    pub model: &'a SensitivityRecord,
    /// The instance's record.
    pub instance: &'a SensitivityRecord,
}

/// Which load a sensitivity analysis performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitivityMode {
    /// `DEVload` at the DC operating point.
    Dc,
    /// `DEVacLoad` at a frequency.
    Ac,
}

/// What a device loads for one state of its records.
#[derive(Debug)]
pub enum SensitivityLoad {
    /// Exactly what the circuit's own device loads.
    Original,
    /// A stand-in with the same name, terminals, branch rows and state layout
    /// that loads what C's device loads in this state: its Newton stamp
    /// ([`Device::stamp`]) for DC, its small-signal assembly for AC.
    Replacement(Box<dyn Device>),
}

/// One device as `.sens` sees it: C's parameter tables, its records, and its
/// setter, setup, temperature and load routines replayed on those records.
pub trait DeviceSensitivity: fmt::Debug {
    /// The C device type.
    fn family(&self) -> SensitivityFamily;

    /// The model's name, `None` for C's default model of the type (instances
    /// without a model card share it).
    fn model(&self) -> Option<&str>;

    /// The perturbable model parameters (`IFparm` model table order).
    fn model_parameters(&self) -> &'static [SensitivityParameter];

    /// The perturbable instance parameters (`IFparm` instance table order).
    fn instance_parameters(&self) -> &'static [SensitivityParameter];

    /// C's model and instance records after the circuit's own setup and
    /// temperature update. Instances of one model return the same model
    /// record; the analysis keeps the first.
    ///
    /// # Errors
    /// Device data C's records cannot represent.
    fn records(&self) -> SpiceResult<(SensitivityRecord, SensitivityRecord)>;

    /// `DEVask`/`DEVmodAsk`: the value `sgen` reads before perturbing. The
    /// default reads the field of the same name.
    ///
    /// # Errors
    /// An unknown field.
    fn ask(
        &self,
        scope: ParameterScope,
        keyword: &str,
        records: RecordPair<'_>,
    ) -> SpiceResult<Real> {
        match scope {
            ParameterScope::Model => records.model.value(keyword),
            ParameterScope::Instance => records.instance.value(keyword),
        }
    }

    /// `DEVparam`/`DEVmodParam`. The default stores the value and marks the
    /// field given.
    ///
    /// # Errors
    /// An unknown field.
    fn set(
        &self,
        scope: ParameterScope,
        keyword: &str,
        value: Real,
        model: &mut SensitivityRecord,
        instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        match scope {
            ParameterScope::Model => model.set(keyword, value),
            ParameterScope::Instance => instance.set(keyword, value),
        }
    }

    /// The record side effects of `DEVsetup` (defaults for fields not given,
    /// flags it clears). Runs once per parameter, before the perturbation.
    ///
    /// # Errors
    /// An unknown field.
    fn setup(
        &self,
        _model: &mut SensitivityRecord,
        _instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        Ok(())
    }

    /// `DEVtemperature`: recomputes the derived fields from the parameters.
    ///
    /// # Errors
    /// An unknown field or invalid physics.
    fn temperature(
        &self,
        _model: &mut SensitivityRecord,
        _instance: &mut SensitivityRecord,
    ) -> SpiceResult<()> {
        Ok(())
    }

    /// What the device loads with the records `current`; `setup` holds the
    /// records as `DEVsetup` last saw them, for quantities only setup derives.
    ///
    /// # Errors
    /// Device physics the port cannot evaluate in this state.
    fn load(
        &self,
        setup: RecordPair<'_>,
        current: RecordPair<'_>,
        mode: SensitivityMode,
    ) -> SpiceResult<SensitivityLoad>;

    /// `DEVacLoad` as `sens_sens()` (`cktsens.c`) calls it in an AC sweep.
    /// Before every frequency C runs `CKTunsetup`/`CKTsetup` (fresh, zeroed
    /// state vectors and solution), `CKTtemp` and a DC load
    /// (`MODEDCOP|MODEINITFLOAT`) at all-zero node voltages, then
    /// `NIacIter`. So an AC load reads whatever that DC load left in
    /// `CKTstate0` (a junction's conductance at zero bias, computed from
    /// `state`, the records at that load), zero for the state slots only a
    /// small-signal or transient load writes (charge capacitances), and its
    /// temperature-derived instance fields from `current`.
    ///
    /// The default is [`Self::load`] in AC mode, right for devices whose AC
    /// load reads no state. A device with state must override it.
    ///
    /// # Errors
    /// As [`Self::load`]; a device whose reset AC load is not ported.
    fn reset_ac_load(
        &self,
        setup: RecordPair<'_>,
        reset: AcReset<'_>,
        current: RecordPair<'_>,
    ) -> SpiceResult<SensitivityLoad> {
        let _ = reset;
        self.load(setup, current, SensitivityMode::Ac)
    }

    /// The C file of this device's AC load when that load reads instance
    /// fields only a small-signal (`MODEINITSMSIG`) or transient load writes
    /// (MOS `capbd`/`capbs`). `cktsens.c`'s reset does not touch them, so
    /// they hold whatever an earlier analysis of the deck left
    /// ([`AcReset::small_signal`]), or C's zero initialisation when none did;
    /// after a `.tran` the analysis refuses.
    fn stale_ac_fields(&self) -> Option<&'static str> {
        None
    }

    /// For a K coupling, the names of its two inductors: the analysis
    /// mirrors their `INDinduct` and branch rows into this device's instance
    /// record (`induct1`, `induct2`, `row1`, `row2`), which `MUTtemp` and
    /// `MUTacLoad` read through C's instance pointers.
    fn coupled_inductors(&self) -> Option<[&str; 2]> {
        None
    }
}

/// What `cktsens.c`'s per-frequency reset leaves for an AC load
/// ([`DeviceSensitivity::reset_ac_load`]).
#[derive(Debug, Clone, Copy)]
pub struct AcReset<'a> {
    /// The records the reset DC load saw.
    pub state: RecordPair<'a>,
    /// C's `CKTomega`.
    pub omega: Real,
    /// The operating point at which an earlier analysis's `MODEINITSMSIG`
    /// load wrote the instance fields the reset leaves alone
    /// ([`DeviceSensitivity::stale_ac_fields`]); `None` when no earlier
    /// analysis did, so they still hold C's zero initialisation.
    pub small_signal: Option<&'a crate::maths::Vector>,
}

/// Where a [`ResetStamps`] entry goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampAt {
    /// Between two circuit nodes (ground rows and columns are skipped).
    Nodes(NodeId, NodeId),
    /// Between two matrix rows (C's branch-equation pointers).
    Rows(usize, usize),
    /// Node row, this device's branch column.
    NodeBranch(NodeId),
    /// This device's branch row, node column.
    BranchNode(NodeId),
}

/// Which part of an element a [`ResetStamp`] adds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampPart {
    /// The real part.
    Real,
    /// The imaginary part, `omega * value` (a capacitance or inductance).
    Reactive,
    /// The imaginary part, `value` as given (C already multiplied by
    /// `CKTomega`).
    Imaginary,
}

/// One element addition of a C `DEVacLoad`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResetStamp {
    /// The element.
    pub at: StampAt,
    /// The addend.
    pub value: Real,
    /// Where it goes.
    pub part: StampPart,
}

/// A stand-in that adds a fixed list of elements in C's order: what a C AC
/// load stamps from the state of `cktsens.c`'s reset load
/// ([`DeviceSensitivity::reset_ac_load`]). It has no DC load.
#[derive(Debug, Clone, PartialEq)]
pub struct ResetStamps {
    name: String,
    designator: char,
    terminals: Vec<NodeId>,
    stamps: Vec<ResetStamp>,
}

impl ResetStamps {
    /// A stand-in for device `name` adding `stamps` in order.
    ///
    /// # Errors
    /// A nonfinite addend.
    pub fn new(
        name: &str,
        designator: char,
        terminals: &[NodeId],
        stamps: Vec<ResetStamp>,
    ) -> SpiceResult<Self> {
        if let Some(stamp) = stamps.iter().find(|stamp| !stamp.value.is_finite()) {
            return Err(SpiceError::Numerical {
                context: format!("AC sensitivity of {name}"),
                message: format!(
                    "the reset load gives a nonfinite matrix element {}",
                    stamp.value
                ),
            });
        }
        Ok(Self {
            name: name.to_owned(),
            designator,
            terminals: terminals.to_vec(),
            stamps,
        })
    }
}

impl Device for ResetStamps {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        self.designator
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn stamp(&self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::circuit(format!(
            "{}: an AC sensitivity stand-in has no DC load",
            self.name
        )))
    }

    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        for stamp in &self.stamps {
            let (row, col) = match stamp.at {
                StampAt::Nodes(a, b) => {
                    match (context.unknowns.node_row(a), context.unknowns.node_row(b)) {
                        (Some(row), Some(col)) => (row, col),
                        _ => continue,
                    }
                }
                StampAt::Rows(row, col) => (row, col),
                StampAt::NodeBranch(node) | StampAt::BranchNode(node) => {
                    let branch = context.branch.ok_or_else(|| {
                        SpiceError::circuit(format!("{}: missing branch row", self.name))
                    })?;
                    let Some(row) = context.unknowns.node_row(node) else {
                        continue;
                    };
                    if matches!(stamp.at, StampAt::NodeBranch(_)) {
                        (row, branch)
                    } else {
                        (branch, row)
                    }
                }
            };
            match stamp.part {
                StampPart::Real => context.system.a.add(row, col, stamp.value)?,
                StampPart::Reactive => context.system.e.add(row, col, stamp.value)?,
                StampPart::Imaginary => context.dynamic_prescaled(row, col, stamp.value)?,
            }
        }
        Ok(())
    }
}

/// The explicit refusal of `.sens` for a device whose sensitivity is not
/// ported.
#[must_use]
pub fn not_ported(name: &str, designator: char, detail: &str) -> SpiceError {
    SpiceError::not_yet_ported(
        format!("sensitivity analysis of device {name} (designator '{designator}'){detail}"),
        "src/spicelib/analysis/cktsens.c, cktsgen.c and the device's DEVparam/DEVtemperature/DEVload",
    )
}

/// A two-terminal conductance standing in for a resistor (or switch) in a
/// sensitivity load: `dc` for the Newton stamp (`RESload`), `ac` for the
/// small-signal assembly (`RESacload`, which uses `RESacConduct`).
#[derive(Debug, Clone, PartialEq)]
pub struct SensitivityConductance {
    name: String,
    designator: char,
    terminals: [NodeId; 2],
    dc: Real,
    ac: Real,
}

impl SensitivityConductance {
    /// A stand-in conductance.
    ///
    /// # Errors
    /// A nonfinite conductance.
    pub fn new(
        name: &str,
        designator: char,
        terminals: [NodeId; 2],
        dc: Real,
        ac: Real,
    ) -> SpiceResult<Self> {
        if !dc.is_finite() || !ac.is_finite() {
            return Err(SpiceError::Numerical {
                context: format!("sensitivity of {name}"),
                message: format!("nonfinite conductance (dc {dc}, ac {ac})"),
            });
        }
        Ok(Self {
            name: name.to_owned(),
            designator,
            terminals,
            dc,
            ac,
        })
    }
}

impl Device for SensitivityConductance {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        self.designator
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        crate::devices::linear::nodal_stamp(
            context.matrix,
            context.unknowns,
            self.terminals,
            self.dc,
        )
    }

    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.terminals, self.ac, false)
    }
}

/// ngspice `CONSTCtoK`.
pub(crate) const CELSIUS_TO_KELVIN: Real = 273.15;

/// A switch model's setters as written (`None` where the card gave none):
/// threshold, hysteresis, on and off resistance.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SwitchValues {
    /// `vt`/`it`.
    pub threshold: Option<Real>,
    /// `vh`/`ih`.
    pub hysteresis: Option<Real>,
    /// `ron`.
    pub on: Option<Real>,
    /// `roff`.
    pub off: Option<Real>,
}

/// A setter the card wrote (`Some`), or `None` for a schema default.
pub(crate) fn written(values: &crate::devices::schema::ScalarValues, name: &str) -> Option<Real> {
    values
        .get(name)
        .filter(|value| value.location.is_some())
        .map(|value| value.value)
}

#[cfg(test)]
mod tests {
    use super::SensitivityRecord;

    #[test]
    fn records_track_values_and_given_flags() {
        let mut record = SensitivityRecord::new()
            .with("tc1", 0.0, false)
            .with("m", 1.0, true);
        assert!(!record.given("tc1"));
        record.set("tc1", 1e-3).unwrap();
        assert!(record.given("tc1"));
        assert_eq!(record.value("tc1").unwrap(), 1e-3);
        record.store("m", 2.0).unwrap();
        assert!(record.given("m"));
        record.set_given("m", false).unwrap();
        assert!(!record.given("m"));
        assert!(record.value("nope").is_err());
        assert!(!record.given("nope"));
    }
}
