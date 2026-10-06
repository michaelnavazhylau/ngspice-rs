//! Production petgraph matrix-coupling API regressions.
//!
//! Vertices are MNA rows, not circuit node IDs: ground has been eliminated,
//! and row 0 is an ordinary unknown. Connectivity only describes structural
//! blocks; it proves neither a DC ground path nor numerical nonsingularity.

use petgraph::algo::{connected_components, has_path_connecting};
use spice_core::SpiceError;
use spice_maths::SparseMatrix;

fn stamp_conductance(stamps: &mut SparseMatrix, a: usize, b: usize, g: f64) {
    stamps.add(a, a, g).unwrap();
    stamps.add(a, b, -g).unwrap();
    stamps.add(b, a, -g).unwrap();
    stamps.add(b, b, g).unwrap();
}

#[test]
fn petgraph_separates_independent_matrix_blocks() {
    let mut matrix = SparseMatrix::new(5, 5);
    stamp_conductance(&mut matrix, 1, 2, 1e-3);
    stamp_conductance(&mut matrix, 2, 0, 5e-4);
    stamp_conductance(&mut matrix, 3, 4, 1e-3);
    let graph = matrix.coupling_graph().unwrap();

    assert_eq!(graph.node_count(), matrix.rows());
    assert_eq!(connected_components(&graph), 2);
    assert!(has_path_connecting(&graph, 0, 1, None));
    assert!(has_path_connecting(&graph, 0, 2, None));
    assert!(has_path_connecting(&graph, 3, 4, None));
    assert!(!has_path_connecting(&graph, 0, 3, None));
}

#[test]
fn diagonal_only_and_unstamped_rows_are_kept() {
    let mut matrix = SparseMatrix::new(3, 3);
    matrix.add(0, 0, 2.0).unwrap();
    matrix.add(1, 1, 3.0).unwrap();
    let graph = matrix.coupling_graph().unwrap();
    assert_eq!(graph.nodes().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(graph.edge_count(), 0);
    assert_eq!(connected_components(&graph), 3);
}

#[test]
fn row_zero_is_the_only_unknown_not_an_implicit_ground() {
    let mut matrix = SparseMatrix::new(1, 1);
    matrix.add(0, 0, 2.0).unwrap();
    let graph = matrix.coupling_graph().unwrap();
    assert_eq!(graph.nodes().collect::<Vec<_>>(), vec![0]);
    assert_eq!(graph.edge_count(), 0);
    // A = [2] is nonsingular despite there being no off-diagonal/ground path.
    assert_eq!(connected_components(&graph), 1);
}

#[test]
fn empty_matrix_has_no_invented_ground_vertex() {
    let graph = SparseMatrix::new(0, 0).coupling_graph().unwrap();
    assert_eq!(graph.node_count(), 0);
    assert_eq!(connected_components(&graph), 0);
}

#[test]
fn cancellations_are_folded_without_mutating_the_input() {
    let mut matrix = SparseMatrix::new(3, 3);
    for (row, col, value) in [
        (0, 1, 1.0),
        (0, 1, -1.0),
        (1, 0, 2.0),
        (1, 0, -2.0),
        (1, 2, 3.0),
        (1, 2, -1.0),
    ] {
        matrix.add(row, col, value).unwrap();
    }
    let original = matrix.clone();
    let graph = matrix.coupling_graph().unwrap();
    assert!(!graph.contains_edge(0, 1));
    assert!(graph.contains_edge(1, 2));
    assert_eq!(graph.edge_count(), 1);
    assert_eq!(connected_components(&graph), 2);
    assert_eq!(matrix, original);
    matrix.fold_duplicates();
    let folded = matrix.coupling_graph().unwrap();
    assert_eq!(
        folded.nodes().collect::<Vec<_>>(),
        graph.nodes().collect::<Vec<_>>()
    );
    assert_eq!(
        folded.all_edges().collect::<Vec<_>>(),
        graph.all_edges().collect::<Vec<_>>()
    );
}

#[test]
fn asymmetric_entries_and_opposite_signs_still_couple_rows() {
    let mut matrix = SparseMatrix::new(3, 3);
    matrix.add(0, 1, 1.0).unwrap();
    matrix.add(1, 0, -1.0).unwrap();
    matrix.add(2, 1, 5.0).unwrap();
    let graph = matrix.coupling_graph().unwrap();
    assert_eq!(graph.edge_count(), 2);
    assert!(graph.contains_edge(0, 1));
    assert!(graph.contains_edge(1, 2));
    assert_eq!(connected_components(&graph), 1);
}

#[test]
fn graph_snapshots_follow_new_stamps_and_clear() {
    let mut matrix = SparseMatrix::new(2, 2);
    let before = matrix.coupling_graph().unwrap();
    matrix.add(0, 1, 1.0).unwrap();
    let stamped = matrix.coupling_graph().unwrap();
    matrix.clear();
    let cleared = matrix.coupling_graph().unwrap();
    assert_eq!(before.edge_count(), 0);
    assert_eq!(stamped.edge_count(), 1);
    assert_eq!(cleared.edge_count(), 0);
    assert_eq!(cleared.node_count(), 2);
}

#[test]
fn rectangular_matrices_have_no_shared_row_column_unknown_set() {
    let mut matrix = SparseMatrix::new(2, 3);
    matrix.add(0, 2, 1.0).unwrap();
    let original = matrix.clone();
    let error = matrix.coupling_graph().unwrap_err();
    assert!(matches!(error, SpiceError::Numerical { .. }));
    assert!(error.to_string().contains("square"));
    assert_eq!(matrix, original);
}
