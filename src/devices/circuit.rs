//! The circuit: nodes, unknowns and device instances.
//!
//! The C equivalent is `CKTcircuit` in `src/include/ngspice/cktdefs.h`, built by
//! `CKTcrtElt`/`CKTbindNode` and friends in `src/spicelib/devices/ckt*.c`. The
//! port keeps the same two-step shape:
//!
//! 1. nodes and devices are *added*, in deck order;
//! 2. [`Circuit::rebuild_unknowns`] numbers the MNA unknowns, which must happen
//!    after every node and device is known, because device-made unknowns
//!    (internal nodes, branch currents) get rows after the parsed nodes, in
//!    C's setup order.
//!
//! Step 2 is easy to forget, so the analyses call
//! [`Circuit::finalize`], which validates the graph and rebuilds the numbering
//! in one go.

use std::collections::BTreeSet;
use std::fmt;

use crate::maths::{Coefficients, SparseMatrix, Vector};
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};
use petgraph::graph::UnGraph;

use crate::devices::linear::Forcing;
use crate::devices::models::ModelContext;
use crate::devices::rlc::Resistor;
use crate::devices::state::{StateHistory, TrialState};
use crate::devices::sweep::{InstanceOverride, ResistorMetadata, ResistorOverride};
use crate::devices::traits::{
    AcceptContext, AnalysisMode, Device, InductanceValue, MnaUnknowns, MutualTerm, StampContext,
};

/// A vertex in the circuit's bipartite incidence graph.
///
/// Node IDs and device ordinals are separate namespaces. Neither is an MNA row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitVertex {
    /// A node in the circuit's node table, including ground.
    Node(NodeId),
    /// A device's position in [`Circuit::devices`], in insertion order.
    Device(usize),
}

/// Petgraph circuit incidence graph: node/device vertices and one edge per port.
///
/// Each edge's weight is the zero-based terminal ordinal. Parallel edges retain
/// repeated terminals (e.g. a device whose two ports both connect to ground).
/// Graph indices are snapshot-local handles, not SPICE node IDs or MNA rows.
/// Connectivity is structural, not proof of a conductive DC path or solvability.
pub type CircuitGraph = UnGraph<CircuitVertex, usize, usize>;

/// A circuit under construction, or ready to be simulated.
#[derive(Default)]
pub struct Circuit {
    pub(crate) observations: Vec<String>,
    nodes: NodeTable,
    unknowns: MnaUnknowns,
    devices: Vec<Box<dyn Device>>,
    /// Per device, the model name it was parsed with (lowercase), or `None`
    /// for its type's default model: C's model list membership.
    models: Vec<Option<String>>,
    /// Per device, the internal nodes its factory created, in creation order:
    /// the nodes C's `DEVsetup` makes with `CKTmkVolt`.
    internal_nodes: Vec<Vec<NodeId>>,
    /// Per device, the ordinal of the URC line whose `URCsetup` created it
    /// (and its model), or `None` for a parsed device.
    generated_by: Vec<Option<usize>>,
    /// Device ordinals in C's `CKTsetup`/`CKTload` order
    /// ([`crate::devices::setup`]), computed by `finalize`.
    order: Vec<usize>,
    branch_rows: Vec<std::ops::Range<usize>>,
    /// Per device, the branch rows of its controlling sources, or why they
    /// could not be resolved (reported by `finalize` and every load).
    control_rows: Vec<Result<Vec<usize>, SpiceError>>,
    state_rows: Vec<std::ops::Range<usize>>,
    state_len: usize,
    /// Inductor pairs coupled by K devices, or why the K references could not
    /// be resolved (reported by `finalize` and every load).
    mutual: MutualBinding,
    /// Element positions, numbered in C's `TSTALLOC` order: allocated by
    /// `finalize` from [`Device::setup_matrix`] in setup order, extended when
    /// a load touches a position no device declared. See
    /// [`Circuit::new_matrix`].
    pattern: std::cell::RefCell<Option<std::sync::Arc<crate::maths::Pattern>>>,
    /// C's `CKTstate0` as the analyses run so far left it (C keeps one state
    /// vector for the whole deck); see [`Circuit::inherit_from`].
    carried_state: std::cell::RefCell<Option<Vec<Real>>>,
    /// The analyses already run on this circuit, in order (C keeps one
    /// circuit, and its devices' instance fields, for the whole deck); see
    /// [`Circuit::inherit_from`].
    completed: Vec<crate::primitives::AnalysisKind>,
    /// The last successful transient's final load, including device instance
    /// quantities stored in trial slots (MOS `capbd`/`capbs`). Kept separate
    /// from `carried_state`: a later DC load must not overwrite these fields.
    final_transient: std::cell::RefCell<Option<FinalTransientState>>,
    /// [`Circuit::unknown_names`], computed on first use after
    /// [`Circuit::rebuild_unknowns`].
    unknown_names: std::cell::RefCell<Option<crate::maths::UnknownNames>>,
    /// Devices' verified fixed stamp batches, lent to each bound load
    /// ([`crate::maths::SparseMatrix::lend_fixed`]).
    fixed_stamps: std::cell::RefCell<crate::maths::FixedStamps>,
}

/// A successful transient's final load and the layout that can interpret it.
#[derive(Clone)]
struct FinalTransientState {
    values: Vec<Real>,
    rows: Vec<std::ops::Range<usize>>,
    devices: Vec<(String, char, Vec<NodeId>)>,
}

impl FinalTransientState {
    fn matches(&self, circuit: &Circuit) -> bool {
        self.values.len() == circuit.state_len
            && self.rows == circuit.state_rows
            && self.devices.len() == circuit.devices.len()
            && self
                .devices
                .iter()
                .zip(&circuit.devices)
                .all(|((name, kind, terminals), device)| {
                    name == device.name()
                        && *kind == device.designator()
                        && terminals == device.terminals()
                })
    }
}

/// One inductor pair coupled by a K device, by device ordinal.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MutualPair {
    /// The K device.
    coupling: usize,
    first: usize,
    second: usize,
    coefficient: Real,
}

/// The resolved K couplings of a circuit.
#[derive(Debug, Clone, Default)]
struct MutualBinding {
    pairs: Vec<MutualPair>,
    error: Option<SpiceError>,
}

/// One device convergence test pass ([`Circuit::convergence_test`]).
#[derive(Debug, Clone, Copy)]
pub struct ConvergenceRequest<'a> {
    /// The new iterate (C `CKTrhs`).
    pub solution: &'a Vector,
    /// The iterate the last load used (C `CKTrhsOld`).
    pub previous: &'a Vector,
    /// The last load's trial (C `CKTstate0`).
    pub trial: &'a TrialState,
    /// Temperatures and `gmin` of the solve.
    pub model_context: &'a ModelContext,
    /// Transient time, zero otherwise.
    pub time: Real,
    /// C `CKTreltol`.
    pub reltol: Real,
    /// C `CKTabstol`.
    pub abstol: Real,
    /// C `CKTvoltTol`.
    pub vntol: Real,
}

/// The driver quantities [`Circuit::accept_transient_point`] and
/// [`Circuit::delay_timestep_limit`] pass to delay lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayAcceptance {
    /// C `CKTdeltaOld[0..3]` ([`crate::devices::delay::DeltaOld`]).
    pub steps: crate::devices::delay::DeltaOld,
    /// C `CKTminBreak` as delay lines see it: the smallest sample spacing
    /// recorded.
    pub min_break: Real,
    /// The `t = 0` point that initializes the delay histories.
    pub start: bool,
    /// The run's relative tolerance (C `CKTreltol`).
    pub reltol: Real,
    /// The run's absolute current tolerance (C `CKTabstol`).
    pub abstol: Real,
}

/// One trial load of every device (C `CKTload`).
#[derive(Debug, Clone, Copy)]
pub struct LoadRequest<'a> {
    /// Which analysis is loading.
    pub mode: AnalysisMode,
    /// The present solution the trial linearizes around.
    pub solution: &'a Vector,
    /// Circuit and nominal temperatures.
    pub model_context: &'a ModelContext,
    /// Companion integration coefficients, for companion transient loads.
    pub integration: Option<&'a Coefficients>,
    /// The accepted state history this trial reads.
    pub history: &'a StateHistory,
    /// Source-forcing context, required by companion transient loads (sources
    /// evaluate their waveform at the mode's time) and `None` otherwise.
    pub forcing: Option<Forcing>,
}

impl fmt::Debug for Circuit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Circuit")
            .field("nodes", &self.nodes.len())
            .field("unknowns", &self.unknowns.len())
            .field(
                "devices",
                &self
                    .devices
                    .iter()
                    .map(|device| device.name())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Circuit {
    /// An empty circuit holding only ground.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The node table.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        &self.nodes
    }

    /// The MNA unknown numbering. Only valid after [`Circuit::rebuild_unknowns`].
    #[must_use]
    pub fn unknowns(&self) -> &MnaUnknowns {
        &self.unknowns
    }

    /// The device instances, in insertion order.
    #[must_use]
    pub fn devices(&self) -> &[Box<dyn Device>] {
        &self.devices
    }

    /// The device instances, mutable, so an analysis can refresh them.
    pub fn devices_mut(&mut self) -> &mut [Box<dyn Device>] {
        // A replaced device may stamp other positions.
        self.fixed_stamps.get_mut().forget();
        &mut self.devices
    }

    /// Number of device instances.
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Size of the MNA system.
    #[must_use]
    pub fn unknown_count(&self) -> usize {
        self.unknowns.len()
    }

    /// Adds a node, or returns the existing one.
    ///
    /// Matrix rows change as nodes are added, so call
    /// [`Circuit::rebuild_unknowns`] once the circuit is complete.
    pub fn add_node(&mut self, name: &str) -> NodeId {
        self.nodes.intern(name)
    }

    /// Rebuilds the unknown numbering as `CKTsetup` creates C's nodes: one row
    /// per parsed (terminal) node, in node order, then the unknowns each
    /// device makes in its `DEVsetup`, visiting devices in setup order
    /// ([`crate::devices::setup`]): its internal nodes (`CKTmkVolt`) and branch
    /// currents (`CKTmkCur`). `vsrcset.c` makes the branch before an RF port's
    /// `res` node; the other setups make internal nodes first. State slots are
    /// numbered in device order, in a separate namespace.
    pub fn rebuild_unknowns(&mut self) {
        self.unknown_names.get_mut().take();
        self.fixed_stamps.get_mut().forget();
        let owned: BTreeSet<NodeId> = self.internal_nodes.iter().flatten().copied().collect();
        self.unknowns
            .rebuild_deferring(&self.nodes, |node| owned.contains(&node));
        use crate::devices::setup::CDeviceType;
        self.branch_rows = vec![0..0; self.devices.len()];
        // `CKTmkCur` runs once per device: in its own DEVsetup, or earlier
        // when a CCCS/CCVS/CSW/ASRC setup asks `CKTfndBranch` for it
        // (`VSRCfindBr`, `VCVSfindBr`, `CCVSfindBr` and `ASRCfindBr` make the
        // branch on demand), so a controlling source's branch is numbered
        // where its first user is set up.
        let mut made = vec![false; self.devices.len()];
        let make = |circuit: &mut Self, made: &mut [bool], index: usize| {
            if made[index] {
                return;
            }
            made[index] = true;
            let count = circuit.devices[index].branch_currents();
            let start = circuit.unknowns.len();
            circuit.unknowns.add_rows(count);
            circuit.branch_rows[index] = start..start + count;
        };
        for index in self.setup_order() {
            let kind = self.devices[index].c_device_type();
            // vsrcset.c, ccvsset.c and asrcset.c make their own branch before
            // anything else; the other setups make internal nodes first.
            let branches_first = matches!(kind, CDeviceType::Vsrc | CDeviceType::Ccvs)
                || (kind == CDeviceType::Asrc && self.devices[index].branch_currents() > 0);
            if branches_first {
                make(self, &mut made, index);
            }
            // cccsset.c, ccvsset.c, cswsetup.c and asrcset.c call
            // CKTfndBranch for each controlling source, in order.
            let found: Vec<usize> = self.devices[index]
                .controlling_sources()
                .iter()
                .filter_map(|control| {
                    self.devices.iter().position(|other| {
                        other.name().eq_ignore_ascii_case(&control.name)
                            && other.findable_branch().is_some()
                    })
                })
                .collect();
            for target in found {
                make(self, &mut made, target);
            }
            for &node in &self.internal_nodes[index] {
                self.unknowns.assign_deferred(node);
            }
            make(self, &mut made, index);
        }
        self.state_rows.clear();
        self.state_len = 0;
        for device in &self.devices {
            let states = device.state_count();
            self.state_rows
                .push(self.state_len..self.state_len + states);
            self.state_len += states;
        }
        self.control_rows = self
            .devices
            .iter()
            .map(|device| self.resolve_controls(device.as_ref()))
            .collect();
        self.mutual = match self.resolve_mutual() {
            Ok(pairs) => MutualBinding { pairs, error: None },
            Err(error) => MutualBinding {
                pairs: Vec::new(),
                error: Some(error),
            },
        };
    }

    /// The inductor pairs of every K device (C `MUTsetup`, which looks the
    /// inductors up with `CKTfndDev`; a card with more than two inductors
    /// couples every pair, as `inp_compat()` expands it). Names are matched
    /// case-insensitively after subcircuit renaming.
    fn resolve_mutual(&self) -> Result<Vec<MutualPair>, SpiceError> {
        let mut pairs = Vec::new();
        for (coupling, device) in self.devices.iter().enumerate() {
            let Some(description) = device.mutual_coupling() else {
                continue;
            };
            let mut inductors = Vec::with_capacity(description.inductors.len());
            for reference in description.inductors {
                let failure = |message: String| match &reference.location {
                    Some(location) => SpiceError::parse(location.clone(), message),
                    None => SpiceError::circuit(message),
                };
                let Some(index) = self
                    .devices
                    .iter()
                    .position(|other| other.name().eq_ignore_ascii_case(&reference.name))
                else {
                    return Err(failure(format!(
                        "{}: coupling to non-existent inductor {}",
                        device.name(),
                        reference.name
                    )));
                };
                let target = &self.devices[index];
                if target.inductance(&ModelContext::default()).is_none()
                    || self.branch_rows[index].len() != 1
                {
                    return Err(failure(format!(
                        "{}: {} is not an inductor (a K card couples inductors only)",
                        device.name(),
                        target.name()
                    )));
                }
                if inductors.contains(&index) {
                    return Err(failure(format!(
                        "{}: couples inductor {} to itself",
                        device.name(),
                        target.name()
                    )));
                }
                inductors.push(index);
            }
            for (position, first) in inductors.iter().enumerate() {
                for second in &inductors[position + 1..] {
                    pairs.push(MutualPair {
                        coupling,
                        first: *first,
                        second: *second,
                        coefficient: description.coefficient,
                    });
                }
            }
        }
        Ok(pairs)
    }

    /// The mutual-inductance terms of every device's branch equation under
    /// `context`, by device ordinal (empty when the circuit has no K device).
    ///
    /// `M = k sqrt(|L1 L2|)` from the inductors' coupling inductances (C
    /// `MUTtemp`). Every inductive system (a connected group of coupled
    /// inductors, found with petgraph) is checked: its inductance matrix must
    /// be positive semidefinite. C only warns ("is not positive definite",
    /// `muttemp.c`) and then simulates a system that stores negative energy;
    /// the port rejects it. See [`crate::devices::mutual`].
    ///
    /// # Errors
    /// Stale numbering, unresolved K references, invalid inductances or a
    /// system that is not positive semidefinite.
    pub fn mutual_terms(&self, context: &ModelContext) -> SpiceResult<Vec<Vec<MutualTerm>>> {
        self.check_numbering()?;
        if let Some(error) = &self.mutual.error {
            return Err(error.clone());
        }
        if self.mutual.pairs.is_empty() {
            return Ok(Vec::new());
        }
        let replacements = self.replacements(context)?;
        let device_at = |index: usize| -> &dyn Device {
            replacements
                .iter()
                .find(|(i, _)| *i == index)
                .map_or(self.devices[index].as_ref(), |(_, d)| d.as_ref())
        };
        let mut values: std::collections::BTreeMap<usize, InductanceValue> =
            std::collections::BTreeMap::new();
        for pair in &self.mutual.pairs {
            for index in [pair.first, pair.second] {
                if values.contains_key(&index) {
                    continue;
                }
                let device = device_at(index);
                let value = device.inductance(context).ok_or_else(|| {
                    SpiceError::circuit(format!("{} is not an inductor", device.name()))
                })??;
                values.insert(index, value);
            }
        }
        let mut terms = vec![Vec::new(); self.devices.len()];
        let mut mutuals = Vec::with_capacity(self.mutual.pairs.len());
        // INDload adds each K's flux in the MUT instance list's order, the
        // last card first (instances are prepended).
        for pair in self.mutual.pairs.iter().rev() {
            let (a, b) = (values[&pair.first], values[&pair.second]);
            let coefficient = device_at(pair.coupling)
                .mutual_coupling()
                .ok_or_else(|| SpiceError::circuit("stale coupling override"))?
                .coefficient;
            let inductance = coefficient * (a.coupling_base * b.coupling_base).abs().sqrt();
            if !inductance.is_finite() {
                return Err(SpiceError::circuit(format!(
                    "{}: nonfinite mutual inductance",
                    self.devices[pair.coupling].name()
                )));
            }
            terms[pair.first].push(MutualTerm {
                row: self.branch_rows[pair.second].start,
                inductance,
                initial: b.initial,
            });
            terms[pair.second].push(MutualTerm {
                row: self.branch_rows[pair.first].start,
                inductance,
                initial: a.initial,
            });
            mutuals.push(inductance);
        }
        // check_inductive_systems pairs these with self.mutual.pairs in order.
        mutuals.reverse();
        self.check_inductive_systems(&values, &mutuals)?;
        Ok(terms)
    }

    /// `MUTtemp`'s inductive-system check, as a rejection: every connected
    /// group of coupled inductors must have a positive semidefinite
    /// inductance matrix. The test runs on the matrix actually stamped
    /// (self-inductances `INDinduct / m`, mutuals from `INDinduct`) normalized
    /// to a unit diagonal (`M_ij / sqrt(L_i L_j)`, a congruence that preserves
    /// definiteness), whose eigenvalues are compared with a rounding
    /// tolerance of `64 n eps` times the largest magnitude. C's own check puts
    /// `INDinduct` on the diagonal; when only the stamped matrix fails (an
    /// `m != 1`), the message says that C stays silent.
    fn check_inductive_systems(
        &self,
        values: &std::collections::BTreeMap<usize, InductanceValue>,
        mutuals: &[Real],
    ) -> SpiceResult<()> {
        let inductors: Vec<usize> = values.keys().copied().collect();
        let position = |index: usize| inductors.binary_search(&index).unwrap_or(0);
        let mut systems = petgraph::unionfind::UnionFind::<usize>::new(inductors.len());
        for pair in &self.mutual.pairs {
            systems.union(position(pair.first), position(pair.second));
        }
        let labels = systems.into_labeling();
        let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (slot, label) in labels.iter().enumerate() {
            groups.entry(*label).or_default().push(slot);
        }
        for members in groups.values() {
            let n = members.len();
            let local = |slot: usize| members.iter().position(|member| *member == slot);
            // The smallest eigenvalue of the group's matrix normalized by
            // `diagonal` (`M_ij / sqrt(d_i d_j)` off the unit diagonal), and
            // its rounding tolerance.
            let smallest = |diagonal: &dyn Fn(&InductanceValue) -> Real| -> SpiceResult<_> {
                let mut matrix = crate::maths::Matrix::zeros(n, n);
                for row in 0..n {
                    matrix.set(row, row, 1.0)?;
                }
                let mut couplings = Vec::new();
                for (pair, mutual) in self.mutual.pairs.iter().zip(mutuals) {
                    let (Some(i), Some(j)) =
                        (local(position(pair.first)), local(position(pair.second)))
                    else {
                        continue;
                    };
                    let scale =
                        (diagonal(&values[&pair.first]) * diagonal(&values[&pair.second])).sqrt();
                    // A zero inductance (accepted, as in C) couples with
                    // M = k sqrt(L1 L2) = 0: its row stays a unit diagonal,
                    // which keeps the rest's definiteness.
                    let normalized = if scale == 0.0 { 0.0 } else { mutual / scale };
                    matrix.add_to(i, j, normalized)?;
                    matrix.add_to(j, i, normalized)?;
                    couplings.push(pair.coupling);
                }
                let eigenvalues = matrix.symmetric_eigenvalues()?;
                let largest = eigenvalues.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                #[allow(clippy::cast_precision_loss)]
                let tolerance = 64.0 * n as Real * Real::EPSILON * largest;
                let smallest = eigenvalues.first().copied().unwrap_or(0.0);
                Ok((smallest, tolerance, couplings))
            };
            // The matrix actually simulated: self-inductances as stamped
            // (`INDinduct / m`), mutuals from `INDinduct` (as C).
            let (stamped, tolerance, mut couplings) = smallest(&|value| value.effective)?;
            if stamped < -tolerance {
                // C's own check (`muttemp.c`) puts `INDinduct` (before `/m`)
                // on the diagonal; it warns only when that matrix fails too.
                let (c_matrix, c_tolerance, _) = smallest(&|value| value.coupling_base.abs())?;
                couplings.sort_unstable();
                couplings.dedup();
                let names = |indices: &mut dyn Iterator<Item = usize>| {
                    indices
                        .map(|index| self.devices[index].name().to_owned())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                let inductor_names = names(&mut members.iter().map(|slot| inductors[*slot]));
                let coupling_names = names(&mut couplings.iter().copied());
                let location = couplings.first().and_then(|index| {
                    self.devices[*index]
                        .mutual_coupling()
                        .and_then(|coupling| coupling.location.cloned())
                });
                let cause = if c_matrix < -c_tolerance {
                    "a coupling with |k| > 1 or an inconsistent set of couplings stores \
                     negative energy; C only warns, see muttemp.c"
                } else {
                    "an inductor multiplicity m divides the self-inductance but not \
                     M = k sqrt(L1 L2), which C computes from INDinduct before /m; C checks \
                     that undivided matrix in muttemp.c, so it neither warns nor rejects \
                     here, and simulates the indefinite system"
                };
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "the inductive system {inductor_names} coupled by {coupling_names} is not \
                         positive semidefinite as stamped (smallest normalized eigenvalue \
                         {stamped:.6e}; {cause})"
                    ),
                    location,
                });
            }
        }
        Ok(())
    }

    /// Branch rows of the devices `device` senses (C `CKTfndBranch`, called
    /// from `CCCSsetup`/`CCVSsetup`): looked up by instance name after every
    /// device is known, so a controlling source may follow its user in the
    /// deck. Only devices with a [`Device::findable_branch`] qualify: C finds
    /// V, E, H (and B) branches, but not inductor currents.
    fn resolve_controls(&self, device: &dyn Device) -> Result<Vec<usize>, SpiceError> {
        device
            .controlling_sources()
            .iter()
            .map(|control| {
                let failure = |message: String| match &control.location {
                    Some(location) => SpiceError::parse(location.clone(), message),
                    None => SpiceError::circuit(message),
                };
                let Some(index) = self
                    .devices
                    .iter()
                    .position(|other| other.name().eq_ignore_ascii_case(&control.name))
                else {
                    return Err(failure(format!(
                        "{}: unknown controlling source {}",
                        device.name(),
                        control.name
                    )));
                };
                let target = &self.devices[index];
                let Some(branch) = target.findable_branch() else {
                    return Err(failure(format!(
                        "{}: controlling source {} has no findable branch current (only \
                         independent voltage sources and E/H sources can be sensed, as by \
                         CKTfndBranch)",
                        device.name(),
                        target.name()
                    )));
                };
                self.branch_rows[index]
                    .clone()
                    .nth(branch)
                    .ok_or_else(|| failure(format!("{}: missing branch row", target.name())))
            })
            .collect()
    }

    /// The resolved controlling-branch rows of device `index`.
    fn controls(&self, index: usize) -> SpiceResult<&[usize]> {
        match self.control_rows.get(index) {
            Some(Ok(rows)) => Ok(rows),
            Some(Err(error)) => Err(error.clone()),
            None => Err(SpiceError::circuit(
                "circuit numbering is stale; finalize after adding devices",
            )),
        }
    }

    /// Controlling-branch rows by device ordinal, after finalization (empty for
    /// devices that sense no branch current).
    ///
    /// # Errors
    /// Stale numbering, or the device's controlling source is unknown or has no
    /// findable branch current.
    pub fn control_rows(&self, index: usize) -> SpiceResult<Vec<usize>> {
        self.controls(index).map(<[usize]>::to_vec)
    }

    /// Validates the graph and renumbers the unknowns.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] if a device refers to a node that is not in the
    /// table, or if two devices share an instance name — ngspice rejects the
    /// latter too, in `INP2dot`/`CKTcrtElt`. A controlling source (F/H) that
    /// names no device, or a device without a findable branch current, is a
    /// [`SpiceError::Parse`] at the reference (C: "unknown controlling source").
    pub fn finalize(&mut self) -> SpiceResult<()> {
        let mut previous = crate::devices::Waveform::Constant(0.);
        let mut previous_dc = 0.;
        for device in self.devices.iter_mut().rev() {
            if let Some((waveform, dc)) = device.bind_voltage_predecessor(&previous, previous_dc)? {
                previous = waveform;
                previous_dc = dc;
            }
        }
        self.topology()?;
        self.rebuild_unknowns();
        for index in 0..self.devices.len() {
            self.controls(index)?;
        }
        if let Some(error) = &self.mutual.error {
            return Err(error.clone());
        }
        self.check_port_numbering()?;
        self.allocate_matrix()
    }

    /// Device ordinals in C's `CKTsetup`/`CKTload` order.
    fn setup_order(&self) -> Vec<usize> {
        let keys: Vec<crate::devices::setup::SetupKey> = self
            .devices
            .iter()
            .enumerate()
            .map(|(index, device)| crate::devices::setup::SetupKey {
                device_type: device.c_device_type(),
                model: self.models.get(index).cloned().flatten(),
                generated_by: self.generated_by.get(index).copied().flatten(),
            })
            .collect();
        crate::devices::setup::setup_order(&keys)
    }

    /// The matrix elements device `index` allocates in its `DEVsetup`
    /// (`TSTALLOC` order; with `pole_zero` its `DEVpzSetup`), appended to
    /// `elements`.
    fn declare_elements(
        &self,
        index: usize,
        pole_zero_setups: u32,
        elements: &mut Vec<(usize, usize)>,
    ) -> SpiceResult<bool> {
        let coupled: Vec<(usize, usize)> = self
            .mutual
            .pairs
            .iter()
            .rev()
            .filter(|pair| pair.coupling == index)
            .map(|pair| {
                (
                    self.branch_rows[pair.first].start,
                    self.branch_rows[pair.second].start,
                )
            })
            .collect();
        let mut setup = crate::devices::setup::MatrixSetup {
            unknowns: &self.unknowns,
            branches: self.branch_rows[index].clone(),
            controls: self.controls(index)?,
            coupled: &coupled,
            pole_zero: pole_zero_setups > 0,
            pole_zero_setups,
            elements,
        };
        self.devices[index].setup_matrix(&mut setup)
    }

    /// The `(row, col)` matrix elements device `index` allocates in its
    /// `DEVsetup`, in `TSTALLOC` order (`.sens` replays them into its
    /// perturbation matrix, `cktsens.c`).
    ///
    /// # Errors
    /// Stale numbering or an ordinal out of range.
    pub fn setup_elements(&self, index: usize) -> SpiceResult<Vec<(usize, usize)>> {
        self.check_numbering()?;
        if index >= self.devices.len() {
            return Err(SpiceError::circuit("device ordinal out of range"));
        }
        let mut elements = Vec::new();
        self.declare_elements(index, 0, &mut elements)?;
        Ok(elements)
    }

    /// C's `CKTsetup`: orders the devices and allocates the matrix elements
    /// they declare, in that order. A pattern that already starts with the
    /// same elements is kept, so its pivot order survives re-finalization
    /// (C sets a circuit up once).
    fn allocate_matrix(&mut self) -> SpiceResult<()> {
        let order = self.setup_order();
        let elements = self.ordered_elements(&order, 0)?;
        self.order = order;
        let n = self.unknown_count();
        let mut slot = self.pattern.borrow_mut();
        if elements.is_empty() || n == 0 {
            return Ok(());
        }
        let allocated = crate::maths::Pattern::new(n, elements);
        let same = slot.as_ref().is_some_and(|current| {
            current.order() == n
                && current.len() >= allocated.len()
                && (0..allocated.len()).all(|index| {
                    let id = crate::maths::sparse::element_id(index);
                    current.position(id) == allocated.position(id)
                })
        });
        if !same {
            *slot = Some(std::sync::Arc::new(allocated));
        }
        Ok(())
    }

    /// The matrix elements the devices declare ([`Device::setup_matrix`]),
    /// visited in `order`: C's `CKTsetup`, or with `pole_zero` the
    /// `DEVpzSetup` pass of `CKTpzSetup` (`cktpzset.c`) on a fresh matrix.
    fn ordered_elements(
        &self,
        order: &[usize],
        pole_zero_setups: u32,
    ) -> SpiceResult<Vec<(usize, usize)>> {
        let mut elements = Vec::new();
        for &index in order {
            self.declare_elements(index, pole_zero_setups, &mut elements)?;
        }
        Ok(elements)
    }

    /// The elements `CKTpzSetup` (`cktpzset.c`) allocates on its fresh
    /// matrix before the drive, in allocation order: every device's
    /// `DEVpzSetup` (its `DEVsetup`, except that `VSRCpzSetup` adds the
    /// branch diagonal), then any element the circuit pattern learned from
    /// a load of a device that declares none. `setups` counts the
    /// `CKTpzSetup` runs, this one included (see
    /// [`crate::devices::setup::MatrixSetup::pole_zero_setups`]).
    ///
    /// The internal nodes of a device that declares its `DEVpzSetup`
    /// elements are its own: only its declaration places them, so a learned
    /// position on one of them (from the circuit's `DEVsetup` pattern) is not
    /// added. A `DEVpzSetup` that drops an internal node (`b4set.c` re-run)
    /// therefore leaves no row for it, as SPARSE creates rows only for
    /// referenced nodes.
    /// # Errors
    /// Mismatched numbering or missing bindings.
    pub fn pole_zero_elements(&self, setups: u32) -> SpiceResult<Vec<(usize, usize)>> {
        self.check_numbering()?;
        let setups = setups.max(1);
        let mut elements = Vec::new();
        let mut private = BTreeSet::new();
        for index in self.setup_order() {
            if self.declare_elements(index, setups, &mut elements)? {
                private.extend(
                    self.internal_nodes[index]
                        .iter()
                        .filter_map(|&node| self.unknowns.node_row(node)),
                );
            }
        }
        if let Some(pattern) = self.matrix_pattern() {
            let known: BTreeSet<(usize, usize)> = elements.iter().copied().collect();
            for index in 0..pattern.len() {
                let position = pattern.position(crate::maths::sparse::element_id(index));
                if !known.contains(&position)
                    && !private.contains(&position.0)
                    && !private.contains(&position.1)
                {
                    elements.push(position);
                }
            }
        }
        Ok(elements)
    }

    /// C `CKTconvTest`: every device's convergence test, in `CKTload` order,
    /// after a Newton pass whose iterate converged; `true` at the first
    /// device that has not converged.
    ///
    /// # Errors
    /// Mismatched numbering or a device evaluation failure.
    pub fn convergence_test(&self, request: &ConvergenceRequest<'_>) -> SpiceResult<bool> {
        self.check_numbering()?;
        let n = self.unknown_count();
        if request.solution.len() != n
            || request.previous.len() != n
            || request.trial.values().len() != self.state_len
        {
            return Err(SpiceError::circuit(
                "convergence test does not match the circuit numbering",
            ));
        }
        for index in self.load_order() {
            let context = crate::devices::traits::ConvergenceContext {
                unknowns: &self.unknowns,
                solution: request.solution,
                previous: request.previous,
                controls: self.controls(index)?,
                states: &request.trial.values()[self.state_rows[index].clone()],
                phase: request.trial.phase(),
                model_context: request.model_context,
                time: request.time,
                reltol: request.reltol,
                abstol: request.abstol,
                vntol: request.vntol,
            };
            if self.devices[index].convergence_test(&context)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Device ordinals in `CKTload` order (card order before `finalize`).
    pub(crate) fn load_order(&self) -> Vec<usize> {
        if self.order.len() == self.devices.len() {
            self.order.clone()
        } else {
            (0..self.devices.len()).collect()
        }
    }

    /// `vsrctemp.c` (`VSRCtemp`, run before every analysis): the RF port
    /// numbers must be exactly `1..=N` for `N` ports. A number above `N` is
    /// "incorrect port ordering" (a gap or an out-of-range index), a repeated
    /// one a "duplicate port Index"; both are fatal in C.
    fn check_port_numbering(&self) -> SpiceResult<()> {
        let ports: Vec<_> = self
            .devices
            .iter()
            .filter_map(|device| device.rf_port().map(|port| (device.name(), port.number)))
            .collect();
        let mut seen = BTreeSet::new();
        for (name, number) in &ports {
            if *number > ports.len() {
                return Err(SpiceError::circuit(format!(
                    "{name}: incorrect port ordering (portnum {number} with {} port(s); \
                     ports must be numbered 1..=N)",
                    ports.len()
                )));
            }
            if !seen.insert(*number) {
                return Err(SpiceError::circuit(format!(
                    "{name}: duplicate port Index {number}"
                )));
            }
        }
        Ok(())
    }

    /// Builds a validated petgraph incidence snapshot of the current circuit.
    ///
    /// Includes every node (even unused ones), every device, and one edge for
    /// each terminal in `Device::terminals()` order. This follows the binding in
    /// `src/spicelib/devices/cktbindnode.c::CKTbindNode`; C numbers ports from 1,
    /// while edge weights here are zero-based. Device order remains deck order.
    /// Use petgraph's connectivity/traversal algorithms on the returned graph.
    ///
    /// The snapshot is rebuilt on demand: `devices_mut()` can change topology,
    /// so a cached graph could silently become stale. A structural path through
    /// a capacitor or multiport device need not be a DC conductive path. This
    /// method never rejects a circuit for disconnectedness or claims a matrix
    /// is nonsingular; analysis-specific checks belong to the eventual engine.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] for dangling terminals or duplicate names, as in
    /// [`Circuit::finalize`]. No circuit state is changed on failure.
    pub fn topology(&self) -> SpiceResult<CircuitGraph> {
        let mut graph = CircuitGraph::default();
        let node_vertices: Vec<_> = self
            .nodes
            .nodes()
            .iter()
            .map(|node| graph.add_node(CircuitVertex::Node(node.id)))
            .collect();
        let mut seen = BTreeSet::new();
        for (index, device) in self.devices.iter().enumerate() {
            let vertex = graph.add_node(CircuitVertex::Device(index));
            for (port, terminal) in device.terminals().iter().enumerate() {
                let Some(node_vertex) = node_vertices.get(terminal.index()) else {
                    return Err(SpiceError::circuit(format!(
                        "device {} refers to {terminal}, which is not in the node table",
                        device.name()
                    )));
                };
                graph.add_edge(vertex, *node_vertex, port);
            }
            if !seen.insert(device.name().to_lowercase()) {
                return Err(SpiceError::circuit(format!(
                    "duplicate instance name '{}'",
                    device.name()
                )));
            }
        }
        Ok(graph)
    }

    /// Adds a device, checking that its terminals are already in the node table
    /// and that its name is not taken.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] on a dangling terminal or a duplicate name.
    pub fn add_device(&mut self, device: Box<dyn Device>) -> SpiceResult<()> {
        self.fixed_stamps.get_mut().forget();
        for terminal in device.terminals() {
            if self.nodes.node(*terminal).is_none() {
                return Err(SpiceError::circuit(format!(
                    "device {} refers to {terminal}, which is not in the node table",
                    device.name()
                )));
            }
        }
        if self.device(device.name()).is_some() {
            return Err(SpiceError::circuit(format!(
                "duplicate instance name '{}'",
                device.name()
            )));
        }
        let internal = device
            .terminals()
            .iter()
            .copied()
            .filter(|&node| {
                self.nodes
                    .node(node)
                    .is_some_and(|node| node.kind == NodeKind::Internal)
            })
            .collect();
        self.devices.push(device);
        self.models.push(None);
        self.internal_nodes.push(internal);
        self.generated_by.push(None);
        Ok(())
    }

    /// Branch rows by device ordinal, after finalization.
    pub fn branch_rows(&self, index: usize) -> Option<std::ops::Range<usize>> {
        self.branch_rows.get(index).cloned()
    }

    /// The name of every unknown by matrix row, as ngspice names them
    /// (`CKTnodName`): the node name, or `<device>#branch` for a branch
    /// current (`CKTmkCur`; a device's further branches add their ordinal).
    /// Solver diagnostics use them (`singular matrix:  check node X`).
    pub fn unknown_names(&self) -> crate::maths::UnknownNames {
        let n = self.unknown_count();
        if let Some(names) = &*self.unknown_names.borrow()
            && names.len() == n
        {
            return std::sync::Arc::clone(names);
        }
        let mut names: Vec<String> = (0..n).map(|row| format!("row {row}")).collect();
        for node in self.nodes.nodes() {
            if let Some(row) = self.unknowns.node_row(node.id)
                && let Some(name) = names.get_mut(row)
            {
                name.clone_from(&node.name);
            }
        }
        for (device, range) in self.devices.iter().zip(&self.branch_rows) {
            for (k, row) in range.clone().enumerate() {
                if let Some(name) = names.get_mut(row) {
                    *name = if k == 0 {
                        format!("{}#branch", device.name())
                    } else {
                        format!("{}#branch{}", device.name(), k + 1)
                    };
                }
            }
        }
        let names: crate::maths::UnknownNames = names.into();
        *self.unknown_names.borrow_mut() = Some(std::sync::Arc::clone(&names));
        names
    }

    /// State slots by device ordinal, after finalization. Slots are not
    /// matrix rows.
    pub fn state_rows(&self, index: usize) -> Option<std::ops::Range<usize>> {
        self.state_rows.get(index).cloned()
    }

    /// Total number of state slots, after finalization.
    #[must_use]
    pub const fn state_len(&self) -> usize {
        self.state_len
    }

    /// An empty accepted-state history sized for this circuit, with an empty
    /// delay history for every device that has a
    /// [`crate::devices::delay::DelayLine`].
    #[must_use]
    pub fn state_history(&self) -> StateHistory {
        StateHistory::new(self.state_len)
            .with_carried(self.carried_state.borrow().clone())
            .with_delay_lines(
                self.devices
                    .iter()
                    .enumerate()
                    .filter_map(|(index, device)| {
                        device.delay_line().map(|line| (index, line.width()))
                    }),
            )
    }

    /// Whether any device has a delay history (a transmission line).
    #[must_use]
    pub fn has_delay_lines(&self) -> bool {
        self.devices
            .iter()
            .any(|device| device.delay_line().is_some())
    }

    /// C's `MODEINITSMSIG` load at the operating point `bias` (`acan.c`,
    /// `distoan.c`, `noisean.c`, `pzan.c`) as far as it outlives the
    /// analysis: the state slots devices write there and later analyses read
    /// ([`Device::small_signal_state`]), recorded into the state vector this
    /// circuit carries forward.
    ///
    /// # Errors
    /// Stale numbering, a bias of the wrong size, or device physics errors.
    pub fn record_small_signal_state(
        &self,
        context: &ModelContext,
        bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        self.check_numbering()?;
        if bias.len() != self.unknown_count() {
            return Err(SpiceError::circuit("small-signal state bias dimensions"));
        }
        let mut state = self
            .carried_state
            .borrow()
            .clone()
            .filter(|vector| vector.len() == self.state_len)
            .unwrap_or_else(|| vec![0.; self.state_len]);
        let operating = crate::devices::distortion::DistortionContext {
            model_context: context,
            unknowns: &self.unknowns,
            bias,
        };
        for (index, device) in self.devices.iter().enumerate() {
            let rows = self.state_rows[index].clone();
            if let Some(slots) = state.get_mut(rows) {
                device.small_signal_state(&operating, slots)?;
            }
        }
        *self.carried_state.borrow_mut() = Some(state);
        Ok(())
    }

    /// Keeps the final accepted transient load's device state for later
    /// analyses (`dctran.c`: instance fields outlive the transient). Called
    /// only after the whole transient succeeds, never on a rejected trial.
    ///
    /// # Errors
    /// Stale numbering, a mismatched history, or no accepted point.
    pub(crate) fn record_final_transient_state(&self, history: &StateHistory) -> SpiceResult<()> {
        self.check_numbering()?;
        if history.len() != self.state_len {
            return Err(SpiceError::circuit("final transient state dimensions"));
        }
        let state = history.accepted(1).ok_or_else(|| {
            SpiceError::circuit("a completed transient has no accepted device state")
        })?;
        *self.final_transient.borrow_mut() = Some(FinalTransientState {
            values: state.to_vec(),
            rows: self.state_rows.clone(),
            devices: self
                .devices
                .iter()
                .map(|device| {
                    (
                        device.name().to_owned(),
                        device.designator(),
                        device.terminals().to_vec(),
                    )
                })
                .collect(),
        });
        Ok(())
    }

    /// A snapshot of the last successful transient's final load state, in
    /// [`Self::state_rows`] order. Unlike the final solution, this retains
    /// the actual load's limited voltages and derived instance quantities.
    /// [`Self::inherit_from`] carries it to the next analysis. A changed
    /// device/state layout invalidates it; re-finalizing the same circuit
    /// preserves it. It is not an integration history.
    #[must_use]
    pub fn final_transient_state(&self) -> Option<Vec<Real>> {
        self.final_transient
            .borrow()
            .as_ref()
            .filter(|state| state.matches(self))
            .map(|state| state.values.clone())
    }

    fn check_numbering(&self) -> SpiceResult<()> {
        if self.branch_rows.len() != self.devices.len()
            || self.state_rows.len() != self.devices.len()
            || self.control_rows.len() != self.devices.len()
        {
            return Err(SpiceError::circuit(
                "circuit numbering is stale; finalize after adding devices",
            ));
        }
        Ok(())
    }

    /// The resistor named `name` (case-insensitive) and its device ordinal,
    /// identified by [`Device::resistor_metadata`], never by the name's first
    /// letter.
    #[must_use]
    pub fn resistor(&self, name: &str) -> Option<(usize, ResistorMetadata)> {
        self.devices.iter().enumerate().find_map(|(index, device)| {
            if !device.name().eq_ignore_ascii_case(name) {
                return None;
            }
            device.resistor_metadata().map(|metadata| (index, metadata))
        })
    }

    /// An immutable per-point override of resistor `name`'s supplied scalar.
    /// Carry it in a [`ModelContext`] ([`ModelContext::with_resistor_override`]);
    /// no device changes. See [`crate::devices::sweep`] for supplied-versus-effective
    /// semantics.
    ///
    /// # Errors
    /// [`SpiceError::Circuit`] if `name` is not a resistor, or `supplied` is
    /// nonfinite, zero, or has a nonfinite conductance.
    pub fn resistor_override(&self, name: &str, supplied: Real) -> SpiceResult<ResistorOverride> {
        let (index, _) = self
            .resistor(name)
            .ok_or_else(|| SpiceError::circuit(format!("{name} is not a resistor")))?;
        if !supplied.is_finite() || supplied == 0.0 || !(1.0 / supplied).is_finite() {
            return Err(SpiceError::circuit(format!(
                "resistor {name}: supplied resistance must be finite and nonzero with finite conductance"
            )));
        }
        Ok(ResistorOverride::new(index, supplied))
    }

    /// The effective resistance (ohms) `target` stamps under `context`'s
    /// temperatures: the supplied scalar with the device's own temperature, TC,
    /// scale and multiplicity laws applied. Nothing is mutated.
    ///
    /// # Errors
    /// A stale override, an invalid context, or an invalid derived value.
    pub fn effective_resistance(
        &self,
        target: &ResistorOverride,
        context: &ModelContext,
    ) -> SpiceResult<Real> {
        let device = self
            .devices
            .get(target.device())
            .ok_or_else(|| SpiceError::circuit("stale resistor override"))?;
        context.validate(&crate::primitives::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        device.resistor_effective(target.supplied(), context)
    }

    /// The device named `name` (case-insensitive) and its ordinal, with the
    /// canonical keyword of its swept instance parameter `keyword`
    /// ([`Device::instance_parameter`]), the target of `.dc @name[keyword]`.
    ///
    /// # Errors
    /// [`SpiceError::Unsupported`] when no device is named `name` or C has no
    /// settable real instance parameter `keyword` on it (C `dctrcurv.c`
    /// rejects both); [`SpiceError::NotYetPorted`] when C sweeps it but this
    /// port does not yet.
    pub fn instance_parameter(
        &self,
        name: &str,
        keyword: &str,
    ) -> SpiceResult<(usize, &'static str)> {
        let Some((index, device)) = self
            .devices
            .iter()
            .enumerate()
            .find(|(_, device)| device.name().eq_ignore_ascii_case(name))
        else {
            return Err(SpiceError::Unsupported {
                feature: format!("DC sweep parameter target: no instance named {name}"),
                location: None,
            });
        };
        if let Some(parameter) = device.instance_parameter(keyword) {
            return Ok((index, parameter));
        }
        if crate::devices::sweep::c_instance_parameter_known(device.designator(), keyword) {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "DC sweep of instance parameter {keyword} of {}",
                    device.name()
                ),
                "src/spicelib/analysis/dctrcurv.c (DCTfindInstParam, DCTsetInstParam)",
            ));
        }
        Err(SpiceError::Unsupported {
            feature: format!(
                "DC sweep parameter target: {} has no settable real instance parameter {keyword}",
                device.name()
            ),
            location: None,
        })
    }

    /// An immutable per-point override giving instance `name`'s parameter
    /// `keyword` the value `value` (`.dc @name[keyword]`). Carry it in a
    /// [`ModelContext`] ([`ModelContext::with_instance_override`]); no device
    /// changes. The value is validated by building the replacement once at
    /// `context`.
    ///
    /// # Errors
    /// As [`Self::instance_parameter`], or an invalid value.
    pub fn instance_override(
        &self,
        name: &str,
        keyword: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<InstanceOverride> {
        let (index, parameter) = self.instance_parameter(name, keyword)?;
        self.devices[index].with_instance_parameter(parameter, value, context)?;
        Ok(InstanceOverride::new(index, parameter, value))
    }

    /// A DC value override of the independent V or I source `name`, as
    /// `dctrcurv.c` applies it to the swept source (`dcValue`, `dcGiven`).
    ///
    /// # Errors
    /// An unknown name, a device that is not an independent source, or a
    /// nonfinite value.
    pub fn source_override(
        &self,
        name: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<InstanceOverride> {
        let index = self
            .devices
            .iter()
            .position(|d| d.name().eq_ignore_ascii_case(name))
            .filter(|&i| matches!(self.devices[i].designator().to_ascii_lowercase(), 'v' | 'i'))
            .filter(|_| value.is_finite())
            .ok_or_else(|| SpiceError::circuit("invalid or unknown DC source override"))?;
        self.devices[index].with_instance_parameter("dc", value, context)?;
        Ok(InstanceOverride::new(index, "dc", value))
    }

    /// The Newton load of the single device at ordinal `index`, with `device`
    /// (the circuit's own or a stand-in with the same terminals, branch rows
    /// and state layout) in its place: C's `DEVload` of one isolated instance,
    /// as `.sens` performs it (`cktsens.c`). Contributions are added to
    /// `matrix` and `rhs`; mutual-inductance terms are those of the circuit's
    /// own devices.
    ///
    /// # Errors
    /// Stale numbering, an ordinal out of range, mismatched dimensions or a
    /// device failure.
    pub fn load_device(
        &self,
        index: usize,
        device: &dyn Device,
        request: &LoadRequest<'_>,
        matrix: &mut SparseMatrix,
        rhs: &mut Vector,
        trial: &mut TrialState,
    ) -> SpiceResult<()> {
        self.check_numbering()?;
        let n = self.unknown_count();
        if index >= self.devices.len()
            || matrix.rows() != n
            || matrix.cols() != n
            || rhs.len() != n
            || request.solution.len() != n
            || request.history.len() != self.state_len
            || trial.values().len() != self.state_len
        {
            return Err(SpiceError::circuit(
                "single-device load does not match the circuit numbering",
            ));
        }
        let mutual = self.mutual_terms(request.model_context)?;
        let states = request
            .history
            .device_of(trial, self.state_rows[index].clone(), index)?;
        device.stamp(&mut StampContext {
            matrix,
            rhs,
            unknowns: &self.unknowns,
            nodes: &self.nodes,
            solution: request.solution,
            temperature: request.model_context.temperature,
            nominal_temperature: request.model_context.nominal_temperature,
            gmin: request.model_context.gmin,
            frequency: request.model_context.frequency,
            mode: request.mode,
            branches: self.branch_rows[index].clone(),
            controls: self.controls(index)?,
            mutual: mutual.get(index).map_or(&[], Vec::as_slice),
            integration: request.integration,
            states,
            forcing: request.forcing,
            source_factor: request.model_context.source_factor,
            // An isolated load has no preceding voltage source.
            voltage_source_value: &mut 0.,
        })
    }

    /// The small-signal assembly of the single device at ordinal `index`, with
    /// `device` in its place, added to `system` (C's `DEVacLoad` of one
    /// isolated instance, as `.sens` performs it). An inductor stamps no
    /// mutual-inductance terms: `.sens` loads each K's `MUTacLoad` as a device
    /// of its own. `bias` and `state` are as for
    /// [`Self::small_signal_system_at`].
    ///
    /// # Errors
    /// Stale numbering, an ordinal out of range, mismatched dimensions or
    /// unsupported device physics.
    pub fn assemble_small_signal_device(
        &self,
        index: usize,
        device: &dyn Device,
        context: &ModelContext,
        bias: &Vector,
        state: Option<&[Real]>,
        system: &mut crate::devices::linear::LinearSystem,
    ) -> SpiceResult<()> {
        self.check_numbering()?;
        let n = self.unknown_count();
        if index >= self.devices.len()
            || bias.len() != n
            || system.a.rows() != n
            || system.e.rows() != n
            || state.is_some_and(|state| state.len() != self.state_len)
        {
            return Err(SpiceError::circuit(
                "single-device small-signal assembly does not match the circuit numbering",
            ));
        }
        let range = &self.branch_rows[index];
        let mut linear = crate::devices::linear::LinearContext {
            model_context: context,
            system,
            unknowns: &self.unknowns,
            branch: (!range.is_empty()).then_some(range.start),
            controls: self.controls(index)?,
            mutual: &[],
            states: match state {
                Some(state) => Some(
                    state
                        .get(self.state_rows[index].clone())
                        .ok_or_else(|| SpiceError::circuit("bias state is too short"))?,
                ),
                None => None,
            },
            pole_zero_statements: None,
            pole_zero_setups: 0,
        };
        device.assemble_small_signal(&mut linear, bias)
    }

    /// Disposable devices carrying the context's resistor and instance-parameter
    /// overrides, as `(device ordinal, replacement)`. The circuit's own devices
    /// are untouched; callers stamp the replacement instead of the original for
    /// that ordinal. Several instance overrides of one device are applied in
    /// slot order to the same replacement.
    pub(crate) fn replacements(
        &self,
        context: &ModelContext,
    ) -> SpiceResult<Vec<(usize, Box<dyn Device>)>> {
        let mut replacements: Vec<(usize, Box<dyn Device>)> = Vec::new();
        for target in context.resistor_overrides.iter().flatten() {
            let device = self
                .devices
                .get(target.device())
                .ok_or_else(|| SpiceError::circuit("stale resistor override"))?;
            let terminals = match device.terminals() {
                [a, b] if device.resistor_metadata().is_some() => [*a, *b],
                _ => {
                    return Err(SpiceError::circuit(format!(
                        "resistor override targets {}, which is not a two-terminal resistor",
                        device.name()
                    )));
                }
            };
            if replacements.iter().any(|(i, _)| *i == target.device()) {
                return Err(SpiceError::circuit("duplicate resistor override"));
            }
            let effective = device.resistor_effective(target.supplied(), context)?;
            replacements.push((
                target.device(),
                Box::new(Resistor::new(device.name(), terminals, effective)?),
            ));
        }
        let resistors = replacements.len();
        for target in context.instance_overrides.iter().flatten() {
            let original = self
                .devices
                .get(target.device())
                .ok_or_else(|| SpiceError::circuit("stale instance parameter override"))?;
            if replacements[..resistors]
                .iter()
                .any(|(i, _)| *i == target.device())
            {
                return Err(SpiceError::circuit(format!(
                    "{} has both a resistor and an instance parameter override",
                    original.name()
                )));
            }
            match replacements.iter_mut().find(|(i, _)| *i == target.device()) {
                Some((_, replacement)) => {
                    *replacement = replacement.with_instance_parameter(
                        target.parameter(),
                        target.value(),
                        context,
                    )?;
                }
                None => replacements.push((
                    target.device(),
                    original.with_instance_parameter(
                        target.parameter(),
                        target.value(),
                        context,
                    )?,
                )),
            }
        }
        Ok(replacements)
    }

    /// Loads every device for one trial into `matrix`, `rhs` and `trial`.
    ///
    /// The accepted history is read-only here. Resistors and instance
    /// parameters named by the model context's overrides stamp their per-point
    /// replacement; no device is modified. On error, `matrix`, `rhs` and
    /// `trial` hold a partial load and must be discarded; nothing the circuit
    /// or history owns has changed.
    ///
    /// # Errors
    ///
    /// Stale numbering, mismatched dimensions/nonfinite solution, device
    /// failures, or an invalid resistor or instance-parameter override.
    pub fn load(
        &self,
        request: &LoadRequest<'_>,
        matrix: &mut SparseMatrix,
        rhs: &mut Vector,
        trial: &mut TrialState,
    ) -> SpiceResult<()> {
        self.check_numbering()?;
        let n = self.unknown_count();
        if matrix.rows() != n
            || matrix.cols() != n
            || rhs.len() != n
            || request.solution.len() != n
            || !request.solution.is_finite()
            || request.history.len() != self.state_len
            || trial.values().len() != self.state_len
        {
            return Err(SpiceError::circuit(
                "load dimensions do not match the circuit numbering",
            ));
        }
        request
            .model_context
            .validate(&crate::primitives::SourceLoc::new(
                std::path::PathBuf::from("<model-context>"),
                1,
                1,
            ))?;
        matrix.set_pivot_tolerances(request.model_context.pivot);
        matrix.set_unknown_names(Some(self.unknown_names()));
        let replacements = self.replacements(request.model_context)?;
        let mutual = self.mutual_terms(request.model_context)?;
        let mut voltage_source_value = 0.;
        let unlent = matrix.lend_fixed(self.fixed_stamps.take(), self.devices.len());
        let stamped = self.stamp_devices(
            request,
            matrix,
            rhs,
            trial,
            &replacements,
            &mutual,
            &mut voltage_source_value,
        );
        self.fixed_stamps
            .replace(unlent.unwrap_or_else(|| matrix.return_fixed()));
        stamped?;
        self.learn_pattern(matrix);
        matrix.record_trace();
        Ok(())
    }

    /// [`Circuit::load`]'s device loop: each device stamps in load order.
    #[allow(clippy::too_many_arguments)]
    fn stamp_devices(
        &self,
        request: &LoadRequest<'_>,
        matrix: &mut SparseMatrix,
        rhs: &mut Vector,
        trial: &mut TrialState,
        replacements: &[(usize, Box<dyn Device>)],
        mutual: &[Vec<MutualTerm>],
        voltage_source_value: &mut Real,
    ) -> SpiceResult<()> {
        for (position, index) in self.load_order().into_iter().enumerate() {
            let device: &dyn Device = match replacements.iter().find(|(i, _)| *i == index) {
                Some((_, replacement)) => &**replacement,
                None => &*self.devices[index],
            };
            let states = request
                .history
                .device_of(trial, self.state_rows[index].clone(), index)?;
            matrix.begin_device(position);
            device.stamp(&mut StampContext {
                matrix: &mut *matrix,
                rhs: &mut *rhs,
                unknowns: &self.unknowns,
                nodes: &self.nodes,
                solution: request.solution,
                temperature: request.model_context.temperature,
                nominal_temperature: request.model_context.nominal_temperature,
                gmin: request.model_context.gmin,
                frequency: request.model_context.frequency,
                mode: request.mode,
                branches: self.branch_rows[index].clone(),
                controls: self.controls(index)?,
                mutual: mutual.get(index).map_or(&[], Vec::as_slice),
                integration: request.integration,
                states,
                forcing: request.forcing,
                source_factor: request.model_context.source_factor,
                voltage_source_value: &mut *voltage_source_value,
            })?;
        }
        Ok(())
    }

    /// A zeroed MNA matrix for [`Circuit::load`]: bound to the learned element
    /// pattern (stamps accumulate by element, no triplet list or sort), or a
    /// plain triplet matrix before the first load.
    #[must_use]
    pub fn new_matrix(&self) -> SparseMatrix {
        let n = self.unknown_count();
        match &*self.pattern.borrow() {
            Some(pattern) if pattern.order() == n => SparseMatrix::bound(pattern.clone()),
            _ => SparseMatrix::new(n, n),
        }
    }

    /// Takes over `previous`'s matrix (C's one `CKTmatrix`, with its pivot
    /// order and reorder flags), carried state vector (C's `CKTstate0`) and
    /// final transient load snapshot (instance fields such as MOS junction
    /// capacitances) for a freshly elaborated copy of the same circuit, as C runs every analysis
    /// of a deck on the matrix and state vectors set up once.
    /// The matrix is taken only when it starts with exactly the elements
    /// this circuit has allocated (the rule `finalize` applies on
    /// re-finalization); otherwise this circuit keeps its own.
    pub fn inherit_from(&mut self, previous: &Self) {
        self.fixed_stamps.get_mut().forget();
        self.completed.clone_from(&previous.completed);
        *self.final_transient.borrow_mut() = previous
            .final_transient
            .borrow()
            .as_ref()
            .filter(|state| state.matches(previous) && state.matches(self))
            .cloned();
        self.carried_state
            .borrow_mut()
            .clone_from(&previous.carried_state.borrow());
        let Some(inherited) = previous.pattern.borrow().clone() else {
            return;
        };
        let mut slot = self.pattern.borrow_mut();
        let compatible = slot.as_ref().is_none_or(|current| {
            inherited.order() == current.order()
                && inherited.len() >= current.len()
                && (0..current.len()).all(|index| {
                    let id = crate::maths::sparse::element_id(index);
                    inherited.position(id) == current.position(id)
                })
        });
        if compatible {
            *slot = Some(inherited);
        }
    }

    /// Records that `kind` has run on this circuit; [`Self::inherit_from`]
    /// carries the record to the next analysis's circuit.
    pub fn note_completed(&mut self, kind: crate::primitives::AnalysisKind) {
        self.completed.push(kind);
    }

    /// The analyses already run on this circuit (and the circuits it
    /// inherited from), in order.
    #[must_use]
    pub fn completed_analyses(&self) -> &[crate::primitives::AnalysisKind] {
        &self.completed
    }

    /// The circuit matrix's element pattern (C's one `CKTmatrix`), once
    /// `finalize` or a load has created it for the present numbering.
    #[must_use]
    pub fn matrix_pattern(&self) -> Option<std::sync::Arc<crate::maths::Pattern>> {
        self.pattern
            .borrow()
            .as_ref()
            .filter(|pattern| pattern.order() == self.unknown_count())
            .cloned()
    }

    /// Records the positions a load stamped: the first load defines the
    /// pattern in stamp order; positions outside it are appended (which, as a
    /// new SPARSE element does, forces the next factorization to reorder).
    fn learn_pattern(&self, matrix: &SparseMatrix) {
        let n = self.unknown_count();
        let mut slot = self.pattern.borrow_mut();
        let current = slot.as_ref().filter(|pattern| pattern.order() == n);
        let next = if let Some((bound, mut extra)) = matrix.overflow() {
            let mut extra = extra.by_ref().peekable();
            if extra.peek().is_none() {
                return;
            }
            match current {
                Some(pattern) if std::sync::Arc::ptr_eq(pattern, bound) => pattern.extended(extra),
                _ => return,
            }
        } else {
            let positions = matrix.triplets().iter().map(|t| (t.row, t.col));
            match current {
                None => crate::maths::Pattern::new(n, positions),
                Some(pattern) => {
                    let mut missing = positions
                        .filter(|&(row, col)| pattern.find(row, col).is_none())
                        .peekable();
                    if missing.peek().is_none() {
                        return;
                    }
                    pattern.extended(missing)
                }
            }
        };
        *slot = Some(std::sync::Arc::new(next));
    }

    fn run_accept_hooks(
        &self,
        solution: &Vector,
        time: Option<Real>,
        trial: Option<&TrialState>,
    ) -> SpiceResult<()> {
        self.check_numbering()?;
        if solution.len() != self.unknown_count() || !solution.is_finite() {
            return Err(SpiceError::circuit(
                "accepted solution does not match the circuit numbering or is nonfinite",
            ));
        }
        if time.is_some_and(|t| !t.is_finite()) {
            return Err(SpiceError::circuit("accepted time is nonfinite"));
        }
        for (index, device) in self.devices.iter().enumerate() {
            let states = match trial {
                Some(trial) => Some(
                    trial
                        .slice(self.state_rows[index].clone())
                        .ok_or_else(|| SpiceError::circuit("trial state is too short"))?,
                ),
                None => None,
            };
            device.accept(&AcceptContext {
                solution,
                time,
                branches: &self.branch_rows[index],
                states,
            })?;
        }
        Ok(())
    }

    /// Accepts a point whose trial state is committed into `history`.
    ///
    /// Validation and every [`Device::accept`] hook run first; the history
    /// changes only if all of them succeed, so a failure is atomic.
    ///
    /// # Errors
    ///
    /// Invalid solution/time, an uncommittable trial, or a hook failure.
    pub fn accept_point(
        &self,
        solution: &Vector,
        time: Option<Real>,
        history: &mut StateHistory,
        trial: TrialState,
    ) -> SpiceResult<()> {
        if history.len() != self.state_len {
            return Err(SpiceError::circuit(
                "state history does not match the circuit numbering",
            ));
        }
        history.check(&trial)?;
        self.run_accept_hooks(solution, time, Some(&trial))?;
        history.commit(trial)
    }

    /// Accepts a companion-transient point: [`Self::accept_point`] plus the
    /// delay histories of transmission lines ([`crate::devices::delay`]).
    ///
    /// With `delay.start` (the `t = 0` point) every delay line initializes
    /// its history ([`crate::devices::delay::DelayLine::start`]); otherwise it
    /// records the accepted sample ([`crate::devices::delay::DelayLine::accept`]).
    /// Every update is computed and validated first, then the hooks run, the
    /// fixed state is committed and the delay updates are applied, so a
    /// failure leaves the history unchanged. Returns the breakpoints the
    /// devices request (C `CKTsetBreak` from `DEVaccept`), in C's load order;
    /// a circuit without delay lines behaves exactly as
    /// [`Self::accept_point`] and returns none.
    ///
    /// # Errors
    ///
    /// As [`Self::accept_point`], and delay-line failures or invalid updates.
    pub fn accept_transient_point(
        &self,
        solution: &Vector,
        time: Real,
        history: &mut StateHistory,
        trial: TrialState,
        delay: &DelayAcceptance,
    ) -> SpiceResult<Vec<Real>> {
        if history.len() != self.state_len {
            return Err(SpiceError::circuit(
                "state history does not match the circuit numbering",
            ));
        }
        history.check(&trial)?;
        let mut updates = Vec::new();
        // TRAaccept walks C's model/instance lists: the load order.
        for index in self.load_order() {
            let device = &self.devices[index];
            let Some(line) = device.delay_line() else {
                continue;
            };
            let samples = history.delay(index).ok_or_else(|| {
                SpiceError::circuit(format!(
                    "{}: the state history has no delay history (use Circuit::state_history)",
                    device.name()
                ))
            })?;
            let context = crate::devices::delay::DelayContext {
                solution,
                unknowns: &self.unknowns,
                time,
                steps: delay.steps,
                min_break: delay.min_break,
                reltol: delay.reltol,
                abstol: delay.abstol,
                initial_conditions: trial.initial_conditions(),
                branches: self.branch_rows(index).unwrap_or(0..0),
            };
            let update = if delay.start {
                let update = line.start(&context)?;
                if update.reset.is_none() {
                    return Err(SpiceError::circuit(format!(
                        "{}: a delay-line start must reset its history",
                        device.name()
                    )));
                }
                update
            } else {
                if samples.is_empty() {
                    return Err(SpiceError::circuit(format!(
                        "{}: delay history used before the transient start",
                        device.name()
                    )));
                }
                line.accept(samples, &context)?
            };
            updates.push((index, update));
        }
        history.check_delays(&updates)?;
        self.run_accept_hooks(solution, Some(time), Some(&trial))?;
        history.commit(trial)?;
        history.apply_delays(&updates)?;
        Ok(updates
            .into_iter()
            .flat_map(|(_, update)| update.breakpoints)
            .collect())
    }

    /// The tightest step bound of every delay line for a converged transient
    /// trial at `time` (C `DEVtrunc` of `TRA`), or `None` without delay lines.
    ///
    /// # Errors
    ///
    /// A missing delay history or a device failure.
    pub fn delay_timestep_limit(
        &self,
        history: &StateHistory,
        solution: &Vector,
        time: Real,
        delay: &DelayAcceptance,
    ) -> SpiceResult<Option<Real>> {
        let mut limit: Option<Real> = None;
        for (index, device) in self.devices.iter().enumerate() {
            let Some(line) = device.delay_line() else {
                continue;
            };
            let samples = history
                .delay(index)
                .filter(|h| !h.is_empty())
                .ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: delay history used before the transient start",
                        device.name()
                    ))
                })?;
            let context = crate::devices::delay::DelayContext {
                solution,
                unknowns: &self.unknowns,
                time,
                steps: delay.steps,
                min_break: delay.min_break,
                reltol: delay.reltol,
                abstol: delay.abstol,
                initial_conditions: false,
                branches: self.branch_rows(index).unwrap_or(0..0),
            };
            if let Some(bound) = line.timestep_limit(samples, &context)? {
                if bound.is_nan() {
                    return Err(SpiceError::circuit(format!(
                        "{}: NaN timestep limit",
                        device.name()
                    )));
                }
                limit = Some(limit.map_or(bound, |l| l.min(bound)));
            }
        }
        Ok(limit)
    }

    /// Accepts a point of an analysis that tracks no device state (linear
    /// DC/AC). Hooks see `states: None`.
    ///
    /// # Errors
    ///
    /// Invalid solution/time or a hook failure.
    pub fn accept_solution(&self, solution: &Vector, time: Option<Real>) -> SpiceResult<()> {
        self.run_accept_hooks(solution, time, None)
    }

    /// Assemble at the default circuit/nominal temperature (27 Celsius).
    /// # Errors
    /// Invalid topology, model derivation or unsupported equations.
    pub fn linear_system(&mut self) -> SpiceResult<crate::devices::linear::LinearSystem> {
        self.linear_system_with_context(&crate::devices::models::ModelContext::default())
    }

    /// Assembles immutable equations at explicit circuit/nominal temperatures.
    /// Model recipes are reevaluated without cumulative value/state changes.
    /// # Errors
    /// Invalid context/topology, model arithmetic or unsupported equations.
    pub fn linear_system_with_context(
        &mut self,
        context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<crate::devices::linear::LinearSystem> {
        context.validate(&crate::primitives::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        self.finalize()?;
        let replacements = self.replacements(context)?;
        let mutual = self.mutual_terms(context)?;
        let mut system = crate::devices::linear::LinearSystem::new(self.unknown_count());
        system.a.set_pivot_tolerances(context.pivot);
        system.a.set_unknown_names(Some(self.unknown_names()));
        for index in self.load_order() {
            let device: &dyn Device = match replacements.iter().find(|(i, _)| *i == index) {
                Some((_, replacement)) => &**replacement,
                None => &*self.devices[index],
            };
            let range = &self.branch_rows[index];
            device.assemble_linear(&mut crate::devices::linear::LinearContext {
                model_context: context,
                system: &mut system,
                unknowns: &self.unknowns,
                branch: (!range.is_empty()).then_some(range.start),
                controls: self.controls(index)?,
                mutual: mutual.get(index).map_or(&[], Vec::as_slice),
                states: None,
                pole_zero_statements: None,
                pole_zero_setups: 0,
            })?;
        }
        system.a.fold_duplicates();
        // E stays unfolded: `CKTacLoad` scales every device term by
        // `omega` before adding it (`xc = cap * omega`), so the complex
        // assembly must scale each contribution, not their sum.
        Ok(system)
    }

    /// Assemble `G(bias)` and `dQ/dx(bias)` for nonlinear small-signal analysis.
    /// This is deliberately separate from immutable linear/BDF assembly.
    /// # Errors
    /// Invalid bias/context, stale numbering, or unsupported device physics.
    pub fn small_signal_system(
        &self,
        context: &ModelContext,
        bias: &Vector,
    ) -> SpiceResult<crate::devices::linear::LinearSystem> {
        self.small_signal_system_at(context, bias, None)
    }

    /// [`Self::small_signal_system`] with a full state vector (C's
    /// `CKTstate0` at `MODEINITSMSIG`) that devices with discrete state
    /// (switches) read through [`crate::devices::LinearContext::states`].
    /// # Errors
    /// As [`Self::small_signal_system`], or a state of the wrong length.
    pub fn small_signal_system_at(
        &self,
        context: &ModelContext,
        bias: &Vector,
        state: Option<&[Real]>,
    ) -> SpiceResult<crate::devices::linear::LinearSystem> {
        self.bias_linearized_system(context, bias, state, 0, None)
    }

    /// The pole-zero pencil `A + s E` at a solved bias point: every device's
    /// [`Device::assemble_pole_zero`] (C `CKTpzLoad` without the drive and
    /// column operations, which belong to the analysis). The same bias and
    /// state rules as [`Self::small_signal_system_at`] apply; no source
    /// forcing is assembled.
    /// # Errors
    /// As [`Self::small_signal_system_at`], and an explicit error for any
    /// device without a pole-zero load.
    pub fn pole_zero_system_at(
        &self,
        context: &ModelContext,
        bias: &Vector,
        state: Option<&[Real]>,
    ) -> SpiceResult<crate::devices::linear::LinearSystem> {
        self.bias_linearized_system(context, bias, state, 1, None)
    }

    /// The pole-zero load as `CKTpzLoad` adds it: every device's
    /// [`Device::assemble_pole_zero`] contributions in load order, one
    /// [`crate::maths::pencil::PencilTerm`] per device and element. A
    /// device's static stamps at one element are summed into its `g` and
    /// its dynamic stamps into its `c` (one [`crate::maths::DynamicGroup`]
    /// as `scale * (x1 + x2 + ...)`), as the C loads add `g + c * s`
    /// (`diopzld.c` adds `geq + gspr + xceq * s->real`).
    /// # Errors
    /// As [`Self::pole_zero_system_at`].
    pub fn pole_zero_terms_at(
        &self,
        context: &ModelContext,
        bias: &Vector,
        state: Option<&[Real]>,
        setups: u32,
    ) -> SpiceResult<Vec<crate::maths::pencil::PencilTerm>> {
        let mut ranges = Vec::new();
        let system =
            self.bias_linearized_system(context, bias, state, setups.max(1), Some(&mut ranges))?;
        let a = system.a.triplets();
        let e = system.e.triplets();
        let mut terms = Vec::new();
        for (a_range, e_range, statements) in ranges {
            if !statements.is_empty() {
                // The device recorded its C statements in order.
                terms.extend(statements);
                continue;
            }
            let first = terms.len();
            let slot = |terms: &mut Vec<crate::maths::pencil::PencilTerm>, row, col| {
                if let Some(i) = terms[first..]
                    .iter()
                    .position(|t: &crate::maths::pencil::PencilTerm| t.row == row && t.col == col)
                {
                    first + i
                } else {
                    terms.push(crate::maths::pencil::PencilTerm {
                        row,
                        col,
                        conductance: None,
                        capacitance: None,
                    });
                    terms.len() - 1
                }
            };
            for t in &a[a_range] {
                let i = slot(&mut terms, t.row, t.col);
                let g = &mut terms[i].conductance;
                *g = Some(g.map_or(t.value, |g| g + t.value));
            }
            let mut index = e_range.start;
            while index < e_range.end {
                let t = &e[index];
                let (value, next) = match system
                    .e_groups
                    .iter()
                    .find(|g| g.range.start == index && g.range.end <= e_range.end)
                {
                    Some(group) => {
                        let mut sum = group.terms.first().copied().unwrap_or(0.);
                        for term in group.terms.iter().skip(1) {
                            sum += term;
                        }
                        (group.scale * sum, group.range.end.max(index + 1))
                    }
                    None => (t.value, index + 1),
                };
                let i = slot(&mut terms, t.row, t.col);
                let c = &mut terms[i].capacitance;
                *c = Some(c.map_or(value, |c| c + value));
                index = next;
            }
        }
        Ok(terms)
    }

    fn bias_linearized_system(
        &self,
        context: &ModelContext,
        bias: &Vector,
        state: Option<&[Real]>,
        pole_zero_setups: u32,
        mut ranges: Option<&mut Vec<PoleZeroRange>>,
    ) -> SpiceResult<crate::devices::linear::LinearSystem> {
        self.check_numbering()?;
        if state.is_some_and(|state| state.len() != self.state_len) {
            return Err(SpiceError::circuit(
                "small-signal bias state does not match the circuit numbering",
            ));
        }
        context.validate(&crate::primitives::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        if bias.len() != self.unknown_count() || !bias.is_finite() {
            return Err(SpiceError::circuit(
                "invalid small-signal bias dimensions/values",
            ));
        }
        let replacements = self.replacements(context)?;
        let mutual = self.mutual_terms(context)?;
        let mut system = crate::devices::linear::LinearSystem::new(self.unknown_count());
        system.a.set_pivot_tolerances(context.pivot);
        system.a.set_unknown_names(Some(self.unknown_names()));
        for index in self.load_order() {
            let device: &dyn Device = match replacements.iter().find(|(i, _)| *i == index) {
                Some((_, replacement)) => &**replacement,
                None => &*self.devices[index],
            };
            let range = &self.branch_rows[index];
            let mut statements = Vec::new();
            let collecting = ranges.is_some();
            let mut linear = crate::devices::linear::LinearContext {
                model_context: context,
                system: &mut system,
                unknowns: &self.unknowns,
                branch: (!range.is_empty()).then_some(range.start),
                controls: self.controls(index)?,
                mutual: mutual.get(index).map_or(&[], Vec::as_slice),
                states: match state {
                    Some(state) => Some(
                        state
                            .get(self.state_rows[index].clone())
                            .ok_or_else(|| SpiceError::circuit("bias state is too short"))?,
                    ),
                    None => None,
                },
                pole_zero_statements: collecting.then_some(&mut statements),
                pole_zero_setups,
            };
            let start = (
                linear.system.a.triplets().len(),
                linear.system.e.triplets().len(),
            );
            if pole_zero_setups > 0 {
                device.assemble_pole_zero(&mut linear, bias)?;
            } else {
                device.assemble_small_signal(&mut linear, bias)?;
            }
            if let Some(ranges) = ranges.as_deref_mut() {
                ranges.push((
                    start.0..system.a.triplets().len(),
                    start.1..system.e.triplets().len(),
                    statements,
                ));
            }
        }
        if ranges.is_some() {
            // The caller reads the unfolded stamps.
            return Ok(system);
        }
        system.a.fold_duplicates();
        // E stays unfolded: `CKTacLoad` scales every device term by
        // `omega` before adding it (`xc = cap * omega`), so the complex
        // assembly must scale each contribution, not their sum.
        Ok(system)
    }

    /// Elaborate one AST instance atomically with explicit model context.
    /// Model lookup/schema validation happens before a factory; unavailable
    /// model-backed/nonlinear factories remain explicit errors. As with
    /// [`Circuit::add_device`], finalize after successful additions to renumber.
    ///
    /// # Errors
    /// Duplicate names, missing/wrong-family models, invalid setters/selectors
    /// or unavailable factories. Nodes, devices and branch rows are unchanged
    /// on failure, including construction/validation failures.
    pub fn add_instance(
        &mut self,
        instance: &crate::netlist::ast::DeviceInstance,
        models: &crate::devices::models::ModelResolver<'_>,
        context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<()> {
        self.add_instances(std::slice::from_ref(instance), models, context)
    }

    /// Elaborate several AST instances atomically, in order.
    ///
    /// Every device is built against one staged copy of the node table, and the
    /// nodes and devices are committed together only once the whole batch has
    /// succeeded. A failure at any point — a duplicate name, an unavailable
    /// factory, an invalid parameter — therefore leaves the caller's node table,
    /// devices and branch rows untouched, which is what subcircuit expansion
    /// (#18) needs when it hands over a flattened deck. As with
    /// [`Circuit::add_instance`], finalize after successful additions.
    ///
    /// # Errors
    /// [`SpiceError::Circuit`] on a duplicate instance name (against the
    /// existing devices and within the batch), or whatever
    /// [`crate::devices::factory::instantiate_with_models`] reports.
    pub fn add_instances(
        &mut self,
        instances: &[crate::netlist::ast::DeviceInstance],
        models: &crate::devices::models::ModelResolver<'_>,
        context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<()> {
        let mut seen: BTreeSet<String> = self
            .devices
            .iter()
            .map(|device| device.name().to_lowercase())
            .collect();
        let mut nodes = self.nodes.clone();
        let mut staged: Vec<Box<dyn Device>> = Vec::with_capacity(instances.len());
        let mut internal: Vec<Vec<NodeId>> = Vec::with_capacity(instances.len());
        let mut model_names: Vec<Option<String>> = Vec::with_capacity(instances.len());
        let mut generated_by: Vec<Option<usize>> = Vec::with_capacity(instances.len());
        // Node names written on the batch's cards, which URC-generated
        // internal nodes must not reuse (crate::devices::urc::expand).
        let deck_nodes: BTreeSet<&str> = instances
            .iter()
            .flat_map(|instance| instance.nodes.iter().map(String::as_str))
            .collect();
        let base = self.devices.len();
        for instance in instances {
            if !seen.insert(instance.name.to_lowercase()) {
                return Err(SpiceError::circuit(format!(
                    "duplicate instance name '{}'",
                    instance.name
                )));
            }
            let first = nodes.len();
            if instance.designator == 'y'
                && let Some(expanded) =
                    crate::devices::txl::fallback(instance, &mut nodes, models, &deck_nodes)?
            {
                for device in expanded {
                    if !seen.insert(device.name().to_ascii_lowercase()) {
                        return Err(SpiceError::circuit(format!(
                            "duplicate TXL generated instance {}",
                            device.name()
                        )));
                    }
                    staged.push(device);
                    internal.push(Vec::new());
                    model_names.push(None);
                    generated_by.push(None);
                }
                continue;
            }
            // A URC line is expanded into its generated R/C/D elements
            // (urcsetup.c), which take its place under their own names.
            if instance.designator == 'u' {
                let expanded = crate::devices::urc::expand(
                    instance,
                    &mut nodes,
                    models,
                    context,
                    &deck_nodes,
                )?;
                let created: Vec<&crate::primitives::Node> = nodes.nodes()[first..]
                    .iter()
                    .filter(|node| !instance.nodes.contains(&node.name))
                    .collect();
                let line = base + staged.len();
                let lower = instance.name.to_lowercase();
                for device in expanded {
                    // The URC instance itself keeps its (already checked) name.
                    let is_line = device.name().eq_ignore_ascii_case(&instance.name);
                    if !is_line && !seen.insert(device.name().to_lowercase()) {
                        return Err(SpiceError::circuit(format!(
                            "duplicate instance name '{}' generated by URC {}",
                            device.name(),
                            instance.name
                        )));
                    }
                    if is_line {
                        // URCsetup's CKTmkVolt calls: the ladder nodes are
                        // numbered at the URC line's own setup.
                        internal.push(
                            created
                                .iter()
                                .filter(|node| node.kind != NodeKind::Internal)
                                .map(|node| node.id)
                                .collect(),
                        );
                        model_names.push(instance.model.as_ref().map(|m| m.to_ascii_lowercase()));
                        generated_by.push(None);
                    } else {
                        // A generated diode's own prime node (diosetup.c).
                        internal.push(
                            device
                                .terminals()
                                .iter()
                                .copied()
                                .filter(|id| {
                                    created.iter().any(|node| {
                                        node.id == *id && node.kind == NodeKind::Internal
                                    })
                                })
                                .collect(),
                        );
                        // URCsetup's CKTmodCrt models `<name>#resmod`,
                        // `#capmod` and `#diodemod`.
                        let suffix = match device.c_device_type() {
                            crate::devices::setup::CDeviceType::Res => "resmod",
                            crate::devices::setup::CDeviceType::Cap => "capmod",
                            _ => "diodemod",
                        };
                        model_names.push(Some(format!("{lower}#{suffix}")));
                        generated_by.push(Some(line));
                    }
                    staged.push(device);
                }
                continue;
            }
            // Builtin factories bind terminals in this staged table. Nothing
            // fallible remains after committing the containers together.
            staged.push(crate::devices::factory::instantiate_with_models(
                instance, &mut nodes, models, context,
            )?);
            internal.push(
                nodes.nodes()[first..]
                    .iter()
                    .filter(|node| node.kind == NodeKind::Internal)
                    .map(|node| node.id)
                    .collect(),
            );
            model_names.push(instance.model.as_ref().map(|m| m.to_ascii_lowercase()));
            generated_by.push(None);
        }
        self.nodes = nodes;
        self.devices.extend(staged);
        self.internal_nodes.extend(internal);
        self.models.extend(model_names);
        self.generated_by.extend(generated_by);
        Ok(())
    }

    /// Elaborate literal (or top-level `.param`/`{expr}`-literalized) R/C/L/V/I and
    /// bounded model-backed R/C/L at 27 Celsius.
    ///
    /// A deck with `.option` cards is rejected here: this entry point would
    /// silently apply default temperatures. Resolve the options with
    /// `crate::analysis::RunConfig` and elaborate with
    /// [`Self::from_netlist_with_context`] (or `RunConfig::circuit`).
    /// # Errors
    /// Unsupported elaboration constructs, `.option` cards, models or invalid
    /// parameters.
    pub fn from_netlist(netlist: &crate::netlist::ast::Netlist) -> SpiceResult<Self> {
        if !netlist.options.is_empty()
            || netlist
                .subcircuits
                .iter()
                .any(|sub| !sub.options.is_empty())
        {
            return Err(SpiceError::Unsupported {
                feature: ".option cards require RunConfig::from_netlist to resolve them; \
                          Circuit::from_netlist would apply default temperatures"
                    .into(),
                location: netlist.options.first().map(|card| card.location.clone()),
            });
        }
        Self::from_netlist_with_context(netlist, &crate::devices::models::ModelContext::default())
    }

    /// Elaborate with explicit validation temperatures; the immutable model recipe
    /// is retained and later assemblies use their own explicit context.
    /// `.option` cards are the caller's responsibility: the supplied context must
    /// come from resolving them (`crate::analysis::RunConfig`). Top-level
    /// `.global` cards name nodes that stay global through subcircuit expansion
    /// (see [`crate::devices::subckt`]); everything else keeps the deck's names.
    /// # Errors
    /// Invalid context, unsupported constructs/models, invalid parameters or a
    /// failing subcircuit expansion ([`crate::devices::subckt::expand_subcircuits`]).
    /// Nothing is added to the returned circuit; a failed deck yields `Err`.
    pub fn from_netlist_with_context(
        netlist: &crate::netlist::ast::Netlist,
        context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<Self> {
        context.validate(&netlist.location)?;
        if !netlist.includes.is_empty() {
            return Err(SpiceError::Unsupported {
                feature: "includes in linear elaboration".into(),
                location: None,
            });
        }
        // The nonlinear E/G/F/H forms become B sources and generated XSPICE
        // instances first, as inpcom.c rewrites them before numparam and
        // subcircuit expansion (#79).
        let lowered = crate::netlist::behavioural::lower_nonlinear_sources(netlist)?;
        // Top-level `.param` values and `{expr}` sites are evaluated into a
        // literal copy before any factory sees them (#15).
        let elaborated = crate::netlist::elaborate::literalize(&lowered)?;
        let netlist = &elaborated.netlist;
        // `X` instances are expanded into a fresh device/model list before any
        // device is built, so a deck that fails to elaborate never leaves a
        // partial circuit behind (#18).
        let expanded = crate::devices::subckt::expand_subcircuits(
            netlist,
            &elaborated.scope,
            crate::devices::subckt::SubcircuitLimits::default(),
        )?;
        let models = crate::devices::models::ModelResolver::new(&expanded.models)?;
        let mut circuit = Self::new();
        circuit.add_instances(&expanded.devices, &models, context)?;
        let referenced: BTreeSet<_> = expanded
            .devices
            .iter()
            .filter_map(|instance| instance.model.as_ref())
            // A binned reference uses its whole `<name>.<n>` set (C's
            // `mark_all_binned`), not only the selected bin.
            .flat_map(|name| models.declarations_for(name))
            .map(|card| card.name.to_ascii_lowercase())
            .collect();
        if let Some(model) = netlist
            .models
            .iter()
            .find(|model| !referenced.contains(&model.name.to_ascii_lowercase()))
        {
            // First-declaration duplicate policy remains explicit; unused
            // declarations cannot silently discard unsupported physics. A root
            // model shadowed only inside a subcircuit body is unused by this
            // rule and is reported, not dropped silently.
            return Err(SpiceError::Unsupported {
                feature: "unused model declarations in scalar linear elaboration".into(),
                location: Some(model.location.clone()),
            });
        }
        circuit.finalize()?;
        // Reject an invalid inductive system (K coupling) before any analysis.
        circuit.mutual_terms(context)?;
        Ok(circuit)
    }

    /// Looks up a device by instance name, case-insensitively.
    #[must_use]
    pub fn device(&self, name: &str) -> Option<&dyn Device> {
        self.devices
            .iter()
            .find(|device| device.name().eq_ignore_ascii_case(name))
            .map(AsRef::as_ref)
    }
}

/// One device's stamps in a pole-zero term collection: its A and E triplet
/// ranges and any C statements it recorded in order
/// ([`crate::devices::linear::LinearContext::pole_zero_statements`]).
type PoleZeroRange = (
    std::ops::Range<usize>,
    std::ops::Range<usize>,
    Vec<crate::maths::pencil::PencilTerm>,
);

#[cfg(test)]
mod tests {
    use super::{Circuit, CircuitGraph, CircuitVertex};
    use crate::devices::traits::Device;
    use crate::primitives::{NodeId, SpiceError, SpiceResult};
    use petgraph::algo::{connected_components, has_path_connecting};
    use petgraph::graph::NodeIndex;
    use petgraph::visit::EdgeRef;

    /// A test double with arbitrary terminals that never stamps.
    #[derive(Debug)]
    struct Stub {
        name: String,
        terminals: Vec<NodeId>,
        branch_currents: usize,
    }

    impl Device for Stub {
        fn name(&self) -> &str {
            &self.name
        }

        fn designator(&self) -> char {
            'r'
        }

        fn terminals(&self) -> &[NodeId] {
            &self.terminals
        }

        fn branch_currents(&self) -> usize {
            self.branch_currents
        }

        fn stamp(
            &self,
            _context: &mut crate::devices::traits::StampContext<'_>,
        ) -> SpiceResult<()> {
            Err(SpiceError::not_yet_ported("test stub", "tests"))
        }
    }

    fn stub(circuit: &mut Circuit, name: &str, a: &str, b: &str, branch_currents: usize) -> Stub {
        let first = circuit.add_node(a);
        let second = circuit.add_node(b);
        Stub {
            name: name.to_owned(),
            terminals: vec![first, second],
            branch_currents,
        }
    }

    #[test]
    fn unknowns_are_renumbered_after_every_device_is_added() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "in", "out", 0);
        circuit.add_device(Box::new(device)).unwrap();
        let source = stub(&mut circuit, "v1", "in", "0", 1);
        circuit.add_device(Box::new(source)).unwrap();

        circuit.finalize().unwrap();
        // Two nodes plus one branch current.
        assert_eq!(circuit.unknown_count(), 3);
        assert_eq!(circuit.device_count(), 2);
        assert_eq!(circuit.device("R1").map(Device::name), Some("r1"));
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let mut circuit = Circuit::new();
        let first = stub(&mut circuit, "r1", "a", "b", 0);
        circuit.add_device(Box::new(first)).unwrap();
        let second = stub(&mut circuit, "R1", "a", "b", 0);
        let error = circuit.add_device(Box::new(second)).unwrap_err();
        assert!(error.to_string().contains("duplicate instance name"));
    }

    #[test]
    fn dangling_terminals_are_rejected() {
        let mut circuit = Circuit::new();
        let known = circuit.add_node("a");
        let dangling = NodeId::new(99);
        let device = Stub {
            name: "r1".to_owned(),
            terminals: vec![known, dangling],
            branch_currents: 0,
        };
        let error = circuit.add_device(Box::new(device)).unwrap_err();
        assert!(error.to_string().contains("not in the node table"));
    }

    #[test]
    fn finalize_catches_devices_added_behind_its_back() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "a", "b", 0);
        circuit.add_device(Box::new(device)).unwrap();
        circuit.finalize().unwrap();
        assert_eq!(circuit.unknown_count(), 2);

        // A node added afterwards shifts nothing until the next rebuild.
        circuit.add_node("late");
        circuit.finalize().unwrap();
        assert_eq!(circuit.unknown_count(), 3);
    }

    #[test]
    fn debug_output_lists_device_names() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "a", "b", 0);
        circuit.add_device(Box::new(device)).unwrap();
        let text = format!("{circuit:?}");
        assert!(text.contains("r1"), "{text}");
        assert!(text.contains("nodes: 3"), "{text}");
    }

    fn vertex(graph: &CircuitGraph, weight: CircuitVertex) -> NodeIndex<usize> {
        graph
            .node_indices()
            .find(|index| graph[*index] == weight)
            .unwrap()
    }

    fn ports(graph: &CircuitGraph, device: usize) -> Vec<(usize, CircuitVertex)> {
        let index = vertex(graph, CircuitVertex::Device(device));
        let mut ports: Vec<_> = graph
            .edges(index)
            .map(|edge| (*edge.weight(), graph[edge.target()]))
            .collect();
        ports.sort_by_key(|(port, _)| *port);
        ports
    }

    #[test]
    fn topology_keeps_ground_and_unused_nodes() {
        let mut circuit = Circuit::new();
        let empty = circuit.topology().unwrap();
        assert_eq!(empty.node_count(), 1);
        assert_eq!(empty.edge_count(), 0);
        assert_eq!(
            empty.node_weights().copied().collect::<Vec<_>>(),
            vec![CircuitVertex::Node(NodeId::GROUND)]
        );
        let unused = circuit.add_node("unused");
        let graph = circuit.topology().unwrap();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(connected_components(&graph), 2);
        assert_eq!(
            graph[vertex(&graph, CircuitVertex::Node(unused))],
            CircuitVertex::Node(unused)
        );
        // Connectivity alone is not grounds for rejecting a circuit here.
        circuit.finalize().unwrap();
        assert_eq!(circuit.unknown_count(), 1);
    }

    #[test]
    fn topology_distinguishes_node_ids_device_ordinals_and_mna_rows() {
        let mut circuit = Circuit::new();
        let resistor = stub(&mut circuit, "r1", "in", "out", 0);
        let [input, output] = [resistor.terminals[0], resistor.terminals[1]];
        circuit.add_device(Box::new(resistor)).unwrap();
        let source = stub(&mut circuit, "v1", "in", "GND", 1);
        circuit.add_device(Box::new(source)).unwrap();
        circuit.finalize().unwrap();
        let graph = circuit.topology().unwrap();
        assert_eq!(graph.node_count(), 5);
        assert_eq!(graph.edge_count(), 4);
        assert_eq!(connected_components(&graph), 1);
        assert_eq!(
            ports(&graph, 0),
            vec![
                (0, CircuitVertex::Node(input)),
                (1, CircuitVertex::Node(output))
            ]
        );
        assert_eq!(
            ports(&graph, 1),
            vec![
                (0, CircuitVertex::Node(input)),
                (1, CircuitVertex::Node(NodeId::GROUND))
            ]
        );
        assert_ne!(
            vertex(&graph, CircuitVertex::Device(0)),
            vertex(&graph, CircuitVertex::Node(NodeId::GROUND))
        );
        assert_eq!(circuit.unknowns().node_row(input), Some(0));
        assert_eq!(circuit.unknowns().node_row(NodeId::GROUND), None);
        assert_eq!(circuit.unknown_count(), 3);
    }

    #[test]
    fn multiport_and_repeated_terminals_keep_parallel_edges_and_ordinals() {
        let mut circuit = Circuit::new();
        let output = circuit.add_node("out");
        let ground = circuit.add_node("gnd");
        circuit
            .add_device(Box::new(Stub {
                name: "m1".to_owned(),
                terminals: vec![ground, output, output, ground],
                branch_currents: 0,
            }))
            .unwrap();
        let graph = circuit.topology().unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 4);
        assert_eq!(
            ports(&graph, 0),
            vec![
                (0, CircuitVertex::Node(ground)),
                (1, CircuitVertex::Node(output)),
                (2, CircuitVertex::Node(output)),
                (3, CircuitVertex::Node(ground)),
            ]
        );
    }

    #[test]
    fn petgraph_finds_structurally_separate_circuit_blocks() {
        let mut circuit = Circuit::new();
        let grounded = stub(&mut circuit, "c1", "out", "0", 0);
        let output = grounded.terminals[0];
        circuit.add_device(Box::new(grounded)).unwrap();
        let isolated = stub(&mut circuit, "r2", "a", "b", 0);
        let island = isolated.terminals[0];
        circuit.add_device(Box::new(isolated)).unwrap();
        let graph = circuit.topology().unwrap();
        let ground = vertex(&graph, CircuitVertex::Node(NodeId::GROUND));
        assert_eq!(connected_components(&graph), 2);
        assert!(has_path_connecting(
            &graph,
            ground,
            vertex(&graph, CircuitVertex::Node(output)),
            None
        ));
        assert!(!has_path_connecting(
            &graph,
            ground,
            vertex(&graph, CircuitVertex::Node(island)),
            None
        ));
        circuit.finalize().unwrap(); // structural islands are not numerical diagnostics
    }

    #[test]
    fn zero_port_devices_are_kept_without_inventing_matrix_row_vertices() {
        let mut circuit = Circuit::new();
        circuit
            .add_device(Box::new(Stub {
                name: "internal".to_owned(),
                terminals: vec![],
                branch_currents: 2,
            }))
            .unwrap();
        circuit.finalize().unwrap();
        let graph = circuit.topology().unwrap();
        assert_eq!(graph.node_count(), 2); // ground and the device, not its rows
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(connected_components(&graph), 2);
        assert_eq!(circuit.unknown_count(), 2);
        assert_eq!(
            graph[vertex(&graph, CircuitVertex::Device(0))],
            CircuitVertex::Device(0)
        );
    }

    #[test]
    fn topology_is_rebuilt_after_node_and_device_mutation() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "a", "0", 0);
        let a = device.terminals[0];
        circuit.add_device(Box::new(device)).unwrap();
        let before = circuit.topology().unwrap();
        circuit.add_node("late");
        circuit.devices_mut()[0] = Box::new(Stub {
            name: "r1".to_owned(),
            terminals: vec![a, a],
            branch_currents: 0,
        });
        let after = circuit.topology().unwrap();
        assert_eq!(connected_components(&before), 1);
        assert_eq!(connected_components(&after), 3);
        assert_eq!(before.node_count(), 3);
        assert_eq!(after.node_count(), 4);
        assert_ne!(
            vertex(&before, CircuitVertex::Device(0)),
            vertex(&after, CircuitVertex::Device(0))
        );
        assert_eq!(
            ports(&after, 0),
            vec![(0, CircuitVertex::Node(a)), (1, CircuitVertex::Node(a))]
        );
    }

    #[test]
    fn mutated_duplicate_names_fail_without_changing_numbering() {
        let mut circuit = Circuit::new();
        let first = stub(&mut circuit, "r1", "a", "0", 1);
        circuit.add_device(Box::new(first)).unwrap();
        let second = stub(&mut circuit, "r2", "b", "0", 1);
        let b = second.terminals[0];
        circuit.add_device(Box::new(second)).unwrap();
        circuit.finalize().unwrap();
        let original = circuit.unknowns().clone();
        circuit.add_node("late");
        circuit.devices_mut()[1] = Box::new(Stub {
            name: "R1".to_owned(),
            terminals: vec![b, NodeId::GROUND],
            branch_currents: 5,
        });
        assert!(
            circuit
                .topology()
                .unwrap_err()
                .to_string()
                .contains("duplicate instance name")
        );
        assert!(
            circuit
                .finalize()
                .unwrap_err()
                .to_string()
                .contains("duplicate instance name")
        );
        assert_eq!(circuit.unknowns(), &original);
    }

    #[test]
    fn mutated_dangling_terminals_fail_without_changing_numbering() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "a", "0", 0);
        let a = device.terminals[0];
        circuit.add_device(Box::new(device)).unwrap();
        circuit.finalize().unwrap();
        let original = circuit.unknowns().clone();
        circuit.devices_mut()[0] = Box::new(Stub {
            name: "bad".to_owned(),
            terminals: vec![a, NodeId::new(99)],
            branch_currents: 5,
        });
        let error = circuit.topology().unwrap_err();
        assert!(matches!(error, SpiceError::Circuit { .. }));
        assert!(error.to_string().contains("node 99"));
        assert!(
            circuit
                .finalize()
                .unwrap_err()
                .to_string()
                .contains("not in the node table")
        );
        assert_eq!(circuit.unknowns(), &original);
    }

    #[test]
    fn topology_preserves_deck_order_and_is_deterministic() {
        let mut circuit = Circuit::new();
        for name in ["z1", "a1"] {
            let device = stub(&mut circuit, name, "a", "b", 0);
            circuit.add_device(Box::new(device)).unwrap();
        }
        let a = circuit.nodes().get("a").unwrap();
        let b = circuit.nodes().get("b").unwrap();
        let first = circuit.topology().unwrap();
        let second = circuit.topology().unwrap();
        let expected = vec![
            CircuitVertex::Node(NodeId::GROUND),
            CircuitVertex::Node(a),
            CircuitVertex::Node(b),
            CircuitVertex::Device(0),
            CircuitVertex::Device(1),
        ];
        assert_eq!(first.node_weights().copied().collect::<Vec<_>>(), expected);
        assert_eq!(second.node_weights().copied().collect::<Vec<_>>(), expected);
        for device in 0..2 {
            assert_eq!(ports(&first, device), ports(&second, device));
        }
        assert_eq!(circuit.devices()[0].name(), "z1");
        assert_eq!(circuit.devices()[1].name(), "a1");
    }
}
