//! #35: typed scalar DC sweeps over V/I sources, literal and model-backed
//! resistors and temperature, against analytic circuit solutions. Grids, limits,
//! restoration, rejection-before-samples and the factor-reuse boundary.
use spice_analysis::sweep::{MAX_SWEEP_POINTS, SweepSpec, SweepTarget, resolve};
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use spice_core::{AnalysisKind, Complex, NodeId, SpiceError, SpiceResult};
use spice_devices::{
    AcceptContext, Capacitor, Circuit, Device, IndependentSource, LinearContext, Resistor,
    StampContext, Waveform,
};
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::{cell::Cell, path::Path, rc::Rc};

fn netlist(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("sweep.cir"),
            &format!("sweep\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&netlist(body)).unwrap()
}
fn hot() -> AnalysisContext {
    AnalysisContext {
        temperature: 47.,
        nominal_temperature: 27.,
    }
}
fn run_in(c: &mut Circuit, args: &[&str], context: &AnalysisContext) -> SpiceResult<Plot> {
    runner(AnalysisKind::DcSweep)?.run(
        c,
        &AnalysisRequest::with_arguments(AnalysisKind::DcSweep, args.iter().copied()),
        context,
    )
}
fn run(c: &mut Circuit, args: &[&str]) -> SpiceResult<Plot> {
    run_in(c, args, &AnalysisContext::default())
}
fn op(c: &mut Circuit, context: &AnalysisContext) -> Plot {
    runner(AnalysisKind::OperatingPoint)
        .unwrap()
        .run(
            c,
            &AnalysisRequest::new(AnalysisKind::OperatingPoint),
            context,
        )
        .unwrap()
}
fn close(got: f64, want: f64, relative: f64, absolute: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs() + absolute,
        "{got:e} != {want:e}"
    );
}
fn column(p: &Plot, name: &str) -> Vec<f64> {
    (0..p.point_count())
        .map(|row| p.value(name, row).unwrap().re)
        .collect()
}
fn assert_column(p: &Plot, name: &str, want: &[f64], relative: f64, absolute: f64) {
    let got = column(p, name);
    assert_eq!(got.len(), want.len(), "{name}: {got:?}");
    for (g, w) in got.iter().zip(want) {
        close(*g, *w, relative, absolute);
    }
}
fn unit(p: &Plot, name: &str) -> String {
    p.variables[p.variable_index(name).unwrap()].unit.clone()
}
fn grid(start: f64, stop: f64, step: f64) -> SpiceResult<Vec<f64>> {
    SweepSpec {
        target: SweepTarget::VoltageSource("v1".into()),
        start,
        stop,
        step,
    }
    .grid()
}

// ---- grids -----------------------------------------------------------------

#[test]
fn reachable_inclusive_endpoints_survive_binary_rounding() {
    // 0.3/0.1 is 2.9999999999999996 steps: the stop is reachable and included.
    let g = grid(0., 0.3, 0.1).unwrap();
    assert_eq!(g.len(), 4);
    assert_eq!(*g.last().unwrap(), 0.3);
    for (i, v) in g.iter().enumerate() {
        close(*v, 0.1 * i as f64, 1e-12, 1e-15);
    }
    let g = grid(0., 1., 0.1).unwrap();
    assert_eq!((g.len(), *g.last().unwrap()), (11, 1.));
    // Reverse: the stop is exactly 0, not 5.5e-17.
    let g = grid(0.3, 0., -0.1).unwrap();
    assert_eq!((g.len(), g[0], *g.last().unwrap()), (4, 0.3, 0.));
    // Large offsets round at the same relative scale.
    let g = grid(1e6, 1e6 + 0.3, 0.1).unwrap();
    assert_eq!((g.len(), *g.last().unwrap()), (4, 1e6 + 0.3));
    // A stop that is not on the grid is not included, and never overshot.
    assert_eq!(grid(0., 1., 0.4).unwrap(), vec![0., 0.4, 0.8]);
    let g = grid(1., 0., -0.4).unwrap();
    assert_eq!(g.len(), 3);
    assert!(g[2] > 0. && g[2] < 0.2 + 1e-9, "{g:?}");
}

#[test]
fn reverse_signed_and_single_point_grids() {
    assert_eq!(grid(1., 0., -0.25).unwrap(), vec![1., 0.75, 0.5, 0.25, 0.]);
    assert_eq!(grid(-1., 1., 0.5).unwrap(), vec![-1., -0.5, 0., 0.5, 1.]);
    assert_eq!(grid(1., -1., -0.5).unwrap(), vec![1., 0.5, 0., -0.5, -1.]);
    assert_eq!(grid(-3., -1., 1.).unwrap(), vec![-3., -2., -1.]);
    // start == stop is one point for either sign of step.
    assert_eq!(grid(0.1, 0.1, 5.).unwrap(), vec![0.1]);
    assert_eq!(grid(0.1, 0.1, -5.).unwrap(), vec![0.1]);
}

#[test]
fn invalid_grids_are_rejected_before_any_value_is_produced() {
    for (start, stop, step) in [
        (0., 1., 0.),       // zero step
        (0., 1., -0.),      // negative zero step
        (0., 1., -0.1),     // wrong direction
        (1., 0., 0.1),      // wrong direction
        (f64::NAN, 1., 1.), // nonfinite
        (0., f64::NAN, 1.),
        (0., 1., f64::NAN),
        (f64::INFINITY, 1., 1.),
        (0., f64::NEG_INFINITY, 1.),
        (0., 1., f64::INFINITY),
        (1e16, 1e16 + 4., 0.5), // step below the spacing: no progress
        (-1e308, 1e308, 1.),    // span overflows
        (0., 1e300, 1.),        // step count overflows the point limit
        (0., 1., 1e-6),         // too many points
        (0., MAX_SWEEP_POINTS as f64, 1.), // one past the limit
    ] {
        assert!(grid(start, stop, step).is_err(), "{start} {stop} {step}");
    }
    // Exactly the limit is allowed.
    let g = grid(0., (MAX_SWEEP_POINTS - 1) as f64, 1.).unwrap();
    assert_eq!(
        (g.len(), g[MAX_SWEEP_POINTS - 1]),
        (MAX_SWEEP_POINTS, 99_999.)
    );
}

#[test]
fn target_specific_grid_domains() {
    let temp = |start, stop, step| {
        SweepSpec {
            target: SweepTarget::Temperature,
            start,
            stop,
            step,
        }
        .grid()
    };
    assert_eq!(temp(27., 37., 10.).unwrap(), vec![27., 37.]);
    assert!(temp(-273.15, 0., 100.).is_err());
    assert!(temp(-300., 0., 100.).is_err());
    let res = |start, stop, step| {
        SweepSpec {
            target: SweepTarget::Resistor("r1".into()),
            start,
            stop,
            step,
        }
        .grid()
    };
    assert_eq!(res(-2., -1., 1.).unwrap(), vec![-2., -1.]); // negative scalars are legal
    assert!(res(-1., 1., 1.).is_err()); // passes through zero
    assert!(res(0., 1., 1.).is_err());
    assert!(res(1e-320, 1e-320, 1.).is_err()); // infinite conductance
    assert_eq!(SweepTarget::Resistor("r1".into()).unit(), "resistance");
}

// ---- run: sources, endpoints, nesting ----------------------------------------

#[test]
fn endpoint_rounding_reaches_the_run() {
    let mut c = circuit("v1 a 0 0\nr1 a 0 1k");
    let p = run(&mut c, &["v1", "0", "0.3", "0.1"]).unwrap();
    assert_eq!(p.point_count(), 4);
    assert_eq!(*column(&p, "sweep").last().unwrap(), 0.3);
    assert_column(&p, "i(v1)", &[0., -1e-4, -2e-4, -3e-4], 1e-12, 1e-18);
    let p = run(&mut c, &["v1", "0.3", "0", "-0.1"]).unwrap();
    assert_eq!(column(&p, "sweep").last(), Some(&0.));
    assert_eq!(p.point_count(), 4);
    assert_eq!(unit(&p, "sweep"), "voltage");
}

#[test]
fn nested_voltage_grids_are_inner_fast_forward_and_reverse() {
    let mut c = circuit("v1 a 0 0\nv2 b 0 0\nr1 a b 1k");
    // i(v1) = (v2 - v1)/R; the first axis is inner, as C's dctrcurv loops.
    let p = run(&mut c, &["v1", "0", "1", "0.5", "v2", "1", "2", "1"]).unwrap();
    assert_column(&p, "sweep", &[0., 0.5, 1., 0., 0.5, 1.], 0., 1e-15);
    assert_column(&p, "sweep(v2)", &[1., 1., 1., 2., 2., 2.], 0., 0.);
    assert_column(
        &p,
        "i(v1)",
        &[1e-3, 5e-4, 0., 2e-3, 1.5e-3, 1e-3],
        1e-12,
        1e-15,
    );
    assert_eq!(
        (unit(&p, "sweep"), unit(&p, "sweep(v2)")),
        ("voltage".into(), "voltage".into())
    );
    let p = run(&mut c, &["v1", "1", "0", "-0.5", "v2", "2", "1", "-1"]).unwrap();
    assert_column(&p, "sweep", &[1., 0.5, 0., 1., 0.5, 0.], 0., 1e-15);
    assert_column(&p, "sweep(v2)", &[2., 2., 2., 1., 1., 1.], 0., 0.);
    assert_column(
        &p,
        "i(v1)",
        &[1e-3, 1.5e-3, 2e-3, 0., 5e-4, 1e-3],
        1e-12,
        1e-15,
    );
}

#[test]
fn nested_current_and_voltage_grids_forward_and_reverse() {
    // KCL at a: I = va/1k + (va - V2)/1k.
    let mut c = circuit("i1 0 a 0\nv2 b 0 0\nr1 a 0 1k\nr2 a b 1k");
    let va = |i: f64, v2: f64| (i * 1e3 + v2) / 2.;
    let p = run(&mut c, &["i1", "0", "2m", "1m", "v2", "0", "2", "2"]).unwrap();
    assert_column(&p, "sweep", &[0., 1e-3, 2e-3, 0., 1e-3, 2e-3], 1e-12, 1e-18);
    assert_column(&p, "sweep(v2)", &[0., 0., 0., 2., 2., 2.], 0., 0.);
    let want: Vec<_> = [
        (0., 0.),
        (1e-3, 0.),
        (2e-3, 0.),
        (0., 2.),
        (1e-3, 2.),
        (2e-3, 2.),
    ]
    .iter()
    .map(|(i, v)| va(*i, *v))
    .collect();
    assert_column(&p, "v(a)", &want, 1e-12, 1e-15);
    assert_eq!(
        (unit(&p, "sweep"), unit(&p, "sweep(v2)")),
        ("current".into(), "voltage".into())
    );
    let p = run(&mut c, &["i1", "2m", "0", "-1m", "v2", "2", "0", "-2"]).unwrap();
    assert_column(&p, "sweep", &[2e-3, 1e-3, 0., 2e-3, 1e-3, 0.], 1e-12, 1e-18);
    assert_column(&p, "sweep(v2)", &[2., 2., 2., 0., 0., 0.], 0., 0.);
    let want: Vec<_> = [
        (2e-3, 2.),
        (1e-3, 2.),
        (0., 2.),
        (2e-3, 0.),
        (1e-3, 0.),
        (0., 0.),
    ]
    .iter()
    .map(|(i, v)| va(*i, *v))
    .collect();
    assert_column(&p, "v(a)", &want, 1e-12, 1e-15);
}

// ---- run: resistors ------------------------------------------------------------

#[test]
fn literal_resistor_grids_recompute_the_operator_at_every_point() {
    // A factor reused from the unswept 1k/1k divider would give v(b) = 1 throughout.
    let mut c = circuit("v1 a 0 2\nr1 a b 1k\nr2 b 0 1k");
    let rs = [500., 1000., 1500., 2000.];
    let p = run(&mut c, &["r1", "500", "2k", "500"]).unwrap();
    assert_column(&p, "sweep", &rs, 1e-12, 0.);
    let vb: Vec<_> = rs.iter().map(|r| 2. * 1e3 / (r + 1e3)).collect();
    assert_column(&p, "v(b)", &vb, 1e-12, 1e-15);
    let i: Vec<_> = rs.iter().map(|r| -2. / (r + 1e3)).collect();
    assert_column(&p, "i(v1)", &i, 1e-12, 1e-15);
    assert_eq!(unit(&p, "sweep"), "resistance");
    // Reverse resistor grid on the other resistor.
    let p = run(&mut c, &["r2", "2k", "1k", "-500"]).unwrap();
    assert_column(
        &p,
        "v(b)",
        &[2. * 2e3 / 3e3, 2. * 1.5e3 / 2.5e3, 1.],
        1e-12,
        1e-15,
    );
    // A literal resistor has no temperature law: the supplied scalar is the effective one.
    let p = run_in(&mut c, &["r1", "500", "2k", "500"], &hot()).unwrap();
    assert_column(&p, "v(b)", &vb, 1e-12, 1e-15);
}

#[test]
fn nested_source_and_resistor_axes_mix_rhs_offsets_with_replaced_operators() {
    // The first axis is inner (fast) and it is a source, while the outer axis
    // replaces a resistor: the point mixes a temporary RHS offset with a
    // rebuilt operator, and the plot carries both nested columns.
    let mut c = circuit("v1 in 0 1\nr1 in out 1k\nr2 out 0 1k");
    let p = run(&mut c, &["v1", "0", "1", "0.5", "r1", "1k", "2k", "1k"]).unwrap();
    assert_eq!(p.point_count(), 6);
    assert_column(&p, "sweep", &[0., 0.5, 1., 0., 0.5, 1.], 0., 1e-15);
    assert_column(&p, "sweep(r1)", &[1e3, 1e3, 1e3, 2e3, 2e3, 2e3], 1e-12, 0.);
    let vout = [1e3, 2e3]
        .into_iter()
        .flat_map(|r1| [0., 0.5, 1.].map(move |v1| v1 * 1e3 / (r1 + 1e3)));
    assert_column(&p, "v(out)", &vout.collect::<Vec<_>>(), 1e-12, 1e-15);
    assert_eq!(
        (unit(&p, "sweep"), unit(&p, "sweep(r1)")),
        ("voltage".into(), "resistance".into())
    );
    // Reverse on both axes keeps the inner-fast ordering.
    let p = run(&mut c, &["v1", "1", "0", "-0.5", "r1", "2k", "1k", "-1k"]).unwrap();
    assert_column(&p, "sweep", &[1., 0.5, 0., 1., 0.5, 0.], 0., 1e-15);
    assert_column(&p, "sweep(r1)", &[2e3, 2e3, 2e3, 1e3, 1e3, 1e3], 1e-12, 0.);
}

#[test]
fn two_resistor_axes_nest_inner_fast_and_reject_the_same_resistor_twice() {
    let mut c = circuit("v1 a 0 2\nr1 a b 1k\nr2 b 0 1k");
    let p = run(&mut c, &["r1", "1k", "2k", "1k", "r2", "1k", "3k", "2k"]).unwrap();
    assert_column(&p, "sweep", &[1e3, 2e3, 1e3, 2e3], 1e-12, 0.);
    assert_column(&p, "sweep(r2)", &[1e3, 1e3, 3e3, 3e3], 1e-12, 0.);
    let vb = [(1e3, 1e3), (2e3, 1e3), (1e3, 3e3), (2e3, 3e3)].map(|(r1, r2)| 2. * r2 / (r1 + r2));
    assert_column(&p, "v(b)", &vb, 1e-12, 1e-15);
    assert_eq!(unit(&p, "sweep(r2)"), "resistance");
    assert!(run(&mut c, &["r1", "1k", "2k", "1k", "R1", "1k", "2k", "1k"]).is_err());
}

#[test]
fn model_backed_sweeps_sweep_the_supplied_scalar_and_keep_temperature_scale_and_multiplicity() {
    // Reffective = supplied * (1 + 0.01*(T - 27)) * 2 / 4.
    let body = "v1 a 0 1\nr1 a 0 rm scale=2 m=4\n.model rm r(r=1k tc1=0.01)";
    let n = netlist(body);
    let mut c = Circuit::from_netlist_with_context(&n, &hot().model_context()).unwrap();
    let p = run_in(&mut c, &["r1", "1k", "3k", "1k"], &hot()).unwrap();
    let want: Vec<_> = [1e3, 2e3, 3e3].iter().map(|s| -1. / (s * 0.6)).collect();
    assert_column(&p, "i(v1)", &want, 1e-9, 0.);
    // The first point supplies the deck's own model R, so it matches the unswept OP.
    close(
        p.value("i(v1)", 0).unwrap().re,
        op(&mut c, &hot()).value("i(v1)", 0).unwrap().re,
        1e-9,
        0.,
    );
    // The swept scalar replaces the model R: 4k is not the model's 1k.
    let p = run_in(&mut c, &["r1", "4k", "4k", "1"], &hot()).unwrap();
    close(p.value("i(v1)", 0).unwrap().re, -1. / (4e3 * 0.6), 1e-9, 0.);
    // At the default temperature only scale/m remain.
    let mut cold = Circuit::from_netlist(&n).unwrap();
    let p = run(&mut cold, &["r1", "1k", "3k", "1k"]).unwrap();
    let want: Vec<_> = [1e3, 2e3, 3e3].iter().map(|s| -1. / (s * 0.5)).collect();
    assert_column(&p, "i(v1)", &want, 1e-9, 0.);
}

#[test]
fn resistor_and_temperature_axes_in_both_orders_recompute_model_backed_values() {
    let mut c = circuit("v1 a 0 1\nr1 a 0 rm\n.model rm r(r=1k tc1=0.01)");
    let r = |supplied: f64, t: f64| supplied * (1. + 0.01 * (t - 27.));
    let p = run(&mut c, &["r1", "1k", "2k", "1k", "temp", "27", "47", "20"]).unwrap();
    assert_column(&p, "sweep", &[1e3, 2e3, 1e3, 2e3], 1e-12, 0.);
    assert_column(&p, "sweep(temp)", &[27., 27., 47., 47.], 0., 0.);
    let want = [(1e3, 27.), (2e3, 27.), (1e3, 47.), (2e3, 47.)].map(|(s, t)| -1. / r(s, t));
    assert_column(&p, "i(v1)", &want, 1e-9, 0.);
    assert_eq!(
        (unit(&p, "sweep"), unit(&p, "sweep(temp)")),
        ("resistance".into(), "temperature".into())
    );
    let p = run(&mut c, &["temp", "27", "47", "20", "r1", "1k", "2k", "1k"]).unwrap();
    assert_column(&p, "sweep", &[27., 47., 27., 47.], 0., 0.);
    assert_column(&p, "sweep(r1)", &[1e3, 1e3, 2e3, 2e3], 1e-12, 0.);
    let want = [(1e3, 27.), (1e3, 47.), (2e3, 27.), (2e3, 47.)].map(|(s, t)| -1. / r(s, t));
    assert_column(&p, "i(v1)", &want, 1e-9, 0.);
    assert_eq!(
        (unit(&p, "sweep"), unit(&p, "sweep(r1)")),
        ("temperature".into(), "resistance".into())
    );
}

#[test]
fn temperature_sweeps_recompute_models_and_instance_temp_stays_fixed() {
    // r1's instance TEMP (77 C) overrides the swept circuit temperature; r2 follows it.
    let mut c =
        circuit("v1 a 0 1\nr1 a 0 rm temp=77\nr2 a 0 rm\n.model rm r(r=1k tc1=0.01 tnom=27)");
    let p = run(&mut c, &["temp", "27", "47", "10"]).unwrap();
    assert_column(&p, "sweep", &[27., 37., 47.], 0., 0.);
    let want: Vec<_> = [27., 37., 47.]
        .iter()
        .map(|t| -(1. / 1500. + 1. / (1e3 * (1. + 0.01 * (t - 27.)))))
        .collect();
    assert_column(&p, "i(v1)", &want, 1e-9, 0.);
}

#[test]
fn nonlinear_circuits_converge_at_every_resistor_point() {
    let n = netlist("v1 in 0 1\nr1 in d 1k\nd1 d 0 dm\n.model dm d(is=1e-14)");
    let mut c = Circuit::from_netlist(&n).unwrap();
    let p = run(&mut c, &["r1", "1k", "4k", "1k"]).unwrap();
    assert_eq!(p.point_count(), 4);
    let vt = (1.38064852e-23 / 1.6021766208e-19) * 300.15;
    let mut previous = f64::INFINITY;
    for (row, supplied) in [1e3, 2e3, 3e3, 4e3].iter().enumerate() {
        let v = p.value("v(d)", row).unwrap().re;
        let diode = 1e-14 * ((v / vt).exp() - 1.) + 1e-12 * v;
        close((1. - v) / supplied, diode, 1e-8, 1e-12); // the physical law at this R
        close(p.value("i(v1)", row).unwrap().re, -diode, 1e-8, 1e-12);
        assert!(
            v < previous,
            "a larger series R must lower the diode voltage"
        );
        previous = v;
    }
}

#[test]
fn deck_dc_cards_with_resistor_targets_use_the_existing_ast_grammar() {
    let n = netlist(
        "v1 a 0 2\nr1 a b 1k\nr2 b 0 1k\n.param lo=500 hi=1500\n.dc r1 {lo} {hi} 500\n.dc temp 27 47 20\n.dc r1 500 1000 500 r2 1k 2k 1k",
    );
    let config = RunConfig::from_netlist(&n).unwrap();
    let mut results = vec![];
    for card in &n.analyses {
        let mut c = config.circuit(&n).unwrap();
        let request = config.request_for(card).unwrap();
        results.push(
            runner(card.kind)
                .unwrap()
                .run(&mut c, &request, &config.context())
                .unwrap(),
        );
    }
    assert_column(
        &results[0],
        "v(b)",
        &[2. * 1e3 / 1.5e3, 1., 2. * 1e3 / 2.5e3],
        1e-12,
        1e-15,
    );
    assert_eq!(results[1].point_count(), 2);
    assert_eq!(results[2].point_count(), 4);
}

// ---- targets identified by physics, not names -----------------------------------

fn programmatic() -> Circuit {
    let mut c = Circuit::new();
    let a = c.add_node("a");
    let b = c.add_node("b");
    // `v9` is a resistor, `r9` a voltage source, `rc` a capacitor.
    c.add_device(Box::new(Resistor::new("v9", [a, b], 1e3).unwrap()))
        .unwrap();
    c.add_device(Box::new(
        IndependentSource::new(
            "r9",
            [a, NodeId::GROUND],
            true,
            1.,
            Complex::ZERO,
            Waveform::Constant(1.),
        )
        .unwrap(),
    ))
    .unwrap();
    c.add_device(Box::new(
        Resistor::new("Load", [b, NodeId::GROUND], 2e3).unwrap(),
    ))
    .unwrap();
    c.add_device(Box::new(
        Capacitor::new("rc", [b, NodeId::GROUND], 1e-6, None).unwrap(),
    ))
    .unwrap();
    c.finalize().unwrap();
    c
}

#[test]
fn programmatic_names_resolve_by_physical_kind_and_keep_their_case() {
    let mut c = programmatic();
    let resolved = |c: &Circuit, name: &str| {
        resolve(
            c,
            &AnalysisRequest::with_arguments(AnalysisKind::DcSweep, [name, "1", "2", "1"]),
            &AnalysisContext::default(),
        )
    };
    assert_eq!(
        resolved(&c, "v9").unwrap()[0].target,
        SweepTarget::Resistor("v9".into())
    );
    assert_eq!(
        resolved(&c, "r9").unwrap()[0].target,
        SweepTarget::VoltageSource("r9".into())
    );
    assert_eq!(
        resolved(&c, "load").unwrap()[0].target,
        SweepTarget::Resistor("Load".into())
    );
    assert!(resolved(&c, "rc").is_err()); // starts with r, is a capacitor
    let p = run(&mut c, &["v9", "1k", "3k", "1k"]).unwrap();
    let want: Vec<_> = [1e3, 2e3, 3e3].iter().map(|r| 2e3 / (r + 2e3)).collect();
    assert_column(&p, "v(b)", &want, 1e-12, 1e-15);
    assert_column(
        &p,
        "i(r9)",
        &[-1. / 3e3, -1. / 4e3, -1. / 5e3],
        1e-12,
        1e-15,
    );
    let p = run(&mut c, &["r9", "0", "2", "1", "load", "1k", "3k", "2k"]).unwrap();
    assert_eq!(p.variables.last().unwrap().name, "sweep(Load)");
    let want = [
        (0., 1e3),
        (1., 1e3),
        (2., 1e3),
        (0., 3e3),
        (1., 3e3),
        (2., 3e3),
    ]
    .map(|(v, r)| v * r / (1e3 + r));
    assert_column(&p, "v(b)", &want, 1e-12, 1e-15);
}

// ---- helper devices ------------------------------------------------------------

/// A linear 1 mS device to ground that counts operator assemblies and accepted points.
#[derive(Debug)]
struct Probe {
    node: NodeId,
    assembled: Rc<Cell<usize>>,
    accepted: Rc<Cell<usize>>,
}
impl Device for Probe {
    fn name(&self) -> &str {
        "x2"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        std::slice::from_ref(&self.node)
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        context.stamp(self.node, self.node, 1e-3)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        self.assembled.set(self.assembled.get() + 1);
        context.nodal([self.node, NodeId::GROUND], 1e-3, false)
    }
    fn accept(&self, _: &AcceptContext<'_>) -> SpiceResult<()> {
        self.accepted.set(self.accepted.get() + 1);
        Ok(())
    }
}
fn probe(c: &mut Circuit, node: &str) -> (Rc<Cell<usize>>, Rc<Cell<usize>>) {
    let (assembled, accepted) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let node = c.nodes().get(node).unwrap();
    c.add_device(Box::new(Probe {
        node,
        assembled: assembled.clone(),
        accepted: accepted.clone(),
    }))
    .unwrap();
    c.finalize().unwrap();
    (assembled, accepted)
}

/// A nonlinear-flagged 1 mS device that fails numerically when |v| exceeds `limit`,
/// as a stand-in for a point the nonlinear solver cannot converge.
#[derive(Debug)]
struct Gate {
    node: NodeId,
    limit: f64,
    accepted: Rc<Cell<usize>>,
}
impl Device for Gate {
    fn name(&self) -> &str {
        "x1"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        std::slice::from_ref(&self.node)
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let v = context.node_voltage(self.node);
        if v.abs() > self.limit {
            return Err(SpiceError::Numerical {
                context: "gate".into(),
                message: format!("{v} V is outside the device's range"),
            });
        }
        context.stamp(self.node, self.node, 1e-3)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        context.nodal([self.node, NodeId::GROUND], 1e-3, false)
    }
    fn accept(&self, _: &AcceptContext<'_>) -> SpiceResult<()> {
        self.accepted.set(self.accepted.get() + 1);
        Ok(())
    }
}
fn gated(body: &str, limit: f64) -> (Circuit, Rc<Cell<usize>>) {
    let mut c = circuit(body);
    let accepted = Rc::new(Cell::new(0));
    let node = c.nodes().get("a").unwrap();
    c.add_device(Box::new(Gate {
        node,
        limit,
        accepted: accepted.clone(),
    }))
    .unwrap();
    c.finalize().unwrap();
    (c, accepted)
}

// ---- factor reuse boundary --------------------------------------------------------

#[test]
fn source_only_linear_sweeps_reuse_one_factor_but_resistor_and_temperature_axes_do_not() {
    let make = || {
        let mut c = circuit("v1 a 0 1\nr1 a b 1k\nr2 b 0 1k");
        let (assembled, _) = probe(&mut c, "b");
        (c, assembled)
    };
    let count = |args: &[&str]| {
        let (mut c, assembled) = make();
        run(&mut c, args).unwrap();
        assembled.get()
    };
    // Source-only: the operator is assembled exactly twice (one probe assembly
    // in `resolve`, then one factor reused for every right-hand side).
    let few = count(&["v1", "0", "1", "0.5"]);
    let many = count(&["v1", "0", "2", "0.25"]);
    assert_eq!(few, many);
    assert_eq!(few, 2);
    // Resistor axis: one assembly per extra point.
    let few = count(&["r1", "1k", "3k", "1k"]);
    let many = count(&["r1", "1k", "6k", "1k"]);
    assert_eq!(many - few, 3);
    // Temperature axis: the probe and every Cartesian point, re-assembled once
    // for the preflight context check and once for the solved point.
    let few = count(&["temp", "0", "10", "5"]);
    let many = count(&["temp", "0", "20", "5"]);
    assert_eq!(few, 1 + 3 + 3);
    assert_eq!(many, 1 + 5 + 5);
}

// ---- restoration and failure ---------------------------------------------------------

#[test]
fn successful_sweeps_leave_sources_resistors_models_and_the_ast_unchanged() {
    let body = "v1 a 0 1\nv2 b 0 2\nr1 a b rm scale=2 m=4\nr2 b 0 1k\n.model rm r(r=1k tc1=0.01)";
    let n = netlist(body);
    let before = n.clone();
    let mut c = Circuit::from_netlist_with_context(&n, &hot().model_context()).unwrap();
    let reference = op(&mut c, &hot());
    let supplied = c.resistor("r1").unwrap().1;
    let operator = c
        .linear_system_with_context(&hot().model_context())
        .unwrap();
    let sweeps: [&[&str]; 4] = [
        &["v1", "0", "1", "0.5"],
        &["r1", "1k", "3k", "1k"],
        &["r2", "1k", "2k", "1k", "temp", "0", "50", "25"],
        &["v1", "0", "1", "0.5", "v2", "0", "1", "1"],
    ];
    for args in sweeps {
        run_in(&mut c, args, &hot()).unwrap();
        assert_eq!(op(&mut c, &hot()), reference, "{args:?}");
        assert_eq!(c.resistor("r1").unwrap().1, supplied);
        let again = c
            .linear_system_with_context(&hot().model_context())
            .unwrap();
        for (r, col) in [(0, 0), (0, 1), (1, 1)] {
            assert_eq!(again.a.get(r, col), operator.a.get(r, col));
        }
        assert_eq!(again.sources.len(), operator.sources.len());
        for (a, b) in again.sources.iter().zip(&operator.sources) {
            assert_eq!((a.dc, a.name.as_str()), (b.dc, b.name.as_str()));
        }
    }
    assert_eq!(n, before);
}

#[test]
fn a_mid_run_nonlinear_failure_accepts_no_failed_point_and_restores_everything() {
    // v(a) = v1/2 until the gate's 0.7 V limit: v1 = 0, 0.5, 1.0 pass, 1.5 fails.
    let (mut c, accepted) = gated("v1 in 0 0\nr1 in a 1k", 0.7);
    let reference = op(&mut c, &AnalysisContext::default());
    let error = run(&mut c, &["v1", "0", "2", "0.5"]).unwrap_err();
    assert!(matches!(error, SpiceError::Numerical { .. }), "{error}");
    assert_eq!(
        accepted.get(),
        3 + 1,
        "three swept points plus the reference op"
    );
    assert_eq!(op(&mut c, &AnalysisContext::default()), reference);
    // The same through a resistor target: v(a) = 2*1k/(R + 1k) passes at 3k and 2k only
    // (the deck's own R is 3k, so the reference operating point is within range).
    let (mut c, accepted) = gated("v1 in 0 2\nr1 in a 3k", 0.7);
    let reference = op(&mut c, &AnalysisContext::default());
    let supplied = c.resistor("r1").unwrap().1;
    assert!(run(&mut c, &["r1", "3k", "1k", "-1k"]).is_err());
    assert_eq!(accepted.get(), 2 + 1);
    assert_eq!(c.resistor("r1").unwrap().1, supplied);
    assert_eq!(op(&mut c, &AnalysisContext::default()), reference);
    // A later run on the same circuit is unaffected by the earlier failure.
    let p = run(&mut c, &["r1", "3k", "2k", "-1k"]).unwrap();
    assert_column(&p, "v(a)", &[0.5, 2. * 1e3 / 3e3], 1e-8, 1e-12);
}

#[test]
fn sweep_points_are_seeded_from_the_previous_accepted_solution() {
    // A soft exponential (N*Vt = 0.5 V) in series with 1 ohm: v(a) grows with the
    // source up to several volts, so a *cold* solve of the last point needs far
    // more damped iterations than the budget, while a ramp whose every point
    // starts from the previous accepted point only ever moves it a little.
    let body = "v1 in 0 0\nr1 in a 1\nd1 a 0 dm\n.model dm d(is=1e-3 n=19.34)";
    let ctx = AnalysisContext::default();
    // The request-argument spelling is `maxiter`; deck `.options itl1=N` maps to it
    // (see DC_CONTINUATION.md), and both continuations are explicitly disabled.
    let tight = ["maxiter=10", "gminsteps=0", "srcsteps=0"];
    let mut c = circuit(body);
    let mut args = vec!["v1", "0", "10", "0.5"];
    args.extend(tight);
    let p = run_in(&mut c, &args, &ctx).unwrap();
    assert_eq!(p.point_count(), 21);
    // The final point is the same physical solution as an unconstrained cold solve.
    let mut reference = circuit(body);
    let want = run_in(&mut reference, &["v1", "10", "10", "1"], &ctx)
        .unwrap()
        .value("v(a)", 0)
        .unwrap()
        .re;
    assert!(want > 4.0 && want < 4.6, "{want}");
    close(p.value("v(a)", 20).unwrap().re, want, 1e-9, 1e-12);
    // That same point as a sweep's *first* point has no warm seed and needs more
    // iterations than the budget allows, so the ramp is doing the work.
    let mut cold = circuit(body);
    let mut args = vec!["v1", "10", "10", "1"];
    args.extend(tight);
    let error = run_in(&mut cold, &args, &ctx).unwrap_err();
    assert!(
        error.to_string().contains("iteration limit"),
        "the tight budget must not solve this point from a cold start: {error}"
    );
}

// ---- rejection before samples --------------------------------------------------------

fn rejected(body: &str, args: &[&str], context: &AnalysisContext) -> SpiceError {
    let mut c =
        Circuit::from_netlist_with_context(&netlist(body), &context.model_context()).unwrap();
    let (_, accepted) = probe(&mut c, "a");
    let error = run_in(&mut c, args, context).unwrap_err();
    assert!(
        !matches!(error, SpiceError::Numerical { .. }),
        "{args:?} was rejected by a mid-run numerical failure, not up front: {error}"
    );
    assert_eq!(
        accepted.get(),
        0,
        "{args:?} accepted a point before being rejected"
    );
    error
}

#[test]
fn unsupported_targets_and_arities_are_rejected_before_any_sample() {
    let ctx = AnalysisContext::default();
    let body = "v1 a 0 1\nr1 a b 1k\nc1 b 0 1u\nl1 b c 1m\nr2 c 0 1k\nd1 a 0 dm\n.model dm d";
    for target in [
        "nope", "c1", "l1", "d1", "dm", "r1.r", "r1.m", "v(a)", "tc1", "i(v1)", "0",
    ] {
        let error = rejected(body, &[target, "1", "2", "1"], &ctx);
        assert!(
            error.to_string().contains("unsupported target"),
            "{target}: {error}"
        );
    }
    for args in [
        vec![],
        vec!["v1"],
        vec!["v1", "0", "1"],
        vec!["v1", "0", "1", "1", "r1"],
        vec!["v1", "0", "1", "1", "r1", "1", "2"],
        vec![
            "v1", "0", "1", "1", "r1", "1", "2", "1", "r2", "1", "2", "1",
        ],
        vec!["v1", "abc", "1", "1"],
        vec!["v1", "0", "1", "1e999"],
        vec!["r1", "1", "2", "1", "R1", "1", "2", "1"],
        vec!["v1", "0", "1", "1", "V1", "0", "1", "1"],
        vec!["temp", "0", "1", "1", "TEMP", "0", "1", "1"],
    ] {
        rejected(body, &args, &ctx);
    }
}

#[test]
fn invalid_resistor_and_temperature_values_are_rejected_before_any_sample() {
    let ctx = AnalysisContext::default();
    let body = "v1 a 0 1\nr1 a 0 1k";
    for args in [
        ["r1", "-1", "1", "1"],     // passes through zero ohms
        ["r1", "0", "1", "1"],      // starts at zero ohms
        ["r1", "2", "1", "1"],      // wrong direction
        ["r1", "1", "2", "0"],      // zero step
        ["r1", "1", "1e6", "1e-3"], // too many points
        ["temp", "-300", "0", "100"],
    ] {
        rejected(body, &args, &ctx);
    }
    // 1e10 * scale 1e300 overflows only at the last resistor point, so the model-backed
    // value is validated across the whole grid up front.
    let error = rejected(
        "v1 a 0 1\nr1 a 0 rm scale=1e300\n.model rm r(r=1)",
        &["r1", "1", "1e10", "9999999999"],
        &ctx,
    );
    assert!(error.to_string().contains("overflow"), "{error}");
    // The temperature factor 1 - 0.03*dT goes nonpositive only at the last temperature.
    rejected(
        "v1 a 0 1\nr1 a 0 rm\n.model rm r(r=1k tc1=-0.03)",
        &["temp", "27", "67", "20"],
        &ctx,
    );
    // ... and a resistor/temperature product catches the invalid corner too.
    rejected(
        "v1 a 0 1\nr1 a 0 rm\n.model rm r(r=1k tc1=-0.03)",
        &["r1", "1k", "2k", "1k", "temp", "27", "67", "20"],
        &ctx,
    );
}

// ---- product and work limits ------------------------------------------------------------

#[test]
fn point_and_product_limits_are_enforced_at_resolution() {
    let c = circuit("v1 a 0 0\nr1 a 0 1k");
    let axes = |args: &[&str]| {
        resolve(
            &c,
            &AnalysisRequest::with_arguments(AnalysisKind::DcSweep, args.iter().copied()),
            &AnalysisContext::default(),
        )
    };
    // 1000 x 100 is exactly the budget; one more outer point is not.
    assert_eq!(
        axes(&["v1", "0", "999", "1", "r1", "1", "100", "1"])
            .unwrap()
            .len(),
        2
    );
    assert!(axes(&["v1", "0", "999", "1", "r1", "1", "101", "1"]).is_err());
    assert!(axes(&["v1", "0", "99999", "1"]).is_ok());
    assert!(axes(&["v1", "0", "100000", "1"]).is_err());
    // Through the driver, over-budget products fail before any point is solved.
    let (mut c, accepted) = gated("v1 in 0 0\nr1 in a 1k", 10.);
    assert!(run(&mut c, &["v1", "0", "399", "1", "r1", "1", "400", "1"]).is_err());
    assert!(run(&mut c, &["v1", "0", "100000", "1"]).is_err());
    assert_eq!(accepted.get(), 0);
}
