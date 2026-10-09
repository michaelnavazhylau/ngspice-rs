//! Independent sources stamp their time-`t` forcing in companion transient loads.
use ngspice_rs::devices::{
    AnalysisMode, Circuit, Forcing, IndependentSource, Limit, LoadRequest, ModelContext, Pulse,
    PulseSpec, Resistor, TransientTiming, Waveform,
};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::primitives::{Complex, NodeId, SpiceResult};

fn circuit(voltage: bool, waveform: Waveform) -> Circuit {
    let mut circuit = Circuit::new();
    let a = circuit.add_node("a");
    circuit
        .add_device(Box::new(
            IndependentSource::new(
                "s1",
                [a, NodeId::GROUND],
                voltage,
                7.0,
                Complex::ZERO,
                waveform,
            )
            .unwrap(),
        ))
        .unwrap();
    circuit
        .add_device(Box::new(
            Resistor::new("r1", [a, NodeId::GROUND], 1e3).unwrap(),
        ))
        .unwrap();
    circuit.finalize().unwrap();
    circuit
}

/// RHS of a transient load at `time` (no integration state is involved).
fn rhs(circuit: &Circuit, mode: AnalysisMode, forcing: Option<Forcing>) -> SpiceResult<Vec<f64>> {
    let n = circuit.unknown_count();
    let history = circuit.state_history();
    let mut matrix = SparseMatrix::new(n, n);
    let mut rhs = Vector::zeros(n);
    circuit.load(
        &LoadRequest {
            mode,
            solution: &Vector::zeros(n),
            model_context: &ModelContext::default(),
            integration: None,
            history: &history,
            forcing,
        },
        &mut matrix,
        &mut rhs,
        &mut history.trial(),
    )?;
    Ok(rhs.as_slice().to_vec())
}

fn at(time: f64, limit: Limit) -> (AnalysisMode, Option<Forcing>) {
    (
        AnalysisMode::Transient { time, dt: 1e-6 },
        Some(Forcing {
            limit,
            timing: TransientTiming::new(1e-6, 1e-3).unwrap(),
        }),
    )
}

#[test]
fn transient_loads_use_the_waveform_with_the_requested_limit() {
    let step = Waveform::Step {
        before: 1.0,
        after: 3.0,
        time: 1e-4,
    };
    // Voltage source: the branch row (index 1) carries the forcing.
    let voltage = circuit(true, step.clone());
    for (time, limit, want) in [
        (5e-5, Limit::Right, 1.0),
        (1e-4, Limit::Left, 1.0),
        (1e-4, Limit::Right, 3.0),
        (2e-4, Limit::Left, 3.0),
    ] {
        let (mode, forcing) = at(time, limit);
        let b = rhs(&voltage, mode, forcing).unwrap();
        assert_eq!(b, [0.0, want], "t={time} {limit:?}");
    }
    // Current source: positive from the first terminal to the second, so a
    // source from `a` to ground draws current out of node `a`.
    let current = circuit(false, step);
    let (mode, forcing) = at(1e-4, Limit::Right);
    assert_eq!(rhs(&current, mode, forcing).unwrap(), [-3.0]);
    // DC loads still stamp the DC value, not the waveform.
    let b = rhs(&voltage, AnalysisMode::OperatingPoint, None).unwrap();
    assert_eq!(b, [0.0, 7.0]);
}

#[test]
fn unresolved_pulse_defaults_are_bound_by_the_forcing_timing() {
    // PULSE(0 4): TR defaults to the .tran step (1 us); it ramps to 4 V.
    let spec = PulseSpec {
        initial: 0.0,
        pulsed: 4.0,
        delay: None,
        rise: None,
        fall: None,
        width: None,
        period: None,
        count: None,
    };
    let source = circuit(true, Waveform::PulseDefaults(spec));
    let (mode, forcing) = at(5e-7, Limit::Right);
    assert_eq!(rhs(&source, mode, forcing).unwrap(), [0.0, 2.0]);
    let (mode, forcing) = at(2e-6, Limit::Right);
    assert_eq!(rhs(&source, mode, forcing).unwrap(), [0.0, 4.0]);
    // A pre-resolved pulse needs no defaults.
    let pulse = Pulse::new(0.0, 4.0, 0.0, 1e-6, 1e-6, 1e-4, 2e-4).unwrap();
    let source = circuit(true, Waveform::Pulse(pulse));
    let (mode, forcing) = at(5e-7, Limit::Right);
    assert_eq!(rhs(&source, mode, forcing).unwrap(), [0.0, 2.0]);
}

#[test]
fn transient_loads_without_forcing_or_in_ac_fail_explicitly() {
    let source = circuit(true, Waveform::Constant(1.0));
    let (mode, _) = at(1e-6, Limit::Right);
    let error = rhs(&source, mode, None).unwrap_err().to_string();
    assert!(error.contains("without a forcing context"), "{error}");
    let error = rhs(&source, AnalysisMode::Ac { frequency: 1.0 }, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("AC"), "{error}");
}
