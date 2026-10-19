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

/// The analyses of `body` in batch order on one inheriting circuit lineage,
/// as `spice-rs simulate` runs a deck (C keeps one circuit for the deck).
fn run_batch(body: &str) -> SpiceResult<Vec<(String, Plot)>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("sens.cir"),
        &format!("sens test\n{body}\n.end\n"),
    ))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let mut plots = Vec::new();
    let mut previous: Option<ngspice_rs::devices::Circuit> = None;
    for entry in batch::schedule(&netlist.analyses) {
        let request = config.request_for(&netlist.analyses[entry.card_index])?;
        let mut circuit = config.circuit(&netlist)?;
        if let Some(previous) = &previous {
            circuit.inherit_from(previous);
        }
        let plot = runner(request.kind)?.run(&mut circuit, &request, &config.context())?;
        circuit.note_completed(entry.kind);
        previous = Some(circuit);
        plots.push((entry.plot_name, plot));
    }
    Ok(plots)
}

/// AC `.sens` linearizes a diode at `cktsens.c`'s reset load (all-zero node
/// voltages), not at its operating point: at the first frequency `v(2)` of
/// `r1` into the diode is `1/(1 + R1 gd)` with `gd = IS/Vt + gmin`, so
/// `d v(2)/d R1 = -gd/(1 + R1 gd)^2` whatever the bias, the capacitance is
/// absent and `is` (frozen in the reset state) has an exactly zero column.
#[test]
fn ac_sensitivity_linearizes_a_diode_at_the_reset_state() {
    let deck = |bias: &str| {
        format!(
            "v1 1 0 dc {bias} ac 1\nr1 1 2 1k\nd1 2 0 dm\n.model dm d is=1e-14 cjo=1p\n\
             .sens v(2) ac dec 1 1k 10k"
        )
    };
    let forward = sens(&deck("0.7")).unwrap();
    let off = sens(&deck("0")).unwrap();
    let at = |plot: &Plot, name: &str| plot.value(name, 0).unwrap();
    assert_eq!(at(&forward, "v(r1)"), at(&off, "v(r1)"));
    // CONSTKoverQ * 300.15 K, gmin = 1e-12.
    let vt = 1.38064852e-23 / 1.6021766208e-19 * 300.15;
    let gd = 1e-14 / vt + 1e-12;
    let want = -gd / ((1. + 1e3 * gd) * (1. + 1e3 * gd));
    let got = at(&forward, "v(r1)");
    close(got.re, want, 2e-6);
    assert_eq!(got.im, 0.);
    assert_eq!(at(&forward, "v(d1:is)"), Complex::ZERO);
    assert_eq!(at(&forward, "v(d1:cjo)"), Complex::ZERO);
}

/// A MOS AC load reads `capbd`/`capbs` from the instance, which only a
/// small-signal or transient load writes. After `.op` (or `.ac`, `.noise`,
/// ...) C reads the operating point's values, which the port reproduces: the
/// columns move against a deck without the `.op`. A `.dc` sweep writes none.
/// When `.tran` wrote them last (C runs it after `.op`), use its final
/// load's values rather than re-evaluating the operating point.
#[test]
fn mos_ac_sensitivity_reads_junction_capacitances_left_by_an_earlier_analysis() {
    let body = "vd d 0 dc 3\nvg g 0 dc 2 ac 1\nvs s 0 0\n\
                m1 d g s 0 nm l=10u w=20u ad=10p as=10p nrs=2\n\
                .model nm nmos level=1 nsub=1e16 tox=20n rsh=50 cj=1e-4 cgso=1e-10\n\
                .sens i(vs) ac dec 1 1k 10k";
    let last = |plots: Vec<(String, Plot)>| plots.into_iter().last().unwrap();
    let (name, fresh) = last(run_batch(body).unwrap());
    assert_eq!(name, "sens1");
    let (_, swept) = last(run_batch(&format!("{body}\n.dc vd 3 3 1")).unwrap());
    let (_, after_op) = last(run_batch(&format!("{body}\n.op")).unwrap());
    let column = |plot: &Plot| plot.value("v(m1:cgso)", 0).unwrap();
    assert_eq!(column(&swept), column(&fresh));
    assert_ne!(column(&after_op), column(&fresh));
    assert_eq!(after_op.value("v(m1:cj)", 0).unwrap(), Complex::ZERO);
    let (_, after_tran) = last(run_batch(&format!("{body}\n.op\n.tran 1n 10n")).unwrap());
    close(column(&after_tran).re, column(&after_op).re, 1e-6);
    close(column(&after_tran).im, column(&after_op).im, 1e-6);
}

/// All AC-capable classic MOS levels retain the final transient load's
/// junction capacitances, not the operating point's. A DC-only analysis and
/// `.sens`'s own bias solve must leave them alone. The bulk sine changes both
/// depletion capacitances before the final point; Meyer halves still reset.
#[test]
fn mos_ac_sensitivity_inherits_the_final_transient_load_for_each_level() {
    for level in [1, 2, 3, 9] {
        let body = format!(
            "vd d 0 dc 3\nvg g 0 dc 2 ac 1\nvs s 0 0\n\
             vb b 0 dc -0.3 sin(-0.3 0.2 100meg)\n\
             m1 d g s b nm l=10u w=20u ad=10p as=20p pd=10u ps=20u nrd=2 nrs=3\n\
             .model nm nmos level={level} vto=0.7 kp=50u tox=20n nsub=1e16 rsh=100 \
             cj=1e-4 cjsw=1e-10 cgso=1e-10\n\
             .options reltol=1e-10 vntol=1e-13 abstol=1e-16\n\
             .sens i(vs) ac dec 1 1k 1meg"
        );
        let last = |plots: Vec<(String, Plot)>| plots.into_iter().last().unwrap().1;
        let op = last(run_batch(&format!("{body}\n.op")).unwrap());
        let tran = last(run_batch(&format!("{body}\n.tran 1n 6n")).unwrap());
        let both = last(run_batch(&format!("{body}\n.op\n.tran 1n 6n")).unwrap());
        // .tf runs after .tran in C's batch order and must not replace the
        // capacitances even though it solves the DC bias again.
        let tf = last(run_batch(&format!("{body}\n.tran 1n 6n\n.tf v(d) vd")).unwrap());
        for point in 0..4 {
            let column = |plot: &Plot| plot.value("v(m1:cgso)", point).unwrap();
            if point == 0 {
                assert_ne!(column(&tran), column(&op), "MOS{level}");
            }
            assert_eq!(column(&both), column(&tran), "MOS{level}, point {point}");
            assert_eq!(column(&tf), column(&tran), "MOS{level}, point {point}");
            assert_eq!(tran.value("v(m1:cj)", point).unwrap(), Complex::ZERO);
        }
    }
}

/// A snapshot follows the device/state layout, not matrix row numbers or
/// just an analysis-kind marker. Re-finalizing the same circuit preserves
/// it; inheriting into a different layout or renumbering swapped devices
/// cannot silently reinterpret another device's slots.
#[test]
fn transient_load_snapshots_require_a_matching_device_layout() {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("snapshot.cir"),
            "snapshot\nv1 in 0 1\nr1 in out 1k\nc1 out 0 1u\n.tran 0.1m 1m\n.end\n",
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut original = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut original, &request, &config.context())
        .unwrap();
    original.note_completed(request.kind);
    let snapshot = original.final_transient_state().expect("final load");
    let mut inherited = config.circuit(&netlist).unwrap();
    inherited.inherit_from(&original);
    inherited.finalize().unwrap();
    assert_eq!(inherited.final_transient_state(), Some(snapshot.clone()));
    inherited.devices_mut().reverse();
    inherited.finalize().unwrap();
    assert!(inherited.final_transient_state().is_none());
    inherited.inherit_from(&original);
    assert!(inherited.final_transient_state().is_none());
    assert_eq!(original.final_transient_state(), Some(snapshot));
}

#[test]
fn a_transient_marker_without_its_load_is_still_an_explicit_refusal() {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("missing.cir"),
            "missing\nvd d 0 3\nvg g 0 dc 2 ac 1\nm1 d g 0 0 nm l=10u w=20u\n\
         .model nm nmos level=1 cj=1e-4\n.sens i(vd) ac dec 1 1k 10k\n.end\n",
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    circuit.note_completed(AnalysisKind::Transient);
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(
        error.to_string().contains("capacitances are missing"),
        "{error}"
    );
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
            "v1 c 0 dc 5\nrb c b 100k\nq1 c b 0 qm\n.model qm npn rc=40\n.sens v(b)",
            "BJT sensitivity with RC != 0",
        ),
        (
            "v1 in 0 dc 0 ac 1\nr1 in a 50\nl1 a 0 1u\nl2 b 0 1u\nl3 c 0 1u\nr2 b 0 50\n\
             r3 c 0 50\nk1 l1 l2 l3 0.5\n.sens v(b) ac dec 1 1meg 10meg",
            "more than two inductors",
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
