//! Opt-in live-C checks of K mutual inductance (GitHub #80): complex AC of
//! coupling forms that the committed goldens do not cover (duplicate K cards,
//! model-backed inductors with `m=` and temperature coefficients, ideal
//! coupling), and the documented divergence that C only *warns* about an
//! inductive system that is not positive definite while the port rejects it.
//!
//! Run with `NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p
//! analysis --test c_mutual_inductance -- --ignored`.
use ngspice_rs::analysis::{AnalysisRequest, Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let scratch = Self(std::env::temp_dir().join(format!(
            "spice-mutual-oracle-{}-{nonce}",
            std::process::id()
        )));
        fs::create_dir(&scratch.0).unwrap();
        scratch
    }
    fn run(&self, text: &str) -> Output {
        fs::write(self.0.join("probe.cir"), text).unwrap();
        let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
        assert!(binary.is_absolute(), "NGSPICE_BIN must be absolute");
        Command::new(binary)
            .args(["-b", "probe.cir"])
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rust(deck: &str) -> ngspice_rs::primitives::SpiceResult<Plot> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("k.cir"), deck))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request(AnalysisRequest::from(&netlist.analyses[0]))?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

/// C's AC plot of `body` (whose analysis is given separately as a control
/// command so that the rawfile can be written).
fn c_ac(body: &str, command: &str) -> Plot {
    let scratch = Scratch::new();
    let output = scratch.run(&format!(
        "k\n{body}\n.control\nset filetype=ascii\nset numdgt=17\n{command}\nwrite out.raw\nquit\n.endc\n.end\n"
    ));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        !stderr.to_lowercase().contains("error"),
        "{stdout}\n{stderr}"
    );
    let raw = fs::read_to_string(scratch.0.join("out.raw")).unwrap();
    let raw = RawFile::parse(&raw).unwrap();
    assert_eq!(raw.plots.len(), 1);
    raw.plots[0].plot.clone()
}

const DECKS: &[(&str, &str)] = &[
    (
        "duplicate K cards are summed",
        "v1 a 0 dc 0 ac 1\nr1 a b 1\nl1 b 0 1m\nl2 c 0 4m\nr2 c 0 10\n\
         k1 l1 l2 0.25\nk2 l2 l1 0.25",
    ),
    (
        "model-backed inductors use INDinduct before /m",
        "v1 a 0 dc 0 ac 1\nr1 a b 1\nl1 b 0 lm m=2\nl2 c 0 lm2\nr2 c 0 10\n\
         k1 l1 l2 0.5\n.model lm l(ind=2m)\n.model lm2 l(ind=4m tc1=0.01)\n\
         .options temp=50",
    ),
    (
        "ideal and negative couplings in a three-winding system",
        "v1 a 0 dc 0 ac 1\nr1 a b 1\nl1 b 0 1m\nl2 c 0 4m\nl3 d 0 9m\nr2 c 0 10\n\
         r3 d 0 20\nk1 l1 l2 -1\nk2 l1 l3 1\nk3 l2 l3 -1",
    ),
];

#[test]
#[ignore = "requires NGSPICE_BIN; compares coupled-inductor AC with live C"]
fn coupled_ac_matches_c() {
    let command = "ac dec 4 10 100k";
    for (what, body) in DECKS {
        let got = rust(&format!("k\n{body}\n.{command}\n.end\n"))
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        let want = c_ac(body, command);
        assert_eq!(got.point_count(), want.point_count(), "{what}");
        for variable in &want.variables {
            for point in 0..want.point_count() {
                let a = got.value(&variable.name, point).unwrap();
                let b = want.value(&variable.name, point).unwrap();
                let error = (a - b).magnitude();
                assert!(
                    error <= 1e-10 * b.magnitude() + 1e-15,
                    "{what}: {}[{point}] Rust {a} != C {b}",
                    variable.name
                );
            }
        }
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; documents C's warning-only definiteness check"]
fn c_only_warns_about_an_indefinite_system_that_the_port_rejects() {
    let body = "v1 a 0 dc 0 ac 1\nr1 a b 1\nl1 b 0 1m\nl2 c 0 4m\nr2 c 0 10\nk1 l1 l2 1.5";
    let scratch = Scratch::new();
    let output = scratch.run(&format!(
        "k\n{body}\n.ac lin 1 1k 1k\n.print ac v(c)\n.end\n"
    ));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stderr.contains("is not positive definite"), "{stderr}");
    assert!(stderr.contains("|k1| > 1"), "{stderr}");
    assert!(
        stdout.contains("1.000000e+03"),
        "C still simulates: {stdout}"
    );
    let error = rust(&format!("k\n{body}\n.ac lin 1 1k 1k\n.end\n")).unwrap_err();
    assert!(
        error.to_string().contains("not positive semidefinite"),
        "{error}"
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; documents that C does not check the stamped L/m matrix"]
fn c_is_silent_when_multiplicity_makes_the_stamped_system_indefinite() {
    // l1 stamps 10m / 2 but M = 0.9 sqrt(10m 40m): C's check uses INDinduct
    // (before /m) on its diagonal, passes and prints nothing; the port
    // rejects the indefinite matrix it would actually simulate.
    let body = "v1 a 0 dc 0 ac 1\nr1 a b 1\nl1 b 0 lm m=2\nl2 c 0 lm2\nr2 c 0 10\n\
                k1 l1 l2 0.9\n.model lm l(ind=10m)\n.model lm2 l(ind=40m)";
    let scratch = Scratch::new();
    let output = scratch.run(&format!(
        "k\n{body}\n.ac lin 1 1k 1k\n.print ac v(c)\n.end\n"
    ));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(!stderr.contains("Inductive System"), "{stderr}");
    let error = rust(&format!("k\n{body}\n.ac lin 1 1k 1k\n.end\n")).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("multiplicity m"), "{message}");
    assert!(!message.contains("C only warns"), "{message}");
}
