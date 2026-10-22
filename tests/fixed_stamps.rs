//! Fixed stamp batches (M10 0.5e): once a load has verified a MOSFET's
//! element list against the recorded stamp sequence, later loads add its
//! values by element (`SparseMatrix::add_fixed`, C's `TSTALLOC` pointers).
//! Every load must assemble exactly the matrix an unbound (triplet) load
//! assembles, and the verification must not outlive a change of devices.
use std::path::Path;

use ngspice_rs::devices::{AnalysisMode, Circuit, IterationPhase, LoadRequest, ModelContext};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;

const CHAIN: &str = "vdd vdd 0 5\nvin n0 0 2\n\
    mn1 n1 n0 0 0 nm w=10u l=1u\nmp1 n1 n0 vdd vdd pm w=20u l=1u\n\
    mn2 n2 n1 0 0 nm w=10u l=1u\nmp2 n2 n1 vdd vdd pm w=20u l=1u\n\
    c1 n1 0 10f\n\
    .model nm nmos(level=1 vto=1 kp=5e-5 rd=10 rs=10)\n\
    .model pm pmos(level=1 vto=-1 kp=2.5e-5)";

fn circuit() -> Circuit {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("fixed.cir"),
            &format!("fixed stamps\n{CHAIN}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&netlist).unwrap()
}

/// An iterate with every MOSFET conducting at a different bias (`mn1`
/// saturated, `mn2` linear), so no two devices stamp the same values.
fn bias(circuit: &Circuit) -> Vector {
    let mut x = Vector::from_slice(
        &(0..circuit.unknown_count())
            .map(|row| 0.01 * row as f64)
            .collect::<Vec<_>>(),
    );
    for (name, volts) in [("vdd", 5.), ("n0", 2.), ("n1", 3.5), ("n2", 1.)] {
        let node = circuit.nodes().get(name).unwrap();
        let row = circuit.unknowns().node_row(node).unwrap();
        x.set(row, volts).unwrap();
    }
    x
}

/// Loads `circuit` at `x` into `matrix` and returns every entry's bits.
fn load(circuit: &Circuit, mut matrix: SparseMatrix, x: &Vector) -> Vec<u64> {
    let n = circuit.unknown_count();
    let history = circuit.state_history();
    // MODEINITFLOAT: devices load at `x` (MODEINITJCT would give every
    // MOSFET the same starting voltages).
    let mut trial = history.trial_in(IterationPhase::Float, None).unwrap();
    circuit
        .load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: x,
                model_context: &ModelContext::default(),
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut matrix,
            &mut Vector::zeros(n),
            &mut trial,
        )
        .unwrap();
    (0..n)
        .flat_map(|row| (0..n).map(move |col| (row, col)))
        .map(|(row, col)| matrix.get(row, col).to_bits())
        .collect()
}

/// Checks several bound loads (recording, verifying, then by element)
/// against unbound loads, which never add by element.
fn assert_bound_loads_match(circuit: &Circuit, x: &Vector) {
    let n = circuit.unknown_count();
    let reference = load(circuit, SparseMatrix::new(n, n), x);
    assert!(reference.iter().any(|&bits| bits != 0));
    for pass in 0..4 {
        assert_eq!(
            load(circuit, circuit.new_matrix(), x),
            reference,
            "bound load {pass}"
        );
    }
}

#[test]
fn loads_by_element_assemble_the_unbound_matrix() {
    let circuit = circuit();
    let x = bias(&circuit);
    assert_bound_loads_match(&circuit, &x);
}

#[test]
fn replacing_devices_forgets_the_verified_batches() {
    let mut circuit = circuit();
    let x = bias(&circuit);
    assert_bound_loads_match(&circuit, &x);
    // Same element set and numbering, but each load position now holds the
    // other n-channel device, which stamps other elements.
    let position = |name: &str| {
        circuit
            .devices()
            .iter()
            .position(|device| device.name() == name)
            .unwrap()
    };
    let (first, second) = (position("mn1"), position("mn2"));
    circuit.devices_mut().swap(first, second);
    assert_bound_loads_match(&circuit, &x);
}
