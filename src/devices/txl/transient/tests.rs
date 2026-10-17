use super::*;
use crate::devices::traits::MnaUnknowns;
use crate::maths::Vector;
use crate::primitives::NodeTable;

#[test]
fn rejected_trials_are_pure_and_the_clamp_survives_pruning() {
    let mut nodes = NodeTable::new();
    let terminals = [nodes.intern("in"), nodes.intern("out")];
    let mut unknowns = MnaUnknowns::new();
    unknowns.rebuild(&nodes);
    let solution = Vector::zeros(4);
    let device = Txl {
        name: "y1".into(),
        terminals,
        instance_length: None,
        resistance: 1.,
        coefficients: super::super::pade::Pade::new(1., 250e-9, 0., 100e-12, 1.).unwrap(),
    };
    let tau = device.coefficients.delay * 1e-12;
    let mut context = DelayContext {
        solution: &solution,
        unknowns: &unknowns,
        time: 0.,
        steps: [tau; 3],
        min_break: 0.,
        reltol: 1e-3,
        abstol: 1e-12,
        initial_conditions: false,
        branches: 2..4,
    };
    let mut history = DelayHistory::new(WIDTH);
    history.apply(&device.start(&context).unwrap()).unwrap();
    let before = history.clone();
    assert_eq!(device.timestep_limit(&history, &context).unwrap(), None);
    let first = device
        .transient_stamp(&history, 2. * tau, 2. * tau, 1e-12)
        .unwrap();
    device
        .transient_stamp(&history, 1.5 * tau, 1.5 * tau, 1e-12)
        .unwrap();
    assert_eq!(history, before, "a rejected load cannot change history");
    assert_eq!(
        first,
        device
            .transient_stamp(&history, 2. * tau, 2. * tau, 1e-12)
            .unwrap()
    );
    assert_eq!(device.timestep_limit(&history, &context).unwrap(), None);

    context.time = 2. * tau;
    context.steps[0] = 2. * tau;
    history
        .apply(&device.accept(&history, &context).unwrap())
        .unwrap();
    assert_eq!(
        device.timestep_limit(&history, &context).unwrap(),
        Some(0.9 * tau)
    );
    for _ in 0..10 {
        context.time += 0.8 * tau;
        context.steps[0] = 0.8 * tau;
        history
            .apply(&device.accept(&history, &context).unwrap())
            .unwrap();
    }
    assert!(
        history.time(0).unwrap() > 2. * tau,
        "the triggering gap is gone"
    );
    assert_eq!(
        device.timestep_limit(&history, &context).unwrap(),
        Some(0.9 * tau)
    );
    history.apply(&device.start(&context).unwrap()).unwrap();
    assert_eq!(
        device.timestep_limit(&history, &context).unwrap(),
        None,
        "restart resets the clamp"
    );
}

#[test]
fn picosecond_history_has_explicit_range_checks() {
    assert_eq!(ticks(1.9e-12).unwrap(), 1.);
    assert_eq!(ticks(0.).unwrap(), 0.);
    for time in [-1., 0.003, f64::INFINITY, f64::NAN] {
        assert!(ticks(time).is_err(), "{time}");
    }
}
