//! Production analysis temperatures and literal/model-backed equivalence.
use spice_analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use spice_core::{AnalysisKind, Complex, NodeId, SpiceResult};
use spice_devices::{Circuit, IndependentSource, Waveform};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

fn circuit(body: &str) -> Circuit {
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("passive-analysis.cir"),
            &format!("passives\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&n).unwrap()
}
fn run(
    circuit: &mut Circuit,
    kind: AnalysisKind,
    args: &[&str],
    context: &AnalysisContext,
) -> SpiceResult<Plot> {
    runner(kind)?.run(
        circuit,
        &AnalysisRequest::with_arguments(kind, args.iter().copied()),
        context,
    )
}
fn close(got: f64, want: f64, relative: f64, absolute: f64) {
    assert!(
        got.is_finite() && (got - want).abs() <= relative * want.abs() + absolute,
        "{got} != {want}"
    );
}
fn equal(got: &Plot, want: &Plot, ac: bool) {
    assert_eq!(got.point_count(), want.point_count());
    assert_eq!(got.variable_count(), want.variable_count());
    for var in &want.variables {
        for point in 0..want.point_count() {
            let a = got.value(&var.name, point).unwrap();
            let b = want.value(&var.name, point).unwrap();
            let (rtol, atol) = if ac { (1e-10, 1e-12) } else { (1e-12, 1e-15) };
            close(a.re, b.re, rtol, atol);
            close(a.im, b.im, rtol, atol);
        }
    }
}

#[test]
fn scalar_models_multiplicity_and_scale_match_literal_dc_sweep_and_ac() {
    let models = "v1 in 0 1 ac 1\nr1 in out rm scale=4 m=2\nc1 out 0 cm scale=0.5 m=3\nl1 out mid lm scale=0.5 m=2\nr2 mid 0 1k\n.model rm r(r=1k)\n.model cm c(cap=2u)\n.model lm l(ind=8m)";
    let literal = "v1 in 0 1 ac 1\nr1 in out 2k\nc1 out 0 3u\nl1 out mid 2m\nr2 mid 0 1k";
    for (kind, args) in [
        (AnalysisKind::OperatingPoint, vec![]),
        (AnalysisKind::DcSweep, vec!["v1", "-1", "1", "0.5"]),
        (AnalysisKind::Ac, vec!["dec", "3", "1", "1meg"]),
    ] {
        let mut a = circuit(models);
        let mut b = circuit(literal);
        let context = AnalysisContext::default();
        equal(
            &run(&mut a, kind, &args, &context).unwrap(),
            &run(&mut b, kind, &args, &context).unwrap(),
            kind == AnalysisKind::Ac,
        );
        assert_eq!(a.branch_rows(3), b.branch_rows(3));
        assert_eq!(a.unknown_count(), b.unknown_count());
    }
}

#[test]
fn geometry_models_match_hand_computed_literal_operating_point_and_ac() {
    let models = "v1 in 0 1 ac 1\nr1 in out rm\nc1 out 0 cm l=4u w=2u\nr2 out 0 1k\n.model rm r(rsh=100 l=4u defw=2u)\n.model cm c(cj=1meg cjsw=1m)";
    let literal = "v1 in 0 1 ac 1\nr1 in out 200\nc1 out 0 8.012u\nr2 out 0 1k";
    for (kind, args) in [
        (AnalysisKind::OperatingPoint, vec![]),
        (AnalysisKind::Ac, vec!["lin", "9", "1", "1000"]),
    ] {
        let context = AnalysisContext::default();
        equal(
            &run(&mut circuit(models), kind, &args, &context).unwrap(),
            &run(&mut circuit(literal), kind, &args, &context).unwrap(),
            kind == AnalysisKind::Ac,
        );
    }
}

#[test]
fn repeated_analysis_contexts_adjust_without_cached_or_cumulative_temperature() {
    let body = "v1 in 0 1 ac 1\nr1 in out rm\nc1 out 0 cm\nl1 out mid lm\nr2 mid 0 1k\n.model rm r(r=1k tc1=0.01 tc2=0.0001)\n.model cm c(cap=1u tc1=0.002)\n.model lm l(ind=1m tc1=0.004)";
    let mut model = circuit(body);
    for context in [
        AnalysisContext::default(),
        AnalysisContext {
            temperature: 77.0,
            nominal_temperature: 22.0,
        },
        AnalysisContext::default(),
    ] {
        let dt = context.temperature - context.nominal_temperature;
        let r = 1000.0 * (1.0 + 0.01 * dt + 0.0001 * dt * dt);
        let c = 1e-6 * (1.0 + 0.002 * dt);
        let l = 1e-3 * (1.0 + 0.004 * dt);
        let literal = format!(
            "v1 in 0 1 ac 1\nr1 in out {r:.17e}\nc1 out 0 {c:.17e}\nl1 out mid {l:.17e}\nr2 mid 0 1k"
        );
        for (kind, args) in [
            (AnalysisKind::OperatingPoint, vec![]),
            (AnalysisKind::DcSweep, vec!["v1", "0", "1", "0.5"]),
            (AnalysisKind::Ac, vec!["lin", "5", "10", "1000"]),
        ] {
            equal(
                &run(&mut model, kind, &args, &context).unwrap(),
                &run(&mut circuit(&literal), kind, &args, &context).unwrap(),
                kind == AnalysisKind::Ac,
            );
        }
    }
}

#[test]
fn explicit_temp_tnom_and_independent_instance_coefficient_overrides() {
    let context = AnalysisContext {
        temperature: 77.0,
        nominal_temperature: 22.0,
    };
    let mut c =
        circuit("v1 a 0 1\nr1 a 0 mdl temp=40 tc1=0\n.model mdl r(r=1k tc1=99 tc2=0.001 tnom=30)");
    let p = run(&mut c, AnalysisKind::OperatingPoint, &[], &context).unwrap();
    close(p.value("i(v1)", 0).unwrap().re, -1.0 / 1100.0, 1e-12, 1e-15);
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("hot-only.cir"),
            "hot\nr1 a 0 mdl\n.model mdl r(r=1k tc1=0.1 tnom=50)\n.end\n",
        ))
        .unwrap();
    assert!(Circuit::from_netlist(&netlist).is_err()); // Invalid factor at default 27.
    let mut hot = Circuit::from_netlist_with_context(&netlist, &context.model_context()).unwrap();
    let system = hot
        .linear_system_with_context(&context.model_context())
        .unwrap();
    close(system.a.get(0, 0), 1.0 / 3700.0, 1e-12, 1e-15);
}

#[test]
fn contextual_model_rc_transient_matches_the_analytic_step() {
    let context = AnalysisContext {
        temperature: 77.0,
        nominal_temperature: 27.0,
    };
    let mut c =
        circuit("r1 a 0 rm\nc1 a 0 cm\n.model rm r(r=1k tc1=0.01)\n.model cm c(cap=1u tc1=0.002)");
    let a = c.nodes().get("a").unwrap();
    c.add_device(Box::new(
        IndependentSource::new(
            "i1",
            [NodeId::GROUND, a],
            false,
            0.0,
            Complex::ZERO,
            Waveform::Step {
                before: 0.0,
                after: 1e-3,
                time: 0.0,
            },
        )
        .unwrap(),
    ))
    .unwrap();
    let p = run(
        &mut c,
        AnalysisKind::Transient,
        &["0.1m", "5m", "0", "0.05m", "backend=diffsol", "method=bdf"],
        &context,
    )
    .unwrap();
    for point in 0..p.point_count() {
        let time = p.value("time", point).unwrap().re;
        let expected = 1.5 * (1.0 - (-time / (1500.0 * 1.1e-6)).exp());
        close(p.value("v(a)", point).unwrap().re, expected, 0.0, 2e-5);
    }
}

#[test]
fn logarithmic_ac_preserves_integer_span_endpoints_without_adding_out_of_range_points() {
    for (mode, end, count, last) in [
        ("dec", "1000", 10, 1000.0),
        ("oct", "8", 10, 8.0),
        ("dec", "999", 9, 10f64.powf(8.0 / 3.0)),
    ] {
        let mut c = circuit("v1 a 0 1 ac 1\nr1 a 0 1k");
        let p = run(
            &mut c,
            AnalysisKind::Ac,
            &[mode, "3", "1", end],
            &AnalysisContext::default(),
        )
        .unwrap();
        assert_eq!(p.point_count(), count);
        close(
            p.value("frequency", count - 1).unwrap().re,
            last,
            1e-12,
            1e-15,
        );
    }
}

#[test]
fn invalid_runtime_temperatures_factors_and_ic_remain_explicit_failures() {
    let mut c = circuit("v1 a 0 1 ac 1\nr1 a 0 mdl\nc1 a 0 1u\n.model mdl r(r=1k tc1=-0.1)");
    let context = AnalysisContext {
        temperature: 77.0,
        nominal_temperature: 27.0,
    };
    for (kind, args) in [
        (AnalysisKind::OperatingPoint, vec![]),
        (AnalysisKind::DcSweep, vec!["v1", "0", "1", "1"]),
        (AnalysisKind::Ac, vec!["lin", "1", "10", "10"]),
        (
            AnalysisKind::Transient,
            vec!["1u", "1m", "backend=diffsol", "method=bdf"],
        ),
    ] {
        assert!(run(&mut c, kind, &args, &context).is_err());
        let invalid = AnalysisContext {
            temperature: f64::NAN,
            nominal_temperature: 27.0,
        };
        assert!(run(&mut c, kind, &args, &invalid).is_err());
    }
    assert!(
        run(
            &mut c,
            AnalysisKind::OperatingPoint,
            &[],
            &AnalysisContext::default()
        )
        .is_ok()
    );
    let mut ic = circuit("v1 a 0 1\nr1 a b 1k\nc1 b 0 mdl ic=1\n.model mdl c(cap=1u)");
    assert!(
        run(
            &mut ic,
            AnalysisKind::Transient,
            &["1u", "1m", "backend=diffsol", "method=bdf"],
            &AnalysisContext::default()
        )
        .unwrap_err()
        .to_string()
        .contains("ic=")
    );
}
