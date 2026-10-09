//! Behavioural sources through the production analyses (GitHub #79):
//! analytic OP/DC/AC/transient results, `temper` in a temperature sweep,
//! large exact steps that Newton must not damp, and explicit failures.
//!
//! The C goldens `bsource_op`, `bsource_dc`, `bsource_ac`, `bsource_tran`,
//! `evalue_op`, `gtable_dc` and `epoly_dc` are compared by
//! `cargo xtask golden verify`.
use std::path::Path;

use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, SpiceResult};

fn circuit(body: &str) -> SpiceResult<Circuit> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("behavioural.cir"),
        &format!("behavioural\n{body}\n.end\n"),
    ))?;
    Circuit::from_netlist(&netlist)
}

fn run(body: &str, kind: AnalysisKind, args: &[&str]) -> SpiceResult<Plot> {
    runner(kind)?.run(
        &mut circuit(body)?,
        &AnalysisRequest::with_arguments(kind, args.iter().copied()),
        &AnalysisContext::default(),
    )
}

fn value(plot: &Plot, name: &str, point: usize) -> f64 {
    plot.value(name, point)
        .unwrap_or_else(|| panic!("no {name}"))
        .re
}

fn close(got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs() + 1e-12,
        "{got} != {want}"
    );
}

#[test]
fn a_nonlinear_operating_point_satisfies_the_device_equations() {
    // A diode-like B current from a 1 V source through 1k: the solved node
    // must satisfy KCL with the exponential exactly (to Newton tolerance).
    let plot = run(
        "vin in 0 dc 1\nr1 in a 1k\nb1 a 0 i=1e-14*(exp(v(a)/0.025852)-1)",
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap();
    let a = value(&plot, "v(a)", 0);
    let kcl = (1. - a) / 1e3 - 1e-14 * ((a / 0.025852).exp() - 1.);
    assert!(kcl.abs() < 1e-12, "KCL residual {kcl} at v(a) = {a}");
    assert!(a > 0.5 && a < 0.8, "{a}");
}

#[test]
fn large_exact_outputs_are_not_damped_to_small_steps() {
    // C applies no limiting to B sources: a 1000x gain on 5 V gives 5 kV in
    // one Newton step instead of thousands of 0.2 V steps, also next to a
    // junction (whose own node stays damped).
    let plot = run(
        "vin in 0 dc 5\nrin in 0 1k\nb1 o 0 v=1000*v(in)\nr1 o 0 1k\n\
         e1 o2 0 value={-20*v(in)}\nr2 o2 0 1k\nd1 o3 0 dmod\nr3 o 3 1meg\nr4 3 o3 1k\n\
         .model dmod d",
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap();
    close(value(&plot, "v(o)", 0), 5000., 1e-12);
    close(value(&plot, "v(o2)", 0), -100., 1e-12);
    // b1 also feeds 1 meg + 1k into the forward-biased diode.
    let diode = value(&plot, "v(o3)", 0);
    assert!(diode > 0.5 && diode < 0.9, "{diode}");
    close(
        value(&plot, "i(b1)", 0),
        -(5. + (5000. - diode) / 1.001e6),
        1e-9,
    );
}

#[test]
fn dc_sweeps_follow_the_expression_and_temper() {
    let plot = run(
        "vin in 0 dc 0\nrin in 0 1k\nb1 o 0 v=v(in)^2 + 2*v(in) - 1\nr1 o 0 1k",
        AnalysisKind::DcSweep,
        &["vin", "-2", "2", "0.5"],
    )
    .unwrap();
    for point in 0..plot.point_count() {
        let x = -2. + 0.5 * point as f64;
        close(value(&plot, "v(o)", point), x * x + 2. * x - 1., 1e-9);
    }
    // `temper` is the circuit temperature in Celsius; tc1 uses T - 27 C.
    let plot = run(
        "b1 o 0 v=temper\nr1 o 0 1k\nb2 p 0 v=1 tc1=0.01\nr2 p 0 1k",
        AnalysisKind::DcSweep,
        &["temp", "-23", "77", "50"],
    )
    .unwrap();
    for (point, temperature) in [-23., 27., 77.].into_iter().enumerate() {
        close(value(&plot, "v(o)", point), temperature, 1e-12);
        close(
            value(&plot, "v(p)", point),
            1. + 0.01 * (temperature - 27.),
            1e-12,
        );
    }
}

#[test]
fn ac_uses_the_bias_point_derivative() {
    let plot = run(
        "vin in 0 dc 0.5 ac 1\nrin in 0 1k\nb1 o 0 v=3*v(in)^2 + i(vin)*1k\nr1 o 0 1k\n\
         b2 0 p i=2m*exp(v(in))\nr2 p 0 1k\nc2 p 0 1u",
        AnalysisKind::Ac,
        &["dec", "2", "10", "1k"],
    )
    .unwrap();
    for point in 0..plot.point_count() {
        // d/dv(in) of 3 v^2 is 3, and the AC current of vin is -1 mA.
        let o = plot.value("v(o)", point).unwrap();
        close(o.re, 3. - 1., 1e-12);
        assert!(o.im.abs() < 1e-12);
        let f = value(&plot, "frequency", point);
        let omega = 2. * std::f64::consts::PI * f;
        let g = 2e-3 * 0.5_f64.exp();
        // g into 1k || 1 uF.
        let admittance = ngspice_rs::primitives::Complex::new(1e-3, omega * 1e-6);
        let want = ngspice_rs::primitives::Complex::new(g, 0.) / admittance;
        let p = plot.value("v(p)", point).unwrap();
        assert!(
            (p - want).magnitude() <= 1e-10 * want.magnitude(),
            "{p} {want}"
        );
    }
}

#[test]
fn hertz_re_solves_the_operating_point_at_every_ac_frequency() {
    // acan.c (CKTvarHertz): the bias, and so the linearisation, follows the
    // frequency. d/dv(in) of v(in)^2 hertz is 2 v(in) hertz = f at 0.5 V;
    // hertz is 0 in the operating point.
    let body = "vin in 0 dc 0.5 ac 1\nrin in 0 1k\nb1 o 0 v=v(in)^2*hertz + hertz/1k\nr1 o 0 1k";
    let plot = run(body, AnalysisKind::Ac, &["lin", "3", "100", "300"]).unwrap();
    for point in 0..3 {
        let f = value(&plot, "frequency", point);
        close(value(&plot, "v(o)", point), f, 1e-12);
    }
    let op = run(body, AnalysisKind::OperatingPoint, &[]).unwrap();
    close(value(&op, "v(o)", 0), 0., 1e-12);
}

#[test]
fn transient_time_functions_are_evaluated_at_each_accepted_time() {
    let plot = run(
        "b1 o 0 v=sin(2*pi*1k*time) + pwl(time, 0, 0, 0.5m, 1, 1m, 0)\nr1 o 0 1k\n\
         b2 0 p i=1m*v(o)^2\nr2 p 0 1k",
        AnalysisKind::Transient,
        &["10u", "1m"],
    )
    .unwrap();
    assert!(plot.point_count() > 50);
    for point in 0..plot.point_count() {
        let t = value(&plot, "time", point);
        let pwl = if t <= 0.5e-3 {
            t / 0.5e-3
        } else {
            (1e-3 - t) / 0.5e-3
        };
        let o = (2. * std::f64::consts::PI * 1e3 * t).sin() + pwl;
        close(value(&plot, "v(o)", point), o, 1e-9);
        close(value(&plot, "v(p)", point), o * o, 1e-9);
    }
}

#[test]
fn the_bdf_backend_rejects_behavioural_sources_explicitly() {
    let error = run(
        "b1 o 0 v=sin(2*pi*1k*time)\nr1 o 0 1k\nc1 o 0 1n",
        AnalysisKind::Transient,
        &["10u", "1m", "backend=diffsol", "method=bdf"],
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            ngspice_rs::primitives::SpiceError::Unsupported { .. }
                | ngspice_rs::primitives::SpiceError::NotYetPorted { .. }
        ),
        "{error}"
    );
}

#[test]
fn singular_slopes_at_the_zero_start_converge_in_every_analysis() {
    // At the 0 V Newton start 1/x, sqrt, log and division have C's ~1e32
    // PTdivide-fudge slopes and log() is -1e99; the first linearisation is
    // well posed but needs the balanced, refined fallback solve (#79).
    let body = "vin in 0 dc 2 ac 1\nrin in 0 1k\n\
                b1 o1 0 v=1/v(in)\nr1 o1 0 1k\n\
                b2 o2 0 v=sqrt(v(in))\nr2 o2 0 1k\n\
                b3 o3 0 v=log(v(in))\nr3 o3 0 1k\n\
                b4 o4 0 v=2/v(in,0)\nr4 o4 0 1k\n\
                b5 0 o5 i=1m*log10(v(in))\nr5 o5 0 1k";
    let expected = |v: f64| [1. / v, v.sqrt(), v.ln(), 2. / v, v.log10()];
    let check = |plot: &Plot, point: usize, v: f64| {
        for (index, want) in expected(v).into_iter().enumerate() {
            close(
                value(plot, &format!("v(o{})", index + 1), point),
                want,
                1e-12,
            );
        }
    };
    check(
        &run(body, AnalysisKind::OperatingPoint, &[]).unwrap(),
        0,
        2.,
    );
    let dc = run(body, AnalysisKind::DcSweep, &["vin", "0.5", "3", "0.5"]).unwrap();
    for point in 0..dc.point_count() {
        check(&dc, point, value(&dc, "v(in)", point));
    }
    let tran = run(body, AnalysisKind::Transient, &["1u", "10u"]).unwrap();
    check(&tran, 0, 2.);
    let ac = run(body, AnalysisKind::Ac, &["dec", "1", "1", "10"]).unwrap();
    // d/dv of 1/v, sqrt v, ln v and 2/v at 2 V.
    close(value(&ac, "v(o1)", 0), -0.25, 1e-12);
    close(value(&ac, "v(o2)", 0), 0.5 / 2_f64.sqrt(), 1e-12);
    close(value(&ac, "v(o3)", 0), 0.5, 1e-12);
    close(value(&ac, "v(o4)", 0), -0.5, 1e-12);
}

#[test]
fn infinite_slopes_at_the_zero_start_name_the_offending_derivative() {
    // pwr(0, -0.5) is infinite, so v(in)^0.5 has an infinite slope at the
    // 0 V start. C loads it and carries a NaN iterate; the port stops with
    // the value and the derivative it could not use.
    let error = run(
        "vin in 0 dc 2\nrin in 0 1k\nb1 o 0 v=v(in)^0.5\nr1 o 0 1k",
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("non-finite derivative inf (value 0) in '^'"),
        "{error}"
    );
}

#[test]
fn evaluation_failures_stop_the_analysis_with_the_function_name() {
    let error = run(
        "vin in 0 dc -1\nrin in 0 1k\nb1 o 0 v=sqrt(v(in) + 0.5)\nr1 o 0 1k",
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap_err();
    assert!(error.to_string().contains("sqrt"), "{error}");
}

#[test]
fn table_and_poly_forms_simulate_through_the_lowered_devices() {
    // The XSPICE pwl map is flat outside the table and linear between
    // corners; the POLY(1) source is c0 + c1 x + c2 x^2.
    let plot = run(
        "vin in 0 dc 0.25\nrin in 0 1k\ne1 o 0 table {v(in)} = (0,0) (1,2) (2,3)\nr1 o 0 1k\n\
         e2 p 0 poly(1) in 0 1 2 4\nr2 p 0 1k\ng1 0 q in 0 0 1m 1m m=2\nr3 q 0 1k",
        AnalysisKind::OperatingPoint,
        &[],
    )
    .unwrap();
    close(value(&plot, "v(o)", 0), 0.5, 1e-12);
    close(value(&plot, "v(p)", 0), 1. + 0.5 + 0.25, 1e-12);
    close(
        value(&plot, "v(q)", 0),
        2. * (0.25e-3 + 0.0625e-3) * 1e3,
        1e-12,
    );
    close(value(&plot, "v(e1_int2)", 0), 0.25, 1e-12);
    assert!(plot.variable_index("i(ae1)").is_some());
    assert!(plot.variable_index("i(a$poly$e2)").is_some());
}
