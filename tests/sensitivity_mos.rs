//! Classic-MOS `.sens` (#164), through the production analysis driver.
//! The five C goldens pin the complete parameter walk, including restoration
//! side effects that are deliberately not physical central derivatives.
use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::SpiceResult;
use std::path::Path;

fn run(text: &str) -> SpiceResult<Plot> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("sens_mos.cir"), text))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    runner(request.kind)?.run(&mut config.circuit(&netlist)?, &request, &config.context())
}
fn value(plot: &Plot, name: &str) -> f64 {
    plot.points[0][plot
        .variable_index(name)
        .unwrap_or_else(|| panic!("missing {name}"))]
    .re
}
fn fixture(level: u8, per_device: usize) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let deck =
        std::fs::read_to_string(root.join(format!("conformance/netlists/sens_mos{level}.cir")))
            .unwrap();
    let golden = RawFile::parse(
        &std::fs::read_to_string(root.join(format!("conformance/golden/sens_mos{level}.raw")))
            .unwrap(),
    )
    .unwrap();
    let plot = run(&deck).unwrap();
    let reference = &golden.plots[0].plot;
    assert_eq!(plot.plotname, reference.plotname);
    // The control `write` used to capture goldens sorts vectors after the
    // scale; batch-mode table order is checked separately by c_sens_reference.
    let variables = |p: &Plot| {
        p.variables
            .iter()
            .map(|v| (v.name.clone(), v.unit.clone(), v.is_real))
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        variables(&plot),
        variables(reference),
        "level {level}: names/units"
    );
    assert_eq!(plot.points.len(), 1);
    for variable in &reference.variables {
        let a = value(&plot, &variable.name);
        let b = value(reference, &variable.name);
        assert!(
            (a - b).abs() <= 1e-9 + 1e-6 * b.abs(),
            "level {level} {}: {a:e} vs {b:e}",
            variable.name
        );
    }
    for instance in ["m1", "m2", "m3"] {
        let count = plot
            .variables
            .iter()
            .filter(|v| v.name.starts_with(&format!("v({instance}")))
            .count();
        assert_eq!(count, per_device, "level {level}: {instance}");
    }
    if matches!(level, 3 | 9) {
        for k in ["xd", "alpha", "input_delta"] {
            assert_eq!(
                value(&plot, &format!("v(m1:{k})")),
                0.,
                "C's missing model setters"
            );
        }
        assert!(
            value(&plot, "v(m1:delta)").abs() > 1e7,
            "scaled ask/raw setter mismatch"
        );
    }
    if level == 6 {
        assert_eq!(value(&plot, "v(m1:lambda)"), 0., "LAMBDA only feeds setup");
        assert!(value(&plot, "v(m1:lambda0)").abs() > 1e-4);
        assert!(
            value(&plot, "v(m1_w)").abs() > 1e7,
            "W ask includes M, setter does not"
        );
        assert!(
            value(&plot, "v(m1:tnom)").abs() > 1.,
            "kelvin ask/Celsius setter"
        );
        for k in ["nvth", "ps"] {
            assert_eq!(value(&plot, &format!("v(m1:{k})")), 0.);
        }
    }
    if level == 9 {
        assert!(
            plot.variable_index("v(m1_dtemp)").is_none(),
            "MOS9ask has no DTEMP"
        );
    }
}
#[test]
fn mos1_matches_cs_full_walk() {
    fixture(1, 37);
}
#[test]
fn mos2_matches_cs_full_walk() {
    fixture(2, 43);
}
#[test]
fn mos3_matches_cs_full_walk() {
    fixture(3, 47);
}
#[test]
fn mos6_matches_cs_full_walk() {
    fixture(6, 40);
}
#[test]
fn mos9_matches_cs_full_walk() {
    fixture(9, 45);
}

#[test]
fn mos1_channel_derivatives_match_the_square_law() {
    let plot = run(
        "MOS1 derivative\n.options reltol=1e-10 abstol=1e-16 vntol=1e-13\n\
        vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm l=10u w=20u\n\
        .model mm nmos level=1 vto=0.7 kp=50u lambda=0.02\n\
        .sens i(vd) m1:kp m1:vto\n.end\n",
    )
    .unwrap();
    for (k, expected) in [("v(m1:kp)", -1.7914), ("v(m1:vto)", 1.378e-4)] {
        let actual = value(&plot, k);
        assert!(
            (actual - expected).abs() <= 1e-9 + 1e-6 * expected.abs(),
            "{k}: {actual} vs {expected}"
        );
    }
}

#[test]
fn invalid_absolute_doping_perturbation_is_an_error_not_a_partial_result() {
    // A default NSUB=0 gets the absolute 1e-6 cm^-3 step. MOS2/3/9temp
    // return E_BADPARM below intrinsic density; C's sens_temp ignores it and
    // continues with stale derived fields. Until that partial-record state is
    // ported, propagate the error rather than fabricate a sensitivity plot.
    for level in [2, 3, 9] {
        let error = run(&format!(
            "Doping failure\nvd d 0 3\nvg g 0 2\n\
            m1 d g 0 0 mm l=10u w=20u\n.model mm nmos level={level}\n\
            .sens i(vd)\n.end\n"
        ))
        .unwrap_err();
        assert!(error.to_string().contains("NSUB"), "{error}");
    }
}

#[test]
fn zero_effective_length_in_a_perturbed_load_fails_explicitly() {
    // Zero LD gets the absolute 1 um step: a drawn L=2 um collapses to 0.
    let error = run(
        "Invalid Leff\nvd d 0 3\nvg g 0 2\nm1 d g 0 0 mm l=2u w=10u\n\
        .model mm nmos level=1\n.sens i(vd)\n.end\n",
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("effective channel length"),
        "{error}"
    );
}

/// A `.sens` filter that never selects `NSUB` avoids the unset-doping step that
/// the full walk takes, so a default level-2/3/9 card still produces C's values
/// instead of the documented explicit error. Compared against ngspice-47+ on
/// `.model mm nmos level=N tox=20n` (no NSUB) with `l=10u w=20u`.
#[test]
fn filtering_the_doping_parameter_avoids_the_failed_temperature_callback() {
    let stage = ".options reltol=1e-10 vntol=1e-13 abstol=1e-16\n\
        vd d 0 3\nvg g 0 2\nvsense s 0 0\nm1 d g s 0 mm l=10u w=20u\n";
    for (level, filter, name, reference) in [
        (2, "m1:kp", "v(m1:kp)", 4.000000001197),
        (2, "m1:vto", "v(m1:vto)", -0.000414377152),
        (2, "m1_l", "v(m1_l)", -41.437684159635),
        (3, "m1:theta", "v(m1:theta)", -0.000828752854),
        (3, "m1_w", "v(m1_w)", 20.718862798921),
    ] {
        let plot = run(&format!(
            "{stage}.model mm nmos level={level} tox=20n\n.sens i(vsense) {filter}\n.end\n"
        ))
        .unwrap();
        let actual = value(&plot, name);
        assert!(
            (actual - reference).abs() <= 1e-9 + 1e-6 * reference.abs(),
            "level {level} {name}: {actual} vs {reference}"
        );
    }
    // MOS9 has no KAPPA column value on this card: C's `mos9mask.c` answers it
    // but the walk's own KAPPA step leaves the load's channel unchanged here.
    let plot = run(&format!(
        "{stage}.model mm nmos level=9 tox=20n\n.sens i(vsense) m1:kappa\n.end\n"
    ))
    .unwrap();
    assert_eq!(value(&plot, "v(m1:kappa)"), 0.);
}

/// A `.model` card's instance parameters become defaults for every instance,
/// and C discards `m` outright (`inpgmod.c`), so the model-level multiplier
/// cannot scale the sensitivity columns. Measured: both cards C-simulate to the
/// same 52 columns.
#[test]
fn a_model_card_multiplier_does_not_scale_the_columns() {
    let body = ".options reltol=1e-10 vntol=1e-13 abstol=1e-16\n\
        vd d 0 3\nvg g 0 2\nvsense s 0 0\nm1 d g s 0 mm\n\
        .model mm nmos level=1 {}\n.sens i(vsense)\n.end\n";
    let with = run(&body.replace("{}", "m=3 w=25u l=12u")).unwrap();
    let without = run(&body.replace("{}", "w=25u l=12u")).unwrap();
    assert_eq!(with.variables.len(), without.variables.len());
    for name in [
        "v(m1:kp)",
        "v(m1:vto)",
        "v(m1_l)",
        "v(m1_temp)",
        "v(vsense)",
    ] {
        assert_eq!(value(&with, name), value(&without, name), "{name}");
    }
}
