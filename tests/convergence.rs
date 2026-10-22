//! #106 convergence parity: ngspice's device voltage limiting and its `CKTop`
//! continuation schedules, exercised without the C binary.
//!
//! The C-parity evidence is the `m7_conv_*` goldens (`cargo xtask golden
//! verify`); these tests pin the mechanisms: `DEVpnjlim`/`DEVfetlim`/
//! `DEVlimvds`, the `MODEINITJCT` start, the limiting-aware convergence test
//! (a returned point is always an exact evaluation), `.options noopiter`, the
//! stage sequences of `dynamic_gmin`, `spice3_gmin`, `gillespie_src` and
//! `spice3_src`, and the explicit failure that names the unported `optran.c`
//! fallback.
use ngspice_rs::analysis::bias::{
    AttemptOutcome, DcOutcome, DcReport, DcSettings, DcStrategy, OPTRAN_NOT_PORTED, solve_dc_with,
};
use ngspice_rs::analysis::newton::StepLimiting;
use ngspice_rs::analysis::{AnalysisRequest, Plot, RunConfig, runner};
use ngspice_rs::devices::limiting::{critical_voltage, fetlim, limvds, pnjlim};
use ngspice_rs::devices::{Circuit, ModelContext};
use ngspice_rs::maths::Vector;
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::ast::Netlist;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::{AnalysisKind, SpiceResult};
use std::path::Path;

/// `kT/q` at 27 C with ngspice's constants.
const VT: f64 = (1.38064852e-23 / 1.6021766208e-19) * 300.15;
/// A 10 V source into a 1e-14 A diode through 1 ohm: from the zero seed a
/// plain Newton step lands the junction near 10 V, where `exp(v/vt)` overflows.
const HARD_DIODE: &str = "v1 a 0 dc 10\nr1 a b 1\nd1 b 0 dm\n.model dm d(is=1e-14)";
/// The cross-coupled BJT pair of the `m7_conv_latch_*` fixtures.
const LATCH: &str = "vcc vcc 0 dc 5
rc1 vcc c1 1k
rc2 vcc c2 1.2k
rb1 c1 b2 10k
rb2 c2 b1 10k
rx1 b1 0 4.7k
rx2 b2 0 4.7k
q1 c1 b1 0 qmod
q2 c2 b2 0 qmod
.model qmod npn(is=1e-14 bf=100)";

fn parse(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("convergence.cir"),
            &format!("convergence\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn request(arguments: &[&str]) -> AnalysisRequest {
    AnalysisRequest::with_arguments(AnalysisKind::OperatingPoint, arguments.iter().copied())
}

/// A DC solve of `body` with request `arguments`, and its report.
fn solve(body: &str, arguments: &[&str]) -> (SpiceResult<Vector>, DcReport) {
    let circuit = Circuit::from_netlist(&parse(body)).unwrap();
    let settings = DcSettings::from_request(&request(arguments)).unwrap();
    match solve_dc_with(
        &circuit,
        &ModelContext::default(),
        &settings,
        &[],
        None,
        None,
    ) {
        Ok(solved) => (Ok(solved.solution.values), solved.report),
        Err(failure) => (Err(failure.error), *failure.report),
    }
}

/// `.op` of `body` through the deck's options, as `spice-rs simulate` runs it.
fn operating_point(body: &str) -> Plot {
    let netlist = parse(body);
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let request = config.request(request(&[])).unwrap();
    runner(AnalysisKind::OperatingPoint)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}

fn value(plot: &Plot, name: &str) -> f64 {
    plot.value(name, 0).unwrap().re
}

fn attempts(report: &DcReport) -> Vec<(DcStrategy, AttemptOutcome)> {
    report
        .attempts
        .iter()
        .map(|attempt| (attempt.strategy, attempt.outcome))
        .collect()
}

fn assert_original_equations_solved(report: &DcReport) {
    let last = report.stages.last().unwrap();
    assert!(last.converged() && last.is_unregularized(), "{last:?}");
}

#[test]
fn pnjlim_compresses_forward_steps_and_bounds_reverse_ones() {
    let vcrit = critical_voltage(VT, 1e-14);
    // DIOtVcrit = vt ln(vt / (sqrt 2 Is)), about 0.73 V for 1e-14 A.
    assert!((vcrit - VT * (VT / (2f64.sqrt() * 1e-14)).ln()).abs() < 1e-15);
    assert!((0.7..0.75).contains(&vcrit), "{vcrit}");
    // From a non-positive voltage a step above vcrit lands at vt ln(v / vt).
    let step = pnjlim(10., 0., VT, vcrit);
    assert!(step.limited);
    assert!((step.value - VT * (10. / VT).ln()).abs() < 1e-15);
    // From a forward bias the step is logarithmic in the excess: a 9 V
    // request moves the junction by a few vt only.
    let step = pnjlim(10., 0.7, VT, vcrit);
    assert!(step.limited && step.value > 0.7 && step.value < 0.7 + 10. * VT);
    // Repeated limiting climbs monotonically and never overshoots.
    let mut v = 0.;
    for _ in 0..50 {
        let next = pnjlim(10., v, VT, vcrit).value;
        assert!(next >= v && next <= 10.);
        v = next;
    }
    // Small steps and steps below vcrit are untouched.
    assert!(!pnjlim(0.70, 0.69, VT, vcrit).limited);
    assert!(!pnjlim(0.5, -3., VT, vcrit).limited);
    // Gillespie's reverse limit: at most -(vold + 1) from a forward bias,
    // 2 vold - 1 from a reverse one.
    assert_eq!(pnjlim(-50., 0.6, VT, vcrit).value, -1.6);
    assert_eq!(pnjlim(-50., -2., VT, vcrit).value, -5.);
}

#[test]
fn fetlim_and_limvds_bound_gate_and_drain_steps() {
    let vto = 0.8;
    // An off device rising past threshold stops at vto + 0.5.
    assert_eq!(fetlim(5., 0., vto), vto + 0.5);
    // A strongly on device may not fall below vto + 2 in one step...
    assert_eq!(fetlim(0., 5., vto), vto + 2.);
    // ... nor rise by more than 2 |vold - vto| + 2.
    let vold = 5.;
    assert_eq!(fetlim(50., vold, vto), vold + 2. * (vold - vto) + 2.);
    // In the middle region the step is clamped to [vto - 0.5, vto + 4].
    assert_eq!(fetlim(-5., 1.5, vto), vto - 0.5);
    assert_eq!(fetlim(10., 1.5, vto), vto + 4.);
    // Drain-source: below 3.5 V the new value is clamped to [-0.5, 4].
    assert_eq!(limvds(10., 1.), 4.);
    assert_eq!(limvds(-10., 1.), -0.5);
    // Above 3.5 V growth is at most 3 vold + 2 and a fall stops at 2 V.
    assert_eq!(limvds(100., 5.), 17.);
    assert_eq!(limvds(0., 5.), 2.);
    assert_eq!(limvds(4., 5.), 4.);
}

#[test]
fn device_limiting_solves_an_overdriven_junction_directly_at_an_exact_point() {
    let (solution, report) = solve(HARD_DIODE, &[]);
    let x = solution.unwrap();
    assert_eq!(report.outcome, DcOutcome::Converged(DcStrategy::Direct));
    assert_eq!(
        attempts(&report),
        [(DcStrategy::Direct, AttemptOutcome::Converged)]
    );
    assert_original_equations_solved(&report);
    // C's itl1 default bounds the direct solve.
    assert!(report.stages[0].iterations <= 100, "{report:?}");
    // The returned point satisfies the unmodified KCL at the junction to
    // NIiter's accuracy: the limited loads never converge, so the last load
    // was exact, but NIiter returns that load's iterate (CKTrhsOld) once the
    // next update is within reltol 1e-3 / vntol 1e-6. C ngspice (SPARSE)
    // returns the same v(b) = 0.8909291905866813 bit for bit, where the diode
    // current exceeds the resistor's by 6.7e-6 relative.
    let circuit = Circuit::from_netlist(&parse(HARD_DIODE)).unwrap();
    let row = |name: &str| {
        circuit
            .unknowns()
            .node_row(circuit.nodes().get(name).unwrap())
            .unwrap()
    };
    let (va, vb): (f64, f64) = (x.as_slice()[row("a")], x.as_slice()[row("b")]);
    assert!((va - 10.).abs() < 1e-12);
    let diode = 1e-14 * ((vb / VT).exp() - 1.) + 1e-12 * vb;
    let resistor = va - vb;
    assert!(
        (diode - resistor).abs() <= 1e-5 * resistor,
        "{diode} vs {resistor}"
    );
    // The port's legacy global damping reaches the same root, through other
    // iterates: the two stop within NIiter's tolerance of it (here
    // 1.7e-7 V apart, the device-limited point being C's above).
    let (global, report) = solve(HARD_DIODE, &["limiting=global"]);
    assert_original_equations_solved(&report);
    let global = global.unwrap().as_slice()[row("b")];
    assert!((global - vb).abs() < 1e-6, "{global} vs {vb}");
}

#[test]
fn the_modeinitjct_load_is_never_a_converged_load() {
    // Seeded with the exact solution, a device-limited solve still spends
    // the MODEINITJCT load (junctions at vcrit, flagged nonconvergent) and at
    // least one exact load after it; global damping may stop at once.
    let circuit = Circuit::from_netlist(&parse(HARD_DIODE)).unwrap();
    let context = ModelContext::default();
    let direct = |arguments: &[&str]| {
        let mut settings = DcSettings::from_request(&request(arguments)).unwrap();
        settings.continuation = ngspice_rs::analysis::bias::ContinuationPolicy::disabled();
        settings
    };
    let exact = solve_dc_with(&circuit, &context, &direct(&[]), &[], None, None)
        .unwrap()
        .solution
        .values;
    let seeded = solve_dc_with(&circuit, &context, &direct(&[]), &[], Some(&exact), None).unwrap();
    assert!(
        seeded.report.stages[0].iterations >= 2,
        "{:?}",
        seeded.report
    );
    let settings = direct(&["limiting=global"]);
    assert_eq!(settings.newton.limiting, StepLimiting::Global);
    let global = solve_dc_with(&circuit, &context, &settings, &[], Some(&exact), None).unwrap();
    assert!(global.report.stages[0].iterations <= seeded.report.stages[0].iterations);
    for (a, b) in seeded
        .solution
        .values
        .as_slice()
        .iter()
        .zip(exact.as_slice())
    {
        assert!((a - b).abs() <= 1e-9 * b.abs().max(1.));
    }
}

#[test]
fn noopiter_skips_direct_newton_and_dynamic_gmin_starts_at_1e_3() {
    let (solution, report) = solve(LATCH, &["noopiter=1"]);
    solution.unwrap();
    assert_eq!(
        attempts(&report),
        [
            (DcStrategy::Direct, AttemptOutcome::Disabled),
            (DcStrategy::GminStepping, AttemptOutcome::Converged),
        ]
    );
    // dynamic_gmin: OldGmin = 1e-2 S divided by gminfactor = 10, from the
    // zero vector, down to the junction gmin, then the original equations.
    let first = &report.stages[0];
    assert_eq!((first.source_scale, first.gmin), (1., 1e-3));
    let gmins: Vec<f64> = report.stages.iter().map(|stage| stage.gmin).collect();
    assert!(gmins.windows(2).all(|pair| pair[1] <= pair[0]), "{gmins:?}");
    let before_last = &report.stages[report.stages.len() - 2];
    assert!((before_last.gmin - 1e-12).abs() < 1e-24, "{before_last:?}");
    assert_original_equations_solved(&report);
}

#[test]
fn spice3_gmin_walks_gmin_times_factor_powers_down_to_gmin() {
    let (solution, report) = solve(LATCH, &["noopiter=1", "gminsteps=4", "srcsteps=0"]);
    solution.unwrap();
    assert_eq!(
        report.outcome,
        DcOutcome::Converged(DcStrategy::GminStepping)
    );
    let gmins: Vec<f64> = report.stages.iter().map(|stage| stage.gmin).collect();
    let expected = [1e-8, 1e-9, 1e-10, 1e-11, 1e-12, 0.];
    assert_eq!(gmins.len(), expected.len(), "{gmins:?}");
    for (got, want) in gmins.iter().zip(expected) {
        assert!((got - want).abs() <= 1e-12 * want, "{gmins:?}");
    }
    assert!(report.stages.iter().all(|stage| stage.source_scale == 1.));
    assert_original_equations_solved(&report);
}

#[test]
fn spice3_src_steps_the_sources_in_equal_increments_without_gmin() {
    let (solution, report) = solve(LATCH, &["noopiter=1", "gminsteps=0", "srcsteps=4"]);
    solution.unwrap();
    assert_eq!(
        attempts(&report),
        [
            (DcStrategy::Direct, AttemptOutcome::Disabled),
            (DcStrategy::GminStepping, AttemptOutcome::Disabled),
            (DcStrategy::SourceStepping, AttemptOutcome::Converged),
        ]
    );
    let scales: Vec<f64> = report.stages.iter().map(|s| s.source_scale).collect();
    assert_eq!(scales, [0., 0.25, 0.5, 0.75, 1.]);
    assert!(report.stages.iter().all(|stage| stage.gmin == 0.));
    assert_original_equations_solved(&report);
}

#[test]
fn gillespie_src_ramps_the_sources_adaptively_from_zero() {
    let (solution, report) = solve(LATCH, &["noopiter=1", "gminsteps=0", "srcsteps=1"]);
    solution.unwrap();
    assert_eq!(
        report.outcome,
        DcOutcome::Converged(DcStrategy::SourceStepping)
    );
    let scales: Vec<f64> = report.stages.iter().map(|s| s.source_scale).collect();
    // All sources off first, then C's first raise of 1e-3, growing by 1.5
    // after quick stages, up to full sources.
    assert_eq!(scales[0], 0.);
    assert!((scales[1] - 1e-3).abs() < 1e-15, "{scales:?}");
    assert!(
        scales.windows(2).all(|pair| pair[1] >= pair[0]),
        "{scales:?}"
    );
    assert!(scales.len() > 5, "{scales:?}");
    assert_original_equations_solved(&report);
}

#[test]
fn the_latch_state_depends_on_the_schedule_as_in_ngspice() {
    // MODEINITJCT starts both junctions at vcrit, and dynamic gmin keeps the
    // symmetric start: the nearly balanced (metastable) point, as C finds.
    let default = operating_point(&format!("{LATCH}\n.options reltol=1e-8"));
    let (c1, c2) = (value(&default, "v(c1)"), value(&default, "v(c2)"));
    assert!(
        (c1 - 2.3073).abs() < 1e-3 && (c2 - 2.3770).abs() < 1e-3,
        "{c1} {c2}"
    );
    // spice3_src tips it into the stable state with q1 saturated.
    let stepped = operating_point(&format!(
        "{LATCH}\n.options reltol=1e-8 noopiter gminsteps=0 srcsteps=4"
    ));
    let (c1, c2) = (value(&stepped, "v(c1)"), value(&stepped, "v(c2)"));
    assert!(c1 < 0.1 && c2 > 4.5, "{c1} {c2}");
}

#[test]
fn exhausted_continuation_names_the_unported_optran_fallback() {
    // Every CKTop strategy disabled: C would go on to OPtran.
    let (solution, report) = solve(LATCH, &["noopiter=1", "gminsteps=0", "srcsteps=0"]);
    let error = solution.unwrap_err();
    assert_eq!(report.outcome, DcOutcome::Exhausted);
    assert!(report.stages.is_empty(), "{report:?}");
    let text = error.to_string();
    assert!(text.contains(OPTRAN_NOT_PORTED), "{text}");
    assert!(text.contains("src/spicelib/analysis/optran.c"), "{text}");
    // A deck's own options reach the same failure.
    let netlist = parse(&format!(
        "{LATCH}\n.options noopiter gminsteps=0 srcsteps=0"
    ));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let request = config.request(request(&[])).unwrap();
    let error = runner(AnalysisKind::OperatingPoint)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(error.to_string().contains("optran.c"), "{error}");
}

#[test]
fn noopiter_is_a_flag_and_the_limiting_key_is_validated() {
    let netlist = parse(&format!("{LATCH}\n.options noopiter"));
    assert!(RunConfig::from_netlist(&netlist).unwrap().dc().noopiter);
    assert!(RunConfig::from_netlist(&parse(&format!("{LATCH}\n.options noopiter=1"))).is_err());
    assert_eq!(
        DcSettings::from_request(&request(&[]))
            .unwrap()
            .newton
            .limiting,
        StepLimiting::Device
    );
    assert!(DcSettings::from_request(&request(&["limiting=sometimes"])).is_err());
    assert!(DcSettings::from_request(&request(&["noopiter=2"])).is_err());
}

/// The benchmark diode chain (`scripts/bench/bench.py`, `diode_chain`): `n`
/// stages of 100 ohm and a 1e-14 A diode from a 5 V source, 1 kohm load.
fn diode_chain(n: usize) -> String {
    let mut body = String::from("v1 n0 0 dc 5\n.model d1 d(is=1e-14 n=1)\n");
    for i in 0..n {
        body.push_str(&format!("r{i} n{i} m{i} 100\nd{i} m{i} n{} d1\n", i + 1));
    }
    body.push_str(&format!("rl n{n} 0 1k"));
    body
}

/// Long diode chains once failed `.op` where C converges (M10 0.5h). The
/// expected strategies, Newton totals (`rusage all`: "Total iterations") and
/// values are those of C ngspice built without FMA contraction, which the port
/// reproduces bit for bit. The default (FMA) C build differs from these values
/// at Newton-tolerance level only: the chain current is below ABSTOL.
#[test]
fn long_diode_chains_take_the_c_continuation_path() {
    // 100 stages: C's direct Newton converges in 7 iterations.
    let body = diode_chain(100);
    let (solution, report) = solve(&body, &[]);
    solution.unwrap();
    assert_eq!(
        attempts(&report),
        [(DcStrategy::Direct, AttemptOutcome::Converged)]
    );
    assert_eq!(report.total_iterations, 7);
    let plot = operating_point(&body);
    assert_eq!(value(&plot, "v(n1)"), 4.950001782213789);
    assert_eq!(value(&plot, "v(n100)"), 1.0911436345166466e-10);
    assert_eq!(value(&plot, "i(v1)"), -1.0909328995722944e-13);

    // 1000 stages: direct Newton exhausts ITL1, dynamic gmin stepping
    // converges ("Note: Dynamic gmin stepping completed"), 216 in total.
    let body = diode_chain(1000);
    let (solution, report) = solve(&body, &[]);
    solution.unwrap();
    assert_eq!(
        attempts(&report),
        [
            (DcStrategy::Direct, AttemptOutcome::Failed),
            (DcStrategy::GminStepping, AttemptOutcome::Converged),
        ]
    );
    assert_original_equations_solved(&report);
    let plot = operating_point(&body);
    let expected = [
        ("v(n1)", 4.9949340993026805),
        ("v(n1000)", 7.145072049497449e-12),
        ("i(v1)", -7.230327447871332e-15),
    ];
    if C_ORACLE_LIBM {
        assert_eq!(report.total_iterations, 216);
        for (name, c) in expected {
            assert_eq!(value(&plot, name), c, "{name}");
        }
    } else {
        // Another libm rounds exp/ln differently in the last bit, which moves
        // this rounding-sensitive chain's Newton path (glibc: 229
        // iterations) as FMA contraction moves C's: the point agrees to the
        // Newton tolerances (reltol 1e-3, vntol 1e-6 V, abstol 1e-12 A).
        for (name, c) in expected {
            let v = value(&plot, name);
            let bound = if name.starts_with("i(") {
                1e-12
            } else {
                1e-3 * c.abs() + 1e-6
            };
            assert!((v - c).abs() <= bound, "{name}: {v:e} vs C {c:e}");
        }
    }
}

/// Whether this target's libm is the one the C oracle values above were
/// captured with (macOS arm64): only then are bit-exact Newton paths of
/// rounding-sensitive decks expected.
const C_ORACLE_LIBM: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));
