//! `.ic`, `.nodeset`, instance `ic=` and `uic` initialization (GitHub #27).
//!
//! C behaviour, all read from the reference sources and re-checked against the
//! reference binary (see `docs/port/TRANSIENT.md` for the table):
//!
//! * `cktsetnp.c`/`inppas3.c` store one `ic` and one `nodeset` per node; every
//!   entry overwrites the previous one, so the **last duplicate wins**. An
//!   unknown node only produces a warning there; the port reports it as an
//!   error ([`resolve`]).
//! * `cktload.c` enforces `.ic` only while `MODETRANOP` and not `MODEUIC`, i.e.
//!   during the initial operating point of an ordinary `.tran`. Each constrained
//!   node row is replaced by `v = ic` (`ZeroNoncurRow`), or, when the row also
//!   holds a branch-current entry (a node on a voltage source or inductor), by a
//!   `1e10` conductance that makes the result inexact. [`constrained_bias`]
//!   performs the exact row replacement for every node and
//!   [`irredundant_constraints`] first resolves the branch-attached cases
//!   structurally, so no `1e10` scaling and no numerical compromise exists.
//! * A node held by an E/H output (directly or through other ideal relations)
//!   keeps the controlled-source relation in C as well (the `1e10`
//!   conductance only loads the ideal output). Such `.ic` entries are not
//!   imposed; [`check_implied`] compares them with the solved bias, and under
//!   `uic` a capacitor across such an output is checked the same way.
//! * `.nodeset` is stamped by `cktload.c` only in the first (`MODEINITJCT`,
//!   `MODEINITFIX`) iterations of a DC iteration and then released; it can only
//!   steer convergence, never change the unique solution of a linear circuit.
//!   Nonlinear solves force it through [`crate::analysis::bias::NodeForcing`] after
//!   [`forced_nodesets`] dropped the rows ideal relations already fix.
//! * Nonlinear `.ic` without `uic` uses the same row replacement in every
//!   Newton load of the transient operating point (C `CKTop` in
//!   `MODETRANOP`), with [`irredundant_constraints`] applied first, and
//!   instance `ic=`/`off` of D/Q/M devices act through their loads
//!   (`crate::devices::limiting`). Under `uic` the one load C performs is the
//!   device-flagged initial load of [`uic_start`]'s vector.
//! * With `uic`, `NIiter()` returns right after one `CKTload` (no solve at all),
//!   `CKTic()` first copies `.nodeset` and then `.ic` values into the node
//!   vector, `CAPgetic()` derives an unset capacitor `ic` from those node values
//!   and `indload.c` takes the inductor current from `ic=` (default 0). Charges
//!   and fluxes therefore come only from the capacitor/inductor initial values;
//!   the first timepoint is solved normally and no `t = 0` row is written.
//!
//! The analyses of this module never touch a circuit: they return values.

use std::collections::BTreeMap;

use crate::devices::{Circuit, ModelContext, StorageKind};
use crate::maths::{SparseMatrix, Vector};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};
use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::unionfind::UnionFind;
use petgraph::visit::{Control, DfsEvent, depth_first_search};

use crate::analysis::{AnalysisRequest, NodeCondition};

fn failure(message: impl Into<String>) -> SpiceError {
    SpiceError::Numerical {
        context: "initial conditions".to_owned(),
        message: message.into(),
    }
}

/// One validated hint on an MNA node row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RowHint {
    pub row: usize,
    pub node: String,
    pub value: Real,
    pub location: SourceLoc,
}

/// The request's `.ic`/`.nodeset` entries bound to matrix rows, one per node
/// (last duplicate wins), ordered by row.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Hints {
    pub initial: Vec<RowHint>,
    pub nodesets: Vec<RowHint>,
}

/// Binds the request's hints to `circuit`.
///
/// # Errors
/// A node that is not part of the circuit (C: warning "IC on non-existent node
/// ... ignored") or that is ground.
pub(crate) fn resolve(circuit: &Circuit, request: &AnalysisRequest) -> SpiceResult<Hints> {
    Ok(Hints {
        initial: bind(circuit, &request.initial_conditions, ".ic")?,
        nodesets: bind(circuit, &request.nodesets, ".nodeset")?,
    })
}

fn bind(circuit: &Circuit, entries: &[NodeCondition], card: &str) -> SpiceResult<Vec<RowHint>> {
    let mut by_row = BTreeMap::new();
    for entry in entries {
        let row = circuit
            .nodes()
            .get(&entry.node)
            .ok_or_else(|| {
                SpiceError::parse(
                    entry.location.clone(),
                    format!(
                        "{card} V({}) names a node that does not exist in the circuit \
                         (ngspice only warns and ignores it; this port rejects it)",
                        entry.node
                    ),
                )
            })
            .and_then(|id| {
                circuit.unknowns().node_row(id).ok_or_else(|| {
                    SpiceError::parse(
                        entry.location.clone(),
                        format!("{card} V({}) names the ground node", entry.node),
                    )
                })
            })?;
        by_row.insert(
            row,
            RowHint {
                row,
                node: entry.node.clone(),
                value: entry.value,
                location: entry.location.clone(),
            },
        );
    }
    Ok(by_row.into_values().collect())
}

/// Relative and absolute voltage tolerances (`reltol`, `vntol`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct VoltageTolerance {
    pub reltol: Real,
    pub vntol: Real,
}

impl VoltageTolerance {
    fn agree(self, a: Real, b: Real) -> bool {
        (a - b).abs() <= self.vntol + self.reltol * a.abs().max(b.abs())
    }
}

/// Result of adding a voltage relation to [`Potentials`].
enum Join {
    /// The relation connected two previously independent groups of nodes.
    Tree,
    /// The nodes were already tied together; the existing structure forces the
    /// difference `expected` (compare with the relation that was added).
    Cycle { expected: Real },
}

/// Node potentials relative to each other from a growing set of voltage
/// relations `v_a - v_b = value`: a petgraph spanning forest (edges carry the
/// signed offset) plus a petgraph `UnionFind` for component membership. A
/// relation between nodes already in one component closes a cycle; the forest
/// is then traversed (depth-first) to find the difference it already forces.
/// Vertex 0 is ground, vertex `row + 1` an MNA node row.
struct Potentials {
    forest: UnGraph<(), (usize, Real), usize>,
    components: UnionFind<usize>,
}

impl Potentials {
    fn new(vertices: usize) -> Self {
        let mut forest = UnGraph::default();
        for _ in 0..vertices {
            forest.add_node(());
        }
        Self {
            forest,
            components: UnionFind::new(vertices),
        }
    }

    /// `potential(b) - potential(a)` through the forest; `a` and `b` must be
    /// in the same component.
    fn difference(&self, a: usize, b: usize) -> Real {
        let mut potential = vec![0.; self.forest.node_count()];
        depth_first_search(&self.forest, [NodeIndex::new(a)], |event| {
            if let DfsEvent::TreeEdge(from, to) = event
                && let Some(edge) = self.forest.find_edge(from, to)
            {
                // Weight (tail, value): potential(tail) - potential(head) = value.
                let &(tail, value) = &self.forest[edge];
                potential[to.index()] = if tail == from.index() {
                    potential[from.index()] - value
                } else {
                    potential[from.index()] + value
                };
            }
            Control::<()>::Continue
        });
        potential[b]
    }

    fn join(&mut self, a: usize, b: usize, value: Real) -> Join {
        if self.components.equiv(a, b) {
            // v_a - v_b as already forced by the forest.
            return Join::Cycle {
                expected: -self.difference(a, b),
            };
        }
        self.components.union(a, b);
        self.forest
            .add_edge(NodeIndex::new(a), NodeIndex::new(b), (a, value));
        Join::Tree
    }
}

/// The ideal voltage sources and inductors (DC shorts) of a circuit as
/// relations `v_p - v_n = value`, with `rhs` giving the source values.
fn source_relations(
    circuit: &Circuit,
    rhs: &Vector,
    inductors_short: bool,
) -> Vec<(usize, usize, Real)> {
    let vertex = |node| circuit.unknowns().node_row(node).map_or(0, |row| row + 1);
    let mut relations = Vec::new();
    for (index, device) in circuit.devices().iter().enumerate() {
        let designator = device.designator();
        let rows = circuit.branch_rows(index).unwrap_or(0..0);
        // An RF port's ideal source sits between its internal `#res` node and
        // the negative terminal; `z0` separates it from the positive one.
        let (positive, negative) = match (device.terminals(), device.rf_port()) {
            ([_, negative, _], Some(port)) => (&port.internal, negative),
            ([positive, negative], None) => (positive, negative),
            _ => continue,
        };
        if rows.len() != 1 {
            continue;
        }
        let value = match designator {
            'v' => rhs.as_slice().get(rows.start).copied().unwrap_or(0.),
            'l' if inductors_short => 0.,
            _ => continue,
        };
        relations.push((vertex(*positive), vertex(*negative), value));
    }
    relations
}

/// The output terminal pairs of the voltage-type controlled sources (E and
/// H) as vertex pairs. Each forces `v_p - v_n` to a value that depends on the
/// rest of the solution, so it is a rigid relation of unknown value.
fn controlled_output_edges(circuit: &Circuit) -> Vec<(usize, usize, String)> {
    let vertex = |node| circuit.unknowns().node_row(node).map_or(0, |row| row + 1);
    circuit
        .devices()
        .iter()
        .filter(|device| matches!(device.designator(), 'e' | 'h'))
        .filter_map(|device| match device.terminals() {
            [positive, negative, ..] => Some((
                vertex(*positive),
                vertex(*negative),
                device.name().to_owned(),
            )),
            _ => None,
        })
        .collect()
}

/// Structural rigidity: which vertices are tied together by ideal voltage
/// relations of any value (independent sources, shorts, E/H outputs, kept
/// constraints), and the E/H names involved.
struct Rigid {
    components: UnionFind<usize>,
    controlled: Vec<String>,
}

impl Rigid {
    fn new(vertices: usize, controlled: &[(usize, usize, String)]) -> Self {
        let mut components = UnionFind::new(vertices);
        for (p, n, _) in controlled {
            components.union(*p, *n);
        }
        Self {
            components,
            controlled: controlled.iter().map(|(_, _, name)| name.clone()).collect(),
        }
    }

    fn names(&self) -> String {
        self.controlled.join(", ")
    }
}

/// The `.ic` constraints of the initial bias, split by how they are handled.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Constraints {
    /// Rows replaced by `v = ic` in [`constrained_bias`].
    pub imposed: Vec<RowHint>,
    /// Nodes already fixed by an E/H output relation (whose value is only
    /// known after the solve): C keeps the controlled source's relation (its
    /// `1e10` conductance just loads the ideal output), so the entry is not
    /// imposed and [`check_implied`] compares it with the solution instead.
    pub implied: Vec<RowHint>,
}

impl Constraints {
    /// No row replacement is needed.
    pub(crate) fn is_unconstrained(&self) -> bool {
        self.imposed.is_empty()
    }
}

/// Checks each implied `.ic` entry against the bias solution `x`.
///
/// # Errors
/// An entry that contradicts the value the controlled sources force.
pub(crate) fn check_implied(
    x: &Vector,
    constraints: &Constraints,
    tolerance: VoltageTolerance,
) -> SpiceResult<()> {
    for hint in &constraints.implied {
        let value = x.as_slice()[hint.row];
        if !tolerance.agree(value, hint.value) {
            return Err(failure(format!(
                ".ic V({})={} at {} contradicts the {value} V that an ideal controlled \
                 source (E/H) output forces on this node, directly or through other ideal \
                 relations; ngspice would return a meaningless 1e10-conductance compromise",
                hint.node, hint.value, hint.location
            )));
        }
    }
    Ok(())
}

/// The `.ic` constraints that actually have to be imposed on the operating
/// point `A x = rhs`, in ascending row order.
///
/// A node whose voltage is already fixed by ideal voltage sources and
/// inductors (shorts at DC) chained to ground, or to another constrained node,
/// is determined by the circuit: the entry is dropped when it agrees with that
/// value within `tolerance` (the source wins) and is an error when it does not.
/// Every other entry is returned unchanged. `rhs` holds the source values at
/// the `t = 0` left limit.
///
/// A node tied to ground (or to a constrained node) through a chain that
/// includes an E/H output is determined by the circuit too, but the value is
/// only known after the solve: such entries are returned as
/// [`Constraints::implied`] and checked by [`check_implied`].
///
/// # Errors
/// An `.ic` value contradicting an ideal source (or two `.ic` values on nodes
/// rigidly tied by sources). C produces a `1e10`-weighted compromise there.
pub(crate) fn irredundant_constraints(
    circuit: &Circuit,
    rhs: &Vector,
    hints: &[RowHint],
    tolerance: VoltageTolerance,
) -> SpiceResult<Constraints> {
    if hints.is_empty() {
        return Ok(Constraints::default());
    }
    let vertices = circuit.unknown_count() + 1;
    let mut potentials = Potentials::new(vertices);
    let mut rigid = Rigid::new(vertices, &controlled_output_edges(circuit));
    // Source loops are the solver's business (a singular system error).
    for (p, n, value) in source_relations(circuit, rhs, true) {
        potentials.join(p, n, value);
        rigid.components.union(p, n);
    }
    let mut kept = Constraints::default();
    for hint in hints {
        let vertex = hint.row + 1;
        if !potentials.components.equiv(vertex, 0) && rigid.components.equiv(vertex, 0) {
            kept.implied.push(hint.clone());
            continue;
        }
        match potentials.join(vertex, 0, hint.value) {
            Join::Tree => {
                rigid.components.union(vertex, 0);
                kept.imposed.push(hint.clone());
            }
            Join::Cycle { expected } if tolerance.agree(expected, hint.value) => {}
            Join::Cycle { expected } => {
                return Err(failure(format!(
                    ".ic V({})={} at {} contradicts the {expected} V that ideal voltage sources \
                     and inductors (DC shorts) force on this node, directly or through other \
                     .ic entries; ngspice would return a meaningless 1e10-conductance \
                     compromise",
                    hint.node, hint.value, hint.location
                )));
            }
        }
    }
    Ok(kept)
}

/// The `.nodeset` rows a nonlinear DC solve forces in its `MODEINITJCT`/
/// `MODEINITFIX` loads ([`crate::analysis::bias::NodeForcing::nodesets`]).
///
/// A nodeset on a node whose voltage is already tied to ground by ideal
/// relations (independent voltage sources, inductors as DC shorts, E/H
/// outputs, the imposed `.ic` rows or an earlier kept nodeset) is dropped: it
/// is only a hint, and the relation wins. C instead loads such a row with a
/// `1e10` conductance (`cktload.c`) and enforces a compromise during those
/// iterations. A nodeset on an `.ic` node is dropped too (C overwrites the
/// row with the `.ic`).
pub(crate) fn forced_nodesets(
    circuit: &Circuit,
    nodesets: &[RowHint],
    imposed: &[RowHint],
) -> Vec<(usize, Real)> {
    let n = circuit.unknown_count();
    let mut rigid = Rigid::new(n + 1, &controlled_output_edges(circuit));
    for (p, q, _) in source_relations(circuit, &Vector::zeros(n), true) {
        rigid.components.union(p, q);
    }
    for hint in imposed {
        rigid.components.union(hint.row + 1, 0);
    }
    let mut kept = Vec::new();
    for hint in nodesets {
        if !rigid.components.equiv(hint.row + 1, 0) {
            rigid.components.union(hint.row + 1, 0);
            kept.push((hint.row, hint.value));
        }
    }
    kept
}

/// Solves `A x = rhs` with every node row in `constraints` replaced by
/// `x[row] = value` (the exact form of `cktload.c`'s `ZeroNoncurRow` branch).
/// The replaced row's KCL residual is the current the hidden constraint
/// supplies, so capacitor-only (floating at DC) nodes become solvable.
///
/// # Errors
/// A singular constrained system.
pub(crate) fn constrained_bias(
    a: &SparseMatrix,
    rhs: &Vector,
    constraints: &[RowHint],
) -> SpiceResult<Vector> {
    let n = a.rows();
    let mut matrix = SparseMatrix::new(n, n);
    let fixed: BTreeMap<usize, Real> = constraints.iter().map(|h| (h.row, h.value)).collect();
    for triplet in a.triplets() {
        if !fixed.contains_key(&triplet.row) {
            matrix.add(triplet.row, triplet.col, triplet.value)?;
        }
    }
    let mut b = rhs.clone();
    for (row, value) in &fixed {
        matrix.add(*row, *row, 1.)?;
        b.as_mut_slice()[*row] = *value;
    }
    matrix.fold_duplicates();
    matrix
        .solve(&b)
        .map_err(|error| failure(format!("operating point with .ic constraints: {error}")))
}

/// The `uic` starting point: node voltages for the first load and the charge
/// slots whose value comes from an instance `ic=`.
#[derive(Debug)]
pub(crate) struct UicStart {
    /// Node rows from `.nodeset` then `.ic` (zero elsewhere); inductor branch
    /// rows hold their initial current.
    pub x: Vector,
    /// `(absolute state slot, charge)` for capacitors with an instance `ic=`.
    pub charges: Vec<(usize, Real)>,
}

/// Builds the `uic` start exactly as `CKTic()`/`CAPgetic()`/`indload.c` do.
///
/// # Errors
/// Missing state or branch bindings.
pub(crate) fn uic_start(
    circuit: &Circuit,
    hints: &Hints,
    context: &ModelContext,
) -> SpiceResult<UicStart> {
    let mut x = Vector::zeros(circuit.unknown_count());
    for hint in hints.nodesets.iter().chain(&hints.initial) {
        x.as_mut_slice()[hint.row] = hint.value;
    }
    let mut charges = Vec::new();
    for (index, device) in circuit.devices().iter().enumerate() {
        let (Some(element), Some(slot)) =
            (device.storage_element(context), device.truncation_slot())
        else {
            continue;
        };
        let element = element?;
        match element.kind {
            StorageKind::Capacitor => {
                if let Some(voltage) = element.initial {
                    let base = circuit
                        .state_rows(index)
                        .ok_or_else(|| failure("missing state range"))?
                        .start;
                    charges.push((base + slot, element.value * voltage));
                }
            }
            StorageKind::Inductor => {
                let row = circuit
                    .branch_rows(index)
                    .filter(|rows| rows.len() == 1)
                    .ok_or_else(|| {
                        failure(format!("inductor {} has no branch row", device.name()))
                    })?
                    .start;
                x.as_mut_slice()[row] = element.initial.unwrap_or(0.);
            }
        }
    }
    Ok(UicStart { x, charges })
}

/// Tolerances for the `uic` consistency check.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tolerances {
    pub reltol: Real,
    pub vntol: Real,
    pub abstol: Real,
}

/// Checks that the `uic` state can start without an impulse at `t = 0+`.
///
/// C does not look: it integrates a first step from whatever charges it was
/// given, so a capacitor whose `ic` differs from an ideal source across it
/// just produces a huge first-step current. Here the instantaneous (`h -> 0`)
/// problem is formed exactly: every capacitor becomes a voltage constraint with
/// a free current, every inductor a fixed current (its voltage is free), every
/// other element and the sources at their `t = 0` right limit (`rhs`) stay as
/// they are. A state is impulse-free exactly when that system has a solution.
///
/// Redundant-but-consistent relations (a capacitor whose `ic` equals the
/// voltage that ideal sources already force, an inductor in series with a
/// current source carrying its current) are recognised structurally and
/// dropped; a contradicting one is an error naming the element. Whatever else
/// leaves the system singular or non-square is reported as ill-posed.
///
/// # Errors
/// An inconsistent (impulsive) or ill-posed initial state.
pub(crate) fn check_impulse_free(
    circuit: &Circuit,
    a: &SparseMatrix,
    rhs: &Vector,
    x: &Vector,
    tolerances: Tolerances,
    context: &ModelContext,
) -> SpiceResult<()> {
    let n = circuit.unknown_count();
    let node_row = |node| circuit.unknowns().node_row(node);
    let vertex = |node| node_row(node).map_or(0, |row| row + 1);
    // Fixed inductor currents (their rows and columns leave the system).
    let mut fixed: BTreeMap<usize, Real> = BTreeMap::new();
    let mut inductors = Vec::new();
    let mut capacitors = Vec::new();
    for (index, device) in circuit.devices().iter().enumerate() {
        let Some(element) = device.storage_element(context).transpose()? else {
            continue;
        };
        let [p, q] = device.terminals() else {
            continue;
        };
        match element.kind {
            StorageKind::Inductor => {
                if let Some(row) = circuit.branch_rows(index).filter(|r| r.len() == 1) {
                    fixed.insert(row.start, x.as_slice()[row.start]);
                    inductors.push(device.name().to_owned());
                }
            }
            StorageKind::Capacitor => {
                let voltage = element.initial.unwrap_or_else(|| {
                    let at = |node| node_row(node).map_or(0., |row| x.as_slice()[row]);
                    at(*p) - at(*q)
                });
                capacitors.push((device.name().to_owned(), vertex(*p), vertex(*q), voltage));
            }
        }
    }
    if capacitors.is_empty() && fixed.is_empty() {
        return Ok(());
    }
    // Voltage-type relations: ideal sources first, then the capacitors.
    let mut potentials = Potentials::new(n + 1);
    let mut rigid = Rigid::new(n + 1, &controlled_output_edges(circuit));
    for (p, q, value) in source_relations(circuit, rhs, false) {
        potentials.join(p, q, value);
        rigid.components.union(p, q);
    }
    let voltage_tolerance = VoltageTolerance {
        reltol: tolerances.reltol,
        vntol: tolerances.vntol,
    };
    let mut kept = Vec::new();
    // Capacitors in a loop closed by an E/H output: the controlled source
    // fixes their voltage from the rest of the solution, so they leave the
    // system and are compared with the solved voltages afterwards.
    let mut implied = Vec::new();
    for (name, p, q, voltage) in &capacitors {
        if !potentials.components.equiv(*p, *q) && rigid.components.equiv(*p, *q) {
            implied.push((name, *p, *q, *voltage));
            continue;
        }
        match potentials.join(*p, *q, *voltage) {
            Join::Tree => {
                rigid.components.union(*p, *q);
                kept.push((*p, *q, *voltage));
            }
            Join::Cycle { expected } if voltage_tolerance.agree(expected, *voltage) => {}
            Join::Cycle { expected } => {
                return Err(failure(format!(
                    "uic initial conditions are inconsistent with the circuit at t = 0+: \
                     capacitor {name} starts at {voltage} V but ideal sources and other \
                     initial conditions force {expected} V across it, an impulse; make the \
                     initial condition agree or add series impedance"
                )));
            }
        }
    }
    // Reduced system: unknowns are the node/branch rows not fixed above, plus
    // one free current per kept capacitor; fixed currents move to the RHS.
    let mut column_of = vec![usize::MAX; n];
    let mut kept_rows = Vec::new();
    for row in (0..n).filter(|row| !fixed.contains_key(row)) {
        column_of[row] = kept_rows.len();
        kept_rows.push(row);
    }
    let size = kept_rows.len() + kept.len();
    let mut entries: Vec<(usize, usize, Real)> = Vec::new();
    let mut b = vec![0.; size];
    let mut scale = vec![0.; size];
    for &row in &kept_rows {
        b[column_of[row]] = rhs.as_slice()[row];
        scale[column_of[row]] = rhs.as_slice()[row].abs();
    }
    for t in a.triplets() {
        if fixed.contains_key(&t.row) {
            continue;
        }
        let r = column_of[t.row];
        if let Some(current) = fixed.get(&t.col) {
            b[r] -= t.value * current;
            scale[r] += (t.value * current).abs();
        } else {
            entries.push((r, column_of[t.col], t.value));
        }
    }
    for (k, (p, q, voltage)) in kept.iter().enumerate() {
        let current = kept_rows.len() + k;
        for (vertex, sign) in [(*p, 1.), (*q, -1.)] {
            if vertex > 0 {
                entries.push((column_of[vertex - 1], current, sign));
                entries.push((current, column_of[vertex - 1], sign));
            }
        }
        b[current] = *voltage;
        scale[current] = voltage.abs();
    }
    // Rows without any unknown must read 0 = 0; columns that appear nowhere
    // (a node touched only by current sources and inductors) are undetermined
    // by the instantaneous problem and leave it.
    let mut row_used = vec![false; size];
    let mut column_used = vec![false; size];
    for (r, c, v) in &entries {
        if *v != 0. {
            row_used[*r] = true;
            column_used[*c] = true;
        }
    }
    let is_branch: Vec<bool> = {
        let mut flags = vec![false; n];
        for index in 0..circuit.device_count() {
            for row in circuit.branch_rows(index).unwrap_or(0..0) {
                flags[row] = true;
            }
        }
        flags
    };
    for r in (0..size).filter(|r| !row_used[*r]) {
        let (absolute, what) = match kept_rows.get(r) {
            Some(row) if !is_branch[*row] => (tolerances.abstol, "currents into a node"),
            _ => (tolerances.vntol, "a voltage relation"),
        };
        if b[r].abs() > absolute + tolerances.reltol * scale[r] {
            let who = if inductors.is_empty() {
                String::new()
            } else {
                format!(" (inductors: {})", inductors.join(", "))
            };
            return Err(failure(format!(
                "uic initial conditions are inconsistent with the circuit at t = 0+: {what} \
                 left over by fixed inductor currents and sources is {:e} instead of 0, an \
                 impulse{who}",
                b[r]
            )));
        }
    }
    let rows: Vec<usize> = (0..size).filter(|r| row_used[*r]).collect();
    let columns: Vec<usize> = (0..size).filter(|c| column_used[*c]).collect();
    if rows.len() != columns.len() {
        return Err(failure(format!(
            "uic initial conditions cannot be shown consistent: the instantaneous system has \
             {} equations for {} unknowns (over- or under-determined by ideal sources, \
             capacitors and inductors)",
            rows.len(),
            columns.len()
        )));
    }
    let mut row_index = vec![usize::MAX; size];
    let mut column_index = vec![usize::MAX; size];
    for (k, r) in rows.iter().enumerate() {
        row_index[*r] = k;
    }
    for (k, c) in columns.iter().enumerate() {
        column_index[*c] = k;
    }
    if rows.is_empty() {
        return if implied.is_empty() {
            Ok(())
        } else {
            Err(failure(format!(
                "uic initial conditions cannot be shown consistent: capacitors across the \
                 output of controlled sources {} are left without an instantaneous system",
                rigid.names()
            )))
        };
    }
    let mut matrix = SparseMatrix::new(rows.len(), rows.len());
    for (r, c, v) in entries {
        if row_used[r] && column_used[c] {
            matrix.add(row_index[r], column_index[c], v)?;
        }
    }
    matrix.fold_duplicates();
    let rhs = Vector::from_slice(&rows.iter().map(|r| b[*r]).collect::<Vec<_>>());
    let solution = matrix.solve(&rhs).map_err(|error| {
        failure(format!(
            "uic initial conditions are over-determined or contradictory at t = 0+ (the \
             instantaneous constraint system is singular): {error}"
        ))
    })?;
    // Node voltages of the instantaneous solution, by vertex (0 is ground).
    let potential = |vertex: usize| -> Option<Real> {
        if vertex == 0 {
            return Some(0.);
        }
        let reduced = column_of[vertex - 1];
        (reduced != usize::MAX && column_used[reduced])
            .then(|| solution.as_slice()[column_index[reduced]])
    };
    for (name, p, q, voltage) in implied {
        let (Some(vp), Some(vq)) = (potential(p), potential(q)) else {
            return Err(failure(format!(
                "uic initial conditions cannot be shown consistent: capacitor {name} is \
                 across the output of controlled sources ({}) but its voltage is not \
                 determined by the instantaneous system",
                rigid.names()
            )));
        };
        let expected = vp - vq;
        if !voltage_tolerance.agree(expected, voltage) {
            return Err(failure(format!(
                "uic initial conditions are inconsistent with the circuit at t = 0+: \
                 capacitor {name} starts at {voltage} V but ideal controlled sources ({}) \
                 force {expected} V across it, an impulse; make the initial condition agree \
                 or add series impedance",
                rigid.names()
            )));
        }
    }
    Ok(())
}
