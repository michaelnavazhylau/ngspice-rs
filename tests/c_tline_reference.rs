//! Opt-in live C comparison of the lossless transmission line `T` (#84).
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_tline_reference -- --ignored
//! ```
//!
//! Decks beyond the committed `m10_tline_*` goldens: C's default `rel`/`abs`
//! (corners not landed), a line in a nonlinear (diode) circuit, an
//! incommensurate delay, `f`/`nl` and a shorted stub inside a subcircuit in
//! AC, the DC wire, and `.sp`. The port follows `dctran.c`, `traacct.c` and
//! `tratrunc.c`, so its transient takes C's timepoints and samples are
//! compared row by row. Ordinary `cargo test` never needs C.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::Complex;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_c(tag: &str, cards: &str, analysis: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-tline-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    let deck = format!(
        "c\n{cards}.control\nset filetype=ascii\n{analysis}\nwrite result.raw\nquit\n.endc\n.end\n"
    );
    fs::write(dir.join("c.cir"), deck).unwrap();
    let output = Command::new(&binary)
        .args(["-b", "c.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{text}");
    let raw = fs::read_to_string(dir.join("result.raw")).unwrap_or_else(|_| panic!("{text}"));
    RawFile::parse(&raw).unwrap().plots[0].plot.clone()
}

fn run_rust(cards: &str, analysis: &str) -> Plot {
    let text = format!("r\n{cards}.{analysis}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("r.cir"), &text))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}

fn column(plot: &Plot, name: &str) -> Vec<Complex> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name} in {:?}", plot.variables))
}

/// Same rows, and every C vector within `relative |C| + absolute`.
fn same_rows(ours: &Plot, theirs: &Plot, relative: f64, absolute: f64) {
    assert_eq!(ours.point_count(), theirs.point_count());
    for variable in &theirs.variables {
        let name = variable.name.to_ascii_lowercase();
        for (k, (x, y)) in column(ours, &name)
            .iter()
            .zip(column(theirs, &name))
            .enumerate()
        {
            assert!(
                (*x - y).magnitude() <= relative * y.magnitude() + absolute,
                "{name}[{k}]: port {x:?}, C {y:?}"
            );
        }
    }
}

const TRANSIENT: &str = "v1 in 0 pulse(0 1 1n 0.1n 0.1n 5n 20n)\nrs in a 50\n\
    t1 a 0 b 0 z0=50 td=2n\nrl b 0 100\n\
    v2 x 0 pwl(0 0 1n 0 1.4n 1 3n 0.3)\nr2 x y 30\nt2 y 0 z 0 z0=90 td=2.0173n\nr3 z 0 400\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn default_tolerance_transients_take_cs_steps() {
    let ours = run_rust(TRANSIENT, "tran 0.05n 30n");
    let theirs = run_c("tran", TRANSIENT, "tran 0.05n 30n");
    same_rows(&ours, &theirs, 1e-9, 1e-12);
}

const DIODE: &str = "v1 in 0 sin(0 2 50meg)\nrs in a 50\nt1 a 0 b 0 z0=50 td=3n\n\
    d1 b 0 dm\nrl b 0 1k\n.model dm d\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn a_line_into_a_diode_follows_c_within_its_newton_tolerance() {
    // Same timepoints; the two Newton solutions differ within C's own
    // reltol = 1e-3 (measured: 2e-4 V on the clamped node).
    let ours = run_rust(DIODE, "tran 0.1n 60n");
    let theirs = run_c("diode", DIODE, "tran 0.1n 60n");
    same_rows(&ours, &theirs, 1e-3, 1e-6);
}

const AC: &str = "v1 in 0 dc 1 ac 1\nrs in a 50\nx1 a b stub\nrl b 0 1k\n\
    .subckt stub p q\nt1 p 0 q 0 z0=50 f=250meg nl=0.25\nt2 p 0 0 0 zo=75 td=1.3n\n.ends\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn ac_dc_and_sparameters_match_c() {
    let ours = run_rust(AC, "ac lin 101 1meg 2g");
    let theirs = run_c("ac", AC, "ac lin 101 1meg 2g");
    same_rows(&ours, &theirs, 1e-10, 1e-12);
    let ours = run_rust(AC, "op");
    let theirs = run_c("op", AC, "op");
    same_rows(&ours, &theirs, 1e-12, 1e-15);
    let ports = "v1 a 0 dc 0 portnum 1\nt1 a 0 b 0 z0=75 td=1n\nv2 b 0 dc 0 portnum 2\n";
    let ours = run_rust(ports, "sp lin 11 10meg 510meg");
    let theirs = run_c("sp", ports, "sp lin 11 10meg 510meg");
    same_rows(&ours, &theirs, 1e-10, 1e-12);
}
