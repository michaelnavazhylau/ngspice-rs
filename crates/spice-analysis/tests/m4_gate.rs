//! M4 bounded production gate: independent equations, conservation and ownership.
//! C-golden DC/AC/common-time comparisons live in `xtask golden verify`.
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use spice_core::AnalysisKind;
use spice_devices::{
    AnalysisMode, Circuit, Forcing, Limit, LoadRequest, ModelContext, TransientTiming,
};
use spice_maths::integrator::{DEFAULT_XMU, IntegrationMethod, StepHistory};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;
const VT: f64 = (1.38064852e-23 / 1.6021766208e-19) * 300.15;
fn circuit(body: &str) -> Circuit {
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("m4.cir"),
            &format!("M4\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&n).unwrap()
}
fn run(c: &mut Circuit, kind: AnalysisKind, args: &[&str]) -> spice_core::SpiceResult<Plot> {
    runner(kind)?.run(
        c,
        &AnalysisRequest::with_arguments(kind, args.iter().copied()),
        &AnalysisContext::default(),
    )
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}
#[test]
fn diode_dc_obeys_junction_law_kcl_and_explains_the_legacy_c_accuracy() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/netlists/diode_dc.cir");
    let n = Parser::new().parse_file(path).unwrap();
    let mut c = Circuit::from_netlist(&n).unwrap();
    let got = run(&mut c, AnalysisKind::DcSweep, &["v1", "0", "1", "0.25"]).unwrap();
    let raw =
        spice_analysis::RawFile::parse(include_str!("../../../conformance/golden/diode_dc.raw"))
            .unwrap();
    let want = &raw.plots[0].plot;
    let mut legacy_worst = 0_f64;
    for point in 0..got.point_count() {
        let vin = got.value("v(in)", point).unwrap().re;
        let v = got.value("v(out)", point).unwrap().re;
        let i = 1e-14 * ((v / VT).exp() - 1.) + 1e-12 * v;
        close((vin - v) / 1e3, i, 1e-8, 1e-12);
        close(got.value("i(v1)", point).unwrap().re, -i, 1e-8, 1e-12);
        let cv = want.value("v(out)", point).unwrap().re;
        let ci = want.value("i(v1)", point).unwrap().re;
        let ratio = (got.value("i(v1)", point).unwrap().re - ci).abs() / (ci.abs() + 1e-12);
        legacy_worst = legacy_worst.max(ratio);
        // C's legacy default-RELTOL/bypass result has less accurate voltages;
        // there is no reason to move the accurate Rust root towards this data.
        close(v, cv, 1e-3, 1e-6);
        close(-i, ci, 1e-3, 1e-12);
    }
    assert!(legacy_worst > 1e-4 && legacy_worst < 1e-3, "{legacy_worst}");
}
#[test]
fn bjt_forward_reverse_gain_kcl_and_polarity_are_physical() {
    for (family, pol) in [("npn", 1.), ("pnp", -1.)] {
        let mut c = circuit(&format!(
            "vc c 0 {}\nvb b 0 {}\nve e 0 0\nq1 c b e qm\n.model qm {family}(is=1e-14 bf=80 br=2)",
            pol * 2.,
            pol * 0.62
        ));
        let p = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
        let ibe = 1e-14 * ((0.62 / VT).exp() - 1.);
        let vbc = 0.62 - 2.;
        let arg = (3. * VT / (vbc * std::f64::consts::E)).powi(3);
        let ibc = -1e-14 * (1. + arg);
        // bjtload.c: the default-geometry substrate junction is gmin from the
        // grounded substrate to the collector (vertical NPN) or the base
        // (lateral PNP); that current leaves through ground.
        let (substrate_c, substrate_b) = if pol > 0. {
            (1e-12 * 2., 0.)
        } else {
            (0., 1e-12 * -0.62)
        };
        let ic = pol * (ibe - 1.5 * ibc - 1e-12 * vbc) + substrate_c;
        let ib = pol * (ibe / 80. + ibc / 2. + 1e-12 * (0.62 + vbc)) + substrate_b;
        close(-p.value("i(vc)", 0).unwrap().re, ic, 1e-8, 1e-12);
        close(-p.value("i(vb)", 0).unwrap().re, ib, 1e-8, 1e-12);
        close(
            p.value("i(vc)", 0).unwrap().re
                + p.value("i(vb)", 0).unwrap().re
                + p.value("i(ve)", 0).unwrap().re,
            -(substrate_c + substrate_b),
            0.,
            1e-15,
        );
    }
}
#[test]
fn mos1_cutoff_linear_saturation_reversal_and_polarity_follow_square_law() {
    for (family, pol) in [("nmos", 1.), ("pmos", -1.)] {
        for (vd, vg) in [(0.2_f64, 0.), (0.2, 2.), (2., 2.), (-0.2, 2.), (-2., 2.)] {
            let mut c = circuit(&format!(
                "vd d 0 {}\nvg g 0 {}\nvb bulk 0 {}\nm1 d g 0 bulk mm w=10u l=1u\n.model mm {family}(vto={} kp=1e-4 lambda=0.02)",
                pol * vd,
                pol * vg,
                pol * vd.min(0.),
                pol
            ));
            let p = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
            let vds = f64::abs(vd);
            let vgs = vg - vd.min(0.);
            let over = vgs - 1.;
            let channel = if over <= 0. {
                0.
            } else if vds >= over {
                0.5 * 1e-3 * over * over * (1. + 0.02 * vds)
            } else {
                1e-3 * vds * (over - 0.5 * vds) * (1. + 0.02 * vds)
            };
            let body = if vd < 0. {
                0.
            } else if vd >= 3. * VT {
                1e-14
            } else {
                -1e-14 * ((-vd / VT).exp() - 1.)
            };
            let expected = pol * (vd.signum() * channel + body + 1e-12 * vd.max(0.));
            close(-p.value("i(vd)", 0).unwrap().re, expected, 1e-8, 1e-12);
            close(p.value("i(vg)", 0).unwrap().re, 0., 0., 1e-15);
        }
    }
}
#[test]
fn nonlinear_charge_companion_conserves_charge_on_accepted_timepoints() {
    let mut c = circuit(
        "v1 in 0 pulse(0.2 0.4 10u 20u 20u 20u 100u)\nd1 in 0 dm\n.model dm d(is=1e-14 cjo=1u vj=1 m=0.5)",
    );
    let p = run(
        &mut c,
        AnalysisKind::Transient,
        &["1u", "50u", "0", "0.05u", "rtol=1e-5"],
    )
    .unwrap();
    let charge = |v: f64| 2e-6 * (1. - (1. - v).sqrt());
    let dynamic = |row: usize| {
        let v = p.value("v(in)", row).unwrap().re;
        -p.value("i(v1)", row).unwrap().re - (1e-14 * ((v / VT).exp() - 1.) + 1e-12 * v)
    };
    let mut integrated = 0.;
    for row in 1..p.point_count() {
        let dt = p.value("time", row).unwrap().re - p.value("time", row - 1).unwrap().re;
        assert!(dt > 0.);
        integrated += 0.5 * dt * (dynamic(row) + dynamic(row - 1));
    }
    let change = charge(p.value("v(in)", p.point_count() - 1).unwrap().re) - charge(0.2);
    close(integrated, change, 1e-4, 1e-14);
}
#[test]
fn all_device_charge_pairs_are_disposable_and_integrate_actual_nonlinear_q() {
    for body in [
        "v1 a 0 0.3\nd1 a 0 dm\n.model dm d(cjo=1u vj=1)",
        "vc c 0 2\nvb a 0 0.3\nq1 c a 0 qm\n.model qm npn(cje=1u cjc=2u tf=1n)",
        "vd d 0 2\nvg a 0 0.3\nm1 d a 0 0 mm\n.model mm nmos(vto=1 cbd=1u cbs=2u cgso=0.01 cgdo=0.02)",
    ] {
        let c = circuit(body);
        let context = ModelContext::default();
        let solved =
            spice_analysis::bias::solve_dc(&c, &context, &Default::default(), &[], None, None)
                .unwrap();
        let mut history = c.state_history();
        for _ in 0..spice_devices::ACCEPTED_DEPTH {
            history.commit(solved.trial.clone()).unwrap();
        }
        let before = history.clone();
        let mut x = solved.values;
        let row = c.unknowns().node_row(c.nodes().get("a").unwrap()).unwrap();
        x.as_mut_slice()[row] += 0.01;
        let dt = 1e-6;
        let coeff = StepHistory::new()
            .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
            .unwrap();
        let load = || {
            let mut trial = history.trial();
            c.load(
                &LoadRequest {
                    mode: AnalysisMode::Transient { time: dt, dt },
                    solution: &x,
                    model_context: &context,
                    integration: Some(&coeff),
                    history: &history,
                    forcing: Some(Forcing {
                        limit: Limit::Right,
                        timing: TransientTiming::new(dt, 10. * dt).unwrap(),
                    }),
                },
                &mut SparseMatrix::new(c.unknown_count(), c.unknown_count()),
                &mut Vector::zeros(c.unknown_count()),
                &mut trial,
            )
            .unwrap();
            trial
        };
        let first = load();
        let repeat = load();
        assert_eq!(first.values(), repeat.values());
        assert_eq!(history, before);
        for (index, device) in c.devices().iter().enumerate() {
            let base = c.state_rows(index).unwrap().start;
            for slot in device.truncation_slots() {
                let q = first.values()[base + slot];
                let old = history.accepted(1).unwrap()[base + slot];
                close(
                    first.values()[base + slot + 1],
                    (q - old) / dt,
                    1e-12,
                    1e-12,
                );
            }
        }
    }
}
#[test]
fn typed_nested_source_and_temperature_sweeps_are_bounded_and_nonmutating() {
    use spice_analysis::sweep::SweepTarget;
    // Programmatic names need not use SPICE's first-letter convention:
    // target types come from physical source metadata, never string prefixes.
    let mut named = circuit("r1 a 0 1k");
    let a = named.nodes().get("a").unwrap();
    for (name, voltage) in [("bias", true), ("drive", false)] {
        named
            .add_device(Box::new(
                spice_devices::IndependentSource::new(
                    name,
                    [a, spice_core::NodeId::GROUND],
                    voltage,
                    1.,
                    spice_core::Complex::ZERO,
                    spice_devices::Waveform::Constant(1.),
                )
                .unwrap(),
            ))
            .unwrap();
    }
    named.finalize().unwrap();
    for (name, expected) in [
        ("bias", SweepTarget::VoltageSource("bias".into())),
        ("drive", SweepTarget::CurrentSource("drive".into())),
    ] {
        let axes = spice_analysis::sweep::resolve(
            &named,
            &AnalysisRequest::with_arguments(AnalysisKind::DcSweep, [name, "0", "1", "0.5"]),
            &AnalysisContext::default(),
        )
        .unwrap();
        assert_eq!(axes[0].target, expected);
    }
    let mut c = circuit("v1 a 0 1\nv2 b 0 2\nr1 a b 1k");
    let p = run(
        &mut c,
        AnalysisKind::DcSweep,
        &["v1", "0", "1", "0.5", "v2", "1", "2", "1"],
    )
    .unwrap();
    assert_eq!(p.point_count(), 6);
    for row in 0..6 {
        let inner = (row % 3) as f64 * 0.5;
        let outer = 1. + (row / 3) as f64;
        close(
            p.value("i(v1)", row).unwrap().re,
            (outer - inner) / 1e3,
            1e-12,
            1e-15,
        );
        close(p.value("sweep(v2)", row).unwrap().re, outer, 0., 0.);
    }
    let original = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
    close(original.value("v(a)", 0).unwrap().re, 1., 0., 1e-15);
    // Resistor targets are supported (tests/dc_sweeps.rs); only invalid axes fail.
    for args in [
        vec!["v1", "0", "1", "0.5", "v1", "1", "2", "1"],
        vec!["v1", "0", "1", "0"],
        vec!["v1", "0", "1000", "0.1", "v2", "0", "1000", "0.1"],
    ] {
        assert!(run(&mut c, AnalysisKind::DcSweep, &args).is_err());
    }
    let mut temp = circuit("v1 a 0 1\nr1 a 0 rm 1k\n.model rm r(tc1=0.001)");
    let p = run(
        &mut temp,
        AnalysisKind::DcSweep,
        &["temp", "27", "37", "10"],
    )
    .unwrap();
    close(p.value("i(v1)", 1).unwrap().re, -1. / 1010., 1e-12, 1e-15);
}
#[test]
fn unsupported_physics_and_initialization_are_explicit_not_successful_zero_stamps() {
    for body in [
        "d1 a 0 dm\n.model dm d(vp=1 tt=1n)",
        // Gummel-Poon physics is ported (#87); quasi-saturation and excess
        // phase are not.
        "q1 c b 0 qm\n.model qm npn(rco=10)",
        "q1 c b 0 qm\n.model qm npn(tf=1n ptf=30)",
        "m1 d g 0 0 mm\n.model mm nmos(tox=10n)",
        "m1 d g 0 0 mm\n.model mm nmos(level=49)",
    ] {
        let n = Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("unsupported.cir"),
                &format!("unsupported\n{body}\n.end\n"),
            ))
            .unwrap();
        assert!(Circuit::from_netlist(&n).is_err());
    }
    let mut c = circuit("v1 a 0 0.3\nd1 a 0 dm\n.model dm d");
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("ic.cir"),
            "ic\nv1 a 0 0.3\nd1 a 0 dm\n.model dm d\n.ic v(a)=0.2\n.tran 1u 10u\n.end\n",
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&n).unwrap();
    let request = config.request_for(&n.analyses[0]).unwrap();
    assert!(
        runner(request.kind)
            .unwrap()
            .run(&mut c, &request, &config.context())
            .unwrap_err()
            .to_string()
            .contains("nonlinear companion .ic/uic")
    );
    assert!(
        run(
            &mut c,
            AnalysisKind::Transient,
            &["1u", "10u", "backend=diffsol", "method=bdf"]
        )
        .is_err()
    );
}
#[test]
fn transistor_small_signal_jacobians_are_derivatives_of_the_dc_equations() {
    for body in [
        "vc c 0 2\nvb b 0 0.62\nve e 0 0\nq1 c b e qm\n.model qm npn(is=1e-14 bf=80 br=2 cje=20p cjc=5p tf=1n)",
        "vc c 0 -2\nvb b 0 -0.62\nve e 0 0\nq1 c b e qm\n.model qm pnp(is=1e-14 bf=80 br=2 cje=20p cjc=5p tf=1n)",
        "vd c 0 0.2\nvg b 0 2\nvs e 0 0\nvb bulk 0 -0.5\nm1 c b e bulk mm w=10u l=1u\n.model mm nmos(vto=1 kp=1e-4 gamma=0.4 phi=0.6 lambda=0.02)",
        "vd c 0 -0.2\nvg b 0 -2\nvs e 0 0\nvb bulk 0 0.5\nm1 c b e bulk mm w=10u l=1u\n.model mm pmos(vto=-1 kp=1e-4 gamma=0.4 phi=0.6 lambda=0.02)",
    ] {
        let c = circuit(body);
        let context = ModelContext::default();
        let history = c.state_history();
        let bias =
            spice_analysis::bias::solve_dc(&c, &context, &Default::default(), &[], None, None)
                .unwrap()
                .values;
        let system = c.small_signal_system(&context, &bias).unwrap();
        let row = c.unknowns().node_row(c.nodes().get("c").unwrap()).unwrap();
        let residual = |x: &Vector| {
            let mut a = SparseMatrix::new(c.unknown_count(), c.unknown_count());
            let mut b = Vector::zeros(c.unknown_count());
            let mut trial = history.trial();
            c.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: x,
                    model_context: &context,
                    integration: None,
                    history: &history,
                    forcing: None,
                },
                &mut a,
                &mut b,
                &mut trial,
            )
            .unwrap();
            a.triplets()
                .iter()
                .filter(|t| t.row == row)
                .map(|t| t.value * x.as_slice()[t.col])
                .sum::<f64>()
                - b.as_slice()[row]
        };
        for node in c.nodes().nodes() {
            let Some(col) = c.unknowns().node_row(node.id) else {
                continue;
            };
            let h = 1e-6;
            let mut low = bias.clone();
            let mut high = bias.clone();
            low.as_mut_slice()[col] -= h;
            high.as_mut_slice()[col] += h;
            let numerical = (residual(&high) - residual(&low)) / (2. * h);
            let analytic = system
                .a
                .triplets()
                .iter()
                .filter(|t| t.row == row && t.col == col)
                .map(|t| t.value)
                .sum::<f64>();
            close(numerical, analytic, 1e-6, 1e-10);
        }
    }
}

#[test]
fn source_stepping_rescues_a_bounded_iteration_budget_without_changing_physics() {
    let c = circuit("i1 0 a 1m\nd1 a 0 dm\n.model dm d(is=1m)");
    let context = ModelContext::default();
    let history = c.state_history();
    let zero = Vector::zeros(c.unknown_count());
    let options = spice_analysis::newton::NewtonOptions {
        max_iterations: 4,
        ..Default::default()
    };
    let load = |x: &Vector, gmin: f64| {
        let mut a = SparseMatrix::new(1, 1);
        let mut b = Vector::zeros(1);
        let mut trial = history.trial();
        c.load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: x,
                model_context: &context,
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut a,
            &mut b,
            &mut trial,
        )?;
        a.add(0, 0, gmin)?;
        Ok::<_, spice_core::SpiceError>((a, b, trial))
    };
    // Both direct Newton and the first full-source gmin stage exhaust the
    // budget. Successful solve_dc therefore necessarily used source stepping.
    for gmin in [0., 1e-3] {
        let error = spice_analysis::newton::solve(&zero, &[false], &options, |x| load(x, gmin))
            .unwrap_err();
        assert!(error.to_string().contains("iteration limit"), "{error}");
    }
    let solved = spice_analysis::bias::solve_dc(&c, &context, &options, &[], None, None).unwrap();
    close(solved.values.as_slice()[0], VT * 2_f64.ln(), 1e-8, 1e-12);
    assert_eq!(history.depth(), 0);
}

#[test]
fn gmin_stepping_resolves_a_singular_initial_jacobian_without_accepting_trials() {
    #[derive(Debug)]
    struct Cubic {
        nodes: [spice_core::NodeId; 2],
        accepted: std::rc::Rc<std::cell::Cell<usize>>,
    }
    impl spice_devices::Device for Cubic {
        fn name(&self) -> &str {
            "x1"
        }
        fn designator(&self) -> char {
            'x'
        }
        fn terminals(&self) -> &[spice_core::NodeId] {
            &self.nodes
        }
        fn is_nonlinear(&self) -> bool {
            true
        }
        fn stamp(
            &self,
            context: &mut spice_devices::StampContext<'_>,
        ) -> spice_core::SpiceResult<()> {
            let [a, b] = self.nodes;
            let v = context.node_voltage(a) - context.node_voltage(b);
            let g = 3. * v * v;
            context.stamp(a, a, g)?;
            context.stamp(b, b, g)?;
            context.stamp(a, b, -g)?;
            context.stamp(b, a, -g)?;
            context.stamp_rhs(a, g * v - v * v * v)?;
            context.stamp_rhs(b, v * v * v - g * v)
        }
        fn assemble_small_signal(
            &self,
            context: &mut spice_devices::LinearContext<'_>,
            bias: &Vector,
        ) -> spice_core::SpiceResult<()> {
            let v = context
                .unknowns
                .node_row(self.nodes[0])
                .and_then(|r| bias.get(r))
                .unwrap_or(0.);
            context.nodal(self.nodes, 3. * v * v, false)
        }
        fn accept(&self, _: &spice_devices::AcceptContext<'_>) -> spice_core::SpiceResult<()> {
            self.accepted.set(self.accepted.get() + 1);
            Ok(())
        }
    }
    let mut c = circuit("i1 0 a 1");
    let a = c.nodes().get("a").unwrap();
    let accepted = std::rc::Rc::new(std::cell::Cell::new(0));
    c.add_device(Box::new(Cubic {
        nodes: [a, spice_core::NodeId::GROUND],
        accepted: accepted.clone(),
    }))
    .unwrap();
    c.finalize().unwrap();
    let solved = spice_analysis::bias::solve_dc(
        &c,
        &ModelContext::default(),
        &Default::default(),
        &[],
        None,
        None,
    )
    .unwrap();
    close(solved.values.as_slice()[0], 1., 1e-8, 1e-12);
    assert_eq!(accepted.get(), 0);
    c.accept_solution(&solved.values, None).unwrap();
    assert_eq!(accepted.get(), 1);
}

#[test]
fn continuation_never_accepts_a_regularized_nonunique_final_circuit() {
    let c = circuit("d1 a b dm\n.model dm d");
    assert!(
        spice_analysis::bias::solve_dc(
            &c,
            &ModelContext::default(),
            &Default::default(),
            &[],
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn new_m4_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in [
        "m4_diode_ac",
        "m4_diode_tran",
        "m4_bjt_ac",
        "m4_bjt_tran",
        "m4_mos1_ac",
        "m4_mos1_tran",
    ] {
        let n = Parser::new()
            .parse_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../conformance/netlists/{name}.cir")),
            )
            .unwrap();
        let text = spice_netlist::write_netlist(&n).unwrap();
        let round = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("round.cir"), &text))
            .unwrap();
        assert!(spice_netlist::semantic_eq(&n, &round));
        assert_eq!(text, spice_netlist::write_netlist(&round).unwrap());
    }
}
