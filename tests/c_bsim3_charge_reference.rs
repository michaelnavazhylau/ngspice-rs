//! Opt-in differential test of the BSIM3 charge core against C's `b3ask.c`
//! operating-point values (`b3ld.c`'s charge block at `capMod=3`).
//!
//! C supplies the converged internal bias, so this isolates the charge physics
//! from the solver wiring. `@m1[qg]`/`[qb]`/`[qd]` are `CKTstate0` charges and
//! `@m1[cgg]`..`[cbs]` are the nine `c*xb` derivatives after the overlap
//! adjustment, exactly what `b3ld.c` leaves for a later `MODEINITSMSIG` load.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::bsim3::{self, charges, dc};
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// C's askable charge/C-V keywords.
const ASKS: [&str; 12] = [
    "qg", "qb", "qd", "cgg", "cgd", "cgs", "cdg", "cdd", "cds", "cbg", "cbd", "cbs",
];

fn parse_asks(log: &str) -> std::collections::BTreeMap<String, f64> {
    log.lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            let name = name.trim().strip_prefix("@m1[")?.strip_suffix(']')?;
            Some((name.to_owned(), value.trim().parse().ok()?))
        })
        .collect()
}

struct Case {
    card: &'static str,
    instance: &'static str,
    temp: f64,
    /// Absolute gate and drain bias; the harness applies the model polarity.
    vgs: f64,
    vds: f64,
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; differential BSIM3 charge-core comparison"]
fn charge_core_matches_c_at_c_converged_bias() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    let cases = [
        // Default card, triode, saturation and subthreshold.
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 2.5,
            vds: 1.0,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 3.0,
            vds: 0.05,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 0.3,
            vds: 0.5,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 0.0,
            vds: 1.0,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: -0.5,
            vds: 0.4,
        },
        // The XPART partitions.
        Case {
            card: "xpart=1",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 2.5,
            vds: 1.0,
        },
        Case {
            card: "xpart=1",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 3.0,
            vds: 0.05,
        },
        Case {
            card: "xpart=0",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 2.5,
            vds: 1.0,
        },
        Case {
            card: "xpart=0",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 3.0,
            vds: 0.05,
        },
        Case {
            card: "xpart=0.5",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 2.5,
            vds: 1.0,
        },
        Case {
            card: "xpart=0.5",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 3.0,
            vds: 0.05,
        },
        Case {
            card: "xpart=0.5",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 0.2,
            vds: 0.6,
        },
        // Sub-micron geometry with a large overlap and poly depletion.
        Case {
            card: "cgdo=1e-10 cgso=1e-10 cgbo=1e-10 cgdl=1e-10 cgsl=1e-10 ckappa=1",
            instance: "l=0.15u w=1u",
            temp: 27.,
            vgs: 1.2,
            vds: 0.8,
        },
        Case {
            card: "cgdo=2e-10 cgso=2e-10 cgdl=1e-10 cgsl=1e-10 ckappa=2",
            instance: "l=0.18u w=0.4u",
            temp: 27.,
            vgs: -0.4,
            vds: 0.1,
        },
        // Junction C-V with areas and perimeters.
        Case {
            card: "cj=1e-3 cjsw=1e-10 cjswg=2e-10 mj=0.4 mjsw=0.2 mjswg=0.3 pb=0.9 pbsw=0.85 pbswg=0.8",
            instance: "l=1u w=10u ad=2p as=1p pd=4u ps=3u",
            temp: 27.,
            vgs: 2.5,
            vds: 1.0,
        },
        Case {
            card: "cj=1e-3 cjsw=1e-10 cjswg=2e-10 mj=0.5 mjsw=0.5 mjswg=0.5",
            instance: "l=1u w=10u ad=2p as=1p pd=40u ps=30u",
            temp: 27.,
            vgs: 0.5,
            vds: 0.6,
        },
        // Temperature-dependent junction and overlap terms.
        Case {
            card: "cj=1e-3 cjsw=1e-10 cjswg=2e-10 tcj=1e-3 tcjsw=2e-3",
            instance: "l=1u w=10u ad=2p as=1p pd=40u ps=30u",
            temp: 100.,
            vgs: 2.0,
            vds: 1.2,
        },
        // A different mobility model changes the DC intermediates used by C-V.
        Case {
            card: "mobmod=2 ua=1e-9 ub=1e-18 uc=1e-11",
            instance: "l=0.5u w=5u",
            temp: 27.,
            vgs: 2.5,
            vds: 1.0,
        },
        // Inverse mode.
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 2.5,
            vds: -0.5,
        },
    ];
    for (index, case) in cases.iter().enumerate() {
        run_case(&binary, index, case, case.vgs, case.vds, "nmos");
        run_case(&binary, index, case, -case.vgs, -case.vds, "pmos");
    }
}

fn run_case(binary: &Path, index: usize, case: &Case, vgs: f64, vds: f64, family: &str) {
    let model_card = format!(".model nn {family} level=8 {}", case.card);
    let deck = format!(
        "dcprobe\nvd d 0 {vds:e}\nvg g 0 {vgs:e}\nvs s 0 0\n\
         m1 d g s 0 nn {}\n{model_card}\n.options temp={} tnom=27\n\
         .control\nset numdgt=17\nop\nprint {}\nquit\n.endc\n.end\n",
        case.instance,
        case.temp,
        ASKS.map(|ask| format!("@m1[{ask}]")).join(" "),
    );
    let directory = Scratch(
        std::env::temp_dir().join(format!("bsim3-q-{}-{index}-{family}", std::process::id())),
    );
    fs::create_dir_all(&directory.0).unwrap();
    fs::write(directory.0.join("probe.cir"), &deck).unwrap();
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&directory.0)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(
        !log.contains("aborted") && !log.contains("Fatal error"),
        "case {index} {family}: {log}"
    );
    let asks = parse_asks(&log);
    assert_eq!(asks.len(), ASKS.len(), "case {index} {family}: {log}");

    // C's converged raw bias and mode for the charge model.
    let text = format!(
        "dcprobe\nm1 d g s 0 nn {}\n{model_card}\n.end\n",
        case.instance
    );
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
        .unwrap();
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let resolved = resolver.resolve(&netlist.devices[0]).unwrap().unwrap();
    let params = bsim3::model_parameters(&resolved).unwrap();
    let context = ModelContext::new(case.temp, 27.);
    let defaults = params.model_defaults(&context).unwrap();
    let temperature = defaults
        .at_temperature(case.temp, &netlist.devices[0].location)
        .unwrap();
    let inputs = params.instance_parameters(&netlist.devices[0], 1.).unwrap();
    let setup = bsim3::setup::InstanceSetup::new(
        &defaults,
        &temperature,
        &inputs,
        &netlist.devices[0].location,
    )
    .unwrap();
    let bias = dc::Bias {
        vbs: 0.0,
        vgs: f64::from(setup.polarity) * vgs,
        vds: f64::from(setup.polarity) * vds,
    };
    let dc_result = dc::DcModel {
        model: &defaults,
        temperature: &temperature,
        setup: &setup,
    }
    .evaluate(bias, context.gmin)
    .unwrap();
    let result = charges::ChargeModel {
        model: &defaults,
        temperature: &temperature,
        setup: &setup,
    }
    .evaluate(&dc_result, bias)
    .unwrap();
    let vgb = bias.vgs - bias.vbs;
    let overlap = result.with_overlap(dc_result.mode, vgb);

    let compare = |name: &str, rust: f64| {
        let c = asks[name];
        let tolerance = 1e-9 * c.abs().max(rust.abs()) + 1e-22;
        assert!(
            (rust - c).abs() <= tolerance,
            "case {index} {family} {name}: Rust {rust:.12e} C {c:.12e} (diff {:.3e})",
            (rust - c).abs()
        );
    };
    compare("qg", overlap.qg);
    compare("qb", overlap.qb);
    compare("qd", overlap.qd);
    for (name, value) in [
        ("cgg", result.cggb),
        ("cgd", result.cgdb),
        ("cgs", result.cgsb),
        ("cdg", result.cdgb),
        ("cdd", result.cddb),
        ("cds", result.cdsb),
        ("cbg", result.cbgb),
        ("cbd", result.cbdb),
        ("cbs", result.cbsb),
    ] {
        compare(name, value);
    }
}
