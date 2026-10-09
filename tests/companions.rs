//! Capacitor/inductor trap and Gear-2 companion stamps through `Circuit::load`.
use ngspice_rs::devices::{
    AnalysisMode, Capacitor, Circuit, Inductor, LoadRequest, ModelContext, Resistor, StateHistory,
    TrialState,
};
use ngspice_rs::maths::{
    Coefficients, IntegrationMethod, SparseMatrix, StepHistory, Vector, integrator::DEFAULT_XMU,
};
use ngspice_rs::primitives::{NodeId, SpiceResult};

const TRAP: IntegrationMethod = IntegrationMethod::Trapezoidal;
const GEAR2: IntegrationMethod = IntegrationMethod::Gear { order: 2 };

fn close(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs().max(1.0),
        "{got} != {want}"
    );
}

struct Loaded {
    matrix: SparseMatrix,
    rhs: Vector,
    trial: TrialState,
}

fn load(
    circuit: &Circuit,
    history: &StateHistory,
    solution: &[f64],
    integration: Option<&Coefficients>,
) -> SpiceResult<Loaded> {
    let n = circuit.unknown_count();
    let mut matrix = SparseMatrix::new(n, n);
    let mut rhs = Vector::zeros(n);
    let mut trial = history.trial();
    let mode = match integration {
        Some(c) => AnalysisMode::Transient {
            time: 1.0,
            dt: c.dt(),
        },
        None => AnalysisMode::OperatingPoint,
    };
    circuit.load(
        &LoadRequest {
            mode,
            solution: &Vector::from_slice(solution),
            model_context: &ModelContext::default(),
            integration,
            history,
            forcing: None,
        },
        &mut matrix,
        &mut rhs,
        &mut trial,
    )?;
    matrix.fold_duplicates();
    Ok(Loaded { matrix, rhs, trial })
}

fn accept(circuit: &Circuit, history: &mut StateHistory, solution: &[f64], loaded: Loaded) {
    circuit
        .accept_point(&Vector::from_slice(solution), None, history, loaded.trial)
        .unwrap();
}

/// `c1 a b` (floating) or `c1 a 0` (grounded).
fn capacitor(floating: bool, c: f64) -> Circuit {
    let mut circuit = Circuit::new();
    let a = circuit.add_node("a");
    let b = if floating {
        circuit.add_node("b")
    } else {
        NodeId::GROUND
    };
    circuit
        .add_device(Box::new(Capacitor::new("c1", [a, b], c, None).unwrap()))
        .unwrap();
    circuit.finalize().unwrap();
    circuit
}

#[test]
fn dc_loads_keep_the_capacitor_open_and_record_charge() {
    let circuit = capacitor(true, 2e-6);
    assert_eq!(circuit.state_len(), 2);
    let history = circuit.state_history();
    let loaded = load(&circuit, &history, &[3.0, 1.0], None).unwrap();
    assert!(loaded.matrix.is_empty());
    assert_eq!(loaded.rhs.as_slice(), &[0.0, 0.0]);
    assert_eq!(loaded.trial.values(), &[4e-6, 0.0]);
}

#[test]
fn capacitor_companions_match_the_derived_equations() {
    let c = 2e-6;
    for floating in [false, true] {
        let circuit = capacitor(floating, c);
        let x = |a: f64, b: f64| if floating { vec![a, b] } else { vec![a - b] };
        let mut history = circuit.state_history();
        let mut steps = StepHistory::new();
        // Accepted DC point at v = 1, then a backward-Euler step to v = 1.5.
        let dc = load(&circuit, &history, &x(1.0, 0.0), None).unwrap();
        accept(&circuit, &mut history, &x(1.0, 0.0), dc);
        let be = steps.trial(TRAP, 1, 1e-6, DEFAULT_XMU).unwrap();
        let first = load(&circuit, &history, &x(2.0, 0.5), Some(&be)).unwrap();
        let i1 = c * (1.5 - 1.0) / 1e-6;
        close(first.trial.values()[1], i1);
        accept(&circuit, &mut history, &x(2.0, 0.5), first);
        steps.accept(&be);
        let (q1, q2) = (c * 1.5, c * 1.0);
        for method in [TRAP, GEAR2] {
            let coefficients = steps.trial(method, 2, 5e-7, DEFAULT_XMU).unwrap();
            let ag = coefficients.ag();
            let geq = ag[0] * c;
            // ceq = derivative - ag0 q0: -ccap1 ag1 - ag0 q1 for trap,
            // ag1 q1 + ag2 q2 for Gear-2.
            let ceq = if method == TRAP {
                -i1 * ag[1] - ag[0] * q1
            } else {
                ag[1] * q1 + ag[2] * q2
            };
            let v = 1.75;
            let loaded = load(&circuit, &history, &x(v + 0.25, 0.25), Some(&coefficients)).unwrap();
            close(loaded.matrix.get(0, 0), geq);
            close(loaded.rhs.as_slice()[0], -ceq);
            if floating {
                close(loaded.matrix.get(1, 1), geq);
                close(loaded.matrix.get(0, 1), -geq);
                close(loaded.matrix.get(1, 0), -geq);
                close(loaded.rhs.as_slice()[1], ceq);
            } else {
                assert_eq!(loaded.matrix.nnz(), 1, "ground row eliminated");
            }
            // The recorded current is the companion current at the trial point.
            close(loaded.trial.values()[0], c * v);
            close(loaded.trial.values()[1], geq * v + ceq);
        }
    }
}

/// `l1 a 0` in parallel with `r1 a 0`.
fn rl(l: f64, r: f64) -> Circuit {
    let mut circuit = Circuit::new();
    let a = circuit.add_node("a");
    circuit
        .add_device(Box::new(
            Inductor::new("l1", [a, NodeId::GROUND], l, None).unwrap(),
        ))
        .unwrap();
    circuit
        .add_device(Box::new(
            Resistor::new("r1", [a, NodeId::GROUND], r).unwrap(),
        ))
        .unwrap();
    circuit.finalize().unwrap();
    circuit
}

#[test]
fn inductor_branch_companions_have_the_documented_signs() {
    let l = 1e-3;
    let circuit = rl(l, 10.0);
    assert_eq!(circuit.branch_rows(0), Some(1..2));
    assert_eq!(circuit.state_rows(0), Some(0..2));
    assert_eq!(circuit.state_rows(1), Some(2..2));
    let mut history = circuit.state_history();
    let mut steps = StepHistory::new();
    // DC: a short, flux = L i.
    let dc = load(&circuit, &history, &[0.0, 0.2], None).unwrap();
    close(dc.matrix.get(0, 1), 1.0);
    close(dc.matrix.get(1, 0), 1.0);
    close(dc.matrix.get(1, 1), 0.0);
    assert_eq!(dc.trial.values()[..2], [0.2 * l, 0.0]);
    accept(&circuit, &mut history, &[0.0, 0.2], dc);
    let be = steps.trial(TRAP, 1, 1e-6, DEFAULT_XMU).unwrap();
    let first = load(&circuit, &history, &[-1.0, 0.1], Some(&be)).unwrap();
    let v1 = l * (0.1 - 0.2) / 1e-6;
    close(first.trial.values()[1], v1);
    accept(&circuit, &mut history, &[-1.0, 0.1], first);
    steps.accept(&be);
    for method in [TRAP, GEAR2] {
        let coefficients = steps.trial(method, 2, 2e-6, DEFAULT_XMU).unwrap();
        let ag = coefficients.ag();
        let req = ag[0] * l;
        let veq = if method == TRAP {
            -v1 * ag[1] - ag[0] * l * 0.1
        } else {
            ag[1] * l * 0.1 + ag[2] * l * 0.2
        };
        let loaded = load(&circuit, &history, &[-0.5, 0.05], Some(&coefficients)).unwrap();
        // KCL at a: +i (leaves a through l1) + v/R.
        close(loaded.matrix.get(0, 1), 1.0);
        close(loaded.matrix.get(0, 0), 0.1);
        // Branch: v(a) - req i = veq.
        close(loaded.matrix.get(1, 0), 1.0);
        close(loaded.matrix.get(1, 1), -req);
        close(loaded.rhs.as_slice()[1], veq);
        close(loaded.rhs.as_slice()[0], 0.0);
    }
}

#[test]
fn repeated_rejected_and_rolled_back_trials_leave_history_intact() {
    let c = 1e-6;
    let circuit = capacitor(false, c);
    let mut history = circuit.state_history();
    let mut steps = StepHistory::new();
    let dc = load(&circuit, &history, &[1.0], None).unwrap();
    accept(&circuit, &mut history, &[1.0], dc);
    let snapshot = (history.clone(), steps.clone());
    // Newton-style repeated loads and changed/rejected dt.
    for dt in [1e-6, 1e-6, 2.5e-7, 1e-9] {
        let coefficients = steps.trial(GEAR2, 1, dt, DEFAULT_XMU).unwrap();
        for v in [0.9, 0.8] {
            load(&circuit, &history, &[v], Some(&coefficients)).unwrap();
        }
    }
    // A failed load (dt disagreement) rolls back by dropping its trial.
    let coefficients = steps.trial(TRAP, 1, 1e-6, DEFAULT_XMU).unwrap();
    let n = circuit.unknown_count();
    let mut trial = history.trial();
    let error = circuit
        .load(
            &LoadRequest {
                mode: AnalysisMode::Transient {
                    time: 1e-6,
                    dt: 2e-6,
                },
                solution: &Vector::from_slice(&[0.5]),
                model_context: &ModelContext::default(),
                integration: Some(&coefficients),
                history: &history,
                forcing: None,
            },
            &mut SparseMatrix::new(n, n),
            &mut Vector::zeros(n),
            &mut trial,
        )
        .unwrap_err();
    assert!(error.to_string().contains("differs"), "{error}");
    assert_eq!((history.clone(), steps.clone()), snapshot);
    // Accept commits exactly once.
    let loaded = load(&circuit, &history, &[0.5], Some(&coefficients)).unwrap();
    accept(&circuit, &mut history, &[0.5], loaded);
    steps.accept(&coefficients);
    assert_eq!(history.depth(), 2);
    close(history.accepted(1).unwrap()[0], 0.5 * c);
    close(history.accepted(2).unwrap()[0], c);
    assert_eq!(steps.accepted(), &[1e-6]);
}

#[test]
fn unsupported_and_incomplete_companion_loads_fail_explicitly() {
    let circuit = capacitor(false, 1e-6);
    let mut history = circuit.state_history();
    let steps = StepHistory::new();
    // No coefficients in a transient load.
    let n = circuit.unknown_count();
    let error = circuit
        .load(
            &LoadRequest {
                mode: AnalysisMode::Transient {
                    time: 1e-6,
                    dt: 1e-6,
                },
                solution: &Vector::from_slice(&[0.5]),
                model_context: &ModelContext::default(),
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut SparseMatrix::new(n, n),
            &mut Vector::zeros(n),
            &mut history.trial(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("without integration"), "{error}");
    // No accepted history yet.
    let be = steps.trial(TRAP, 1, 1e-6, DEFAULT_XMU).unwrap();
    let error = load(&circuit, &history, &[0.5], Some(&be)).err().unwrap();
    assert!(error.to_string().contains("accepted point"), "{error}");
    // Gear orders above 2 cannot even produce coefficients.
    let error = IntegrationMethod::Gear { order: 3 }
        .validate_runtime()
        .unwrap_err();
    assert!(error.to_string().contains("only orders 1 and 2"));
    // Trap order 2 needs an accepted derivative, which a DC point provides.
    let dc = load(&circuit, &history, &[1.0], None).unwrap();
    accept(&circuit, &mut history, &[1.0], dc);
    let mut warm = StepHistory::new();
    warm.accept(&be);
    let trap = warm.trial(TRAP, 2, 1e-6, DEFAULT_XMU).unwrap();
    assert!(load(&circuit, &history, &[0.5], Some(&trap)).is_ok());
    let gear = warm.trial(GEAR2, 2, 1e-6, DEFAULT_XMU).unwrap();
    let error = load(&circuit, &history, &[0.5], Some(&gear)).err().unwrap();
    assert!(error.to_string().contains("order 2 needs 2"), "{error}");
}

/// Fixed-step trap and Gear-2 companion RC/RL decays converge at second order.
#[test]
fn companion_rc_and_rl_decays_converge_at_second_order() {
    fn decay(
        circuit: &Circuit,
        initial: &[f64],
        method: IntegrationMethod,
        steps: usize,
    ) -> Vec<f64> {
        let mut history = circuit.state_history();
        let mut step_history = StepHistory::new();
        let dc = load(circuit, &history, initial, None).unwrap();
        accept(circuit, &mut history, initial, dc);
        let h = 1e-3 / steps as f64;
        let mut x = initial.to_vec();
        for _ in 0..steps {
            let order = step_history.available_order(method);
            let coefficients = step_history.trial(method, order, h, DEFAULT_XMU).unwrap();
            let loaded = load(circuit, &history, &x, Some(&coefficients)).unwrap();
            let next = loaded
                .matrix
                .solve(&loaded.rhs)
                .unwrap()
                .as_slice()
                .to_vec();
            // Linear: the solution is exact for this trial; reload at it so
            // the committed state matches the accepted point.
            let loaded = load(circuit, &history, &next, Some(&coefficients)).unwrap();
            accept(circuit, &mut history, &next, loaded);
            step_history.accept(&coefficients);
            x = next;
        }
        x
    }
    // RC: r1 a 0 1k, c1 a 0 1u (tau = 1 ms), v(0) = 1.
    let mut rc = Circuit::new();
    let a = rc.add_node("a");
    rc.add_device(Box::new(
        Resistor::new("r1", [a, NodeId::GROUND], 1e3).unwrap(),
    ))
    .unwrap();
    rc.add_device(Box::new(
        Capacitor::new("c1", [a, NodeId::GROUND], 1e-6, None).unwrap(),
    ))
    .unwrap();
    rc.finalize().unwrap();
    // RL: tau = L/R = 1 ms, i(0) = 1 A from a to ground through l1.
    let rl = rl(1e-2, 10.0);
    let exact = (-1.0_f64).exp();
    for method in [TRAP, GEAR2] {
        let errors: Vec<f64> = [50, 100, 200]
            .iter()
            .map(|&n| (decay(&rc, &[1.0], method, n)[0] - exact).abs())
            .collect();
        for w in errors.windows(2) {
            let ratio = w[0] / w[1];
            assert!((3.5..4.6).contains(&ratio), "{method:?} RC ratio {ratio}");
        }
        assert!(errors[2] < 2e-5, "{method:?} {errors:?}");
        let errors: Vec<f64> = [50, 100, 200]
            .iter()
            .map(|&n| {
                let x = decay(&rl, &[-10.0, 1.0], method, n);
                // KCL at a: resistor carries the inductor current back.
                close(x[0], -10.0 * x[1]);
                (x[1] - exact).abs()
            })
            .collect();
        for w in errors.windows(2) {
            let ratio = w[0] / w[1];
            assert!((3.5..4.6).contains(&ratio), "{method:?} RL ratio {ratio}");
        }
    }
}
