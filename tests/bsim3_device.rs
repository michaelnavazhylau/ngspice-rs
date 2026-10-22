//! Offline BSIM3 device tests: DC `.op` through the solver, terminal/state
//! wiring and explicit refusals for the unported physics. C parity lives in the
//! opt-in `c_bsim3_device_reference` suite.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::{Circuit, ModelContext};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::SpiceError;

fn deck(body: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("bsim3-device.cir"),
            &format!("BSIM3 device\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&deck(body)).unwrap()
}
fn operating(body: &str) -> Plot {
    let netlist = deck(&format!("{body}\n.op"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}

/// Bias of the fixed-point tests: C's bare BSIM3 default model is above
/// threshold at Vgs = 2.5 V.
const CORE: &str = "vd d 0 1\nvg g 0 2.5\nvs s 0 0\n";

#[test]
fn operating_point_solves_the_drain_current_in_both_polarities() {
    for (family, polarity) in [("nmos", 1.), ("pmos", -1.)] {
        let body = format!(
            "vd d 0 {}e\nvg g 0 {}e\nvs s 0 0\n\
             m1 d g s 0 nn l=1u w=10u\n.model nn {family} level=8",
            1.0 * polarity,
            2.5 * polarity
        );
        let plot = operating(&body);
        let current = plot.value("i(vd)", 0).unwrap().re;
        // C's bare BSIM3 default model conducts about 1.2e-4 A here; the sign
        // follows the polarity because the drain source value does.
        assert!(current.is_finite(), "{family}: {current}");
        // `i(vd)` is the current the source drives into the drain, so it is
        // negative of the device's drain current for both polarities.
        assert!(
            current * polarity < -1e-5 && current * polarity > -1e-3,
            "{family}: {current}"
        );
        assert!(plot.value("i(vg)", 0).unwrap().re.abs() < 1e-15, "{family}");
    }
}

#[test]
fn device_state_and_observations_expose_the_c_quantities() {
    let netlist = deck(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8"
    ));
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let device = circuit.device("m1").expect("m1");
    assert_eq!(device.designator(), 'm');
    assert_eq!(device.terminals().len(), 4);
    // C's `BSIM3numStates` (`bsim3def.h`): the four voltages, the
    // `qb`/`qg`/`qd` charges and their currents, `qbs`, `qbd`, the NQS
    // `qcheq`/`qcdump` pairs and `qdef`.
    assert_eq!(device.state_count(), 17);
    assert!(device.is_nonlinear());
    assert!(!device.has_start_settings());

    let plot = operating(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8"
    ));
    let solution: Vec<f64> = circuit
        .nodes()
        .nodes()
        .iter()
        .map(|node| {
            plot.value(&format!("v({})", node.name), 0)
                .map_or(0., |value| value.re)
        })
        .collect();
    let voltage = |node: ngspice_rs::primitives::NodeId| solution[node.index()];
    let context = ModelContext::default();
    let device = circuit.device("m1").unwrap();
    let id = device
        .observation_operating("id", &context, &voltage, false)
        .unwrap()
        .unwrap();
    let gm = device
        .observation_operating("gm", &context, &voltage, false)
        .unwrap()
        .unwrap();
    let vth = device
        .observation_operating("vth", &context, &voltage, false)
        .unwrap()
        .unwrap();
    assert!(id > 0. && gm > 0.);
    assert!(vth > 1.0 && vth < 3.0, "{vth}");
    // The charge block's asks are available at `capMod=3`. The charge
    // currents `cqg`/`cqb`/`cqd` are output rows of C's instance table (zero
    // at an operating point, as C prints them); the junction charges are not
    // in the table, so they are not askable.
    let ask = |name: &str| {
        device
            .observation_operating(name, &context, &voltage, false)
            .unwrap()
    };
    let qg = ask("qg").expect("qg");
    let qb = ask("qb").expect("qb");
    let qd = ask("qd").expect("qd");
    assert!(qg.is_finite() && qg != 0.);
    assert!(qb.is_finite() && qd.is_finite());
    // The gate/bulk/drain charges leave the source to balance them, so their
    // sum is the source charge, not zero.
    assert!((qg + qb + qd).abs() < 10. * qg.abs().max(qb.abs()));
    for name in [
        "cgg", "cgs", "cgd", "cbg", "cbs", "cbd", "cdg", "cds", "cdd",
    ] {
        assert!(ask(name).expect(name).is_finite(), "{name}");
    }
    assert_eq!(ask("capbd"), Some(0.0));
    assert_eq!(ask("capbs"), Some(0.0));
    for current in ["cqg", "cqb", "cqd"] {
        assert_eq!(ask(current), Some(0.0), "{current}");
    }
    for unavailable in ["qbs", "qbd", "nonsense"] {
        assert!(ask(unavailable).is_none(), "{unavailable}");
    }
}

#[test]
fn series_resistance_creates_the_internal_primes() {
    let netlist = deck(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u nrd=2 nrs=3\n.model nn nmos level=8 rsh=10"
    ));
    let circuit = Circuit::from_netlist(&netlist).unwrap();
    let device = circuit.device("m1").unwrap();
    assert_eq!(device.terminals().len(), 6);
    let plot = operating(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u nrd=2 nrs=3\n.model nn nmos level=8 rsh=10"
    ));
    let current = plot.value("i(vd)", 0).unwrap().re;
    assert!(current.is_finite() && current.abs() > 1e-6, "{current}");
    // The internal nodes are unknowns of the solve.
    assert!(circuit.unknown_count() >= 6);
}

#[test]
fn unported_analyses_refuse_instead_of_loading_zero() {
    let body = "vd d 0 1 ac 1\nvg g 0 2.5\nvs s 0 0\nm1 d g s 0 nn l=1u w=10u\n\
                .model nn nmos level=8";
    // `.sens` is not ported and must say so instead of loading zero. (C's
    // BSIM3 has no DEVdisto, so `.disto` runs with the device linear.)
    let (analysis, reference) = (".sens v(d)", "cktsgen.c");
    let netlist = deck(&format!("{body}\n{analysis}"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(
        matches!(error, SpiceError::NotYetPorted { .. }),
        "{analysis}: {error}"
    );
    assert!(error.to_string().contains(reference), "{analysis}: {error}");
    // A `capMod` other than 3 is a different equation set: AC and transient
    // refuse instead of substituting the charge-thickness model.
    for analysis in [".ac dec 1 1k 10k", ".tran 1u 2u"] {
        let netlist = deck(&format!(
            "vd d 0 1 ac 1\nvg g 0 2.5\nvs s 0 0\nm1 d g s 0 nn l=1u w=10u\n\
             .model nn nmos level=8 capmod=1\n{analysis}"
        ));
        let config = RunConfig::from_netlist(&netlist).unwrap();
        let request = config.request_for(&netlist.analyses[0]).unwrap();
        let mut circuit = config.circuit(&netlist).unwrap();
        let error = runner(request.kind)
            .unwrap()
            .run(&mut circuit, &request, &config.context())
            .unwrap_err();
        assert!(error.is_not_yet_ported(), "{analysis}: {error}");
        assert!(
            error.to_string().contains("capMod=1"),
            "{analysis}: {error}"
        );
    }
}

#[test]
fn off_and_ic_start_settings_are_reported() {
    let plain = circuit(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8"
    ));
    assert!(!plain.device("m1").unwrap().has_start_settings());
    for extra in ["off", "ic=(0.1,0.2,-0.3)", "icvds=0.1"] {
        let netlist = deck(&format!(
            "{CORE}\nm1 d g s 0 nn l=1u w=10u {extra}\n.model nn nmos level=8"
        ));
        let circuit = Circuit::from_netlist(&netlist).unwrap();
        assert!(
            circuit.device("m1").unwrap().has_start_settings(),
            "{extra}"
        );
    }
}

#[test]
fn acm_geometry_is_refused_by_the_factory() {
    let netlist = deck(&format!(
        "{CORE}\nm1 d g s 0 nn l=1u w=10u\n.model nn nmos level=8 acm=1"
    ));
    let error = Circuit::from_netlist(&netlist).unwrap_err();
    assert!(matches!(error, SpiceError::NotYetPorted { .. }), "{error}");
    assert!(
        error.to_string().contains("ACM_SourceDrainResistances"),
        "{error}"
    );
}

/// A deck whose small-signal behaviour is fixed by the operating-point asks:
/// a common-source stage with `rd`, a large load capacitor and a known gate
/// bias.
const SMALL_SIGNAL: &str = "vin g 0 dc 1.2 ac 1\nvdd vdd 0 1.8\nvs s 0 0\n\
                            rd vdd d 10k\ncl d 0 100p\n\
                            m1 d g s 0 nn l=0.25u w=2u\n\
                            .model nn nmos level=8\n.options reltol=1e-9";

/// `@m1[gm]` and `@m1[gds]` at the `.op` of [`SMALL_SIGNAL`].
fn op_asks() -> (f64, f64) {
    let netlist = deck(&format!("{SMALL_SIGNAL}\n.op"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let solution: Vec<f64> = circuit
        .nodes()
        .nodes()
        .iter()
        .map(|node| {
            plot.value(&format!("v({})", node.name), 0)
                .map_or(0., |value| value.re)
        })
        .collect();
    let voltage = |node: ngspice_rs::primitives::NodeId| solution[node.index()];
    let device = circuit.device("m1").unwrap();
    let ask = |name: &str| {
        device
            .observation_operating(name, &ModelContext::default(), &voltage, false)
            .unwrap()
            .unwrap()
    };
    (ask("gm"), ask("gds"))
}

#[test]
fn transfer_function_matches_the_small_signal_conductances() {
    // `v(d)/v(g) = -gm / (1/rd + gds)` and the output impedance is `rd || 1/gds`.
    let (gm, gds) = op_asks();
    assert!(gm > 0. && gds > 0.);
    let expected_gain = -gm / (1.0 / 10e3 + gds);
    let expected_impedance = 1.0 / (1.0 / 10e3 + gds);

    let netlist = deck(&format!("{SMALL_SIGNAL}\n.tf v(d) vin"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let gain = plot.value("v(Transfer_function)", 0).unwrap().re;
    let impedance = plot.value("v(output_impedance_at_V(d))", 0).unwrap().re;
    // `.tf` solves with the load matrix, which also carries the junction and
    // gmin conductances, so this is a 1e-6 identity rather than bit-exact.
    assert!(
        (gain - expected_gain).abs() <= 1e-6 * expected_gain.abs(),
        "gain {gain:e} vs {expected_gain:e}"
    );
    assert!(
        (impedance - expected_impedance).abs() <= 1e-6 * expected_impedance,
        "impedance {impedance:e} vs {expected_impedance:e}"
    );
}

#[test]
fn pole_zero_and_ac_agree_on_the_dominant_pole() {
    let (_, gds) = op_asks();
    // The 100 pF load dominates every device capacitance, so the pole of
    // `v(d)` is `-(1/rd + gds)/C` per the `b3acld.c`/`b3pzld.c` pencil.
    let expected_radians = -(1.0 / 10e3 + gds) / 100e-12;

    let netlist = deck(&format!("{SMALL_SIGNAL}\n.pz g 0 d 0 cur pol"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let pole = plot.value("v(pole(1))", 0).unwrap();
    assert!(
        (pole.re - expected_radians).abs() <= 1e-3 * expected_radians.abs(),
        "pole {pole:?} vs {expected_radians:e}"
    );
    assert!(pole.im.abs() <= 1e-6 * expected_radians.abs(), "{pole:?}");

    // The AC load is the same pencil: at 1 Hz the drain follows the
    // small-signal expression `-gm / (1/rd + gds)`.
    let netlist = deck(&format!("{SMALL_SIGNAL}\n.ac dec 2 1 100meg"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let (gm, gds) = op_asks();
    let expected_gain = -gm / (1.0 / 10e3 + gds);
    let low = plot.value("v(d)", 0).unwrap();
    assert_eq!(plot.value("frequency", 0).unwrap().re, 1.0);
    assert!(
        (low.re - expected_gain).abs() <= 1e-6 * expected_gain.abs(),
        "|v(d)| {low:?} vs {expected_gain:e}"
    );
}
