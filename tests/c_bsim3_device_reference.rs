//! Opt-in end-to-end BSIM3 comparison: the port's own `.op` solution against
//! C's, including internal-node circuits (RSH/NRD/NRS) and PMOS polarity.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn c_values(log: &str) -> BTreeMap<String, f64> {
    log.lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            Some((name.trim().to_owned(), value.trim().parse().ok()?))
        })
        .collect()
}

fn close(actual: f64, expected: f64, relative: f64, absolute: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= relative * expected.abs() + absolute,
        "{label}: Rust {actual:.12e}, C {expected:.12e}"
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; compares the port's .op with C's"]
fn operating_point_matches_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    // (model card, instance card, family, vgs, vds, temperature)
    let cases: [(&str, &str, &str, f64, f64, f64); 8] = [
        ("", "l=1u w=10u", "nmos", 2.5, 1.0, 27.),
        ("", "l=1u w=10u", "nmos", 3.0, 0.05, 27.),
        ("", "l=0.25u w=1u", "nmos", 2.2, 1.5, 27.),
        ("", "l=1u w=10u", "pmos", 2.5, 1.0, 27.),
        ("rsh=10", "l=1u w=10u nrd=2 nrs=3", "nmos", 2.5, 1.0, 27.),
        (
            "rsh=10 js=1e-5",
            "l=2u w=4u nrd=1 nrs=1 ad=2p as=1p pd=4u ps=3u",
            "nmos",
            2.0,
            0.7,
            27.,
        ),
        (
            "mobmod=2 ua=1e-9 ub=1e-18 uc=1e-11",
            "l=0.5u w=5u",
            "nmos",
            2.5,
            1.0,
            27.,
        ),
        ("paramchk=1", "l=1u w=10u", "nmos", 2.5, 1.0, 100.),
    ];
    for (index, (card, instance, family, vgs, vds, temperature)) in cases.into_iter().enumerate() {
        let polarity = if family == "nmos" { 1. } else { -1. };
        let (vgs, vds) = (vgs * polarity, vds * polarity);
        let model_line = format!(".model nn {family} level=8 {card}");
        let deck = format!(
            "devprobe\nvd d 0 {vds:e}\nvg g 0 {vgs:e}\nvs s 0 0\n\
             m1 d g s 0 nn {instance}\n{model_line}\n\
             .options temp={temperature} tnom=27\n\
             .control\nset numdgt=17\nop\nprint all\n\
             print @m1[id] @m1[gm] @m1[gds] @m1[gmbs] @m1[vth] @m1[vdsat]\nquit\n.endc\n.end\n"
        );
        let directory =
            Scratch(std::env::temp_dir().join(format!("bsim3-dev-{}-{index}", std::process::id())));
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(directory.0.join("probe.cir"), &deck).unwrap();
        let output = Command::new(&binary)
            .args(["-b", "probe.cir"])
            .current_dir(&directory.0)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        assert!(!log.contains("aborted"), "case {index}: {log}");
        let expected = c_values(&log);

        let text = format!(
            "devprobe\nvd d 0 {vds:e}\nvg g 0 {vgs:e}\nvs s 0 0\n\
             m1 d g s 0 nn {instance}\n{model_line}\n\
             .options temp={temperature} tnom=27\n.op\n.end\n"
        );
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .unwrap();
        // `.options temp` needs RunConfig so the analysis point carries the
        // deck temperature, exactly as C's `CKTtemp` does.
        let config = RunConfig::from_netlist(&netlist).unwrap();
        let request = config.request_for(&netlist.analyses[0]).unwrap();
        let mut circuit = config.circuit(&netlist).unwrap();
        let plot = runner(request.kind)
            .unwrap()
            .run(&mut circuit, &request, &config.context())
            .unwrap();
        for node in ["d", "g", "s"] {
            let actual = plot.value(&format!("v({node})"), 0).unwrap().re;
            close(
                actual,
                expected[node],
                1e-6,
                1e-9,
                &format!("case {index} v({node})"),
            );
        }
        // Terminal currents through the bias sources, including the junction
        // and substrate terms C reports as `vd#branch`/`vs#branch`.
        for (source, name) in [
            ("vd", "vd#branch"),
            ("vg", "vg#branch"),
            ("vs", "vs#branch"),
        ] {
            let actual = plot.value(&format!("i({source})"), 0).unwrap().re;
            close(
                actual,
                expected[name],
                1e-6,
                1e-15,
                &format!("case {index} i({source})"),
            );
        }
        // C's asks at C's own solution, through the port's device.
        let context = ngspice_rs::devices::ModelContext::new(temperature, 27.);
        let voltage = {
            let expected = expected.clone();
            let names: Vec<String> = circuit
                .nodes()
                .nodes()
                .iter()
                .map(|node| node.name.clone())
                .collect();
            move |node: ngspice_rs::primitives::NodeId| -> f64 {
                let name = &names[node.index()];
                expected.get(name).copied().unwrap_or(0.)
            }
        };
        let device = circuit.device("m1").expect("BSIM3 device");
        if device.terminals().len() > 4 {
            // RSH creates drain/source primes. C does not print their voltages
            // in `print all`, so the ask comparison would evaluate the intrinsic
            // device at the wrong bias; the terminal currents above already
            // cover this case end to end.
            continue;
        }
        for ask in ["id", "gm", "gds", "gmbs", "vth", "vdsat"] {
            let rust = device
                .observation_operating(ask, &context, &voltage, false)
                .unwrap()
                .unwrap_or_else(|| panic!("case {index}: no {ask}"));
            let c = expected[&format!("@m1[{ask}]")];
            close(rust, c, 1e-9, 1e-18, &format!("case {index} @m1[{ask}]"));
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; .dc waits for the small-signal port"]
fn dc_sweep_is_refused_explicitly_until_small_signal_exists() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    let deck = "sweep\nvd d 0 1.5\nvg g 0 0\nvs s 0 0\n\
                m1 d g s 0 nn l=0.5u w=5u\n.model nn nmos level=8\n\
                .dc vg 0 3 0.5\n.end\n";
    let c_deck = deck.replace(
        ".dc vg 0 3 0.5\n.end\n",
        ".dc vg 0 3 0.5\n.control\nrun\nquit\n.endc\n.end\n",
    );
    let directory =
        Scratch(std::env::temp_dir().join(format!("bsim3-sweep-{}", std::process::id())));
    fs::create_dir_all(&directory.0).unwrap();
    fs::write(directory.0.join("sweep.cir"), c_deck).unwrap();
    let output = Command::new(&binary)
        .args(["-b", "sweep.cir"])
        .current_dir(&directory.0)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    // C sweeps; the port's sweep setup assembles the bias-linearized system,
    // which needs the (unported) charge/AC load, so it must refuse rather than
    // sweep with a DC-only conductance.
    assert!(log.contains("No. of Data Rows : 7"), "{log}");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("sweep.cir"), deck))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("b3acld.c"), "{error}");
}
