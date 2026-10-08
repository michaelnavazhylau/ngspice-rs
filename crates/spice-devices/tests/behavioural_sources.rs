//! Behavioural sources (GitHub #79): the compiled expression's values and
//! analytic derivatives for every `inpptree.c` function (finite-difference
//! checks), C's value quirks, the `ASRCload` stamps and the consistency of
//! the Newton residual with its Jacobian, and explicit errors.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use spice_core::{Real, SourceLoc, SpiceError};
use spice_devices::behavioural::program::{Environment, Program, Quantity};
use spice_devices::{AnalysisMode, Circuit, LoadRequest, ModelContext, Registry};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, RawCard, ast::Netlist, source::parse_deck_text};

fn origin() -> SourceLoc {
    SourceLoc::new(PathBuf::from("expr.cir"), 1, 1)
}

fn program(text: &str) -> Program {
    let parsed = Parser::new()
        .parse_behavioural_expression(text, &origin(), false)
        .unwrap_or_else(|error| panic!("{text}: {error}"));
    Program::compile(&parsed.root).unwrap_or_else(|error| panic!("{text}: {error}"))
}

fn values(program: &Program, point: &BTreeMap<&str, Real>) -> Vec<Real> {
    program
        .quantities()
        .iter()
        .map(|quantity| match quantity {
            Quantity::Node(name) | Quantity::Branch(name) => point[name.as_str()],
        })
        .collect()
}

fn environment(values: &[Real]) -> Environment<'_> {
    Environment {
        values,
        time: 1.25e-3,
        temperature: 27.,
        gmin: 1e-12,
        frequency: 50.,
    }
}

fn value(text: &str, point: &[(&str, Real)]) -> Real {
    let program = program(text);
    let point: BTreeMap<&str, Real> = point.iter().copied().collect();
    let values = values(&program, &point);
    program
        .evaluate(&environment(&values))
        .unwrap_or_else(|error| panic!("{text}: {error}"))
        .value
}

/// Central differences against the analytic gradient at every point.
fn assert_gradient(text: &str, points: &[&[(&str, Real)]]) {
    let program = program(text);
    for point in points {
        let point: BTreeMap<&str, Real> = point.iter().copied().collect();
        let x = values(&program, &point);
        let analytic = program
            .evaluate(&environment(&x))
            .unwrap_or_else(|error| panic!("{text} at {point:?}: {error}"));
        for (index, slope) in analytic.gradient.iter().enumerate() {
            let h = 1e-6 * x[index].abs().max(1.);
            let mut up = x.clone();
            up[index] += h;
            let mut down = x.clone();
            down[index] -= h;
            let f = |values: &[Real]| program.evaluate(&environment(values)).unwrap().value;
            let numeric = (f(&up) - f(&down)) / (2. * h);
            assert!(
                (numeric - slope).abs() <= 2e-6 * slope.abs().max(1.),
                "{text} at {point:?}, d/d{:?}: analytic {slope}, numeric {numeric}",
                program.quantities()[index]
            );
        }
    }
}

const POINTS: &[&[(&str, Real)]] = &[
    &[("a", 0.3), ("b", -0.7), ("vx", 2e-3)],
    &[("a", -0.45), ("b", 0.2), ("vx", -1.5e-3)],
    &[("a", 0.8), ("b", 0.55), ("vx", 0.5e-3)],
];

#[test]
fn every_function_has_a_derivative_matching_finite_differences() {
    for text in [
        // inpptree.c funcs[] in table order, each with an inner expression so
        // the chain rule is exercised.
        "abs(v(a) - 1.5*v(b))",
        "acos(0.9*v(a)*v(b))",
        "acosh(2 + v(a)^2 + v(b))",
        "asin(0.5*v(a) - 0.2*v(b))",
        "asinh(3*v(a)*v(b))",
        "atan(2*v(a) + v(b))",
        "atanh(0.6*v(a) + 0.3*v(b))",
        "cos(4*v(a) - v(b))",
        "cosh(v(a) + 2*v(b))",
        "exp(2*v(a) - v(b))",
        "ln(2 + v(a) + v(b))",
        "log(2 + v(a) + v(b))",
        "log10(2 + v(a)*v(b))",
        "sgn(v(a) - 5)*v(b)",
        "sin(3*v(a)*v(b))",
        "sinh(v(a) - v(b))",
        "sqrt(3 + v(a) + v(b))",
        "tan(v(a) + 0.5*v(b))",
        "tanh(2*v(a) - v(b))",
        "u(v(a) - 5) + u(v(a) + 5)*v(b)",
        "uramp(v(a) + 2*v(b) + 3)",
        "ceil(v(a)*7 + 0.5) * v(b)",
        "floor(v(a)*7 + 0.5) * v(b)",
        "nint(v(a)*7 + 0.25) * v(b)",
        "-v(a)*v(b)",
        "u2(0.3*v(a) + 0.5) + u2(v(b) + 5) + u2(v(b) - 5)",
        "pwl(v(a), -1, 2, 0, 0, 0.5, 1, 2, 3) * v(b)",
        "eq0(v(a)) + ne0(v(a)) + gt0(v(a)) + lt0(v(a)) + ge0(v(a)) + le0(v(a))",
        "pow(v(a) + 2, v(b) + 1)",
        "pow(v(a) + 2, 3)",
        "pow(2, v(b)*v(a))",
        "pow(v(a) - 3, 2)",
        "pwr(v(a) + 3, 2.5)",
        "pwr(v(a) + 2, v(b))",
        "min(v(a), v(b)) + max(2*v(a), v(b) + 1)",
        "ternary_fcn(v(a) > 0, v(a)^2, v(b)^3)",
        // Operators.
        "v(a) + v(b) - 2*v(a)*v(b)",
        "v(a) / (2 + v(b))",
        "(1 + v(a)) / v(b)",
        "(v(a) + 3)^(v(b) + 1)",
        "(v(a) + 3)^2.5",
        "(v(a) - 3)^2",
        "2^(v(a) - v(b))",
        "(v(a) + 2) ** 3",
        "v(a, b)^2 + v(b, a)",
        "v(a) > v(b) ? exp(v(a)) : cos(v(b))",
        "(v(a) < 0.5 && v(b) != 0) + (v(a) >= 0 || v(b) <= 0) + !(v(a) == v(b))",
        "i(vx)*1k + v(a)*i(vx)",
        "time*v(a) + temper*v(b) + hertz*v(a)*v(b)",
        "pi*v(a) + e*v(b)",
    ] {
        assert_gradient(text, POINTS);
    }
}

#[test]
fn values_follow_the_c_functions() {
    let at = |text: &str| value(text, &[("a", 0.)]);
    // PTdivide moves the divisor by gmin * 1e-20 away from zero.
    assert_eq!(at("1/v(a)"), 1e32);
    assert_eq!(at("-1/v(a)"), -1e32);
    // PTlog/PTlog10: log(0) is -1e99; PTexp caps above 227.9559242.
    assert_eq!(at("log(v(a))"), -1e99);
    assert_eq!(at("log10(v(a))"), -1e99);
    assert_eq!(at("exp(300 + v(a))"), 1e99);
    // PTustep, PTuramp, PTustep2, nearbyint.
    assert_eq!(at("u(v(a))"), 0.5);
    assert_eq!(at("uramp(v(a) - 1)"), 0.);
    assert_eq!(at("u2(v(a) + 0.25)"), 0.25);
    assert_eq!(at("nint(2.5 + v(a)) + nint(3.5)"), 6.);
    // PTpowerH: a negative base needs a quasi-integer exponent; pow(0, b)=0.
    assert_eq!(at("(v(a) - 2)^2.5"), 0.);
    assert_eq!(at("(v(a) - 2)^3"), -8.);
    assert_eq!(at("v(a)^0"), 1.);
    assert_eq!(at("pow(v(a), 0)"), 0.);
    assert_eq!(at("pwr(v(a) - 2, 2)"), -4.);
    // Comparisons, logic and the ternary select by != 0.
    assert_eq!(
        at("(v(a) < 1) + (v(a) <> 0) + !v(a) + (1 && v(a)) + (0 || 2)"),
        3.
    );
    assert_eq!(at("v(a) ? 1 : 2"), 2.);
    // min/max/pwl.
    assert_eq!(at("min(v(a), -1) + max(v(a), 3)"), 2.);
    assert_eq!(at("pwl(v(a) - 2, 0, 0, 1, 1, 3, 5)"), -2.);
    assert_eq!(at("pwl(v(a) + 4, 0, 0, 1, 1, 3, 5)"), 7.);
    // Constants, time and temper.
    assert_eq!(
        at("pi + e + v(a)"),
        std::f64::consts::PI + std::f64::consts::E
    );
    assert_eq!(at("time*1000 + temper + v(a)"), 1.25 + 27.);
    assert_eq!(at("hertz + v(a)"), 50.);
    // sin/cos/tan reduce their argument like C's MODULUS before evaluating.
    let x: Real = 100.;
    let reduced =
        x - Real::from((x / (2. * std::f64::consts::PI)) as i32) * 2. * std::f64::consts::PI;
    assert_eq!(at("sin(100 + v(a))"), reduced.sin());
}

#[test]
fn descending_pwl_keeps_cs_ascending_derivative_search() {
    // PTpwl interpolates a descending table correctly, but PTpwl_derivative
    // always searches as if the abscissas ascended, so at 0.3 it returns the
    // slope of the first segment (2,3)-(0.5,1), not of (0.5,1)-(0,0).
    let program = program("pwl(v(a), 2, 3, 0.5, 1, 0, 0, -1, 2)");
    let evaluation = program.evaluate(&environment(&[0.3])).unwrap();
    assert!((evaluation.value - 0.6).abs() < 1e-15);
    assert!((evaluation.gradient[0] - 4. / 3.).abs() < 1e-15);
}

#[test]
fn pwr_of_a_negative_base_keeps_cs_zero_derivative() {
    // PTdifferentiate writes d pwr(a, 2.5) as 2.5 pow(a, 1.5) a', and PTpower
    // of a negative base with a non-integer exponent is 0: C's Newton sees a
    // flat function there although pwr(-3.3, 2.5) = -19.78.
    let program = program("pwr(v(a) - 3, 2.5)");
    let evaluation = program.evaluate(&environment(&[-0.3])).unwrap();
    assert!((evaluation.value + 3.3_f64.powf(2.5)).abs() < 1e-12);
    assert_eq!(evaluation.gradient[0], 0.);
}

#[test]
fn odd_powers_of_a_negative_base_keep_cs_derivative_sign() {
    // PTdifferentiate assumes a^b = |a|^b and writes b pwr(a, b-1) a', but
    // PTpowerH evaluates (-0.45)^3 as pow(-0.45, 3) < 0: the derivative has
    // the wrong sign. The C binary's AC analysis of `v(in)**3` at -0.45 V
    // reports -0.6075, and so does the port.
    let program = program("v(a)**3");
    let evaluation = program.evaluate(&environment(&[-0.45])).unwrap();
    assert!((evaluation.value + 0.091_125).abs() < 1e-15);
    assert!((evaluation.gradient[0] + 0.6075).abs() < 1e-15);
}

#[test]
fn domain_errors_are_explicit_numerical_errors() {
    for text in [
        "sqrt(v(a) - 1)",
        "log(v(a) - 1)",
        "log10(v(a) - 1)",
        "acos(v(a) + 2)",
        "acosh(v(a))",
        "atanh(v(a) + 1)",
        "cosh(v(a) + 1000)",
    ] {
        let program = program(text);
        let values = vec![0.; program.quantities().len()];
        let error = program.evaluate(&environment(&values)).unwrap_err();
        assert!(
            matches!(error, SpiceError::Numerical { .. }),
            "{text}: {error}"
        );
    }
}

#[test]
fn unported_and_unknown_functions_are_rejected_at_compile_time() {
    let compile = |text: &str| {
        let parsed = Parser::new()
            .parse_behavioural_expression(text, &origin(), false)
            .unwrap();
        Program::compile(&parsed.root)
    };
    for text in ["ddt(v(a))", "gauss(1, 0.1, 3)"] {
        let error = compile(text).unwrap_err();
        assert!(error.is_not_yet_ported(), "{text}: {error}");
    }
    for (text, message) in [
        ("sqr(v(a))", "no such function 'sqr'"),
        ("sin(v(a), 2)", "takes 1 argument"),
        ("pow(v(a))", "takes 2 argument"),
        ("pwl(v(a), 0, 0, 1)", "even number"),
        ("pwl(v(a), 0, 0, 0, 1)", "monotonic"),
        ("pwl(v(a), 0, 0, 1, 1, 0.5, 2)", "monotonic"),
        ("pwl(v(a), 0, v(b), 1, 1)", "literal"),
        ("pwl_derivative(v(a))", "internal"),
        ("k*v(a)", "unresolved name 'k'"),
    ] {
        let error = compile(text).unwrap_err();
        assert!(error.to_string().contains(message), "{text}: {error}");
    }
}

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("behavioural.cir"),
            &format!("behavioural sources\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn circuit(body: &str) -> spice_core::SpiceResult<Circuit> {
    Circuit::from_netlist(&deck(body))
}

fn row(circuit: &Circuit, node: &str) -> usize {
    circuit
        .unknowns()
        .node_row(circuit.nodes().get(node).expect("node exists"))
        .expect("not ground")
}

fn branch(circuit: &Circuit, name: &str) -> usize {
    let index = circuit
        .devices()
        .iter()
        .position(|device| device.name() == name)
        .expect("device exists");
    circuit.branch_rows(index).unwrap().start
}

/// One operating-point load of `circuit` at `x`.
fn load(circuit: &Circuit, x: &Vector, context: &ModelContext) -> (SparseMatrix, Vector) {
    let n = circuit.unknown_count();
    let history = circuit.state_history();
    let mut trial = history.trial();
    let mut a = SparseMatrix::new(n, n);
    let mut b = Vector::zeros(n);
    circuit
        .load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: x,
                model_context: context,
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut a,
            &mut b,
            &mut trial,
        )
        .unwrap();
    a.fold_duplicates();
    (a, b)
}

fn residual(circuit: &Circuit, x: &Vector) -> Vec<Real> {
    let (a, b) = load(circuit, x, &ModelContext::default());
    let mut r: Vec<Real> = b.as_slice().iter().map(|v| -v).collect();
    for t in a.triplets() {
        r[t.row] += t.value * x.as_slice()[t.col];
    }
    r
}

#[test]
fn newton_residuals_are_consistent_with_their_jacobians() {
    // The linearised load A(x0) x0 - b(x0) is the physical residual, and its
    // finite-difference Jacobian is A(x0) for every row a B source touches.
    let c = circuit(
        "vin in 0 dc 1\nrin in 0 1k\nvx x 0 dc 0.5\nrx x 0 2k\n\
         b1 o1 0 v=exp(v(in,x))*i(vx) + sqrt(1 + v(o2)^2) m=2 tc1=1m temp=40\n\
         r1 o1 0 1k\n\
         b2 o2 o3 i=1m*tanh(v(o1)/2) + 2m*pwl(v(in), 0,0, 1,2, 3,1) reciprocm=1 m=4\n\
         r2 o2 0 1k\nr3 o3 0 3k\n\
         e1 o4 0 value={v(o1)*v(o3) + i(b1)*100}\nr4 o4 0 1k",
    )
    .unwrap();
    let n = c.unknown_count();
    let mut x = Vector::zeros(n);
    for (row, value) in x.as_mut_slice().iter_mut().enumerate() {
        *value = 0.3 + 0.17 * row as Real - 0.05 * (row % 3) as Real;
    }
    let (a, _) = load(&c, &x, &ModelContext::default());
    let dense = |row: usize, col: usize| a.get(row, col);
    for col in 0..n {
        let h = 1e-6 * x.as_slice()[col].abs().max(1.);
        let mut up = x.clone();
        up.as_mut_slice()[col] += h;
        let mut down = x.clone();
        down.as_mut_slice()[col] -= h;
        let (ru, rd) = (residual(&c, &up), residual(&c, &down));
        for row in 0..n {
            let numeric = (ru[row] - rd[row]) / (2. * h);
            let analytic = dense(row, col);
            assert!(
                (numeric - analytic).abs() <= 1e-6 * analytic.abs().max(1.),
                "A[{row}][{col}]: analytic {analytic}, numeric {numeric}"
            );
        }
    }
}

#[test]
fn voltage_and_current_outputs_stamp_like_asrcload() {
    let c = circuit(
        "vin in 0 dc 1\nrin in 0 1k\nb1 o 0 v=3*v(in)^2 m=2\nr1 o 0 1k\n\
         b2 p q i=5m*v(in)^3\nrp p 0 1k\nrq q 0 1k",
    )
    .unwrap();
    let n = c.unknown_count();
    let mut x = Vector::zeros(n);
    let (input, o, p, q) = (row(&c, "in"), row(&c, "o"), row(&c, "p"), row(&c, "q"));
    x.as_mut_slice()[input] = 0.5;
    let (a, b) = load(&c, &x, &ModelContext::default());
    let k = branch(&c, "b1");
    // v(o) - m f = m (f - f' x0): f = 0.75, f' = 3 at 0.5, m = 2.
    assert_eq!(a.get(o, k), 1.);
    assert_eq!(a.get(k, o), 1.);
    assert_eq!(a.get(k, input), -6.);
    assert!((b.as_slice()[k] - 2. * (0.75 - 1.5)).abs() < 1e-15);
    // The current 5m v^3 flows from p through the source to q.
    let g = 15e-3 * 0.25;
    assert!((a.get(p, input) - g).abs() < 1e-18);
    assert!((a.get(q, input) + g).abs() < 1e-18);
    let equivalent = 5e-3 * 0.125 - g * 0.5;
    assert!((b.as_slice()[p] + equivalent).abs() < 1e-18);
    assert!((b.as_slice()[q] - equivalent).abs() < 1e-18);
}

#[test]
fn the_ac_jacobian_is_the_bias_point_derivative() {
    let c = circuit("vin in 0 dc 0.5\nrin in 0 1k\nb1 o 0 v=3*v(in)^2\nr1 o 0 1k\nb2 0 p i=2m*exp(v(in))\nrp p 0 1k").unwrap();
    let mut bias = Vector::zeros(c.unknown_count());
    bias.as_mut_slice()[row(&c, "in")] = 0.5;
    let system = c
        .small_signal_system(&ModelContext::default(), &bias)
        .unwrap();
    let (input, p) = (row(&c, "in"), row(&c, "p"));
    assert_eq!(system.a.get(branch(&c, "b1"), input), -3.);
    assert!((system.a.get(p, input) + 2e-3 * 0.5_f64.exp()).abs() < 1e-18);
    assert!(system.e.triplets().is_empty());
}

#[test]
fn temperature_factor_follows_asrcload() {
    let scale = spice_devices::BehaviouralScale {
        m: 2.,
        tc1: 1e-2,
        tc2: 1e-4,
        temperature: None,
        dtemp: 3.,
        reciprocal_tc: false,
        reciprocal_m: false,
    };
    // d = 27 + 273.15 + 3 - 300.15 = 3.
    let factor = 2. * (1. + 3e-2 + 9e-4);
    assert!((scale.factor(27.) - factor).abs() < 1e-14);
    let reciprocal = spice_devices::BehaviouralScale {
        reciprocal_tc: true,
        reciprocal_m: true,
        temperature: Some(37.),
        dtemp: 0.,
        ..scale
    };
    assert!((reciprocal.factor(0.) - 1. / (1. + 0.1 + 0.01) / 2.).abs() < 1e-14);
}

#[test]
fn invalid_instances_are_explicit_errors() {
    for (body, message) in [
        ("b1 a a v=1\nr1 a 0 1", "shorted ASRC"),
        ("b1 a 0 v=i(vx)\nr1 a 0 1", "unknown controlling source vx"),
        // After a comma inp_meas_current() leaves i() alone, and a current
        // B source has no branch to sense.
        (
            "b1 b 0 v=max(0,i(b2))\nb2 c 0 i=1m\nr2 b 0 1\nr3 c 0 1",
            "no findable branch",
        ),
        (
            "b1 a 0 v=1 temp=30 dtemp=2\nr1 a 0 1",
            "both temp= and dtemp=",
        ),
        ("b1 a 0 v=k\nr1 a 0 1", "undefined parameter [k]"),
        ("b1 a 0 v=sqr(2)\nr1 a 0 1", "no such function 'sqr'"),
    ] {
        let error = circuit(body).unwrap_err();
        assert!(error.to_string().contains(message), "{body}: {error}");
    }
}

#[test]
fn the_registry_builds_b_sources_from_cards() {
    let registry = Registry::with_builtins();
    assert!(registry.get('b').is_some_and(|entry| entry.ported));
    let deck = parse_deck_text(Path::new("b.cir"), "t\nb1 a 0 v=2*v(c)+i(v1)\n");
    let card = RawCard::parse(&deck.lines[0]).unwrap();
    let mut nodes = spice_core::NodeTable::new();
    let device = registry.instantiate(&card, &mut nodes).unwrap();
    assert_eq!(device.designator(), 'b');
    assert_eq!(device.terminals().len(), 3);
    assert_eq!(device.branch_currents(), 1);
    assert_eq!(device.findable_branch(), Some(0));
    assert_eq!(device.controlling_sources()[0].name, "v1");
    assert!(device.is_nonlinear());
    assert!(!device.limits_voltage_steps());
}
