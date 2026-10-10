//! `.pz` pole-zero analysis (#103) through the production runner and the CLI:
//! analytic poles and zeros, C's drive/column conventions, the plot layout C
//! writes, and the explicit errors for every case C refuses or the port does
//! not support. Nothing here needs the C binary; see `c_pole_zero.rs` for the
//! opt-in live comparison and `conformance/golden/pz_*.raw` for the goldens.

use std::fs;
use std::path::Path;
use std::process::Command;

use ngspice_rs::analysis::{Plot, PlotFlags, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{Complex, Real, SpiceResult};

/// Runs the deck's single analysis card through the production runner.
fn run(cards: &str, analysis: &str) -> SpiceResult<Plot> {
    let text = format!("pz test\n{cards}{analysis}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("pz.cir"), &text))
        .expect("the deck parses");
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

fn roots(plot: &Plot, kind: &str) -> Vec<Complex> {
    plot.variables
        .iter()
        .enumerate()
        .filter(|(_, v)| v.name.starts_with(&format!("v({kind}(")))
        .map(|(i, _)| plot.points[0][i])
        .collect()
}

fn assert_close(got: Complex, re: Real, im: Real, relative: Real) {
    let scale = re.hypot(im);
    assert!(
        (got.re - re).abs() <= relative * scale && (got.im - im).abs() <= relative * scale,
        "got {got:?}, expected {re} {im:+}j"
    );
}

fn error(cards: &str, analysis: &str) -> String {
    run(cards, analysis)
        .expect_err("the analysis is refused")
        .to_string()
}

const RC: &str = "v1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\n";

#[test]
fn an_rc_lowpass_has_one_real_pole_and_no_finite_zero() {
    let plot = run(RC, ".pz in 0 out 0 vol pz").unwrap();
    assert_eq!(plot.plotname, "Pole-Zero Analysis");
    assert_eq!(plot.flags, PlotFlags::Complex);
    assert_eq!(plot.point_count(), 1);
    assert_eq!(plot.variables.len(), 1);
    let pole = &plot.variables[0];
    assert_eq!(
        (pole.name.as_str(), pole.unit.as_str(), pole.is_real),
        ("v(pole(1))", "voltage", false)
    );
    assert_eq!(plot.points[0][0].im, 0.0);
    assert_close(plot.points[0][0], -1e3, 0.0, 1e-12);
}

#[test]
fn a_series_rlc_has_a_conjugate_pole_pair_positive_part_first() {
    let (r, l, c): (Real, Real, Real) = (10.0, 1e-3, 1e-6);
    let plot = run(
        "v1 in 0 dc 0 ac 1\nr1 in a 10\nl1 a out 1m\nc1 out 0 1u\n",
        ".pz in 0 out 0 vol pz",
    )
    .unwrap();
    let poles = roots(&plot, "pole");
    assert_eq!(poles.len(), 2);
    assert!(roots(&plot, "zero").is_empty());
    let (sigma, omega) = (
        -r / (2.0 * l),
        (1.0 / (l * c) - (r / (2.0 * l)).powi(2)).sqrt(),
    );
    assert_close(poles[0], sigma, omega, 1e-12);
    // C's PZpost writes the conjugate right after, as an exact mirror.
    assert_eq!(poles[1], Complex::new(poles[0].re, -poles[0].im));
}

#[test]
fn a_two_pole_rc_ladder_matches_its_characteristic_polynomial() {
    let (r1, c1, r2, c2): (Real, Real, Real, Real) = (1e3, 1e-6, 2e3, 3e-7);
    let plot = run(
        "v1 in 0 dc 0 ac 1\nr1 in a 1k\nc1 a 0 1u\nr2 a out 2k\nc2 out 0 0.3u\n",
        ".pz in 0 out 0 vol pol",
    )
    .unwrap();
    // H = 1 / (a s^2 + b s + 1).
    let (a, b) = (r1 * r2 * c1 * c2, r1 * c1 + r2 * c2 + r1 * c2);
    let disc = (b * b - 4.0 * a).sqrt();
    let poles = roots(&plot, "pole");
    assert_eq!(plot.variables.len(), 2, "pol computes no zeros");
    // Ascending real part, as C lists them.
    assert_close(poles[0], (-b - disc) / (2.0 * a), 0.0, 1e-12);
    assert_close(poles[1], (-b + disc) / (2.0 * a), 0.0, 1e-12);
}

#[test]
fn a_notch_finds_every_pole_where_c_muller_gives_up() {
    // C ngspice-47 reports "Pole-zero iteration limit reached" for this deck
    // and returns no poles at all; the eigenvalue method has no search.
    let (r1, l, c1, r2, c2): (Real, Real, Real, Real, Real) = (1e3, 10e-3, 1e-6, 1e3, 100e-9);
    let plot = run(
        "v1 in 0 dc 0 ac 1\nr1 in a 1k\nc1 a out 1u\nl1 a out 10m\nr2 out 0 1k\nc2 out 0 100n\n",
        ".pz in 0 out 0 vol pz",
    )
    .unwrap();
    let zeros = roots(&plot, "zero");
    assert_eq!(zeros.len(), 2);
    let notch = 1.0 / (l * c1).sqrt();
    assert!(zeros[0].re.abs() <= 1e-9 * notch, "{zeros:?}");
    assert_close(Complex::new(0.0, zeros[0].im), 0.0, notch, 1e-12);
    // D(s) = (R1 (1 + s R2 C2) + R2)(1 + s^2 L C1) + s L (1 + s R2 C2).
    let poles = roots(&plot, "pole");
    assert_eq!(poles.len(), 3);
    for p in poles {
        let one = Complex::real(1.0);
        let k = |x: Real| Complex::real(x);
        let rc = one + p * k(r2 * c2);
        let lc = one + p * p * k(l * c1);
        let terms = [k(r1) * rc * lc, k(r2) * lc, p * k(l) * rc];
        let d = terms[0] + terms[1] + terms[2];
        let size: Real = terms.iter().map(|t| t.magnitude()).sum();
        assert!(
            d.magnitude() <= 1e-10 * size,
            "pole {p:?}: |D| = {}",
            d.magnitude()
        );
        assert!(p.re < 0.0);
    }
}

#[test]
fn a_highpass_zero_sits_at_the_origin() {
    let plot = run(
        "v1 in 0 dc 0 ac 1\nc1 in out 1u\nr1 out 0 1k\n",
        ".pz in 0 out 0 vol pz",
    )
    .unwrap();
    let (poles, zeros) = (roots(&plot, "pole"), roots(&plot, "zero"));
    assert_close(poles[0], -1e3, 0.0, 1e-12);
    assert_eq!(zeros.len(), 1);
    assert!(zeros[0].magnitude() <= 1e-9 * 1e3, "{zeros:?}");
}

#[test]
fn current_input_at_the_output_gives_the_input_impedance() {
    // Zin = r2 || (r1 + 1/(s c1)): pole -1/((r1 + r2) c1), zero -1/(r1 c1).
    let plot = run(
        "i1 0 in dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\nr2 in 0 1k\n",
        ".pz in 0 in 0 cur pz",
    )
    .unwrap();
    assert_close(roots(&plot, "pole")[0], -500.0, 0.0, 1e-12);
    assert_close(roots(&plot, "zero")[0], -1e3, 0.0, 1e-12);
}

#[test]
fn a_capacitor_across_an_ideal_supply_adds_no_spurious_pole() {
    // cdec and the DC source vdd form a loop (an index-two block of the
    // pencil): its infinite eigenvalues are deflated, never reported.
    let plot = run(
        "vdd vdd 0 dc 5\ncdec vdd 0 1u\nv1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out vdd 1u\n",
        ".pz in 0 out 0 vol pz",
    )
    .unwrap();
    assert_eq!(roots(&plot, "pole").len(), 1, "{plot:?}");
    assert_close(roots(&plot, "pole")[0], -1e3, 0.0, 1e-12);
    assert!(roots(&plot, "zero").is_empty());
}

#[test]
fn an_explicit_ac_zero_still_removes_the_input_source() {
    // vsrcpzld.c tests VSRCacGiven, not the value: `ac 0` is given.
    let given = run(
        "v1 in 0 dc 0 ac 0\nr1 in out 1k\nc1 out 0 1u\nr2 in 0 2k\n",
        ".pz in 0 out 0 cur pol",
    )
    .unwrap();
    // The opened source leaves `in` undriven: det Y has the pole of c1
    // through r1 + r2, -1/((r1 + r2) c1).
    assert_close(roots(&given, "pole")[0], -1.0 / (3e3 * 1e-6), 0.0, 1e-12);
    // Without an AC value the source shorts `in`, so the pole is -1/(r1 c1).
    let shorted = run(
        "v1 in 0 dc 0\nr1 in out 1k\nc1 out 0 1u\nr2 in 0 2k\n",
        ".pz in 0 out 0 cur pol",
    )
    .unwrap();
    assert_close(roots(&shorted, "pole")[0], -1e3, 0.0, 1e-12);
}

#[test]
fn a_resistive_circuit_gives_an_empty_real_plot() {
    let plot = run(
        "v1 in 0 dc 0 ac 1\nr1 in out 1k\nr2 out 0 1k\n",
        ".pz in 0 out 0 vol pz",
    )
    .unwrap();
    // C writes `Flags: real`, no variables and one point.
    assert_eq!(plot.flags, PlotFlags::Real);
    assert!(plot.variables.is_empty());
    assert_eq!(plot.point_count(), 1);
}

#[test]
fn nonlinear_devices_are_linearized_at_the_operating_point() {
    // A diode biased by a DC source: the pole moves with the bias.
    let pole = |bias: &str| {
        let plot = run(
            &format!(
                "v1 in 0 dc {bias} ac 1\nr1 in out 1k\nd1 out 0 dm\nc1 out 0 1n\n\
                 .model dm d(is=1e-14 n=1)\n"
            ),
            ".pz in 0 out 0 vol pol",
        )
        .unwrap();
        roots(&plot, "pole")[0].re
    };
    let (off, on) = (pole("0"), pole("0.8"));
    // Off: only r1 (plus gmin and the junction's tiny conductance).
    assert_close(Complex::real(off), -1e6, 0.0, 1e-6);
    assert!(
        on < 2.0 * off,
        "a forward-biased diode speeds the pole up: {on}"
    );
}

#[test]
fn c_input_checks_are_reported() {
    assert!(error(RC, ".pz in in out 0 vol pz").contains("Input is shorted"));
    assert!(error(RC, ".pz in 0 out out vol pz").contains("Output is shorted"));
    assert!(error(RC, ".pz in 0 in 0 vol pz").contains("Transfer function is unity"));
    assert!(error(RC, ".pz in out out in vol pz").contains("Transfer function is -1"));
    assert!(error(RC, ".pz in 0 nowhere 0 vol pz").contains("does not exist"));
    for card in [
        ".pz in 0 out 0 vol",
        ".pz in 0 out 0 vol cur",
        ".pz in 0 out 0 volt pz",
        ".pz in 0 out 0 vol pz extra",
    ] {
        assert!(error(RC, card).contains(".pz in+ in- out+ out-"), "{card}");
    }
}

#[test]
fn a_vanishing_determinant_is_c_input_shorted_error() {
    // The output is not connected to the input: Vout/Iin is identically 0.
    let message = error(
        "v1 in 0 dc 0 ac 1\nr1 in 0 1k\nc1 in 0 1u\nr2 out 0 1k\nc2 out 0 1u\n",
        ".pz in 0 out 0 vol pz",
    );
    assert!(message.contains("singular pencil"), "{message}");
    assert!(
        message.contains("shorted on the way to the output"),
        "{message}"
    );
    // A DC-only source at the input shorts the drive: also C's E_SHORT.
    let message = error(
        "v1 in 0 dc 1\nr1 in out 1k\nc1 out 0 1u\nr2 out 0 1k\n",
        ".pz in 0 out 0 vol pz",
    );
    assert!(message.contains("singular pencil"), "{message}");
}

#[test]
fn devices_without_a_c_pole_zero_load_are_refused() {
    // POLY lowers to an XSPICE code model: C has no pole-zero load for it.
    let message = error(
        "vin in 0 dc 0 ac 1\nr1 in a 1k\nc1 a 0 1u\ne1 b 0 poly(1) a 0 0 2\nr2 b 0 1k\n",
        ".pz in 0 b 0 vol pz",
    );
    assert!(message.contains("XSPICE code-model"), "{message}");
    let message = error(
        "vin in 0 dc 0 ac 1\nr1 in a 1k\nc1 a 0 1u\nb1 b 0 v=v(a)*hertz\nr2 b 0 1k\n",
        ".pz in 0 b 0 vol pz",
    );
    assert!(message.contains("hertz"), "{message}");
    // C's vsrcpzld.c stamps an RF port's ideal source but not its z0.
    let message = error(
        "v1 in 0 dc 0 ac 1 portnum 1 z0 50\nr1 in out 1k\nc1 out 0 1u\n",
        ".pz in 0 out 0 vol pz",
    );
    assert!(message.contains("RF port"), "{message}");
}

#[test]
fn simulate_writes_c_layout_and_composes_with_other_analyses() {
    let dir = std::env::temp_dir().join(format!("spice-rs-pz-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let deck =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/netlists/multi_analysis_pz.cir");
    let output = dir.join("out.raw");
    let run = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("simulate")
        .args(["--format", "ascii"])
        .arg("--output")
        .arg(&output)
        .arg(&deck)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let raw = RawFile::load(&output).unwrap();
    let plotnames: Vec<_> = raw.plots.iter().map(|p| p.plot.plotname.as_str()).collect();
    assert_eq!(
        plotnames,
        [
            "AC Analysis",
            "Operating Point",
            "Pole-Zero Analysis",
            "Pole-Zero Analysis"
        ]
    );
    // ngspice runs the later `.pz` card (zeros) first.
    let names = |i: usize| -> Vec<String> {
        raw.plots[i]
            .plot
            .variables
            .iter()
            .map(|v| v.name.clone())
            .collect()
    };
    assert_eq!(names(2), ["v(zero(1))", "v(zero(2))", "v(zero(3))"]);
    assert_eq!(names(3), ["v(pole(1))", "v(pole(2))", "v(pole(3))"]);
    let text = fs::read_to_string(&output).unwrap();
    assert!(text.contains("\t0\tv(pole(1))\tvoltage\n"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_save_card_naming_a_node_vector_fails_the_run() {
    // C: "no data saved for pole-zero analysis; analysis not run". The port
    // refuses the selection explicitly instead of writing an empty plot.
    let dir = std::env::temp_dir().join(format!("spice-rs-pz-save-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    fs::write(
        &deck,
        format!("save\n{RC}.save v(out)\n.pz in 0 out 0 vol pz\n.end\n"),
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("simulate")
        .arg("--output")
        .arg(dir.join("out.raw"))
        .arg(&deck)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("v(out)"));
    assert!(!dir.join("out.raw").exists());
    let _ = fs::remove_dir_all(&dir);
}
