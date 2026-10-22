//! #189 item 1: circuit-scoped BSIM4 model/check and geometry state.
//! Pinned currents/fatal outcomes: ngspice 059066b5, macOS arm64,
//! fidelity-tools/bin/ngspice-trace-nofma (-O2 -ffp-contract=off), SPARSE.
#![allow(clippy::excessive_precision)] // Keep C's numdgt=17 tokens verbatim.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::{Circuit, ModelContext, ModelResolver};
use ngspice_rs::netlist::{Parser, ast::Netlist, source::parse_deck_text};
use ngspice_rs::primitives::SpiceError;

const SOURCES: &str = "v1 d1 0 1\nv2 d2 0 1\nv3 d3 0 1\nvg g 0 1\n";
const GEOMETRIES: &str =
    "m1 d1 g 0 0 nn l=1u w=2u\nm2 d2 g 0 0 nn l=2u w=2u\nm3 d3 g 0 0 nn l=1u w=2u\n";
const RESETS: &str = ".model nn nmos level=54 version=4.8.3 a1=0.8 a2=1.2 rdsw=-10 prwg=-1 ckappas=0.01 ckappad=0.01\n";

fn parse(body: &str, analysis: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("bsim4-model-state.cir"),
            &format!("model state\n{SOURCES}{body}{analysis}\n.end\n"),
        ))
        .unwrap()
}

fn run(netlist: &Netlist) -> Result<Vec<Plot>, SpiceError> {
    let config = RunConfig::from_netlist(netlist)?;
    let mut circuit = config.circuit(netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    runner(request.kind)?.run_plots(&mut circuit, &request, &config.context())
}

fn same(value: f64, expected: f64) {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(
            value.to_bits(),
            expected.to_bits(),
            "{value:e} != {expected:e}"
        );
    } else {
        assert!(
            (value - expected).abs() <= 1e-9 * expected.abs() + 1e-15,
            "{value:e} != {expected:e}"
        );
    }
}

#[test]
fn checked_geometry_resets_are_shared_but_other_geometry_is_not_reset() {
    let plots = run(&parse(&format!("{GEOMETRIES}{RESETS}"), ".op")).unwrap();
    let p = plots.last().unwrap();
    for (name, expected) in [
        ("i(v1)", -2.4434869357909588e-4),
        ("i(v2)", -1.348300485862076e-4),
        ("i(v3)", -2.4434869357909588e-4),
    ] {
        same(p.value(name, 0).unwrap().re, expected);
    }
}

#[test]
fn only_last_parsed_instance_is_checked_including_size_dependent_fatals() {
    let model = ".model nn nmos level=54 version=4.8.3 pclm=0.1 lpclm=-0.15\n";
    let small = "m1 d1 g 0 0 nn l=1u w=2u\n";
    let large = "m2 d2 g 0 0 nn l=2u w=2u\n";
    let plots = run(&parse(&format!("{small}{large}{model}"), ".op")).unwrap();
    same(
        plots.last().unwrap().value("i(v1)", 0).unwrap().re,
        -2.2747652266160849e-4,
    );
    same(
        plots.last().unwrap().value("i(v2)", 0).unwrap().re,
        -1.2978353019043814e-4,
    );
    let error = run(&parse(&format!("{large}{small}{model}"), ".op"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("m1") && error.contains("Pclm = -0.05"),
        "{error}"
    );
}

#[test]
fn model_first_reference_order_and_asymmetric_name_prefix_match_c() {
    // Model declarations deliberately do not determine model traversal order.
    let models =
        ".model n nmos level=54 version=4.8.3 pclm=-1\n.model nch nmos level=54 version=4.8.3\n";
    let short = "m1 d1 g 0 0 n l=1u w=2u\n";
    let long = "m2 d2 g 0 0 nch l=1u w=2u\n";
    let plots = run(&parse(&format!("{short}{long}{models}"), ".op")).unwrap();
    same(
        plots.last().unwrap().value("i(v1)", 0).unwrap().re,
        -2.2747652266160849e-4,
    );
    same(
        plots.last().unwrap().value("i(v2)", 0).unwrap().re,
        -2.3959396794550841e-4,
    );
    let error = run(&parse(&format!("{long}{short}{models}"), ".op"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Pclm = -1"), "{error}");
    // A skipped name is not a member of a permanent checked-name set:
    // nch -> x -> n must check n, despite its prefix of the earlier nch.
    let separated = format!(
        "{short}mx d3 g 0 0 x l=1u w=2u\n{long}{models}.model x nmos level=54 version=4.8.3\n"
    );
    assert!(
        run(&parse(&separated, ".op"))
            .unwrap_err()
            .to_string()
            .contains("Pclm = -1")
    );
}

#[test]
fn independently_elaborated_circuits_have_fresh_check_name_domains() {
    let clean = format!("{GEOMETRIES}.model nn nmos level=54 version=4.8.3\n");
    run(&parse(&clean, ".op")).unwrap();
    let fatal = format!("{GEOMETRIES}.model nn nmos level=54 version=4.8.3 pclm=-1\n");
    for _ in 0..2 {
        assert!(
            run(&parse(&fatal, ".op"))
                .unwrap_err()
                .to_string()
                .contains("Pclm = -1")
        );
    }
}

#[test]
fn batches_are_atomic_and_only_later_bsim4_additions_are_refused() {
    let netlist = parse(&format!("{GEOMETRIES}{RESETS}"), ".op");
    let models = ModelResolver::new(&netlist.models).unwrap();
    let context = ModelContext::default();
    let mut circuit = Circuit::new();
    circuit.add_instances(&[], &models, &context).unwrap();
    circuit
        .add_instances(&netlist.devices[..4], &models, &context)
        .unwrap();
    let before_nodes = circuit.nodes().len();
    let before_devices = circuit.devices().len();
    // The invalid last-parsed instance must fail without binding shared state.
    let mut invalid = netlist.devices[4..].to_vec();
    invalid.last_mut().unwrap().nodes.pop();
    assert!(circuit.add_instances(&invalid, &models, &context).is_err());
    assert_eq!(circuit.nodes().len(), before_nodes);
    assert_eq!(circuit.devices().len(), before_devices);
    // A first BSIM4 batch into a populated non-BSIM4 circuit still succeeds.
    circuit
        .add_instances(&netlist.devices[4..], &models, &context)
        .unwrap();
    circuit.add_instances(&[], &models, &context).unwrap();
    let before_nodes = circuit.nodes().len();
    let before_devices = circuit.devices().len();
    let mut extra = netlist.devices[4].clone();
    extra.name = "mextra".into();
    extra.nodes[0] = "newnode".into();
    let error = circuit.add_instance(&extra, &models, &context).unwrap_err();
    assert!(matches!(error, SpiceError::NotYetPorted { .. }));
    assert!(error.to_string().contains("b4check.c") && error.to_string().contains("b4temp.c"));
    assert_eq!(circuit.nodes().len(), before_nodes);
    assert_eq!(circuit.devices().len(), before_devices);
    let extra = parse("rnew d1 0 1k\n", ".op");
    circuit
        .add_instance(extra.device("rnew").unwrap(), &models, &context)
        .unwrap();
}

/// C points of `.dc` instance sweeps on the shared `RESETS` model.
const SHARED_SWEEPS: [(&str, [[f64; 3]; 2]); 2] = [
    // Sweeping m3 also moves m1 (same geometry): every override pass
    // rebuilds the size knots, and the check that reset a1/a2/rdsw/prwg on
    // the first pass's knot does not run again.
    (
        ".dc @m3[w] 2u 3u 1u",
        [
            [
                -2.4612436882601971e-4,
                -1.3483004858620760e-4,
                -2.4612436882601971e-4,
            ],
            [
                -2.4612436882601971e-4,
                -1.3483004858620760e-4,
                -3.7665243471317160e-4,
            ],
        ],
    ),
    (
        ".dc @m2[l] 1u 2u 1u",
        [
            [
                -2.4612436882601971e-4,
                -2.4612436882601971e-4,
                -2.4612436882601971e-4,
            ],
            [
                -2.4612436882601971e-4,
                -1.3483004858620760e-4,
                -2.4612436882601971e-4,
            ],
        ],
    ),
];

#[test]
fn shared_model_instance_sweeps_rerun_temp_for_every_instance() {
    for (analysis, expected) in SHARED_SWEEPS {
        let plots = run(&parse(&format!("{GEOMETRIES}{RESETS}"), analysis)).unwrap();
        let plot = plots.last().unwrap();
        assert_eq!(plot.points.len(), 2, "{analysis}");
        for (point, currents) in expected.iter().enumerate() {
            for (name, value) in ["i(v1)", "i(v2)", "i(v3)"].iter().zip(currents) {
                same(plot.value(name, point).unwrap().re, *value);
            }
        }
    }
    // The override itself leaves the circuit's devices untouched.
    let netlist = parse(&format!("{GEOMETRIES}{RESETS}"), ".op");
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let circuit = config.circuit(&netlist).unwrap();
    let w = |circuit: &Circuit| {
        circuit
            .devices()
            .iter()
            .find(|d| d.name() == "m1")
            .unwrap()
            .observation_parameter("w", &ModelContext::default())
            .unwrap()
    };
    let before = w(&circuit);
    circuit
        .instance_override("m1", "w", 3e-6, &ModelContext::default())
        .unwrap();
    assert_eq!(before, w(&circuit));
}

#[test]
fn different_scoped_models_and_single_instance_multiplicity_do_not_refuse_sweeps() {
    let netlist = parse(
        "m1 d1 g 0 0 nn l=1u w=2u m=2 nf=2\nm2 d2 g 0 0 other l=1u w=2u\n.model nn nmos level=54 version=4.8.3\n.model other nmos level=54 version=4.8.3\n",
        ".dc @m1[w] 2u 3u 1u",
    );
    run(&netlist).unwrap();
    let subckt = "x1 d1 g cell\nx2 d2 g cell\n.subckt cell d g\nm1 d g 0 0 nn l=1u w=2u\n.model nn nmos level=54 version=4.8.3\n.ends\n";
    run(&parse(subckt, ".dc @m.x1.m1[w] 2u 3u 1u")).unwrap();
}

#[test]
fn shared_model_source_and_temperature_sweeps_still_run() {
    let body = format!("{GEOMETRIES}.model nn nmos level=54 version=4.8.3\n");
    for (analysis, expected) in [
        (
            ".dc v1 0.5 1 0.5",
            [
                [
                    -2.1155895296955042e-4,
                    -1.3297767425087884e-4,
                    -2.3959396794550841e-4,
                ],
                [
                    -2.3959396794550841e-4,
                    -1.3297767425087884e-4,
                    -2.3959396794550841e-4,
                ],
            ],
        ),
        (
            ".dc temp 37 47 10",
            [
                [
                    -2.3085912386178181e-4,
                    -1.2779158154330021e-4,
                    -2.3085912386178181e-4,
                ],
                [
                    -2.2267638623498556e-4,
                    -1.2296277408157310e-4,
                    -2.2267638623498556e-4,
                ],
            ],
        ),
    ] {
        let plots = run(&parse(&body, analysis)).unwrap();
        let plot = plots.last().unwrap();
        assert_eq!(plot.points.len(), 2);
        for (point, currents) in expected.iter().enumerate() {
            for (name, value) in ["i(v1)", "i(v2)", "i(v3)"].iter().zip(currents) {
                same(plot.value(name, point).unwrap().re, *value);
            }
        }
    }
}

/// Reproduce the pinned outcomes without overwriting any committed oracle data.
#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM4 shared-model check proofs"]
fn c_bsim4_model_state_proofs() {
    use std::{fs, process::Command};
    let binary =
        std::path::PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let dir =
        Scratch(std::env::temp_dir().join(format!("bsim4-model-state-{}", std::process::id())));
    fs::create_dir_all(&dir.0).unwrap();
    let geometry = format!("{GEOMETRIES}{RESETS}");
    let small = "m1 d1 g 0 0 nn l=1u w=2u\n";
    let large = "m2 d2 g 0 0 nn l=2u w=2u\n";
    let fatal_model = ".model nn nmos level=54 version=4.8.3 pclm=0.1 lpclm=-0.15\n";
    let short = "m1 d1 g 0 0 n l=1u w=2u\n";
    let long = "m2 d2 g 0 0 nch l=1u w=2u\n";
    let models =
        ".model n nmos level=54 version=4.8.3 pclm=-1\n.model nch nmos level=54 version=4.8.3\n";
    let cases = [
        (
            geometry,
            vec![
                -2.4434869357909588e-4,
                -1.348300485862076e-4,
                -2.4434869357909588e-4,
            ],
            None,
        ),
        (
            format!("{small}{large}{fatal_model}"),
            vec![-2.2747652266160849e-4, -1.2978353019043814e-4, 0.],
            None,
        ),
        (
            format!("{large}{small}{fatal_model}"),
            vec![],
            Some("Pclm = -0.05"),
        ),
        (
            format!("{short}{long}{models}"),
            vec![-2.2747652266160849e-4, -2.3959396794550841e-4, 0.],
            None,
        ),
        (format!("{long}{short}{models}"), vec![], Some("Pclm = -1")),
        (
            format!(
                "{short}mx d3 g 0 0 x l=1u w=2u\n{long}{models}.model x nmos level=54 version=4.8.3\n"
            ),
            vec![],
            Some("Pclm = -1"),
        ),
    ];
    for (analysis, expected) in SHARED_SWEEPS {
        let text = format!(
            "model state\n.options sparse\n{SOURCES}{GEOMETRIES}{RESETS}{analysis}\n.control\nset numdgt=17\nrun\nprint i(v1) i(v2) i(v3)\nquit\n.endc\n.end\n"
        );
        let path = dir.0.join("sweep.cir");
        fs::write(&path, text).unwrap();
        let output = Command::new(&binary)
            .args(["-b"])
            .arg(&path)
            .current_dir(&dir.0)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout).into_owned();
        let rows: Vec<Vec<f64>> = log
            .lines()
            .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()) && l.contains('\t'))
            .map(|l| {
                l.split_whitespace()
                    .skip(2)
                    .map(|v| v.parse().unwrap())
                    .collect()
            })
            .collect();
        assert_eq!(rows.len(), 2, "{analysis}: {log}");
        for (row, currents) in rows.iter().zip(expected) {
            for (actual, value) in row.iter().zip(currents) {
                same(*actual, value);
            }
        }
    }
    for (index, (body, expected, fatal)) in cases.iter().enumerate() {
        let text = format!(
            "model state\n.options sparse\n{SOURCES}{body}.control\nset numdgt=17\nop\nprint i(v1) i(v2) i(v3)\nquit\n.endc\n.end\n"
        );
        let path = dir.0.join("probe.cir");
        fs::write(&path, text).unwrap();
        let output = Command::new(&binary)
            .args(["-b"])
            .arg(&path)
            .current_dir(&dir.0)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        if let Some(fatal) = fatal {
            assert!(
                log.contains(fatal) && log.contains("op simulation(s) aborted"),
                "case {index}: {log}"
            );
            continue;
        }
        assert!(
            output.status.success() && !log.contains("aborted"),
            "case {index}: {log}"
        );
        for (name, value) in ["i(v1)", "i(v2)", "i(v3)"].iter().zip(expected) {
            let line = log
                .lines()
                .find(|l| l.starts_with(&format!("{name} = ")))
                .unwrap_or_else(|| panic!("case {index}: {log}"));
            let actual: f64 = line.split_once(" = ").unwrap().1.parse().unwrap();
            same(actual, *value);
        }
        if index == 0 {
            assert_eq!(
                log.matches("Checking parameters for BSIM 4.8 model nn")
                    .count(),
                1
            );
            assert_eq!(log.matches("Warning: A2 = 1.2").count(), 1);
        }
    }
}

/// #189 item 2. After a `.tran`, C's `CKTag` keeps the last step's
/// coefficients and the NQS charges integrate with them in the
/// charge-computing loads of the analyses `analInfo` runs later (`.pz`,
/// `.tf`, `.disto`, `.noise`); the port integrates with zero there. Only the
/// `ccap` state slots differ, which none of those analyses reads: each is
/// EXACT against the FMA-free oracle with and without the earlier `.tran`
/// (`trnqsmod=1`, `acnqsmod` 0 and 1). Pin that the `.tran` changes nothing.
#[test]
fn analyses_after_an_nqs_transient_match_their_standalone_runs() {
    use ngspice_rs::analysis::rawfile::{RawFile, RawFormat};
    use ngspice_rs::cli::simulate::run_with_format;
    let deck = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/netlists/m11_bsim4_tran.cir"),
    )
    .unwrap()
    .replace("+ trnqsmod=1", "+ trnqsmod=1 acnqsmod=1")
    .replace("vin in 0 pulse", "vin in 0 dc 0.6 ac 1 pulse");
    assert!(deck.contains("acnqsmod=1") && deck.contains(".tran 10p 1.5n\n"));
    let dir = std::env::temp_dir().join(format!("bsim4-nqs-after-tran-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let simulate = |name: &str, text: &str| {
        let input = dir.join(format!("{name}.cir"));
        let output = dir.join(format!("{name}.raw"));
        std::fs::write(&input, text).unwrap();
        run_with_format(&input, &output, false, Some(RawFormat::Binary)).unwrap();
        RawFile::parse_bytes(&std::fs::read(&output).unwrap()).unwrap()
    };
    for analysis in [
        ".tf v(o2) vin",
        ".noise v(o2) vin dec 3 1meg 10g",
        ".pz in 0 o2 0 vol pol",
        ".disto dec 3 1meg 1g",
    ] {
        let after = simulate(
            "after",
            &deck.replace(".tran 10p 1.5n\n", &format!(".tran 10p 1.5n\n{analysis}\n")),
        );
        let alone = simulate(
            "alone",
            &deck.replace(".tran 10p 1.5n\n", &format!("{analysis}\n")),
        );
        // The batch writes the .tran plot first (analInfo order).
        let later: Vec<_> = after.plots.iter().skip(1).map(|p| &p.plot).collect();
        let standalone: Vec<_> = alone.plots.iter().map(|p| &p.plot).collect();
        assert_eq!(later.len(), standalone.len(), "{analysis}");
        for (a, b) in later.iter().zip(&standalone) {
            assert_eq!(a.variables, b.variables, "{analysis}");
            assert_eq!(a.points.len(), b.points.len(), "{analysis}");
            for (pa, pb) in a.points.iter().zip(&b.points) {
                for (x, y) in pa.iter().zip(pb) {
                    assert!(
                        x.re.to_bits() == y.re.to_bits() && x.im.to_bits() == y.im.to_bits(),
                        "{analysis}: {x:?} != {y:?}"
                    );
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
