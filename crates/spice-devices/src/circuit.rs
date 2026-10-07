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
use crate::state::{StateHistory, TrialState};
use crate::traits::{AcceptContext, AnalysisMode, Device, MnaUnknowns, StampContext};

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
    state_rows: Vec<std::ops::Range<usize>>,
    state_len: usize,
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
    }

    /// Validates the graph and renumbers the unknowns.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] if a device refers to a node that is not in the
    /// table, or if two devices share an instance name — ngspice rejects the
    /// latter too, in `INP2dot`/`CKTcrtElt`.
    pub fn finalize(&mut self) -> SpiceResult<()> {
        self.topology()?;
        self.rebuild_unknowns();
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
        {
            return Err(SpiceError::circuit(
                "circuit numbering is stale; finalize after adding devices",
            ));
        }
        Ok(())
    }

    /// Loads every device for one trial into `matrix`, `rhs` and `trial`.
    ///
    /// The accepted history is read-only here. On error, `matrix`, `rhs` and
    /// `trial` hold a partial load and must be discarded; nothing the circuit
    /// or history owns has changed.
    ///
    /// # Errors
    ///
    /// Stale numbering, mismatched dimensions/nonfinite solution, or device
    /// failures.
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
        for (index, device) in self.devices.iter().enumerate() {
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
                mode: request.mode,
                branches: self.branch_rows[index].clone(),
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
        let mut system = crate::linear::LinearSystem::new(self.unknown_count());
        for (index, device) in self.devices.iter().enumerate() {
            let range = &self.branch_rows[index];
            device.assemble_linear(&mut crate::linear::LinearContext {
                model_context: context,
                system: &mut system,
                unknowns: &self.unknowns,
                branch: (!range.is_empty()).then_some(range.start),
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
        self.check_numbering()?;
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
        let mut system = crate::linear::LinearSystem::new(self.unknown_count());
        for (index, device) in self.devices.iter().enumerate() {
            let range = &self.branch_rows[index];
            device.assemble_small_signal(
                &mut crate::linear::LinearContext {
                    model_context: context,
                    system: &mut system,
                    unknowns: &self.unknowns,
                    branch: (!range.is_empty()).then_some(range.start),
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
        if self.device(&instance.name).is_some() {
            return Err(SpiceError::circuit(format!(
                "duplicate instance name '{}'",
                instance.name
            )));
        }
        let mut nodes = self.nodes.clone();
        let device =
            crate::factory::instantiate_with_models(instance, &mut nodes, models, context)?;
        // Builtin factories bind terminals in this staged table. Nothing
        // fallible remains after committing the two containers together.
        self.nodes = nodes;
        self.devices.push(device);
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
    /// `.global` cards only name top-level nodes, which are already global in a
    /// flat circuit, so they need no elaboration.
    /// # Errors
    /// Invalid context, unsupported constructs/models or invalid parameters.
    pub fn from_netlist_with_context(
        netlist: &spice_netlist::ast::Netlist,
        context: &crate::models::ModelContext,
    ) -> SpiceResult<Self> {
        context.validate(&netlist.location)?;
        if !netlist.subcircuits.is_empty() || !netlist.includes.is_empty() {
            return Err(SpiceError::Unsupported {
                feature: "subcircuits/includes in linear elaboration".into(),
                location: None,
            });
        }
        // Top-level `.param` values and `{expr}` sites are evaluated into a
        // literal copy before any factory sees them (#15).
        let elaborated = spice_netlist::elaborate::literalize(netlist)?;
        let netlist = &elaborated.netlist;
        let models = crate::models::ModelResolver::new(&netlist.models)?;
        let mut circuit = Self::new();
        let referenced: BTreeSet<_> = netlist
            .devices
            .iter()
            .filter_map(|instance| {
                instance
                    .model
                    .as_ref()
                    .map(|name| name.to_ascii_lowercase())
            })
            .collect();
        for instance in &netlist.devices {
            circuit.add_instance(instance, &models, context)?;
        }
        if let Some(model) = netlist
            .models
            .iter()
            .find(|model| !referenced.contains(&model.name.to_ascii_lowercase()))
        {
            // First-declaration duplicate policy remains explicit; unused
            // declarations cannot silently discard unsupported physics.
            return Err(SpiceError::Unsupported {
                feature: "unused model declarations in scalar linear elaboration".into(),
                location: Some(model.location.clone()),
            });
        }
        circuit.finalize()?;
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
