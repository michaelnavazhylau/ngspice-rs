//! The [`Device`] trait and the context devices stamp into.
//!
//! # How stamping works in ngspice
//!
//! Every analysis builds a modified nodal analysis (MNA) system `A x = b` where
//! the unknowns are the node voltages (plus a current for each voltage source and
//! inductor). Each device contributes to `A` and `b` through a `<dev>load()`
//! function — `resload.c`, `capload.c`, `bsim4v7load.c`, … The structure of those
//! functions is uniform: look up the matrix positions of the device's terminals,
//! then add conductances and current sources.
//!
//! [`StampContext`] models exactly that: it owns the matrix and right-hand side
//! being assembled, and offers [`StampContext::stamp`], which resolves
//! [`NodeId`]s to matrix rows and silently drops contributions that involve
//! ground — the row/column for ground is eliminated from the system, which is
//! what `CKTground` accomplishes in the C code.
//!
//! # Trial and accepted state
//!
//! [`Device::stamp`] takes `&self`: a load is a *trial* that may read the
//! accepted history and write only its [`crate::state::DeviceState`] trial
//! slots. Nothing a trial does survives unless the analysis accepts the point
//! through [`crate::Circuit::accept_point`], which first runs every
//! [`Device::accept`] hook and only then commits the trial state. See
//! [`crate::state`].

use std::fmt;
use std::ops::Range;

use spice_core::{Node, NodeId, NodeTable, Real, SpiceError, SpiceResult};
use spice_maths::{Coefficients, SparseMatrix, Vector};

use crate::linear::Forcing;
use crate::state::DeviceState;

/// Which stored quantity a [`StorageElement`] integrates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// Capacitor: charge `q = C v`, `v` across the terminals (first minus second).
    Capacitor,
    /// Inductor: flux `phi = L i`, `i` positive from the first terminal to the second.
    Inductor,
}

/// A charge/flux-storing element and its instance `ic=`, seen by analyses that
/// apply initial conditions. The quantity lives in the state slot
/// [`Device::truncation_slot`]; the derivative in the next slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageElement {
    /// Capacitor or inductor.
    pub kind: StorageKind,
    /// Capacitance in farads or inductance in henries.
    pub value: Real,
    /// The instance `ic=` (volts for a capacitor, amperes for an inductor),
    /// if given.
    pub initial: Option<Real>,
}

/// Which analysis is currently loading the matrix.
///
/// Devices branch on this the way the C `CKTmode` bitmask does: a capacitor
/// stamps an open circuit at DC and a companion model in transient; a diode
/// linearises at DC and adds junction capacitances in AC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalysisMode {
    /// `.op` and the bias point of every other analysis.
    OperatingPoint,
    /// A step of a `.dc` sweep.
    DcSweep,
    /// `.ac` and `.noise`, at a single frequency.
    Ac {
        /// Frequency in Hz.
        frequency: Real,
    },
    /// A step of a `.tran` analysis.
    Transient {
        /// Absolute time in seconds.
        time: Real,
        /// Current timestep in seconds.
        dt: Real,
    },
}

impl AnalysisMode {
    /// True for the AC and noise analyses, where everything is complex and the
    /// system is loaded at `omega = 2 pi f`.
    #[must_use]
    pub const fn is_ac(self) -> bool {
        matches!(self, Self::Ac { .. })
    }

    /// True for transient analysis.
    #[must_use]
    pub const fn is_transient(self) -> bool {
        matches!(self, Self::Transient { .. })
    }

    /// True for the DC analyses, where capacitors are open and inductors are
    /// shorts.
    #[must_use]
    pub const fn is_dc(self) -> bool {
        matches!(self, Self::OperatingPoint | Self::DcSweep)
    }
}

/// Maps circuit unknowns to MNA matrix rows.
///
/// Ground is not an unknown: it is the reference and its row is eliminated.
/// Branch currents are unknowns too, and get rows after every node, in the order
/// the devices were bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MnaUnknowns {
    node_rows: Vec<Option<usize>>,
    next_row: usize,
}

impl MnaUnknowns {
    /// An empty mapping.
    #[must_use]
    pub fn new() -> Self {
        Self {
            node_rows: Vec::new(),
            next_row: 0,
        }
    }

    /// Assigns a row to every unknown node of `nodes`, in node-id order.
    ///
    /// Ground and any node whose kind is not an unknown are left without a row.
    /// The whole mapping is rebuilt, so call this after the circuit's nodes are
    /// all known.
    pub fn rebuild(&mut self, nodes: &NodeTable) {
        self.node_rows.clear();
        self.next_row = 0;
        for node in nodes.nodes() {
            self.push(node);
        }
    }

    fn push(&mut self, node: &Node) -> Option<usize> {
        let row = if node.kind.is_unknown() {
            let row = self.next_row;
            self.next_row += 1;
            Some(row)
        } else {
            None
        };
        debug_assert_eq!(node.id.index(), self.node_rows.len());
        self.node_rows.push(row);
        row
    }

    /// Assigns a row to a node that was added after the last rebuild.
    ///
    /// Returns `None` for ground.
    pub fn assign(&mut self, node: &Node) -> Option<usize> {
        while self.node_rows.len() < node.id.index() {
            self.node_rows.push(None);
        }
        if node.id.index() < self.node_rows.len() {
            return self.node_rows[node.id.index()];
        }
        self.push(node)
    }

    /// The matrix row for a node, or `None` for ground.
    #[must_use]
    pub fn node_row(&self, node: NodeId) -> Option<usize> {
        self.node_rows.get(node.index()).copied().flatten()
    }

    /// Reserves `count` rows after every node row, for branch currents.
    ///
    /// Returns the first reserved row, or `None` when `count` is zero.
    pub fn add_rows(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let first = self.next_row;
        self.next_row += count;
        Some(first)
    }

    /// Number of unknowns, i.e. the size of the MNA system.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.next_row
    }

    /// True when the system has no unknowns, which means the circuit is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.next_row == 0
    }
}

impl Default for MnaUnknowns {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything a device needs in order to stamp itself.
#[derive(Debug)]
pub struct StampContext<'a> {
    /// The MNA matrix being assembled.
    pub matrix: &'a mut SparseMatrix,
    /// The right-hand side being assembled.
    pub rhs: &'a mut Vector,
    /// Where the unknowns live.
    pub unknowns: &'a MnaUnknowns,
    /// The node table, for diagnostics.
    pub nodes: &'a NodeTable,
    /// The present solution, linearised around it for nonlinear devices.
    pub solution: &'a Vector,
    /// Circuit temperature in degrees Celsius; model instances may override it.
    pub temperature: Real,
    /// Default model nominal temperature in degrees Celsius.
    pub nominal_temperature: Real,
    /// Junction minimum conductance (S), [`crate::ModelContext::gmin`].
    pub gmin: Real,
    /// Which analysis is loading the matrix.
    pub mode: AnalysisMode,
    /// Branch-current rows allocated to this device, in order (empty if none).
    pub branches: Range<usize>,
    /// Companion integration coefficients for this trial step; `None` outside
    /// companion transient loads.
    pub integration: Option<&'a Coefficients>,
    /// This device's trial and accepted state slots.
    pub states: DeviceState<'a>,
    /// Source-forcing context of a companion transient load; `None` otherwise.
    pub forcing: Option<Forcing>,
}

impl StampContext<'_> {
    /// The temperatures and junction `gmin` of this load as a [`crate::ModelContext`]
    /// (without resistor overrides, which [`crate::Circuit`] has already applied).
    #[must_use]
    pub const fn model_context(&self) -> crate::ModelContext {
        crate::ModelContext::new(self.temperature, self.nominal_temperature).with_gmin(self.gmin)
    }

    /// The `index`-th branch row of this device.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] when the device has no such branch.
    pub fn branch(&self, index: usize) -> SpiceResult<usize> {
        self.branches
            .clone()
            .nth(index)
            .ok_or_else(|| SpiceError::circuit("missing branch-row binding"))
    }

    /// The present solution value of a matrix row.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Numerical`] for a row outside the solution.
    pub fn row_value(&self, row: usize) -> SpiceResult<Real> {
        self.solution.get(row).ok_or_else(|| SpiceError::Numerical {
            context: "stamp".to_owned(),
            message: format!("solution row {row} out of range"),
        })
    }

    /// Adds `value` to `A[row][col]`, ignoring contributions that involve ground.
    ///
    /// # Errors
    ///
    /// [`spice_core::SpiceError::Numerical`] if a row is out of range, which
    /// would be a bug in the caller rather than bad input.
    pub fn stamp(&mut self, row: NodeId, col: NodeId, value: Real) -> SpiceResult<()> {
        let (Some(row), Some(col)) = (self.unknowns.node_row(row), self.unknowns.node_row(col))
        else {
            return Ok(());
        };
        self.matrix.add(row, col, value)
    }

    /// Adds `value` to `b[row]`, ignoring ground.
    ///
    /// # Errors
    ///
    /// [`spice_core::SpiceError::Numerical`] if the row is out of range.
    pub fn stamp_rhs(&mut self, row: NodeId, value: Real) -> SpiceResult<()> {
        let Some(row) = self.unknowns.node_row(row) else {
            return Ok(());
        };
        self.rhs.add_to(row, value)
    }

    /// The present solution value at a node, or zero at ground.
    #[must_use]
    pub fn node_voltage(&self, node: NodeId) -> Real {
        self.unknowns
            .node_row(node)
            .and_then(|row| self.solution.get(row))
            .unwrap_or(0.0)
    }

    /// The name of a node, for diagnostics.
    #[must_use]
    pub fn node_name(&self, node: NodeId) -> &str {
        self.nodes
            .node(node)
            .map_or("<unknown node>", |node| node.name.as_str())
    }
}

/// A device instance in a circuit.
///
/// The C equivalent is the `CKTdevice` vtable plus its per-device `*load`,
/// `*accept`, `*ask` and `*delete` functions.
///
/// [`fmt::Debug`] is a supertrait so that a `Box<dyn Device>` can be printed in
/// diagnostics and compared in tests.
pub trait Device: fmt::Debug {
    /// The instance name, e.g. `r1`.
    fn name(&self) -> &str;

    /// The designator letter, lowercased.
    fn designator(&self) -> char;

    /// The nodes this device is connected to, in terminal order.
    fn terminals(&self) -> &[NodeId];

    /// How many extra current unknowns the device adds to the MNA system.
    ///
    /// Non-zero for voltage sources and inductors, which contribute a branch
    /// current row; zero for everything else.
    fn branch_currents(&self) -> usize {
        0
    }

    /// True when the device's contribution depends on the present solution, so
    /// the analysis has to iterate.
    fn is_nonlinear(&self) -> bool {
        false
    }

    /// How many state slots (C `CKTnumStates`) the device owns. Slots are
    /// allocated after the branch rows, in device order.
    fn state_count(&self) -> usize {
        0
    }

    /// The state slot holding the integrated charge or flux that takes part in
    /// local-truncation-error control; the *next* slot holds its derivative.
    /// `None` (the default) for devices without a `DEVtrunc` equivalent
    /// (`captrunc.c`, `indtrunc.c`, `CKTterr`).
    fn truncation_slot(&self) -> Option<usize> {
        None
    }

    /// All charge/flux pairs participating in truncation control. Nonlinear
    /// multi-junction devices override this; each listed slot is followed by
    /// its time derivative. Existing single-storage devices retain their API.
    fn truncation_slots(&self) -> Vec<usize> {
        self.truncation_slot().into_iter().collect()
    }

    /// The charge/flux-storage description used to seed initial conditions
    /// (`CAPgetic`/`INDgetic`-style `ic=` handling, see `capload.c`/`indload.c`).
    /// `None` (the default) for devices that store no charge or flux in the
    /// state slots named by [`Self::truncation_slot`].
    fn storage_element(&self) -> Option<StorageElement> {
        None
    }

    /// Loads the device's contribution into the MNA system for one trial.
    ///
    /// Implementations may read accepted history and write their trial state
    /// slots, but cannot change the device: a rejected or repeated trial has
    /// no lasting effect.
    ///
    /// # Errors
    ///
    /// Device-specific failures, and [`spice_core::SpiceError::NotYetPorted`] for
    /// devices that have not been ported.
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()>;

    /// Assembles immutable, state-independent linear equations.
    ///
    /// # Errors
    /// Unsupported devices must not silently contribute zero.
    fn assemble_linear(&self, _context: &mut crate::linear::LinearContext<'_>) -> SpiceResult<()> {
        Err(spice_core::SpiceError::Unsupported {
            feature: format!("linear equation assembly for {}", self.name()),
            location: None,
        })
    }

    /// Assemble the bias-linearized conductance and charge Jacobians for AC.
    /// Linear devices reuse their state-independent operators; nonlinear devices
    /// must explicitly implement this and must not provide a DC equivalent RHS.
    /// # Errors
    /// Invalid bias or unsupported small-signal physics.
    fn assemble_small_signal(
        &self,
        context: &mut crate::linear::LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        self.assemble_linear(context)
    }

    /// Observes an accepted solution point before its state is committed.
    ///
    /// Called by [`crate::Circuit::accept_point`] for the accepted initial
    /// state, accepted adaptive steps and changed event (right-limit) states;
    /// never for Newton trials, rejected steps or interpolated output samples.
    /// Any error aborts the acceptance and nothing is committed. Device history
    /// lives in state slots, so the hook cannot mutate the device.
    ///
    /// # Errors
    ///
    /// Device-specific failures.
    fn accept(&self, _context: &AcceptContext<'_>) -> SpiceResult<()> {
        Ok(())
    }

    /// Physical metadata when this device is a two-terminal resistor a typed
    /// `.dc` sweep may target; `None` (the default) for everything else. This is
    /// the only way a sweep identifies a resistor: never the instance-name prefix.
    fn resistor_metadata(&self) -> Option<crate::sweep::ResistorMetadata> {
        None
    }

    /// The effective resistance in ohms this resistor would stamp if its supplied
    /// scalar were `supplied`, under `context`'s temperatures. Immutable: nothing
    /// about the device changes. See [`crate::sweep`] for the semantics.
    ///
    /// # Errors
    /// Not a resistor, or `supplied` / the derived value is invalid (nonfinite,
    /// zero, nonfinite conductance, nonpositive temperature factor).
    fn resistor_effective(
        &self,
        _supplied: Real,
        _context: &crate::models::ModelContext,
    ) -> SpiceResult<Real> {
        Err(SpiceError::circuit(format!(
            "{} is not a resistor",
            self.name()
        )))
    }
}

/// What [`Device::accept`] sees for one accepted point.
#[derive(Debug, Clone, Copy)]
pub struct AcceptContext<'a> {
    /// The accepted solution.
    pub solution: &'a Vector,
    /// The accepted time, `None` for DC points.
    pub time: Option<Real>,
    /// This device's branch-current rows.
    pub branches: &'a Range<usize>,
    /// This device's slots of the state about to be committed, when the
    /// analysis tracks state.
    pub states: Option<&'a [Real]>,
}

#[cfg(test)]
mod tests {
    use super::{AnalysisMode, MnaUnknowns};
    use spice_core::{NodeId, NodeKind, NodeTable};

    #[test]
    fn ground_gets_no_row() {
        let mut nodes = NodeTable::new();
        let a = nodes.intern("a");
        let b = nodes.intern("b");
        let mut unknowns = MnaUnknowns::new();
        unknowns.rebuild(&nodes);
        assert_eq!(unknowns.node_row(NodeId::GROUND), None);
        assert_eq!(unknowns.node_row(a), Some(0));
        assert_eq!(unknowns.node_row(b), Some(1));
        assert_eq!(unknowns.len(), 2);
    }

    #[test]
    fn internal_nodes_are_unknowns_too() {
        let mut nodes = NodeTable::new();
        let terminal = nodes.intern("out");
        let internal = nodes.intern("x1#1");
        nodes.set_kind(internal, NodeKind::Internal);
        let mut unknowns = MnaUnknowns::new();
        unknowns.rebuild(&nodes);
        assert_eq!(unknowns.node_row(terminal), Some(0));
        assert_eq!(unknowns.node_row(internal), Some(1));
    }

    #[test]
    fn rows_can_be_assigned_incrementally() {
        let mut nodes = NodeTable::new();
        nodes.intern("a");
        let mut unknowns = MnaUnknowns::new();
        unknowns.rebuild(&nodes);
        assert_eq!(unknowns.len(), 1);
        let late = nodes.intern("late");
        let row = unknowns.assign(nodes.node(late).expect("node exists"));
        assert_eq!(row, Some(1));
        assert_eq!(unknowns.len(), 2);
        // Idempotent.
        assert_eq!(
            unknowns.assign(nodes.node(late).expect("node exists")),
            Some(1)
        );
    }

    #[test]
    fn rows_can_be_reserved_for_branch_currents() {
        let mut unknowns = MnaUnknowns::new();
        assert_eq!(unknowns.add_rows(0), None);
        assert_eq!(unknowns.add_rows(2), Some(0));
        assert_eq!(unknowns.add_rows(1), Some(2));
        assert_eq!(unknowns.len(), 3);
    }

    #[test]
    fn unsupported_modes_are_classified() {
        assert!(AnalysisMode::OperatingPoint.is_dc());
        assert!(AnalysisMode::DcSweep.is_dc());
        assert!(AnalysisMode::Ac { frequency: 1e3 }.is_ac());
        assert!(!AnalysisMode::Ac { frequency: 1e3 }.is_dc());
        assert!(
            AnalysisMode::Transient {
                time: 0.0,
                dt: 1e-6
            }
            .is_transient()
        );
    }
}
