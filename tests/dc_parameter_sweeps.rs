//! #97: `.dc @instance[parameter]` sweeps (C `dctrcurv.c` `PARAM_CODE`)
//! against analytic solutions and equivalent typed targets, the explicit
//! rejection of what C does not sweep (model parameters, more than two axes)
//! or the port does not yet, and immutability of the swept circuit.
use ngspice_rs::analysis::sweep::{ParameterRoute, SweepTarget, resolve};
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::{Circuit, MAX_INSTANCE_OVERRIDES, ModelContext};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, SpiceError, SpiceResult};
use std::path::Path;

fn circuit(body: &str) -> Circuit {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("sweep.cir"),
            &format!("sweep\n{body}\n.end\n"),
        ))
        .unwrap();
    Circuit::from_netlist(&netlist).unwrap()
}
fn request(args: &str) -> AnalysisRequest {
    AnalysisRequest::with_arguments(AnalysisKind::DcSweep, args.split_whitespace())
}
fn run(c: &mut Circuit, args: &str) -> SpiceResult<Plot> {
    runner(AnalysisKind::DcSweep)?.run(c, &request(args), &AnalysisContext::default())
}
fn op(c: &mut Circuit) -> Plot {
    runner(AnalysisKind::OperatingPoint)
        .unwrap()
        .run(
            c,
            &AnalysisRequest::new(AnalysisKind::OperatingPoint),
            &AnalysisContext::default(),
        )
        .unwrap()
}
fn column(p: &Plot, name: &str) -> Vec<f64> {
    (0..p.point_count())
        .map(|row| p.value(name, row).unwrap().re)
        .collect()
}
fn close(got: f64, want: f64, relative: f64, absolute: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs() + absolute,
        "{got:e} != {want:e}"
    );
}
fn same_columns(a: &Plot, b: &Plot, names: &[&str], relative: f64) {
    assert_eq!(a.point_count(), b.point_count());
    for name in names {
        for (x, y) in column(a, name).iter().zip(column(b, name)) {
            close(*x, y, relative, 1e-15);
        }
    }
}
fn unsupported(error: &SpiceError) -> bool {
    matches!(error, SpiceError::Unsupported { .. })
}

const DIODE: &str = "i1 0 a 1m\nd1 a 0 dx area=1\n.model dx d(is=1e-14 n=1.2)";

#[test]
fn diode_area_follows_the_junction_law() {
    // An ideal junction forced by 1 mA: I = IS*AREA*(exp(V/(N Vt)) - 1).
    // N*Vt is taken from the AREA=1 point, so no constant is assumed.
    let mut c = circuit(DIODE);
    let plot = run(&mut c, "@d1[area] 1 8 0.5").unwrap();
    assert_eq!(plot.variables[0].name, "sweep");
    assert_eq!(plot.variables[0].unit, "parameter");
    let (current, is): (f64, f64) = (1e-3, 1e-14);
    let v = column(&plot, "v(a)");
    let nvt = v[0] / (current / is + 1.).ln();
    for (area, v) in column(&plot, "sweep").iter().zip(&v) {
        // gmin (1e-12 S) carries < 1e-12 A of the 1 mA.
        close(*v, nvt * (current / (is * area) + 1.).ln(), 1e-9, 0.);
    }
    assert_eq!(v.len(), 15);
    assert!(v.windows(2).all(|w| w[1] < w[0]));
}

#[test]
fn diode_m_area_pj_aliases_and_instance_temperatures() {
    let mut c = circuit(DIODE);
    let area = run(&mut c, "@d1[area] 1 3 1").unwrap();
    // AREA and M both multiply IS of a junction without RS, PJ or charge.
    same_columns(
        &area,
        &run(&mut c, "@d1[m] 1 3 1").unwrap(),
        &["v(a)"],
        1e-12,
    );
    // Keywords are case-insensitive; PERIM is PJ (`dio.c`).
    same_columns(
        &area,
        &run(&mut c, "@D1[AREA] 1 3 1").unwrap(),
        &["v(a)"],
        0.,
    );
    let perim = run(&mut c, "@d1[perim] 0 2 1").unwrap();
    assert_eq!(perim.point_count(), 3);
    // The diode is the only temperature-dependent device here, so its
    // instance TEMP, DTEMP and the circuit temperature are one sweep.
    let circuit_temp = run(&mut c, "temp -20 80 20").unwrap();
    same_columns(
        &circuit_temp,
        &run(&mut c, "@d1[temp] -20 80 20").unwrap(),
        &["v(a)"],
        1e-12,
    );
    same_columns(
        &circuit_temp,
        &run(&mut c, "@d1[dtemp] -47 53 20").unwrap(),
        &["v(a)"],
        1e-12,
    );
}

#[test]
fn controlled_gain_sources_and_resistance_routes() {
    let body = "v1 in 0 2\nr1 in 0 1k\ne1 eo 0 in 0 1\nre eo 0 1k\n\
                g1 0 go in 0 1m m=3\nrg go 0 1k\nf1 0 fo v1 1\nrf fo 0 1k";
    let mut c = circuit(body);
    let e = run(&mut c, "@e1[gain] -2 2 1").unwrap();
    for (gain, v) in column(&e, "sweep").iter().zip(column(&e, "v(eo)")) {
        close(v, 2. * gain, 1e-12, 1e-15);
    }
    // VCCSparam: a given m multiplies the swept gain.
    let g = run(&mut c, "@g1[gain] 1m 3m 1m").unwrap();
    for (gain, v) in column(&g, "sweep").iter().zip(column(&g, "v(go)")) {
        close(v, gain * 3. * 2. * 1e3, 1e-12, 1e-15);
    }
    // F senses i(v1) = -2 mA; nested with E so both replacements coexist.
    let f = run(&mut c, "@f1[gain] 1 2 1 @e1[gain] 1 2 1").unwrap();
    assert_eq!(column(&f, "sweep(@e1[gain])"), [1., 1., 2., 2.]);
    for (gain, v) in column(&f, "sweep").iter().zip(column(&f, "v(fo)")) {
        close(v, gain * -2e-3 * 1e3, 1e-12, 1e-15);
    }
    // `@v1[dc]`, `@i1[c]` and `@r1[r]` are the typed source/resistor sweeps.
    let mut c = circuit(
        "v1 in 0 2\ni1 0 out 1m\nr1 in out rm m=2\nr2 out 0 1k\n.model rm r(r=1k tc1=0.01)",
    );
    for (parameter, typed) in [
        ("@v1[dc] 1 3 1", "v1 1 3 1"),
        ("@i1[c] 0 2m 1m", "i1 0 2m 1m"),
        ("@I1[DC] 0 2m 1m", "i1 0 2m 1m"),
        (
            "@r1[r] 1k 2k 500 temp 27 47 20",
            "r1 1k 2k 500 temp 27 47 20",
        ),
        ("@r1[resistance] 2k 1k -500", "r1 2k 1k -500"),
    ] {
        let a = run(&mut c, parameter).unwrap();
        let b = run(&mut c, typed).unwrap();
        same_columns(&a, &b, &["sweep", "v(in)", "v(out)", "i(v1)"], 0.);
        assert_eq!(a.variables[0].unit, "parameter");
    }
}

#[test]
fn resolve_names_parameter_targets_canonically() {
    // The deck parser folds instance names; keywords are folded and aliased.
    let c = circuit("V1 in 0 1\nR1 in a 1k\nD1 a 0 dx\n.model dx d");
    let axes = resolve(
        &c,
        &request("@d1[PERIM] 0 1 1 @v1[dc] 1 2 1"),
        &AnalysisContext::default(),
    )
    .unwrap();
    assert_eq!(
        axes[0].target,
        SweepTarget::InstanceParameter {
            name: "@d1[pj]".into(),
            instance: "d1".into(),
            parameter: "pj".into(),
            route: ParameterRoute::Device,
        }
    );
    assert_eq!(axes[0].target.name(), "@d1[pj]");
    assert!(matches!(
        &axes[1].target,
        SweepTarget::InstanceParameter { route: ParameterRoute::VoltageSource, name, .. } if name == "@v1[dc]"
    ));
    let axes = resolve(&c, &request("@r1[r] 1k 2k 1k"), &AnalysisContext::default()).unwrap();
    assert!(matches!(
        axes[0].target,
        SweepTarget::InstanceParameter {
            route: ParameterRoute::Resistor,
            ..
        }
    ));
}

#[test]
fn bjt_and_mos1_replacements_reproduce_the_card_and_scale() {
    let bjt = "vcc c 0 5\nvb b 0 0.7\nrc c cc 1k\nq1 cc b 0 qx area=2\n\
               .model qx npn(is=1e-15 bf=100 rb=50 rc=5 re=1)";
    let mut c = circuit(bjt);
    let before = op(&mut c);
    // Sweeping a parameter at the card's own value is the unswept circuit.
    for sweep in [
        "@q1[area] 2 2 1",
        "@q1[m] 1 1 1",
        "@q1[dtemp] 0 0 1",
        "@q1[temp] 27 27 1",
    ] {
        let swept = run(&mut c, sweep).unwrap();
        for name in ["v(cc)", "i(vcc)", "i(vb)"] {
            close(
                swept.value(name, 0).unwrap().re,
                before.value(name, 0).unwrap().re,
                1e-12,
                1e-18,
            );
        }
    }
    // M multiplies every current of the device: with RC much smaller than
    // the load, two parallel copies nearly double the collector current.
    let m = run(&mut c, "@q1[m] 1 2 1").unwrap();
    let ic = column(&m, "i(vcc)");
    assert!(ic[1] / ic[0] > 1.9 && ic[1] / ic[0] < 2.0, "{ic:?}");

    let mos = "vdd d 0 3\nvg g 0 1.5\nrd d dd 10\nm1 dd g 0 0 nx w=10u l=2u\n\
               .model nx nmos(vto=0.7 kp=50u lambda=0)";
    let mut c = circuit(mos);
    let before = op(&mut c);
    let swept = run(&mut c, "@m1[w] 10u 10u 1u").unwrap();
    close(
        swept.value("i(vdd)", 0).unwrap().re,
        before.value("i(vdd)", 0).unwrap().re,
        1e-12,
        1e-18,
    );
    // Saturation: Id = KP/2 * W/L * (Vgs - Vto)^2 * M.
    let w = run(&mut c, "@m1[w] 10u 30u 10u").unwrap();
    for (width, i) in column(&w, "sweep").iter().zip(column(&w, "i(vdd)")) {
        close(-i, 25e-6 * width / 2e-6 * 0.8 * 0.8, 1e-6, 1e-15);
    }
    let l = run(&mut c, "@m1[l] 2u 4u 1u").unwrap();
    for (length, i) in column(&l, "sweep").iter().zip(column(&l, "i(vdd)")) {
        close(-i, 25e-6 * 10e-6 / length * 0.8 * 0.8, 1e-6, 1e-15);
    }
    let m = run(&mut c, "@m1[m] 1 3 1").unwrap();
    for (count, i) in column(&m, "sweep").iter().zip(column(&m, "i(vdd)")) {
        close(-i, count * 25e-6 * 5. * 0.8 * 0.8, 1e-6, 1e-15);
    }
}

#[test]
fn subcircuit_instances_use_their_flattened_c_names() {
    let body = ".subckt cell a\nr1 a b 1k\nd1 b 0 dx\n.ends\n.model dx d(is=1e-14)\n\
                v1 in 0 1\nx1 in cell";
    let mut c = circuit(body);
    let nested = run(&mut c, "@d.x1.d1[area] 1 2 1").unwrap();
    let mut flat = circuit("r1 in b 1k\nd1 b 0 dx\n.model dx d(is=1e-14)\nv1 in 0 1");
    let reference = run(&mut flat, "@d1[area] 1 2 1").unwrap();
    for (a, b) in column(&nested, "v(x1.b)")
        .iter()
        .zip(column(&reference, "v(b)"))
    {
        close(*a, b, 1e-12, 0.);
    }
    assert!(unsupported(&run(&mut c, "@d1[area] 1 2 1").unwrap_err()));
}

#[test]
fn sweeps_leave_the_circuit_unchanged() {
    let mut c = circuit(
        "v1 in 0 2\nr1 in a 1k\nd1 a 0 dx area=2\ng1 0 go a 0 1m\nrg go 0 1k\n.model dx d(rs=5)",
    );
    let before = op(&mut c);
    run(&mut c, "@d1[area] 1 4 1 @g1[gain] 1m 2m 1m").unwrap();
    // A failing sweep (area 0 at the last point) changes nothing either.
    assert!(run(&mut c, "@d1[area] 2 0 -1").is_err());
    let after = op(&mut c);
    for name in ["v(a)", "v(go)", "i(v1)"] {
        assert_eq!(before.value(name, 0), after.value(name, 0));
    }
}

#[test]
fn unsupported_targets_fail_before_any_sample() {
    let body = "v1 in 0 1\ni1 0 in 1m\nr1 in a rm\nc1 a 0 1p\nd1 a 0 dx temp=50\n\
                .model rm r(r=1k)\n.model dx d(is=1e-14)";
    let mut c = circuit(body);
    let context = AnalysisContext::default();
    let resolve_error = |args: &str| resolve(&c, &request(args), &context).unwrap_err();
    // Model parameters: C's DCTfindInstParam only searches instances.
    for args in [
        "@dx[is] 1e-14 2e-14 1e-14",
        "@rm[r] 1k 2k 1k",
        "dx 1 2 1",
        "dx[is] 1 2 1",
    ] {
        assert!(unsupported(&resolve_error(args)), "{args}");
    }
    // No such instance parameter in C either.
    for args in [
        "@d1[is] 1 2 1",
        "@d1[foo] 1 2 1",
        "@v1[c] 1 2 1",
        "@nope[area] 1 2 1",
    ] {
        assert!(unsupported(&resolve_error(args)), "{args}");
    }
    // Malformed targets (C ignores text after `]`; the port rejects it).
    for args in [
        "@d1 1 2 1",
        "@d1[] 1 2 1",
        "@[area] 1 2 1",
        "@d1[area 1 2 1",
        "@d1[area]x 1 2 1",
    ] {
        assert!(unsupported(&resolve_error(args)), "{args}");
    }
    // C sweeps these but the port does not yet.
    for args in [
        "@d1[ic] 1 2 1",
        "@r1[tc1] 0 1m 1m",
        "@c1[c] 1p 2p 1p",
        "@i1[m] 1 2 1",
        "@v1[acmag] 1 2 1",
    ] {
        assert!(resolve_error(args).is_not_yet_ported(), "{args}");
    }
    // Two nesting levels only (C silently ignores a third axis).
    assert!(unsupported(&resolve_error(
        "v1 0 1 1 r1 1k 2k 1k temp 27 28 1"
    )));
    // One quantity twice, under either spelling.
    for args in [
        "@d1[area] 1 2 1 @d1[area] 3 4 1",
        "@d1[perim] 1 2 1 @d1[pj] 3 4 1",
        "r1 1k 2k 1k @r1[r] 1k 2k 1k",
        "@v1[dc] 0 1 1 v1 0 1 1",
        "@i1[c] 0 1m 1m @i1[dc] 0 1m 1m",
    ] {
        assert!(unsupported(&resolve_error(args)), "{args}");
    }
    // A swept DTEMP on a diode with an instance TEMP would be ignored by C.
    assert!(unsupported(&resolve_error("@d1[dtemp] 0 10 10")));
    // Invalid values anywhere in the grid are rejected before solving.
    for args in [
        "@d1[area] 2 0 -1",
        "@d1[m] 1 -1 -1",
        "@d1[pj] 1 -1 -2",
        "@d1[temp] -200 -300 -100",
    ] {
        assert!(run(&mut c, args).is_err(), "{args}");
    }
    let mos = circuit("vd d 0 1\nm1 d d 0 0 nx w=1u l=1u nrd=1\n.model nx nmos(ld=0.2u rsh=10)");
    let mut mos = mos;
    // NRD=0 with RSH would short the internal drain node created at setup.
    assert!(run(&mut mos, "@m1[nrd] 1 0 -1").is_err());
    // L - 2 LD must stay positive.
    assert!(run(&mut mos, "@m1[l] 1u 0.2u -0.4u").is_err());
}

#[test]
fn instance_overrides_are_bounded_per_context() {
    let c = circuit("v1 a 0 1\nd1 a 0 dx\nd2 a 0 dx\n.model dx d");
    let base = ModelContext::default();
    let area = c.instance_override("d1", "area", 2., &base).unwrap();
    assert_eq!(
        (area.device(), area.parameter(), area.value()),
        (1, "area", 2.)
    );
    let pj = c.instance_override("D1", "PERIM", 1., &base).unwrap();
    assert_eq!(pj.parameter(), "pj");
    let context = base
        .with_instance_override(area)
        .unwrap()
        .with_instance_override(pj)
        .unwrap();
    assert_eq!(MAX_INSTANCE_OVERRIDES, 2);
    assert!(
        context
            .with_instance_override(c.instance_override("d2", "m", 2., &base).unwrap())
            .is_err()
    );
    assert!(
        base.with_instance_override(area)
            .unwrap()
            .with_instance_override(area)
            .is_err()
    );
    assert!(c.instance_override("d1", "area", 0., &base).is_err());
    assert!(
        c.instance_override("d1", "ic", 1., &base)
            .unwrap_err()
            .is_not_yet_ported()
    );
    assert!(
        c.instance_override("v1", "dc", 1., &base)
            .unwrap_err()
            .is_not_yet_ported()
    );
    // Both parameters of d1 reach the same replacement.
    let system = c.small_signal_system(
        &context,
        &ngspice_rs::maths::Vector::zeros(c.unknown_count()),
    );
    assert!(system.is_ok());
}
