//! The circuit: nodes, unknowns and device instances.
//!
//! The C equivalent is `CKTcircuit` in `src/include/ngspice/cktdefs.h`, built by
//! `CKTcrtElt`/`CKTbindNode` and friends in `src/spicelib/devices/ckt*.c`. The
//! port keeps the same two-step shape:
//!
//! 1. nodes and devices are *added*, in deck order;
//! 2. [`Circuit::rebuild_unknowns`] numbers the MNA unknowns, which must happen
//!    after every node and device is known, because branch currents get rows
//!    after the nodes.
//!
//! Step 2 is easy to forget, so the analyses call
//! [`Circuit::finalize`], which validates the graph and rebuilds the numbering
//! in one go.

use std::collections::BTreeSet;
use std::fmt;

use petgraph::graph::UnGraph;
use spice_core::{NodeId, NodeTable, Real, SpiceError, SpiceResult};
use spice_maths::{Coefficients, SparseMatrix, Vector};

use crate::linear::Forcing;
use crate::models::ModelContext;
use crate::rlc::Resistor;
use crate::state::{StateHistory, TrialState};
use crate::sweep::{ResistorMetadata, ResistorOverride};
use crate::traits::{
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
    nodes: NodeTable,
    unknowns: MnaUnknowns,
    devices: Vec<Box<dyn Device>>,
    branch_rows: Vec<std::ops::Range<usize>>,
    /// Per device, the branch rows of its controlling sources, or why they
    /// could not be resolved (reported by `finalize` and every load).
    control_rows: Vec<Result<Vec<usize>, SpiceError>>,
    state_rows: Vec<std::ops::Range<usize>>,
    state_len: usize,
    /// Inductor pairs coupled by K devices, or why the K references could not
    /// be resolved (reported by `finalize` and every load).
    mutual: MutualBinding,
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

    /// Rebuilds the unknown numbering: one row per non-ground node, then one row
    /// per branch current, in device order. State slots are numbered the same
    /// way, in a separate namespace.
    pub fn rebuild_unknowns(&mut self) {
        self.unknowns.rebuild(&self.nodes);
        self.branch_rows.clear();
        self.state_rows.clear();
        self.state_len = 0;
        for device in &self.devices {
            let count = device.branch_currents();
            let start = self.unknowns.len();
            self.unknowns.add_rows(count);
            self.branch_rows.push(start..start + count);
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
    /// the port rejects it. See [`crate::mutual`].
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
        let mut values: std::collections::BTreeMap<usize, InductanceValue> =
            std::collections::BTreeMap::new();
        for pair in &self.mutual.pairs {
            for index in [pair.first, pair.second] {
                if values.contains_key(&index) {
                    continue;
                }
                let device = &self.devices[index];
                let value = device.inductance(context).ok_or_else(|| {
                    SpiceError::circuit(format!("{} is not an inductor", device.name()))
                })??;
                values.insert(index, value);
            }
        }
        let mut terms = vec![Vec::new(); self.devices.len()];
        let mut mutuals = Vec::with_capacity(self.mutual.pairs.len());
        for pair in &self.mutual.pairs {
            let (a, b) = (values[&pair.first], values[&pair.second]);
            let inductance = pair.coefficient * (a.coupling_base * b.coupling_base).abs().sqrt();
            if !inductance.is_finite() {
                return Err(SpiceError::circuit(format!(
                    "{}: nonfinite mutual inductance",
                    self.devices[pair.coupling].name()
                )));
            }
            terms[pair.first].push(MutualTerm {
                row: self.branch_rows[pair.second].start,
                inductance,
            });
            terms[pair.second].push(MutualTerm {
                row: self.branch_rows[pair.first].start,
                inductance,
            });
            mutuals.push(inductance);
        }
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
                let mut matrix = spice_maths::Matrix::zeros(n, n);
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
                    let normalized = mutual / scale;
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
        self.topology()?;
        self.rebuild_unknowns();
        for index in 0..self.devices.len() {
            self.controls(index)?;
        }
        if let Some(error) = &self.mutual.error {
            return Err(error.clone());
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
        self.devices.push(device);
        Ok(())
    }

    /// Branch rows by device ordinal, after finalization.
    pub fn branch_rows(&self, index: usize) -> Option<std::ops::Range<usize>> {
        self.branch_rows.get(index).cloned()
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

    /// An empty accepted-state history sized for this circuit.
    #[must_use]
    pub fn state_history(&self) -> StateHistory {
        StateHistory::new(self.state_len)
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
    /// no device changes. See [`crate::sweep`] for supplied-versus-effective
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
        context.validate(&spice_core::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        device.resistor_effective(target.supplied(), context)
    }

    /// Disposable resistors carrying the context's overrides' effective values,
    /// as `(device ordinal, resistor)`. The circuit's own devices are untouched;
    /// callers stamp the replacement instead of the original for that ordinal.
    fn resistor_replacements(&self, context: &ModelContext) -> SpiceResult<Vec<(usize, Resistor)>> {
        let mut replacements: Vec<(usize, Resistor)> = Vec::new();
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
                Resistor::new(device.name(), terminals, effective)?,
            ));
        }
        Ok(replacements)
    }

    /// Loads every device for one trial into `matrix`, `rhs` and `trial`.
    ///
    /// The accepted history is read-only here. Resistors named by the model
    /// context's overrides stamp their per-point effective value; no device is
    /// modified. On error, `matrix`, `rhs` and
    /// `trial` hold a partial load and must be discarded; nothing the circuit
    /// or history owns has changed.
    ///
    /// # Errors
    ///
    /// Stale numbering, mismatched dimensions/nonfinite solution, device
    /// failures, or an invalid resistor override.
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
        request.model_context.validate(&spice_core::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        let replacements = self.resistor_replacements(request.model_context)?;
        let mutual = self.mutual_terms(request.model_context)?;
        for (index, device) in self.devices.iter().enumerate() {
            let device: &dyn Device = match replacements.iter().find(|(i, _)| *i == index) {
                Some((_, resistor)) => resistor,
                None => &**device,
            };
            let states = request
                .history
                .device(trial, self.state_rows[index].clone())?;
            device.stamp(&mut StampContext {
                matrix: &mut *matrix,
                rhs: &mut *rhs,
                unknowns: &self.unknowns,
                nodes: &self.nodes,
                solution: request.solution,
                temperature: request.model_context.temperature,
                nominal_temperature: request.model_context.nominal_temperature,
                gmin: request.model_context.gmin,
                mode: request.mode,
                branches: self.branch_rows[index].clone(),
                controls: self.controls(index)?,
                mutual: mutual.get(index).map_or(&[], Vec::as_slice),
                integration: request.integration,
                states,
                forcing: request.forcing,
            })?;
        }
        Ok(())
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

    /// Accepts a point of an analysis that tracks no device state (linear
    /// DC/AC and the explicit diffsol BDF backend). Hooks see `states: None`.
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
    pub fn linear_system(&mut self) -> SpiceResult<crate::linear::LinearSystem> {
        self.linear_system_with_context(&crate::models::ModelContext::default())
    }

    /// Assembles immutable equations at explicit circuit/nominal temperatures.
    /// Model recipes are reevaluated without cumulative value/state changes.
    /// # Errors
    /// Invalid context/topology, model arithmetic or unsupported equations.
    pub fn linear_system_with_context(
        &mut self,
        context: &crate::models::ModelContext,
    ) -> SpiceResult<crate::linear::LinearSystem> {
        context.validate(&spice_core::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        self.finalize()?;
        let replacements = self.resistor_replacements(context)?;
        let mutual = self.mutual_terms(context)?;
        let mut system = crate::linear::LinearSystem::new(self.unknown_count());
        for (index, device) in self.devices.iter().enumerate() {
            let device: &dyn Device = match replacements.iter().find(|(i, _)| *i == index) {
                Some((_, resistor)) => resistor,
                None => &**device,
            };
            let range = &self.branch_rows[index];
            device.assemble_linear(&mut crate::linear::LinearContext {
                model_context: context,
                system: &mut system,
                unknowns: &self.unknowns,
                branch: (!range.is_empty()).then_some(range.start),
                controls: self.controls(index)?,
                mutual: mutual.get(index).map_or(&[], Vec::as_slice),
                states: None,
            })?;
        }
        system.a.fold_duplicates();
        system.e.fold_duplicates();
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
    ) -> SpiceResult<crate::linear::LinearSystem> {
        self.small_signal_system_at(context, bias, None)
    }

    /// [`Self::small_signal_system`] with a full state vector (C's
    /// `CKTstate0` at `MODEINITSMSIG`) that devices with discrete state
    /// (switches) read through [`crate::LinearContext::states`].
    /// # Errors
    /// As [`Self::small_signal_system`], or a state of the wrong length.
    pub fn small_signal_system_at(
        &self,
        context: &ModelContext,
        bias: &Vector,
        state: Option<&[Real]>,
    ) -> SpiceResult<crate::linear::LinearSystem> {
        self.check_numbering()?;
        if state.is_some_and(|state| state.len() != self.state_len) {
            return Err(SpiceError::circuit(
                "small-signal bias state does not match the circuit numbering",
            ));
        }
        context.validate(&spice_core::SourceLoc::new(
            std::path::PathBuf::from("<model-context>"),
            1,
            1,
        ))?;
        if bias.len() != self.unknown_count() || !bias.is_finite() {
            return Err(SpiceError::circuit(
                "invalid small-signal bias dimensions/values",
            ));
        }
        let replacements = self.resistor_replacements(context)?;
        let mutual = self.mutual_terms(context)?;
        let mut system = crate::linear::LinearSystem::new(self.unknown_count());
        for (index, device) in self.devices.iter().enumerate() {
            let device: &dyn Device = match replacements.iter().find(|(i, _)| *i == index) {
                Some((_, resistor)) => resistor,
                None => &**device,
            };
            let range = &self.branch_rows[index];
            device.assemble_small_signal(
                &mut crate::linear::LinearContext {
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
                },
                bias,
            )?;
        }
        system.a.fold_duplicates();
        system.e.fold_duplicates();
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
        instance: &spice_netlist::ast::DeviceInstance,
        models: &crate::models::ModelResolver<'_>,
        context: &crate::models::ModelContext,
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
    /// [`crate::factory::instantiate_with_models`] reports.
    pub fn add_instances(
        &mut self,
        instances: &[spice_netlist::ast::DeviceInstance],
        models: &crate::models::ModelResolver<'_>,
        context: &crate::models::ModelContext,
    ) -> SpiceResult<()> {
        let mut seen: BTreeSet<String> = self
            .devices
            .iter()
            .map(|device| device.name().to_lowercase())
            .collect();
        let mut nodes = self.nodes.clone();
        let mut staged: Vec<Box<dyn Device>> = Vec::with_capacity(instances.len());
        for instance in instances {
            if !seen.insert(instance.name.to_lowercase()) {
                return Err(SpiceError::circuit(format!(
                    "duplicate instance name '{}'",
                    instance.name
                )));
            }
            // Builtin factories bind terminals in this staged table. Nothing
            // fallible remains after committing the two containers together.
            staged.push(crate::factory::instantiate_with_models(
                instance, &mut nodes, models, context,
            )?);
        }
        self.nodes = nodes;
        self.devices.extend(staged);
        Ok(())
    }

    /// Elaborate literal (or top-level `.param`/`{expr}`-literalized) R/C/L/V/I and
    /// bounded model-backed R/C/L at 27 Celsius.
    ///
    /// A deck with `.option` cards is rejected here: this entry point would
    /// silently apply default temperatures. Resolve the options with
    /// `spice_analysis::RunConfig` and elaborate with
    /// [`Self::from_netlist_with_context`] (or `RunConfig::circuit`).
    /// # Errors
    /// Unsupported elaboration constructs, `.option` cards, models or invalid
    /// parameters.
    pub fn from_netlist(netlist: &spice_netlist::ast::Netlist) -> SpiceResult<Self> {
        if !netlist.options.is_empty() {
            return Err(SpiceError::Unsupported {
                feature: ".option cards require RunConfig::from_netlist to resolve them; \
                          Circuit::from_netlist would apply default temperatures"
                    .into(),
                location: netlist.options.first().map(|card| card.location.clone()),
            });
        }
        Self::from_netlist_with_context(netlist, &crate::models::ModelContext::default())
    }

    /// Elaborate with explicit validation temperatures; the immutable model recipe
    /// is retained and later assemblies use their own explicit context.
    /// `.option` cards are the caller's responsibility: the supplied context must
    /// come from resolving them (`spice_analysis::RunConfig`). Top-level
    /// `.global` cards name nodes that stay global through subcircuit expansion
    /// (see [`crate::subckt`]); everything else keeps the deck's names.
    /// # Errors
    /// Invalid context, unsupported constructs/models, invalid parameters or a
    /// failing subcircuit expansion ([`crate::subckt::expand_subcircuits`]).
    /// Nothing is added to the returned circuit; a failed deck yields `Err`.
    pub fn from_netlist_with_context(
        netlist: &spice_netlist::ast::Netlist,
        context: &crate::models::ModelContext,
    ) -> SpiceResult<Self> {
        context.validate(&netlist.location)?;
        if !netlist.includes.is_empty() {
            return Err(SpiceError::Unsupported {
                feature: "includes in linear elaboration".into(),
                location: None,
            });
        }
        // Top-level `.param` values and `{expr}` sites are evaluated into a
        // literal copy before any factory sees them (#15).
        let elaborated = spice_netlist::elaborate::literalize(netlist)?;
        let netlist = &elaborated.netlist;
        // `X` instances are expanded into a fresh device/model list before any
        // device is built, so a deck that fails to elaborate never leaves a
        // partial circuit behind (#18).
        let expanded = crate::subckt::expand_subcircuits(
            netlist,
            &elaborated.scope,
            crate::subckt::SubcircuitLimits::default(),
        )?;
        let models = crate::models::ModelResolver::new(&expanded.models)?;
        let mut circuit = Self::new();
        circuit.add_instances(&expanded.devices, &models, context)?;
        let referenced: BTreeSet<_> = expanded
            .devices
            .iter()
            .filter_map(|instance| {
                instance
                    .model
                    .as_ref()
                    .map(|name| name.to_ascii_lowercase())
            })
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

#[cfg(test)]
mod tests {
    use super::{Circuit, CircuitGraph, CircuitVertex};
    use crate::traits::Device;
    use petgraph::algo::{connected_components, has_path_connecting};
    use petgraph::graph::NodeIndex;
    use petgraph::visit::EdgeRef;
    use spice_core::{NodeId, SpiceError, SpiceResult};

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

        fn stamp(&self, _context: &mut crate::traits::StampContext<'_>) -> SpiceResult<()> {
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
