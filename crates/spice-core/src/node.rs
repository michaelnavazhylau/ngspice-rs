//! Node identity, and the `gnd` → `0` rule.
//!
//! Ported from:
//!
//! - `src/frontend/inpcom.c`, `inp_fix_gnd_name()` — rewrites the node name
//!   `gnd` to `0`, unless the `no_auto_gnd` variable is set, in which case
//!   `gnd` stays an ordinary node. ngspice also inserts `.global gnd` so the
//!   name survives subcircuit flattening; the port models the aliasing here and
//!   leaves the `.global` insertion to the parser.
//! - `src/spicelib/devices/cktbindnode.c` — node creation and the node index
//!   space.

use std::collections::BTreeMap;
use std::fmt;

/// Canonical name of the ground node.
pub const GROUND_NAME: &str = "0";

/// The node name ngspice rewrites to [`GROUND_NAME`] unless `no_auto_gnd` is set.
pub const GROUND_ALIAS: &str = "gnd";

/// A node's index in the circuit's node table.
///
/// Ids are dense and stable: id 0 is always ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// Ground, which is always node 0.
    pub const GROUND: Self = Self(0);

    /// Builds an id from a raw index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The raw index, for use as a row/column in the MNA matrix.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.0)
    }
}

/// What a node is used for, which decides whether it adds an unknown to the
/// MNA system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Ground. Never contributes an unknown.
    Ground,
    /// An externally visible terminal of the circuit.
    Terminal,
    /// A node introduced by the simulator, for instance inside a device.
    Internal,
    /// The branch-current unknown of a voltage source or inductor, named
    /// `<instance>#branch`.
    BranchCurrent,
}

impl NodeKind {
    /// Whether this node contributes a row/column to the MNA matrix.
    #[must_use]
    pub const fn is_unknown(self) -> bool {
        !matches!(self, Self::Ground)
    }
}

/// One node: its canonical name, its id, and what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Index in the owning [`NodeTable`].
    pub id: NodeId,
    /// Canonical (lowercased) name as first written.
    pub name: String,
    /// What the node is used for.
    pub kind: NodeKind,
}

/// Maps node names to ids, case-insensitively.
///
/// SPICE node names are case-insensitive, so `In`, `IN` and `in` are one node.
/// The first spelling seen is kept for display and for rawfile output.
#[derive(Debug, Clone)]
pub struct NodeTable {
    nodes: Vec<Node>,
    index: BTreeMap<String, NodeId>,
    auto_gnd: bool,
}

impl NodeTable {
    /// Creates a table containing only ground, with `gnd` aliasing enabled.
    #[must_use]
    pub fn new() -> Self {
        Self::with_auto_gnd(true)
    }

    /// Creates a table containing only ground.
    ///
    /// With `auto_gnd` disabled, `gnd` is an ordinary node name, matching
    /// ngspice when `no_auto_gnd` is set.
    #[must_use]
    pub fn with_auto_gnd(auto_gnd: bool) -> Self {
        let ground = Node {
            id: NodeId::GROUND,
            name: GROUND_NAME.to_owned(),
            kind: NodeKind::Ground,
        };
        let mut index = BTreeMap::new();
        index.insert(GROUND_NAME.to_owned(), NodeId::GROUND);
        Self {
            nodes: vec![ground],
            index,
            auto_gnd,
        }
    }

    /// Whether `gnd` is aliased to ground.
    #[must_use]
    pub fn auto_gnd(&self) -> bool {
        self.auto_gnd
    }

    /// The canonical spelling of a node name: lowercased, with `gnd` folded into
    /// `0` when aliasing is on.
    #[must_use]
    pub fn canonical_name(name: &str, auto_gnd: bool) -> String {
        let lowered = name.to_lowercase();
        if auto_gnd && lowered == GROUND_ALIAS {
            GROUND_NAME.to_owned()
        } else {
            lowered
        }
    }

    /// Returns the id of `name`, creating the node if needed.
    ///
    /// The node starts out as a [`NodeKind::Terminal`]; use
    /// [`NodeTable::set_kind`] to mark simulator-internal nodes.
    pub fn intern(&mut self, name: &str) -> NodeId {
        let canonical = Self::canonical_name(name, self.auto_gnd);
        if let Some(id) = self.index.get(&canonical) {
            return *id;
        }
        let id = NodeId::new(u32::try_from(self.nodes.len()).expect("node table overflow"));
        let kind = if canonical == GROUND_NAME {
            NodeKind::Ground
        } else {
            NodeKind::Terminal
        };
        self.nodes.push(Node {
            id,
            name: canonical.clone(),
            kind,
        });
        self.index.insert(canonical, id);
        id
    }

    /// Looks up a node without creating it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<NodeId> {
        let canonical = Self::canonical_name(name, self.auto_gnd);
        self.index.get(&canonical).copied()
    }

    /// The node with this id.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// Every node, ground first.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Number of nodes, including ground.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the table holds nothing but ground.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    /// Number of unknowns the MNA matrix needs for these nodes.
    #[must_use]
    pub fn unknown_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.kind.is_unknown())
            .count()
    }

    /// Changes what a node is used for.
    pub fn set_kind(&mut self, id: NodeId, kind: NodeKind) {
        if let Some(node) = self.nodes.get_mut(id.index()) {
            node.kind = kind;
        }
    }

    /// Ground.
    #[must_use]
    pub fn ground(&self) -> NodeId {
        NodeId::GROUND
    }
}

impl Default for NodeTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{GROUND_ALIAS, GROUND_NAME, NodeId, NodeKind, NodeTable};

    #[test]
    fn ground_is_node_zero() {
        let table = NodeTable::new();
        assert_eq!(table.len(), 1);
        assert!(table.is_empty());
        assert_eq!(table.ground(), NodeId::GROUND);
        assert_eq!(table.node(NodeId::GROUND).unwrap().kind, NodeKind::Ground);
        assert_eq!(table.unknown_count(), 0);
    }

    #[test]
    fn names_are_case_insensitive_and_deduplicated() {
        let mut table = NodeTable::new();
        let first = table.intern("Out");
        assert_eq!(table.intern("OUT"), first);
        assert_eq!(table.intern("out"), first);
        assert_eq!(table.len(), 2);
        assert_eq!(table.node(first).unwrap().name, "out");
    }

    #[test]
    fn gnd_is_aliased_to_ground_by_default() {
        let mut table = NodeTable::new();
        assert_eq!(table.intern(GROUND_ALIAS), NodeId::GROUND);
        assert_eq!(table.intern("GND"), NodeId::GROUND);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn gnd_is_a_node_when_auto_gnd_is_off() {
        let mut table = NodeTable::with_auto_gnd(false);
        let gnd = table.intern("gnd");
        assert_ne!(gnd, NodeId::GROUND);
        assert_eq!(table.len(), 2);
        assert_eq!(table.unknown_count(), 1);
    }

    #[test]
    fn canonical_name_only_folds_the_exact_alias() {
        assert_eq!(NodeTable::canonical_name("GND", true), GROUND_NAME);
        assert_eq!(NodeTable::canonical_name("gnd1", true), "gnd1");
        assert_eq!(NodeTable::canonical_name("gnd", false), "gnd");
    }

    #[test]
    fn unknown_count_excludes_ground_and_internal_nodes() {
        let mut table = NodeTable::new();
        let terminal = table.intern("1");
        let internal = table.intern("x1#internal");
        table.set_kind(internal, NodeKind::Internal);
        assert_eq!(table.unknown_count(), 2);
        table.set_kind(terminal, NodeKind::Terminal);
        assert_eq!(table.node(internal).unwrap().kind, NodeKind::Internal);
    }
}
