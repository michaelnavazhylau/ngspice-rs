//! Linear production analyses against analytic MNA results.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::AnalysisKind;
use std::path::Path;

fn circuit(body: &str) -> Circuit {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("test.cir"),
            &format!("test\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&netlist).unwrap()
}
fn run(
    c: &mut Circuit,
    kind: AnalysisKind,
    args: &[&str],
) -> ngspice_rs::primitives::SpiceResult<Plot> {
    runner(kind)?.run(
        c,
        &AnalysisRequest::with_arguments(kind, args.iter().copied()),
        &AnalysisContext::default(),
    )
}
fn close(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs() + 1e-15,
        "{got} != {want}"
    );
}

#[test]
fn divider_branch_sign_and_ground_elimination() {
    let mut c = circuit("v1 in 0 5\nr1 in out 1k\nr2 out 0 1k");
    let p = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
    close(p.value("v(in)", 0).unwrap().re, 5.);
    close(p.value("v(out)", 0).unwrap().re, 2.5);
    close(p.value("i(v1)", 0).unwrap().re, -0.0025);
    assert!(p.variable_index("v(0)").is_none());
    assert_eq!(c.branch_rows(0), Some(2..3));
    // Rebinding after a new node shifts branch rows, without stale device state.
    let n = c.add_node("later");
    c.add_device(Box::new(
        ngspice_rs::devices::Resistor::new("r3", [n, ngspice_rs::primitives::NodeId::GROUND], 1.)
            .unwrap(),
    ))
    .unwrap();
    let p = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
    close(p.value("v(out)", 0).unwrap().re, 2.5);
    assert_eq!(c.branch_rows(0), Some(3..4));
}
#[test]
fn source_orientation_inductor_dc_and_independent_blocks() {
    let mut c = circuit("i1 0 a 1m\nr1 a 0 2k\nv1 b 0 3\nr2 b c 1k\nl1 c 0 1m\nc1 a 0 1u");
    let p = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
    close(p.value("v(a)", 0).unwrap().re, 2.);
    close(p.value("v(c)", 0).unwrap().re, 0.);
    close(p.value("i(l1)", 0).unwrap().re, 0.003);
    close(p.value("i(v1)", 0).unwrap().re, -0.003);
}
#[test]
fn ideal_source_loops_and_floating_dc_are_errors() {
    for body in [
        "v1 a 0 1\nv2 a 0 1",
        "v1 a 0 0\nv2 a 0 0",
        "r1 a b 1k",
        "c1 a 0 1u",
    ] {
        assert!(
            run(&mut circuit(body), AnalysisKind::OperatingPoint, &[]).is_err(),
            "{body}"
        );
    }
}
#[test]
fn dc_sweep_reuses_factors_without_mutating_sources() {
    let mut c = circuit("v1 a 0 5\nr1 a 0 1k");
    let p = run(&mut c, AnalysisKind::DcSweep, &["v1", "2", "0", "-1"]).unwrap();
    assert_eq!(p.point_count(), 3);
    close(p.value("i(v1)", 1).unwrap().re, -0.001);
    close(
        run(&mut c, AnalysisKind::OperatingPoint, &[])
            .unwrap()
            .value("v(a)", 0)
            .unwrap()
            .re,
        5.,
    );
    for args in [
        ["v1", "0", "1", "0"],
        ["r1", "0", "1", "1"],
        ["v1", "0", "1", "-1"],
        ["v1", "0", "1", "1p"],
    ] {
        assert!(run(&mut c, AnalysisKind::DcSweep, &args).is_err());
    }
}
fn waveform(c: &mut Circuit, w: ngspice_rs::devices::Waveform) {
    let t = c.devices()[0].terminals();
    let t = [t[0], t[1]];
    c.devices_mut()[0] = Box::new(
        ngspice_rs::devices::IndependentSource::new(
            "v1",
            t,
            true,
            0.,
            ngspice_rs::primitives::Complex::real(1.),
            w,
        )
        .unwrap(),
    );
}
fn transient(c: &mut Circuit) -> ngspice_rs::primitives::SpiceResult<Plot> {
    run(
        c,
        AnalysisKind::Transient,
        &["0.0001", "0.006", "0", "0.00005"],
    )
}

#[test]
fn rl_branch_dynamics_have_the_correct_sign() {
    let mut c = circuit("v1 in 0 0\nr1 in out 10\nl1 out 0 0.01");
    waveform(
        &mut c,
        ngspice_rs::devices::Waveform::Step {
            before: 0.,
            after: 1.,
            time: 0.001,
        },
    );
    let p = transient(&mut c).unwrap();
    for i in 0..p.point_count() {
        let t = p.value("time", i).unwrap().re;
        let want = if t <= 0.001 {
            0.
        } else {
            0.1 * (1. - (-(t - 0.001) / 0.001).exp())
        };
        // Companion trapezoid truncation error after the step (worst 7.3e-6 A on the 0.1 A scale;
        // the removed BDF backend held 2e-6).
        assert!((p.value("i(l1)", i).unwrap().re - want).abs() < 1.5e-5);
        assert!((p.value("i(v1)", i).unwrap().re + want).abs() < 1.5e-5);
    }
}

#[test]
fn rlc_underdamped_reference_and_pwl_breakpoints() {
    // Series R/L, shunt C: damping=500/s, natural frequency=1000/s.
    let mut c = circuit("v1 in 0 0\nr1 in mid 1000\nl1 mid out 1\nc1 out 0 1u");
    waveform(
        &mut c,
        ngspice_rs::devices::Waveform::Step {
            before: 0.,
            after: 1.,
            time: 0.001,
        },
    );
    let p = transient(&mut c).unwrap();
    for i in 0..p.point_count() {
        let t = (p.value("time", i).unwrap().re - 0.001).max(0.);
        let w = 750_000_f64.sqrt();
        let want = 1. - (-500. * t).exp() * ((w * t).cos() + 500. / w * (w * t).sin());
        // Companion trapezoid truncation error after the step (worst 1.3e-4 V;
        // the removed BDF backend held 2e-5).
        assert!((p.value("v(out)", i).unwrap().re - want).abs() < 3e-4);
    }
    let mut c = circuit("v1 in 0 0\nr1 in out 1k\nc1 out 0 1u");
    waveform(
        &mut c,
        ngspice_rs::devices::Waveform::Pwl(vec![(0., 0.), (0.001, 1.), (0.002, 1.)]),
    );
    let p = transient(&mut c).unwrap();
    for i in 0..p.point_count() {
        let t = p.value("time", i).unwrap().re;
        let want = if t <= 0.001 {
            t / 0.001 - 1. + (-t / 0.001).exp()
        } else {
            1. - (1. - (-1_f64).exp()) * (-(t - 0.001) / 0.001).exp()
        };
        // Companion trapezoid truncation error (worst 8.0e-5 V).
        assert!((p.value("v(out)", i).unwrap().re - want).abs() < 2e-4);
    }
}

#[test]
fn complex_ac_divider_rc_and_rlc_gain_phase() {
    for body in [
        "v1 in 0 dc 0 ac 1\nr1 in out 1k\nr2 out 0 1k",
        "v1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u",
        "v1 in 0 dc 0 ac 1\nr1 in mid 1000\nl1 mid out 1\nc1 out 0 1u",
    ] {
        let p = run(
            &mut circuit(body),
            AnalysisKind::Ac,
            &["dec", "3", "1", "1k"],
        )
        .unwrap();
        for i in 0..p.point_count() {
            let w = 2. * std::f64::consts::PI * p.value("frequency", i).unwrap().re;
            let want = if body.contains("r2") {
                ngspice_rs::primitives::Complex::real(0.5)
            } else if body.contains("l1") {
                ngspice_rs::primitives::Complex::real(1.)
                    / ngspice_rs::primitives::Complex::new(1. - w * w * 1e-6, w * 0.001)
            } else {
                ngspice_rs::primitives::Complex::real(1.)
                    / ngspice_rs::primitives::Complex::new(1., w * 0.001)
            };
            let got = p.value("v(out)", i).unwrap();
            assert!((got - want).magnitude() < 1e-10 * want.magnitude() + 1e-12);
        }
    }
}

#[test]
fn elaboration_rejects_unimplemented_parameters() {
    let deck = parse_deck_text(
        Path::new("test.cir"),
        "test\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u\n.tran 0.1m 1m\n.end\n",
    );
    let netlist = Parser::new().parse_deck(&deck).unwrap();
    let request = AnalysisRequest::from(&netlist.analyses[0]);
    let mut c = Circuit::from_netlist(&netlist).unwrap();
    assert!(
        runner(request.kind)
            .unwrap()
            .run(&mut c, &request, &AnalysisContext::default())
            .is_ok()
    );
    let deck = parse_deck_text(Path::new("test.cir"), "test\nr1 a 0 1k tc1=1\n.end\n");
    assert!(Circuit::from_netlist(&Parser::new().parse_deck(&deck).unwrap()).is_err());
    let mut nodes = ngspice_rs::primitives::NodeTable::new();
    let card = ngspice_rs::netlist::RawCard::parse(&deck.lines[0]).unwrap();
    assert!(
        ngspice_rs::devices::Registry::with_builtins()
            .instantiate(&card, &mut nodes)
            .is_err()
    );
    assert!(nodes.is_empty());
}

/// Records accepted times; delegates equations to a resistor.
#[derive(Debug)]
struct AcceptProbe {
    inner: ngspice_rs::devices::Resistor,
    times: std::rc::Rc<std::cell::RefCell<Vec<Option<f64>>>>,
}
impl ngspice_rs::devices::Device for AcceptProbe {
    fn name(&self) -> &str {
        ngspice_rs::devices::Device::name(&self.inner)
    }
    fn designator(&self) -> char {
        'r'
    }
    fn terminals(&self) -> &[ngspice_rs::primitives::NodeId] {
        ngspice_rs::devices::Device::terminals(&self.inner)
    }
    fn stamp(
        &self,
        context: &mut ngspice_rs::devices::StampContext<'_>,
    ) -> ngspice_rs::primitives::SpiceResult<()> {
        self.inner.stamp(context)
    }
    fn assemble_linear(
        &self,
        context: &mut ngspice_rs::devices::LinearContext<'_>,
    ) -> ngspice_rs::primitives::SpiceResult<()> {
        self.inner.assemble_linear(context)
    }
    fn accept(
        &self,
        context: &ngspice_rs::devices::AcceptContext<'_>,
    ) -> ngspice_rs::primitives::SpiceResult<()> {
        self.times.borrow_mut().push(context.time);
        Ok(())
    }
}

#[test]
fn accept_sees_the_initial_point_and_every_accepted_step_and_dc_has_no_time() {
    let mut c = circuit("v1 in 0 0\nr1 in out 1k\nc1 out 0 1u");
    waveform(
        &mut c,
        ngspice_rs::devices::Waveform::Step {
            before: 0.,
            after: 1.,
            time: 0.001,
        },
    );
    let times = std::rc::Rc::default();
    let t = c.devices()[1].terminals();
    c.devices_mut()[1] = Box::new(AcceptProbe {
        inner: ngspice_rs::devices::Resistor::new("r1", [t[0], t[1]], 1e3).unwrap(),
        times: std::rc::Rc::clone(&times),
    });
    let p = transient(&mut c).unwrap();
    let times: Vec<f64> = times.borrow().iter().map(|t| t.unwrap()).collect();
    assert_eq!(times[0], 0.);
    assert!(times.windows(2).all(|w| w[1] >= w[0]));
    assert_eq!(*times.last().unwrap(), 0.006);
    // The companion driver accepts exactly the points it reports.
    assert_eq!(times.len(), p.point_count(), "{times:?}");
    // DC analyses accept without a time.
    let mut c = circuit("v1 in 0 1\nr1 in 0 1k");
    let times = std::rc::Rc::default();
    let t = c.devices()[1].terminals();
    c.devices_mut()[1] = Box::new(AcceptProbe {
        inner: ngspice_rs::devices::Resistor::new("r1", [t[0], t[1]], 1e3).unwrap(),
        times: std::rc::Rc::clone(&times),
    });
    run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
    assert_eq!(*times.borrow(), vec![None]);
}

fn step(c: &mut Circuit) {
    waveform(
        c,
        ngspice_rs::devices::Waveform::Step {
            before: 0.,
            after: 1.,
            time: 0.001,
        },
    );
}

/// Coupled capacitances with a nondiagonal, nonsingular mass block.
#[test]
fn coupled_capacitance_network_matches_its_modal_solution() {
    let mut c = circuit("v1 in 0 0\nr1 in a 1k\nc1 a 0 1u\nc12 a b 2u\nc2 b 0 1u\nr2 b 0 1k");
    step(&mut c);
    let p = transient(&mut c).unwrap();
    // E^-1 G has modes (1,1) at 1000/s and (1,-1) at 200/s.
    for i in 0..p.point_count() {
        let t = p.value("time", i).unwrap().re;
        let (want_a, want_b) = if t < 0.001 {
            (0., 0.)
        } else {
            let (fast, slow) = ((-1e3 * (t - 0.001)).exp(), (-200. * (t - 0.001)).exp());
            (1. - 0.5 * (fast + slow), 0.5 * (slow - fast))
        };
        let a = p.value("v(a)", i).unwrap().re;
        let b = p.value("v(b)", i).unwrap().re;
        // Companion trapezoid truncation error (worst ~2e-5 V; the removed BDF
        // backend held 2e-5 with margin).
        assert!((a - want_a).abs() < 1e-4, "t={t} v(a)={a} want {want_a}");
        assert!((b - want_b).abs() < 1e-4, "t={t} v(b)={b} want {want_b}");
    }
}
