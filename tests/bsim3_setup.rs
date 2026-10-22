//! Offline BSIM3 effective-geometry, size-dependent and check-diagnostic tests.
//! These validate setup formulas, not a simulated device.
use std::path::Path;

use ngspice_rs::devices::bsim3;
use ngspice_rs::devices::bsim3::Diagnostic;
use ngspice_rs::devices::bsim3::setup::InstanceSetup;
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::SpiceError;

fn setup(card: &str, instance: &str, temperature: f64) -> InstanceSetup {
    try_setup(card, instance, temperature).unwrap()
}

fn try_setup(card: &str, instance: &str, temperature: f64) -> Result<InstanceSetup, SpiceError> {
    let text = format!("setup\nm1 d g 0 0 nn {instance}\n.model nn nmos level=8 {card}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("setup.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    bsim3::model_parameters(&resolved).unwrap().setup_instance(
        &netlist.devices[0],
        1.,
        &ModelContext::new(temperature, 27.),
    )
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs() * 1e-13 + 1e-30,
        "{actual:e} != {expected:e}"
    );
}

fn messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.message.as_str()).collect()
}

#[test]
fn instance_defaults_match_b3set_including_acm_squares() {
    let m = setup("", "", 27.);
    close(m.length, 5e-6);
    close(m.width, 5e-6);
    close(m.leff, 5e-6);
    close(m.weff, 5e-6);
    close(m.leff_cv, 5e-6);
    close(m.weff_cv, 5e-6);
    close(m.drain_area, 0.);
    close(m.source_perimeter, 0.);
    close(m.drain_squares, 1.);
    close(m.source_squares, 1.);
    close(m.delvto, 0.);
    close(m.mulu0, 1.);
    close(m.ic_vbs, 0.);
    close(m.ic_vgs, 0.);
    close(m.multiplier, 1.);
    assert_eq!((m.nqs_mod, m.acnqs_mod, m.geo), (0, 0, 0));
    assert_eq!(m.polarity, 1);
    // Sheet resistance defaults to zero, so the internal nodes collapse.
    close(m.drain_conductance, 0.);
    close(m.source_conductance, 0.);
    // C warns because the default perimeters are below W and CJSW is positive.
    assert_eq!(
        messages(&m.warnings),
        [
            "Warning: Pd = 0 is less than W.",
            "Warning: Ps = 0 is less than W.",
        ]
    );
}

#[test]
fn geometry_subtracts_size_dependent_deltas_per_binning_unit() {
    // Ll/Lw/Lwl use the drawn L and W powers, independent of BINUNIT.
    let m = setup(
        "lint=20n ll=1e-18 wint=30n wl=2e-18",
        "l=1u w=2u pd=4u ps=4u",
        27.,
    );
    let dl = 20e-9 + 1e-18 / 1e-6;
    // Wl/Ww/Wwl divide by powers of the drawn L (T2), as b3temp.c does.
    let dw = 30e-9 + 2e-18 / 1e-6;
    close(m.pparam("dl").unwrap(), dl);
    close(m.pparam("dw").unwrap(), dw);
    close(m.leff, 1e-6 - 2. * dl);
    close(m.weff, 2e-6 - 2. * dw);
    let metres = setup("lint=20n ll=1e-18 binunit=0", "l=1u w=2u pd=4u ps=4u", 27.);
    close(metres.leff, m.leff);
    // BINUNIT selects micron- or metre-scaled coefficient inverses: the C
    // default is 1, so `1e-6/Leff`, not `1/Leff`.
    let cdsc = m.pparam("cdsc").unwrap();
    close(cdsc, 2.4e-4);
    let metres_cdsc = metres.pparam("cdsc").unwrap();
    close(metres_cdsc, 2.4e-4);
}

#[test]
fn binned_coefficients_add_l_w_and_area_terms() {
    // cdsc default 2.4e-4; lcdsc=1e-4, wcdsc=2e-4, pcdsc=3e-10. The default
    // BINUNIT=1 scales each inverse by a micron, exactly as b3temp.c does.
    let m = setup(
        "lcdsc=1e-4 wcdsc=2e-4 pcdsc=3e-10",
        "l=1u w=2u pd=4u ps=4u",
        27.,
    );
    let expected =
        2.4e-4 + 1e-4 * 1e-6 / m.leff + 2e-4 * 1e-6 / m.weff + 3e-10 * 1e-12 / (m.leff * m.weff);
    close(m.pparam("cdsc").unwrap(), expected);
    // BINUNIT=0 uses plain metre inverses.
    let metres = setup(
        "binunit=0 lcdsc=1e-4 wcdsc=2e-4 pcdsc=3e-10",
        "l=1u w=2u pd=4u ps=4u",
        27.,
    );
    let expected =
        2.4e-4 + 1e-4 / metres.leff + 2e-4 / metres.weff + 3e-10 / (metres.leff * metres.weff);
    close(metres.pparam("cdsc").unwrap(), expected);
    // Every generated row is present and finite.
    let names = bsim3::setup::size_dependent_names();
    assert_eq!(names.len(), 83);
    for name in names {
        let value = m.pparam(name).unwrap_or_else(|| panic!("missing {name}"));
        assert!(value.is_finite(), "{name} is not finite");
    }
}

#[test]
fn mobility_temperature_and_series_resistance_follow_c_order() {
    // u0 > 1 selects C's 1e4 downscale; ute scales the ratio.
    let m = setup("u0=600 ute=-1.5 rsh=10", "l=1u w=2u pd=4u ps=4u", 127.);
    let ratio: f64 = (127.0 + 273.15) / (27.0 + 273.15);
    close(m.pparam("u0").unwrap(), 0.06);
    close(m.pparam("u0temp").unwrap(), 0.06 * ratio.powf(-1.5));
    close(m.u0temp, 0.06 * ratio.powf(-1.5));
    // One square per junction at 10 ohm/square (C's NRD/NRS default).
    close(m.drain_conductance, 0.1);
    close(m.source_conductance, 0.1);
    // DELVTO shifts the instance threshold, not the model one.
    let shifted = setup("u0=600", "l=1u w=2u pd=4u ps=4u delvto=0.25 mulu0=2", 27.);
    close(shifted.vth0, shifted.pparam("vth0").unwrap() + 0.25);
    close(shifted.u0temp, shifted.pparam("u0temp").unwrap() * 2.);
}

#[test]
fn potential_and_threshold_derivations_are_self_consistent() {
    let m = setup("vth0=0.7", "l=1u w=2u", 27.);
    let phi = m.pparam("phi").unwrap();
    let k1 = m.pparam("k1").unwrap();
    let vth0 = m.pparam("vth0").unwrap();
    let sqrt_phi = phi.sqrt();
    close(m.pparam("sqrtphi").unwrap(), sqrt_phi);
    close(m.pparam("phis3").unwrap(), sqrt_phi * phi);
    // VFB is derived from the given VTH0, and VTH0 is left as given.
    close(vth0, 0.7);
    close(m.pparam("vfb").unwrap(), 0.7 - phi - k1 * sqrt_phi);
    // K1/K2 come from the doping defaults consistently.
    let npeak = m.pparam("npeak").unwrap();
    let nsub = m.pparam("nsub").unwrap();
    let cox = 3.453_133e-11 / 1.5e-8;
    close(m.pparam("gamma1").unwrap(), 5.753e-12 * npeak.sqrt() / cox);
    close(m.pparam("gamma2").unwrap(), 5.753e-12 * nsub.sqrt() / cox);
    assert!(m.pparam("k1").unwrap() > 0.);
}

#[test]
fn explicit_k_pair_ignores_doping_and_warns_about_ignored_inputs() {
    let m = setup(
        "k1=0.5 k2=0.1 vbx=-1 gamma1=0.6",
        "l=1u w=2u pd=4u ps=4u",
        27.,
    );
    close(m.pparam("k1").unwrap(), 0.5);
    close(m.pparam("k2").unwrap(), 0.1);
    let expected = [
        "Warning: vbx is ignored because k1 or k2 is given.",
        "Warning: gamma1 is ignored because k1 or k2 is given.",
    ];
    assert_eq!(messages(&m.warnings), expected);
    // A half-specified pair is completed with C's literals.
    let half = setup("k1=0.5", "l=1u w=2u pd=4u ps=4u", 27.);
    close(half.pparam("k2").unwrap(), -0.0186);
    assert_eq!(
        messages(&half.warnings),
        ["Warning: k2 should be specified with k1."]
    );
}

#[test]
fn instance_nqs_selectors_fall_back_to_the_model_with_a_warning() {
    let m = setup(
        "nqsmod=1 acnqsmod=1",
        "l=1u w=2u pd=4u ps=4u nqsmod=1 acnqsmod=1",
        27.,
    );
    assert_eq!((m.nqs_mod, m.acnqs_mod), (1, 1));
    assert!(m.warnings.is_empty(), "{:?}", m.warnings);
    let bad = setup(
        "nqsmod=1 acnqsmod=0",
        "l=1u w=2u pd=4u ps=4u nqsmod=7 acnqsmod=-3",
        27.,
    );
    assert_eq!((bad.nqs_mod, bad.acnqs_mod), (1, 0));
    assert_eq!(
        messages(&bad.warnings),
        [
            "Warning: nqsMod has been set to its global value 1.",
            "Warning: acnqsMod has been set to its global value 0.",
        ]
    );
    // An explicit zero is a valid instance choice.
    let zero = setup("nqsmod=1", "l=1u w=2u pd=4u ps=4u nqsmod=0", 27.);
    assert_eq!(zero.nqs_mod, 0);
}

#[test]
fn effective_geometry_and_check_fatals_fail_before_any_device_exists() {
    let error = try_setup("lint=600n", "l=1u w=2u", 27.).expect_err("negative length");
    assert!(
        format!("{error}").contains("Effective channel length <= 0"),
        "{error}"
    );
    let error = try_setup("wint=1.1u", "l=1u w=2u", 27.).expect_err("negative width");
    assert!(
        format!("{error}").contains("Effective channel width <= 0"),
        "{error}"
    );
    let error = try_setup("dlc=600n", "l=1u w=2u", 27.).expect_err("negative C-V length");
    assert!(
        format!("{error}").contains("Effective channel length for C-V <= 0"),
        "{error}"
    );
    let error = try_setup("clc=-1e-7", "l=1u w=2u", 27.).expect_err("negative clc");
    assert!(
        format!("{error}").contains("Fatal: Clc = -1e-07 is negative."),
        "{error}"
    );
    assert!(matches!(error, SpiceError::Parse { .. }));
    let error = try_setup("xj=0", "l=1u w=2u", 27.).expect_err("zero xj");
    assert!(
        format!("{error}").contains("Fatal: Xj = 0 is not positive."),
        "{error}"
    );
}

#[test]
fn parameter_checks_warn_and_reset_like_b3check() {
    let m = setup(
        "paramchk=1 noff=0.05 voffcv=0.8 moin=100 a2=5 rdsw=-1 pdiblc1=-0.1",
        "l=1u w=2u pd=4u ps=4u",
        27.,
    );
    let text = messages(&m.warnings).join("\n");
    for expected in [
        "Warning: Noff = 0.05 is too small.",
        "Warning: Voffcv = 0.8 is too large.",
        "Warning: Moin = 100 is too large.",
        "Warning: A2 = 5 is larger than 1. A2 is set to 1 and A1 is set to 0.",
        "Warning: Rdsw = -1 is negative. Set to zero.",
        "Warning: Pdibl1 = -0.1 is negative.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
    close(m.pparam("a2").unwrap(), 1.);
    close(m.pparam("a1").unwrap(), 0.);
    close(m.pparam("rdsw").unwrap(), 0.);
    close(m.pparam("rds0").unwrap(), 0.);
    // A2 too small is clamped upward without touching A1.
    let small = setup("paramchk=1 a2=0.001 a1=0.3", "l=1u w=2u pd=4u ps=4u", 27.);
    close(small.pparam("a2").unwrap(), 0.01);
    close(small.pparam("a1").unwrap(), 0.3);
}

#[test]
fn junction_temperature_and_breakdown_follow_b3temp() {
    // At TNOM the temperature factors are exactly one, so the drain junction
    // current is area*JS + perimeter*JSW. Source keeps C's 1e-14 fallback.
    let nominal = setup("js=1e-4 jsw=1e-10 nj=1 xti=3", "l=1u w=2u ad=1p pd=2u", 27.);
    close(nominal.drain_sat_current, 1e-12 * 1e-4 + 2e-6 * 1e-10);
    close(nominal.source_sat_current, 1e-14);
    assert!(nominal.drain_area == 1e-12 && nominal.drain_perimeter == 2e-6);
    // With zero areas and perimeters C substitutes 1e-14 A.
    let cold = setup("", "", 27.);
    close(cold.source_sat_current, 1e-14);
    close(cold.drain_sat_current, 1e-14);
    // Default IJTH > 0 gives a finite breakdown voltage.
    assert!(cold.vjsm > 0. && cold.isevjsm > 0.);
    let no_ijth = setup("ijth=0", "", 27.);
    close(no_ijth.vjsm, 0.);
    close(no_ijth.isevjsm, 0.);
    // C clamps a junction potential below 0.1 V.
    let clamped = setup("pb=0.05", "", 27.);
    assert!(messages(&clamped.warnings).contains(&"Given pb is less than 0.1. Pb is set to 0.1."));
}

#[test]
fn shared_model_resets_apply_once_across_instances_like_c() {
    // C resets the model's negative CGDO and CALCACM while setting up the
    // first instance, so the second instance sees the corrected model.
    let text = "shared\nm1 d g 0 0 nn l=1u w=2u pd=4u ps=4u\n\
                m2 d g 0 0 nn l=2u w=2u pd=4u ps=4u\n\
                .model nn nmos level=8 paramchk=1 calcacm=3 cgdo=-1p cgso=-2p cgbo=-3p\n.end\n";
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("shared.cir"), text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let context = ModelContext::new(27., 27.);
    let mut shared =
        bsim3::model_parameters(&resolver.resolve(&netlist.devices[0]).unwrap().unwrap())
            .unwrap()
            .model_setup(&context, &netlist.devices[0].location)
            .unwrap();
    let mut per_instance = Vec::new();
    for device in &netlist.devices {
        let resolved = resolver.resolve(device).unwrap().unwrap();
        let inputs = bsim3::model_parameters(&resolved)
            .unwrap()
            .instance_parameters(device, 1.)
            .unwrap();
        per_instance.push(shared.setup_instance(&inputs, &device.location).unwrap());
    }
    let expected = [
        "Warning: CALCACM = 3 is wrong. Set back to 0.",
        "Warning: cgdo = -1e-12 is negative. Set to zero.",
        "Warning: cgso = -2e-12 is negative. Set to zero.",
        "Warning: cgbo = -3e-12 is negative. Set to zero.",
    ];
    assert_eq!(messages(&per_instance[0].warnings), expected);
    assert!(
        per_instance[1].warnings.is_empty(),
        "{:?}",
        per_instance[1].warnings
    );
    // Both instances then use the reset model values, as C does.
    let cf = per_instance[0].pparam("cf").unwrap();
    close(per_instance[0].cgdo, cf * per_instance[0].weff_cv);
    close(per_instance[1].cgdo, cf * per_instance[1].weff_cv);
    // A fresh single-instance setup still reports the resets.
    assert_eq!(
        messages(
            &setup(
                "paramchk=1 calcacm=3 cgdo=-1p cgso=-2p cgbo=-3p",
                "l=1u w=2u pd=4u ps=4u",
                27.
            )
            .warnings
        ),
        expected
    );
}

#[test]
fn acm_geometry_is_refused_explicitly_not_approximated() {
    let error = try_setup("acm=1", "l=1u w=2u", 27.).expect_err("ACM");
    assert!(
        matches!(error, SpiceError::NotYetPorted { .. }),
        "{error:?}"
    );
    assert!(
        format!("{error}").contains("ACM_SourceDrainResistances"),
        "{error}"
    );
    // CALCACM without ACM12 is reset with a warning, as in b3check.c.
    let reset = setup("calcacm=3", "pd=4u ps=4u", 27.);
    assert!(messages(&reset.warnings).contains(&"Warning: CALCACM = 3 is wrong. Set back to 0."));
}
