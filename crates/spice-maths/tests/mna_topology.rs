//! Proves `petgraph` is wired into `spice-maths` by using it on the structure
//! of the MNA matrix.
//!
//! Index 0 stands for ground, the way `spice-core`'s `NodeTable` aliases `gnd`
//! to 0. Two unknowns are coupled when a device stamps an off-diagonal entry,
//! so those off-diagonals are exactly the edges of the coupling graph. A
//! component with no path to ground has no DC solution; those are the blocks
//! `spice-analysis` has to reject before the factorisation is handed a singular
//! matrix.

use petgraph::algo::{connected_components, has_path_connecting};
use petgraph::graphmap::UnGraphMap;
use spice_maths::SparseMatrix;

/// Stamps a conductance `g` between two nodes the way a resistor does: `+g` on
/// both diagonals and `-g` on both off-diagonals.
fn stamp_conductance(stamps: &mut SparseMatrix, a: usize, b: usize, g: f64) {
    stamps.add(a, a, g).unwrap();
    stamps.add(a, b, -g).unwrap();
    stamps.add(b, a, -g).unwrap();
    stamps.add(b, b, g).unwrap();
}

/// Five unknowns: ground (0), a grounded 1 kΩ/2 kΩ divider on nodes 1 and 2,
/// and a floating 1 kΩ island on nodes 3 and 4.
fn fixture() -> SparseMatrix {
    let mut stamps = SparseMatrix::new(5, 5);
    stamp_conductance(&mut stamps, 1, 2, 1e-3); // R1 = 1 kΩ
    stamp_conductance(&mut stamps, 2, 0, 5e-4); // R2 = 2 kΩ to ground
    stamp_conductance(&mut stamps, 3, 4, 1e-3); // R3 = 1 kΩ, no path to ground
    stamps.fold_duplicates();
    stamps
}

/// The coupling graph, with every unknown as a node even when it only carries
/// diagonal entries.
fn coupling_graph(stamps: &SparseMatrix) -> UnGraphMap<usize, ()> {
    let mut graph = UnGraphMap::<usize, ()>::new();
    for index in 0..5 {
        graph.add_node(index);
    }
    for stamp in stamps.triplets() {
        if stamp.row != stamp.col {
            graph.add_edge(stamp.row, stamp.col, ());
        }
    }
    graph
}

#[test]
fn petgraph_separates_the_grounded_block_from_the_floating_island() {
    let graph = coupling_graph(&fixture());

    assert_eq!(
        connected_components(&graph),
        2,
        "the divider and the floating island are electrically separate blocks"
    );

    // The divider reaches ground; the island does not.
    assert!(has_path_connecting(&graph, 0, 1, None));
    assert!(has_path_connecting(&graph, 0, 2, None));
    assert!(has_path_connecting(&graph, 3, 4, None));
    assert!(
        !has_path_connecting(&graph, 0, 3, None),
        "nodes 3 and 4 have no path to ground, so that block is singular"
    );
}
