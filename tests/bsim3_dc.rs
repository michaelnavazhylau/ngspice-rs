//! Offline sanity checks for the BSIM3 DC core. Numeric parity with C lives in
//! the opt-in `c_bsim3_dc_reference` suite.
use std::path::Path;

use ngspice_rs::devices::bsim3::{self, dc};
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

fn evaluate(card: &str, instance: &str, bias: dc::Bias) -> dc::DcEvaluation {
    evaluate_family("nmos", card, instance, bias)
}

fn evaluate_family(family: &str, card: &str, instance: &str, bias: dc::Bias) -> dc::DcEvaluation {
    let text = format!("dc\nm1 d g 0 0 nn {instance}\n.model nn {family} level=8 {card}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("dc.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    let params = bsim3::model_parameters(&resolved).unwrap();
    let context = ModelContext::new(27., 27.);
    let defaults = params.model_defaults(&context).unwrap();
    let temperature = defaults
        .at_temperature(27., &netlist.devices[0].location)
        .unwrap();
    let inputs = params.instance_parameters(&netlist.devices[0], 1.).unwrap();
    let setup = bsim3::setup::InstanceSetup::new(
        &defaults,
        &temperature,
        &inputs,
        &netlist.devices[0].location,
    )
    .unwrap();
    dc::DcModel {
        model: &defaults,
        temperature: &temperature,
        setup: &setup,
    }
    .evaluate(bias, context.gmin)
    .unwrap()
}

fn at(vgs: f64, vds: f64) -> dc::Bias {
    dc::Bias { vbs: 0., vgs, vds }
}

#[test]
fn zero_bias_leaves_only_the_junction_offsets() {
    let d = evaluate("", "l=1u w=10u", at(0., 0.));
    assert!(d.ids.abs() < 1e-18, "{}", d.ids);
    // C's junction model has no gmin offset at zero bias.
    assert_eq!(d.cbs, 0.);
    assert_eq!(d.cbd, 0.);
    assert!(d.gbs >= 1e-12 && d.gbd >= 1e-12);
    assert!(d.von.is_finite());
    assert_eq!(d.mode, 1);
}

#[test]
fn drain_current_rises_with_gate_and_saturates() {
    // A bare BSIM3 model has Vth around 1.8 V (C reports the same), so these
    // biases sit above threshold.
    let off = evaluate("", "l=1u w=10u", at(1.5, 1.0));
    let on = evaluate("", "l=1u w=10u", at(2.5, 1.0));
    assert!(on.ids > off.ids * 1e3, "{} {}", on.ids, off.ids);
    let linear = evaluate("", "l=1u w=10u", at(2.5, 0.05));
    let saturation = evaluate("", "l=1u w=10u", at(2.5, 1.0));
    assert!(saturation.ids > linear.ids);
    assert!(saturation.ids.is_finite() && saturation.gm > 0. && saturation.gds > 0.);
    // Long-channel saturation is only weakly dependent on Vds.
    let deeper = evaluate("", "l=1u w=10u", at(2.5, 2.0));
    assert!(deeper.ids > saturation.ids);
    assert!((deeper.ids - saturation.ids).abs() / saturation.ids < 0.2);
}

#[test]
fn inverse_mode_mirrors_the_terminals() {
    // Inverse mode swaps source and drain: C evaluates Vgs=vgd and Vbs=vbd,
    // so the same device driven with those forward biases must agree.
    let mirror = evaluate(
        "",
        "l=1u w=10u",
        dc::Bias {
            vbs: 0.8,
            vgs: 3.0,
            vds: 0.8,
        },
    );
    let reverse = evaluate(
        "",
        "l=1u w=10u",
        dc::Bias {
            vbs: 0.,
            vgs: 2.2,
            vds: -0.8,
        },
    );
    assert_eq!(mirror.mode, 1);
    assert_eq!(reverse.mode, -1);
    assert!(
        reverse.ids > 0.,
        "inverse current stays positive: {}",
        reverse.ids
    );
    assert!(
        (reverse.ids - mirror.ids).abs() / mirror.ids < 1e-12,
        "{} vs {}",
        reverse.ids,
        mirror.ids
    );
    assert_eq!(reverse.vds, 0.8);
}

#[test]
fn mobility_models_and_series_resistance_change_the_current() {
    let base = evaluate("", "l=0.5u w=5u", at(2.5, 1.0)).ids;
    let model2 = evaluate(
        "mobmod=2 ua=1e-9 ub=1e-18 uc=1e-11",
        "l=0.5u w=5u",
        at(2.5, 1.0),
    )
    .ids;
    let model3 = evaluate(
        "mobmod=3 ua=4.9e-10 ub=1e-18 uc=-3e-11",
        "l=0.5u w=5u",
        at(2.5, 1.0),
    )
    .ids;
    assert!((base - model2).abs() / base > 1e-3, "{base:e} {model2:e}");
    assert!((base - model3).abs() / base > 1e-3, "{base:e} {model3:e}");
    // RDSW creates the channel series resistance C folds into the load.
    // RSH/NRD/NRS only move the junction nodes, which the core does not see.
    let resistive = evaluate("rdsw=100", "l=1u w=10u", at(2.5, 0.4));
    let plain = evaluate("", "l=1u w=10u", at(2.5, 0.4));
    assert!(resistive.rds > 0., "{}", resistive.rds);
    assert!(resistive.ids < plain.ids);
}

#[test]
fn junction_diodes_follow_area_perimeter_and_ijth() {
    let card = "js=1e-5 jsw=1e-10";
    let area = evaluate(card, "l=1u w=10u ad=2p as=1p pd=4u ps=3u", at(0., 0.3));
    // vbd = -0.3 V: the drain diode is reverse biased but not clamped.
    assert!(area.cbd < 0. && area.cbs == 0.);
    let no_ijth = evaluate(
        "ijth=0 js=1e-5 jsw=1e-10",
        "l=1u w=10u ad=2p as=1p pd=4u ps=3u",
        at(0., 0.3),
    );
    // Both select C's exponential branch below breakdown.
    assert!((area.cbd - no_ijth.cbd).abs() <= 1e-12 * area.cbd.abs());
    // Larger area increases the saturation current and thus the magnitude.
    let bigger = evaluate(card, "l=1u w=10u ad=20p pd=4u", at(0., 0.3));
    assert!(bigger.cbd < area.cbd);
}

#[test]
fn ask_keywords_read_the_evaluated_values() {
    let d = evaluate("", "l=1u w=10u", at(2.5, 1.0));
    assert_eq!(d.ask("id"), Some(d.ids));
    assert_eq!(d.ask("vth"), Some(d.von));
    assert_eq!(d.ask("gm"), Some(d.gm));
    assert_eq!(d.ask("ibs"), Some(d.cbs));
    assert_eq!(d.ask("ibd"), Some(d.cbd));
    // `cbs`/`cbd` are capacitances in C, so they are not DC asks.
    assert_eq!(d.ask("cbs"), None);
    assert_eq!(d.ask("cgg"), None);
}

#[test]
fn narrow_weff_continuation_preserves_c_product_grouping() {
    // Values from b3ld.c's DC statements at ngspice
    // 059066b532f7073fccf4173ef269309eafdc7f0c (Apple clang 21.0.0, arm64
    // macOS, -O2 -ffp-contract=off), at C's exact converged internal biases.
    // They pin the narrow-Weff (`T0 *= T0 * 4e-16`) continuation, which the
    // statement-by-statement load reproduces. On other libm platforms retain
    // a tight numerical check.
    let cases = [
        (
            "dwg=5.66336256480132049e-08 dwb=4.43519864619213457e-08 pvag=-5.68213123122809236e+01",
            3.436745739803129,
            0.6881263962794835,
            [
                [6.559413161624127e-7, 4.3301436301752303e-7],
                [3.0732277991108097e-7, 1.9164899436315728e-7],
            ],
        ),
        (
            "dwg=1.20659197801292422e-07 dwb=7.47031727728139807e-08 pvag=-7.74709299330928047e+01",
            3.6179029802691343,
            0.6768355065674196,
            [
                [7.137588870935034e-7, 4.219884935651082e-7],
                [3.2287837510213314e-7, 1.8618667976474388e-7],
            ],
        ),
        (
            "dwg=1.59970859961868845e-07 dwb=3.86054572048661918e-08 pvag=-3.75168453234650912e+01",
            3.883952204047605,
            0.6349801391395404,
            [
                [6.897670543785235e-7, 4.138294097972818e-7],
                [2.96208528785934e-7, 1.8084942481066144e-7],
            ],
        ),
    ];
    for (card, vgs, vds, expected) in cases {
        for (family, expected) in ["nmos", "pmos"].into_iter().zip(expected) {
            let d = evaluate_family(family, card, "l=0.5u w=0.1u", at(vgs, vds));
            for (name, rust, c) in [("gm", d.gm, expected[0]), ("gmbs", d.gmbs, expected[1])] {
                if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                    assert_eq!(rust, c, "{family} {name}: {card}");
                } else {
                    assert!(
                        (rust - c).abs() <= 1e-12 * c.abs() + 1e-24,
                        "{family} {name}: {rust:e} vs {c:e}"
                    );
                }
            }
        }
    }
}
