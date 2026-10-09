//! `.sens` sensitivity analysis (GitHub #102) through the production driver:
//! closed-form derivatives of dividers and an RC low-pass, C's names, order,
//! filters and units, a diode against central differences of `.op`, C's
//! artefacts the port reproduces, batch composition and explicit refusals.
//!
//! The committed C goldens `sens_*` are compared by `cargo xtask golden
//! verify`; `tests/c_sens_reference.rs` compares further decks with live C.
use std::path::Path;

use ngspice_rs::analysis::{Plot, PlotFlags, RunConfig, batch, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, Complex, SpiceResult};

/// Every analysis of `body` in ngspice batch order, each on a fresh circuit.
fn run_all(body: &str) -> SpiceResult<Vec<(String, Plot)>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("sens.cir"),
        &format!("sens test\n{body}\n.end\n"),
    ))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let mut plots = Vec::new();
    for entry in batch::schedule(&netlist.analyses) {
        let request = config.request_for(&netlist.analyses[entry.card_index])?;
        let mut circuit = config.circuit(&netlist)?;
        let plot = runner(request.kind)?.run(&mut circuit, &request, &config.context())?;
        plots.push((entry.plot_name, plot));
    }
    Ok(plots)
}

fn sens(body: &str) -> SpiceResult<Plot> {
    let mut plots = run_all(body)?;
    assert_eq!(plots.len(), 1);
    let plot = plots.remove(0).1;
    assert_eq!(plot.plotname, "Sensitivity Analysis");
    Ok(plot)
}

fn value(plot: &Plot, name: &str) -> f64 {
    let v = plot
        .value(name, 0)
        .unwrap_or_else(|| panic!("no {name} in {:?}", names(plot)));
    assert_eq!(v.im, 0.);
    v.re
}

fn names(plot: &Plot) -> Vec<&str> {
    plot.variables.iter().map(|v| v.name.as_str()).collect()
}

fn close(got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs() + 1e-15,
        "{got} != {want}"
    );
}

const DIVIDER: &str = "v1 in 0 dc 10\nr1 in out 1k\nr2 out 0 2k";

/// `V = 10 R2/(R1+R2)`: `dV/dR1 = -10 R2/(R1+R2)^2`, `dV/dR2 = 10 R1/(R1+R2)^2`,
/// `dV/dV1 = R2/(R1+R2)`; `m` divides and `scale` multiplies the resistance.
/// C's forward difference with a relative step of 1e-6 is first-order
/// accurate, so the bound is 2e-6.
#[test]
fn a_divider_matches_its_closed_form_derivatives() {
    let plot = sens(&format!("{DIVIDER}\n.sens v(out)")).unwrap();
    assert_eq!(plot.flags, PlotFlags::Real);
    assert_eq!(plot.point_count(), 1);
    assert!(
        plot.variables
            .iter()
            .all(|v| v.unit == "voltage" && v.is_real)
    );
    let (r1, r2) = (1e3, 2e3);
    let sum2 = (r1 + r2) * (r1 + r2);
    close(value(&plot, "v(r1)"), -10. * r2 / sum2, 2e-6);
    close(value(&plot, "v(r2)"), 10. * r1 / sum2, 2e-6);
    close(value(&plot, "v(v1)"), r2 / (r1 + r2), 1e-9);
    // R/m: dV/dm = dV/dR * (-R); R*scale: dV/dscale = dV/dR * R.
    close(value(&plot, "v(r2_m)"), -10. * r1 / sum2 * r2, 2e-6);
    close(value(&plot, "v(r2_scale)"), 10. * r1 / sum2 * r2, 2e-6);
    // At TNOM the temperature coefficients and every model parameter of
    // C's default resistor model have no effect.
    for name in [
        "v(r1:tc1)",
        "v(r1:rsh)",
        "v(r1_tc)",
        "v(r1_temp)",
        "v(v1_z0)",
    ] {
        assert_eq!(value(&plot, name), 0., "{name}");
    }
}

/// C's `sgen` order and names: device types in `DEVices[]` order, later
/// models and instances first, model parameters `inst:kw`, the first
/// principal instance parameter `inst`, the rest `inst_kw`.
#[test]
fn names_and_order_follow_c_sgen() {
    let plot = sens(&format!("{DIVIDER}\n.sens v(out)")).unwrap();
    let names = names(&plot);
    assert_eq!(names.len(), 53);
    assert_eq!(&names[..3], ["v(r2:rsh)", "v(r2:narrow)", "v(r2:short)"]);
    assert_eq!(names[13], "v(r2)");
    assert_eq!(names[14], "v(r2_temp)");
    assert_eq!(names[24], "v(r1:rsh)");
    assert_eq!(
        &names[48..],
        [
            "v(v1)",
            "v(v1_z0)",
            "v(v1_pwr)",
            "v(v1_freq)",
            "v(v1_phase)"
        ]
    );
}

/// `Sens_filter` (`scan()` of `cktsens.c`): `*` and `?` wildcards.
#[test]
fn filters_select_parameter_names() {
    let plot = sens(&format!("{DIVIDER}\n.sens v(out) r?_m v1 dc")).unwrap();
    assert_eq!(names(&plot), ["v(r2_m)", "v(r1_m)", "v(v1)"]);
    let error = sens(&format!("{DIVIDER}\n.sens v(out) nothing")).unwrap_err();
    assert!(
        error.to_string().contains("perturbs no parameter"),
        "{error}"
    );
}

/// A current output read through a voltage source's branch, and a
/// differential voltage output.
#[test]
fn current_and_differential_outputs() {
    let plot = sens(&format!("{DIVIDER}\n.sens i(v1)")).unwrap();
    // i(v1) = -10/(R1+R2): d/dR1 = 10/(R1+R2)^2.
    close(value(&plot, "v(r1)"), 10. / 9e6, 2e-6);
    let plot = sens(&format!("{DIVIDER}\n.sens v(in,out)")).unwrap();
    close(value(&plot, "v(r1)"), 10. * 2e3 / 9e6, 2e-6);
}

/// An RC low-pass `H = 1/(1 + j w R C)`: `dH/dC = -j w R H^2`, complex, at
/// every frequency of a decade sweep.
#[test]
fn an_rc_low_pass_has_closed_form_ac_sensitivities() {
    let plot =
        sens("v1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\n.sens v(out) ac dec 1 10 1k").unwrap();
    assert_eq!(plot.flags, PlotFlags::Complex);
    assert_eq!(plot.variables[0].name, "frequency");
    assert_eq!(plot.point_count(), 3);
    for point in 0..3 {
        let f = plot.value("frequency", point).unwrap().re;
        let w = 2. * std::f64::consts::PI * f;
        let h = Complex::real(1.) / Complex::new(1., w * 1e3 * 1e-6);
        let want = Complex::new(0., -w * 1e3) * h * h;
        let got = plot.value("v(c1)", point).unwrap();
        assert!(
            (got - want).magnitude() <= 2e-6 * want.magnitude(),
            "{got} {want}"
        );
        let want_r = Complex::new(0., -w * 1e-6) * h * h;
        let got_r = plot.value("v(r1)", point).unwrap();
        // r1's resistance is perturbed at the first frequency only: C then
        // leaves its AC resistance "given", so later perturbations of the
        // resistance no longer reach the AC load (`RESacload`).
        if point == 0 {
            assert!((got_r - want_r).magnitude() <= 2e-6 * want_r.magnitude());
        } else {
            assert_eq!(got_r, Complex::ZERO);
        }
        let got_ac = plot.value("v(r1_ac)", point).unwrap();
        assert!((got_ac - want_r).magnitude() <= 2e-6 * want_r.magnitude());
    }
}

/// The diode's saturation-current sensitivity against a central difference
/// of two `.op` solutions; its `rs` reads zero (C's setup alone derives the
/// series conductance) and an unset IKF reads NaN (C divides by its zero
/// instance knee current).
#[test]
fn a_diode_matches_central_differences_and_c_artefacts() {
    let deck = |is: f64| {
        format!(
            "v1 in 0 dc 5\nr1 in a 1k\nd1 a 0 dm\n.model dm d is={is:e} rs=10 n=1.5\n\
             .options reltol=1e-12\n"
        )
    };
    let op = |is: f64| {
        let plots = run_all(&format!("{}.op", deck(is))).unwrap();
        plots[0].1.value("v(a)", 0).unwrap().re
    };
    let plot = sens(&format!("{}.sens v(a)", deck(1e-14))).unwrap();
    let h = 1e-17;
    let central = (op(1e-14 + h) - op(1e-14 - h)) / (2. * h);
    close(value(&plot, "v(d1:is)"), central, 1e-4);
    assert_eq!(value(&plot, "v(d1:rs)"), 0.);
    assert!(value(&plot, "v(d1:ikf)").is_nan());
}

/// `.sens` runs after `.op` and `.ac` in a batch (C's `analInfo` order).
#[test]
fn sensitivity_composes_with_other_analyses() {
    let plots = run_all(&format!("{DIVIDER}\n.sens v(out) v1\n.op\n.ac dec 1 1 10")).unwrap();
    let order: Vec<_> = plots.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(order, ["ac1", "op1", "sens1"]);
    assert_eq!(names(&plots[2].1), ["v(v1)"]);
}

#[test]
fn unsupported_cases_fail_explicitly() {
    for (body, message) in [
        (
            "v1 c 0 dc 5\nrb c b 100k\nq1 c b 0 qm\n.model qm npn\n.sens v(b)",
            "sensitivity analysis of device q1",
        ),
        (
            "v1 in 0 dc 5 ac 1\nr1 in a 1k\nd1 a 0 dm\n.model dm d\n.sens v(a) ac dec 1 1 10",
            "nonlinear and switch devices are linearized",
        ),
        (&format!("{DIVIDER}\n.sens i(r1)"), "no findable branch"),
        (
            &format!("{DIVIDER}\n.sens v(nowhere)"),
            "not in the circuit",
        ),
        (
            &format!("{DIVIDER}\n.sens v(out) ac lin 2 1k 1k"),
            "not positive",
        ),
        (
            "v1 in 0 dc 1 portnum=1\nr1 in 0 50\n.sens v(in)",
            "an RF port source",
        ),
    ] {
        let error = sens(body).unwrap_err();
        assert!(error.to_string().contains(message), "{body}: {error}");
    }
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("two.cir"),
            &format!("two\n{DIVIDER}\n.sens v(out)\n.sens v(in)\n.end\n"),
        ))
        .unwrap();
    let schedule = batch::schedule(&netlist.analyses);
    assert_eq!(
        schedule
            .iter()
            .filter(|s| s.kind == AnalysisKind::Sensitivity)
            .count(),
        2
    );
    let error = batch::check_targets(&schedule, &Default::default(), &[], &[]).unwrap_err();
    assert!(
        error.to_string().contains("more than one .sens card"),
        "{error}"
    );
}
