//! Trial-versus-accepted state ownership through the production circuit API.
use std::cell::RefCell;
use std::rc::Rc;

use ngspice_rs::devices::{
    AcceptContext, AnalysisMode, Circuit, Device, LoadRequest, ModelContext, Resistor,
    StampContext, StateHistory,
};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::primitives::{NodeId, SpiceError, SpiceResult};

type Log = Rc<RefCell<Vec<(Option<f64>, Option<Vec<f64>>)>>>;

/// Integrates its node voltage into one state slot: `s0 = s1 + v`. Fails to
/// evaluate at `v = 13` and refuses acceptance at `t = 99`.
#[derive(Debug)]
struct Accumulator {
    name: String,
    terminals: [NodeId; 2],
    branches: usize,
    log: Log,
}

impl Device for Accumulator {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn branch_currents(&self) -> usize {
        self.branches
    }
    fn state_count(&self) -> usize {
        1
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let v = context.node_voltage(self.terminals[0]);
        if v == 13.0 {
            return Err(SpiceError::circuit("evaluation failed"));
        }
        let previous = context.states.accepted(1, 0).unwrap_or(0.0);
        context.states.set(0, previous + v)?;
        context.stamp(self.terminals[0], self.terminals[0], 1.0)?;
        for i in 0..self.branches {
            let row = context.branch(i)?;
            context.matrix.add(row, row, 1.0)?;
        }
        assert!(context.branch(self.branches).is_err());
        Ok(())
    }
    fn accept(&self, context: &AcceptContext<'_>) -> SpiceResult<()> {
        if context.time == Some(99.0) {
            return Err(SpiceError::circuit("accept refused"));
        }
        self.log
            .borrow_mut()
            .push((context.time, context.states.map(<[f64]>::to_vec)));
        Ok(())
    }
}

fn circuit(log: &Log) -> Circuit {
    let mut circuit = Circuit::new();
    let a = circuit.add_node("a");
    let b = circuit.add_node("b");
    for (name, node, branches) in [("x1", a, 0), ("x2", b, 1)] {
        circuit
            .add_device(Box::new(Accumulator {
                name: name.into(),
                terminals: [node, NodeId::GROUND],
                branches,
                log: Rc::clone(log),
            }))
            .unwrap();
    }
    circuit.finalize().unwrap();
    circuit
}

fn load(
    circuit: &Circuit,
    history: &StateHistory,
    solution: &[f64],
) -> SpiceResult<ngspice_rs::devices::TrialState> {
    let n = circuit.unknown_count();
    let mut matrix = SparseMatrix::new(n, n);
    let mut rhs = Vector::zeros(n);
    let mut trial = history.trial();
    let solution = Vector::from_slice(solution);
    circuit.load(
        &LoadRequest {
            mode: AnalysisMode::Transient { time: 1.0, dt: 0.1 },
            solution: &solution,
            model_context: &ModelContext::default(),
            integration: None,
            history,
            forcing: None,
        },
        &mut matrix,
        &mut rhs,
        &mut trial,
    )?;
    Ok(trial)
}

#[test]
fn slots_and_branch_ranges_are_separate_namespaces() {
    let log = Log::default();
    let circuit = circuit(&log);
    assert_eq!(circuit.unknown_count(), 3);
    assert_eq!(circuit.branch_rows(1), Some(2..3));
    assert_eq!(circuit.state_rows(0), Some(0..1));
    assert_eq!(circuit.state_rows(1), Some(1..2));
    assert_eq!(circuit.state_len(), 2);
    assert_eq!(circuit.state_history().len(), 2);
}

#[test]
fn newton_trials_and_rejected_steps_never_advance_accepted_history() {
    let log = Log::default();
    let circuit = circuit(&log);
    let mut history = circuit.state_history();
    let first = load(&circuit, &history, &[1.0, 2.0, 0.0]).unwrap();
    circuit
        .accept_point(
            &Vector::from_slice(&[1.0, 2.0, 0.0]),
            Some(0.0),
            &mut history,
            first,
        )
        .unwrap();
    let accepted = history.clone();
    // Repeated Newton trials read the same accepted state each time.
    for v in [3.0, 4.0, 5.0] {
        let trial = load(&circuit, &history, &[v, v, 0.0]).unwrap();
        assert_eq!(trial.values(), &[1.0 + v, 2.0 + v]);
        assert_eq!(history, accepted);
    }
    // A rejected step drops its trial.
    drop(load(&circuit, &history, &[7.0, 7.0, 0.0]).unwrap());
    assert_eq!(history, accepted);
    // A failed evaluation leaves only a partial trial behind.
    let error = load(&circuit, &history, &[13.0, 1.0, 0.0]).unwrap_err();
    assert!(error.to_string().contains("evaluation failed"));
    let error = load(&circuit, &history, &[1.0, 13.0, 0.0]).unwrap_err();
    assert!(error.to_string().contains("evaluation failed"));
    assert_eq!(history, accepted);
    // Accept commits exactly once.
    let trial = load(&circuit, &history, &[0.5, 0.25, 0.0]).unwrap();
    circuit
        .accept_point(
            &Vector::from_slice(&[0.5, 0.25, 0.0]),
            Some(0.1),
            &mut history,
            trial,
        )
        .unwrap();
    assert_eq!(history.depth(), 2);
    assert_eq!(history.accepted(1), Some(&[1.5, 2.25][..]));
    assert_eq!(history.accepted(2), Some(&[1.0, 2.0][..]));
    let log = log.borrow();
    assert_eq!(log.len(), 4, "two devices, two accepted points");
    assert_eq!(log[2], (Some(0.1), Some(vec![1.5])));
    assert_eq!(log[3], (Some(0.1), Some(vec![2.25])));
}

#[test]
fn failed_commits_and_accept_hooks_are_atomic() {
    let log = Log::default();
    let circuit = circuit(&log);
    let mut history = circuit.state_history();
    let x = Vector::from_slice(&[1.0, 1.0, 0.0]);
    // An unloaded trial has unset slots: rejected before any hook runs.
    let unloaded = history.trial();
    let error = circuit
        .accept_point(&x, Some(0.0), &mut history, unloaded)
        .unwrap_err();
    assert!(error.to_string().contains("unset"), "{error}");
    assert!(log.borrow().is_empty());
    // A hook failure aborts the commit.
    let trial = load(&circuit, &history, &[1.0, 1.0, 0.0]).unwrap();
    let error = circuit
        .accept_point(&x, Some(99.0), &mut history, trial.clone())
        .unwrap_err();
    assert!(error.to_string().contains("accept refused"));
    assert_eq!(history.depth(), 0);
    // Invalid solutions/times/histories are rejected before hooks.
    for (solution, time) in [
        (vec![1.0, 1.0], Some(0.0)),
        (vec![f64::NAN, 1.0, 0.0], Some(0.0)),
        (vec![1.0, 1.0, 0.0], Some(f64::INFINITY)),
    ] {
        assert!(
            circuit
                .accept_point(
                    &Vector::from_slice(&solution),
                    time,
                    &mut history,
                    trial.clone()
                )
                .is_err()
        );
    }
    let mut wrong = StateHistory::new(5);
    assert!(
        circuit
            .accept_point(&x, Some(0.0), &mut wrong, trial.clone())
            .is_err()
    );
    assert!(log.borrow().is_empty());
    assert_eq!(history.depth(), 0);
    // Stateless acceptance only observes.
    circuit.accept_solution(&x, None).unwrap();
    assert_eq!(log.borrow()[0], (None, None));
    assert_eq!(history.depth(), 0);
}

#[test]
fn loads_validate_dimensions_and_stale_numbering() {
    let log = Log::default();
    let mut circuit = circuit(&log);
    let history = circuit.state_history();
    assert!(load(&circuit, &history, &[1.0, 1.0]).is_err());
    assert!(load(&circuit, &history, &[f64::NAN, 1.0, 0.0]).is_err());
    assert!(load(&circuit, &StateHistory::new(1), &[1.0, 1.0, 0.0]).is_err());
    let c = circuit.add_node("c");
    circuit
        .add_device(Box::new(
            Resistor::new("r1", [c, NodeId::GROUND], 1.0).unwrap(),
        ))
        .unwrap();
    let error = load(&circuit, &history, &[1.0, 1.0, 0.0]).unwrap_err();
    assert!(error.to_string().contains("stale"), "{error}");
}
