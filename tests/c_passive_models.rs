//! Opt-in C effective-value queries plus production DC/AC conformance.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, RawFile, runner};
use ngspice_rs::devices::{Circuit, ModelResolver};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::AnalysisKind;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const PROBE: &str = include_str!("../conformance/parser/passive_elaboration.cir");
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let scratch = Self(std::env::temp_dir().join(format!(
            "spice-passive-oracle-{}-{nonce}",
            std::process::id()
        )));
        fs::create_dir(&scratch.0).unwrap();
        scratch
    }
    fn run(&self, text: &str) -> String {
        fs::write(self.0.join("probe.cir"), text).unwrap();
        let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
        assert!(binary.is_absolute(), "NGSPICE_BIN must be absolute");
        let result = Command::new(binary)
            .args(["-b", "probe.cir"])
            .current_dir(&self.0)
            .output()
            .unwrap();
        let stdout = String::from_utf8(result.stdout).unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{stdout}\n{stderr}");
        assert!(
            !stderr.to_lowercase().contains("error"),
            "{stdout}\n{stderr}"
        );
        stdout
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn close(got: f64, want: f64, relative: f64, absolute: f64) {
    assert!(
        got.is_finite()
            && want.is_finite()
            && (got - want).abs() <= relative * want.abs() + absolute,
        "Rust {got} != C {want}"
    );
}
fn compare(got: &Plot, want: &Plot, ac: bool) {
    assert_eq!(got.point_count(), want.point_count());
    assert_eq!(got.variable_count(), want.variable_count());
    for var in &want.variables {
        for point in 0..want.point_count() {
            let a = got.value(&var.name, point).unwrap();
            let b = want.value(&var.name, point).unwrap();
            let (rtol, atol) = if ac { (1e-10, 1e-12) } else { (1e-12, 1e-15) };
            close(a.re, b.re, rtol, atol);
            close(a.im, b.im, rtol, atol);
        }
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; validates bounded passive effective values against C setup"]
fn passive_effective_geometry_temperature_and_precedence_match_c() {
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("passive_elaboration.cir"),
            PROBE,
        ))
        .unwrap();
    let resolver = ModelResolver::new(&n.models).unwrap();
    for context in [
        AnalysisContext::default(),
        AnalysisContext {
            temperature: 77.0,
            nominal_temperature: 22.0,
            ..Default::default()
        },
    ] {
        let mut expected = BTreeMap::new();
        let mut lets = String::new();
        for instance in &n.devices {
            if instance.model.is_none() {
                continue;
            }
            let model = resolver.resolve(instance).unwrap().unwrap();
            let value = model
                .passive_parameters(instance)
                .unwrap()
                .effective_value(&context.model_context())
                .unwrap();
            let name = format!("got_{}", instance.name);
            let expression = match instance.designator {
                'r' => format!("1/@{}[i]", instance.name), // V=1; resistance query is unadjusted.
                'c' => format!("@{}[capacitance]", instance.name), // CAPask includes M.
                _ => format!("@{}[inductance]/@{}[m]", instance.name, instance.name), // INDask excludes M.
            };
            lets.push_str(&format!("let {name} = {expression}\n"));
            expected.insert(name, value);
        }
        // Independent physical checks avoid hiding a mirrored formula bug.
        close(expected["got_rgeom"], 156.0, 1e-12, 1e-24);
        close(expected["got_cmodel"], 6.144e-6, 1e-12, 1e-24);
        close(expected["got_lmodel"], 9.216e-3, 1e-12, 1e-24);
        let queries = expected.keys().cloned().collect::<Vec<_>>().join(" ");
        let script = format!(
            ".options temp={} tnom={}\n.control\nset numdgt=17\nop\n{lets}print {queries}\nquit\n.endc\n.end\n",
            context.temperature, context.nominal_temperature
        );
        let scratch = Scratch::new();
        let output = scratch.run(&PROBE.replace(".end\n", &script));
        let mut actual = BTreeMap::new();
        for line in output.lines() {
            if let Some((name, value)) = line.split_once(" = ")
                && name.starts_with("got_")
            {
                actual.insert(name.to_owned(), value.trim().parse::<f64>().unwrap());
            }
        }
        assert_eq!(actual.len(), expected.len(), "{output}");
        for (name, value) in expected {
            close(value, actual[&name], 1e-12, 1e-24);
        }
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; compares production passive-model DC and complex AC with live C"]
fn passive_model_production_dc_ac_matches_c() {
    let body = "v1 in 0 1 ac 1\nr1 in out rm m=2 scale=3\nc1 out 0 cm l=4u w=2u m=2\nl1 out mid lm m=2\nr2 mid 0 1k\n.model rm r(rsh=100 l=6u defw=2u tc1=0.001)\n.model cm c(cj=1meg cjsw=1m tc1=0.002)\n.model lm l(ind=1 tc1=0.003)";
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("passive-response.cir"),
            &format!("responses\n{body}\n.end\n"),
        ))
        .unwrap();
    for context in [
        AnalysisContext::default(),
        AnalysisContext {
            temperature: 77.0,
            nominal_temperature: 22.0,
            ..Default::default()
        },
    ] {
        for (kind, args, command) in [
            (AnalysisKind::OperatingPoint, vec![], "op"),
            (
                AnalysisKind::Ac,
                vec!["dec", "3", "1", "1000"],
                "ac dec 3 1 1000",
            ),
        ] {
            let mut circuit =
                Circuit::from_netlist_with_context(&n, &context.model_context()).unwrap();
            let got = runner(kind)
                .unwrap()
                .run(
                    &mut circuit,
                    &AnalysisRequest::with_arguments(kind, args),
                    &context,
                )
                .unwrap();
            let text = format!(
                "responses\n{body}\n.options temp={} tnom={}\n.control\nset filetype=ascii\nset numdgt=17\n{command}\nwrite response.raw\nquit\n.endc\n.end\n",
                context.temperature, context.nominal_temperature
            );
            let scratch = Scratch::new();
            scratch.run(&text);
            let raw = fs::read_to_string(scratch.0.join("response.raw")).unwrap();
            let raw = RawFile::parse(&raw).unwrap();
            assert_eq!(raw.plots.len(), 1);
            compare(&got, &raw.plots[0].plot, kind == AnalysisKind::Ac);
        }
    }
}
