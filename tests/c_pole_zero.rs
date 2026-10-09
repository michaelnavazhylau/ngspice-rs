//! Opt-in live C comparison of `.pz` pole-zero analysis (#103).
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_pole_zero -- --ignored
//! ```
//!
//! Decks beyond the committed `pz_*` goldens run in both engines and the
//! poles and zeros are compared as unordered root sets under the bound of
//! `xtask`'s `compare::POLE_ZERO`: `|Rust - C| <= 1e-6 |C| + 1e-9 max|C|`.
//! Every deck has at least two roots: C's `write` command adds a copy of the
//! vector named `all` to a plot holding a single vector.
//!
//! One deliberate divergence is pinned here: C's `CCVSpzLoad`
//! (`ccvspzld.c`) adds the transresistance to the branch row with the
//! opposite sign of `CCVSload`, so C's pole-zero analysis of a deck with an H
//! source analyses the circuit with the H gain negated. The port uses the AC
//! load (the circuit as written); it matches C's result for the deck with the
//! gain negated and differs from C for the deck as written.
//!
//! Ordinary `cargo test` never needs C: everything here is `#[ignore]`d.

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

/// Runs `deck` (cards and the `.pz` card, no `.end`) in C.
fn run_c(tag: &str, deck: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-pz-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    let text = format!(
        "c\n{deck}.control\nset filetype=ascii\nrun\nwrite result.raw\nquit\n.endc\n.end\n"
    );
    fs::write(dir.join("c.cir"), text).unwrap();
    let output = Command::new(&binary)
        .args(["-b", "c.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{log}");
    assert!(
        !log.contains("Pole-zero iteration limit") && !log.contains("aberrations"),
        "C's root search did not finish: {log}"
    );
    let raw = fs::read_to_string(dir.join("result.raw")).unwrap_or_else(|_| panic!("{log}"));
    let plot = RawFile::parse(&raw).unwrap().plots[0].plot.clone();
    assert_eq!(plot.plotname, "Pole-Zero Analysis", "{log}");
    plot
}

fn run_rust(deck: &str) -> Plot {
    let text = format!("r\n{deck}.end\n");
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

fn roots(plot: &Plot, kind: &str) -> Vec<Complex> {
    plot.variables
        .iter()
        .enumerate()
        .filter(|(_, v)| v.name.starts_with(&format!("v({kind}(")))
        .map(|(i, _)| plot.points[0][i])
        .collect()
}

/// Whether the two plots have the same variables and every C root has its
/// own Rust root within the bound.
fn same_roots(ours: &Plot, theirs: &Plot) -> Result<(), String> {
    let names = |p: &Plot| -> Vec<String> { p.variables.iter().map(|v| v.name.clone()).collect() };
    if names(ours) != names(theirs) || ours.flags != theirs.flags {
        return Err(format!(
            "layout: port {:?}, C {:?}",
            names(ours),
            names(theirs)
        ));
    }
    let scale = theirs.points[0]
        .iter()
        .fold(0.0_f64, |m, z| m.max(z.magnitude()));
    for kind in ["pole", "zero"] {
        let mut free = roots(ours, kind);
        for c in roots(theirs, kind) {
            let bound = 1e-6 * c.magnitude() + 1e-9 * scale;
            let nearest = free
                .iter()
                .enumerate()
                .min_by(|x, y| (*x.1 - c).magnitude().total_cmp(&(*y.1 - c).magnitude()))
                .map(|(i, _)| i)
                .ok_or_else(|| format!("{kind} {c:?}: no port root left"))?;
            let error = (free[nearest] - c).magnitude();
            if error > bound {
                return Err(format!(
                    "{kind}: C {c:?}, nearest port root {:?} ({error:e} > {bound:e})",
                    free[nearest]
                ));
            }
            free.swap_remove(nearest);
        }
    }
    Ok(())
}

fn check(tag: &str, deck: &str) {
    let (ours, theirs) = (run_rust(deck), run_c(tag, deck));
    same_roots(&ours, &theirs)
        .unwrap_or_else(|e| panic!("{tag}: {e}\nport {ours:?}\nC {theirs:?}"));
}

#[test]
#[ignore = "needs NGSPICE_BIN (absolute path to the C ngspice binary)"]
fn current_input_impedance_matches_c() {
    check(
        "zin",
        "i1 0 in dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\nr2 in 0 1k\n.pz in 0 in 0 cur pz\n",
    );
}

#[test]
#[ignore = "needs NGSPICE_BIN (absolute path to the C ngspice binary)"]
fn switch_behavioural_source_and_subcircuit_match_c() {
    check(
        "mixed",
        "vin in 0 dc 1 ac 1\nvc c 0 2\ns1 in a c 0 sm\nr1 a out 1k\nc1 out 0 1u\n\
         b1 o2 0 v=2*v(out)+0.1*v(out)*v(out)\nr2 o2 o3 1k\nc2 o3 0 1n\nx1 o3 o4 rcsub\n\
         .subckt rcsub p q\nr p q 2k\nc q 0 3n\n.ends\n\
         .model sm sw(vt=1 vh=0.2 ron=10 roff=1meg)\n.pz in 0 o4 0 vol pz\n",
    );
}

#[test]
#[ignore = "needs NGSPICE_BIN (absolute path to the C ngspice binary)"]
fn zeros_only_and_poles_only_match_c() {
    check(
        "zer",
        "i1 0 in dc 0 ac 1\nr1 in a 1k\nc1 a 0 1u\nl1 a out 1m\nr3 a out 30\nc4 a out 10n\n\
         r2 out 0 50\nc3 in 0 1n\n.pz in 0 out 0 cur zer\n",
    );
    check(
        "pol",
        "vin in 0 dc 0 ac 1\nr1 in a rmod l=10u w=1u\n.model rmod r(rsh=100 tc1=0.01)\n\
         c1 a 0 2u\nr2 a b 1k\nc2 b 0 1u\n.options temp=60\n.pz in 0 b 0 vol pol\n",
    );
}

#[test]
#[ignore = "needs NGSPICE_BIN (absolute path to the C ngspice binary)"]
fn a_bjt_stage_and_a_differential_input_match_c() {
    check(
        "bjt",
        "vcc vcc 0 5\nvin in 0 dc 0.7 ac 1\nrb in b 10k\nrc vcc c 2k\nq1 c b 0 qm\ncl c 0 10p\n\
         .model qm npn(is=1e-15 bf=100 cje=1p cjc=0.5p tf=10p)\n.pz in 0 c 0 vol pz\n",
    );
    check(
        "differential",
        "vin p n dc 0 ac 1\nrp p a 1k\nrn n gnd 1k\nx1 a out lcsub\n\
         .subckt lcsub i o\nl1 i o 1m\nc1 o 0 1u\nr1 o 0 100\n.ends\nc9 a gnd 10n\n\
         .pz p n out gnd vol pz\n",
    );
}

#[test]
#[ignore = "needs NGSPICE_BIN (absolute path to the C ngspice binary)"]
fn c_negates_the_ccvs_gain_in_its_pole_zero_load() {
    let deck = |h: &str| {
        format!(
            "vin in 0 dc 0 ac 1\nr1 in a 1k\nc1 a 0 1u\ne1 b 0 a 0 3\nr2 b c 2k\nc2 c 0 2u\n\
             g1 0 d c 0 1m\nr3 d 0 1k\nc3 d 0 5u\nvs d e 0\nr4 e 0 4k\nf1 0 f vs 2\nr5 f 0 1k\n\
             c5 f 0 7u\nh1 g 0 vs {h}\nr6 g out 1k\nc6 out 0 11u\nr7 out a 100k\n\
             .pz in 0 out 0 vol pz\n"
        )
    };
    let ours = run_rust(&deck("100"));
    // The port's roots are C's for the H gain negated ...
    same_roots(&ours, &run_c("ccvs-negated", &deck("-100"))).unwrap();
    // ... and not C's for the deck as written (`ccvspzld.c` sign).
    assert!(same_roots(&ours, &run_c("ccvs", &deck("100"))).is_err());
}
