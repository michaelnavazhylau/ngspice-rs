//! Opt-in live C comparison of the S/W switches (#81).
//!
//! ```text
//! NGSPICE_BIN=/abs/path/to/ngspice \
//!   cargo test -p ngspice-rs --test c_switches -- --ignored
//! ```
//!
//! * **Transients.** Decks beyond the committed goldens (a resistive PWL ramp
//!   whose steps only `swtrunc.c` bounds, a self-oscillating switch with
//!   negative hysteresis driven by a PULSE, an ON-flagged W fed by a PULSE,
//!   switches inside a subcircuit) run in both engines. A self-controlled
//!   switch with negative hysteresis chatters and fails with "timestep too
//!   small" at the same instant in both engines. The port reproduces C's accepted
//!   timepoints, so the samples are compared point by point.
//! * **Sweeps.** An upward and a downward `.dc` sweep of the same deck: the
//!   hysteresis carried from point to point must match. Decimal steps (`0.1`,
//!   `-0.25m`) whose accumulated values land next to a threshold, and nested
//!   sweeps whose every inner sweep restarts from the instance flags.
//! * **AC.** C's `ACan` reloads with `MODEINITSMSIG`, which copies its zero
//!   `CKTstate1` into `CKTstate0`, so a switch closed at the operating point is
//!   open in the AC sweep; the port reproduces that.
//!
//! Ordinary `cargo test` never needs C: everything here is `#[ignore]`d.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::Real;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs `cards` with `analysis` in C and returns the written plot.
fn run_c(tag: &str, cards: &str, analysis: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-switches-{}-{tag}", std::process::id()));
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

/// Runs `cards` with the dot card `analysis` through the production runner.
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

fn column(plot: &Plot, name: &str) -> Vec<Real> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}

/// Same rows, and every named vector within `1e-6 |C| + 1e-9`.
fn same_points(ours: &Plot, theirs: &Plot, scale: &str, names: &[&str]) {
    let (a, b) = (column(ours, scale), column(theirs, scale));
    assert_eq!(a.len(), b.len(), "{scale}: port {a:?}\nC {b:?}");
    for name in std::iter::once(&scale).chain(names) {
        for (k, (x, y)) in column(ours, name)
            .iter()
            .zip(column(theirs, name))
            .enumerate()
        {
            assert!(
                (x - y).abs() <= 1e-6 * y.abs() + 1e-9,
                "{name}[{k}] at {scale} = {}: port {x}, C {y}",
                b[k]
            );
        }
    }
}

const RAMP: &str = "vc c 0 pwl(0 0 10m 2 20m 0)\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
    r2 in b 1k\ns2 b 0 0 c sn on\n\
    .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)\n.model sn sw(vt=-1 vh=-0.25 ron=5)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn a_resistive_ramp_takes_cs_switch_limited_steps() {
    let ours = run_rust(RAMP, "tran 1u 20m 0 20m");
    let theirs = run_c("ramp", RAMP, "tran 1u 20m 0 20m");
    same_points(&ours, &theirs, "time", &["v(a)", "v(b)", "v(c)"]);
}

const OSCILLATORS: &str = "i1 0 c pulse(0 1m 10u 1u)\nc1 c 0 100n\ns1 c 0 p 0 sneg\n\
    vp p 0 pulse(0 3 50u 20u 20u 100u 300u)\nvsense p q 0\nrq q 0 1k\n\
    vdd vdd 0 2\nr2 vdd c2 2k\nc2 c2 0 100n\nw1 c2 0 vsense wl on\n\
    .model sneg sw(vt=1 vh=-0.4 ron=100 roff=1meg)\n\
    .model wl csw(it=1.5m ih=0.5m ron=100 roff=1meg)\n";

/// A self-controlled switch with negative hysteresis: inside its band
/// `SWload` maps really-off to on and back at every predicted point, so the
/// state can never settle and both engines give up at the same instant.
const CHATTER: &str = "i1 0 c pulse(0 1m 10u 1u)\nc1 c 0 100n\ns1 c 0 c 0 sneg\n\
    .model sneg sw(vt=1 vh=-0.4 ron=100 roff=1meg)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn a_chattering_switch_fails_where_c_fails() {
    let text = format!("r\n{CHATTER}.tran 2u 1m\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("r.cir"), &text))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("timestep too small at t = 7.0517"),
        "{error}"
    );
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-switches-{}-chatter", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    fs::write(dir.join("c.cir"), &text).unwrap();
    // `-r` makes batch mode run the deck without `.print` cards.
    let output = Command::new(&binary)
        .args(["-b", "-r", "out.raw", "c.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("Timestep too small; time = 7.0517"),
        "C did not fail as expected:\n{log}"
    );
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn negative_hysteresis_and_w_latches_match_c() {
    let ours = run_rust(OSCILLATORS, "tran 2u 1m");
    let theirs = run_c("osc", OSCILLATORS, "tran 2u 1m");
    same_points(&ours, &theirs, "time", &["v(c)", "v(c2)", "i(vsense)"]);
}

const SUBCKT: &str = ".subckt cell ctl out\ns1 out 0 ctl 0 sm\nvs ctl m 0\nrm m 0 1k\n\
    w1 out 0 vs wm off\n.ends cell\n\
    vc c 0 sin(1 1.5 2k)\nvdd vdd 0 1\nr1 vdd o1 1k\nc1 o1 0 10n\nx1 c o1 cell\n\
    .model sm sw(vt=1.5 vh=0.2 ron=50)\n.model wm csw(it=-0.2m ih=0.1m ron=20)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn switches_inside_subcircuits_match_c() {
    let ours = run_rust(SUBCKT, "tran 2u 2m");
    let theirs = run_c("subckt", SUBCKT, "tran 2u 2m");
    same_points(&ours, &theirs, "time", &["v(o1)", "i(v.x1.vs)"]);
}

const SWEEP: &str = "vc c 0 0\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\nr2 in b 1k\ns2 b 0 c 0 sz on\n\
    rs c d 1k\nvs d 0 0\nr3 in e 1k\nw1 e 0 vs wm\n\
    .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)\n.model sz sw(vt=0.5)\n\
    .model wm csw(it=0.6m ih=-0.3m ron=30)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn upward_and_downward_sweeps_match_c() {
    for (tag, sweep) in [("up", "dc vc -1 3 0.125"), ("down", "dc vc 3 -1 -0.125")] {
        let ours = run_rust(SWEEP, sweep);
        let theirs = run_c(tag, SWEEP, sweep);
        same_points(&ours, &theirs, "v(c)", &["v(a)", "v(b)", "v(e)", "i(vs)"]);
    }
}

/// Decimal steps land on round thresholds only through C's accumulated sweep
/// values (`dctrcurv.c`: `value += step`): at `0.1 * 10` C has
/// `0.9999999999999999` and `1.5000000000000004` at the fifteenth point, which
/// decide `vt=1` (no band) and `vt=1 vh=0.5` (band edge) differently from the
/// exact `start + i step`. The port must visit C's values.
const DECIMAL: &str = "vc c 0 0\nvin in 0 1\nr1 in a 1k\ns1 a 0 c 0 sm\nr2 in b 1k\ns2 b 0 c 0 sh\n\
    .model sm sw(vt=1 ron=10 roff=1meg)\n.model sh sw(vt=1 vh=0.5 ron=10 roff=1meg)\n";

const DECIMAL_W: &str = "i1 0 c 0\nrc c 0 1k\nvs c d 0\nrd d 0 1\nvin in 0 1\nr1 in b 1k\nw1 b 0 vs wm\n\
    .model wm csw(it=0.75m ih=0.25m ron=10 roff=1meg)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn decimal_sweep_steps_visit_cs_accumulated_values() {
    for (tag, deck, sweep, names) in [
        ("d-up", DECIMAL, "dc vc 0 2 0.1", &["v(a)", "v(b)"][..]),
        ("d-up3", DECIMAL, "dc vc 0 3 0.1", &["v(a)", "v(b)"][..]),
        ("d-down", DECIMAL, "dc vc 3 0 -0.1", &["v(a)", "v(b)"][..]),
        (
            "w-down",
            DECIMAL_W,
            "dc i1 3m -1m -0.25m",
            &["v(b)", "i(vs)"][..],
        ),
    ] {
        let ours = run_rust(deck, sweep);
        let theirs = run_c(tag, deck, sweep);
        // C's raw file rounds the swept values, so they are compared with a
        // tolerance; the switch states they decide are what must agree.
        let (a, b) = (scale(&ours), scale(&theirs));
        assert_eq!(a.len(), b.len(), "{tag}");
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() <= 1e-12 * y.abs().max(1e-3), "{tag}: {x} {y}");
        }
        same_points(&ours, &theirs, "v(c)", names);
    }
}

/// Nested sweeps: when the inner sweep wraps for the next outer value, C sets
/// `firstTime` and MODEINITJCT again, so the first inner point is decided from
/// the instance flag (here OFF, open inside the band), not carried over from
/// the last point of the previous inner sweep (closed at `vc = 3`).
fn scale(plot: &Plot) -> Vec<Real> {
    column(plot, &plot.variables[0].name)
}

const NESTED: &str = "vc c 0 0\nvdd vdd 0 1\nr1 vdd a 1k\ns1 a 0 c 0 swh\n\
    r2 vdd b 1k\ns2 b 0 c 0 swh on\n\
    .model swh sw(vt=1 vh=0.5 ron=10 roff=1meg)\n";

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn nested_sweeps_restart_each_inner_sweep_from_the_flags() {
    for (tag, sweep) in [
        ("n-up", "dc vc 1 3 0.25 vdd 1 2 1"),
        ("n-down", "dc vc 3 -1 -0.5 vdd 1 3 1"),
        ("n-dec", "dc vc 0 2 0.1 vdd 1 2 0.5"),
    ] {
        let ours = run_rust(NESTED, sweep);
        let theirs = run_c(tag, NESTED, sweep);
        same_points(&ours, &theirs, "v(c)", &["v(a)", "v(b)", "v(vdd)"]);
    }
}

const AC: &str = "vc c 0 2\nvin in 0 0 ac 1\nr1 in a 1k\ns1 a 0 c 0 sm\n\
    .model sm sw(vt=1 vh=0.5 ron=10 roff=1meg)\n";

/// A switch that is off inside its band at the operating point (HYST_OFF), a
/// closed one, an ON-flagged one and a W, with AC on the control too.
const AC_MIX: &str = "vc ctrl 0 dc 2 ac 1\nvdd vdd 0 dc 1 ac 1\nr1 vdd a 1k\n\
    s1 a 0 ctrl 0 sm off\nc1 a 0 1u\nr2 vdd b 1k\ns2 b 0 ctrl 0 sn on\n\
    rc ctrl d 1k\nvs d 0 0\nr3 vdd e 1k\nw1 e 0 vs wm\nc3 e 0 1u\n\
    .model sm sw(vt=1 vh=0.2 ron=10 roff=1meg)\n.model sn sw(vt=2.5 vh=1 ron=20 roff=2meg)\n\
    .model wm csw(it=1m ih=0.5m ron=30)\n";

/// Same rows, and every named complex vector within `1e-6 |C| + 1e-12`.
fn same_complex(ours: &Plot, theirs: &Plot, names: &[&str]) {
    for name in names {
        let (a, b) = (ours.column(name).unwrap(), theirs.column(name).unwrap());
        assert_eq!(a.len(), b.len(), "{name}");
        for (k, (x, y)) in a.iter().zip(&b).enumerate() {
            let scale = 1e-6 * y.re.hypot(y.im) + 1e-12;
            assert!(
                (x.re - y.re).abs() <= scale && (x.im - y.im).abs() <= scale,
                "{name}[{k}]: port {x:?}, C {y:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs a temporary C deck out of process"]
fn ac_uses_cs_minitsmsig_state() {
    let ours = run_rust(AC, "ac lin 1 1k 1k");
    let theirs = run_c("ac", AC, "ac lin 1 1k 1k");
    // Closed at the operating point, open in both engines' AC (CKTstate1 = 0).
    assert!((ours.value("v(a)", 0).unwrap().re - 1e6 / (1e6 + 1e3)).abs() < 1e-9);
    same_complex(&ours, &theirs, &["v(a)", "i(vin)"]);
    let ours = run_rust(AC_MIX, "ac dec 2 10 1k");
    let theirs = run_c("ac-mix", AC_MIX, "ac dec 2 10 1k");
    same_complex(
        &ours,
        &theirs,
        &["frequency", "v(a)", "v(b)", "v(e)", "i(vdd)", "i(vs)"],
    );
}
