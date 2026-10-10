//! Opt-in live C comparison of the lossy transmission line `O` (#85).
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_ltra_reference -- --ignored
//! ```
//!
//! Decks beyond the committed `m10_ltra_*` goldens: every model flag that
//! changes the simulation, C's default `rel`/`abs`, RC and RG lines, lines
//! inside a subcircuit, a line into a diode, AC, `.op`, `.dc` and `.sp`. The
//! port follows `dctran.c`, `ltraacct.c` and `ltratrun.c`, so its transient
//! takes C's timepoints and samples are compared row by row. Ordinary
//! `cargo test` never needs C.

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
    let dir = std::env::temp_dir().join(format!("spice-ltra-{}-{tag}", std::process::id()));
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

/// Same rows, and every C vector within `relative |C| + absolute` (C's
/// `.dc` scale `v-sweep` is the port's `sweep`).
fn same_rows(ours: &Plot, theirs: &Plot, relative: f64, absolute: f64) {
    assert_eq!(ours.point_count(), theirs.point_count());
    for variable in &theirs.variables {
        let name = variable.name.to_ascii_lowercase();
        let mine = if name == "v(v-sweep)" {
            "sweep"
        } else {
            name.as_str()
        };
        for (k, (x, y)) in column(ours, mine)
            .iter()
            .zip(column(theirs, &variable.name))
            .enumerate()
        {
            assert!(
                (*x - y).magnitude() <= relative * y.magnitude() + absolute,
                "{name}[{k}]: port {x:?}, C {y:?}"
            );
        }
    }
}

/// An RLC and an LC line driven by a PWL with a negative step, plus an RC
/// line into a capacitor and an RG line. `{flags}` goes on every model.
fn transient(flags: &str) -> String {
    format!(
        "v1 in 0 pwl(0 0 1n 0 1.5n 1 6n 1 6.5n 0 12n 0 12.2n -0.5 20n -0.5)\n\
         rs in a 30\no1 a 0 b 0 rlc\nrl b 0 120\n\
         rs2 in c 50\no2 c 0 d 0 lc\ncl d 0 2p\n\
         rs3 in e 50\no3 e 0 f 0 rc\ncl3 f 0 1p\n\
         rs4 in g 50\no4 g 0 h 0 rg\nrl4 h 0 100\n\
         .model rlc ltra r=5 l=251n c=100p len=1 {flags}\n\
         .model lc ltra l=300n c=80p len=0.5 {flags}\n\
         .model rc ltra r=1k c=10p len=1 {flags}\n\
         .model rg ltra r=10 g=1m len=2 {flags}\n"
    )
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn transients_take_cs_steps_under_every_flag() {
    // Same timepoints for every flag set. LC, RC and RG agree to rounding;
    // the RLC line differs by up to 3e-7 V where a step lands on a delayed
    // source corner: the port lands exactly on breakpoints while C steps to
    // t + (bp - t), and ltramisc.c's RLC coefficients branch on exact time
    // equalities (curtime - t_i == TD), so the two can take different
    // branches there (see docs/port/TRANSMISSION_LINES.md).
    for flags in [
        "",
        "lininterp",
        "mixedinterp",
        "quadinterp nosteplimit",
        "truncdontcut",
        "rel=0.1 abs=0.01",
        "compactrel=1e-2 compactabs=1e-6",
        "ltra nocontrol truncnr steplimit",
    ] {
        let cards = transient(flags);
        let ours = run_rust(&cards, "tran 0.05n 40n");
        let theirs = run_c("tran", &cards, "tran 0.05n 40n");
        same_rows(&ours, &theirs, 1e-5, 1e-9);
    }
}

const DIODE: &str = "v1 in 0 sin(0 2 50meg)\nrs in a 50\no1 a 0 b 0 lc\n\
    d1 b 0 dm\nrl b 0 1k\n.model dm d\n\
    .model lc ltra l=250n c=100p len=0.6 mixedinterp\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn a_line_into_a_diode_follows_c_within_its_newton_tolerance() {
    // Same timepoints; the two Newton solutions differ within C's own
    // reltol = 1e-3 once the wave reaches the diode (the lossless T line
    // does the same on this kind of deck).
    let ours = run_rust(DIODE, "tran 0.1n 60n");
    let theirs = run_c("diode", DIODE, "tran 0.1n 60n");
    same_rows(&ours, &theirs, 1e-3, 1e-6);
}

const AC: &str = "v1 in 0 dc 1 ac 1\nrs in a 50\nx1 a b lines\nrl b 0 1k\n\
    .subckt lines p q\no1 p 0 m 0 lc\no2 m 0 n 0 rlc\no3 n 0 q 0 rc\n.ends\n\
    .model lc ltra l=250n c=100p len=1\n.model rlc ltra r=20 l=250n c=100p len=1\n\
    .model rc ltra r=100 c=10p len=1\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn ac_dc_and_sparameters_match_c() {
    let ours = run_rust(AC, "ac dec 20 1meg 1g");
    let theirs = run_c("ac", AC, "ac dec 20 1meg 1g");
    same_rows(&ours, &theirs, 1e-10, 1e-12);
    let ours = run_rust(AC, "op");
    let theirs = run_c("op", AC, "op");
    same_rows(&ours, &theirs, 1e-12, 1e-15);
    // DC with an RG line and a line whose negative terminals float
    // (ltraload.c's DC rows ignore them).
    let dc = "v1 in 0 dc 1\nrs in a 50\no1 a b c 0 rlc\nrb b 0 10\nrl c 0 50\n\
        o2 a 0 d 0 rg\nrl2 d 0 100\n.model rlc ltra r=3 l=250n c=100p len=1\n\
        .model rg ltra r=10 g=1m len=2\n";
    let ours = run_rust(dc, "dc v1 -1 1 0.25");
    let theirs = run_c("dc", dc, "dc v1 -1 1 0.25");
    same_rows(&ours, &theirs, 1e-12, 1e-15);
    let ports = "v1 a 0 dc 0 portnum 1\no1 a 0 b 0 rlc\nv2 b 0 dc 0 portnum 2\n\
        .model rlc ltra r=20 l=250n c=100p len=1\n";
    let ours = run_rust(ports, "sp lin 11 10meg 510meg");
    let theirs = run_c("sp", ports, "sp lin 11 10meg 510meg");
    same_rows(&ours, &theirs, 1e-10, 1e-12);
}

const RG_AC: &str = "v1 in 0 dc 0 ac 1\nrs in a 50\no1 a 0 b 0 rg\nrl b 0 100\nc1 b 0 1n\n\
    .model rg ltra r=10 g=1m len=2\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn the_rg_ac_refusal_guards_a_c_defect() {
    // C's LTRAacLoad loads the DC stamp of every LTRA model and returns as
    // soon as it meets an RG model. An RG line alone is right in C, but with
    // another lossy line the other line's AC load is lost or doubled.
    let mixed = format!(
        "{RG_AC}rs2 in c 50\no2 c 0 d 0 lc\nrl2 d 0 200\n.model lc ltra l=250n c=100p len=1\n"
    );
    let theirs = run_c("rgac", &mixed, "ac lin 3 50meg 100meg");
    let d = column(&theirs, "v(d)");
    // The LC line is matched at its source and loaded with 200 ohm, so its
    // exact far-end response has magnitude 0.8 at every frequency; C's is
    // two orders of magnitude off.
    assert!(d.iter().all(|v| v.magnitude() < 0.1), "{d:?}");
    let text = format!("r\n{mixed}.ac lin 3 50meg 100meg\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("r.cir"), &text))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}
