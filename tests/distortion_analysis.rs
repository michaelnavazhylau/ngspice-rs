//! `.disto` (GitHub #104) through production APIs: parsed decks, `RunConfig`,
//! `runner` and `Analysis::run_plots`. Accuracy is judged against the
//! closed-form Volterra kernels of a diode's exponential; the committed C
//! goldens are verified by `cargo xtask golden verify` and the opt-in live-C
//! comparison is `c_disto_reference.rs`.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::devices::noise::{BOLTZMANN, CHARGE};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{Complex, SpiceResult};

const KELVIN: f64 = 300.15;
const GMIN: f64 = 1e-12;

fn run_at(deck: &str, index: usize) -> SpiceResult<Vec<Plot>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("disto.cir"), deck))?;
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

fn column(plot: &Plot, name: &str) -> Vec<Complex> {
    plot.column(name)
        .unwrap_or_else(|| panic!("{name} missing from {}", plot.plotname))
}

fn close(got: Complex, want: f64, relative: f64) {
    assert!(
        (got.re - want).abs() <= relative * want.abs() && got.im.abs() <= relative * want.abs(),
        "{got} != {want:e} (relative {relative:e})"
    );
}

/// A diode biased by a current source, with a parallel resistor:
/// `I0 = IS (exp(v / (N Vt)) - 1) + v / R + GMIN v`.
struct DiodeStage {
    i0: f64,
    r: f64,
    is: f64,
    n: f64,
}

impl DiodeStage {
    const STAGE: Self = Self {
        i0: 1e-3,
        r: 10e3,
        is: 1e-14,
        n: 1.5,
    };

    fn vte(&self) -> f64 {
        self.n * BOLTZMANN / CHARGE * KELVIN
    }

    /// The bias by bisection of the (monotonic) closed form.
    fn bias(&self) -> f64 {
        let vte = self.vte();
        let f = |v: f64| self.is * ((v / vte).exp() - 1.) + v / self.r + GMIN * v - self.i0;
        let (mut low, mut high) = (0., 2.);
        for _ in 0..200 {
            let mid = 0.5 * (low + high);
            if f(mid) > 0. {
                high = mid;
            } else {
                low = mid;
            }
        }
        0.5 * (low + high)
    }

    /// (total small-signal conductance, c2, c3) at the bias: the exponential's
    /// Taylor coefficients `IS e / (2 vte^2)` and `IS e / (6 vte^3)`.
    fn coefficients(&self) -> (f64, f64, f64) {
        let vte = self.vte();
        let e = (self.bias() / vte).exp();
        let gd = self.is * e / vte;
        (
            gd + GMIN + 1. / self.r,
            gd / (2. * vte),
            gd / (6. * vte * vte),
        )
    }

    fn deck(&self, inputs: &str, card: &str) -> String {
        format!(
            "diode stage\ni1 0 a dc {} {inputs}\nr1 a 0 {}\nd1 a 0 dm\n\
             .model dm d(is={} n={})\n.option reltol=1e-12\n{card}\n.end\n",
            self.i0, self.r, self.is, self.n
        )
    }
}

#[test]
fn diode_harmonics_follow_the_exponential_taylor_coefficients() {
    let stage = DiodeStage::STAGE;
    let amplitude = 2e-6;
    let plots = run(&stage.deck(&format!("distof1 {amplitude}"), ".disto dec 2 1k 100k"));
    assert_eq!(plots.len(), 2);
    assert_eq!(plots[0].plotname, "DISTORTION - 2nd harmonic");
    assert_eq!(plots[1].plotname, "DISTORTION - 3rd harmonic");
    let (g, c2, c3) = stage.coefficients();
    // cktdisto.c drives a current input with the opposite sign of its AC
    // stamp: `i1 0 a ... distof1 A` pulls 0.5 A out of `a`.
    let h1 = -0.5 * amplitude / g;
    let h2 = -c2 * h1 * h1 / g;
    let h3 = -(c2 * 2. * h1 * h2 + c3 * h1 * h1 * h1) / g;
    for plot in &plots {
        assert_eq!(column(plot, "frequency").len(), 5);
    }
    for value in column(&plots[0], "v(a)") {
        close(value, 2. * h2, 1e-9);
    }
    for value in column(&plots[1], "v(a)") {
        close(value, 2. * h3, 1e-9);
    }
    // The classical small-signal result: the second harmonic is
    // c2 a^2 / (2 g) for a fundamental of amplitude a = 2 h1 at the node.
    let a = 2. * h1;
    close(column(&plots[0], "v(a)")[0], -c2 * a * a / (2. * g), 1e-9);
    // Without a value, `distof1` drives magnitude 1 (vsrcpar.c).
    let unit = run(&stage.deck("distof1", ".disto lin 0 1k 1k"));
    let h1 = -0.5 / g;
    close(column(&unit[0], "v(a)")[0], -2. * c2 * h1 * h1 / g, 1e-9);
}

#[test]
fn diode_intermodulation_follows_the_volterra_kernels() {
    let stage = DiodeStage::STAGE;
    let (a, b) = (2e-6, 1e-6);
    let plots = run(&stage.deck(
        &format!("distof1 {a} distof2 {b}"),
        ".disto oct 1 1k 4k 0.9",
    ));
    let titles: Vec<&str> = plots.iter().map(|p| p.plotname.as_str()).collect();
    assert_eq!(
        titles,
        [
            "DISTORTION - IM: f1+f2",
            "DISTORTION - IM: f1-f2",
            "DISTORTION - IM: 2f1-f2"
        ]
    );
    let (g, c2, c3) = stage.coefficients();
    let (x, y) = (-0.5 * a / g, -0.5 * b / g);
    let sum = -c2 * x * y / g;
    let h2 = -c2 * x * x / g;
    // dloadfns.c D1n2F12: (c2 (4 X M + 2 B X2) + 3 c3 X^2 B) / 3.
    let third = -(c2 * (4. * x * sum + 2. * y * h2) + 3. * c3 * x * x * y) / 3. / g;
    for value in column(&plots[0], "v(a)") {
        close(value, 4. * sum, 1e-9);
    }
    for value in column(&plots[1], "v(a)") {
        close(value, 4. * sum, 1e-9);
    }
    for value in column(&plots[2], "v(a)") {
        close(value, 6. * third, 1e-9);
    }
}

#[test]
fn a_linear_circuit_has_no_distortion_and_c_names() {
    let plots = run(
        "rc\nv1 in 0 dc 1 ac 1 distof1 0.1\nr1 in out 1k\nc1 out 0 1u\n\
         .disto lin 2 1k 4k\n.end\n",
    );
    let names: Vec<&str> = plots[0].variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["frequency", "v(in)", "v(out)", "i(v1)"]);
    assert_eq!(plots[0].variables[1].unit, "voltage");
    assert!(plots[0].flags.is_complex());
    // distoan.c steps a linear sweep by (stop - start) / (pts + 1).
    let freqs: Vec<f64> = column(&plots[0], "frequency")
        .iter()
        .map(|c| c.re)
        .collect();
    assert_eq!(freqs, [1e3, 2e3, 3e3, 4e3]);
    for plot in &plots {
        for name in ["v(in)", "v(out)", "i(v1)"] {
            assert!(column(plot, name).iter().all(|v| *v == Complex::ZERO));
        }
    }
}

#[test]
fn invalid_cards_and_unsupported_devices_are_refused() {
    let base = "d\nv1 in 0 dc 0.7 distof1 0.01\nr1 in a 1k\nd1 a 0 dm\n.model dm d\n";
    let cases = [
        (".disto dec 2 1k 10k 0.9", "distof2"),
        (".disto log 2 1k 10k", "dec, oct or lin"),
        (".disto dec 2 0 10k", "positive"),
        (".disto dec 2 10k 1k", "fstart <= fstop"),
        (".disto dec 0 1k 10k", "at least 1 step"),
        (".disto dec 2 1k", "needs {dec|oct|lin}"),
    ];
    for (card, message) in cases {
        let error = error_of(&format!("{base}{card}\n.end\n"));
        assert!(error.contains(message), "{card}: {error}");
    }
    let behavioural = error_of(&format!(
        "{base}b1 b 0 v=v(a)*v(a)\nrb b 0 1k\n.disto dec 2 1k 10k\n.end\n"
    ));
    assert!(behavioural.contains("B source"), "{behavioural}");
}
