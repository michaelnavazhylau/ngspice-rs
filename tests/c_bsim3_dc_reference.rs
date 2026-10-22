//! Opt-in differential test of the BSIM3 DC core against C's `b3ask.c`
//! operating-point values. C supplies the converged internal bias, so this
//! separates DC/setup arithmetic from Newton iteration and stamping.
//! The port is `b3ld.c` statement by statement, so against the FMA-free
//! oracle (`fidelity-tools/README.md`) the ULP diagnostics are zero; an
//! ordinary FMA build differs in the last bits.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::devices::bsim3::{self, dc};
use ngspice_rs::devices::{ModelContext, ModelResolver};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// `cbs`/`cbd` in the instance table are capacitances (`BSIM3_CBSB/CBDB`);
// the junction currents are `ibs`/`ibd` (`BSIM3_CBS/CBD`).
const ASKS: [&str; 13] = [
    "vth", "id", "gm", "gds", "gmbs", "vdsat", "vbs", "vgs", "vds", "ibs", "ibd", "gbs", "gbd",
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

fn cases() -> [Case; 19] {
    [
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 1.0,
            vds: 1.0,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 0.5,
            vds: 0.05,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 0.0,
            vds: 0.5,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 1.5,
            vds: 2.0,
        },
        Case {
            card: "",
            instance: "l=0.15u w=1u",
            temp: 27.,
            vgs: 1.2,
            vds: 0.8,
        },
        Case {
            card: "vth0=0.4 dvt0=1.5 dvt1=0.4 dvt2=0.05 dvt0w=0.1 dvt1w=2e6 dvt2w=-0.03 nlx=1e-8",
            instance: "l=0.18u w=0.4u",
            temp: 27.,
            vgs: 1.0,
            vds: 1.0,
        },
        Case {
            card: "mobmod=2 ua=1e-9 ub=1e-18 uc=1e-11",
            instance: "l=0.5u w=5u",
            temp: 27.,
            vgs: 1.0,
            vds: 1.0,
        },
        Case {
            card: "mobmod=3 ua=4.9e-10 ub=1e-18 uc=-3e-11",
            instance: "l=0.5u w=5u",
            temp: 27.,
            vgs: 1.0,
            vds: 0.5,
        },
        Case {
            card: "rsh=10 prwg=0.1 prwb=-0.05",
            instance: "l=1u w=10u nrd=2 nrs=3",
            temp: 27.,
            vgs: 1.2,
            vds: 0.4,
        },
        Case {
            card: "k1=0.6 k2=0.05 vbx=-1",
            instance: "l=0.35u w=2u",
            temp: 27.,
            vgs: 1.2,
            vds: 0.6,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 100.,
            vgs: 1.0,
            vds: 1.0,
        },
        Case {
            card: "tnom=50",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 1.0,
            vds: 1.0,
        },
        Case {
            card: "js=1e-5 jsw=1e-10 ijth=0.1",
            instance: "l=1u w=10u ad=2p as=1p pd=4u ps=3u",
            temp: 27.,
            vgs: 0.8,
            vds: 0.6,
        },
        Case {
            card: "ijth=0 js=1e-5",
            instance: "l=1u w=10u ad=2p as=1p pd=4u ps=3u",
            temp: 27.,
            vgs: 0.8,
            vds: 0.3,
        },
        Case {
            card: "pclm=0.2 pscbe1=4.24e8 pscbe2=1e-5 pdiblc1=0.39 pdiblc2=0.0086 pdiblcb=-0.5 pvag=0",
            instance: "l=0.25u w=1u",
            temp: 27.,
            vgs: 1.4,
            vds: 1.5,
        },
        Case {
            card: "",
            instance: "l=1u w=10u",
            temp: 27.,
            vgs: 0.9,
            vds: -0.5,
        },
        // Measured Weff < 2e-8 continuation cases (T0 *= T0 * 4e-16).
        Case {
            card: "dwg=5.66336256480132049e-08 dwb=4.43519864619213457e-08 pvag=-5.68213123122809236e+01",
            instance: "l=0.5u w=0.1u",
            temp: 27.,
            vgs: 3.436745739803129,
            vds: 0.6881263962794837,
        },
        Case {
            card: "dwg=1.20659197801292422e-07 dwb=7.47031727728139807e-08 pvag=-7.74709299330928047e+01",
            instance: "l=0.5u w=0.1u",
            temp: 27.,
            vgs: 3.6179029802691343,
            vds: 0.6768355065674198,
        },
        Case {
            card: "dwg=1.59970859961868845e-07 dwb=3.86054572048661918e-08 pvag=-3.75168453234650912e+01",
            instance: "l=0.5u w=0.1u",
            temp: 27.,
            vgs: 3.8839522040476044,
            vds: 0.6349801391395405,
        },
    ]
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; differential BSIM3 DC core comparison"]
fn dc_core_matches_c_at_c_converged_bias() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for (index, case) in cases().iter().enumerate() {
        let polarity = 1.0;
        let vgs = case.vgs * polarity;
        let vds = case.vds * polarity;
        run_case(&binary, index, case, vgs, vds, "nmos");
        run_case(&binary, index, case, -vgs, -vds, "pmos");
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
        std::env::temp_dir().join(format!("bsim3-dc-{}-{index}-{family}", std::process::id())),
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
    let model = dc::DcModel {
        model: &defaults,
        temperature: &temperature,
        setup: &setup,
    };
    let bias = dc::Bias {
        vbs: asks["vbs"],
        vgs: asks["vgs"],
        vds: asks["vds"],
    };
    let result = model.evaluate(bias, context.gmin).unwrap();

    let compare = |name: &str, rust: f64| {
        let c = asks[name];
        eprintln!(
            "DC_ULP case={index} family={family} ask={name} ulp={} rust={rust:.17e} c={c:.17e}",
            rust.to_bits().abs_diff(c.to_bits())
        );
        let tolerance = 1e-9 * c.abs().max(rust.abs()) + 1e-16;
        assert!(
            (rust - c).abs() <= tolerance,
            "case {index} {family} {name}: Rust {rust:.12e} C {c:.12e} (diff {:.3e})",
            (rust - c).abs()
        );
    };
    compare("vth", result.von);
    compare("id", result.ids);
    compare("gm", result.gm);
    compare("gds", result.gds);
    compare("gmbs", result.gmbs);
    compare("vdsat", result.vdsat);
    compare("ibs", result.cbs);
    compare("ibd", result.cbd);
    compare("gbs", result.gbs);
    compare("gbd", result.gbd);
}
