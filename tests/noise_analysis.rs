//! `.noise` (GitHub #100) through production APIs: parsed decks, `RunConfig`,
//! `runner` and `Analysis::run_plots`. Accuracy is judged against closed-form
//! thermal, flicker and shot noise; the committed C goldens are verified by
//! `cargo xtask golden verify` and the opt-in live-C comparison is
//! `c_noise_reference.rs`.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::noise::{BOLTZMANN, CHARGE};
use ngspice_rs::devices::{Circuit, Device, LinearContext, StampContext};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{NodeId, SpiceResult};

const KELVIN: f64 = 300.15;

fn parse(deck: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(Path::new("noise.cir"), deck))
        .unwrap()
}

/// Every plot of analysis `index`, exactly as the CLI path runs it.
fn run_at(deck: &str, index: usize) -> SpiceResult<Vec<Plot>> {
    let netlist = parse(deck);
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[index])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run_plots(&mut circuit, &request, &config.context())
}

fn run(deck: &str) -> Vec<Plot> {
    run_at(deck, 0).unwrap()
}

fn error_of(deck: &str) -> String {
    run_at(deck, 0).expect_err("expected failure").to_string()
}

fn column(plot: &Plot, name: &str) -> Vec<f64> {
    plot.column(name)
        .unwrap_or_else(|| panic!("{name} missing from {}", plot.plotname))
        .iter()
        .map(|value| {
            assert_eq!(value.im, 0.0);
            value.re
        })
        .collect()
}

fn names(plot: &Plot) -> Vec<&str> {
    plot.variables.iter().map(|v| v.name.as_str()).collect()
}

fn close(got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs(),
        "{got:e} != {want:e} (relative {relative:e})"
    );
}

const DIVIDER: &str = "divider\nv1 in 0 dc 1 ac 1\nr1 in out 1k\nr2 out 0 3k\n";

#[test]
fn resistor_divider_thermal_noise_is_4ktr_of_the_parallel_resistance() {
    let plots = run(&format!("{DIVIDER}.noise v(out) v1 lin 4 1k 4k\n.end\n"));
    assert_eq!(plots.len(), 2);
    let [spectrum, integrated] = [&plots[0], &plots[1]];
    assert_eq!(spectrum.plotname, "Noise Spectral Density Curves");
    assert_eq!(
        names(spectrum),
        ["frequency", "onoise_spectrum", "inoise_spectrum"]
    );
    assert_eq!(
        spectrum.variables[1].unit, "voltage-density",
        "{:?}",
        spectrum.variables
    );
    assert_eq!(column(spectrum, "frequency"), [1e3, 2e3, 3e3, 4e3]);
    let parallel = 1e3 * 3e3 / 4e3;
    let gain = 3e3 / 4e3;
    let density = 4.0 * BOLTZMANN * KELVIN * parallel;
    for value in column(spectrum, "onoise_spectrum") {
        close(value, density.sqrt(), 1e-12);
    }
    for value in column(spectrum, "inoise_spectrum") {
        close(value, (density / (gain * gain)).sqrt(), 1e-12);
    }
    // A flat density integrates to density * (f2 - f1) exactly.
    assert_eq!(integrated.plotname, "Integrated Noise");
    assert_eq!(names(integrated), ["v(onoise_total)", "v(inoise_total)"]);
    assert_eq!(integrated.point_count(), 1);
    close(
        column(integrated, "v(onoise_total)")[0],
        (density * 3e3).sqrt(),
        1e-12,
    );
    close(
        column(integrated, "v(inoise_total)")[0],
        (density * 3e3 / (gain * gain)).sqrt(),
        1e-12,
    );
}

#[test]
fn per_instance_summaries_follow_c_names_order_and_sampling() {
    let plots = run(&format!(
        "{DIVIDER}r3 out 0 6k noisy=0\n.noise v(out) v1 dec 4 1 1k 3\n.end\n"
    ));
    let spectrum = &plots[0];
    // r3 is noiseless (no columns); C visits the default model's instances
    // in reverse deck order.
    assert_eq!(
        names(spectrum),
        [
            "frequency",
            "onoise_r2_thermal",
            "onoise_r2_1overf",
            "onoise_r2",
            "onoise_r1_thermal",
            "onoise_r1_1overf",
            "onoise_r1",
            "onoise_spectrum",
            "inoise_spectrum",
        ]
    );
    // 13 frequencies, every third written (steps 0, 3, 6, 9, 12).
    let frequencies = column(spectrum, "frequency");
    assert_eq!(frequencies.len(), 5);
    close(frequencies[1], 10f64.powf(0.75), 1e-12);
    close(frequencies[4], 1e3, 1e-12);
    let parallel: f64 = 1.0 / (1.0 / 1e3 + 1.0 / 3e3 + 1.0 / 6e3);
    let r1 = 4.0 * BOLTZMANN * KELVIN / 1e3 * parallel * parallel;
    close(column(spectrum, "onoise_r1_thermal")[2], r1.sqrt(), 1e-12);
    assert_eq!(column(spectrum, "onoise_r1_1overf"), [0.0; 5]);
    assert_eq!(
        column(spectrum, "onoise_r1"),
        column(spectrum, "onoise_r1_thermal")
    );
    let integrated = &plots[1];
    assert_eq!(
        names(integrated)[..6],
        [
            "v(onoise_total_r2_thermal)",
            "v(inoise_total_r2_thermal)",
            "v(onoise_total_r2_1overf)",
            "v(inoise_total_r2_1overf)",
            "v(onoise_total_r2)",
            "v(inoise_total_r2)",
        ]
    );
    assert_eq!(integrated.variable_count(), 14);
    close(
        column(integrated, "v(onoise_total_r1_thermal)")[0],
        (r1 * (1e3 - 1.0)).sqrt(),
        1e-11,
    );
    let total = column(integrated, "v(onoise_total)")[0];
    let r2 = column(integrated, "v(onoise_total_r2)")[0];
    let r1_total = column(integrated, "v(onoise_total_r1)")[0];
    close(total * total, r1_total * r1_total + r2 * r2, 1e-12);
}

#[test]
fn rc_lowpass_integrates_to_kt_over_c() {
    let plots =
        run("rc\nv1 in 0 ac 1\nr1 in out 10k\nc1 out 0 1n\n.noise v(out) v1 dec 40 1 1e12\n.end\n");
    let total = column(&plots[1], "v(onoise_total)")[0];
    // The piecewise power-law fit is exact on both asymptotes; the knee
    // region costs well under 0.1 % at 40 points per decade.
    close(total * total, BOLTZMANN * KELVIN / 1e-9, 1e-3);
}

#[test]
fn resistor_flicker_noise_follows_kf_af_ef() {
    let plots = run("flicker\nv1 in 0 dc 2 ac 1\nr1 in out 1k\nr2 out 0 rf\n\
         .model rf r(r=1k kf=1e-12 af=2 ef=1.5)\n.noise v(out) v1 dec 1 10 1k 1\n.end\n");
    let spectrum = &plots[0];
    let current: f64 = 1e-3;
    for (f, value) in column(spectrum, "frequency")
        .into_iter()
        .zip(column(spectrum, "onoise_r2_1overf"))
    {
        let density = 1e-12 * current * current / f.powf(1.5) * 500.0 * 500.0;
        close(value, density.sqrt(), 1e-11);
    }
    // 1/f^1.5 integrates analytically between 10 Hz and 1 kHz.
    let integrated = column(&plots[1], "v(onoise_total_r2_1overf)")[0];
    let exact =
        1e-12 * current * current * 500.0 * 500.0 * 2.0 * (10f64.powf(-0.5) - 1e3f64.powf(-0.5));
    close(integrated * integrated, exact, 1e-10);
}

#[test]
fn diode_shot_noise_is_2qi_relative_to_the_series_resistor() {
    let deck = "shot\nv1 in 0 dc 2 ac 1\nr1 in a 1k\nd1 a 0 dm\n.model dm d(is=1e-14)\n";
    let noise = run(&format!("{deck}.noise v(a) v1 lin 1 1k 1k 1\n.end\n"));
    assert_eq!(
        noise.len(),
        1,
        "a one-point sweep writes no integrated plot"
    );
    let op = &run(&format!("{deck}.op\n.end\n"))[0];
    let anode = column(op, "v(a)")[0];
    let current = (2.0 - anode) / 1e3;
    let spectrum = &noise[0];
    assert_eq!(
        names(spectrum)[1..8],
        [
            "onoise_d1_rs",
            "onoise_d1_id",
            "onoise_d1_1overf",
            "onoise_d1_rsw",
            "onoise_d1_idsw",
            "onoise_d1_1overfsw",
            "onoise_d1",
        ]
    );
    // Both generators see the same transfer impedance from the anode.
    let shot = column(spectrum, "onoise_d1_id")[0];
    let thermal = column(spectrum, "onoise_r1_thermal")[0];
    close(
        shot * shot / (thermal * thermal),
        2.0 * CHARGE * current * 1e3 / (4.0 * BOLTZMANN * KELVIN),
        1e-6,
    );
    for zero in [
        "onoise_d1_rs",
        "onoise_d1_rsw",
        "onoise_d1_idsw",
        "onoise_d1_1overf",
    ] {
        assert_eq!(column(spectrum, zero), [0.0], "{zero}");
    }
}

#[test]
fn current_inputs_refer_noise_to_amperes() {
    let plots =
        run("current\ni1 0 out dc 0 ac 1\nr1 out 0 2k\n.noise v(out) i1 dec 1 1 10\n.end\n");
    let spectrum = &plots[0];
    assert_eq!(spectrum.variables[2].unit, "current-density");
    let density = 4.0 * BOLTZMANN * KELVIN * 2e3;
    close(
        column(spectrum, "inoise_spectrum")[0],
        (density / 4e6).sqrt(),
        1e-12,
    );
    assert_eq!(names(&plots[1]), ["v(onoise_total)", "i(inoise_total)"]);
    assert_eq!(plots[1].variables[1].unit, "current");
}

#[test]
fn single_frequency_sweeps_write_one_plot() {
    for card in [
        ".noise v(out) v1 dec 10 1k 1k",
        ".noise v(out) v1 lin 1 1k 5k",
        ".noise v(out,0) v1 oct 3 2k 2k",
        ".noise v(out) v1 lin 5 3k 3k",
    ] {
        let plots = run(&format!("{DIVIDER}{card}\n.end\n"));
        assert_eq!(plots.len(), 1, "{card}");
        assert_eq!(plots[0].point_count(), 1, "{card}");
    }
}

#[test]
fn differential_outputs_and_temperature_are_honoured() {
    let plots = run("diff\nv1 in 0 ac 1\nr1 in a 1k\nr2 a b 1k\nr3 b 0 1k\n\
         .option temp=127\n.noise v(a,b) v1 lin 2 1 2\n.end\n");
    // Thevenin resistance between a and b: 1k || 2k.
    let density = 4.0 * BOLTZMANN * 400.15 * (1e3 * 2e3 / 3e3);
    close(
        column(&plots[0], "onoise_spectrum")[0],
        density.sqrt(),
        1e-12,
    );
}

#[test]
fn invalid_cards_and_inputs_are_explicit_errors() {
    for (card, message) in [
        (".noise v(out) v9 dec 1 1 10", "not in the circuit"),
        (".noise v(out) r1 dec 1 1 10", "not an independent"),
        (".noise v(nowhere) v1 dec 1 1 10", "output node 'nowhere'"),
        (".noise v(out,out) v1 dec 1 1 10", "must differ"),
        (".noise i(v1) v1 dec 1 1 10", "must be v(out)"),
        (".noise v(out) v1 log 1 1 10", "dec, oct or lin"),
        (".noise v(out) v1 dec 0 1 10", "at least one step"),
        (".noise v(out) v1 dec 1 10 1", "0 < fstart <= fstop"),
        (".noise v(out) v1 dec 1 0 10", "0 < fstart <= fstop"),
        (".noise v(out) v1 dec 1 1", "pts fstart fstop"),
        (".noise v(out) v1 dec 1 1 10 2 3", "pts fstart fstop"),
        (".noise v(out) v1 dec 1 1 10 -1", "must not be negative"),
    ] {
        let error = error_of(&format!("{DIVIDER}{card}\n.end\n"));
        assert!(error.contains(message), "{card}: {error}");
    }
    let error = error_of("noac\nv1 in 0 dc 1\nr1 in 0 1k\n.noise v(in) v1 dec 1 1 10\n.end\n");
    assert!(error.contains("has no AC value"), "{error}");
    // `ac 0` is an AC value: C drives the input with 1 + 0j regardless.
    let plots = run(
        "ac0\nv1 in 0 dc 1 ac 0\nr1 in out 1k\nr2 out 0 1k\n.noise v(out) v1 dec 1 1 10\n.end\n",
    );
    close(
        column(&plots[0], "inoise_spectrum")[0],
        2.0 * column(&plots[0], "onoise_spectrum")[0],
        1e-12,
    );
}

#[test]
fn model_noise_selectors_are_validated() {
    let deck = "m\nv1 g 0 2 ac 1\nm1 d g 0 0 nm w=10u l=2u\nr1 d 0 1k\n";
    let error = error_of(&format!(
        "{deck}.model nm nmos(vto=1 nlev=5)\n.noise v(d) v1 dec 1 1 10\n.end\n"
    ));
    assert!(error.contains("NLEV"), "{error}");
    for (model, flicker) in [
        ("nlev=0", "kf=1e-25"),
        ("nlev=3 gdsnoi=2", "kf=1e-25 af=1.2"),
    ] {
        let plots = run(&format!(
            "{deck}.model nm nmos(vto=1 {model} {flicker})\n.noise v(d) v1 dec 1 1 10 1\n.end\n"
        ));
        assert!(column(&plots[0], "onoise_m1_1overf")[0] > 0.0, "{model}");
    }
}

/// A device that implements nothing but its stamps: `.noise` must refuse it
/// rather than treat it as noiseless.
#[derive(Debug)]
struct Unported {
    terminals: [NodeId; 2],
}

impl Device for Unported {
    fn name(&self) -> &str {
        "x1"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let [a, b] = self.terminals;
        context.stamp(a, a, 1e-3)?;
        context.stamp(b, b, 1e-3)?;
        context.stamp(a, b, -1e-3)?;
        context.stamp(b, a, -1e-3)
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.terminals, 1e-3, false)
    }
}

#[test]
fn devices_without_a_noise_port_are_refused() {
    let netlist = parse(&format!("{DIVIDER}.noise v(out) v1 dec 1 1 10\n.end\n"));
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit: Circuit = config.circuit(&netlist).unwrap();
    let out = circuit.nodes().get("out").unwrap();
    circuit
        .add_device(Box::new(Unported {
            terminals: [out, NodeId::GROUND],
        }))
        .unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run_plots(&mut circuit, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("x1"), "{error}");
}

#[test]
fn simulate_writes_both_plots_and_composes_with_other_analyses() {
    let directory = std::env::temp_dir().join(format!("spice-rs-noise-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let deck = directory.join("deck.cir");
    std::fs::write(
        &deck,
        format!("{DIVIDER}.noise v(out) v1 dec 2 10 1k\n.op\n.ac dec 1 10 100\n.end\n"),
    )
    .unwrap();
    let output = directory.join("out.raw");
    let report = ngspice_rs::cli::simulate::run(&deck, &output, true).unwrap();
    let plots: Vec<&str> = report.plots.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(plots, ["ac1", "op1", "noise1", "noise2"]);
    let raw = ngspice_rs::analysis::RawFile::load(&output).unwrap();
    assert_eq!(raw.plots[2].plot.plotname, "Noise Spectral Density Curves");
    assert_eq!(raw.plots[3].plot.plotname, "Integrated Noise");
    assert_eq!(raw.plots[3].plot.variables[0].name, "v(onoise_total)");
    let _ = std::fs::remove_dir_all(&directory);
}
