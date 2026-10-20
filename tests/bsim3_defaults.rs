//! Offline BSIM3 model-default behavior. Simulation and geometry checks remain pending.
use std::path::Path;

use ngspice_rs::devices::bsim3::{self, defaults::ModelDefaults};
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

fn model(card: &str, family: &str) -> ModelDefaults {
    let text =
        format!("defaults\nm1 d g 0 0 nn l=1u w=2u\n.model nn {family} level=8 {card}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("defaults.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    bsim3::model_parameters(&resolved)
        .unwrap()
        .model_defaults(&ModelContext::new(27., 27.))
        .unwrap()
}

fn numbered(card: &str, family: &str, temperature: f64, nominal: f64) -> ModelDefaults {
    let text =
        format!("defaults\nm1 d g 0 0 nn l=1u w=2u\n.model nn {family} level=8 {card}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("defaults.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    bsim3::model_parameters(&resolved)
        .unwrap()
        .model_defaults(&ModelContext::new(temperature, nominal))
        .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs() * 1e-15 + 1e-300,
        "{actual:e} != {expected:e}"
    );
}

#[test]
fn literal_defaults_are_applied_but_given_bits_stay_honest() {
    let nmos = model("", "nmos");
    for (name, value) in [
        ("lintnoi", 0.),
        ("lint", 0.),
        ("ll", 0.),
        ("lln", 1.),
        ("lmin", 0.),
        ("lmax", 1.),
        ("wmin", 0.),
        ("wmax", 1.),
        ("xl", 0.),
        ("xw", 0.),
        ("xpart", 0.),
        ("rsh", 0.),
        ("cj", 5e-4),
        ("cjsw", 5e-10),
        ("js", 1e-4),
        ("jsw", 0.),
        ("pb", 1.),
        ("mj", 0.5),
        ("mjsw", 0.33),
        ("nj", 1.),
        ("xti", 3.),
        ("em", 4.1e7),
        ("ef", 1.),
        ("af", 1.),
        ("kf", 0.),
        ("vgs_max", 1e99),
        ("vbd_max", 1e99),
    ] {
        close(nmos.get(name).unwrap(), value);
        assert!(!nmos.is_given(name), "{name} became given");
    }
    // Setup leaves these at GENmodel's zero allocation and never gives them:
    // b3temp.c distinguishes that state from an explicit zero.
    for name in [
        "k1", "k2", "vfb", "gamma1", "gamma2", "vbx", "lgamma1", "pk1",
    ] {
        close(nmos.get(name).unwrap(), 0.);
        assert!(!nmos.is_given(name), "{name} became given");
    }
    // TYPE is a flag, not a numeric field: polarity is its projection.
    assert_eq!(nmos.polarity, 1);
    // Flags have no C given bit (`IF_FLAG` writes the value directly), so the
    // given query is numeric-only by design; polarity carries the selector.
    assert_eq!(nmos.version, "3.3.0");
    assert!(nmos.warnings.is_empty());
}

#[test]
fn polarities_select_their_own_literal_defaults() {
    let nmos = model("", "nmos");
    let pmos = model("", "pmos");
    for (name, n, p) in [
        ("vth0", 0.7, -0.7),
        ("u0", 0.067, 0.025),
        ("noia", 1e20, 9.9e18),
        ("noib", 5e4, 2.4e3),
        ("noic", -1.4e-12, 1.4e-12),
    ] {
        close(nmos.get(name).unwrap(), n);
        close(pmos.get(name).unwrap(), p);
    }
    assert_eq!((nmos.polarity, pmos.polarity), (1, -1));
    // A later model-level type flag changes both polarity and dependent
    // defaults, matching C's setter order rather than the card's leading name.
    let flipped = model("nmos", "pmos");
    assert_eq!(flipped.polarity, 1);
    close(flipped.get("vth0").unwrap(), 0.7);
    close(flipped.get("u0").unwrap(), 0.067);
}

#[test]
fn mobility_selector_changes_the_uc_defaults() {
    close(model("", "nmos").get("uc").unwrap(), -0.0465e-9);
    close(model("", "nmos").get("uc1").unwrap(), -0.056e-9);
    let three = model("mobmod=3", "nmos");
    close(three.get("uc").unwrap(), -0.0465);
    close(three.get("uc1").unwrap(), -0.056);
    // An explicit value always wins, whatever MOBMOD selects.
    let given = model("mobmod=3 uc=1e-9", "nmos");
    close(given.get("uc").unwrap(), 1e-9);
    assert!(given.is_given("uc"));
}

#[test]
fn dependent_defaults_follow_the_given_values_they_reference() {
    let m = model(
        "tox=8n drout=0.8 lint=20n wint=30n ll=1e-18 lw=2e-18 lwl=3e-25 \
         wl=4e-18 ww=5e-18 wwl=6e-25 cjsw=7e-10 pbsw=0.9 mjsw=0.2",
        "nmos",
    );
    close(m.get("toxm").unwrap(), 8e-9);
    close(m.get("dsub").unwrap(), 0.8);
    close(m.get("dlc").unwrap(), 20e-9);
    close(m.get("dwc").unwrap(), 30e-9);
    for (to, from) in [
        ("llc", "ll"),
        ("lwc", "lw"),
        ("lwlc", "lwl"),
        ("wlc", "wl"),
        ("wwc", "ww"),
        ("wwlc", "wwl"),
        ("cjswg", "cjsw"),
        ("pbswg", "pbsw"),
        ("mjswg", "mjsw"),
    ] {
        assert_eq!(m.get(to).unwrap(), m.get(from).unwrap(), "{to}");
        assert!(!m.is_given(to), "{to} became given");
    }
    // An explicit copy target is not overwritten.
    let explicit = model("tox=8n toxm=9n", "nmos");
    close(explicit.get("toxm").unwrap(), 9e-9);
    assert!(explicit.is_given("toxm"));
}

#[test]
fn oxide_capacitance_and_overlap_defaults_match_the_setup_formulas() {
    const EPS_OX: f64 = 3.453_133e-11;
    #[allow(clippy::approx_constant)] // b3set.c's intentionally rounded PI
    const PI: f64 = 3.141_592_654;
    let m = model("tox=10n xj=0.15u dlc=10n cgdl=1e-12 cgsl=2e-12", "nmos");
    let cox = EPS_OX / 10e-9;
    close(m.oxide_capacitance, cox);
    close(m.get("cgdo").unwrap(), 10e-9 * cox - 1e-12);
    close(m.get("cgso").unwrap(), 10e-9 * cox - 2e-12);
    close(m.get("cgbo").unwrap(), 0.);
    close(
        m.get("cf").unwrap(),
        2. * EPS_OX / PI * (1. + 0.4e-6 / 10e-9_f64).ln(),
    );

    // DLC given but nonpositive selects C's XJ branch.
    let zero = model("tox=10n xj=0.15u dlc=0", "nmos");
    close(zero.get("cgdo").unwrap(), 0.6 * 0.15e-6 * cox);
    close(zero.get("cgso").unwrap(), 0.6 * 0.15e-6 * cox);

    // DLC ungiven (hence 0) also selects the XJ branch, even with CGDL set.
    let absent = model("tox=10n xj=0.15u cgdl=1e-12", "nmos");
    close(absent.get("cgdo").unwrap(), 0.6 * 0.15e-6 * cox);

    let given = model("tox=10n cf=1p cgdo=2p cgso=3p cgbo=4p dwc=20n", "nmos");
    close(given.get("cf").unwrap(), 1e-12);
    close(given.get("cgdo").unwrap(), 2e-12);
    close(given.get("cgso").unwrap(), 3e-12);
    close(given.get("cgbo").unwrap(), 4e-12);
}

#[test]
fn tnom_defaults_to_the_nominal_temperature_and_keeps_kelvin_setters() {
    let default = numbered("", "nmos", 27., 27.);
    close(default.get("tnom").unwrap(), 300.15);
    assert!(!default.is_given("tnom"));
    let other = numbered("", "nmos", 100., 50.);
    close(other.get("tnom").unwrap(), 323.15);
    let explicit = numbered("tnom=40", "nmos", 27., 50.);
    close(explicit.get("tnom").unwrap(), 313.15);
    assert!(explicit.is_given("tnom"));
}

#[test]
fn aliases_share_the_given_bit_and_the_last_setter_value() {
    let m = model("vth0=0.3 vtho=0.4", "nmos");
    close(m.get("vtho").unwrap(), 0.4);
    close(m.get("vth0").unwrap(), 0.4);
    assert!(m.is_given("vth0") && m.is_given("vtho"));
}

#[test]
fn invalid_nqs_selectors_are_reset_with_a_diagnostic_instead_of_silence() {
    let m = model("nqsmod=2 acnqsmod=-1", "nmos");
    close(m.get("nqsmod").unwrap(), 0.);
    close(m.get("acnqsmod").unwrap(), 0.);
    assert_eq!(
        m.warnings.iter().map(|w| w.parameter).collect::<Vec<_>>(),
        ["nqsmod", "acnqsmod"]
    );
    for warning in &m.warnings {
        assert!(warning.c_reference.contains("b3set.c"));
        assert_eq!(warning.replacement, 0.);
    }
    assert_eq!(m.warnings[0].supplied, 2.);
    assert_eq!(m.warnings[1].supplied, -1.);
    // Valid selectors are untouched, including explicit zeros.
    for card in ["", "nqsmod=1 acnqsmod=1", "nqsmod=0 acnqsmod=0"] {
        assert!(model(card, "nmos").warnings.is_empty(), "{card}");
    }
}

#[test]
fn model_defaults_do_not_pre_empt_b3check_oxide_and_junction_fatals() {
    // C reports these in `b3check.c` during instance setup, so the model-only
    // projection must not turn them into a different, earlier error.
    let zero = model("tox=0", "nmos");
    assert!(zero.is_given("tox"));
    assert!(!zero.oxide_capacitance.is_finite(), "1/0 is infinite");
    let negative = model("tox=-1n", "nmos");
    assert!(negative.oxide_capacitance.is_finite() && negative.oxide_capacitance < 0.);
    // A negative IJTH is legal until the instance check runs.
    let ijth = model("ijth=-1", "nmos");
    close(ijth.get("ijth").unwrap(), -1.);
    assert!(ijth.is_given("ijth"));
}

#[test]
fn defaults_do_not_enable_simulation_or_mutate_the_input_projection() {
    let text = "atomic\nm1 d g 0 0 nn l=1u w=2u\n.model nn nmos level=8 tox=8n\n.end\n";
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("atomic.cir"), text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    let input = bsim3::model_parameters(&resolved).unwrap();
    let defaults = input.model_defaults(&ModelContext::new(27., 27.)).unwrap();
    close(defaults.get("toxm").unwrap(), 8e-9);
    // The ordered explicit projection is unchanged: TOXM was never supplied.
    assert!(input.inputs.get("toxm").is_none());
    assert!(input.inputs.get("tox").is_some());
}
