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
